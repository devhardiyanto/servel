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

use crate::commands::config::{load_config, Profile, Project};
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
    start_profile_services(&app, &project);
    process::start(&app, &project).await
}

#[tauri::command]
pub async fn site_restart(app: AppHandle, project: Project) -> Result<(), String> {
    start_profile_services(&app, &project);
    process::restart(&app, &project).await
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
        log(format!("menyalakan service profil: {}", ids.join(", ")), "stdout");
        let result = match crate::commands::services::load_services_internal(&app).await {
            Ok(defs) => crate::runtime::start(&app, &defs, &ids).await,
            Err(e) => Err(e),
        };
        if let Err(e) = result {
            log(format!("service profil gagal dinyalakan: {}", e), "stderr");
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

/// Saat Servel dibuka: bersihkan dev server sisa sesi sebelumnya, lalu
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
        for project in cfg.projects.iter().filter(|p| p.auto_start) {
            start_profile_services(&app, project);
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

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
const CADDY_SUBJECT: &str = "*Caddy Local Authority*";

/// Sertifikat Caddy yang masih dipercaya, dipisah menurut lokasinya:
/// `user` hanya di store pengguna (hapus tanpa admin), `machine` di store
/// komputer (butuh admin). `Cert:\CurrentUser\Root` di PowerShell juga
/// menampilkan sertifikat komputer, jadi `user` = CurrentUser dikurangi
/// LocalMachine.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
#[derive(Debug, Default, PartialEq)]
pub struct CertScopes {
    pub user: Vec<String>,
    pub machine: Vec<String>,
}

/// Parse output skrip daftar sertifikat: baris `U:<thumb>,<thumb>` dan
/// `M:<thumb>`. Thumbprint selain 40 hex dibuang supaya tidak pernah
/// diteruskan ke certutil.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub fn parse_cert_scopes(stdout: &str) -> CertScopes {
    let thumbs = |rest: &str| -> Vec<String> {
        rest.split(',')
            .map(str::trim)
            .filter(|t| t.len() == 40 && t.chars().all(|c| c.is_ascii_hexdigit()))
            .map(str::to_ascii_uppercase)
            .collect()
    };
    let mut out = CertScopes::default();
    for line in stdout.lines().map(str::trim) {
        if let Some(rest) = line.strip_prefix("U:") {
            out.user = thumbs(rest);
        } else if let Some(rest) = line.strip_prefix("M:") {
            out.machine = thumbs(rest);
        }
    }
    out
}

#[cfg(target_os = "windows")]
fn hidden(program: &str) -> std::process::Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    let mut cmd = std::process::Command::new(program);
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

#[cfg(target_os = "windows")]
fn cert_scopes() -> CertScopes {
    let script = format!(
        "$m=@(Get-ChildItem Cert:\\LocalMachine\\Root | Where-Object {{ $_.Subject -like '{s}' }} | ForEach-Object Thumbprint); \
         $u=@(Get-ChildItem Cert:\\CurrentUser\\Root | Where-Object {{ $_.Subject -like '{s}' }} | ForEach-Object Thumbprint | Where-Object {{ $m -notcontains $_ }}); \
         'U:' + ($u -join ','); 'M:' + ($m -join ',')",
        s = CADDY_SUBJECT
    );
    hidden("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .map(|o| parse_cert_scopes(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or_default()
}

#[cfg(target_os = "windows")]
fn cert_present() -> bool {
    let s = cert_scopes();
    !s.user.is_empty() || !s.machine.is_empty()
}

/// macOS/Linux: Servel tidak pernah memasang sertifikat di sana.
#[cfg(not(target_os = "windows"))]
fn cert_present() -> bool {
    false
}

/// Hapus dari store pengguna. Windows selalu menampilkan dialog konfirmasi
/// untuk store Root; menolaknya membuat certutil gagal.
#[cfg(target_os = "windows")]
fn remove_user_cert(thumb: &str) -> Result<(), String> {
    let out = hidden("certutil.exe")
        .args(["-user", "-delstore", "Root", thumb])
        .output()
        .map_err(|e| format!("certutil tidak bisa dijalankan: {}", e))?;
    if out.status.success() {
        return Ok(());
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let reason = text
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with("CertUtil:") && !l.contains("-delstore command"))
        .unwrap_or("certutil gagal");
    Err(format!("store pengguna: {}", reason))
}

/// Hapus dari store komputer lewat certutil yang dijalankan sebagai admin
/// (memunculkan UAC). Batal UAC → kode 1223.
#[cfg(target_os = "windows")]
fn remove_machine_cert(thumb: &str) -> Result<(), String> {
    let script = format!(
        "try {{ $p = Start-Process -FilePath 'certutil.exe' -ArgumentList '-delstore','Root','{}' \
         -Verb RunAs -WindowStyle Hidden -Wait -PassThru; exit $p.ExitCode }} catch {{ exit 1223 }}",
        thumb
    );
    let status = hidden("powershell.exe")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &script])
        .status()
        .map_err(|e| format!("powershell tidak bisa dijalankan: {}", e))?;
    match status.code() {
        Some(0) => Ok(()),
        Some(1223) => Err("store komputer: izin admin (UAC) ditolak".to_string()),
        Some(c) => Err(format!("store komputer: certutil gagal (kode {})", c)),
        None => Err("store komputer: proses dihentikan".to_string()),
    }
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

/// Cabut sertifikat lokal Caddy. Yang ada di store pengguna dihapus dengan
/// dialog konfirmasi Windows; yang ada di store komputer butuh UAC. Error
/// menyebut lokasi dan alasan sebenarnya, bukan tebakan.
#[tauri::command]
pub async fn legacy_remove_cert() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        tauri::async_runtime::spawn_blocking(|| {
            let scopes = cert_scopes();
            let mut errors: Vec<String> = Vec::new();
            for t in &scopes.user {
                if let Err(e) = remove_user_cert(t) {
                    errors.push(e);
                }
            }
            for t in &scopes.machine {
                if let Err(e) = remove_machine_cert(t) {
                    errors.push(e);
                }
            }
            if !cert_present() {
                return Ok(());
            }
            if errors.is_empty() {
                errors.push("sertifikat masih terdaftar setelah dihapus".to_string());
            }
            errors.dedup();
            Err(format!("Sertifikat belum terhapus — {}", errors.join("; ")))
        })
        .await
        .map_err(|e| format!("gagal menjalankan pembersihan: {}", e))?
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn services_for_follows_option_and_profile() {
        let profiles = vec![Profile {
            id: "astra".into(),
            name: "Astra".into(),
            service_ids: vec!["mysql".into(), "redis".into()],
        }];
        let mut p: Project = serde_json::from_str(r#"{"id":"1","name":"api","profileId":"astra"}"#).unwrap();
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

    const A: &str = "0123456789ABCDEF0123456789ABCDEF01234567";
    const B: &str = "89abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn cert_scopes_split_and_sanitize() {
        let out = format!("U:{A}\r\nM:{B},not-a-thumb,{A}; rm -rf\r\n");
        let s = parse_cert_scopes(&out);
        assert_eq!(s.user, vec![A.to_string()]);
        // Huruf kecil dinormalkan; nilai yang bukan 40 hex dibuang.
        assert_eq!(s.machine, vec![B.to_ascii_uppercase()]);
        assert_eq!(parse_cert_scopes("U:\nM:\n"), CertScopes::default());
        assert_eq!(parse_cert_scopes(""), CertScopes::default());
    }
}
