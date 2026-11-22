//! Popup tray.
//!
//! Klik kiri ikon tray membuka jendela kecil tanpa bingkai di dekat ikon,
//! berisi project dan service dengan status berwarna. Klik kanan tetap
//! membuka menu native (`tray.rs`).
//!
//! - Jendela dibuat sekali saat pertama dibuka, lalu hanya disembunyikan dan
//!   ditampilkan, supaya muncul tanpa menunggu WebView2 dimuat ulang.
//! - Posisi dihitung dari kotak ikon dan work area monitornya (`place`), jadi
//!   benar untuk taskbar di bawah, atas, kiri, atau kanan, di monitor mana pun.
//! - Kehilangan fokus = disembunyikan. Klik ikon yang membuat popup kehilangan
//!   fokus tidak langsung membukanya lagi (`REOPEN_GUARD`).

use std::sync::Mutex;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder};

pub const LABEL: &str = "tray";
/// Lebar popup dalam piksel logis.
const WIDTH: f64 = 320.0;
/// Tinggi awal sebelum halaman melaporkan tingginya.
const INITIAL_HEIGHT: f64 = 240.0;
/// Tinggi maksimal popup. Halaman juga membatasi dirinya ke tinggi
/// work area (`maxHeight` di TrayPopup.vue) dan hanya daftar project yang
/// digulir, jadi nilai ini cukup sebagai pengaman.
const MAX_HEIGHT: f64 = 760.0;
/// Jarak dari tepi work area, piksel logis.
const MARGIN: f64 = 8.0;
/// Klik ikon dalam rentang ini setelah popup hilang karena blur = tutup saja.
const REOPEN_GUARD: Duration = Duration::from_millis(300);

/// Kotak dalam piksel fisik.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Box2 {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

struct State {
    /// Kotak ikon tray pada klik terakhir.
    anchor: Option<Box2>,
    /// Kapan popup terakhir disembunyikan karena kehilangan fokus.
    blurred_at: Option<Instant>,
    /// Popup menunggu halaman melaporkan tingginya sebelum ditampilkan.
    pending_show: bool,
}

static STATE: Mutex<State> = Mutex::new(State { anchor: None, blurred_at: None, pending_show: false });

fn state() -> std::sync::MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Posisi kiri-atas popup berukuran `size` di dekat `icon`, di dalam `work`.
/// Sisi taskbar ditebak dari letak ikon terhadap work area: ikon di luar work
/// area di bawah = taskbar bawah, dan seterusnya. Ikon di dalam work area
/// (mis. taskbar disembunyikan otomatis) diperlakukan seperti taskbar bawah.
pub fn place(icon: Box2, work: Box2, size: (f64, f64), margin: f64) -> (f64, f64) {
    let (w, h) = size;
    let cx = icon.x + icon.w / 2.0;
    let cy = icon.y + icon.h / 2.0;
    let clamp_x = |x: f64| x.max(work.x + margin).min(work.x + work.w - w - margin);
    let clamp_y = |y: f64| y.max(work.y + margin).min(work.y + work.h - h - margin);

    if cy < work.y {
        // Taskbar atas (atau menu bar macOS).
        (clamp_x(cx - w / 2.0), work.y + margin)
    } else if cx < work.x {
        // Taskbar kiri.
        (work.x + margin, clamp_y(cy - h / 2.0))
    } else if cx > work.x + work.w {
        // Taskbar kanan.
        (work.x + work.w - w - margin, clamp_y(cy - h / 2.0))
    } else {
        // Taskbar bawah.
        (clamp_x(cx - w / 2.0), work.y + work.h - h - margin)
    }
}

fn to_box(rect: &tauri::Rect, scale: f64) -> Box2 {
    let p: PhysicalPosition<f64> = rect.position.to_physical(scale);
    let s: PhysicalSize<f64> = rect.size.to_physical(scale);
    Box2 { x: p.x, y: p.y, w: s.width, h: s.height }
}

/// Pindahkan popup ke dekat ikon terakhir, dengan ukuran jendela saat ini.
fn position(app: &AppHandle, win: &tauri::WebviewWindow) {
    let Some(icon) = state().anchor else { return };
    let cx = icon.x + icon.w / 2.0;
    let cy = icon.y + icon.h / 2.0;
    let Ok(Some(monitor)) = app.monitor_from_point(cx, cy) else { return };
    let wa = monitor.work_area();
    let work = Box2 {
        x: wa.position.x as f64,
        y: wa.position.y as f64,
        w: wa.size.width as f64,
        h: wa.size.height as f64,
    };
    let scale = monitor.scale_factor();
    let Ok(size) = win.outer_size() else { return };
    let (x, y) = place(icon, work, (size.width as f64, size.height as f64), MARGIN * scale);
    let _ = win.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
}

fn reveal(app: &AppHandle, win: &tauri::WebviewWindow) {
    position(app, win);
    let _ = win.show();
    let _ = win.set_focus();
    let _ = app.emit_to(LABEL, "tray-popup-shown", ());
}

/// Klik kiri ikon tray: buka popup, atau tutup bila sedang terbuka.
pub fn toggle(app: &AppHandle, rect: &tauri::Rect) {
    if let Some(win) = app.get_webview_window(LABEL) {
        if win.is_visible().unwrap_or(false) {
            let _ = win.hide();
            return;
        }
    }
    {
        let mut st = state();
        // Klik ini yang membuat popup kehilangan fokus dan tersembunyi.
        if st.blurred_at.is_some_and(|t| t.elapsed() < REOPEN_GUARD) {
            st.blurred_at = None;
            return;
        }
        let scale = app
            .monitor_from_point(0.0, 0.0)
            .ok()
            .flatten()
            .map(|m| m.scale_factor())
            .unwrap_or(1.0);
        st.anchor = Some(to_box(rect, scale));
    }

    if let Some(win) = app.get_webview_window(LABEL) {
        reveal(app, &win);
        return;
    }

    // Pertama kali: buat jendela di luar handler event (membuat webview di
    // dalam handler bisa macet di Windows). Ditampilkan setelah halaman
    // melaporkan tingginya (`tray_popup_resize`).
    state().pending_show = true;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let built = WebviewWindowBuilder::new(&app, LABEL, WebviewUrl::App("index.html?view=tray".into()))
            .title("Kova")
            .inner_size(WIDTH, INITIAL_HEIGHT)
            .resizable(false)
            .maximizable(false)
            .minimizable(false)
            .decorations(false)
            .shadow(true)
            .always_on_top(true)
            .skip_taskbar(true)
            .visible(false)
            .focused(true)
            .background_color(tauri::window::Color(0x1c, 0x1f, 0x25, 0xff))
            .build();
        if let Err(e) = built {
            state().pending_show = false;
            crate::commands::util::emit_log_line(&app, "TRAY", &format!("could not open the tray popup: {}", e));
        }
    });
}

/// Kejadian jendela popup: blur = sembunyikan; tutup (Alt+F4) = sembunyikan.
pub fn on_window_event(window: &tauri::Window, event: &tauri::WindowEvent) {
    match event {
        tauri::WindowEvent::Focused(false) => {
            if window.is_visible().unwrap_or(false) {
                let _ = window.hide();
                state().blurred_at = Some(Instant::now());
            }
        }
        tauri::WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            let _ = window.hide();
        }
        _ => {}
    }
}

/// Halaman popup melaporkan tinggi isinya (piksel logis).
#[tauri::command]
pub fn tray_popup_resize(app: AppHandle, height: f64) {
    let Some(win) = app.get_webview_window(LABEL) else { return };
    let h = height.clamp(80.0, MAX_HEIGHT);
    let _ = win.set_size(tauri::LogicalSize::new(WIDTH, h));
    let show = std::mem::take(&mut state().pending_show);
    if show {
        reveal(&app, &win);
    } else if win.is_visible().unwrap_or(false) {
        position(&app, &win);
    }
}

#[tauri::command]
pub fn tray_popup_hide(app: AppHandle) {
    if let Some(win) = app.get_webview_window(LABEL) {
        let _ = win.hide();
    }
}

/// "Open Kova" dari popup.
#[tauri::command]
pub fn app_open_main(app: AppHandle) {
    tray_popup_hide(app.clone());
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// "Quit Kova" dari popup: jalur yang sama dengan menu tray.
#[tauri::command]
pub fn app_quit(app: AppHandle) {
    tray_popup_hide(app.clone());
    if let Some(w) = app.get_webview_window("main") {
        // Tampilkan layar "Stopping services…" selama menunggu.
        let _ = w.show();
    }
    crate::shutdown::begin(&app);
}

#[cfg(test)]
mod tests {
    use super::*;

    // Layar 1920×1080, taskbar 48 px.
    const SIZE: (f64, f64) = (320.0, 400.0);

    #[test]
    fn bottom_taskbar_opens_above_the_icon() {
        let work = Box2 { x: 0.0, y: 0.0, w: 1920.0, h: 1032.0 };
        let icon = Box2 { x: 1700.0, y: 1040.0, w: 24.0, h: 32.0 };
        assert_eq!(place(icon, work, SIZE, 8.0), (1552.0, 624.0));
    }

    #[test]
    fn stays_inside_the_work_area_near_the_corner() {
        let work = Box2 { x: 0.0, y: 0.0, w: 1920.0, h: 1032.0 };
        let icon = Box2 { x: 1890.0, y: 1040.0, w: 24.0, h: 32.0 };
        let (x, _) = place(icon, work, SIZE, 8.0);
        assert_eq!(x, 1920.0 - 320.0 - 8.0);
    }

    #[test]
    fn top_left_and_right_taskbars() {
        let top = Box2 { x: 0.0, y: 48.0, w: 1920.0, h: 1032.0 };
        assert_eq!(place(Box2 { x: 1700.0, y: 8.0, w: 24.0, h: 32.0 }, top, SIZE, 8.0), (1552.0, 56.0));

        let left = Box2 { x: 48.0, y: 0.0, w: 1872.0, h: 1080.0 };
        assert_eq!(place(Box2 { x: 8.0, y: 900.0, w: 32.0, h: 24.0 }, left, SIZE, 8.0), (56.0, 672.0));

        let right = Box2 { x: 0.0, y: 0.0, w: 1872.0, h: 1080.0 };
        assert_eq!(place(Box2 { x: 1880.0, y: 900.0, w: 32.0, h: 24.0 }, right, SIZE, 8.0), (1544.0, 672.0));
    }

    #[test]
    fn second_monitor_with_negative_coordinates() {
        // Monitor kiri di x = -1920, taskbar bawah.
        let work = Box2 { x: -1920.0, y: 0.0, w: 1920.0, h: 1032.0 };
        let icon = Box2 { x: -300.0, y: 1040.0, w: 24.0, h: 32.0 };
        assert_eq!(place(icon, work, SIZE, 8.0), (-448.0, 624.0));
    }

    #[test]
    fn auto_hidden_taskbar_counts_as_bottom() {
        let work = Box2 { x: 0.0, y: 0.0, w: 1920.0, h: 1080.0 };
        let icon = Box2 { x: 1700.0, y: 1050.0, w: 24.0, h: 30.0 };
        assert_eq!(place(icon, work, SIZE, 8.0).1, 1080.0 - 400.0 - 8.0);
    }
}
