//! Perintah terminal `kova` di macOS dan Linux.
//!
//! Windows: installer NSIS yang memasang CLI (PowerShell) dan PATH. Di macOS
//! dan Linux tidak ada installer, jadi aplikasi menyalin skrip `kova.sh` ke
//! `~/.local/bin/kova` atas permintaan pengguna (Settings → General). Skrip
//! ditanam di biner (`include_str!`), jadi berlaku juga untuk AppImage.

// Di Windows hanya `cli_status` (supported: false) yang terpakai.
#![cfg_attr(not(unix), allow(dead_code, unused_imports))]

use serde::Serialize;
use std::path::{Path, PathBuf};

#[cfg(any(unix, test))]
const SCRIPT: &str = include_str!("../../assets/cli/kova.sh");
/// Baris penanda di skrip; file lain bernama `kova` tidak ditimpa.
const MARKER: &str = "# kova: Kova CLI for macOS and Linux.";

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CliStatus {
    /// `false` di Windows (CLI dipasang installer).
    pub supported: bool,
    pub installed: bool,
    /// Skrip terpasang berbeda dari versi aplikasi ini.
    pub outdated: bool,
    pub path: String,
    /// Folder tujuan ada di PATH proses ini.
    pub on_path: bool,
}

fn target_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("bin"))
}

pub fn dir_on_path(dir: &Path, path_var: &str) -> bool {
    std::env::split_paths(path_var).any(|p| p == dir)
}

#[cfg(any(unix, test))]
pub fn status_at(dir: &Path, path_var: &str) -> CliStatus {
    let file = dir.join("kova");
    let existing = std::fs::read_to_string(&file).ok();
    let ours = existing.as_deref().is_some_and(|s| s.contains(MARKER));
    CliStatus {
        supported: true,
        installed: ours,
        outdated: ours && existing.as_deref() != Some(SCRIPT),
        path: file.to_string_lossy().into_owned(),
        on_path: dir_on_path(dir, path_var),
    }
}

#[cfg(any(unix, test))]
pub fn install_at(dir: &Path) -> Result<PathBuf, String> {
    let file = dir.join("kova");
    if let Ok(existing) = std::fs::read_to_string(&file) {
        if !existing.contains(MARKER) {
            return Err(format!("{} already exists and was not installed by Kova; move it away first.", file.display()));
        }
    } else if file.exists() {
        return Err(format!("{} already exists and was not installed by Kova; move it away first.", file.display()));
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
    std::fs::write(&file, SCRIPT).map_err(|e| format!("could not write {}: {e}", file.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("could not make {} executable: {e}", file.display()))?;
    }
    Ok(file)
}

fn current_status() -> CliStatus {
    #[cfg(unix)]
    if let Some(dir) = target_dir() {
        return status_at(&dir, &std::env::var("PATH").unwrap_or_default());
    }
    let _ = target_dir;
    CliStatus { supported: false, installed: false, outdated: false, path: String::new(), on_path: false }
}

#[tauri::command]
pub fn cli_status() -> CliStatus {
    current_status()
}

#[tauri::command]
pub fn cli_install() -> Result<CliStatus, String> {
    #[cfg(unix)]
    {
        let dir = target_dir().ok_or_else(|| "could not find your home folder".to_string())?;
        install_at(&dir)?;
        Ok(current_status())
    }
    #[cfg(not(unix))]
    {
        Err("on Windows the kova command is installed by the installer".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("kova-cli-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn install_writes_script_and_reports_status() {
        let dir = tmp("install").join("bin");
        let before = status_at(&dir, "/usr/bin");
        assert!(!before.installed && !before.on_path && before.supported);

        let file = install_at(&dir).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), SCRIPT);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o755);
        }
        let path_var = std::env::join_paths([Path::new("/usr/bin"), dir.as_path()]).unwrap();
        let after = status_at(&dir, path_var.to_str().unwrap());
        assert!(after.installed && after.on_path && !after.outdated);

        // Versi lama milik Kova: ditandai usang, lalu diperbarui saat install ulang.
        std::fs::write(&file, format!("#!/bin/sh\n{MARKER}\necho old\n")).unwrap();
        assert!(status_at(&dir, "").outdated);
        install_at(&dir).unwrap();
        assert!(!status_at(&dir, "").outdated);
        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
    }

    #[test]
    fn foreign_file_is_not_overwritten() {
        let dir = tmp("foreign");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("kova"), "#!/bin/sh\necho something else\n").unwrap();
        assert!(!status_at(&dir, "").installed);
        assert!(install_at(&dir).unwrap_err().contains("was not installed by Kova"));
        assert_eq!(std::fs::read_to_string(dir.join("kova")).unwrap(), "#!/bin/sh\necho something else\n");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn script_carries_the_marker() {
        assert!(SCRIPT.contains(MARKER));
        assert!(SCRIPT.starts_with("#!/bin/sh\n"));
    }
}
