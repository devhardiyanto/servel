//! Update dari dalam aplikasi lewat `tauri-plugin-updater`.
//!
//! Sumber: `latest.json` di GitHub Release terbaru (endpoint di
//! `tauri.conf.json`). File update diverifikasi dengan public key yang
//! tertanam di aplikasi sebelum dipasang.
//!
//! Frontend hanya memakai `update_check` (tidak ada cadangan lewat GitHub
//! API). Bila gagal (build dev, jaringan, release tanpa `latest.json`),
//! errornya ditampilkan di Settings → About.

use serde::Serialize;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};
use tauri_plugin_updater::UpdaterExt;

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub current_version: String,
    pub notes: Option<String>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProgress {
    pub downloaded: u64,
    /// `None` bila server tidak mengirim Content-Length.
    pub total: Option<u64>,
    /// 0–100; `None` bila `total` tidak diketahui.
    pub percent: Option<u8>,
}

pub fn progress(downloaded: u64, total: Option<u64>) -> UpdateProgress {
    let percent = total
        .filter(|t| *t > 0)
        .map(|t| ((downloaded.min(t) as u128 * 100) / t as u128) as u8);
    UpdateProgress { downloaded, total, percent }
}

/// `Some(info)` bila ada versi lebih baru yang bisa dipasang dari sini.
#[tauri::command]
pub async fn update_check(app: AppHandle) -> Result<Option<UpdateInfo>, String> {
    let updater = app.updater().map_err(|e| format!("updater not available: {}", e))?;
    let update = updater.check().await.map_err(|e| format!("could not check for updates: {}", e))?;
    Ok(update.map(|u| UpdateInfo {
        version: u.version.clone(),
        current_version: u.current_version.clone(),
        notes: u.body.clone(),
    }))
}

/// Update yang sudah diunduh dan lolos verifikasi, menunggu "Restart now".
static READY: Mutex<Option<(tauri_plugin_updater::Update, Vec<u8>)>> = Mutex::new(None);

/// Unduh dan verifikasi update, lalu simpan sampai pengguna memilih restart
/// (banner "Ready to restart" dengan Later / Restart now). Progress
/// dikirim lewat event `update-progress`. Mengembalikan versi yang siap.
#[tauri::command]
pub async fn update_download(app: AppHandle) -> Result<String, String> {
    if let Some((u, _)) = READY.lock().map_err(|e| e.to_string())?.as_ref() {
        return Ok(u.version.clone());
    }
    let updater = app.updater().map_err(|e| format!("updater not available: {}", e))?;
    let update = updater
        .check()
        .await
        .map_err(|e| format!("could not check for updates: {}", e))?
        .ok_or_else(|| "no update available".to_string())?;

    let mut downloaded: u64 = 0;
    let emitter = app.clone();
    // `download` memverifikasi tanda tangan; berkas yang tidak cocok ditolak di sini.
    let bytes = update
        .download(
            move |chunk, total| {
                downloaded += chunk as u64;
                let _ = emitter.emit("update-progress", progress(downloaded, total));
            },
            || {},
        )
        .await
        .map_err(|e| format!("could not download the update: {}", e))?;
    let version = update.version.clone();
    *READY.lock().map_err(|e| e.to_string())? = Some((update, bytes));
    Ok(version)
}

/// Pasang update yang sudah diunduh, lalu restart. Di Windows proses keluar
/// saat installer berjalan, jadi command ini tidak selalu sempat mengembalikan `Ok`.
#[tauri::command]
pub async fn update_apply(app: AppHandle) -> Result<(), String> {
    let (update, bytes) = READY
        .lock()
        .map_err(|e| e.to_string())?
        .take()
        .ok_or_else(|| "the update is not downloaded yet".to_string())?;

    // Installer Windows menutup Kova secara paksa, jadi hook "stop saat
    // keluar" tidak sempat berjalan. Hentikan dev server dan (sesuai setelan
    // "Stop services when Kova quits") service di sini, supaya tidak ada proses
    // atau container yang tertinggal memegang port saat Kova dibuka lagi.
    crate::sites::process::stop_all(&app).await;
    if crate::shutdown::stop_services_enabled(&app) {
        crate::shutdown::stop_services_now(&app, "update").await;
    }

    update
        .install(bytes)
        .map_err(|e| format!("could not install the update: {}", e))?;

    // Instance baru tidak boleh menyerahkan diri ke proses ini yang sedang keluar.
    crate::control::forget();
    // Langkah rapi sudah dijalankan di atas.
    crate::shutdown::skip();
    app.restart();
}

/// "Later" = pasang saat Kova ditutup.
static INSTALL_ON_QUIT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[tauri::command]
pub fn update_install_on_quit(enabled: bool) -> Result<bool, String> {
    if enabled && READY.lock().map_err(|e| e.to_string())?.is_none() {
        return Err("the update is not downloaded yet".to_string());
    }
    INSTALL_ON_QUIT.store(enabled, std::sync::atomic::Ordering::SeqCst);
    Ok(enabled)
}

/// Dipanggil `shutdown::begin` setelah dev server dan service dihentikan.
/// Di Windows installer menutup proses ini sendiri; di OS lain berkas
/// aplikasi diganti lalu Kova keluar seperti biasa.
pub fn install_pending_on_quit(app: &AppHandle) {
    if !INSTALL_ON_QUIT.swap(false, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    let Some((update, bytes)) = READY.lock().ok().and_then(|mut r| r.take()) else {
        return;
    };
    crate::control::forget();
    if let Err(e) = update.install(bytes) {
        crate::commands::util::emit_log_line(app, "KOVA", &format!("quit: could not install the update: {}", e));
    }
}

/// Unduh lalu langsung pasang, tanpa menunggu "Restart now".
#[tauri::command]
pub async fn update_install(app: AppHandle) -> Result<(), String> {
    update_download(app.clone()).await?;
    update_apply(app).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_percent() {
        assert_eq!(progress(0, Some(200)).percent, Some(0));
        assert_eq!(progress(50, Some(200)).percent, Some(25));
        assert_eq!(progress(200, Some(200)).percent, Some(100));
        // Lebih dari total (Content-Length meleset) tidak melewati 100.
        assert_eq!(progress(300, Some(200)).percent, Some(100));
        assert_eq!(progress(10, None).percent, None);
        assert_eq!(progress(10, Some(0)).percent, None);
    }
}
