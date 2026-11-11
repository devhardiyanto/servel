//! Project yang dijalankan Servel dan dibuka lewat `localhost:<port>` (v2.2).
//!
//! - `detect`: kenali jenis project → usulan perintah dan port.
//! - `process`: jalankan/hentikan dev server.
//! - `legacy_*`: bersihkan sisa fitur domain `.test` (hosts, sertifikat,
//!   binary Caddy) yang dihapus di v2.2.

pub mod detect;
pub mod process;

use serde::Serialize;
use std::collections::HashSet;
use std::path::Path;
use tauri::{AppHandle, Manager};

use crate::commands::config::{load_config, Project};
use crate::commands::hosts;
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
}

/// Kenali project di `path` dan usulkan perintah + port. `taken` = port yang
/// sudah dipakai project lain (dikirim frontend, karena config di disk bisa
/// tertinggal dari perubahan yang belum tersimpan).
#[tauri::command]
pub async fn project_detect(path: String, taken: Vec<u16>) -> Result<ProjectSuggestion, String> {
    let dir = Path::new(&path);
    if !dir.is_dir() {
        return Err(format!("folder tidak ditemukan: {}", path));
    }
    let detection = detect::detect(dir);
    let taken: HashSet<u16> = taken.into_iter().collect();
    let port = detect::pick_port(detection.default_port, &taken, process::port_bindable)
        .ok_or_else(|| "tidak ada port kosong".to_string())?;
    Ok(ProjectSuggestion {
        name: detect::name_from_path(&path),
        detection,
        port,
    })
}

#[tauri::command]
pub async fn sites_runtime(app: AppHandle) -> Vec<SiteRuntime> {
    process::snapshot(&app)
}

/// Frontend mengirim project utuh: perubahan yang baru diketik belum tentu
/// sudah tersimpan ke config di disk.
#[tauri::command]
pub async fn site_start(app: AppHandle, project: Project) -> Result<(), String> {
    process::start(&app, &project).await
}

#[tauri::command]
pub async fn site_restart(app: AppHandle, project: Project) -> Result<(), String> {
    process::restart(&app, &project).await
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

/// Jalankan project ber-`auto_start` saat Servel dibuka. Gagal start satu
/// project tidak menghalangi yang lain; errornya masuk ke panel log.
pub fn autostart(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let Ok(cfg) = load_config(&app) else { return };
        for project in cfg.projects.iter().filter(|p| p.auto_start) {
            if let Err(e) = process::start(&app, project).await {
                use tauri::Emitter;
                let _ = app.emit(
                    "cmd-output",
                    serde_json::json!({
                        "line": format!("tidak bisa dijalankan otomatis: {}", e),
                        "stream": "stderr",
                        "source": project.name,
                    }),
                );
            }
        }
    });
}

// ---------------------------------------------------------------------------
// Sisa fitur domain `.test`
// ---------------------------------------------------------------------------

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct LegacyStatus {
    /// Blok `.test` Servel masih ada di file hosts.
    pub hosts_block: bool,
    /// Sertifikat lokal Caddy masih dipercaya (Windows saja yang bisa dicek).
    pub cert: bool,
}

#[cfg(target_os = "windows")]
fn powershell(script: &str) -> std::process::Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    let mut cmd = std::process::Command::new("powershell.exe");
    cmd.args(["-NoProfile", "-NonInteractive", "-Command", script])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .creation_flags(CREATE_NO_WINDOW);
    cmd
}

/// Caddy memasang root CA-nya ke `CurrentUser\Root` (tanpa elevation).
#[cfg(target_os = "windows")]
fn cert_present() -> bool {
    powershell(
        "if (Get-ChildItem Cert:\\CurrentUser\\Root | \
         Where-Object { $_.Subject -like '*Caddy Local Authority*' }) { exit 0 } else { exit 1 }",
    )
    .status()
    .map(|s| s.success())
    .unwrap_or(false)
}

/// macOS/Linux: Servel tidak pernah memasang sertifikat di sana.
#[cfg(not(target_os = "windows"))]
fn cert_present() -> bool {
    false
}

/// Binary Caddy dan Caddyfile milik Servel tidak dipakai lagi.
fn remove_proxy_files(app: &AppHandle) {
    let Ok(dir) = app.path().app_data_dir() else { return };
    for name in ["caddy.exe", "caddy"] {
        let _ = std::fs::remove_file(dir.join("bin").join(name));
    }
    let _ = std::fs::remove_dir(dir.join("bin"));
    let _ = std::fs::remove_dir_all(dir.join("proxy"));
}

#[tauri::command]
pub async fn legacy_status(app: AppHandle) -> LegacyStatus {
    // Berkas milik Servel sendiri dihapus diam-diam; tidak butuh izin apa pun.
    remove_proxy_files(&app);
    tauri::async_runtime::spawn_blocking(|| LegacyStatus {
        hosts_block: hosts::managed_block_present(),
        cert: cert_present(),
    })
    .await
    .unwrap_or_default()
}

/// Hapus blok `.test` Servel dari file hosts (memunculkan UAC / dialog admin).
#[tauri::command]
pub async fn legacy_cleanup_hosts() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(hosts::remove_managed_block)
        .await
        .map_err(|e| format!("gagal menjalankan pembersihan: {}", e))?
}

/// Cabut sertifikat lokal Caddy dari trust store pengguna. Windows
/// menampilkan dialog konfirmasi; menolaknya membuat sertifikat tetap ada.
#[tauri::command]
pub async fn legacy_remove_cert() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        tauri::async_runtime::spawn_blocking(|| {
            let _ = powershell(
                "Get-ChildItem Cert:\\CurrentUser\\Root | \
                 Where-Object { $_.Subject -like '*Caddy Local Authority*' } | Remove-Item",
            )
            .status();
            if cert_present() {
                Err("Sertifikat belum terhapus (dialog konfirmasi ditolak?).".to_string())
            } else {
                Ok(())
            }
        })
        .await
        .map_err(|e| format!("gagal menjalankan pembersihan: {}", e))?
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok(())
    }
}
