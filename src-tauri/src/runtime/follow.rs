//! Output container service di tab Logs.
//!
//! Saat service berjalan, Kova mengikuti `logs -f` container-nya dan menulis
//! tiap baris dengan sumber id service (huruf besar), yaitu sumber yang sama
//! dengan tab service di Logs. Pengikut dihentikan saat service turun
//! (`retain_running` dari loop status).
//!
//! Bila engine tidak mendukung `logs -f` (proses langsung keluar dengan
//! error), Kova membaca `logs --tail` berkala dan hanya menulis baris baru.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};

use super::RuntimeKind;
use crate::sites::process::{line_level, strip_ansi};

/// Baris lama yang ditampilkan saat mulai mengikuti.
const TAIL: u32 = 50;
const POLL: Duration = Duration::from_secs(3);

/// Pengikut aktif per id service: nomor urut untuk membatalkan yang lama.
static ACTIVE: Mutex<Option<HashMap<String, u64>>> = Mutex::new(None);
static SEQ: Mutex<u64> = Mutex::new(0);

fn with_active<T>(f: impl FnOnce(&mut HashMap<String, u64>) -> T) -> T {
    let mut g = ACTIVE.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(HashMap::new))
}

fn current(id: &str, seq: u64) -> bool {
    with_active(|m| m.get(id) == Some(&seq))
}

fn emit(app: &AppHandle, source: &str, raw: &str) {
    let line = strip_ansi(raw);
    let line = line.trim_end();
    if line.trim().is_empty() {
        return;
    }
    let mut payload = serde_json::json!({ "line": line, "stream": "stdout", "source": source });
    if let Some(l) = line_level(line) {
        payload["level"] = serde_json::Value::String(l.to_string());
    }
    let _ = app.emit("cmd-output", payload);
}

/// Mulai mengikuti log `id` (bila belum).
pub fn start(app: &AppHandle, id: &str, container: &str) {
    let seq = {
        let mut s = SEQ.lock().unwrap_or_else(|e| e.into_inner());
        *s += 1;
        *s
    };
    let fresh = with_active(|m| {
        if m.contains_key(id) {
            false
        } else {
            m.insert(id.to_string(), seq);
            true
        }
    });
    if !fresh {
        return;
    }
    let (app, id, container) = (app.clone(), id.to_string(), container.to_string());
    tauri::async_runtime::spawn(async move {
        let source = id.to_uppercase();
        let mut cmd = match super::active() {
            RuntimeKind::Docker => super::docker::follow_command(&container, TAIL),
            RuntimeKind::Wslc => super::wslc::follow_command(&container, TAIL),
        };
        let started = tokio::time::Instant::now();
        let mut lines_seen = 0usize;
        if let Ok(mut child) = cmd.spawn() {
            let out = child.stdout.take();
            let err = child.stderr.take();
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
            if let Some(o) = out {
                let tx = tx.clone();
                tauri::async_runtime::spawn(async move {
                    let mut l = BufReader::new(o).lines();
                    while let Ok(Some(x)) = l.next_line().await {
                        let _ = tx.send(x);
                    }
                });
            }
            if let Some(e) = err {
                let tx = tx.clone();
                tauri::async_runtime::spawn(async move {
                    let mut l = BufReader::new(e).lines();
                    while let Ok(Some(x)) = l.next_line().await {
                        let _ = tx.send(x);
                    }
                });
            }
            drop(tx);
            loop {
                tokio::select! {
                    line = rx.recv() => match line {
                        Some(l) => {
                            lines_seen += 1;
                            emit(&app, &source, &l);
                        }
                        None => break,
                    },
                    _ = tokio::time::sleep(Duration::from_secs(1)) => {
                        if !current(&id, seq) {
                            let _ = child.kill().await;
                            return;
                        }
                    }
                }
            }
            let ok = child.wait().await.map(|s| s.success()).unwrap_or(false);
            // Keluar normal (container berhenti): lepas catatan supaya bisa diikuti lagi.
            if ok || lines_seen > 0 || started.elapsed() > Duration::from_secs(5) {
                with_active(|m| {
                    if m.get(&id) == Some(&seq) {
                        m.remove(&id);
                    }
                });
                return;
            }
        }
        poll(&app, &id, &container, &source, seq).await;
    });
}

/// Cadangan tanpa `-f`: ambil `--tail` berkala, tulis baris setelah baris
/// terakhir yang sudah ditulis.
async fn poll(app: &AppHandle, id: &str, container: &str, source: &str, seq: u64) {
    let mut last: Vec<String> = Vec::new();
    while current(id, seq) {
        let out = match super::active() {
            RuntimeKind::Docker => super::docker::logs_tail(container, 200).await,
            RuntimeKind::Wslc => super::wslc::logs_tail(container, 200).await,
        };
        if let Some(text) = out {
            let now: Vec<String> = text.lines().filter(|l| !l.trim().is_empty()).map(str::to_string).collect();
            for l in new_lines(&last, &now, TAIL as usize) {
                emit(app, source, l);
            }
            last = now;
        }
        tokio::time::sleep(POLL).await;
    }
}

/// Baris di `now` yang belum ada di `last`: cari akhir `last` di `now`.
/// Putaran pertama (`last` kosong) mengembalikan `first` baris terakhir.
pub fn new_lines<'a>(last: &[String], now: &'a [String], first: usize) -> &'a [String] {
    if last.is_empty() {
        return &now[now.len().saturating_sub(first)..];
    }
    // Cocokkan beberapa baris terakhir `last` supaya baris yang sama
    // (mis. "ready") tidak salah dijadikan patokan.
    for k in (1..=last.len().min(3)).rev() {
        let tail = &last[last.len() - k..];
        for end in (k..=now.len()).rev() {
            if &now[end - k..end] == tail {
                return &now[end..];
            }
        }
    }
    now
}

/// Hentikan pengikut service yang tidak lagi berjalan.
pub fn retain_running(running: &std::collections::HashSet<String>) {
    with_active(|m| m.retain(|id, _| running.contains(id)));
}


#[cfg(test)]
mod tests {
    use super::*;

    fn v(x: &[&str]) -> Vec<String> {
        x.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn new_lines_continues_after_last_seen() {
        let last = v(&["a", "b", "c"]);
        let now = v(&["b", "c", "d", "e"]);
        assert_eq!(new_lines(&last, &now, 50), v(&["d", "e"]).as_slice());
        assert!(new_lines(&now, &now, 50).is_empty());
        assert_eq!(new_lines(&[], &now, 2), v(&["d", "e"]).as_slice());
        // Log terpotong habis (container dibuat ulang): semuanya baru.
        assert_eq!(new_lines(&last, &v(&["x"]), 50), v(&["x"]).as_slice());
    }
}
