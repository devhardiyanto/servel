use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Window};

use super::util::{emit_env_line, extract_semver};

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

/// Validate that `version` is a safe semver string (`major.minor[.patch]`).
/// Rejects anything that could inject shell metacharacters.
fn validate_version_string(version: &str) -> Result<(), String> {
    let valid = version
        .chars()
        .all(|c| c.is_ascii_digit() || c == '.');
    if !valid || version.is_empty() {
        return Err(format!("version '{}' tidak valid — hanya digit dan titik yang diizinkan", version));
    }
    // Must match major.minor pattern at minimum
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() < 2 || parts.len() > 3 {
        return Err(format!("version '{}' tidak valid — format harus major.minor[.patch]", version));
    }
    for part in &parts {
        if part.is_empty() || part.parse::<u32>().is_err() {
            return Err(format!("version '{}' tidak valid — setiap segmen harus angka", version));
        }
    }
    Ok(())
}

/// Run `phpvm list` and parse output into a list of installed PHP versions.
/// Output format per line (from phpvm):
///   "  8.3.0"    → inactive
///   "* 8.2.21"   → active (leading asterisk)
/// Lines that are blank or do not contain a recognisable version are skipped.
#[tauri::command]
pub async fn php_list_installed() -> Result<Vec<PhpVersion>, String> {
    let output = build_phpvm_command("list")
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
    let output = build_phpvm_command("current")
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
    Ok(extract_semver(&raw))
}

/// Run `phpvm use <version>` and stream all stdout/stderr to the frontend
/// via the `cmd-output` event.
#[tauri::command]
pub async fn php_switch(window: Window, version: String) -> Result<(), String> {
    validate_version_string(&version)?;

    let mut child = build_phpvm_command(&format!("use {}", version))
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

    emit_env_line(&window, &format!("switched php → {}", version));
    Ok(())
}

/// Run `phpvm install <version>` and stream output to the frontend.
/// `install-progress` event is NOT emitted — phpvm install output does not
/// produce a consistent percentage/progress pattern that can be reliably parsed.
/// Frontend should display indeterminate progress for the duration of this command.
#[tauri::command]
pub async fn php_install(window: Window, version: String) -> Result<(), String> {
    validate_version_string(&version)?;

    let mut child = build_phpvm_command(&format!("install {}", version))
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

/// Probe `phpvm hook status` and classify the result.
/// Returns `"installed"`, `"not_installed"`, or `"unknown"` (probe failed
/// or output not recognisable). Never errors — the UI uses this purely as a hint.
#[tauri::command]
pub async fn php_hook_status() -> Result<String, String> {
    let output = build_phpvm_command("hook status")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await;

    let Ok(output) = output else {
        return Ok("unknown".to_string());
    };

    let raw = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    // Strip ANSI escape sequences (ESC [ ... letter)
    let mut cleaned = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for cc in chars.by_ref() {
                    if cc.is_ascii_alphabetic() {
                        break;
                    }
                }
                continue;
            }
        }
        cleaned.push(c);
    }
    let combined = cleaned.to_lowercase();

    // Order matters — check negative patterns first.
    let negative_keywords = [
        "hook not installed",
        "not installed",
        "no $profile",
        "no profile",
    ];
    for kw in negative_keywords {
        if combined.contains(kw) {
            return Ok("not_installed".to_string());
        }
    }
    let positive_keywords = [
        "hook installed",
        "hook is installed",
        "hook: installed",
        "powershell hook: installed",
    ];
    for kw in positive_keywords {
        if combined.contains(kw) {
            return Ok("installed".to_string());
        }
    }
    // Bare "installed" fallback — risky karena bisa salah, jadi terakhir.
    if combined.contains("installed") && !combined.contains("not") {
        return Ok("installed".to_string());
    }
    Ok("unknown".to_string())
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Build a `tokio::process::Command` for `phpvm <subcommand>`.
///
/// On Windows, phpvm resolves to `phpvm.ps1` via PATHEXT. Invoking it directly
/// from a non-PowerShell context (like Tauri) causes:
///   "Cannot run a document in the middle of a pipeline"
/// Fix: wrap via `powershell.exe -Command "phpvm <subcommand>"`.
///
/// On non-Windows, invoke `phpvm` directly.
fn build_phpvm_command(subcommand: &str) -> tokio::process::Command {
    #[cfg(target_os = "windows")]
    {
        super::util::silent_powershell_command(&format!("phpvm {}", subcommand))
    }

    #[cfg(not(target_os = "windows"))]
    {
        use super::util::silent_command;
        let mut cmd = silent_command("phpvm");
        for arg in subcommand.split_whitespace() {
            cmd.arg(arg);
        }
        cmd
    }
}

/// Parse `phpvm list` output into a `Vec<PhpVersion>`.
/// Recognised active markers (prefix): `*`, `->`, `=>`.
/// Recognised active suffix: ` (active)` (case-insensitive).
/// Leading/trailing whitespace and optional `v` prefix are stripped.
/// Example real-world Windows output:
///   "Installed versions:"
///   "    7.4.33"
///   " -> 8.3.31 (active)"
///   "    8.5.6"
fn parse_phpvm_list(output: &str) -> Vec<PhpVersion> {
    output
        .lines()
        .filter_map(|raw_line| {
            let line = raw_line.trim();
            if line.is_empty() {
                return None;
            }

            // Detect & strip active marker prefix.
            let (mut active, rest_after_prefix) = if let Some(s) = line.strip_prefix("->") {
                (true, s.trim_start())
            } else if let Some(s) = line.strip_prefix("=>") {
                (true, s.trim_start())
            } else if let Some(s) = line.strip_prefix('*') {
                (true, s.trim_start())
            } else {
                (false, line)
            };

            // Detect & strip active suffix " (active)" (case-insensitive).
            let rest_trimmed = rest_after_prefix.trim_end();
            let rest_no_suffix = {
                let lower = rest_trimmed.to_ascii_lowercase();
                if let Some(idx) = lower.rfind("(active)") {
                    // Ensure preceding char (if any) is whitespace so we don't
                    // accidentally slice into a version segment.
                    let before = &rest_trimmed[..idx];
                    if before.is_empty() || before.ends_with(char::is_whitespace) {
                        active = true;
                        before.trim_end()
                    } else {
                        rest_trimmed
                    }
                } else {
                    rest_trimmed
                }
            };

            // Strip optional leading `v` (e.g. "v8.3.0" → "8.3.0").
            let version = rest_no_suffix.trim_start_matches('v').trim().to_string();

            // Skip lines that don't look like a version (e.g. header text).
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
    fn test_validate_version_valid() {
        assert!(validate_version_string("8.3.31").is_ok());
        assert!(validate_version_string("8.3").is_ok());
        assert!(validate_version_string("7.4.33").is_ok());
    }

    #[test]
    fn test_validate_version_invalid_chars() {
        assert!(validate_version_string("8.3.31; rm -rf /").is_err());
        assert!(validate_version_string("8.3$(evil)").is_err());
        assert!(validate_version_string("lts/iron").is_err());
        assert!(validate_version_string("").is_err());
    }

    #[test]
    fn test_validate_version_bad_format() {
        assert!(validate_version_string("8").is_err());
        assert!(validate_version_string("8.3.31.1").is_err());
        assert!(validate_version_string("8.").is_err());
        assert!(validate_version_string(".3.31").is_err());
    }

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

    #[test]
    fn test_parse_phpvm_list_arrow_marker() {
        // Real-world phpvm (Windows) output uses "->" marker plus "(active)" suffix.
        let output = "Installed versions:\n    7.4.33\n -> 8.3.31 (active)\n    8.5.6\n";
        let versions = parse_phpvm_list(output);
        assert_eq!(versions.len(), 3);
        assert_eq!(versions[0].version, "7.4.33");
        assert!(!versions[0].active);
        assert_eq!(versions[1].version, "8.3.31");
        assert!(versions[1].active);
        assert_eq!(versions[2].version, "8.5.6");
        assert!(!versions[2].active);
    }

    #[test]
    fn test_parse_phpvm_list_fat_arrow_marker() {
        let output = " => 8.2.21\n    8.1.29\n";
        let versions = parse_phpvm_list(output);
        assert_eq!(versions.len(), 2);
        assert_eq!(versions[0].version, "8.2.21");
        assert!(versions[0].active);
        assert!(!versions[1].active);
    }

    #[test]
    fn test_parse_phpvm_list_active_suffix_only() {
        // Some phpvm builds omit prefix marker but keep "(active)" suffix.
        let output = "    7.4.33\n    8.3.31 (active)\n    8.5.6\n";
        let versions = parse_phpvm_list(output);
        assert_eq!(versions.len(), 3);
        assert!(!versions[0].active);
        assert_eq!(versions[1].version, "8.3.31");
        assert!(versions[1].active);
        assert!(!versions[2].active);
    }
}
