//! Menu tray (v3.6.0, canvas "Tray & accessibility").
//!
//! Isi dari atas: Open Kova, daftar project dengan status (klik = buka di
//! browser bila berjalan, Start bila berhenti), Start/Stop service, submenu
//! PHP/Node untuk versi global, Quit Kova.
//!
//! v3.6.1: menu native Windows menampilkan item non-aktif dalam abu-abu,
//! jadi tidak ada lagi baris yang hanya label ("Projects", "No projects yet",
//! "Services: none running"). Versi Kova dan jumlah service pindah ke tooltip
//! dan ke teks "Stop all services (N running)".
//!
//! Menu dibangun ulang saat status project berubah (`site-status-changed`),
//! saat jumlah service berjalan berubah (polling status), saat config ditulis,
//! dan setelah daftar versi PHP/Node dibaca. Pembangunan ulang dilewati bila
//! isinya sama dengan menu yang sedang tampil.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Listener, Manager};

use crate::commands::config::{load_config, ConfigState, Project};
use crate::commands::services::load_services_internal;
use crate::commands::util::emit_log_line;
use crate::sites::process::{SiteRuntime, SiteState};

/// Versi PHP dan Node terakhir yang dibaca: (versi, aktif).
type Versions = Vec<(String, bool)>;
static VERSIONS: Mutex<(Versions, Versions)> = Mutex::new((Vec::new(), Vec::new()));
/// Jumlah service berjalan menurut polling status terakhir.
static SERVICES_RUNNING: AtomicUsize = AtomicUsize::new(0);
/// Ringkasan menu yang sedang tampil, untuk melewati pembangunan ulang yang sama.
static SHOWN: Mutex<String> = Mutex::new(String::new());

/// Satu baris project di menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectEntry {
    pub id: String,
    pub label: String,
    pub enabled: bool,
}

/// Teks dan status aktif satu project. Bentuk penanda berbeda per status dan
/// statusnya selalu ditulis, jadi tidak bergantung pada warna.
pub fn project_entry(project: &Project, runtime: Option<&SiteRuntime>) -> ProjectEntry {
    let name = &project.name;
    let state = runtime.map(|r| r.state).unwrap_or(SiteState::Stopped);
    let runnable = !project.path.is_empty() && !project.command.trim().is_empty() && project.port >= 1024;
    let (label, enabled) = match state {
        SiteState::Running => (format!("● {} — running · open localhost:{}", name, project.port), true),
        SiteState::Unresponsive => (format!("◐ {} — not answering · open localhost:{}", name, project.port), true),
        SiteState::Starting => (format!("◌ {} — starting…", name), false),
        SiteState::Exited if runnable => {
            let code = runtime
                .and_then(|r| r.exit_code)
                .map(|c| format!("exited ({})", c))
                .unwrap_or_else(|| "exited".into());
            (format!("■ {} — {} · start", name, code), true)
        }
        _ if runnable => (format!("○ {} — stopped · start", name), true),
        _ => (format!("○ {} — not set up yet", name), false),
    };
    ProjectEntry { id: project.id.clone(), label, enabled }
}

/// Teks "Stop all services"; tidak ditampilkan bila tidak ada yang berjalan.
pub fn stop_label(running: usize) -> Option<String> {
    (running > 0).then(|| format!("Stop all services ({} running)", running))
}

/// Tooltip ikon tray, dengan versi Kova.
pub fn tooltip_text(version: &str, running: usize) -> String {
    match running {
        0 => format!("Kova {} — idle", version),
        1 => format!("Kova {} — 1 service running", version),
        n => format!("Kova {} — {} services running", version, n),
    }
}

fn build_menu(
    app: &AppHandle,
    projects: &[ProjectEntry],
    running: usize,
    php: &[(String, bool)],
    node: &[(String, bool)],
) -> tauri::Result<Menu<tauri::Wry>> {
    let menu = Menu::new(app)?;
    menu.append(&MenuItem::with_id(app, "show", "Open Kova", true, None::<&str>)?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;

    if !projects.is_empty() {
        for p in projects {
            menu.append(&MenuItem::with_id(app, format!("project:{}", p.id), &p.label, p.enabled, None::<&str>)?)?;
        }
        menu.append(&PredefinedMenuItem::separator(app)?)?;
    }

    menu.append(&MenuItem::with_id(app, "start_all", "Start selected services", true, None::<&str>)?)?;
    if let Some(label) = stop_label(running) {
        menu.append(&MenuItem::with_id(app, "stop_all", label, true, None::<&str>)?)?;
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;

    menu.append(&build_version_submenu(app, "PHP (global)", "php", php)?)?;
    menu.append(&build_version_submenu(app, "Node (global)", "node", node)?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;

    menu.append(&MenuItem::with_id(app, "quit", "Quit Kova", true, None::<&str>)?)?;
    Ok(menu)
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
            "No version installed",
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

/// Bangun ulang menu dari keadaan sekarang. Murah: membaca config dari disk dan
/// status project dari memori; tidak menjalankan subprocess.
pub fn refresh(app: &AppHandle) {
    let projects: Vec<ProjectEntry> = match load_config(app) {
        Ok(cfg) => {
            let runtime = crate::sites::process::snapshot(app);
            cfg.projects
                .iter()
                .map(|p| project_entry(p, runtime.iter().find(|r| r.id == p.id)))
                .collect()
        }
        Err(_) => Vec::new(),
    };
    let running = SERVICES_RUNNING.load(Ordering::Relaxed);
    let (php, node) = VERSIONS.lock().unwrap_or_else(|e| e.into_inner()).clone();

    let summary = format!("{:?}|{}|{:?}|{:?}", projects, running, php, node);
    {
        let mut shown = SHOWN.lock().unwrap_or_else(|e| e.into_inner());
        if *shown == summary {
            return;
        }
        *shown = summary;
    }

    match build_menu(app, &projects, running, &php, &node) {
        Ok(menu) => {
            if let Some(tray) = app.tray_by_id("main") {
                let _ = tray.set_menu(Some(menu));
            }
        }
        Err(e) => emit_log_line(app, "TRAY", &format!("rebuilding the menu failed: {}", e)),
    }
}

/// Baca daftar versi PHP & Node lalu bangun ulang menu. Async karena listing
/// versi menjalankan mise.
///
/// Penanda aktif (●) diambil dari `php_get_active()` / `node_get_active()`,
/// sumber yang sama dengan frontend.
pub(crate) async fn refresh_version_submenus(app: &AppHandle) {
    let php_active = crate::commands::php::php_get_active().await.ok().flatten();
    let php: Versions = crate::commands::php::php_list_installed()
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|v| {
            let active = php_active.as_deref() == Some(v.version.as_str());
            (v.version, active)
        })
        .collect();

    let node_active = crate::commands::node::node_get_active().await.ok().flatten();
    let node: Versions = crate::commands::node::node_list_installed()
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|v| {
            let active = node_active.as_deref() == Some(v.version.as_str());
            (v.version, active)
        })
        .collect();

    *VERSIONS.lock().unwrap_or_else(|e| e.into_inner()) = (php, node);
    refresh(app);
}

/// Klik project: buka di browser bila berjalan, Start bila berhenti.
fn on_project(app: &AppHandle, id: &str) {
    let Ok(cfg) = load_config(app) else { return };
    let Some(project) = cfg.projects.into_iter().find(|p| p.id == id) else { return };
    let state = crate::sites::process::snapshot(app)
        .into_iter()
        .find(|r| r.id == id)
        .map(|r| r.state)
        .unwrap_or(SiteState::Stopped);
    match state {
        SiteState::Running | SiteState::Unresponsive => {
            use tauri_plugin_opener::OpenerExt;
            let url = format!("http://localhost:{}", project.port);
            if let Err(e) = app.opener().open_url(&url, None::<&str>) {
                emit_log_line(app, "TRAY", &format!("could not open {}: {}", url, e));
            }
        }
        SiteState::Starting => {}
        SiteState::Stopped | SiteState::Exited => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = crate::sites::start_with_services(&app, &project).await {
                    let _ = app.emit(
                        "cmd-output",
                        serde_json::json!({ "line": format!("could not start: {}", e), "stream": "stderr", "source": project.name }),
                    );
                }
            });
        }
    }
}

pub fn init(app: &AppHandle) -> tauri::Result<()> {
    // Menu awal tanpa versi PHP/Node; diisi setelah listing selesai.
    let menu = build_menu(app, &[], 0, &[], &[])?;

    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or(tauri::Error::AssetNotFound("default window icon".into()))?;

    TrayIconBuilder::with_id("main")
        .icon(icon)
        .tooltip(tooltip_text(&app.package_info().version.to_string(), 0))
        .menu(&menu)
        .on_menu_event(|app, event| {
            let id = event.id.as_ref();
            if let Some(pid) = id.strip_prefix("project:") {
                on_project(app, pid);
                return;
            }
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
                    let _ = w.unminimize();
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
                // v3.5.1: hentikan dev server dan service dulu, baru keluar.
                crate::shutdown::begin(app);
            }
            _ => {}
            }
        })
        .build(app)?;

    // Status project berubah → baris project ikut berubah.
    let h = app.clone();
    app.listen_any("site-status-changed", move |_| refresh(&h));

    // Isi daftar project sekarang, lalu versi PHP/Node secara async.
    refresh(app);
    let refresh_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        refresh_version_submenus(&refresh_handle).await;
    });

    Ok(())
}

/// Update tooltip dan baris service berdasarkan jumlah container running.
/// Dipanggil setiap putaran polling status.
pub fn update_tooltip(app: &AppHandle, running_count: usize) {
    if crate::shutdown::in_progress() {
        return;
    }
    if let Some(tray) = app.tray_by_id("main") {
        let text = tooltip_text(&app.package_info().version.to_string(), running_count);
        let _ = tray.set_tooltip(Some(&text));
    }
    if SERVICES_RUNNING.swap(running_count, Ordering::Relaxed) != running_count {
        refresh(app);
    }
}

async fn tray_services_start(app: &AppHandle, services: Vec<String>) -> Result<(), String> {
    let defs = load_services_internal(app).await?;
    crate::runtime::start(app, &defs, &services).await
}

async fn tray_services_stop_all(app: &AppHandle) -> Result<(), String> {
    let defs = load_services_internal(app).await?;
    crate::runtime::stop_all(app, &defs).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(path: &str, command: &str, port: u16) -> Project {
        Project {
            id: "p1".into(),
            name: "ce-console-api".into(),
            path: path.into(),
            profile_id: None,
            kind: "laravel".into(),
            command: command.into(),
            port,
            auto_start: false,
            env: Default::default(),
            start_services: false,
        }
    }

    fn rt(state: SiteState, exit_code: Option<i32>) -> SiteRuntime {
        SiteRuntime { id: "p1".into(), state, exit_code, last_line: None }
    }

    #[test]
    fn project_entries_name_state_and_action() {
        let p = project("D:\\dev\\ce", "php artisan serve --port {port}", 8801);
        let e = project_entry(&p, Some(&rt(SiteState::Running, None)));
        assert_eq!(e.label, "● ce-console-api — running · open localhost:8801");
        assert!(e.enabled);
        assert_eq!(e.id, "p1");

        let e = project_entry(&p, None);
        assert_eq!(e.label, "○ ce-console-api — stopped · start");
        assert!(e.enabled);

        let e = project_entry(&p, Some(&rt(SiteState::Exited, Some(1))));
        assert_eq!(e.label, "■ ce-console-api — exited (1) · start");

        let e = project_entry(&p, Some(&rt(SiteState::Starting, None)));
        assert!(!e.enabled);
        assert!(e.label.contains("starting"));

        let e = project_entry(&p, Some(&rt(SiteState::Unresponsive, None)));
        assert!(e.label.contains("not answering"));
        assert!(e.enabled);
    }

    #[test]
    fn unfinished_projects_are_disabled() {
        let e = project_entry(&project("D:\\dev\\ce", "  ", 8801), None);
        assert_eq!((e.label.as_str(), e.enabled), ("○ ce-console-api — not set up yet", false));
        assert!(!project_entry(&project("", "npm run dev", 3000), None).enabled);
    }

    #[test]
    fn service_texts() {
        assert_eq!(stop_label(0), None);
        assert_eq!(stop_label(3).as_deref(), Some("Stop all services (3 running)"));
        assert_eq!(tooltip_text("3.6.1", 0), "Kova 3.6.1 — idle");
        assert_eq!(tooltip_text("3.6.1", 1), "Kova 3.6.1 — 1 service running");
        assert_eq!(tooltip_text("3.6.1", 4), "Kova 3.6.1 — 4 services running");
    }
}
