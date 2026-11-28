//! File untuk Kova Editor: daftar folder, index Quick Open, baca, simpan.
//!
//! Semua path diperiksa terhadap folder milik jendela pemanggil. Simpan
//! ditulis ke file sementara di folder yang sama lalu diganti (atomik), dan
//! ditolak bila file berubah di disk sejak dibaca (konflik, lihat F7).

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use super::EditorRoots;

/// Lebih besar dari ini: dibuka tanpa highlight dan language server.
pub const LARGE: u64 = 5 * 1024 * 1024;
/// Lebih besar dari ini: tidak dibuka di editor (spike).
pub const MAX: u64 = 50 * 1024 * 1024;
/// Batas jumlah file di index Quick Open.
const INDEX_MAX: usize = 100_000;

/// Path untuk ditampilkan: tanpa awalan `\\?\` dari `canonicalize` di Windows.
pub fn display_path(p: &Path) -> String {
    let s = p.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{}", rest)
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        rest.to_string()
    } else {
        s.to_string()
    }
}

fn root_of(window: &tauri::WebviewWindow, roots: &EditorRoots) -> Result<PathBuf, String> {
    roots.0.lock().unwrap().get(window.label()).cloned().ok_or_else(|| "this window has no folder".to_string())
}

/// Path kanonik di dalam `root`, atau error. File yang belum ada diperiksa
/// lewat folder induknya.
pub fn resolve(root: &Path, path: &str) -> Result<PathBuf, String> {
    let p = Path::new(path);
    let full = if p.is_absolute() { p.to_path_buf() } else { root.join(p) };
    let canon = match std::fs::canonicalize(&full) {
        Ok(c) => c,
        Err(_) => {
            let parent = full.parent().ok_or("invalid path")?;
            let name = full.file_name().ok_or("invalid path")?;
            std::fs::canonicalize(parent).map_err(|e| format!("{}: {}", display_path(parent), e))?.join(name)
        }
    };
    if canon.starts_with(root) {
        Ok(canon)
    } else {
        Err("path is outside this folder".to_string())
    }
}

fn mtime_ms(meta: &std::fs::Metadata) -> u64 {
    meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64).unwrap_or(0)
}

#[derive(Serialize)]
pub struct Entry {
    name: String,
    path: String,
    dir: bool,
}

/// Isi satu folder: folder dulu, lalu file, urut nama tanpa membedakan huruf.
/// `.git` disembunyikan seperti default VS Code.
pub fn list_dir(dir: &Path) -> Result<Vec<Entry>, String> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
        let Ok(e) = e else { continue };
        let name = e.file_name().to_string_lossy().to_string();
        if name == ".git" {
            continue;
        }
        let dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
        out.push(Entry { name, path: display_path(&e.path()), dir });
    }
    out.sort_by(|a, b| b.dir.cmp(&a.dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    Ok(out)
}

#[tauri::command]
pub fn editor_list_dir(window: tauri::WebviewWindow, roots: tauri::State<'_, EditorRoots>, path: String) -> Result<Vec<Entry>, String> {
    let root = root_of(&window, &roots)?;
    list_dir(&resolve(&root, &path)?)
}

/// Path relatif (pemisah `/`) semua file di folder, mengikuti .gitignore.
pub fn index(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let walker = ignore::WalkBuilder::new(root).hidden(false).filter_entry(|e| e.file_name() != ".git").build();
    for e in walker.flatten() {
        if out.len() >= INDEX_MAX {
            break;
        }
        if e.file_type().is_some_and(|t| t.is_file()) {
            if let Ok(rel) = e.path().strip_prefix(root) {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    out
}

#[tauri::command]
pub async fn editor_index(window: tauri::WebviewWindow, roots: tauri::State<'_, EditorRoots>) -> Result<Vec<String>, String> {
    let root = root_of(&window, &roots)?;
    tokio::task::spawn_blocking(move || index(&root)).await.map_err(|e| e.to_string())
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FileText {
    text: String,
    /// "\n" atau "\r\n", diambil dari isi (mayoritas).
    eol: String,
    bom: bool,
    mtime: u64,
    size: u64,
    large: bool,
    readonly: bool,
}

/// Teks dari byte file: BOM dibuang, akhir baris dinormalkan ke `\n`.
pub fn decode(bytes: &[u8]) -> Result<(String, String, bool), String> {
    let head = &bytes[..bytes.len().min(8192)];
    if head.contains(&0) {
        return Err("binary: this file is not text".to_string());
    }
    let (bom, body) = match bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        Some(b) => (true, b),
        None => (false, bytes),
    };
    let text = String::from_utf8(body.to_vec()).map_err(|_| "encoding: this file is not UTF-8 (other encodings come after the spike)".to_string())?;
    let crlf = text.matches("\r\n").count();
    let lf = text.matches('\n').count() - crlf;
    let eol = if crlf > lf { "\r\n" } else { "\n" };
    Ok((text.replace("\r\n", "\n"), eol.to_string(), bom))
}

#[tauri::command]
pub fn editor_read(window: tauri::WebviewWindow, roots: tauri::State<'_, EditorRoots>, path: String) -> Result<FileText, String> {
    let root = root_of(&window, &roots)?;
    let p = resolve(&root, &path)?;
    let meta = std::fs::metadata(&p).map_err(|e| e.to_string())?;
    if meta.len() > MAX {
        return Err(format!("too-large: {} MB is over the 50 MB limit", meta.len() / 1024 / 1024));
    }
    let bytes = std::fs::read(&p).map_err(|e| e.to_string())?;
    let (text, eol, bom) = decode(&bytes)?;
    Ok(FileText { text, eol, bom, mtime: mtime_ms(&meta), size: meta.len(), large: meta.len() > LARGE, readonly: meta.permissions().readonly() })
}

/// Tulis atomik: file sementara di folder yang sama, lalu ganti.
pub fn write_atomic(p: &Path, data: &[u8]) -> Result<(), String> {
    use std::io::Write;
    let dir = p.parent().ok_or("invalid path")?;
    let name = p.file_name().ok_or("invalid path")?.to_string_lossy();
    let tmp = dir.join(format!(".{}.kova-tmp", name));
    let res = (|| -> std::io::Result<()> {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
        drop(f);
        // Windows: MoveFileExW dengan REPLACE_EXISTING.
        std::fs::rename(&tmp, p)
    })();
    if let Err(e) = res {
        let _ = std::fs::remove_file(&tmp);
        return Err(match e.kind() {
            std::io::ErrorKind::PermissionDenied => "denied: access denied. The file is read-only, or another program is holding it.".to_string(),
            _ => e.to_string(),
        });
    }
    Ok(())
}

/// Simpan. `expected_mtime` = mtime saat dibaca; bila file di disk sudah
/// berubah, ditolak dengan `conflict:` supaya editor menampilkan Compare.
#[tauri::command]
pub fn editor_write(
    window: tauri::WebviewWindow,
    roots: tauri::State<'_, EditorRoots>,
    path: String,
    text: String,
    eol: String,
    bom: bool,
    expected_mtime: Option<u64>,
) -> Result<u64, String> {
    let root = root_of(&window, &roots)?;
    let p = resolve(&root, &path)?;
    if let (Some(exp), Ok(meta)) = (expected_mtime, std::fs::metadata(&p)) {
        if mtime_ms(&meta) != exp {
            return Err("conflict: the file changed on disk after you opened it".to_string());
        }
    }
    let body = if eol == "\r\n" { text.replace('\n', "\r\n") } else { text };
    let mut data = Vec::with_capacity(body.len() + 3);
    if bom {
        data.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
    }
    data.extend_from_slice(body.as_bytes());
    write_atomic(&p, &data)?;
    std::fs::metadata(&p).map(|m| mtime_ms(&m)).map_err(|e| e.to_string())
}

/// mtime file sekarang (None bila sudah tidak ada). Dipanggil saat jendela
/// mendapat fokus untuk mendeteksi perubahan dari luar (watcher menyusul).
#[tauri::command]
pub fn editor_stat(window: tauri::WebviewWindow, roots: tauri::State<'_, EditorRoots>, path: String) -> Result<Option<u64>, String> {
    let root = root_of(&window, &roots)?;
    let p = resolve(&root, &path)?;
    Ok(std::fs::metadata(&p).ok().map(|m| mtime_ms(&m)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_strips_bom_and_normalizes_crlf() {
        let (t, eol, bom) = decode(b"\xEF\xBB\xBFa\r\nb\r\n").unwrap();
        assert_eq!((t.as_str(), eol.as_str(), bom), ("a\nb\n", "\r\n", true));
        let (t, eol, bom) = decode(b"a\nb\r\nc\n").unwrap();
        assert_eq!((t.as_str(), eol.as_str(), bom), ("a\nb\nc\n", "\n", false));
    }

    #[test]
    fn decode_rejects_binary_and_non_utf8() {
        assert!(decode(b"PK\x03\x04\0\0").unwrap_err().starts_with("binary:"));
        assert!(decode(b"caf\xe9").unwrap_err().starts_with("encoding:"));
    }

    #[test]
    fn resolve_stays_inside_root() {
        let dir = std::env::temp_dir().join(format!("kova-ed-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("app")).unwrap();
        std::fs::write(dir.join("app/a.php"), "x").unwrap();
        let root = std::fs::canonicalize(&dir).unwrap();
        assert!(resolve(&root, "app/a.php").is_ok());
        assert!(resolve(&root, "app/new.php").is_ok());
        assert!(resolve(&root, "../outside.txt").is_err());
        assert!(resolve(&root, "app/../../x").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_atomic_replaces_and_index_lists() {
        let dir = std::env::temp_dir().join(format!("kova-ed-w-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        let f = dir.join("src/a.txt");
        std::fs::write(&f, "old").unwrap();
        write_atomic(&f, b"new").unwrap();
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "new");
        assert!(!dir.join("src/.a.txt.kova-tmp").exists());
        let idx = index(&dir);
        assert_eq!(idx, vec!["src/a.txt".to_string()]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
