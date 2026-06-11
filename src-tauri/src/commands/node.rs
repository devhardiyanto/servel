use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use serde::{Deserialize, Serialize};
use tauri::Window;

use super::util::silent_command;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct NodeVersion {
    pub version: String,
    pub active: bool,
}

fn emit_line(window: &Window, line: &str, stream: &str) {
    let _ = window.emit(
        "cmd-output",
        serde_json::json!({ "line": line, "stream": stream }),
    );
}

/// Run `fnm list` and parse output into a list of installed Node versions.
/// Output format per line (from fnm):
///   "* v20.14.0 default (system)"  → active, version = "20.14.0"
///   "  v18.20.3"                   → inactive
/// Alias tokens after the version (e.g. "default", "system", "lts/iron") are ignored.
/// Lines without a recognisable semver are skipped.
#[tauri::command]
pub async fn node_list_installed() -> Result<Vec<NodeVersion>, String> {
    let output = build_fnm_command()
        .args(["list"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| format!("fnm tidak ditemukan di PATH: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("fnm list gagal: {}", stderr.trim()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let versions = parse_fnm_list(&stdout);
    Ok(versions)
}

/// Return the currently active Node version, or `None` if none is active.
/// Uses `fnm current`. Output `system` (no semver) or empty → returns `None`.
/// Prefix `v` is stripped before returning (e.g. `v20.14.0` → `"20.14.0"`).
#[tauri::command]
pub async fn node_get_active() -> Result<Option<String>, String> {
    let output = build_fnm_command()
        .args(["current"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| format!("fnm tidak ditemukan di PATH: {}", e))?;

    if !output.status.success() {
        return Ok(None);
    }

    let raw = String::from_utf8_lossy(&output.stdout);
    let trimmed = raw.trim().trim_start_matches('v');
    if trimmed.is_empty() || !trimmed.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
        return Ok(None);
    }

    Ok(Some(trimmed.to_string()))
}

/// Run `fnm use <version>` and stream all stdout/stderr to the frontend
/// via the `cmd-output` event.
#[tauri::command]
pub async fn node_switch(window: Window, version: String) -> Result<(), String> {
    let mut child = build_fnm_command()
        .args(["use", &version])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("fnm tidak ditemukan di PATH: {}", e))?;

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
        .map_err(|e| format!("Gagal menunggu fnm: {}", e))?;

    if !status.success() {
        return Err(format!("fnm use {} gagal (exit {})", version, status));
    }

    Ok(())
}

/// Run `fnm install <version>` and stream output to the frontend.
/// `install-progress` event is NOT emitted — fnm install output does not
/// produce a consistent percentage/progress pattern that can be reliably parsed.
/// Frontend should display indeterminate progress for the duration of this command.
#[tauri::command]
pub async fn node_install(window: Window, version: String) -> Result<(), String> {
    let mut child = build_fnm_command()
        .args(["install", &version])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("fnm tidak ditemukan di PATH: {}", e))?;

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
        .map_err(|e| format!("Gagal menunggu fnm: {}", e))?;

    if !status.success() {
        return Err(format!("fnm install {} gagal (exit {})", version, status));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Build a `tokio::process::Command` for `fnm`.
/// fnm is a standalone binary on all platforms — no `cmd /c` wrapping needed.
fn build_fnm_command() -> tokio::process::Command {
    silent_command("fnm")
}

/// Parse `fnm list` output into a `Vec<NodeVersion>`.
///
/// Input line formats:
///   `* v20.14.0 default (system)`  → active, version "20.14.0"
///   `  v18.20.3`                   → inactive, version "18.20.3"
///
/// Rules:
/// - Leading asterisk `*` = active.
/// - First token after trimming marker = version token (must start with digit or `v`+digit).
/// - Strip prefix `v` from version token.
/// - Remaining alias tokens (default, system, lts/iron, etc.) are discarded.
/// - Lines where the first non-marker token has no semver (e.g. alias-only) are skipped.
fn parse_fnm_list(output: &str) -> Vec<NodeVersion> {
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

            // First whitespace-separated token is the version (e.g. "v20.14.0")
            let version_token = rest.split_whitespace().next().unwrap_or("");
            let version = version_token.trim_start_matches('v');

            // Must start with a digit to be a valid semver-like version
            if version.is_empty() || !version.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
                return None;
            }

            Some(NodeVersion {
                version: version.to_string(),
                active,
            })
        })
        .collect()
}

/// Parse output of `fnm current`.
/// Returns Some(version) for semver-like output, None for "system", empty, or alias-only.
fn parse_current_output(raw: &str) -> Option<String> {
    let trimmed = raw.trim().trim_start_matches('v');
    if trimmed.is_empty() || !trimmed.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_fnm_list_basic() {
        let output = "* v20.14.0 default (system)\n  v18.20.3\n  v22.3.0\n";
        let versions = parse_fnm_list(output);
        assert_eq!(versions.len(), 3);
        assert_eq!(versions[0].version, "20.14.0");
        assert!(versions[0].active);
        assert_eq!(versions[1].version, "18.20.3");
        assert!(!versions[1].active);
        assert_eq!(versions[2].version, "22.3.0");
        assert!(!versions[2].active);
    }

    #[test]
    fn test_parse_fnm_list_strips_v_prefix() {
        let output = "  v18.20.3\n* v20.14.0\n";
        let versions = parse_fnm_list(output);
        assert_eq!(versions[0].version, "18.20.3");
        assert_eq!(versions[1].version, "20.14.0");
        assert!(versions[1].active);
    }

    #[test]
    fn test_parse_fnm_list_alias_tokens_ignored() {
        // Aliases after version token must not create separate entries
        let output = "* v20.14.0 default (system)\n  v18.20.3 lts/hydrogen\n";
        let versions = parse_fnm_list(output);
        assert_eq!(versions.len(), 2);
        assert_eq!(versions[0].version, "20.14.0");
        assert_eq!(versions[1].version, "18.20.3");
    }

    #[test]
    fn test_parse_fnm_list_empty_lines_skipped() {
        let output = "\n  v18.20.3\n\n* v20.14.0\n\n";
        let versions = parse_fnm_list(output);
        assert_eq!(versions.len(), 2);
    }

    #[test]
    fn test_parse_fnm_list_alias_only_skipped() {
        // A line with active marker but only an alias (no semver) must be skipped
        let output = "* system\n  v18.20.3\n";
        let versions = parse_fnm_list(output);
        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].version, "18.20.3");
    }

    #[test]
    fn test_parse_current_semver() {
        assert_eq!(parse_current_output("v20.14.0\n"), Some("20.14.0".to_string()));
        assert_eq!(parse_current_output("20.14.0"), Some("20.14.0".to_string()));
    }

    #[test]
    fn test_parse_current_system_returns_none() {
        assert_eq!(parse_current_output("system"), None);
        assert_eq!(parse_current_output("system\n"), None);
    }

    #[test]
    fn test_parse_current_empty_returns_none() {
        assert_eq!(parse_current_output(""), None);
        assert_eq!(parse_current_output("\n"), None);
    }

    #[test]
    fn test_parse_current_alias_only_returns_none() {
        assert_eq!(parse_current_output("lts/iron"), None);
        assert_eq!(parse_current_output("default"), None);
    }
}
