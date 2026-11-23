mod commands;
mod mise;
mod control;
mod pecl;
mod open_in;
mod autostart;
mod cli_install;
mod php_ext;
mod sites;
mod runtime;
mod updater;
mod tray;
mod tray_popup;
mod shutdown;
mod wsl;
mod port_owner;

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

static WINDOW_SHOWN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Tampilkan jendela utama sekali saja saat start. Setelah itu tidak
/// berpengaruh, supaya jendela yang sengaja disembunyikan ke tray tidak
/// muncul sendiri oleh pengaman batas waktu.
/// Tray berhasil dibuat; tanpa tray jendela tidak boleh disembunyikan saat start.
static TRAY_OK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Dibuka oleh autostart saat login (`--hidden`) dan tray tersedia: tetap di tray.
fn started_hidden() -> bool {
    TRAY_OK.load(std::sync::atomic::Ordering::SeqCst)
        && std::env::args().any(|a| a == autostart::HIDDEN_ARG)
}

fn show_main_window(app: &tauri::AppHandle) {
    use tauri::Manager;
    if started_hidden() {
        return;
    }
    if WINDOW_SHOWN.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Dipanggil frontend setelah UI ter-mount.
#[tauri::command]
fn app_ready(app: tauri::AppHandle) {
    show_main_window(&app);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(target_os = "windows")]
    suppress_windows_error_popups();

    // Satu Kova saja: bila sudah ada yang berjalan, minta ia menampilkan
    // jendelanya lalu keluar. Tanpa ini tiap klik ikon membuat proses, ikon
    // tray, dan dev server kedua.
    if control::hand_off_to_running_instance(std::env::args().any(|a| a == autostart::HIDDEN_ARG)) {
        return;
    }

    // Dishare antar loop polling + shutdown callback.
    let daemon_ready = Arc::new(AtomicBool::new(false));
    let shutting_down = Arc::new(AtomicBool::new(false));

    // Clone untuk callback `app.run` SEBELUM `shutting_down` di-move ke setup closure.
    let run_shutting_down = shutting_down.clone();

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(Mutex::new(HashMap::<String, bool>::new()))
        .manage(Mutex::new(ConfigState::default()))
        .manage(sites::process::SiteProcs::default())
        .invoke_handler(tauri::generate_handler![
            updater::update_check,
            commands::stats::stats_set_mode,
            updater::update_install,
            updater::update_download,
            updater::update_apply,
            updater::update_install_on_quit,
            commands::prereq::check_prerequisites,
            commands::prereq::start_engine,
            commands::prereq::get_platform,
            commands::php::php_list_installed,
            commands::php::php_get_active,
            commands::php::php_switch,
            commands::php::php_install,
            commands::node::node_list_installed,
            commands::node::node_get_active,
            commands::node::node_switch,
            commands::node::node_install,
            commands::services::load_services,
            commands::services::services_status,
            commands::services::check_port_conflicts,
            commands::services::services_start,
            commands::services::services_stop,
            commands::services::services_stop_all,
            commands::services::services_restore,
            commands::compose::get_compose_path,
            commands::config::config_read,
            commands::config::config_write,
            mise::mise_context,
            mise::mise_project_tools,
            wsl::wsl_project_tools,
            wsl::wsl_path_info,
            port_owner::port_owner,
            port_owner::port_owner_stop,
            commands::service_actions::service_shell,
            commands::service_actions::service_delete_data,
            commands::service_actions::service_remove,
            mise::mise_install_winget,
            mise::runtime_versions,
            mise::runtime_install,
            mise::runtime_use,
            mise::runtime_overview,
            mise::runtime_install_version,
            mise::runtime_install_cancel,
            mise::runtime_set_global,
            mise::runtime_use_in,
            mise::runtime_uninstall,
            open_in::project_open_in,
            autostart::autostart_get,
            autostart::autostart_set,
            cli_install::cli_status,
            cli_install::cli_install,
            mise::mise_trust,
            mise::mise_repair,
            app_ready,
            tray_popup::tray_popup_resize,
            tray_popup::tray_popup_hide,
            tray_popup::app_open_main,
            tray_popup::app_quit,
            php_ext::php_ext_status,
            php_ext::php_ext_set,
            php_ext::php_ext_install,
            php_ext::php_ext_fix_https,
            sites::project_detect,
            sites::project_scripts,
            sites::project_port_check,
            sites::sites_runtime,
            sites::site_start,
            sites::site_stop,
            sites::site_restart,
            sites::site_forget,
        ])
        .on_window_event(|window, event| {
            if window.label() == tray_popup::LABEL {
                tray_popup::on_window_event(window, event);
                return;
            }
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let should_minimize = {
                    let state = window.app_handle().state::<Mutex<ConfigState>>();
                    let val = state.lock().unwrap().minimize_to_tray;
                    val
                };
                api.prevent_close();
                if should_minimize {
                    let _ = window.hide();
                } else {
                    // Popup tray tersembunyi tetap terhitung jendela, jadi
                    // menutup jendela utama tidak memicu keluar dengan
                    // sendirinya. Keluar lewat jalur Quit.
                    shutdown::begin(window.app_handle());
                }
            }
        })
        .setup(move |app| {
            // Start saat login menunjuk ke program ini (nama/lokasi bisa berubah setelah update).
            std::thread::spawn(autostart::refresh_if_enabled);

            // Tray non-fatal: kalau gagal (mis. Linux tanpa appindicator) log warning lalu lanjut.
            match tray::init(app.handle()) {
                Ok(_) => TRAY_OK.store(true, std::sync::atomic::Ordering::SeqCst),
                Err(e) => eprintln!("[tray] init failed, skipping tray: {}", e),
            }

            runtime::init(app.handle());
            mise::init(app.handle());
            sites::autostart(app.handle());
            control::start(app.handle());

            // Jendela mulai tersembunyi dan ditampilkan frontend lewat
            // `app_ready` setelah UI ter-mount (tanpa kilatan putih). Pengaman:
            // kalau UI gagal dimuat, jendela tetap muncul setelah batas waktu.
            let fallback = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(8)).await;
                show_main_window(&fallback);
            });

            let app_handle = app.handle().clone();
            let status_daemon_ready = daemon_ready.clone();
            let status_shutting_down = shutting_down.clone();
            tauri::async_runtime::spawn(async move {
                // Resolve engine (Docker / WSLC) dari config SEBELUM polling pertama,
                // supaya tick awal tidak memanggil engine yang salah.
                let engine = runtime::ensure_resolved().await;
                emit_log_line(
                    &app_handle,
                    "KOVA",
                    &format!("container engine: {}", engine.label()),
                );

                // Load service definitions sekali di awal — services.json bundled, tidak berubah runtime.
                let service_defs: Vec<ServiceDef> =
                    match commands::services::load_services_internal(&app_handle).await {
                        Ok(defs) => defs,
                        Err(err) => {
                            emit_log_line(
                                &app_handle,
                                "KOVA",
                                &format!("[polling] could not load services.json: {}", err),
                            );
                            Vec::new()
                        }
                    };

                // Backoff scheduling: 3s normal, 10s/30s saat engine (daemon Docker /
                // sesi WSLC) tidak merespons.
                // Hindari spam log + retry agresif yang bikin CPU/IO sia-sia.
                let base_delay = tokio::time::Duration::from_secs(3);
                #[allow(unused_assignments)]
                let mut current_delay = base_delay;
                let mut consecutive_failures: u32 = 0;
                let mut last_daemon_ok: Option<bool> = None;
                let mut is_first_tick = true;
                // Readiness: "running" ke UI = container running DAN service
                // siap menerima koneksi. UI menahan "starting" sampai true.
                let mut readiness = runtime::readiness::Tracker::default();

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
                                    "KOVA",
                                    &format!("[polling] {} recovered", runtime::active().label()),
                                );
                            }
                            last_daemon_ok = Some(true);
                            consecutive_failures = 0;
                            current_delay = base_delay;

                            let running_ids: std::collections::HashSet<String> = statuses
                                .iter()
                                .filter(|s| s.running)
                                .map(|s| s.id.clone())
                                .collect();
                            readiness.retain_running(&running_ids);
                            for s in statuses.iter().filter(|s| s.running) {
                                use runtime::readiness::Decision;
                                match readiness.decide(&s.id, std::time::Instant::now()) {
                                    Decision::Ready => {}
                                    Decision::TimedOut => emit_log_line(
                                        &app_handle,
                                        &s.id.to_uppercase(),
                                        "readiness check has not passed after 120 s; treated as ready (check the credentials and the service's logs)",
                                    ),
                                    Decision::Probe => {
                                        // Custom service: cek dari PC sesuai definisinya.
                                        let custom = app_handle
                                            .try_state::<std::sync::Mutex<commands::config::ConfigState>>()
                                            .and_then(|st| st.lock().ok().map(|c| (c.custom_services.clone(), c.service_ports.clone())))
                                            .and_then(|(list, ports)| {
                                                let d = list.into_iter().find(|d| d.id == s.id)?;
                                                let port = ports.get(&d.id).copied().or_else(|| d.ports.first()?.host.parse().ok())?;
                                                Some((d.ready, port))
                                            });
                                        let outcome = match custom {
                                            Some((Some(check), port)) => runtime::probe_custom(&check, port).await,
                                            Some((None, _)) => runtime::readiness::ProbeOutcome::Ready,
                                            None => runtime::probe(&s.id, &s.container_name).await,
                                        };
                                        readiness.record(&s.id, outcome);
                                    }
                                }
                            }

                            statuses
                                .into_iter()
                                .map(|s| {
                                    let up = s.running && readiness.is_ready(&s.id);
                                    (s.id, up)
                                })
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
                                    "KOVA",
                                    &format!(
                                        "[polling] {} not reachable \u{2014} backing off",
                                        runtime::active().label()
                                    ),
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
                    // Snapshot penuh tiap putaran: event per perubahan di
                    // atas terlewat bila start lalu stop terjadi di antara dua
                    // putaran, dan UI tertahan di "starting"/"stopping".
                    let _ = app_handle.emit("container-status-snapshot", &current);
                    *prev = current;
                    is_first_tick = false;
                    }

                    tray::update_tooltip(&app_handle, running_count);

                    tokio::time::sleep(current_delay).await;
                }
            });

            // Polling kedua: RAM untuk status bar dan layar Services, tiap 2 detik
            // selama jendela terlihat. Tiap putaran mengukur proses (Kova, WebView,
            // dev server) dan, bila engine siap, container. Container yang gagal
            // dibaca diberi jeda sendiri (10 s, lalu 30 s) tanpa emit error, karena
            // daemon mati sudah dilaporkan polling status; angka proses tetap jalan.
            let stats_handle = app.handle().clone();
            let stats_daemon_ready = daemon_ready.clone();
            let stats_shutting_down = shutting_down.clone();
            tauri::async_runtime::spawn(async move {
                use commands::stats::{interval_for, mode, sleep_or_mode_change, RamStats};
                let down_delay = tokio::time::Duration::from_secs(30);
                let mut consecutive_failures: u32 = 0;
                let mut containers_at = tokio::time::Instant::now();
                let mut services_mb: Option<u64> = None;
                let mut sys = Some(sysinfo::System::new());

                loop {
                    if stats_shutting_down.load(Ordering::Relaxed) {
                        break;
                    }

                    // Jendela tersembunyi: tidak ada yang melihat angkanya.
                    let Some(base_delay) = interval_for(mode()) else {
                        sleep_or_mode_change(down_delay).await;
                        continue;
                    };

                    let roots = sites::process::running_pids(&stats_handle);
                    let mut s = sys.take().unwrap_or_default();
                    let ((projects_mb, kova_mb), s) = tauri::async_runtime::spawn_blocking(move || {
                        let r = commands::stats::measure_processes(&mut s, &roots);
                        (r, s)
                    })
                    .await
                    .unwrap_or_else(|_| ((0, 0), sysinfo::System::new()));
                    sys = Some(s);

                    // Jangan spawn `docker stats` saat daemon down: itu yang memicu
                    // popup loader-error docker.exe/wsl.exe.
                    if !stats_daemon_ready.load(Ordering::Relaxed) {
                        services_mb = None;
                    } else if tokio::time::Instant::now() >= containers_at {
                        match commands::stats::fetch_container_stats().await {
                            Ok(stats) => {
                                consecutive_failures = 0;
                                services_mb = Some(stats.iter().map(|c| c.mem_mb).sum());
                                // Snapshot kosong juga dikirim, supaya nilai lama
                                // terhapus setelah semua service berhenti.
                                let _ = stats_handle.emit("container-stats-changed", &stats);
                            }
                            Err(_) => {
                                consecutive_failures = consecutive_failures.saturating_add(1);
                                services_mb = None;
                                containers_at = tokio::time::Instant::now()
                                    + match consecutive_failures {
                                        1..=2 => tokio::time::Duration::from_secs(10),
                                        _ => tokio::time::Duration::from_secs(30),
                                    };
                            }
                        }
                    }

                    let measured_at = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis() as u64)
                        .unwrap_or(0);
                    let _ = stats_handle.emit(
                        "ram-stats-changed",
                        &RamStats { services_mb, projects_mb, kova_mb, measured_at },
                    );

                    sleep_or_mode_change(base_delay).await;
                }
            });

            Ok(())
        });

    let app = builder
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    // Set flag shutdown saat app exit / Windows session-end → kedua loop polling
    // break di awal iterasi berikutnya, hindari spawn docker.exe saat teardown.
    app.run(move |handle, event| {
        // Keluar lewat jendela terakhir (Minimize to tray mati) juga
        // menghentikan service dulu. Ditunda sekali; shutdown::begin memanggil
        // app.exit(0) lagi setelah selesai.
        if let tauri::RunEvent::ExitRequested { api, .. } = &event {
            if !shutdown::is_done() {
                api.prevent_exit();
                run_shutting_down.store(true, Ordering::Relaxed);
                shutdown::begin(handle);
                return;
            }
        }
        if let tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit = event {
            run_shutting_down.store(true, Ordering::Relaxed);

            // Dev server tiap project adalah child-process yang memegang port.
            // Tanpa stop eksplisit, port bisa nyangkut setelah Kova ditutup.
            let handle = handle.clone();
            tauri::async_runtime::block_on(async move {
                sites::process::stop_all(&handle).await;
            });
        }
    });
}
