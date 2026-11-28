//! Cadangan file yang belum disimpan (hot exit sederhana, spike 3).
//!
//! Halaman editor menulis isi tab yang berubah ke folder data Kova, bukan ke
//! repo, sekitar satu detik setelah mengetik berhenti. Saat jendela dimuat
//! ulang (renderer WebView2 mati, S2) atau dibuka lagi, tab itu kembali
//! dengan isinya. Cadangan dihapus saat file disimpan atau perubahannya dibuang.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::Manager;

use super::EditorRoots;

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Backup {
    pub path: String,
    pub text: String,
    pub eol: String,
    pub bom: bool,
    /// mtime file saat dibaca, untuk deteksi konflik saat disimpan nanti.
    pub mtime: Option<u64>,
}

/// Satu folder cadangan per jendela editor (label stabil per folder).
fn dir(window: &tauri::WebviewWindow) -> Result<PathBuf, String> {
    let base = window.app_handle().path().app_data_dir().map_err(|e| e.to_string())?;
    Ok(base.join("editor-backups").join(window.label()))
}

pub fn file_name(path: &str) -> String {
    use sha2::{Digest, Sha256};
    let h = Sha256::digest(path.to_lowercase().as_bytes());
    format!("{}.json", h.iter().take(12).map(|b| format!("{:02x}", b)).collect::<String>())
}

fn check(window: &tauri::WebviewWindow, roots: &EditorRoots, path: &str) -> Result<(), String> {
    let root = roots.0.lock().unwrap().get(window.label()).cloned().ok_or("this window has no folder")?;
    super::fs::resolve(&root, path).map(|_| ())
}

#[tauri::command]
pub fn editor_backup_put(window: tauri::WebviewWindow, roots: tauri::State<'_, EditorRoots>, backup: Backup) -> Result<(), String> {
    check(&window, &roots, &backup.path)?;
    let d = dir(&window)?;
    std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
    let data = serde_json::to_vec(&backup).map_err(|e| e.to_string())?;
    super::fs::write_atomic(&d.join(file_name(&backup.path)), &data)
}

#[tauri::command]
pub fn editor_backup_drop(window: tauri::WebviewWindow, path: String) -> Result<(), String> {
    let p = dir(&window)?.join(file_name(&path));
    match std::fs::remove_file(p) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
        _ => Ok(()),
    }
}

#[tauri::command]
pub fn editor_backup_list(window: tauri::WebviewWindow, roots: tauri::State<'_, EditorRoots>) -> Result<Vec<Backup>, String> {
    let d = dir(&window)?;
    let Ok(entries) = std::fs::read_dir(&d) else { return Ok(vec![]) };
    let mut out = Vec::new();
    for e in entries.flatten() {
        let Ok(bytes) = std::fs::read(e.path()) else { continue };
        let Ok(b) = serde_json::from_slice::<Backup>(&bytes) else { continue };
        if check(&window, &roots, &b.path).is_ok() {
            out.push(b);
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_is_stable_and_case_insensitive() {
        assert_eq!(file_name("C:\\dev\\A.php"), file_name("c:\\dev\\a.php"));
        assert_ne!(file_name("C:\\dev\\a.php"), file_name("C:\\dev\\b.php"));
        assert!(file_name("x").ends_with(".json"));
    }

    #[test]
    fn round_trips_as_json() {
        let b = Backup { path: "a".into(), text: "x\ny".into(), eol: "\r\n".into(), bom: false, mtime: Some(5) };
        let v: Backup = serde_json::from_slice(&serde_json::to_vec(&b).unwrap()).unwrap();
        assert_eq!(v, b);
    }
}
