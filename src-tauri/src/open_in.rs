//! Buka folder project di file manager, terminal, atau VS Code.
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

/// Windows, folder di distro WSL: terminal dan VS Code dibuka di dalam distro.
/// Distro sudah divalidasi `wsl::parse`; path Linux adalah satu argumen utuh
/// untuk program yang bukan shell. File manager tetap memakai path UNC.
pub fn plan_wsl(target: Target, wp: &crate::wsl::WslPath) -> Option<Vec<Attempt>> {
    let d = wp.distro.as_str();
    let p = wp.path.as_str();
    match target {
        Target::Folder => None,
        Target::Terminal => Some(vec![Attempt {
            new_console: true,
            ..attempt("wsl.exe", &["-d", d, "--cd", p], false)
        }]),
        // code.cmd langsung (bukan lewat `cmd /C`): Rust meng-escape argumen
        // untuk file batch, jadi `&` di path tidak dijalankan.
        Target::VsCode => Some(vec![attempt("code.cmd", &["--remote", &format!("wsl+{}", d), p], false)]),
    }
}

fn not_found_message(target: Target) -> String {
    match target {
        Target::VsCode => "VS Code's `code` command was not found. In VS Code, run \"Shell Command: Install 'code' command in PATH\" (macOS/Linux) or reinstall with \"Add to PATH\" (Windows).".into(),
        Target::Terminal => "No terminal app was found.".into(),
        Target::Folder => "Could not open the file manager.".into(),
    }
}

fn spawn(a: &Attempt, dir: &Path, env: &[(String, String)]) -> std::io::Result<()> {
    let mut cmd = Command::new(a.program);
    cmd.envs(env.iter().map(|(k, v)| (k.as_str(), v.as_str())));
    cmd.args(&a.args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    // Terminal/editor yang dibuka dari AppImage tidak boleh mewarisi library-nya.
    #[cfg(not(target_os = "windows"))]
    crate::commands::util::apply_host_env_std(&mut cmd);
    if a.in_dir {
        cmd.current_dir(dir);
    }
    #[cfg(not(target_os = "windows"))]
    cmd.env("PATH", crate::commands::util::resolved_path().as_str());
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
        crate::commands::util::refresh_windows_path();
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

/// Escape satu nilai untuk string berkutip tunggal PowerShell.
pub fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// Escape satu nilai untuk string berkutip tunggal sh.
pub fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Nama variabel yang sah di sh (`[A-Za-z_][A-Za-z0-9_]*`).
fn sh_name(k: &str) -> bool {
    let mut c = k.chars();
    c.next().is_some_and(|f| f.is_ascii_alphabetic() || f == '_') && c.all(|x| x.is_ascii_alphanumeric() || x == '_')
}

/// Script PowerShell yang memasang ENV project lalu menghapus dirinya.
pub fn ps_script(env: &[(String, String)], banner: &str) -> String {
    let mut out = String::new();
    for (k, v) in env {
        out.push_str(&format!("[Environment]::SetEnvironmentVariable({}, {}, 'Process')\n", ps_quote(k), ps_quote(v)));
    }
    out.push_str(&format!("Write-Host {} -ForegroundColor Cyan\n", ps_quote(banner)));
    out.push_str("Remove-Item -LiteralPath $PSCommandPath -ErrorAction SilentlyContinue\n");
    out
}

/// Script sh yang memasang ENV, pindah ke folder, lalu membuka shell pengguna.
pub fn sh_script(env: &[(String, String)], banner: &str, dir: &str) -> String {
    let mut out = String::from("#!/bin/sh\nrm -f -- \"$0\"\n");
    for (k, v) in env.iter().filter(|(k, _)| sh_name(k)) {
        out.push_str(&format!("export {}={}\n", k, sh_quote(v)));
    }
    out.push_str(&format!("printf '%s\\n' {}\n", sh_quote(banner)));
    out.push_str(&format!("cd -- {} || exit 1\n", sh_quote(dir)));
    out.push_str("exec \"${SHELL:-/bin/sh}\" -i\n");
    out
}

fn temp_script(ext: &str, content: &str) -> Result<PathBuf, String> {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let p = std::env::temp_dir().join(format!("kova-term-{}-{}.{}", std::process::id(), nanos, ext));
    std::fs::write(&p, content).map_err(|e| format!("could not write the terminal script: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o700));
    }
    Ok(p)
}

/// Terminal here untuk project: ENV yang sama dengan Start (PORT, Environment
/// aktif, dan versi mise di Windows/macOS/Linux). Lewat script sementara,
/// karena Windows Terminal dan gnome-terminal tidak mewarisi environment
/// proses yang membukanya.
fn terminal_plan(project: &crate::commands::config::Project, mut env: Vec<(String, String)>, dir: &str) -> Result<(Vec<Attempt>, Vec<(String, String)>), String> {
    let label = project.active_env_entry().map(|e| e.name.as_str()).unwrap_or("Default");
    let names: Vec<&str> = env.iter().map(|(k, _)| k.as_str()).filter(|k| !matches!(*k, "PATH" | "Path")).collect();
    let banner = format!("Kova: {} from {} > {}", if names.is_empty() { "no variables".to_string() } else { names.join(", ") }, project.name, label);
    let os = current_os();
    if os == "windows" {
        if let Some(wp) = crate::wsl::parse(dir).filter(|_| project.wsl) {
            let keys: Vec<&str> = env.iter().map(|(k, _)| k.as_str()).filter(|k| !matches!(*k, "PATH" | "Path")).collect();
            let wslenv = crate::wsl::wslenv(std::env::var("WSLENV").ok().as_deref(), &keys);
            env.retain(|(k, _)| !matches!(k.as_str(), "PATH" | "Path"));
            env.push(("WSLENV".into(), wslenv));
            let a = Attempt { new_console: true, ..attempt("wsl.exe", &["-d", &wp.distro, "--cd", &wp.path], false) };
            return Ok((vec![a], env));
        }
        let script = temp_script("ps1", &ps_script(&env, &banner))?;
        let f = script.display().to_string();
        let args = ["-NoExit", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", f.as_str()];
        let mut wt: Vec<&str> = vec!["-d", ".", "powershell"];
        wt.extend_from_slice(&args);
        return Ok((
            vec![attempt("wt", &wt, true), Attempt { new_console: true, ..attempt("powershell", &args, true) }],
            Vec::new(),
        ));
    }
    let script = temp_script(if os == "macos" { "command" } else { "sh" }, &sh_script(&env, &banner, dir))?;
    let f = script.display().to_string();
    let f = f.as_str();
    Ok((
        if os == "macos" {
            vec![attempt("open", &["-a", "Terminal", f], false)]
        } else {
            vec![
                attempt("x-terminal-emulator", &["-e", "sh", f], true),
                attempt("gnome-terminal", &["--", "sh", f], true),
                attempt("konsole", &["-e", "sh", f], true),
                attempt("xfce4-terminal", &["-x", "sh", f], true),
                attempt("xterm", &["-e", "sh", f], true),
            ]
        },
        Vec::new(),
    ))
}

#[tauri::command]
pub async fn project_open_in(
    app: tauri::AppHandle,
    target: String,
    path: String,
    project_id: Option<String>,
) -> Result<(), String> {
    let target = parse_target(&target)?;
    let dir = PathBuf::from(&path);
    if !dir.is_dir() {
        return Err(format!("Folder not found: {path}"));
    }
    let project = project_id.as_deref().and_then(|id| {
        use tauri::Manager;
        let state = app.try_state::<std::sync::Mutex<crate::commands::config::ConfigState>>()?;
        let cfg = state.lock().ok()?;
        cfg.projects.iter().find(|p| p.id == id).cloned()
    });
    let mut env: Vec<(String, String)> = Vec::new();
    let wsl = if cfg!(windows) { crate::wsl::parse(&path) } else { None };
    let attempts = match (&project, target) {
        (Some(p), Target::Terminal) => {
            let mut vars = crate::sites::process::terminal_env(p);
            if !(p.wsl && wsl.is_some()) && crate::mise::version().await.is_some() {
                if let Ok(m) = crate::mise::env_for(&dir).await {
                    let mut m: Vec<(String, String)> = m.into_iter().collect();
                    m.sort();
                    m.extend(vars);
                    vars = m;
                }
            }
            let (a, e) = terminal_plan(p, vars, &path)?;
            env = e;
            a
        }
        _ => wsl
            .as_ref()
            .and_then(|wp| plan_wsl(target, wp))
            .unwrap_or_else(|| plan(target, current_os(), &path)),
    };
    for a in &attempts {
        match spawn(a, &dir, &env) {
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
    fn terminal_scripts_quote_values() {
        let env = vec![("APP_ENV".to_string(), "it's local".to_string()), ("bad name".to_string(), "x".to_string())];
        let ps = ps_script(&env, "Kova: APP_ENV");
        assert!(ps.contains("SetEnvironmentVariable('APP_ENV', 'it''s local', 'Process')"));
        assert!(ps.contains("Remove-Item -LiteralPath $PSCommandPath"));
        let sh = sh_script(&env, "Kova: APP_ENV", "/home/dev/a b");
        assert!(sh.contains("export APP_ENV='it'\\''s local'"));
        assert!(!sh.contains("bad name"));
        assert!(sh.contains("cd -- '/home/dev/a b'"));
    }

    #[test]
    fn wsl_folders_open_inside_the_distro() {
        let wp = crate::wsl::parse(r"\\wsl.localhost\Ubuntu\home\dev\a&b").unwrap();
        let t = plan_wsl(Target::Terminal, &wp).unwrap();
        assert_eq!(t[0].args, ["-d", "Ubuntu", "--cd", "/home/dev/a&b"]);
        assert!(t[0].new_console);
        let v = plan_wsl(Target::VsCode, &wp).unwrap();
        assert_eq!(v[0].program, "code.cmd");
        assert_eq!(v[0].args, ["--remote", "wsl+Ubuntu", "/home/dev/a&b"]);
        assert!(plan_wsl(Target::Folder, &wp).is_none());
    }

    #[test]
    fn windows_terminal_falls_back_to_powershell() {
        let p = plan(Target::Terminal, "windows", r"C:\dev\x");
        assert_eq!(p[0].program, "wt");
        assert_eq!(p[1].program, "powershell");
        assert!(p[1].new_console && p[1].in_dir);
    }
}
