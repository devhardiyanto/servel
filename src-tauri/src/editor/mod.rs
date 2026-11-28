//! Kova Editor (spike): jendela editor terpisah per folder.
//!
//! Satu folder = satu jendela (label `editor-<hash>`). Membuka folder yang
//! sama memfokuskan jendela yang ada. Perintah file hanya boleh menyentuh
//! path di dalam folder milik jendela pemanggil (lihat `fs::resolve`).
//!
//! Status spike: belum dirilis. Dibuka hanya bila "Kova Editor (spike)"
//! dinyalakan di Settings › About.

pub mod fs;
pub mod lsp;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Manager, Runtime, WebviewUrl, WebviewWindowBuilder};

pub const LABEL_PREFIX: &str = "editor-";

/// Folder yang dibuka tiap jendela editor (label → root kanonik).
#[derive(Default)]
pub struct EditorRoots(pub Mutex<HashMap<String, PathBuf>>);

pub fn is_editor(label: &str) -> bool {
    label.starts_with(LABEL_PREFIX)
}

/// Label stabil untuk satu folder: membuka folder yang sama memakai jendela yang sama.
fn label_for(root: &std::path::Path) -> String {
    use sha2::{Digest, Sha256};
    let key = root.to_string_lossy().to_lowercase();
    let h = Sha256::digest(key.as_bytes());
    format!("{}{}", LABEL_PREFIX, h.iter().take(6).map(|b| format!("{:02x}", b)).collect::<String>())
}

fn encode_query(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

/// Buka folder di Kova Editor, atau fokuskan jendela yang sudah membukanya.
/// `async` karena membuat jendela dari command sinkron bisa macet di Windows.
#[tauri::command]
pub async fn editor_open(app: AppHandle, folder: String) -> Result<String, String> {
    let root = std::fs::canonicalize(&folder).map_err(|e| format!("{}: {}", folder, e))?;
    if !root.is_dir() {
        return Err(format!("{} is not a folder", folder));
    }
    let label = label_for(&root);
    if let Some(w) = app.get_webview_window(&label) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        return Ok(label);
    }
    let display = fs::display_path(&root);
    let name = root.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| display.clone());
    app.state::<EditorRoots>().0.lock().unwrap().insert(label.clone(), root.clone());

    let url = format!("index.html?view=editor&root={}", encode_query(&display));
    let built = WebviewWindowBuilder::new(&app, &label, WebviewUrl::App(url.into()))
        .title(format!("{} · Kova Editor", name))
        .inner_size(1200.0, 780.0)
        .min_inner_size(640.0, 400.0)
        .background_color(tauri::window::Color(0x15, 0x17, 0x1b, 0xff))
        .build();
    let win = match built {
        Ok(w) => w,
        Err(e) => {
            app.state::<EditorRoots>().0.lock().unwrap().remove(&label);
            return Err(format!("could not open the editor window: {}", e));
        }
    };
    disable_browser_keys(&win);
    Ok(label)
}

/// Folder milik jendela ini (dipakai halaman editor saat dimuat).
#[tauri::command]
pub fn editor_root(window: tauri::WebviewWindow, roots: tauri::State<'_, EditorRoots>) -> Result<String, String> {
    roots
        .0
        .lock()
        .unwrap()
        .get(window.label())
        .map(|p| fs::display_path(p))
        .ok_or_else(|| "this window has no folder".to_string())
}

/// Bawa jendela utama Kova ke depan dari editor.
#[tauri::command]
pub fn editor_show_main(app: AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Jendela editor ditutup biasa (bukan disembunyikan seperti jendela utama).
pub fn on_window_event(window: &tauri::Window, event: &tauri::WindowEvent) {
    if let tauri::WindowEvent::Destroyed = event {
        if let Some(roots) = window.app_handle().try_state::<EditorRoots>() {
            roots.0.lock().unwrap().remove(window.label());
        }
        lsp::stop_window(window.app_handle(), window.label());
    }
}

/// Matikan pintasan browser WebView2 (cetak, cari di halaman, muat ulang,
/// devtools, riwayat) di level webview, supaya Ctrl F, Ctrl G, F5, F12, Alt ←
/// sampai ke editor tanpa perlu `preventDefault` dari JS. Spike S1 menguji ini.
#[cfg(windows)]
fn disable_browser_keys<R: Runtime>(win: &tauri::WebviewWindow<R>) {
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Settings3;
    use windows_core::Interface;
    let _ = win.with_webview(|wv| unsafe {
        let Ok(core) = wv.controller().CoreWebView2() else { return };
        let Ok(settings) = core.Settings() else { return };
        if let Ok(s3) = settings.cast::<ICoreWebView2Settings3>() {
            let _ = s3.SetAreBrowserAcceleratorKeysEnabled(false);
        }
    });
}

#[cfg(not(windows))]
fn disable_browser_keys<R: Runtime>(_win: &tauri::WebviewWindow<R>) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_folder_same_label() {
        let a = label_for(std::path::Path::new("C:\\dev\\Shop-API"));
        let b = label_for(std::path::Path::new("c:\\dev\\shop-api"));
        assert_eq!(a, b);
        assert!(is_editor(&a));
        assert_ne!(a, label_for(std::path::Path::new("C:\\dev\\other")));
    }

    #[test]
    fn query_is_encoded() {
        assert_eq!(encode_query("C:\\dev\\a b"), "C%3A%5Cdev%5Ca%20b");
    }
}
