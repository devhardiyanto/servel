//! "Start Servel when you sign in" (v2.11).
//!
//! Ditulis sendiri, tanpa tauri-plugin-autostart: versi plugin yang tersedia
//! menuntut upgrade tauri/wry. Mekanismenya standar per OS:
//! - Windows: nilai `Servel` di HKCU\Software\Microsoft\Windows\CurrentVersion\Run (lewat reg.exe)
//! - macOS: LaunchAgent ~/Library/LaunchAgents/com.devhardiyanto.servel.plist
//! - Linux: ~/.config/autostart/servel.desktop
//!
//! Aplikasi dijalankan dengan `--hidden`: bila tray aktif, jendela tidak
//! dimunculkan saat login (lihat `started_hidden` di lib.rs).

use std::path::{Path, PathBuf};

pub const HIDDEN_ARG: &str = "--hidden";
#[cfg(any(windows, test))]
const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
#[cfg(any(windows, test))]
const VALUE_NAME: &str = "Servel";
#[cfg(any(target_os = "macos", test))]
const LABEL: &str = "com.devhardiyanto.servel";

/// Path program yang didaftarkan. AppImage: path file .AppImage, bukan mount sementara.
fn program() -> Result<String, String> {
    if let Ok(p) = std::env::var("APPIMAGE") {
        if !p.is_empty() {
            return Ok(p);
        }
    }
    std::env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .map_err(|e| format!("could not find the Servel program: {e}"))
}

#[allow(dead_code)]
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

#[cfg(any(target_os = "macos", test))]
pub fn launch_agent_plist(program: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{LABEL}</string>
  <key>ProgramArguments</key>
  <array><string>{}</string><string>{HIDDEN_ARG}</string></array>
  <key>RunAtLoad</key><true/>
</dict>
</plist>
"#,
        xml_escape(program)
    )
}

/// Exec di file .desktop: argumen dikutip, `"`, `` ` ``, `$`, `\` di-escape (spesifikasi Desktop Entry).
#[cfg(any(all(unix, not(target_os = "macos")), test))]
pub fn desktop_entry(program: &str) -> String {
    let mut q = String::new();
    for c in program.chars() {
        if matches!(c, '"' | '`' | '$' | '\\') {
            q.push('\\');
        }
        q.push(c);
    }
    format!(
        "[Desktop Entry]\nType=Application\nName=Servel\nExec=\"{q}\" {HIDDEN_ARG}\nX-GNOME-Autostart-enabled=true\nNoDisplay=true\n"
    )
}

/// Argumen reg.exe untuk mendaftarkan program (nilai: "<exe>" --hidden).
#[cfg(any(windows, test))]
pub fn reg_add_args(program: &str) -> Vec<String> {
    vec![
        "add".into(),
        RUN_KEY.into(),
        "/v".into(),
        VALUE_NAME.into(),
        "/t".into(),
        "REG_SZ".into(),
        "/d".into(),
        format!("\"{program}\" {HIDDEN_ARG}"),
        "/f".into(),
    ]
}

#[allow(dead_code)]
fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default()
}

#[allow(dead_code)]
fn entry_path() -> PathBuf {
    if cfg!(target_os = "macos") {
        home().join("Library/LaunchAgents").join("com.devhardiyanto.servel.plist")
    } else {
        let base = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".config"));
        base.join("autostart").join("servel.desktop")
    }
}

#[allow(dead_code)]
fn write_file(path: &Path, content: &str) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
    }
    std::fs::write(path, content).map_err(|e| format!("could not write {}: {e}", path.display()))
}

#[cfg(windows)]
fn reg(args: &[String]) -> Result<bool, String> {
    use std::os::windows::process::CommandExt;
    let out = std::process::Command::new("reg")
        .args(args)
        .creation_flags(0x0800_0000)
        .output()
        .map_err(|e| format!("could not run reg.exe: {e}"))?;
    Ok(out.status.success())
}

pub fn is_enabled() -> bool {
    #[cfg(windows)]
    {
        let args: Vec<String> = vec!["query".into(), RUN_KEY.into(), "/v".into(), VALUE_NAME.into()];
        reg(&args).unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        entry_path().is_file()
    }
}

pub fn set_enabled(enabled: bool) -> Result<(), String> {
    #[cfg(windows)]
    {
        let args = if enabled {
            reg_add_args(&program()?)
        } else {
            if !is_enabled() {
                return Ok(());
            }
            vec!["delete".into(), RUN_KEY.into(), "/v".into(), VALUE_NAME.into(), "/f".into()]
        };
        if reg(&args)? {
            Ok(())
        } else {
            Err("Windows did not accept the startup setting (reg.exe failed).".into())
        }
    }
    #[cfg(not(windows))]
    {
        let path = entry_path();
        if !enabled {
            return match std::fs::remove_file(&path) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(format!("could not remove {}: {e}", path.display())),
            };
        }
        let prog = program()?;
        #[cfg(target_os = "macos")]
        let content = launch_agent_plist(&prog);
        #[cfg(not(target_os = "macos"))]
        let content = desktop_entry(&prog);
        write_file(&path, &content)
    }
}

#[tauri::command]
pub async fn autostart_get() -> bool {
    tokio::task::spawn_blocking(is_enabled).await.unwrap_or(false)
}

#[tauri::command]
pub async fn autostart_set(enabled: bool) -> Result<bool, String> {
    tokio::task::spawn_blocking(move || set_enabled(enabled).map(|_| is_enabled()))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_value_quotes_the_program_and_adds_hidden() {
        let a = reg_add_args(r"C:\Program Files\Servel\servel.exe");
        assert_eq!(a[1], RUN_KEY);
        assert_eq!(a[7], r#""C:\Program Files\Servel\servel.exe" --hidden"#);
        assert_eq!(a.last().unwrap(), "/f");
    }

    #[test]
    fn plist_escapes_xml() {
        let p = launch_agent_plist("/Applications/A&B.app/Contents/MacOS/servel");
        assert!(p.contains("<string>/Applications/A&amp;B.app/Contents/MacOS/servel</string><string>--hidden</string>"));
        assert!(p.contains("<key>RunAtLoad</key><true/>"));
    }

    #[test]
    fn desktop_exec_is_quoted_and_escaped() {
        let d = desktop_entry("/home/u/Apps/Ser$vel \"x\".AppImage");
        assert!(d.contains(r#"Exec="/home/u/Apps/Ser\$vel \"x\".AppImage" --hidden"#), "{d}");
    }
}
