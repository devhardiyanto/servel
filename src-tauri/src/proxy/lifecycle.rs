//! Lifecycle proses Caddy: start / stop / reload / status, plus pemasangan
//! sertifikat CA lokal.
//!
//! Caddy dijalankan sebagai **child-process** (bukan Docker service) supaya
//! `127.0.0.1:PORT` milik host bisa dijangkau apa adanya. Konsekuensinya Servel
//! yang memegang lifecycle-nya — termasuk memastikan proses ikut mati saat app
//! ditutup, agar port `:80`/`:443` tidak nyangkut.

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::process::Child;

use super::binary::{caddy_command, caddy_path};
use super::caddyfile::{render, TlsMode, ADMIN_ADDR};
use crate::commands::config::load_config;
use crate::commands::services::probe_host_port;

/// Port yang dibind Caddy. Konflik di sini bikin start gagal total, jadi
/// diprobe lebih dulu supaya pesannya jelas alih-alih "proses mati sendiri".
const HTTP_PORT: u16 = 80;
const HTTPS_PORT: u16 = 443;

/// Handle proses caddy yang sedang berjalan. `None` = proxy mati.
#[derive(Default)]
pub struct ProxyProcess(pub Mutex<Option<Child>>);

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub struct ProxyStatus {
    pub running: bool,
    /// `true` saat berjalan dengan HTTPS, `false` saat fallback HTTP-only.
    pub https: bool,
    /// Jumlah site yang benar-benar diproyeksikan ke Caddyfile.
    pub routed_sites: usize,
    /// Sertifikat CA lokal Caddy sudah terpasang di trust store OS.
    pub cert_installed: bool,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub struct PortInUse {
    pub port: u16,
}

fn caddyfile_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("gagal resolve app_data_dir: {}", e))?
        .join("proxy");
    std::fs::create_dir_all(&dir).map_err(|e| format!("gagal membuat direktori proxy: {}", e))?;
    Ok(dir.join("Caddyfile"))
}

/// Tulis Caddyfile hasil proyeksi config, kembalikan path + jumlah rute.
fn write_caddyfile(app: &AppHandle, mode: TlsMode) -> Result<(PathBuf, usize), String> {
    let cfg = load_config(app)?;
    let content = render(&cfg.sites, mode);
    let routed = content.matches("reverse_proxy ").count();

    let path = caddyfile_path(app)?;
    std::fs::write(&path, &content).map_err(|e| format!("gagal menulis Caddyfile: {}", e))?;
    Ok((path, routed))
}

/// Mode TLS yang dipakai: HTTPS hanya kalau CA lokal sudah dipercaya OS.
/// Tanpa itu browser akan memblokir dengan peringatan, jadi lebih baik jalan
/// HTTP-only daripada terlihat rusak.
fn mode_for(app: &AppHandle) -> TlsMode {
    if cert_installed(app) {
        TlsMode::Https
    } else {
        TlsMode::HttpOnly
    }
}

/// Root CA lokal Caddy dianggap terpasang bila ada di trust store OS.
/// Windows: cek store `CurrentUser\Root` — di situlah Caddy memasangnya, tanpa
/// elevation (temuan F2 spike T0).
#[cfg(target_os = "windows")]
fn cert_installed(_app: &AppHandle) -> bool {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "if (Get-ChildItem Cert:\\CurrentUser\\Root | \
             Where-Object { $_.Subject -like '*Caddy Local Authority*' }) { exit 0 } else { exit 1 }",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// macOS/Linux: jalur pemasangan CA berbeda per-distro dan belum diuji empiris
/// (lihat §5 spike T0). Sampai itu terverifikasi, anggap belum terpasang →
/// proxy jalan HTTP-only, yang selalu aman.
#[cfg(not(target_os = "windows"))]
fn cert_installed(_app: &AppHandle) -> bool {
    false
}

fn is_running(app: &AppHandle) -> bool {
    let state = app.state::<ProxyProcess>();
    let mut guard = state.0.lock().unwrap();

    match guard.as_mut() {
        // `try_wait` Ok(Some(_)) = proses sudah keluar sendiri (mis. crash);
        // bersihkan handle supaya status tidak bohong.
        Some(child) => match child.try_wait() {
            Ok(Some(_)) => {
                *guard = None;
                false
            }
            Ok(None) => true,
            Err(_) => false,
        },
        None => false,
    }
}

fn emit_status(app: &AppHandle, status: &ProxyStatus) {
    let _ = app.emit("proxy-status-changed", status.clone());
}

fn status_of(app: &AppHandle) -> ProxyStatus {
    let running = is_running(app);
    // Sekali probe saja: cek trust store men-spawn proses, dan status ini
    // dipanggil berulang dari UI.
    let cert = cert_installed(app);
    let mode = if cert { TlsMode::Https } else { TlsMode::HttpOnly };
    let routed = load_config(app)
        .map(|cfg| render(&cfg.sites, mode).matches("reverse_proxy ").count())
        .unwrap_or(0);

    ProxyStatus {
        running,
        https: running && cert,
        routed_sites: routed,
        cert_installed: cert,
    }
}

#[tauri::command]
pub async fn proxy_status(app: AppHandle) -> Result<ProxyStatus, String> {
    Ok(status_of(&app))
}

/// Port yang dibutuhkan proxy tapi sedang dipakai proses lain (IIS, Skype,
/// proxy lain). Dipanggil frontend sebelum start supaya pesannya actionable.
#[tauri::command]
pub async fn proxy_check_ports(app: AppHandle) -> Result<Vec<PortInUse>, String> {
    // Kalau proxy kita sendiri yang memegang port, itu bukan konflik.
    if is_running(&app) {
        return Ok(Vec::new());
    }

    let mut busy = Vec::new();
    for port in [HTTP_PORT, HTTPS_PORT] {
        if probe_host_port(port) {
            busy.push(PortInUse { port });
        }
    }
    Ok(busy)
}

#[tauri::command]
pub async fn proxy_start(app: AppHandle) -> Result<ProxyStatus, String> {
    if is_running(&app) {
        return Ok(status_of(&app));
    }

    let bin = caddy_path(&app)?;
    if !bin.is_file() {
        return Err(
            "Binary Caddy belum terpasang. Pasang dulu lewat tombol di halaman Sites.".to_string(),
        );
    }

    let busy = proxy_check_ports(app.clone()).await?;
    if !busy.is_empty() {
        let ports: Vec<String> = busy.iter().map(|p| p.port.to_string()).collect();
        return Err(format!(
            "Port {} sedang dipakai aplikasi lain. Hentikan aplikasi itu dulu \
             (biasanya IIS, Skype, atau web server lain), lalu coba lagi.",
            ports.join(" dan ")
        ));
    }

    let mode = mode_for(&app);
    let (path, routed) = write_caddyfile(&app, mode)?;
    if routed == 0 {
        return Err(
            "Belum ada site yang bisa di-proxy. Aktifkan minimal satu site dan isi port tujuannya."
                .to_string(),
        );
    }

    validate(&bin, &path).await?;

    let child = caddy_command(&bin)
        .args(["run", "--config"])
        .arg(&path)
        .args(["--adapter", "caddyfile"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Gagal menjalankan Caddy: {}", e))?;

    {
        let state = app.state::<ProxyProcess>();
        *state.0.lock().unwrap() = Some(child);
    }

    let status = status_of(&app);
    emit_status(&app, &status);
    Ok(status)
}

/// Cek config lewat `caddy adapt` sebelum dipakai. Tanpa ini, config yang salah
/// baru ketahuan sebagai proses yang mati diam-diam beberapa saat setelah start.
async fn validate(bin: &PathBuf, config: &PathBuf) -> Result<(), String> {
    let output = caddy_command(bin)
        .arg("adapt")
        .arg("--config")
        .arg(config)
        .args(["--adapter", "caddyfile"])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| format!("Gagal memvalidasi konfigurasi Caddy: {}", e))?;

    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "Konfigurasi proxy tidak valid: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    ))
}

#[tauri::command]
pub async fn proxy_stop(app: AppHandle) -> Result<ProxyStatus, String> {
    stop_process(&app).await?;
    let status = status_of(&app);
    emit_status(&app, &status);
    Ok(status)
}

/// Hentikan caddy lewat admin API supaya port dilepas bersih; kalau itu gagal
/// (mis. admin API tak responsif), jatuh ke kill langsung.
pub async fn stop_process(app: &AppHandle) -> Result<(), String> {
    if let Ok(bin) = caddy_path(app) {
        if bin.is_file() {
            let _ = caddy_command(&bin)
                .args(["stop", "--address", ADMIN_ADDR])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .await;
        }
    }

    let mut child = {
        let state = app.state::<ProxyProcess>();
        let mut guard = state.0.lock().unwrap();
        guard.take()
    };

    if let Some(child) = child.as_mut() {
        if matches!(child.try_wait(), Ok(None)) {
            let _ = child.kill().await;
        }
    }
    Ok(())
}

/// Terapkan perubahan sites ke proxy yang sedang jalan tanpa mematikan proses
/// (hot-reload lewat admin API). Kalau proxy belum jalan, tak ada yang perlu
/// dilakukan — start berikutnya sudah membaca config terbaru.
#[tauri::command]
pub async fn proxy_reload(app: AppHandle) -> Result<ProxyStatus, String> {
    if !is_running(&app) {
        return Ok(status_of(&app));
    }

    let bin = caddy_path(&app)?;
    let (path, _) = write_caddyfile(&app, mode_for(&app))?;
    validate(&bin, &path).await?;

    let output = caddy_command(&bin)
        .arg("reload")
        .arg("--config")
        .arg(&path)
        .args(["--adapter", "caddyfile", "--address", ADMIN_ADDR])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| format!("Gagal memuat ulang konfigurasi proxy: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "Gagal memuat ulang konfigurasi proxy: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    let status = status_of(&app);
    emit_status(&app, &status);
    Ok(status)
}

/// Pasang root CA lokal Caddy ke trust store OS supaya `https://<domain>`
/// dipercaya browser. Di Windows ini memunculkan dialog konfirmasi sertifikat
/// (bukan UAC) — user bisa menolak, dan penolakan bukan error fatal: proxy
/// tetap bisa jalan HTTP-only.
#[tauri::command]
pub async fn proxy_install_cert(app: AppHandle) -> Result<ProxyStatus, String> {
    let bin = caddy_path(&app)?;
    if !bin.is_file() {
        return Err("Binary Caddy belum terpasang.".to_string());
    }

    let output = caddy_command(&bin)
        .arg("trust")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| format!("Gagal memasang sertifikat: {}", e))?;

    if !output.status.success() {
        return Err(
            "Sertifikat tidak jadi dipasang. Servel tetap bisa melayani domain lewat HTTP."
                .to_string(),
        );
    }

    // Trust berubah → mode TLS ikut berubah, jadi config harus dirender ulang.
    if is_running(&app) {
        return proxy_reload(app).await;
    }

    let status = status_of(&app);
    emit_status(&app, &status);
    Ok(status)
}
