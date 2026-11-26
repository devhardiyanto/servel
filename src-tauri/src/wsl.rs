//! Project di dalam distro WSL (Windows).
//!
//! Folder `\\wsl.localhost\<distro>\home\...` (atau `\\wsl$\...`) dibuka
//! Windows lewat 9P. Tool Windows yang dijalankan di sana lambat dan tidak
//! cocok dengan `vendor`/`node_modules` yang dipasang di Linux, jadi project
//! yang ditandai "Run inside WSL" dijalankan di distro itu sendiri:
//!
//! - Start: `wsl.exe -d <distro> --cd <path> --exec setsid -w bash -lc <script>`.
//!   `setsid` memberi proses grup sendiri; baris pertama output adalah
//!   penanda PGID supaya Stop bisa mematikan seluruh pohon proses di distro.
//! - ENV: WSL hanya meneruskan variabel yang namanya ada di `WSLENV`.
//! - Versi tool: `mise ls --current --json` di distro, atau versi di PATH.

use serde::Serialize;

use crate::commands::util::silent_command;
use crate::mise::{parse_current, SourceScope, ToolCurrent};

/// Penanda baris pertama output: `__KOVA_PGID__<pid>`.
pub const PGID_MARK: &str = "__KOVA_PGID__";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WslPath {
    pub distro: String,
    /// Path Linux, mis. `/home/dev/app`.
    pub path: String,
}

/// `\\wsl.localhost\Ubuntu-24.04\home\dev\app` → (`Ubuntu-24.04`, `/home/dev/app`).
/// Garis miring biasa juga diterima. Nama distro dibatasi ke karakter yang
/// dipakai WSL, supaya tidak bisa membawa argumen lain.
pub fn parse(path: &str) -> Option<WslPath> {
    let p = path.replace('/', "\\");
    let lower = p.to_ascii_lowercase();
    let rest = ["\\\\wsl.localhost\\", "\\\\wsl$\\"]
        .iter()
        .find(|pre| lower.starts_with(*pre))
        .map(|pre| &p[pre.len()..])?;
    let mut parts = rest.split('\\').filter(|s| !s.is_empty());
    let distro = parts.next()?.to_string();
    if !distro.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_')) {
        return None;
    }
    let segs: Vec<&str> = parts.collect();
    Some(WslPath { distro, path: format!("/{}", segs.join("/")) })
}

/// Script yang dijalankan `bash -lc`. `bash -l` membaca `~/.profile`
/// (PATH Go, `~/.local/bin`); mise diaktifkan bila ada, karena aktivasinya
/// biasanya hanya di `~/.bashrc` yang tidak dibaca shell non-interaktif.
pub fn start_script(command: &str) -> String {
    format!(
        "echo {mark}$$; \
         if command -v mise >/dev/null 2>&1; then eval \"$(mise env -s bash 2>/dev/null)\"; fi; \
         {command}",
        mark = PGID_MARK,
        command = command
    )
}

/// Argumen `wsl.exe` untuk menjalankan `command` di folder project.
pub fn start_args(wp: &WslPath, command: &str) -> Vec<String> {
    vec![
        "-d".into(),
        wp.distro.clone(),
        "--cd".into(),
        wp.path.clone(),
        "--exec".into(),
        "setsid".into(),
        // -w: bila setsid harus fork (prosesnya sudah ketua grup), tetap tunggu
        // anaknya, supaya wsl.exe tidak langsung selesai.
        "-w".into(),
        "bash".into(),
        "-lc".into(),
        start_script(command),
    ]
}

/// `WSLENV` yang meneruskan `names` ke distro, digabung dengan nilai yang ada.
/// `/u` = hanya dari Windows ke Linux.
pub fn wslenv(existing: Option<&str>, names: &[&str]) -> String {
    let mut parts: Vec<String> = existing
        .unwrap_or("")
        .split(':')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    for n in names {
        let taken = parts.iter().any(|p| p.split('/').next() == Some(*n));
        if !taken {
            parts.push(format!("{}/u", n));
        }
    }
    parts.join(":")
}

/// PGID dari baris penanda.
pub fn parse_pgid(line: &str) -> Option<u32> {
    line.trim().strip_prefix(PGID_MARK)?.parse().ok().filter(|p| *p > 1)
}

/// Kirim sinyal ke seluruh grup proses di distro.
pub async fn kill_group(distro: &str, pgid: u32, signal: &str) {
    // wsl.exe yang macet tidak boleh menahan Stop atau keluar dari Kova.
    let fut = silent_command("wsl.exe")
        .args(["-d", distro, "--exec", "kill", &format!("-{}", signal), "--", &format!("-{}", pgid)])
        .kill_on_drop(true)
        .output();
    let _ = tokio::time::timeout(std::time::Duration::from_secs(15), fut).await;
}

/// Script versi tool: baris pertama `$HOME`, lalu JSON mise (atau kosong),
/// lalu baris `@<tool> <versi>` dari PATH untuk tool tanpa mise.
const TOOLS_SCRIPT: &str = r#"printf '%s\n' "$HOME"
if command -v mise >/dev/null 2>&1; then mise ls --current --json 2>/dev/null | tr -d '\n'; fi
printf '\n'
command -v go >/dev/null 2>&1 && go env GOVERSION 2>/dev/null | sed 's/^go/@go /'
command -v node >/dev/null 2>&1 && node -v 2>/dev/null | sed 's/^v/@node /'
command -v php >/dev/null 2>&1 && php -r 'echo "@php ".PHP_VERSION."\n";' 2>/dev/null
command -v python3 >/dev/null 2>&1 && python3 -c 'import platform;print("@python "+platform.python_version())' 2>/dev/null
true"#;

/// Parse keluaran `TOOLS_SCRIPT`. Versi dari mise didahulukan; tool yang
/// tidak dikelola mise memakai versi di PATH distro.
pub fn parse_tools(out: &str, distro: &str) -> Vec<ToolCurrent> {
    let mut lines = out.lines();
    let home = lines.next().unwrap_or("").trim().to_string();
    let json = lines.next().unwrap_or("").trim().to_string();
    let roots = if home.is_empty() {
        Vec::new()
    } else {
        vec![format!("{}/.config/mise", home), format!("{}/.tool-versions", home)]
    };
    let mut tools = if json.is_empty() { Vec::new() } else { parse_current(&json, &roots) };
    for l in lines {
        let Some((tool, version)) = l.trim().strip_prefix('@').and_then(|r| r.split_once(' ')) else {
            continue;
        };
        if tools.iter().any(|t| t.tool == tool) || version.trim().is_empty() {
            continue;
        }
        tools.push(ToolCurrent {
            tool: tool.to_string(),
            version: version.trim().to_string(),
            requested: None,
            source: Some(format!("PATH in {}", distro)),
            scope: SourceScope::None,
            installed: true,
        });
    }
    tools.sort_by(|a, b| a.tool.cmp(&b.tool));
    tools
}

/// Versi tool yang berlaku di folder project di dalam distro.
#[tauri::command]
pub async fn wsl_project_tools(path: String) -> Result<Vec<ToolCurrent>, String> {
    let wp = parse(&path).ok_or_else(|| format!("not a WSL folder: {}", path))?;
    tools_in(&wp.distro, &wp.path).await
}

/// Versi tool di home distro (Runtimes › WSL tanpa mise: "Found on PATH").
#[tauri::command]
pub async fn wsl_path_tools(distro: String) -> Result<Vec<ToolCurrent>, String> {
    let crate::mise::Host::Wsl(d) = crate::mise::host_of(&Some(distro))? else { return Err("no distro".into()) };
    tools_in(&d, "~").await
}

/// Folder php.ini yang dibaca `php` di distro (Linux path), dari `php --ini`.
#[tauri::command]
pub async fn wsl_php_ini_dir(distro: String) -> Result<String, String> {
    let crate::mise::Host::Wsl(d) = crate::mise::host_of(&Some(distro))? else { return Err("no distro".into()) };
    let out = silent_command("wsl.exe")
        .args(["-d", &d, "--cd", "~", "--exec", "bash", "-lc", "php --ini"])
        .env("WSL_UTF8", "1")
        .kill_on_drop(true)
        .output();
    let out = tokio::time::timeout(std::time::Duration::from_secs(30), out)
        .await
        .map_err(|_| format!("{d} did not answer within 30 s"))?
        .map_err(|e| format!("could not run wsl.exe: {e}"))?;
    parse_php_ini_dir(&String::from_utf8_lossy(&out.stdout)).ok_or_else(|| format!("php was not found in {d}, or it reads no php.ini"))
}

/// Folder dari `Loaded Configuration File:`, atau `Configuration File (php.ini) Path:`.
pub fn parse_php_ini_dir(out: &str) -> Option<String> {
    let value = |prefix: &str| {
        out.lines().find_map(|l| l.trim().strip_prefix(prefix).map(|v| v.trim().to_string())).filter(|v| v.starts_with('/'))
    };
    if let Some(file) = value("Loaded Configuration File:") {
        return file.rsplit_once('/').map(|(dir, _)| if dir.is_empty() { "/".to_string() } else { dir.to_string() });
    }
    value("Configuration File (php.ini) Path:")
}

async fn tools_in(distro: &str, cd: &str) -> Result<Vec<ToolCurrent>, String> {
    let out = silent_command("wsl.exe")
        .args(["-d", distro, "--cd", cd, "--exec", "bash", "-lc", TOOLS_SCRIPT])
        .kill_on_drop(true)
        .output();
    let out = tokio::time::timeout(std::time::Duration::from_secs(30), out)
        .await
        .map_err(|_| format!("{} did not answer within 30 s", distro))?
        .map_err(|e| format!("could not run wsl.exe: {}", e))?;
    if !out.status.success() && out.stdout.is_empty() {
        return Err(format!("{} did not answer", distro));
    }
    Ok(parse_tools(&String::from_utf8_lossy(&out.stdout), distro))
}

/// Distro dan path Linux dari sebuah folder, untuk tampilan.
#[tauri::command]
pub fn wsl_path_info(path: String) -> Option<WslPath> {
    parse(&path)
}

#[cfg(test)]
mod php_ini_tests {
    #[test]
    fn php_ini_dir_from_php_ini_output() {
        let out = "Configuration File (php.ini) Path: /etc/php/8.3/cli\nLoaded Configuration File:         /etc/php/8.3/cli/php.ini\nScan for additional .ini files in: /etc/php/8.3/cli/conf.d\n";
        assert_eq!(super::parse_php_ini_dir(out).as_deref(), Some("/etc/php/8.3/cli"));
        let none = "Configuration File (php.ini) Path: /home/dev/.local/share/mise/installs/php/8.4.14/lib\nLoaded Configuration File:         (none)\n";
        assert_eq!(super::parse_php_ini_dir(none).as_deref(), Some("/home/dev/.local/share/mise/installs/php/8.4.14/lib"));
        assert_eq!(super::parse_php_ini_dir("bash: php: command not found"), None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_wsl_folders() {
        let w = parse(r"\\wsl.localhost\Ubuntu-24.04\home\dev\work\app").unwrap();
        assert_eq!(w.distro, "Ubuntu-24.04");
        assert_eq!(w.path, "/home/dev/work/app");
        assert_eq!(parse(r"\\WSL$\Debian\srv\x\").unwrap().path, "/srv/x");
        assert_eq!(parse("//wsl.localhost/Ubuntu/").unwrap().path, "/");
        assert!(parse(r"D:\dev\app").is_none());
        assert!(parse(r"\\server\share\app").is_none());
        assert!(parse(r"\\wsl.localhost\bad name\x").is_none());
    }

    #[test]
    fn start_args_keep_the_command_as_one_argument() {
        let w = WslPath { distro: "Ubuntu".into(), path: "/home/dev/a b".into() };
        let a = start_args(&w, "go run . & echo x");
        assert_eq!(&a[..6], ["-d", "Ubuntu", "--cd", "/home/dev/a b", "--exec", "setsid"]);
        assert_eq!(a[6], "-w");
        assert!(a[9].starts_with("echo __KOVA_PGID__$$;"));
        assert!(a[9].ends_with("go run . & echo x"));
        assert_eq!(a.len(), 10);
    }

    #[test]
    fn wslenv_adds_names_once() {
        assert_eq!(wslenv(None, &["PORT", "APP_ENV"]), "PORT/u:APP_ENV/u");
        assert_eq!(wslenv(Some("USERPROFILE/p:PORT"), &["PORT", "X"]), "USERPROFILE/p:PORT:X/u");
    }

    #[test]
    fn pgid_marker() {
        assert_eq!(parse_pgid("__KOVA_PGID__1234"), Some(1234));
        assert_eq!(parse_pgid("__KOVA_PGID__1"), None);
        assert_eq!(parse_pgid("listening on :8080"), None);
    }

    #[test]
    fn tools_prefer_mise_then_path() {
        let out = "/home/dev\n{\"go\":[{\"version\":\"1.23.4\",\"active\":true,\"installed\":true,\"source\":{\"path\":\"/home/dev/app/mise.toml\"}}]}\n@go 1.22.0\n@node 22.11.0\n";
        let t = parse_tools(out, "Ubuntu");
        assert_eq!(t.len(), 2);
        assert_eq!((t[0].tool.as_str(), t[0].version.as_str()), ("go", "1.23.4"));
        assert_eq!(t[1].source.as_deref(), Some("PATH in Ubuntu"));
        let none = parse_tools("/home/dev\n\n@python 3.12.3\n", "Ubuntu");
        assert_eq!(none[0].tool, "python");
    }
}
