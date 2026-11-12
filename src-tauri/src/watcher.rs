use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use notify_debouncer_mini::notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::{new_debouncer, Debouncer};
use tauri::{Emitter, Window};

pub type WatcherState = Arc<Mutex<Option<Debouncer<RecommendedWatcher>>>>;

pub fn default_watcher_state() -> WatcherState {
    Arc::new(Mutex::new(None))
}

/// Parse the first meaningful line from a version file (.phpvmrc / .nvmrc).
/// - Skip blank lines and lines starting with `#`
/// - Strip leading `v` prefix and surrounding whitespace
///
/// Returns `None` if no valid line found.
fn parse_version_file(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let version = trimmed.trim_start_matches('v').trim().to_string();
        if !version.is_empty() {
            return Some(version);
        }
    }
    None
}

/// Emit `phpvmrc-detected` or `nvmrc-detected` for a given file path.
/// Reads the file, parses the version, and emits the event.
/// On parse failure or read error, logs to stderr and does nothing.
fn emit_for_file(window: &Window, file_path: &Path) {
    let name = match file_path.file_name().and_then(|n| n.to_str()) {
        Some(n) => n,
        None => return,
    };

    let event_name = match name {
        ".phpvmrc" => "phpvmrc-detected",
        ".nvmrc" => "nvmrc-detected",
        _ => return,
    };

    let content = match std::fs::read_to_string(file_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[watcher] failed to read {:?}: {}", file_path, e);
            return;
        }
    };

    let version = match parse_version_file(&content) {
        Some(v) => v,
        None => {
            eprintln!("[watcher] no valid version in {:?}", file_path);
            return;
        }
    };

    let path_str = file_path.to_string_lossy().to_string();
    let payload = serde_json::json!({ "version": version, "path": path_str });
    let _ = window.emit(event_name, payload);
}

/// Check if the project folder already contains .phpvmrc or .nvmrc and emit
/// immediately (initial sync before the watcher catches any change events).
fn initial_scan(window: &Window, dir: &Path) {
    for filename in [".phpvmrc", ".nvmrc"] {
        let file_path = dir.join(filename);
        if file_path.exists() {
            emit_for_file(window, &file_path);
        }
    }
}

/// Watch the given project directory for `.phpvmrc` and `.nvmrc` changes.
///
/// Behaviour:
/// - Performs an initial scan immediately on call (emit if files exist).
/// - Replaces any previously registered watcher (single-watch policy).
/// - Watch is non-recursive — only the root of `path` is monitored.
/// - Debounce: 500 ms.
#[tauri::command]
pub async fn watch_project(
    window: Window,
    state: tauri::State<'_, WatcherState>,
    path: String,
) -> Result<(), String> {
    let dir = PathBuf::from(&path);

    if !dir.exists() {
        return Err(format!("Path tidak ditemukan: {}", path));
    }
    if !dir.is_dir() {
        return Err(format!("Path bukan direktori: {}", path));
    }

    // Initial scan — emit immediately if version files already present.
    initial_scan(&window, &dir);

    let window_clone = window.clone();
    let dir_clone = dir.clone();

    let mut debouncer = new_debouncer(
        Duration::from_millis(500),
        move |res: notify_debouncer_mini::DebounceEventResult| match res {
            Ok(events) => {
                for event in events {
                    let file_path: &Path = &event.path;
                    let name = file_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("");
                    if name == ".phpvmrc" || name == ".nvmrc" {
                        // Only emit for files directly inside the watched dir.
                        if file_path.parent() == Some(dir_clone.as_path()) {
                            emit_for_file(&window_clone, file_path);
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("[watcher] debouncer error: {:?}", e);
            }
        },
    )
    .map_err(|e| format!("Gagal membuat file watcher: {}", e))?;

    debouncer
        .watcher()
        .watch(&dir, RecursiveMode::NonRecursive)
        .map_err(|e| format!("Gagal watch direktori: {}", e))?;

    // Replace the previous watcher (drop old, store new).
    let mut guard = state.lock().map_err(|e| format!("State lock error: {}", e))?;
    *guard = Some(debouncer);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_version_file_basic() {
        assert_eq!(parse_version_file("8.3\n"), Some("8.3".to_string()));
        assert_eq!(parse_version_file("v20.14.0\n"), Some("20.14.0".to_string()));
    }

    #[test]
    fn test_parse_version_file_comment_skip() {
        let content = "# this is a comment\n8.2\n";
        assert_eq!(parse_version_file(content), Some("8.2".to_string()));
    }

    #[test]
    fn test_parse_version_file_blank_lines() {
        let content = "\n\n  \nv18\n";
        assert_eq!(parse_version_file(content), Some("18".to_string()));
    }

    #[test]
    fn test_parse_version_file_lts_alias() {
        // lts/iron is a valid fnm alias — returned as-is (no digit requirement)
        assert_eq!(parse_version_file("lts/iron\n"), Some("lts/iron".to_string()));
    }

    #[test]
    fn test_parse_version_file_only_comments() {
        let content = "# comment\n# another\n";
        assert_eq!(parse_version_file(content), None);
    }

    #[test]
    fn test_parse_version_file_empty() {
        assert_eq!(parse_version_file(""), None);
    }

    #[test]
    fn test_parse_version_file_v_prefix_with_spaces() {
        assert_eq!(parse_version_file("  v8.3.0  \n"), Some("8.3.0".to_string()));
    }
}
