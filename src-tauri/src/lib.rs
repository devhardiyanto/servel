mod commands;
mod tray;
mod watcher;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager};

use commands::config::ConfigState;
use commands::services::ServiceDef;
use commands::util::emit_log_line;

fn format_up_message(def: Option<&ServiceDef>, id: &str) -> String {
    if let Some(d) = def {
        let version = d.image.split(':').nth(1).unwrap_or("").to_string();
        let version_part = if version.is_empty() {
            String::new()
        } else {
            format!(" {}", version)
        };
        format!("{}{} started \u{2192} {}", d.name, version_part, d.container_name)
    } else {
        format!("{} started \u{2192} servel_{}", id, id)
    }
}

fn format_down_message(def: Option<&ServiceDef>, id: &str) -> String {
    match def {
        Some(d) => format!("{} stopped", d.name),
        None => format!("{} stopped", id),
    }
}

/// Suppress hard-error MessageBox dari child `docker.exe`/`wsl.exe` (yang inherit
/// error mode proses ini) saat WSL boot/teardown — cegah loader popup intermittent.
#[cfg(target_os = "windows")]
fn suppress_windows_error_popups() {
    use windows_sys::Win32::System::Diagnostics::Debug::{
        SetErrorMode, SEM_FAILCRITICALERRORS, SEM_NOGPFAULTERRORBOX, SEM_NOOPENFILEERRORBOX,
    };
    unsafe {
        SetErrorMode(SEM_FAILCRITICALERRORS | SEM_NOGPFAULTERRORBOX | SEM_NOOPENFILEERRORBOX);
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(target_os = "windows")]
    suppress_windows_error_popups();

    // Dishare antar loop polling + shutdown callback.
    let daemon_ready = Arc::new(AtomicBool::new(false));
    let shutting_down = Arc::new(AtomicBool::new(false));

    // Clone untuk callback `app.run` SEBELUM `shutting_down` di-move ke setup closure.
    let run_shutting_down = shutting_down.clone();

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(watcher::default_watcher_state())
        .manage(Mutex::new(HashMap::<String, bool>::new()))
        .manage(Mutex::new(ConfigState::default()))
        .invoke_handler(tauri::generate_handler![
            commands::prereq::check_prerequisites,
            commands::prereq::start_docker,
            commands::php::php_list_installed,
            commands::php::php_get_active,
            commands::php::php_switch,
            commands::php::php_install,
            commands::php::php_hook_status,
            commands::node::node_list_installed,
            commands::node::node_get_active,
            commands::node::node_switch,
            commands::node::node_install,
            commands::services::load_services,
            commands::services::services_status,
            commands::services::services_start,
            commands::services::services_stop,
            commands::services::services_stop_all,
            commands::compose::get_compose_path,
            watcher::watch_project,
            commands::config::config_read,
            commands::config::config_write,
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let should_minimize = {
                    let state = window.app_handle().state::<Mutex<ConfigState>>();
                    let val = state.lock().unwrap().minimize_to_tray;
                    val
                };
                if should_minimize {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .setup(move |app| {
            // Tray init — non-fatal: kalau gagal (mis. Linux tanpa appindicator) log warning + lanjut.
            if let Err(e) = tray::init(app.handle()) {
                eprintln!("[tray] init gagal, skip tray: {}", e);
            }

            let app_handle = app.handle().clone();
            let status_daemon_ready = daemon_ready.clone();
            let status_shutting_down = shutting_down.clone();
            tauri::async_runtime::spawn(async move {
                // Load service definitions sekali di awal — services.json bundled, tidak berubah runtime.
                let service_defs: Vec<ServiceDef> =
                    match commands::services::load_services_internal(&app_handle).await {
                        Ok(defs) => defs,
                        Err(err) => {
                            emit_log_line(
                                &app_handle,
                                "SERVEL",
                                &format!("[polling] gagal load services.json: {}", err),
                            );
                            Vec::new()
                        }
                    };

                // Backoff scheduling: 3s normal, 10s/30s saat docker daemon mati.
                // Hindari spam log + retry agresif yang bikin CPU/IO sia-sia.
                let base_delay = tokio::time::Duration::from_secs(3);
                #[allow(unused_assignments)]
                let mut current_delay = base_delay;
                let mut consecutive_failures: u32 = 0;
                let mut last_daemon_ok: Option<bool> = None;
                let mut is_first_tick = true;

                loop {
                    if status_shutting_down.load(Ordering::Relaxed) {
                        break;
                    }

                    let status_result = commands::services::services_status_internal().await;

                    let current = match status_result {
                        Ok(statuses) => {
                            status_daemon_ready.store(true, Ordering::Relaxed);
                            // Recovery transition: false -> true.
                            if last_daemon_ok == Some(false) {
                                let _ = app_handle.emit(
                                    "docker-daemon-status",
                                    serde_json::json!({ "running": true, "error": null }),
                                );
                                emit_log_line(
                                    &app_handle,
                                    "SERVEL",
                                    "[polling] docker daemon recovered",
                                );
                            }
                            last_daemon_ok = Some(true);
                            consecutive_failures = 0;
                            current_delay = base_delay;

                            statuses
                                .into_iter()
                                .map(|s| (s.id, s.running))
                                .collect::<HashMap<String, bool>>()
                        }
                        Err(err) => {
                            status_daemon_ready.store(false, Ordering::Relaxed);
                            consecutive_failures = consecutive_failures.saturating_add(1);
                            // Mapping backoff: 1-2 fail -> 10s, 3-5 -> 30s, >5 -> cap 30s.
                            current_delay = match consecutive_failures {
                                1..=2 => tokio::time::Duration::from_secs(10),
                                _ => tokio::time::Duration::from_secs(30),
                            };

                            // Emit event hanya saat transition None/true -> false.
                            if last_daemon_ok != Some(false) {
                                let stripped = err.trim().to_string();
                                let _ = app_handle.emit(
                                    "docker-daemon-status",
                                    serde_json::json!({
                                        "running": false,
                                        "error": stripped,
                                    }),
                                );
                                emit_log_line(
                                    &app_handle,
                                    "SERVEL",
                                    "[polling] docker daemon not reachable \u{2014} backing off",
                                );
                            }
                            last_daemon_ok = Some(false);

                            tokio::time::sleep(current_delay).await;
                            continue;
                        }
                    };

                    let running_count;
                    {
                    let state = app_handle.state::<Mutex<HashMap<String, bool>>>();
                    let mut prev = state.lock().unwrap();

                    for (id, running) in &current {
                        let prev_running = prev.get(id).copied();
                        if prev_running != Some(*running) {
                            let _ = app_handle.emit(
                                "container-status-changed",
                                serde_json::json!({ "service": id, "running": running }),
                            );

                            if !is_first_tick {
                                let def = service_defs.iter().find(|d| d.id == *id);
                                if *running {
                                    emit_log_line(
                                        &app_handle,
                                        "UP",
                                        &format_up_message(def, id),
                                    );
                                } else {
                                    emit_log_line(
                                        &app_handle,
                                        "DOWN",
                                        &format_down_message(def, id),
                                    );
                                }
                            }
                        }
                    }

                    for id in prev.keys() {
                        if !current.contains_key(id) {
                            let _ = app_handle.emit(
                                "container-status-changed",
                                serde_json::json!({ "service": id, "running": false }),
                            );

                            if !is_first_tick {
                                let def = service_defs.iter().find(|d| d.id == *id);
                                emit_log_line(
                                    &app_handle,
                                    "DOWN",
                                    &format_down_message(def, id),
                                );
                            }
                        }
                    }

                    running_count = current.values().filter(|&&r| r).count();
                    *prev = current;
                    is_first_tick = false;
                    }

                    tray::update_tooltip(&app_handle, running_count);

                    tokio::time::sleep(current_delay).await;
                }
            });

            // Polling kedua: `docker stats` untuk realtime memory usage per container.
            // Interval 5s (lebih lambat dari status 3s) karena `docker stats` lebih mahal.
            // Backoff lokal saat error (3s -> 10s -> 30s) — daemon-down event sudah di-handle
            // polling status, jadi di sini cukup silent backoff tanpa emit error.
            let stats_handle = app.handle().clone();
            let stats_daemon_ready = daemon_ready.clone();
            let stats_shutting_down = shutting_down.clone();
            tauri::async_runtime::spawn(async move {
                let base_delay = tokio::time::Duration::from_secs(5);
                let down_delay = tokio::time::Duration::from_secs(30);
                #[allow(unused_assignments)]
                let mut current_delay = base_delay;
                let mut consecutive_failures: u32 = 0;

                loop {
                    if stats_shutting_down.load(Ordering::Relaxed) {
                        break;
                    }

                    // Jangan spawn `docker stats` saat daemon down — itu yang memicu
                    // loader-error popup docker.exe/wsl.exe. Tunggu status loop melaporkan UP.
                    if !stats_daemon_ready.load(Ordering::Relaxed) {
                        tokio::time::sleep(down_delay).await;
                        continue;
                    }

                    match commands::stats::fetch_container_stats().await {
                        Ok(stats) => {
                            consecutive_failures = 0;
                            current_delay = base_delay;

                            if !stats.is_empty() {
                                let _ = stats_handle.emit("container-stats-changed", &stats);
                            }
                        }
                        Err(_) => {
                            consecutive_failures = consecutive_failures.saturating_add(1);
                            current_delay = match consecutive_failures {
                                1..=2 => tokio::time::Duration::from_secs(10),
                                _ => tokio::time::Duration::from_secs(30),
                            };
                        }
                    }

                    tokio::time::sleep(current_delay).await;
                }
            });

            Ok(())
        });

    let app = builder
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    // Set flag shutdown saat app exit / Windows session-end → kedua loop polling
    // break di awal iterasi berikutnya, hindari spawn docker.exe saat teardown.
    app.run(move |_handle, event| {
        if let tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit = event {
            run_shutting_down.store(true, Ordering::Relaxed);
        }
    });
}
