//! Terminal di Kova Editor (spike 3): PowerShell lewat ConPTY di Windows,
//! shell login di Linux/macOS. Byte dari pty dikirim mentah ke xterm.js
//! (UTF-8 yang terpotong antar potongan disusun ulang di sisi xterm).

use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::Manager;

use super::EditorRoots;

struct Term {
    window: String,
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
}

#[derive(Default)]
pub struct Terminals {
    next: AtomicU64,
    map: Mutex<HashMap<u64, Term>>,
}

/// Shell default: pwsh bila ada, lalu Windows PowerShell; di Unix `$SHELL`.
pub fn default_shell() -> (String, Vec<String>) {
    if cfg!(windows) {
        let exe = super::lsp::which("pwsh").map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|| "powershell.exe".to_string());
        (exe, vec!["-NoLogo".to_string()])
    } else {
        (std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string()), vec!["-l".to_string()])
    }
}

fn size(cols: u16, rows: u16) -> PtySize {
    PtySize { rows: rows.max(2), cols: cols.max(10), pixel_width: 0, pixel_height: 0 }
}

/// Buka terminal di folder jendela ini. Byte keluaran dikirim ke `on_data`
/// (ArrayBuffer di halaman); saat shell keluar dikirim `{"exit": kode}`.
#[tauri::command]
pub fn term_open(
    window: tauri::WebviewWindow,
    roots: tauri::State<'_, EditorRoots>,
    terms: tauri::State<'_, Terminals>,
    cols: u16,
    rows: u16,
    on_data: Channel<InvokeResponseBody>,
) -> Result<u64, String> {
    let root = roots.0.lock().unwrap().get(window.label()).cloned().ok_or("this window has no folder")?;
    let pair = native_pty_system().openpty(size(cols, rows)).map_err(|e| e.to_string())?;
    let (shell, args) = default_shell();
    let mut cmd = CommandBuilder::new(&shell);
    cmd.args(&args);
    cmd.cwd(&root);
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    let child = pair.slave.spawn_command(cmd).map_err(|e| format!("{}: {}", shell, e))?;
    drop(pair.slave);
    #[cfg(windows)]
    if let Some(pid) = child.process_id() {
        // Ikut mati bila Kova dimatikan paksa.
        let _ = crate::sites::process::job::assign(pid);
    }
    let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
    let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
    let id = terms.next.fetch_add(1, Ordering::Relaxed) + 1;
    terms.map.lock().unwrap().insert(id, Term { window: window.label().to_string(), master: pair.master, writer, child });

    let app = window.app_handle().clone();
    std::thread::spawn(move || {
        let mut buf = [0u8; 16 * 1024];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if on_data.send(InvokeResponseBody::Raw(buf[..n].to_vec())).is_err() {
                        break;
                    }
                }
            }
        }
        let code = app
            .try_state::<Terminals>()
            .and_then(|t| t.map.lock().unwrap().remove(&id))
            .and_then(|mut t| t.child.try_wait().ok().flatten())
            .map(|s| s.exit_code());
        let _ = on_data.send(InvokeResponseBody::Json(serde_json::json!({ "exit": code }).to_string()));
    });
    Ok(id)
}

#[tauri::command]
pub fn term_write(window: tauri::WebviewWindow, terms: tauri::State<'_, Terminals>, id: u64, data: String) -> Result<(), String> {
    let mut map = terms.map.lock().unwrap();
    let t = map.get_mut(&id).filter(|t| t.window == window.label()).ok_or("terminal is closed")?;
    t.writer.write_all(data.as_bytes()).and_then(|_| t.writer.flush()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn term_resize(window: tauri::WebviewWindow, terms: tauri::State<'_, Terminals>, id: u64, cols: u16, rows: u16) -> Result<(), String> {
    let map = terms.map.lock().unwrap();
    let t = map.get(&id).filter(|t| t.window == window.label()).ok_or("terminal is closed")?;
    t.master.resize(size(cols, rows)).map_err(|e| e.to_string())
}

fn kill(mut t: Term) {
    let pid = t.child.process_id();
    let _ = t.child.kill();
    drop(t);
    if let Some(pid) = pid {
        tauri::async_runtime::spawn(crate::sites::process::kill_tree(pid));
    }
}

#[tauri::command]
pub fn term_close(window: tauri::WebviewWindow, terms: tauri::State<'_, Terminals>, id: u64) {
    let t = {
        let mut map = terms.map.lock().unwrap();
        match map.get(&id) {
            Some(t) if t.window == window.label() => map.remove(&id),
            _ => None,
        }
    };
    if let Some(t) = t {
        kill(t);
    }
}

/// Tutup semua terminal milik jendela yang ditutup.
pub fn stop_window(app: &tauri::AppHandle, label: &str) {
    let Some(terms) = app.try_state::<Terminals>() else { return };
    let gone: Vec<Term> = {
        let mut map = terms.map.lock().unwrap();
        let ids: Vec<u64> = map.iter().filter(|(_, t)| t.window == label).map(|(k, _)| *k).collect();
        ids.into_iter().filter_map(|k| map.remove(&k)).collect()
    };
    for t in gone {
        kill(t);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_has_a_floor() {
        let s = size(0, 0);
        assert_eq!((s.cols, s.rows), (10, 2));
    }

    /// Shell sungguhan lewat pty: tulis perintah, baca keluarannya.
    #[test]
    fn pty_runs_a_command() {
        if cfg!(windows) {
            return;
        }
        let pair = native_pty_system().openpty(size(80, 24)).unwrap();
        let mut cmd = CommandBuilder::new("/bin/sh");
        cmd.env("PS1", "$ ");
        let mut child = pair.slave.spawn_command(cmd).unwrap();
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader().unwrap();
        let mut writer = pair.master.take_writer().unwrap();
        writer.write_all(b"echo kova-$((40+2))\nexit\n").unwrap();
        let mut out = Vec::new();
        let _ = reader.read_to_end(&mut out);
        let _ = child.wait();
        assert!(String::from_utf8_lossy(&out).contains("kova-42"));
    }
}
