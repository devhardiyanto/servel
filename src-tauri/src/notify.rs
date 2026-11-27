//! Notifikasi OS saat sesuatu berhenti sendiri.
//!
//! Hanya dikirim bila setting `notifications` aktif dan jendela utama tidak
//! sedang dilihat (tersembunyi di tray atau tidak fokus): saat jendela
//! terbuka, status dan Logs sudah menunjukkannya.

use tauri::{AppHandle, Manager, Runtime};

fn window_in_view<R: Runtime>(app: &AppHandle<R>) -> bool {
    app.get_webview_window("main")
        .is_some_and(|w| w.is_visible().unwrap_or(false) && w.is_focused().unwrap_or(false))
}

/// Config terakhir yang dibaca atau ditulis Kova (state yang dikelola).
fn enabled<R: Runtime>(app: &AppHandle<R>) -> bool {
    app.try_state::<std::sync::Mutex<crate::commands::config::ConfigState>>()
        .and_then(|s| s.lock().ok().map(|c| c.notifications))
        .unwrap_or(true)
}

fn send<R: Runtime>(app: &AppHandle<R>, title: &str, body: &str) {
    if window_in_view(app) || !enabled(app) {
        return;
    }
    // Plugin belum terpasang (mis. unit test): lewati tanpa panic.
    if let Some(n) = app.try_state::<tauri_plugin_notification::Notification<R>>() {
        let _ = n.builder().title(title).body(body).show();
    }
}

fn exit_text(code: Option<i32>) -> String {
    match code {
        Some(c) => format!("exit {}", c),
        None => "no exit code".to_string(),
    }
}

/// Dev server project berhenti sendiri.
pub fn project_exited<R: Runtime>(app: &AppHandle<R>, project: &str, code: Option<i32>) {
    send(app, &format!("{} stopped", project), &format!("The dev server ended ({}). Details are in Logs.", exit_text(code)));
}

/// Proses tambahan tanpa restart otomatis berhenti sendiri.
pub fn process_exited<R: Runtime>(app: &AppHandle<R>, project: &str, name: &str, code: Option<i32>) {
    send(app, &format!("{}: {} stopped", project, name), &format!("The process ended ({}). Details are in Logs.", exit_text(code)));
}

/// Proses tambahan terlalu sering crash; Kova berhenti menjalankannya ulang.
pub fn process_gave_up<R: Runtime>(app: &AppHandle<R>, project: &str, name: &str) {
    send(
        app,
        &format!("{}: {} keeps crashing", project, name),
        "Kova stopped restarting it. Check Logs, then start it again from the project.",
    );
}

/// Container service berhenti tanpa diminta.
pub fn service_stopped<R: Runtime>(app: &AppHandle<R>, service: &str) {
    send(app, &format!("{} stopped", service), "The service container is no longer running. Details are in Logs.");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_text_names_the_code() {
        assert_eq!(exit_text(Some(1)), "exit 1");
        assert_eq!(exit_text(None), "no exit code");
    }
}
