//! Project yang dijalankan Kova dan dibuka lewat `localhost:<port>`.
//!
//! - `detect`: kenali jenis project → usulan perintah dan port.
//! - `process`: jalankan/hentikan dev server.

pub mod container;
pub mod detect;
pub mod process;

use serde::Serialize;
use std::collections::HashSet;
use std::path::Path;
use tauri::{AppHandle, Manager};

use crate::commands::config::{load_config, Profile, Project};
use detect::Detection;
use process::SiteRuntime;

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSuggestion {
    pub name: String,
    pub detection: Detection,
    /// Port usulan: port lazim jenisnya, digeser sampai menemukan yang kosong
    /// dan belum dipakai project lain.
    pub port: u16,
    /// Berkas yang membuat jenisnya dikenali, untuk ditampilkan.
    pub evidence: Vec<String>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PortCheck {
    pub free: bool,
    /// Kosong bila bebas; selain itu alasan singkat yang bisa ditampilkan.
    pub reason: String,
    /// Port bebas terdekat di atasnya, bila port ini tidak bisa dipakai.
    pub suggestion: Option<u16>,
}

/// Periksa satu port: dipakai project lain (`taken`) atau program lain.
pub fn check_port(port: u16, taken: &HashSet<u16>, is_free: impl Fn(u16) -> bool) -> PortCheck {
    let reason = if port < 1024 {
        "Use a port from 1024 to 65535.".to_string()
    } else if taken.contains(&port) {
        format!("Another project in Kova uses {}.", port)
    } else if !is_free(port) {
        format!("Another program is listening on {}.", port)
    } else {
        return PortCheck { free: true, reason: String::new(), suggestion: None };
    };
    let suggestion = detect::pick_port(port.saturating_add(1).max(1024), taken, is_free);
    PortCheck { free: false, reason, suggestion }
}

/// Cek port untuk form project. Project yang sedang berjalan memegang port-nya
/// sendiri, jadi port project yang sedang diedit tidak dikirim di `taken` dan
/// `own` = true melewatkan cek bind.
#[tauri::command]
pub async fn project_port_check(port: u16, taken: Vec<u16>, own: Option<bool>) -> PortCheck {
    let taken: HashSet<u16> = taken.into_iter().collect();
    if own.unwrap_or(false) && !taken.contains(&port) && port >= 1024 {
        return PortCheck { free: true, reason: String::new(), suggestion: None };
    }
    check_port(port, &taken, process::port_bindable)
}

/// Kenali project di `path` dan usulkan perintah + port. `taken` = port yang
/// sudah dipakai project lain (dikirim frontend, karena config di disk bisa
/// tertinggal dari perubahan yang belum tersimpan).
#[tauri::command]
pub async fn project_detect(path: String, taken: Vec<u16>) -> Result<ProjectSuggestion, String> {
    let dir = Path::new(&path);
    if !dir.is_dir() {
        return Err(format!("folder not found: {}", path));
    }
    let detection = detect::detect(dir);
    let taken: HashSet<u16> = taken.into_iter().collect();
    let port = detect::pick_port(detection.default_port, &taken, process::port_bindable)
        .ok_or_else(|| "no free port".to_string())?;
    Ok(ProjectSuggestion {
        name: detect::name_from_path(&path),
        evidence: detect::evidence(dir),
        detection,
        port,
    })
}

/// Pilihan perintah untuk field Command.
#[tauri::command]
pub async fn project_scripts(path: String) -> Result<Vec<detect::ScriptOption>, String> {
    let dir = Path::new(&path);
    if !dir.is_dir() {
        return Err(format!("folder not found: {}", path));
    }
    Ok(detect::script_options(dir, cfg!(windows)))
}

#[tauri::command]
pub async fn sites_runtime(app: AppHandle) -> Vec<SiteRuntime> {
    process::snapshot(&app)
}

/// Frontend mengirim project utuh: perubahan yang baru diketik belum tentu
/// sudah tersimpan ke config di disk.
#[tauri::command]
pub async fn site_start(app: AppHandle, project: Project) -> Result<(), String> {
    start_with_services(&app, &project).await
}

#[tauri::command]
pub async fn site_restart(app: AppHandle, project: Project) -> Result<(), String> {
    restart_with_services(&app, &project).await
}

/// Start dev server; service profilnya ikut bila opsinya aktif. Dipakai UI
/// dan CLI (`control`).
pub async fn start_with_services(app: &AppHandle, project: &Project) -> Result<(), String> {
    start_profile_services(app, project);
    process::start(app, project).await
}

pub async fn restart_with_services(app: &AppHandle, project: &Project) -> Result<(), String> {
    start_profile_services(app, project);
    process::restart(app, project).await
}

/// Service yang ikut dinyalakan saat `project` dijalankan: isi profilnya,
/// bila opsi `start_services` aktif dan profilnya masih ada.
pub fn services_for(project: &Project, profiles: &[Profile]) -> Vec<String> {
    if !project.start_services {
        return Vec::new();
    }
    project
        .profile_id
        .as_deref()
        .and_then(|id| profiles.iter().find(|p| p.id == id))
        .map(|p| p.service_ids.clone())
        .unwrap_or_default()
}

/// Nyalakan service profil project di latar belakang. Dev server tidak
/// menunggu: menarik image bisa lama, dan aplikasi baru butuh database saat
/// melayani request. Service lain yang sedang berjalan dibiarkan; kegagalan
/// hanya masuk log.
fn start_profile_services(app: &AppHandle, project: &Project) {
    if !project.start_services {
        return;
    }
    let app = app.clone();
    let project = project.clone();
    tauri::async_runtime::spawn(async move {
        use tauri::Emitter;
        let log = |line: String, stream: &str| {
            let _ = app.emit(
                "cmd-output",
                serde_json::json!({ "line": line, "stream": stream, "source": project.name }),
            );
        };
        let Ok(cfg) = load_config(&app) else { return };
        let ids = services_for(&project, &cfg.profiles);
        if ids.is_empty() {
            return;
        }
        log(format!("starting the profile's services: {}", ids.join(", ")), "stdout");
        let result = match crate::commands::services::load_services_internal(&app).await {
            Ok(defs) => crate::runtime::start(&app, &defs, &ids).await,
            Err(e) => Err(e),
        };
        if let Err(e) = result {
            log(format!("the profile's services could not be started: {}", e), "stderr");
        }
    });
}

#[tauri::command]
pub async fn site_stop(app: AppHandle, id: String) -> Result<(), String> {
    process::stop(&app, &id).await
}

/// Lupakan status project yang sudah dihapus (hanya bila tidak berjalan).
#[tauri::command]
pub async fn site_forget(app: AppHandle, id: String) {
    process::forget(&app, &id);
}

/// Saat Kova dibuka: bersihkan dev server sisa sesi sebelumnya, lalu
/// jalankan project ber-`auto_start`. Gagal start satu
/// project tidak menghalangi yang lain; errornya masuk ke panel log.
pub fn autostart(app: &AppHandle) {
    if let Ok(dir) = app.path().app_data_dir() {
        process::init_records(app, dir.join("running-projects.json"));
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        // Bersihkan sisa sesi yang mati paksa dulu, supaya port-nya bebas.
        process::cleanup_leftovers(&app).await;
        let Ok(cfg) = load_config(&app) else { return };
        if !cfg.auto_start_projects {
            return;
        }
        for project in cfg.projects.iter().filter(|p| p.auto_start) {
            start_profile_services(&app, project);
            if let Err(e) = process::start(&app, project).await {
                use tauri::Emitter;
                let _ = app.emit(
                    "cmd-output",
                    serde_json::json!({
                        "line": format!("could not start on launch: {}", e),
                        "stream": "stderr",
                        "source": project.name,
                    }),
                );
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn port_check_explains_and_suggests() {
        let taken: HashSet<u16> = [3001].into_iter().collect();
        assert!(check_port(3000, &taken, |_| true).free);
        let c = check_port(3001, &taken, |_| true);
        assert_eq!((c.free, c.suggestion), (false, Some(3002)));
        assert!(c.reason.contains("Another project"));
        let c = check_port(3002, &taken, |p| p != 3002);
        assert_eq!(c.suggestion, Some(3003));
        assert!(c.reason.contains("Another program"));
        assert!(!check_port(80, &taken, |_| true).free);
    }

    #[test]
    fn services_for_follows_option_and_profile() {
        let profiles = vec![Profile {
            id: "acme".into(),
            name: "Acme".into(),
            service_ids: vec!["mysql".into(), "redis".into()],
        }];
        let mut p: Project = serde_json::from_str(r#"{"id":"1","name":"api","profileId":"acme"}"#).unwrap();
        // Config lama tanpa field: opsi mati, tidak ada yang dinyalakan.
        assert!(!p.start_services);
        assert!(services_for(&p, &profiles).is_empty());
        p.start_services = true;
        assert_eq!(services_for(&p, &profiles), vec!["mysql".to_string(), "redis".to_string()]);
        // Profil sudah dihapus atau tidak dipilih.
        p.profile_id = Some("gone".into());
        assert!(services_for(&p, &profiles).is_empty());
        p.profile_id = None;
        assert!(services_for(&p, &profiles).is_empty());
    }
}
