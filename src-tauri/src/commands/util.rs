use std::process::Stdio;
use tauri::{Emitter, Window};
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

    cmd
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

