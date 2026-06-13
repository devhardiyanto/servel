mod commands;
mod tray;
mod watcher;

use std::collections::HashMap;
use std::sync::Mutex;
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
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
        .setup(|app| {
            // Tray init — non-fatal: kalau gagal (mis. Linux tanpa appindicator) log warning + lanjut.
            if let Err(e) = tray::init(app.handle()) {
                eprintln!("[tray] init gagal, skip tray: {}", e);
            }

            let app_handle = app.handle().clone();
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

                let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(3));
                let mut is_first_tick = true;

                loop {
                    interval.tick().await;

                    let current = match commands::services::services_status_internal().await {
                        Ok(statuses) => statuses
                            .into_iter()
                            .map(|s| (s.id, s.running))
                            .collect::<HashMap<String, bool>>(),
                        Err(err) => {
                            let _ = app_handle.emit(
                                "cmd-output",
                                serde_json::json!({
                                    "line": format!("[polling] services_status error: {}", err),
                                    "stream": "stderr",
                                    "source": "SERVEL",
                                    "level": "warn",
                                }),
                            );
                            continue;
                        }
                    };

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

                    let running_count = current.values().filter(|&&r| r).count();
                    *prev = current;
                    is_first_tick = false;

                    tray::update_tooltip(&app_handle, running_count);
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
