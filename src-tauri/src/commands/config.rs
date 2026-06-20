use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ConfigState {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub selected_service_ids: Vec<String>,
    #[serde(default)]
    pub last_php_version: Option<String>,
    #[serde(default)]
    pub last_node_version: Option<String>,
    #[serde(default)]
    pub watched_path: Option<String>,
    #[serde(default)]
    pub auto_start: bool,
    #[serde(default = "default_true")]
    pub remember_session: bool,
    #[serde(default = "default_true")]
    pub minimize_to_tray: bool,
}

fn default_true() -> bool {
    true
}

impl Default for ConfigState {
    fn default() -> Self {
        Self {
            version: 1,
            selected_service_ids: Vec::new(),
            last_php_version: None,
            last_node_version: None,
            watched_path: None,
            auto_start: false,
            remember_session: true,
            minimize_to_tray: true,
        }
    }
}

/// Tulis config ke `path` secara atomik + durable terhadap hard restart PC.
///
/// Urutan: tulis ke `.tmp` → `sync_all()` (fsync isi file ke disk) → `rename` →
/// fsync direktori parent (best-effort) agar entri rename ikut durable.
/// Tanpa fsync file, pada hard power-loss isi bisa belum ter-flush walau rename
/// sudah ter-journal → config.json kosong/truncated.
fn write_config_atomic(path: &Path, json: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("gagal create_dir_all: {}", e))?;
    }

    let tmp_path = path.with_extension("tmp");
    {
        let mut f = std::fs::File::create(&tmp_path)
            .map_err(|e| format!("gagal buat config.tmp: {}", e))?;
        f.write_all(json.as_bytes())
            .map_err(|e| format!("gagal tulis config.tmp: {}", e))?;
        f.sync_all()
            .map_err(|e| format!("gagal fsync config.tmp: {}", e))?;
    }

    std::fs::rename(&tmp_path, path)
        .map_err(|e| format!("gagal rename config.tmp \u{2192} config.json: {}", e))?;

    // fsync direktori parent agar entri rename durable. Pada Windows File::open ke
    // direktori tidak selalu didukung — best-effort, abaikan error.
    #[cfg(unix)]
    if let Some(parent) = path.parent() {
        if let Ok(dir) = std::fs::File::open(parent) {
            let _ = dir.sync_all();
        }
    }

    Ok(())
}

pub fn config_path(app: &AppHandle) -> Result<PathBuf, String> {
    let config_dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("gagal resolve app_config_dir: {}", e))?;
    Ok(config_dir.join("config.json"))
}

#[tauri::command]
pub async fn config_read(app: AppHandle) -> Result<ConfigState, String> {
    let path = config_path(&app)?;

    if !path.exists() {
        return Ok(ConfigState::default());
    }

    let raw = std::fs::read_to_string(&path)
        .map_err(|e| format!("gagal baca config.json: {}", e));

    let raw = match raw {
        Ok(s) => s,
        Err(err) => {
            eprintln!("[config_read] warning: {} — returning default", err);
            return Ok(ConfigState::default());
        }
    };

    let mut cfg: ConfigState = match serde_json::from_str(&raw) {
        Ok(c) => c,
        Err(err) => {
            eprintln!(
                "[config_read] warning: config.json corrupt ({}), returning default",
                err
            );
            return Ok(ConfigState::default());
        }
    };

    // Migrasi v0 → v1: kalau version absent di JSON, serde(default) akan set 0.
    if cfg.version == 0 {
        cfg.version = 1;
    }

    // Sync ke in-memory Mutex agar tray (yang baca Mutex) lihat selection tersimpan
    // tanpa harus menunggu user toggle ulang setelah app boot.
    if let Some(state) = app.try_state::<Mutex<ConfigState>>() {
        if let Ok(mut guard) = state.lock() {
            *guard = cfg.clone();
        }
    }

    Ok(cfg)
}

#[tauri::command]
pub async fn config_write(app: AppHandle, config: ConfigState) -> Result<(), String> {
    let path = config_path(&app)?;

    let json = serde_json::to_string_pretty(&config)
        .map_err(|e| format!("gagal serialize config: {}", e))?;

    // Atomic + durable write: tulis ke .tmp, fsync, lalu rename.
    write_config_atomic(&path, &json)?;

    // Sync ke in-memory Mutex<ConfigState> — tray membaca dari Mutex ini
    // saat user klik "Start all selected". Tanpa sync, Mutex stale = default kosong.
    if let Some(state) = app.try_state::<Mutex<ConfigState>>() {
        if let Ok(mut guard) = state.lock() {
            *guard = config.clone();
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_shape() {
        let cfg = ConfigState::default();
        assert_eq!(cfg.version, 1);
        assert!(!cfg.auto_start);
        assert!(cfg.remember_session);
        assert!(cfg.minimize_to_tray);
        assert!(cfg.selected_service_ids.is_empty());
        assert!(cfg.last_php_version.is_none());
        assert!(cfg.last_node_version.is_none());
        assert!(cfg.watched_path.is_none());
    }

    #[test]
    fn test_serialize_deserialize_roundtrip() {
        let original = ConfigState {
            version: 1,
            selected_service_ids: vec!["mysql".to_string(), "redis".to_string()],
            last_php_version: Some("8.3".to_string()),
            last_node_version: Some("20.0.0".to_string()),
            watched_path: Some("/home/user/project".to_string()),
            auto_start: true,
            remember_session: false,
            minimize_to_tray: true,
        };

        let json = serde_json::to_string(&original).expect("serialize gagal");

        // Verifikasi camelCase keys ada di JSON output
        assert!(json.contains("selectedServiceIds"));
        assert!(json.contains("lastPhpVersion"));
        assert!(json.contains("lastNodeVersion"));
        assert!(json.contains("watchedPath"));
        assert!(json.contains("autoStart"));
        assert!(json.contains("rememberSession"));
        assert!(json.contains("minimizeToTray"));

        let restored: ConfigState = serde_json::from_str(&json).expect("deserialize gagal");
        assert_eq!(restored.version, original.version);
        assert_eq!(restored.selected_service_ids, original.selected_service_ids);
        assert_eq!(restored.last_php_version, original.last_php_version);
        assert_eq!(restored.last_node_version, original.last_node_version);
        assert_eq!(restored.watched_path, original.watched_path);
        assert_eq!(restored.auto_start, original.auto_start);
        assert_eq!(restored.remember_session, original.remember_session);
        assert_eq!(restored.minimize_to_tray, original.minimize_to_tray);
    }

    #[test]
    fn test_parse_with_unknown_field() {
        // Field tak dikenal harus di-skip (bukan error) — default serde behavior
        let json = r#"{
            "version": 1,
            "selectedServiceIds": ["mysql"],
            "autoStart": false,
            "rememberSession": true,
            "minimizeToTray": true,
            "unknownFutureField": "some_value",
            "anotherUnknown": 42
        }"#;

        let cfg: ConfigState = serde_json::from_str(json).expect("harus berhasil parse");
        assert_eq!(cfg.version, 1);
        assert_eq!(cfg.selected_service_ids, vec!["mysql"]);
        assert!(!cfg.auto_start);
        assert!(cfg.remember_session);
        assert!(cfg.minimize_to_tray);
    }

    #[test]
    fn test_write_config_atomic_roundtrip() {
        // Tulis ke tempdir lalu baca ulang & verifikasi isi durable + benar.
        let mut dir = std::env::temp_dir();
        dir.push(format!("servel_test_{}", std::process::id()));
        let path = dir.join("config.json");

        let cfg = ConfigState {
            version: 1,
            selected_service_ids: vec!["mysql".to_string(), "minio".to_string()],
            last_php_version: Some("8.3".to_string()),
            last_node_version: None,
            watched_path: Some("/tmp/proj".to_string()),
            auto_start: true,
            remember_session: true,
            minimize_to_tray: false,
        };
        let json = serde_json::to_string_pretty(&cfg).expect("serialize");

        write_config_atomic(&path, &json).expect("write_config_atomic gagal");

        let raw = std::fs::read_to_string(&path).expect("baca ulang gagal");
        let restored: ConfigState = serde_json::from_str(&raw).expect("parse ulang gagal");
        assert_eq!(restored.selected_service_ids, cfg.selected_service_ids);
        assert_eq!(restored.last_php_version, cfg.last_php_version);
        assert_eq!(restored.watched_path, cfg.watched_path);
        assert!(restored.auto_start);
        assert!(!restored.minimize_to_tray);

        // tmp tidak boleh tersisa setelah rename.
        assert!(!path.with_extension("tmp").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_migrate_v0_to_v1() {
        // JSON tanpa field "version" → serde(default) set 0 → harus di-bump ke 1
        let json = r#"{
            "selectedServiceIds": ["postgres"],
            "autoStart": false,
            "rememberSession": true,
            "minimizeToTray": false
        }"#;

        let mut cfg: ConfigState = serde_json::from_str(json).expect("parse gagal");
        // Simulasi logika migrasi yang ada di config_read
        if cfg.version == 0 {
            cfg.version = 1;
        }

        assert_eq!(cfg.version, 1);
        assert_eq!(cfg.selected_service_ids, vec!["postgres"]);
        assert!(!cfg.minimize_to_tray);
    }
}
