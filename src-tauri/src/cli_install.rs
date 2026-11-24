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

// --- Windows -----------------------------------------------------------------
// Installer NSIS menyalin `cli\` ke folder instalasi dan menambah `cli\bin`
// ke PATH user. Bila entri PATH hilang (dibersihkan tool lain, profil baru),
// Settings menawarkan Reinstall: menjalankan `install-path.ps1` yang sama.

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WinCliStatus {
    pub bin_dir: String,
    /// `kova.cmd` ada di folder instalasi.
    pub files_present: bool,
    /// Folder `cli\bin` tercantum di PATH user (registry, bukan PATH proses).
    pub on_path: bool,
}

#[cfg_attr(not(windows), allow(dead_code))]
fn norm_dir(p: &str) -> String {
    p.trim().trim_end_matches(['\\', '/']).to_lowercase()
}

#[cfg_attr(not(windows), allow(dead_code))]
/// `%VAR%` diganti nilai dari `lookup`; nama yang tidak dikenal dibiarkan.
fn expand_vars(s: &str, lookup: &dyn Fn(&str) -> Option<String>) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) => {
                let name = &after[..end];
                match lookup(name) {
                    Some(v) if !name.is_empty() => out.push_str(&v),
                    _ => {
                        out.push('%');
                        out.push_str(name);
                        out.push('%');
                    }
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg_attr(not(windows), allow(dead_code))]
/// Apakah `dir` ada di nilai PATH (dipisah `;`, entri boleh memakai `%VAR%`).
pub fn path_value_contains(path_value: &str, dir: &str, lookup: &dyn Fn(&str) -> Option<String>) -> bool {
    let want = norm_dir(dir);
    path_value.split(';').filter(|p| !p.trim().is_empty()).any(|p| norm_dir(&expand_vars(p, lookup)) == want)
}

#[cfg_attr(not(windows), allow(dead_code))]
/// Nilai `Path` dari keluaran `reg query HKCU\Environment /v Path`.
pub fn parse_reg_path(out: &str) -> Option<String> {
    out.lines().find_map(|l| {
        let t = l.trim_start();
        let rest = t.strip_prefix("Path").or_else(|| t.strip_prefix("PATH"))?;
        let rest = rest.trim_start();
        let rest = rest.strip_prefix("REG_EXPAND_SZ").or_else(|| rest.strip_prefix("REG_SZ"))?;
        Some(rest.trim().to_string())
    })
}

#[cfg(windows)]
async fn user_path_value() -> String {
    let out = crate::commands::util::silent_command("reg")
        .args(["query", "HKCU\\Environment", "/v", "Path"])
        .stdout(std::process::Stdio::piped())
        .output()
        .await;
    out.ok().and_then(|o| parse_reg_path(&String::from_utf8_lossy(&o.stdout))).unwrap_or_default()
}

#[cfg(windows)]
fn cli_root(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    use tauri::Manager;
    app.path().resource_dir().map(|d| d.join("cli")).map_err(|e| format!("could not find the install folder: {e}"))
}

#[cfg(windows)]
async fn win_status(app: &tauri::AppHandle) -> Result<WinCliStatus, String> {
    let bin = cli_root(app)?.join("bin");
    let bin_s = bin.to_string_lossy().into_owned();
    let path = user_path_value().await;
    Ok(WinCliStatus {
        files_present: bin.join("kova.cmd").is_file(),
        on_path: path_value_contains(&path, &bin_s, &|n| std::env::var(n).ok()),
        bin_dir: bin_s,
    })
}

#[tauri::command]
pub async fn cli_windows_status(app: tauri::AppHandle) -> Result<WinCliStatus, String> {
    #[cfg(windows)]
    {
        win_status(&app).await
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        Err("only on Windows".to_string())
    }
}

/// Tambah lagi `cli\bin` ke PATH user lewat skrip installer.
#[tauri::command]
pub async fn cli_windows_reinstall(app: tauri::AppHandle) -> Result<WinCliStatus, String> {
    #[cfg(windows)]
    {
        let root = cli_root(&app)?;
        let script = root.join("install-path.ps1");
        if !script.is_file() || !root.join("bin").join("kova.cmd").is_file() {
            return Err(format!(
                "The kova files are missing from {}. Run the Kova installer again to restore them.",
                root.display()
            ));
        }
        let out = crate::commands::util::silent_command("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(&script)
            .args(["-Action", "add", "-Dir"])
            .arg(root.join("bin"))
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
            .await
            .map_err(|e| format!("could not run PowerShell: {e}"))?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
            return Err(if err.is_empty() { "PowerShell could not update your PATH".to_string() } else { err });
        }
        win_status(&app).await
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        Err("only on Windows".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_value_matches_expanded_and_trailing_slash() {
        let env = |n: &str| (n == "LOCALAPPDATA").then(|| r"C:\Users\dev\AppData\Local".to_string());
        let dir = r"C:\Users\dev\AppData\Local\Kova\cli\bin";
        assert!(path_value_contains(r"C:\Windows;%LOCALAPPDATA%\Kova\cli\bin\", dir, &env));
        assert!(path_value_contains(r"c:\users\dev\appdata\local\kova\CLI\bin", dir, &env));
        assert!(!path_value_contains(r"C:\Windows;%UNKNOWN%\Kova\cli\bin", dir, &env));
        assert!(!path_value_contains("", dir, &env));
    }

    #[test]
    fn reg_query_output_is_parsed() {
        let out = "\r\nHKEY_CURRENT_USER\\Environment\r\n    Path    REG_EXPAND_SZ    %USERPROFILE%\\bin;C:\\Tools\r\n\r\n";
        assert_eq!(parse_reg_path(out).as_deref(), Some(r"%USERPROFILE%\bin;C:\Tools"));
        assert_eq!(parse_reg_path("ERROR: not found"), None);
    }

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
