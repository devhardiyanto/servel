use std::sync::Mutex;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager};

use crate::commands::config::ConfigState;
use crate::commands::compose::compose_path;
use crate::commands::services::load_services_internal;
use crate::commands::util::emit_log_line;

pub fn init(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show Servel", true, None::<&str>)?;
    let start_all =
        MenuItem::with_id(app, "start_all", "Start all selected", true, None::<&str>)?;
    let stop_all = MenuItem::with_id(app, "stop_all", "Stop all", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Servel", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &show,
            &PredefinedMenuItem::separator(app)?,
            &start_all,
            &stop_all,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or(tauri::Error::AssetNotFound("default window icon".into()))?;

    TrayIconBuilder::with_id("main")
        .icon(icon)
        .tooltip("Servel — idle")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
            "start_all" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let selected_ids = {
                        let state = app.state::<Mutex<ConfigState>>();
                        let cfg = state.lock().unwrap();
                        cfg.selected_service_ids.clone()
                    };

                    if selected_ids.is_empty() {
                        emit_log_line(&app, "TRAY", "[TRAY] no selected services");
                        return;
                    }

                    if let Err(e) = tray_services_start(&app, selected_ids).await {
                        emit_log_line(&app, "TRAY", &format!("[TRAY] start_all error: {}", e));
                    }
                });
            }
            "stop_all" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = tray_services_stop_all(&app).await {
                        emit_log_line(&app, "TRAY", &format!("[TRAY] stop_all error: {}", e));
                    }
                });
            }
            "quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .build(app)?;

    Ok(())
}

/// Update tray tooltip berdasarkan jumlah container running.
/// Dipanggil setiap kali event `container-status-changed` diterima.
pub fn update_tooltip(app: &AppHandle, running_count: usize) {
    if let Some(tray) = app.tray_by_id("main") {
        let text = if running_count > 0 {
            format!("Servel — {} running", running_count)
        } else {
            "Servel — idle".to_string()
        };
        let _ = tray.set_tooltip(Some(&text));
    }
}

async fn tray_services_start(app: &AppHandle, services: Vec<String>) -> Result<(), String> {
    use std::process::Stdio;
    use tokio::io::{AsyncBufReadExt, BufReader};

    let defs = load_services_internal(app).await?;
    let yaml = crate::commands::compose::generate_compose(&defs, &services);

    let compose_file = compose_path();
    if let Some(parent) = compose_file.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Gagal buat dir compose: {}", e))?;
    }
    std::fs::write(&compose_file, yaml)
        .map_err(|e| format!("Gagal tulis compose file: {}", e))?;

    let path_str = compose_file
        .to_str()
        .ok_or("compose path tidak valid UTF-8")?
        .to_string();

    let app_out = app.clone();
    let app_err = app.clone();

    let mut cmd = new_docker_cmd();
    cmd.args(["compose", "-f", &path_str, "up", "-d"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Gagal spawn docker: {}", e))?;

    let stdout = child.stdout.take().ok_or("stdout tidak tersedia")?;
    let stderr = child.stderr.take().ok_or("stderr tidak tersedia")?;

    let stdout_task = tokio::spawn(async move {
        let mut reader = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            emit_log_line(&app_out, "TRAY", &line);
        }
    });

    let stderr_task = tokio::spawn(async move {
        let mut reader = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            let _ = app_err.emit(
                "cmd-output",
                serde_json::json!({
                    "line": line,
                    "stream": "stderr",
                    "source": "TRAY",
                }),
            );
        }
    });

    let _ = tokio::join!(stdout_task, stderr_task);

    let status = child
        .wait()
        .await
        .map_err(|e| format!("Gagal menunggu docker: {}", e))?;

    if !status.success() {
        return Err(format!("docker compose up gagal (exit {})", status));
    }

    Ok(())
}

async fn tray_services_stop_all(app: &AppHandle) -> Result<(), String> {
    use std::process::Stdio;
    use tokio::io::{AsyncBufReadExt, BufReader};

    let compose_file = compose_path();
    if !compose_file.exists() {
        return Ok(());
    }

    let path_str = compose_file
        .to_str()
        .ok_or("compose path tidak valid UTF-8")?
        .to_string();

    let app_out = app.clone();
    let app_err = app.clone();

    let mut cmd = new_docker_cmd();
    cmd.args(["compose", "-f", &path_str, "down"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Gagal spawn docker: {}", e))?;

    let stdout = child.stdout.take().ok_or("stdout tidak tersedia")?;
    let stderr = child.stderr.take().ok_or("stderr tidak tersedia")?;

    let stdout_task = tokio::spawn(async move {
        let mut reader = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            emit_log_line(&app_out, "TRAY", &line);
        }
    });

    let stderr_task = tokio::spawn(async move {
        let mut reader = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            let _ = app_err.emit(
                "cmd-output",
                serde_json::json!({
                    "line": line,
                    "stream": "stderr",
                    "source": "TRAY",
                }),
            );
        }
    });

    let _ = tokio::join!(stdout_task, stderr_task);

    let status = child
        .wait()
        .await
        .map_err(|e| format!("Gagal menunggu docker: {}", e))?;

    if !status.success() {
        return Err(format!("docker compose down gagal (exit {})", status));
    }

    Ok(())
}

fn new_docker_cmd() -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new("docker");
    #[cfg(target_os = "windows")]
    {
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}
