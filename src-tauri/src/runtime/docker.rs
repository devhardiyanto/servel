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

pub async fn status() -> Result<Vec<ServiceStatus>, String> {
    let output = silent_command("docker")
        .args(["ps", "-a", "--filter", "name=servel_", "--format", "json"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| format!("Gagal jalankan docker ps: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("docker ps gagal: {}", stderr));
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
        .map_err(|e| format!("Gagal jalankan docker stats: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("docker stats gagal: {}", stderr));
    }

    Ok(parse_docker_stats_json(&String::from_utf8_lossy(&output.stdout)))
}

fn compose_path_str() -> Result<String, String> {
    compose_path()
        .to_str()
        .map(str::to_string)
        .ok_or_else(|| "compose path tidak valid UTF-8".to_string())
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
        std::fs::create_dir_all(parent).map_err(|e| format!("Gagal buat dir compose: {}", e))?;
    }
    std::fs::write(&compose_file, yaml).map_err(|e| format!("Gagal tulis compose file: {}", e))?;

    let path = compose_path_str()?;
    let mut cmd = silent_command("docker");
    cmd.args(["compose", "-f", &path, "up", "-d", "--remove-orphans"]);
    stream_and_wait_app(cmd, app).await
}

pub async fn stop(app: &AppHandle, services: &[String]) -> Result<(), String> {
    if !compose_path().exists() {
        return Err("compose file tidak ditemukan, jalankan Start terlebih dahulu".to_string());
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
