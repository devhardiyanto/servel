use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Window};

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PhpVersion {
    pub version: String,
    pub active: bool,
}

/// Emit a single line of process output to the frontend via "cmd-output" event.
fn emit_line(window: &Window, line: &str, stream: &str) {
    let _ = window.emit(
        "cmd-output",
        serde_json::json!({ "line": line, "stream": stream }),
    );
}

/// Run `phpvm list` and parse output into a list of installed PHP versions.
/// Output format per line (from phpvm):
///   "  8.3.0"    → inactive
///   "* 8.2.21"   → active (leading asterisk)
/// Lines that are blank or do not contain a recognisable version are skipped.
#[tauri::command]
pub async fn php_list_installed() -> Result<Vec<PhpVersion>, String> {
    let output = build_phpvm_command()
        .args(["list"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| format!("phpvm tidak ditemukan di PATH: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("phpvm list gagal: {}", stderr.trim()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let versions = parse_phpvm_list(&stdout);
    Ok(versions)
}

/// Return the currently active PHP version, or `None` if none is active.
/// Strategy: run `phpvm current` — faster than parsing `phpvm list` and
/// returns exactly the active version string (or empty/error if none active).
#[tauri::command]
pub async fn php_get_active() -> Result<Option<String>, String> {
    let output = build_phpvm_command()
        .args(["current"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| format!("phpvm tidak ditemukan di PATH: {}", e))?;

    if !output.status.success() {
        // phpvm current may exit non-zero when no version is active — treat as None.
        return Ok(None);
    }

    let raw = String::from_utf8_lossy(&output.stdout);
    let version = raw.trim().trim_start_matches('v').to_string();
    if version.is_empty() {
        Ok(None)
    } else {
        Ok(Some(version))
    }
}

/// Run `phpvm use <version>` and stream all stdout/stderr to the frontend
/// via the `cmd-output` event.
#[tauri::command]
pub async fn php_switch(window: Window, version: String) -> Result<(), String> {
    let mut child = build_phpvm_command()
        .args(["use", &version])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("phpvm tidak ditemukan di PATH: {}", e))?;

    let stdout = child.stdout.take().ok_or("stdout tidak tersedia")?;
    let stderr = child.stderr.take().ok_or("stderr tidak tersedia")?;

    let window_out = window.clone();
    let stdout_task = tokio::spawn(async move {
        let mut reader = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            emit_line(&window_out, &line, "stdout");
        }
    });

    let window_err = window.clone();
    let stderr_task = tokio::spawn(async move {
        let mut reader = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            emit_line(&window_err, &line, "stderr");
        }
    });

    let _ = tokio::join!(stdout_task, stderr_task);

    let status = child
        .wait()
        .await
        .map_err(|e| format!("Gagal menunggu phpvm: {}", e))?;

    if !status.success() {
        return Err(format!("phpvm use {} gagal (exit {})", version, status));
    }

    Ok(())
}

/// Run `phpvm install <version>` and stream output to the frontend.
/// `install-progress` event is NOT emitted — phpvm install output does not
/// produce a consistent percentage/progress pattern that can be reliably parsed.
/// Frontend should display indeterminate progress for the duration of this command.
#[tauri::command]
pub async fn php_install(window: Window, version: String) -> Result<(), String> {
    let mut child = build_phpvm_command()
        .args(["install", &version])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("phpvm tidak ditemukan di PATH: {}", e))?;

    let stdout = child.stdout.take().ok_or("stdout tidak tersedia")?;
    let stderr = child.stderr.take().ok_or("stderr tidak tersedia")?;

    let window_out = window.clone();
    let stdout_task = tokio::spawn(async move {
        let mut reader = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            emit_line(&window_out, &line, "stdout");
        }
    });

    let window_err = window.clone();
    let stderr_task = tokio::spawn(async move {
        let mut reader = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            emit_line(&window_err, &line, "stderr");
        }
    });

    let _ = tokio::join!(stdout_task, stderr_task);

    let status = child
        .wait()
        .await
        .map_err(|e| format!("Gagal menunggu phpvm: {}", e))?;

    if !status.success() {
        return Err(format!("phpvm install {} gagal (exit {})", version, status));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Build a `tokio::process::Command` for `phpvm`, handling the Windows quirk
/// where phpvm is a `.cmd` script requiring `cmd /c` wrapping.
fn build_phpvm_command() -> tokio::process::Command {
    #[cfg(target_os = "windows")]
    {
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        let mut cmd = tokio::process::Command::new("cmd");
        cmd.args(["/c", "phpvm"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW);
        cmd
    }

    #[cfg(not(target_os = "windows"))]
    {
        silent_command("phpvm")
    }
}

/// Parse `phpvm list` output into a `Vec<PhpVersion>`.
/// Each line is either `"  <version>"` (inactive) or `"* <version>"` (active).
/// Leading/trailing whitespace and optional `v` prefix are stripped.
fn parse_phpvm_list(output: &str) -> Vec<PhpVersion> {
    output
        .lines()
        .filter_map(|raw_line| {
            let line = raw_line.trim();
            if line.is_empty() {
                return None;
            }

            let (active, rest) = if let Some(stripped) = line.strip_prefix('*') {
                (true, stripped.trim())
            } else {
                (false, line)
            };

            // Strip optional leading `v` (e.g. "v8.3.0" → "8.3.0")
            let version = rest.trim_start_matches('v').trim().to_string();

            // Skip lines that don't look like a version (e.g. header text)
            if version.is_empty() || !version.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
                return None;
            }

            Some(PhpVersion { version, active })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_phpvm_list_basic() {
        let output = "  8.3.0\n* 8.2.21\n  8.1.29\n";
        let versions = parse_phpvm_list(output);
        assert_eq!(versions.len(), 3);
        assert_eq!(versions[0].version, "8.3.0");
        assert!(!versions[0].active);
        assert_eq!(versions[1].version, "8.2.21");
        assert!(versions[1].active);
        assert_eq!(versions[2].version, "8.1.29");
        assert!(!versions[2].active);
    }

    #[test]
    fn test_parse_phpvm_list_v_prefix() {
        let output = "  v8.3.0\n* v8.2.21\n";
        let versions = parse_phpvm_list(output);
        assert_eq!(versions[0].version, "8.3.0");
        assert_eq!(versions[1].version, "8.2.21");
        assert!(versions[1].active);
    }

    #[test]
    fn test_parse_phpvm_list_empty_lines() {
        let output = "\n  8.3.0\n\n* 8.2.21\n\n";
        let versions = parse_phpvm_list(output);
        assert_eq!(versions.len(), 2);
    }

    #[test]
    fn test_parse_phpvm_list_header_skipped() {
        let output = "Installed PHP versions:\n  8.3.0\n* 8.2.21\n";
        let versions = parse_phpvm_list(output);
        // "Installed PHP versions:" starts with 'I', not digit — skipped
        assert_eq!(versions.len(), 2);
    }
}
