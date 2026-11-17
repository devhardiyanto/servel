//! Quit Kova dengan rapi (v3.5.1): hentikan dev server, lalu service container
//! (bila "Stop services when Kova quits" menyala), baru keluar.
//!
//! Dipakai oleh menu tray "Quit Kova" dan oleh penutupan jendela terakhir saat
//! "Minimize to tray" mati. Update (`update_apply`) melewati langkah ini lewat
//! `skip()`: installer mengambil alih, dan service sengaja dibiarkan berjalan.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};

use crate::commands::config::ConfigState;
use crate::commands::util::emit_log_line;

/// Batas waktu menghentikan service. Lewat dari ini Kova tetap keluar, supaya
/// Quit tidak pernah menggantung karena engine yang tidak menjawab.
const SERVICES_TIMEOUT: Duration = Duration::from_secs(60);

static STARTED: AtomicBool = AtomicBool::new(false);
static DONE: AtomicBool = AtomicBool::new(false);

/// Keluar boleh langsung (langkah rapi sudah selesai atau sengaja dilewati).
pub fn is_done() -> bool {
    DONE.load(Ordering::SeqCst)
}

/// Keluar sedang berjalan (tooltip tray tidak boleh ditimpa polling).
pub fn in_progress() -> bool {
    STARTED.load(Ordering::SeqCst)
}

/// Lewati langkah rapi untuk keluar berikutnya (restart setelah update).
pub fn skip() {
    DONE.store(true, Ordering::SeqCst);
}

fn stop_services_enabled(app: &AppHandle) -> bool {
    app.try_state::<Mutex<ConfigState>>()
        .and_then(|s| s.lock().ok().map(|c| c.stop_services_on_quit))
        .unwrap_or(true)
}

fn set_tooltip(app: &AppHandle, text: &str) {
    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_tooltip(Some(text));
    }
}

/// Mulai keluar dengan rapi. Panggilan kedua diabaikan.
pub fn begin(app: &AppHandle) {
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let stop_services = stop_services_enabled(&app);
        let _ = app.emit("app-quitting", serde_json::json!({ "stopServices": stop_services }));
        set_tooltip(&app, if stop_services { "Kova — stopping services…" } else { "Kova — quitting…" });

        crate::sites::process::stop_all(&app).await;

        if stop_services {
            emit_log_line(&app, "KOVA", "quit: stopping services");
            let result = tokio::time::timeout(SERVICES_TIMEOUT, async {
                let defs = crate::commands::services::load_services_internal(&app).await?;
                crate::runtime::stop_all(&app, &defs).await
            })
            .await;
            match result {
                Ok(Ok(())) => emit_log_line(&app, "KOVA", "quit: services stopped"),
                Ok(Err(e)) => emit_log_line(&app, "KOVA", &format!("quit: could not stop services: {}", e)),
                Err(_) => emit_log_line(&app, "KOVA", "quit: services did not stop within 60 s; quitting anyway"),
            }
        }

        DONE.store(true, Ordering::SeqCst);
        app.exit(0);
    });
}
