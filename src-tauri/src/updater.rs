//! Update dari dalam aplikasi (v2.1) lewat `tauri-plugin-updater`.
//!
//! Sumber: `latest.json` di GitHub Release terbaru (endpoint di
//! `tauri.conf.json`). File update diverifikasi dengan public key yang
//! tertanam di aplikasi sebelum dipasang.
//!
//! Frontend tetap memakai GitHub API sebagai cadangan: bila `update_check`
//! gagal (build dev, jaringan, release tanpa `latest.json`), banner kembali
//! menawarkan halaman rilis.

use serde::Serialize;
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

/// Unduh, verifikasi tanda tangan, hentikan dev server, pasang, lalu restart. Progress dikirim
/// lewat event `update-progress`. Di Windows proses keluar saat installer
/// berjalan, jadi command ini tidak selalu sempat mengembalikan `Ok`.
#[tauri::command]
pub async fn update_install(app: AppHandle) -> Result<(), String> {
    let updater = app.updater().map_err(|e| format!("updater not available: {}", e))?;
    let update = updater
        .check()
        .await
        .map_err(|e| format!("could not check for updates: {}", e))?
        .ok_or_else(|| "no update available".to_string())?;

    let mut downloaded: u64 = 0;
    let emitter = app.clone();
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

    // Installer Windows menutup Servel secara paksa, jadi hook "stop saat
    // keluar" tidak sempat berjalan. Hentikan dev server di sini; kalau tidak,
    // prosesnya tertinggal memegang port dan Start setelah update gagal.
    crate::sites::process::stop_all(&app).await;

    update
        .install(bytes)
        .map_err(|e| format!("could not install the update: {}", e))?;

    app.restart();
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
