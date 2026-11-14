//! Buka folder project di file manager, terminal, atau VS Code (v2.10).
//!
//! Path dipakai sebagai `current_dir` atau satu argumen utuh, tidak pernah
//! disusun ke dalam string perintah shell, supaya karakter seperti `&` atau
//! `;` di nama folder tidak bisa menjalankan perintah lain.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Folder,
    Terminal,
    VsCode,
}

pub fn parse_target(s: &str) -> Result<Target, String> {
    match s {
        "folder" => Ok(Target::Folder),
        "terminal" => Ok(Target::Terminal),
        "vscode" => Ok(Target::VsCode),
        other => Err(format!("unknown target: {other}")),
    }
}

/// Satu percobaan: program, argumen, dan apakah dijalankan di folder project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attempt {
    pub program: &'static str,
    pub args: Vec<String>,
    pub in_dir: bool,
    /// Windows: buka di jendela konsol baru (powershell tanpa Windows Terminal).
    pub new_console: bool,
}

fn attempt(program: &'static str, args: &[&str], in_dir: bool) -> Attempt {
    Attempt { program, args: args.iter().map(|s| s.to_string()).collect(), in_dir, new_console: false }
}

/// Urutan percobaan per OS; yang pertama berhasil dipakai.
pub fn plan(target: Target, os: &str, dir: &str) -> Vec<Attempt> {
    match (target, os) {
        (Target::Folder, "windows") => vec![attempt("explorer", &[dir], false)],
        (Target::Folder, "macos") => vec![attempt("open", &[dir], false)],
        (Target::Folder, _) => vec![attempt("xdg-open", &[dir], false)],

        // `code` di Windows adalah code.cmd: perlu cmd. Argumen tetap literal
        // ("code ."), folder lewat current_dir.
        (Target::VsCode, "windows") => vec![attempt("cmd", &["/C", "code", "."], true)],
        (Target::VsCode, _) => vec![attempt("code", &["."], true)],

        (Target::Terminal, "windows") => vec![
            attempt("wt", &["-d", "."], true),
            Attempt { new_console: true, ..attempt("powershell", &["-NoExit"], true) },
        ],
        (Target::Terminal, "macos") => vec![attempt("open", &["-a", "Terminal", dir], false)],
        (Target::Terminal, _) => vec![
            attempt("x-terminal-emulator", &[], true),
            attempt("gnome-terminal", &[], true),
            attempt("konsole", &[], true),
            attempt("xfce4-terminal", &[], true),
            attempt("xterm", &[], true),
        ],
    }
}

fn not_found_message(target: Target) -> String {
    match target {
        Target::VsCode => "VS Code's `code` command was not found. In VS Code, run \"Shell Command: Install 'code' command in PATH\" (macOS/Linux) or reinstall with \"Add to PATH\" (Windows).".into(),
        Target::Terminal => "No terminal app was found.".into(),
        Target::Folder => "Could not open the file manager.".into(),
    }
}

fn spawn(a: &Attempt, dir: &Path) -> std::io::Result<()> {
    let mut cmd = Command::new(a.program);
    cmd.args(&a.args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    if a.in_dir {
        cmd.current_dir(dir);
    }
    #[cfg(not(target_os = "windows"))]
    cmd.env("PATH", crate::commands::util::resolved_path());
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
        cmd.creation_flags(if a.new_console { CREATE_NEW_CONSOLE } else { CREATE_NO_WINDOW });
    }
    cmd.spawn().map(|_| ())
}

fn current_os() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

#[tauri::command]
pub async fn project_open_in(target: String, path: String) -> Result<(), String> {
    let target = parse_target(&target)?;
    let dir = PathBuf::from(&path);
    if !dir.is_dir() {
        return Err(format!("Folder not found: {path}"));
    }
    let attempts = plan(target, current_os(), &path);
    for a in &attempts {
        match spawn(a, &dir) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(format!("Could not start {}: {e}", a.program)),
        }
    }
    Err(not_found_message(target))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_parse_and_reject_unknown() {
        assert_eq!(parse_target("vscode").unwrap(), Target::VsCode);
        assert!(parse_target("rm -rf").is_err());
    }

    #[test]
    fn path_is_never_part_of_a_shell_string() {
        let dir = r"C:\dev\a&b; calc";
        for os in ["windows", "macos", "linux"] {
            for t in [Target::Folder, Target::Terminal, Target::VsCode] {
                for a in plan(t, os, dir) {
                    // Folder dipakai sebagai current_dir, atau sebagai satu
                    // argumen utuh untuk program yang bukan shell.
                    if a.program == "cmd" || a.program == "powershell" {
                        assert!(a.in_dir && !a.args.iter().any(|x| x.contains(dir)), "{os} {t:?}");
                    }
                    assert!(a.args.iter().all(|x| x == dir || !x.contains("calc")));
                }
            }
        }
    }

    #[test]
    fn windows_terminal_falls_back_to_powershell() {
        let p = plan(Target::Terminal, "windows", r"C:\dev\x");
        assert_eq!(p[0].program, "wt");
        assert_eq!(p[1].program, "powershell");
        assert!(p[1].new_console && p[1].in_dir);
    }
}
