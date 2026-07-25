use std::process::Stdio;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Window};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

/// Build a `tokio::process::Command` pre-configured with:
/// - CREATE_NO_WINDOW flag on Windows (prevents terminal flicker)
/// - stdout / stderr set to `Stdio::null()` by default
///
/// Caller may override stdio (e.g. `.stdout(Stdio::piped())`) before spawning.
pub fn silent_command(program: &str) -> Command {
    let mut cmd = Command::new(program);
    cmd.stdout(Stdio::null()).stderr(Stdio::null());

    #[cfg(target_os = "windows")]
    {
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    // GUI-launched apps (AppImage / .app dari launcher) TIDAK mewarisi PATH shell
    // interaktif, jadi tool yang di-install via shell rc (fnm, phpvm → ~/.local/bin,
    // ~/.fnm, dll.) tak ketemu. Inject PATH hasil resolusi login-shell + dir umum.
    #[cfg(not(target_os = "windows"))]
    {
        cmd.env("PATH", resolved_path());
    }

    cmd
}

/// PATH yang sudah di-augment untuk spawn tool eksternal di Linux/macOS.
/// Di-resolve sekali (login-interactive shell → source .bashrc/.zshrc) lalu di-cache.
#[cfg(not(target_os = "windows"))]
pub fn resolved_path() -> &'static str {
    use std::sync::OnceLock;
    static CACHE: OnceLock<String> = OnceLock::new();
    CACHE.get_or_init(build_resolved_path)
}

/// Susun PATH: PATH login-shell (bila terbaca) ∪ PATH proses ∪ dir install umum.
/// Dedup jaga urutan; buang segmen kosong.
#[cfg(not(target_os = "windows"))]
fn build_resolved_path() -> String {
    use std::collections::HashSet;

    let mut parts: Vec<String> = Vec::new();

    if let Some(shell_path) = capture_login_shell_path() {
        parts.extend(shell_path.split(':').map(str::to_string));
    }
    if let Ok(current) = std::env::var("PATH") {
        parts.extend(current.split(':').map(str::to_string));
    }

    let home = std::env::var("HOME").unwrap_or_default();
    if !home.is_empty() {
        parts.push(format!("{home}/.local/bin"));
        parts.push(format!("{home}/.fnm"));
        parts.push(format!("{home}/.local/share/fnm"));
        parts.push(format!("{home}/.cargo/bin"));
    }
    parts.push("/usr/local/bin".to_string());
    parts.push("/opt/homebrew/bin".to_string());

    let mut seen = HashSet::new();
    parts
        .into_iter()
        .filter(|p| !p.is_empty() && seen.insert(p.clone()))
        .collect::<Vec<_>>()
        .join(":")
}

/// Jalankan login-interactive shell user untuk membaca PATH final-nya (setelah
/// .bashrc/.zshrc di-source). Timeout 5s + stdin null supaya tak pernah hang.
/// Return None kalau shell gagal / tak ada / kosong (caller fallback ke dir umum).
#[cfg(not(target_os = "windows"))]
fn capture_login_shell_path() -> Option<String> {
    use std::process::{Command as StdCommand, Stdio as StdStdio};
    use std::sync::mpsc;
    use std::time::Duration;

    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
    let (tx, rx) = mpsc::channel();

    std::thread::spawn(move || {
        let out = StdCommand::new(&shell)
            .args(["-lic", "printf '%s' \"$PATH\""])
            .stdin(StdStdio::null())
            .stderr(StdStdio::null())
            .output();
        let _ = tx.send(out);
    });

    match rx.recv_timeout(Duration::from_secs(5)) {
        Ok(Ok(out)) if out.status.success() => {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if s.is_empty() {
                None
            } else {
                Some(s)
            }
        }
        _ => None,
    }
}

/// Build a PowerShell command that executes `script_body` as a `-Command` string.
/// Flags: -NoProfile -NoLogo -NonInteractive -ExecutionPolicy Bypass.
/// Stdout/stderr default to null; caller overrides before spawning.
///
/// Use on Windows only — wrap phpvm.ps1 invocations to avoid
/// "CantActivateDocumentInPipeline" when Tauri spawns outside PowerShell context.
#[cfg(target_os = "windows")]
pub fn silent_powershell_command(script_body: &str) -> Command {
    let wrapped = format!(
        "$ErrorActionPreference='Stop'; $env:Path='C:\\Windows\\System32;C:\\Windows;' + $env:Path; {}",
        script_body
    );

    let mut cmd = Command::new("powershell.exe");
    cmd.args([
        "-NoProfile",
        "-NoLogo",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
        &wrapped,
    ])
    .stdout(Stdio::null())
    .stderr(Stdio::null());

    const CREATE_NO_WINDOW: u32 = 0x08000000;
    cmd.creation_flags(CREATE_NO_WINDOW);

    cmd
}

/// Emit a synthesized ENV-level log line to the frontend via `cmd-output`.
/// Use for success milestones: switch, init, ready.
/// `source: "ENV"` signals the frontend to render with the ENV tag style.
pub fn emit_env_line(window: &Window, message: &str) {
    let _ = window.emit(
        "cmd-output",
        serde_json::json!({
            "line": message,
            "stream": "stdout",
            "source": "ENV",
        }),
    );
}

/// Emit a log line dengan source tag arbitrary ke frontend via `cmd-output`.
/// Menerima `AppHandle` — dipakai oleh polling task untuk emit `[UP]`/`[DOWN]` log lines.
pub fn emit_log_line(app: &tauri::AppHandle, source: &str, message: &str) {
    let _ = app.emit(
        "cmd-output",
        serde_json::json!({
            "line": message,
            "stream": "stdout",
            "source": source,
        }),
    );
}

/// Extract the first semver-like token (`major.minor[.patch]`) from an arbitrary string.
///
/// Strips a leading `v` prefix before scanning. Returns `None` if no digit-dot-digit
/// sequence is found (e.g. warn messages, alias strings, empty input).
///
/// Examples:
///   "PHP 8.3.31 (cli) ..."        → Some("8.3.31")
///   "Active: 8.3 .31 PHP 8.3.31"  → Some("8.3")   ← first occurrence wins
///   "v20.14.0\n"                   → Some("20.14.0")
///   "[warn] No PHP version active" → None
pub fn extract_semver(input: &str) -> Option<String> {
    let s = input.trim().trim_start_matches('v');
    let chars: Vec<char> = s.chars().collect();
    let n = chars.len();
    let mut i = 0;

    while i < n {
        // Find start of a digit run
        if !chars[i].is_ascii_digit() {
            i += 1;
            continue;
        }

        // Collect major digits
        let start = i;
        while i < n && chars[i].is_ascii_digit() {
            i += 1;
        }

        // Must be followed by '.'
        if i >= n || chars[i] != '.' {
            continue;
        }
        i += 1; // consume '.'

        // Must be followed by at least one digit (minor)
        if i >= n || !chars[i].is_ascii_digit() {
            continue;
        }
        while i < n && chars[i].is_ascii_digit() {
            i += 1;
        }

        // Optional: '.patch'
        if i < n && chars[i] == '.' {
            let dot_pos = i;
            i += 1;
            if i < n && chars[i].is_ascii_digit() {
                while i < n && chars[i].is_ascii_digit() {
                    i += 1;
                }
            } else {
                // dot not followed by digit — rewind, don't include it
                i = dot_pos;
            }
        }

        return Some(chars[start..i].iter().collect());
    }

    None
}

const SERVICE_IDS: &[&str] = &[
    "mysql", "postgres", "redis", "rabbitmq", "mongodb",
    "minio", "mailpit", "gotenberg", "sqlserver",
];

/// Derive log source tag dari docker compose output line.
/// Scan `servel_<id>` substring — return `<ID>` uppercase. Fallback "SERVEL".
pub fn src_from_line(line: &str) -> String {
    for id in SERVICE_IDS {
        if line.contains(&format!("servel_{}", id)) {
            return id.to_uppercase();
        }
    }
    "SERVEL".to_string()
}

/// Parse accumulated stderr dari docker compose untuk pesan error friendly.
pub fn parse_docker_error(stderr_acc: &str) -> Option<String> {
    let lower = stderr_acc.to_lowercase();
    if lower.contains("port is already allocated") {
        if let Some(idx) = stderr_acc.find("Bind for 0.0.0.0:") {
            let after = &stderr_acc[idx + "Bind for 0.0.0.0:".len()..];
            let port: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
            if !port.is_empty() {
                return Some(format!(
                    "Port {} sudah dipakai aplikasi lain — close dulu sebelum start service ini",
                    port
                ));
            }
        }
        return Some("Port conflict — salah satu port service sudah dipakai aplikasi lain".to_string());
    }
    if lower.contains("network") && lower.contains("already exists") {
        return Some("Network conflict — restart Docker Desktop".to_string());
    }
    if lower.contains("cannot connect to the docker daemon") {
        return Some("Docker Desktop tidak running — start Docker dulu".to_string());
    }
    None
}

/// Stream stdout/stderr dari `cmd` ke frontend via `cmd-output` event, pakai `AppHandle`.
/// Source tag di-derive dari konten line via `src_from_line` (per-service tagging).
/// Tunggu sampai proses selesai; return Err kalau exit code != 0.
pub async fn stream_and_wait_app(mut cmd: Command, app: &AppHandle) -> Result<(), String> {
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| format!("Gagal spawn docker: {}", e))?;

    let stdout = child.stdout.take().ok_or("stdout tidak tersedia")?;
    let stderr = child.stderr.take().ok_or("stderr tidak tersedia")?;

    let stderr_buf: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    let app_out = app.clone();
    let stdout_task = tokio::spawn(async move {
        let mut reader = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            let source = src_from_line(&line);
            let _ = app_out.emit(
                "cmd-output",
                serde_json::json!({ "line": line, "stream": "stdout", "source": source }),
            );
        }
    });

    let app_err = app.clone();
    let stderr_buf_clone = Arc::clone(&stderr_buf);
    let stderr_task = tokio::spawn(async move {
        let mut reader = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            if let Ok(mut buf) = stderr_buf_clone.lock() {
                buf.push(line.clone());
            }
            let source = src_from_line(&line);
            let _ = app_err.emit(
                "cmd-output",
                serde_json::json!({ "line": line, "stream": "stderr", "source": source }),
            );
        }
    });

    let _ = tokio::join!(stdout_task, stderr_task);

    let status = child
        .wait()
        .await
        .map_err(|e| format!("Gagal menunggu docker: {}", e))?;

    if !status.success() {
        let acc = stderr_buf
            .lock()
            .map(|b| b.join("\n"))
            .unwrap_or_default();
        if let Some(friendly) = parse_docker_error(&acc) {
            return Err(friendly);
        }
        return Err(format!("docker compose gagal (exit {})", status));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_semver_plain() {
        assert_eq!(extract_semver("8.3.31"), Some("8.3.31".to_string()));
    }

    #[test]
    fn test_extract_semver_v_prefix() {
        assert_eq!(extract_semver("v8.3.31"), Some("8.3.31".to_string()));
    }

    #[test]
    fn test_extract_semver_php_verbose() {
        let s = "PHP 8.3.31 (cli) (built: May 5 2026 17:23:04) (NTS Visual C++ 2019 x64)";
        assert_eq!(extract_semver(s), Some("8.3.31".to_string()));
    }

    #[test]
    fn test_extract_semver_active_prefix() {
        assert_eq!(extract_semver("Active: 8.3.31"), Some("8.3.31".to_string()));
    }

    #[test]
    fn test_extract_semver_warn_message() {
        assert_eq!(
            extract_semver("[warn] No PHP version active. Run: phpvm use <version>"),
            None
        );
    }

    #[test]
    fn test_extract_semver_empty() {
        assert_eq!(extract_semver(""), None);
    }

    #[test]
    fn test_extract_semver_major_minor_only() {
        assert_eq!(extract_semver("8.3"), Some("8.3".to_string()));
    }

    #[test]
    fn test_extract_semver_node_version() {
        assert_eq!(extract_semver("v20.14.0\n"), Some("20.14.0".to_string()));
        assert_eq!(extract_semver("20.14.0"), Some("20.14.0".to_string()));
    }
}

