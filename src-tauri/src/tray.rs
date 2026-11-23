//! Menu tray native (hanya Linux, lihat `NATIVE_MENU`; Windows dan macOS
//! memakai popup tray).
//!
//! Isi dari atas: Open Kova, daftar project dengan status (klik = buka di
//! browser bila berjalan, Start bila berhenti), Start/Stop service, submenu
//! PHP/Node untuk versi global, Quit Kova. Tidak ada baris yang hanya label:
//! menu native menampilkan item non-aktif dalam abu-abu, jadi versi Kova dan
//! jumlah service ada di tooltip dan di teks "Stop all services (N running)".
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

use crate::commands::config::{load_config, ConfigState, Project, ProjectGroup};
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
    /// Berjalan, memulai, tidak menjawab, atau keluar: naik ke atas menu.
    pub active: bool,
    /// Kelompok yang ada di config; `None` = Ungrouped.
    pub group_id: Option<String>,
}

/// Jumlah project yang tampil langsung di menu; sisanya masuk submenu, supaya
/// menu tidak memanjang tanpa batas.
pub const PROJECT_LIMIT: usize = 5;

/// Project dalam urutan layar Projects: per kelompok sesuai urutan `groups`,
/// lalu Ungrouped (termasuk id kelompok yang sudah tidak ada).
pub fn ordered_projects<'a>(projects: &'a [Project], groups: &[ProjectGroup]) -> Vec<&'a Project> {
    let known = |id: &Option<String>| id.as_ref().is_some_and(|id| groups.iter().any(|g| &g.id == id));
    let mut out: Vec<&Project> = Vec::with_capacity(projects.len());
    for g in groups {
        out.extend(projects.iter().filter(|p| p.group_id.as_deref() == Some(g.id.as_str())));
    }
    out.extend(projects.iter().filter(|p| !known(&p.group_id)));
    out
}

/// Sisa project di luar menu utama, dipecah menjadi submenu per kelompok
/// ("Acme (3)", lalu "Ungrouped (N)"). Tanpa kelompok: satu submenu
/// "More projects (N)". Kelompok tanpa sisa tidak dibuatkan submenu.
pub fn rest_submenus(rest: &[ProjectEntry], groups: &[ProjectGroup]) -> Vec<(String, Vec<ProjectEntry>)> {
    if rest.is_empty() {
        return Vec::new();
    }
    if groups.is_empty() {
        return vec![(format!("More projects ({})", rest.len()), rest.to_vec())];
    }
    let mut out = Vec::new();
    for g in groups {
        let items: Vec<ProjectEntry> = rest.iter().filter(|e| e.group_id.as_deref() == Some(g.id.as_str())).cloned().collect();
        if !items.is_empty() {
            out.push((format!("{} ({})", g.name, items.len()), items));
        }
    }
    let ungrouped: Vec<ProjectEntry> = rest
        .iter()
        .filter(|e| !e.group_id.as_ref().is_some_and(|id| groups.iter().any(|g| &g.id == id)))
        .cloned()
        .collect();
    if !ungrouped.is_empty() {
        out.push((format!("Ungrouped ({})", ungrouped.len()), ungrouped));
    }
    out
}

/// Urutkan project aktif lebih dulu (urutan asli tetap di tiap kelompok),
/// lalu pisahkan `limit` yang pertama dari sisanya.
pub fn split_projects(entries: &[ProjectEntry], limit: usize) -> (Vec<ProjectEntry>, Vec<ProjectEntry>) {
    let mut sorted: Vec<ProjectEntry> = entries.iter().filter(|e| e.active).cloned().collect();
    sorted.extend(entries.iter().filter(|e| !e.active).cloned());
    let rest = sorted.split_off(limit.min(sorted.len()));
    (sorted, rest)
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
    let active = matches!(state, SiteState::Running | SiteState::Unresponsive | SiteState::Starting | SiteState::Exited);
    ProjectEntry { id: project.id.clone(), label, enabled, active, group_id: project.group_id.clone() }
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
    groups: &[ProjectGroup],
    running: usize,
    php: &[(String, bool)],
    node: &[(String, bool)],
) -> tauri::Result<Menu<tauri::Wry>> {
    let menu = Menu::new(app)?;
    menu.append(&MenuItem::with_id(app, "show", "Open Kova", true, None::<&str>)?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;

    if !projects.is_empty() {
        let (shown, rest) = split_projects(projects, PROJECT_LIMIT);
        for p in &shown {
            menu.append(&MenuItem::with_id(app, format!("project:{}", p.id), &p.label, p.enabled, None::<&str>)?)?;
        }
        for (i, (title, items)) in rest_submenus(&rest, groups).into_iter().enumerate() {
            let more = Submenu::with_id(app, format!("projects_more:{i}"), title, true)?;
            for p in &items {
                more.append(&MenuItem::with_id(app, format!("project:{}", p.id), &p.label, p.enabled, None::<&str>)?)?;
            }
            menu.append(&more)?;
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
    if !NATIVE_MENU {
        return;
    }
    let (projects, groups): (Vec<ProjectEntry>, Vec<ProjectGroup>) = match load_config(app) {
        Ok(cfg) => {
            let runtime = crate::sites::process::snapshot(app);
            let entries = ordered_projects(&cfg.projects, &cfg.groups)
                .into_iter()
                .map(|p| project_entry(p, runtime.iter().find(|r| r.id == p.id)))
                .collect();
            (entries, cfg.groups)
        }
        Err(_) => (Vec::new(), Vec::new()),
    };
    let running = SERVICES_RUNNING.load(Ordering::Relaxed);
    let (php, node) = VERSIONS.lock().unwrap_or_else(|e| e.into_inner()).clone();

    let summary = format!("{:?}|{:?}|{}|{:?}|{:?}", projects, groups, running, php, node);
    {
        let mut shown = SHOWN.lock().unwrap_or_else(|e| e.into_inner());
        if *shown == summary {
            return;
        }
        *shown = summary;
    }

    match build_menu(app, &projects, &groups, running, &php, &node) {
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
    if !NATIVE_MENU {
        return;
    }
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

/// Di Windows dan macOS klik kiri maupun kanan membuka popup (`tray_popup`);
/// menu native hanya dipasang di Linux, karena di sana ikon tray tidak
/// mengirim event klik sehingga popup tidak bisa dibuka.
const NATIVE_MENU: bool = cfg!(target_os = "linux");

fn set_global_version(app: &AppHandle, tool: &'static str, version: &str) {
    let app = app.clone();
    let version = version.to_string();
    tauri::async_runtime::spawn(async move {
        match crate::mise::runtime_set_global(app.clone(), tool.to_string(), version.clone(), None).await {
            Ok(()) => {
                let _ = app.emit("runtime-global-changed", serde_json::json!({ "tool": tool, "version": version }));
                refresh_version_submenus(&app).await;
            }
            Err(e) => emit_log_line(&app, "TRAY", &format!("could not switch {tool} to {version}: {e}")),
        }
    });
}

pub fn init(app: &AppHandle) -> tauri::Result<()> {
    // Menu awal tanpa versi PHP/Node; diisi setelah listing selesai.
    let menu = build_menu(app, &[], &[], 0, &[], &[])?;

    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or(tauri::Error::AssetNotFound("default window icon".into()))?;

    let mut builder = TrayIconBuilder::with_id("main")
        .icon(icon)
        .tooltip(tooltip_text(&app.package_info().version.to_string(), 0));
    if NATIVE_MENU {
        builder = builder.menu(&menu).show_menu_on_left_click(true);
    }
    builder
        .on_tray_icon_event(|tray, event| {
            if let tauri::tray::TrayIconEvent::Click {
                button: tauri::tray::MouseButton::Left | tauri::tray::MouseButton::Right,
                button_state: tauri::tray::MouseButtonState::Up,
                rect,
                ..
            } = event
            {
                crate::tray_popup::toggle(tray.app_handle(), &rect);
            }
        })
        .on_menu_event(|app, event| {
            let id = event.id.as_ref();
            if let Some(pid) = id.strip_prefix("project:") {
                on_project(app, pid);
                return;
            }
            // Versi global: id `php:<ver>` / `node:<ver>`, dijalankan langsung
            // (`mise use -g`) seperti di Runtimes dan popup tray.
            for tool in ["php", "node"] {
                if let Some(ver) = id.strip_prefix(&format!("{tool}:")) {
                    set_global_version(app, tool, ver);
                    return;
                }
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
                // Hentikan dev server dan service dulu, baru keluar.
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
            name: "acme-api".into(),
            path: path.into(),
            profile_id: None,
            kind: "laravel".into(),
            command: command.into(),
            port,
            auto_start: false,
            env: Default::default(),
            start_services: false,
            ..Default::default()
        }
    }

    fn rt(state: SiteState, exit_code: Option<i32>) -> SiteRuntime {
        SiteRuntime { id: "p1".into(), state, exit_code, last_line: None, env_name: None, printed_port: None }
    }

    #[test]
    fn project_entries_name_state_and_action() {
        let p = project("C:\\dev\\acme\\acme-api", "php artisan serve --port {port}", 8801);
        let e = project_entry(&p, Some(&rt(SiteState::Running, None)));
        assert_eq!(e.label, "● acme-api — running · open localhost:8801");
        assert!(e.enabled);
        assert_eq!(e.id, "p1");

        let e = project_entry(&p, None);
        assert_eq!(e.label, "○ acme-api — stopped · start");
        assert!(e.enabled);

        let e = project_entry(&p, Some(&rt(SiteState::Exited, Some(1))));
        assert_eq!(e.label, "■ acme-api — exited (1) · start");

        let e = project_entry(&p, Some(&rt(SiteState::Starting, None)));
        assert!(!e.enabled);
        assert!(e.label.contains("starting"));

        let e = project_entry(&p, Some(&rt(SiteState::Unresponsive, None)));
        assert!(e.label.contains("not answering"));
        assert!(e.enabled);
    }

    #[test]
    fn unfinished_projects_are_disabled() {
        let e = project_entry(&project("C:\\dev\\acme\\acme-api", "  ", 8801), None);
        assert_eq!((e.label.as_str(), e.enabled), ("○ acme-api — not set up yet", false));
        assert!(!project_entry(&project("", "npm run dev", 3000), None).enabled);
    }

    #[test]
    fn active_projects_first_and_the_rest_in_more() {
        let e = |id: &str, active: bool| ProjectEntry { id: id.into(), label: id.into(), enabled: true, active, group_id: None };
        let list = vec![e("a", false), e("b", true), e("c", false), e("d", true), e("e", false), e("f", false), e("g", false)];
        let (shown, rest) = split_projects(&list, 5);
        let ids = |v: &[ProjectEntry]| v.iter().map(|p| p.id.clone()).collect::<Vec<_>>();
        assert_eq!(ids(&shown), ["b", "d", "a", "c", "e"]);
        assert_eq!(ids(&rest), ["f", "g"]);

        let (shown, rest) = split_projects(&list[..3], 5);
        assert_eq!(shown.len(), 3);
        assert!(rest.is_empty());

        assert!(project_entry(&project("C:\\dev\\acme\\acme-api", "npm run dev", 3000), Some(&rt(SiteState::Exited, Some(1)))).active);
        assert!(!project_entry(&project("C:\\dev\\acme\\acme-api", "npm run dev", 3000), None).active);
    }

    #[test]
    fn projects_follow_group_order_and_the_rest_goes_into_group_submenus() {
        let g = |id: &str, name: &str| ProjectGroup { id: id.into(), name: name.into(), collapsed: true };
        let groups = vec![g("g-shop", "Shop"), g("g-acme", "Acme")];
        let p = |id: &str, group: Option<&str>| Project { id: id.into(), name: id.into(), group_id: group.map(Into::into), ..Default::default() };
        let projects = vec![
            p("docs-site", None),
            p("acme-api", Some("g-acme")),
            p("shop-web", Some("g-shop")),
            p("acme-web", Some("g-acme")),
            p("old", Some("deleted")),
            p("shop-api", Some("g-shop")),
            p("billing-worker", Some("g-acme")),
        ];
        let ordered: Vec<&str> = ordered_projects(&projects, &groups).iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ordered, ["shop-web", "shop-api", "acme-api", "acme-web", "billing-worker", "docs-site", "old"]);

        let entries: Vec<ProjectEntry> = ordered_projects(&projects, &groups)
            .into_iter()
            .map(|p| {
                let mut e = project_entry(p, None);
                e.active = p.id == "billing-worker";
                e
            })
            .collect();
        let (shown, rest) = split_projects(&entries, PROJECT_LIMIT);
        let ids = |v: &[ProjectEntry]| v.iter().map(|p| p.id.clone()).collect::<Vec<_>>();
        assert_eq!(ids(&shown), ["billing-worker", "shop-web", "shop-api", "acme-api", "acme-web"]);
        let subs = rest_submenus(&rest, &groups);
        assert_eq!(subs.iter().map(|(t, _)| t.as_str()).collect::<Vec<_>>(), ["Ungrouped (2)"]);
        assert_eq!(ids(&subs[0].1), ["docs-site", "old"]);

        // Semua tidak aktif dengan batas 2: kelompok berurutan, kosong dilewati.
        let quiet: Vec<ProjectEntry> = entries.iter().cloned().map(|mut e| { e.active = false; e }).collect();
        let (_, rest) = split_projects(&quiet, 2);
        let subs = rest_submenus(&rest, &groups);
        assert_eq!(subs.iter().map(|(t, _)| t.as_str()).collect::<Vec<_>>(), ["Acme (3)", "Ungrouped (2)"]);

        // Tanpa kelompok: satu submenu seperti sebelumnya.
        let subs = rest_submenus(&rest, &[]);
        assert_eq!(subs.iter().map(|(t, _)| t.as_str()).collect::<Vec<_>>(), ["More projects (5)"]);
        assert!(rest_submenus(&[], &groups).is_empty());
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
