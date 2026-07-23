use std::sync::Mutex;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager};

use crate::commands::config::ConfigState;
use crate::commands::compose::compose_path;
use crate::commands::services::load_services_internal;
use crate::commands::util::{emit_log_line, stream_and_wait_app};

/// Bangun menu tray lengkap termasuk submenu quick-switch versi PHP/Node.
/// `php`/`node` = daftar (versi, aktif). Kosong → submenu berisi item disabled.
/// Item versi ber-id `php:<versi>` / `node:<versi>` → di-handle di on_menu_event.
fn build_menu(
    app: &AppHandle,
    php: &[(String, bool)],
    node: &[(String, bool)],
) -> tauri::Result<Menu<tauri::Wry>> {
    let show = MenuItem::with_id(app, "show", "Show Servel", true, None::<&str>)?;
    let start_all = MenuItem::with_id(app, "start_all", "Start all selected", true, None::<&str>)?;
    let stop_all = MenuItem::with_id(app, "stop_all", "Stop all", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Servel", true, None::<&str>)?;

    let php_submenu = build_version_submenu(app, "PHP", "php", php)?;
    let node_submenu = build_version_submenu(app, "Node", "node", node)?;

    Menu::with_items(
        app,
        &[
            &show,
            &PredefinedMenuItem::separator(app)?,
            &php_submenu,
            &node_submenu,
            &PredefinedMenuItem::separator(app)?,
            &start_all,
            &stop_all,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )
}

/// Submenu versi: satu item per versi (id `<kind>:<versi>`), versi aktif diberi
/// prefix ●. Kalau kosong → satu item disabled sebagai placeholder.
fn build_version_submenu(
    app: &AppHandle,
    title: &str,
    kind: &str,
    versions: &[(String, bool)],
) -> tauri::Result<Submenu<tauri::Wry>> {
    if versions.is_empty() {
        let empty = MenuItem::with_id(
            app,
            format!("{}_none", kind),
            "Tidak ada versi terpasang",
            false,
            None::<&str>,
        )?;
        return Submenu::with_items(app, title, true, &[&empty]);
    }

    let mut items = Vec::with_capacity(versions.len());
    for (ver, active) in versions {
        let label = if *active {
            format!("● {}", ver)
        } else {
            format!("   {}", ver)
        };
        items.push(MenuItem::with_id(
            app,
            format!("{}:{}", kind, ver),
            label,
            true,
            None::<&str>,
        )?);
    }
    let refs: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> =
        items.iter().map(|i| i as &dyn tauri::menu::IsMenuItem<tauri::Wry>).collect();
    Submenu::with_items(app, title, true, &refs)
}

/// Ambil daftar versi PHP & Node lalu rebuild menu tray (set_menu). Dipanggil
/// async setelah init karena listing versi butuh spawn subprocess (phpvm/fnm).
async fn refresh_version_submenus(app: &AppHandle) {
    let php: Vec<(String, bool)> = crate::commands::php::php_list_installed()
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|v| (v.version, v.active))
        .collect();
    let node: Vec<(String, bool)> = crate::commands::node::node_list_installed()
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|v| (v.version, v.active))
        .collect();

    match build_menu(app, &php, &node) {
        Ok(menu) => {
            if let Some(tray) = app.tray_by_id("main") {
                let _ = tray.set_menu(Some(menu));
            }
        }
        Err(e) => emit_log_line(app, "TRAY", &format!("rebuild menu gagal: {}", e)),
    }
}

pub fn init(app: &AppHandle) -> tauri::Result<()> {
    // Menu awal dengan submenu versi kosong (placeholder) — di-refresh async
    // setelah init lewat refresh_version_submenus.
    let menu = build_menu(app, &[], &[])?;

    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or(tauri::Error::AssetNotFound("default window icon".into()))?;

    TrayIconBuilder::with_id("main")
        .icon(icon)
        .tooltip("Servel — idle")
        .menu(&menu)
        .on_menu_event(|app, event| {
            let id = event.id.as_ref();
            // Quick-switch versi: id `php:<ver>` / `node:<ver>` → emit ke frontend,
            // reuse php_switch/node_switch (butuh Window untuk streaming).
            if let Some(ver) = id.strip_prefix("php:") {
                let _ = app.emit(
                    "tray-switch-version",
                    serde_json::json!({ "kind": "php", "version": ver }),
                );
                return;
            }
            if let Some(ver) = id.strip_prefix("node:") {
                let _ = app.emit(
                    "tray-switch-version",
                    serde_json::json!({ "kind": "node", "version": ver }),
                );
                return;
            }
            match id {
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
                        emit_log_line(&app, "TRAY", "no selected services");
                        return;
                    }

                    let _ = app.emit(
                        "services-action",
                        serde_json::json!({
                            "action": "start",
                            "services": selected_ids.clone()
                        }),
                    );

                    if let Err(e) = tray_services_start(&app, selected_ids).await {
                        emit_log_line(&app, "TRAY", &format!("start_all error: {}", e));
                    }
                });
            }
            "stop_all" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let selected_ids = {
                        let state = app.state::<Mutex<ConfigState>>();
                        let cfg = state.lock().unwrap();
                        cfg.selected_service_ids.clone()
                    };

                    let _ = app.emit(
                        "services-action",
                        serde_json::json!({
                            "action": "stop_all",
                            "services": selected_ids
                        }),
                    );

                    if let Err(e) = tray_services_stop_all(&app).await {
                        emit_log_line(&app, "TRAY", &format!("stop_all error: {}", e));
                    }
                });
            }
            "quit" => {
                app.exit(0);
            }
            _ => {}
            }
        })
        .build(app)?;

    // Populate submenu versi PHP/Node secara async (listing butuh subprocess).
    let refresh_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        refresh_version_submenus(&refresh_handle).await;
    });

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

    let mut cmd = new_docker_cmd();
    cmd.args(["compose", "-f", &path_str, "up", "-d", "--remove-orphans"]);
    stream_and_wait_app(cmd, app).await
}

async fn tray_services_stop_all(app: &AppHandle) -> Result<(), String> {
    let compose_file = compose_path();
    if !compose_file.exists() {
        return Ok(());
    }

    let path_str = compose_file
        .to_str()
        .ok_or("compose path tidak valid UTF-8")?
        .to_string();

    let mut cmd = new_docker_cmd();
    cmd.args(["compose", "-f", &path_str, "down"]);
    stream_and_wait_app(cmd, app).await
}

fn new_docker_cmd() -> tokio::process::Command {
    #[cfg_attr(not(target_os = "windows"), allow(unused_mut))]
    let mut cmd = tokio::process::Command::new("docker");
    #[cfg(target_os = "windows")]
    {
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}
