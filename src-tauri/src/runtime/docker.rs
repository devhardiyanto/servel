//! Backend Docker: `docker compose` dengan file compose yang di-generate.
//! Perilaku identik dengan Servel v1.5 — hanya dipindah dari `commands::services`.

use std::process::Stdio;
use tauri::AppHandle;

use crate::commands::compose::{compose_path, generate_compose};
use crate::commands::services::{parse_docker_ps_json, union_with_running, ServiceDef, ServiceStatus};
use crate::commands::stats::{parse_docker_stats_json, ContainerMemStat};
use crate::commands::util::{silent_command, stream_and_wait_app};

/// `docker` CLI ada di PATH (tidak berarti daemon running).
pub async fn is_installed() -> bool {
    silent_command("docker")
        .arg("--version")
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Apakah Docker masih menyimpan container Servel (`servel_*`)?
/// `None` bila daemon tidak bisa ditanya (mati / tidak terpasang / timeout).
pub async fn has_servel_containers() -> Option<bool> {
    let fut = silent_command("docker")
        .args(["ps", "-a", "-q", "--filter", "name=servel_"])
        .stdout(Stdio::piped())
        .output();
    let out = tokio::time::timeout(std::time::Duration::from_secs(10), fut).await.ok()?.ok()?;
    if !out.status.success() {
        return None;
    }
    Some(!String::from_utf8_lossy(&out.stdout).trim().is_empty())
}

/// Daemon bisa dihubungi.
pub async fn is_running() -> bool {
    silent_command("docker")
        .arg("info")
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false)
}

pub async fn version() -> Option<String> {
    let out = silent_command("docker")
        .arg("--version")
        .stdout(Stdio::piped())
        .output()
        .await
        .ok()?;
    if !out.status.success() {
        return None;
    }
    crate::commands::util::extract_semver(&String::from_utf8_lossy(&out.stdout))
}

/// `docker cp <host> <name>:<dest>` (path host absolut aman di Docker).
pub async fn copy_file_into(name: &str, host: &std::path::Path, dest: &str) -> Result<(), String> {
    let out = silent_command("docker")
        .arg("cp")
        .arg(host)
        .arg(format!("{}:{}", name, dest))
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| format!("Could not run docker cp: {}", e))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!("docker cp failed: {}", String::from_utf8_lossy(&out.stderr).trim()))
    }
}

/// `docker exec <name> sh -c <cmd>` dengan output di-stream ke panel log.
pub async fn exec_streamed(app: &AppHandle, name: &str, cmd: &str) -> Result<(), String> {
    let mut c = silent_command("docker");
    c.args(["exec", name, "sh", "-c", cmd]);
    stream_and_wait_app(c, app).await
}

/// `docker exec <name> sh -c <cmd>` → exit code.
pub async fn exec_status(name: &str, cmd: &str, timeout: std::time::Duration) -> Option<i32> {
    let fut = silent_command("docker").args(["exec", name, "sh", "-c", cmd]).status();
    tokio::time::timeout(timeout, fut).await.ok()?.ok()?.code()
}

pub async fn status() -> Result<Vec<ServiceStatus>, String> {
    let output = silent_command("docker")
        .args(["ps", "-a", "--filter", "name=servel_", "--format", "json"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| format!("Could not run docker ps: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("docker ps failed: {}", stderr));
    }

    Ok(parse_docker_ps_json(&String::from_utf8_lossy(&output.stdout)))
}

/// `docker stats --no-stream --format json` (tanpa `--filter` agar kompatibel
/// di Docker lama). Filter prefix `servel_` dilakukan saat parse.
pub async fn stats() -> Result<Vec<ContainerMemStat>, String> {
    let output = silent_command("docker")
        .args(["stats", "--no-stream", "--format", "json"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| format!("Could not run docker stats: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("docker stats failed: {}", stderr));
    }

    Ok(parse_docker_stats_json(&String::from_utf8_lossy(&output.stdout)))
}

fn compose_path_str() -> Result<String, String> {
    compose_path()
        .to_str()
        .map(str::to_string)
        .ok_or_else(|| "compose path is not valid UTF-8".to_string())
}

pub async fn start(app: &AppHandle, defs: &[ServiceDef], services: &[String]) -> Result<(), String> {
    // Safety net: gabungkan container servel_ yang SEDANG running ke compose.
    // Tanpa ini, `up --remove-orphans` menghancurkan service yang tidak ada di
    // `services` — mis. saat frontend mengirim subset karena race state setelah
    // boot. Union hanya melindungi yang running; yang sudah exited (di-deselect)
    // tetap boleh dibersihkan orphan. Jika docker belum ready, status gagal →
    // pakai `services` apa adanya.
    let running = status().await.unwrap_or_default();
    let effective = union_with_running(services, &running);

    let yaml = generate_compose(defs, &effective);
    let compose_file = compose_path();
    if let Some(parent) = compose_file.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Could not create the compose folder: {}", e))?;
    }
    std::fs::write(&compose_file, yaml).map_err(|e| format!("Could not write the compose file: {}", e))?;

    let path = compose_path_str()?;
    let mut cmd = silent_command("docker");
    cmd.args(["compose", "-f", &path, "up", "-d", "--remove-orphans"]);
    stream_and_wait_app(cmd, app).await
}

pub async fn stop(app: &AppHandle, services: &[String]) -> Result<(), String> {
    if !compose_path().exists() {
        return Err("compose file not found; run Start first".to_string());
    }
    let path = compose_path_str()?;
    let mut cmd = silent_command("docker");
    cmd.args(["compose", "-f", &path, "stop"]);
    cmd.args(services);
    stream_and_wait_app(cmd, app).await
}

pub async fn stop_all(app: &AppHandle) -> Result<(), String> {
    if !compose_path().exists() {
        return Ok(());
    }
    let path = compose_path_str()?;
    let mut cmd = silent_command("docker");
    cmd.args(["compose", "-f", &path, "down"]);
    stream_and_wait_app(cmd, app).await
}
