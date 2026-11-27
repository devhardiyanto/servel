//! Backend Docker: `docker compose` dengan file compose yang di-generate.

use std::process::Stdio;
use tauri::AppHandle;

use crate::commands::compose::{compose_path, generate_compose, COMPOSE_PROJECT};
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

/// Apakah Docker masih menyimpan container Kova (`servel_*`)?
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

/// Kenapa Docker belum bisa dipakai, untuk pesan yang bisa
/// ditindaklanjuti (terutama Linux dengan Docker Engine tanpa Desktop).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DockerProblem {
    /// `permission denied` ke socket: user belum di grup `docker`.
    Permission,
    /// Daemon tidak bisa dihubungi (mati / belum dinyalakan).
    Daemon,
    /// Daemon hidup, tetapi plugin Compose v2 (`docker compose`) tidak ada.
    Compose,
}

impl DockerProblem {
    pub fn as_str(self) -> &'static str {
        match self {
            DockerProblem::Permission => "permission",
            DockerProblem::Daemon => "daemon",
            DockerProblem::Compose => "compose",
        }
    }
}

/// Klasifikasi stderr `docker info` yang gagal.
pub fn classify_info_error(stderr: &str) -> DockerProblem {
    let s = stderr.to_lowercase();
    if s.contains("permission denied") && (s.contains("docker.sock") || s.contains("docker daemon socket")) {
        DockerProblem::Permission
    } else {
        DockerProblem::Daemon
    }
}

/// Pesan pengganti untuk error compose yang dikenal; `None` = pakai aslinya.
pub fn friendly_error(stderr: &str) -> Option<String> {
    let s = stderr.to_lowercase();
    if s.contains("'compose' is not a docker command") || s.contains("unknown command \"compose\"") {
        return Some(
            "Docker Compose v2 is not installed. Install the docker-compose-plugin package (Debian/Ubuntu: sudo apt install docker-compose-plugin), then try again.".to_string(),
        );
    }
    if s.contains("permission denied") && s.contains("docker.sock") {
        return Some(
            "Permission denied on the Docker socket. Add your user to the docker group (sudo usermod -aG docker $USER), then log out and back in.".to_string(),
        );
    }
    None
}

/// `None` = daemon terjangkau dan Compose v2 ada.
pub async fn diagnose() -> Option<DockerProblem> {
    let fut = silent_command("docker").arg("info").stdout(Stdio::null()).stderr(Stdio::piped()).output();
    let out = match tokio::time::timeout(std::time::Duration::from_secs(15), fut).await {
        Ok(Ok(o)) => o,
        _ => return Some(DockerProblem::Daemon),
    };
    if !out.status.success() {
        return Some(classify_info_error(&String::from_utf8_lossy(&out.stderr)));
    }
    let compose = silent_command("docker")
        .args(["compose", "version"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false);
    if compose {
        None
    } else {
        Some(DockerProblem::Compose)
    }
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

/// Hapus container. Tidak ada = berhasil.
pub async fn remove_container(name: &str) -> Result<(), String> {
    let out = silent_command("docker").args(["rm", "-f", name]).output().await.map_err(|e| e.to_string())?;
    let err = String::from_utf8_lossy(&out.stderr).to_lowercase();
    if out.status.success() || err.contains("no such container") {
        Ok(())
    } else {
        Err(format!("docker rm failed: {}", err.trim()))
    }
}

/// Hapus volume. Tidak ada = berhasil.
pub async fn remove_volume(name: &str) -> Result<(), String> {
    let out = silent_command("docker").args(["volume", "rm", "-f", name]).output().await.map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!("docker volume rm failed: {}", String::from_utf8_lossy(&out.stderr).trim()))
    }
}

/// `docker exec <name> sh -c <cmd>` dengan stdout dialirkan (untuk backup).
pub fn exec_command(name: &str, cmd: &str) -> tokio::process::Command {
    let mut c = silent_command("docker");
    c.args(["exec", name, "sh", "-c", cmd]).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    c
}

/// `docker exec <name> sh -c <cmd>` dengan output di-stream ke panel log.
pub async fn exec_streamed(app: &AppHandle, name: &str, cmd: &str, source: &str) -> Result<(), String> {
    let mut c = silent_command("docker");
    c.args(["exec", name, "sh", "-c", cmd]);
    crate::commands::util::stream_and_wait_as(c, app, Some(source)).await
}

/// stdout + stderr dari `docker logs --tail <n> <name>`.
pub async fn logs_tail(name: &str, n: u32) -> Option<String> {
    let fut = silent_command("docker")
        .args(["logs", "--tail", &n.to_string(), name])
        .kill_on_drop(true)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();
    let out = tokio::time::timeout(std::time::Duration::from_secs(10), fut).await.ok()?.ok()?;
    out.status.success().then(|| {
        format!("{}\n{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
    })
}

/// `docker exec <name> sh -c <cmd>` → exit code. Timeout membunuh `docker exec`.
pub async fn exec_status(name: &str, cmd: &str, timeout: std::time::Duration) -> Option<i32> {
    let fut = silent_command("docker").args(["exec", name, "sh", "-c", cmd]).kill_on_drop(true).status();
    tokio::time::timeout(timeout, fut).await.ok()?.ok()?.code()
}

const QUERY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

pub async fn status() -> Result<Vec<ServiceStatus>, String> {
    let output = silent_command("docker")
        .args(["ps", "-a", "--filter", "name=servel_", "--format", "json"])
        .kill_on_drop(true)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();
    // Docker Desktop yang macet tidak boleh menahan polling status selamanya.
    let output = tokio::time::timeout(QUERY_TIMEOUT, output)
        .await
        .map_err(|_| "docker ps did not answer within 20 s".to_string())?
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
        .kill_on_drop(true)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();
    let output = tokio::time::timeout(QUERY_TIMEOUT, output)
        .await
        .map_err(|_| "docker stats did not answer within 20 s".to_string())?
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
    crate::commands::compose::write_private(&compose_path(), &yaml)?;

    let path = compose_path_str()?;
    let mut cmd = silent_command("docker");
    cmd.args(["compose", "-p", COMPOSE_PROJECT, "-f", &path, "up", "-d", "--remove-orphans"]);
    stream_and_wait_app(cmd, app).await
}

/// Container project compose Kova yang sedang berjalan, opsional per service.
/// Dipakai bila file compose tidak ada (mis. dihapus atau belum pernah ditulis).
async fn running_by_label(service: Option<&str>) -> Vec<String> {
    let mut cmd = silent_command("docker");
    cmd.args(["ps", "-q", "--filter"]).arg(format!("label=com.docker.compose.project={COMPOSE_PROJECT}"));
    if let Some(svc) = service {
        cmd.arg("--filter").arg(format!("label=com.docker.compose.service={svc}"));
    }
    let out = tokio::time::timeout(QUERY_TIMEOUT, cmd.kill_on_drop(true).stdout(Stdio::piped()).output()).await;
    match out {
        Ok(Ok(o)) if o.status.success() => String::from_utf8_lossy(&o.stdout).split_whitespace().map(str::to_string).collect(),
        _ => Vec::new(),
    }
}

async fn stop_containers(app: &AppHandle, ids: Vec<String>) -> Result<(), String> {
    if ids.is_empty() {
        return Ok(());
    }
    let mut cmd = silent_command("docker");
    cmd.arg("stop").args(ids);
    stream_and_wait_app(cmd, app).await
}

pub async fn stop(app: &AppHandle, services: &[String]) -> Result<(), String> {
    if !compose_path().exists() {
        let mut ids = Vec::new();
        for s in services {
            ids.extend(running_by_label(Some(s)).await);
        }
        return stop_containers(app, ids).await;
    }
    let path = compose_path_str()?;
    let mut cmd = silent_command("docker");
    cmd.args(["compose", "-p", COMPOSE_PROJECT, "-f", &path, "stop"]);
    cmd.args(services);
    stream_and_wait_app(cmd, app).await
}

pub async fn stop_all(app: &AppHandle) -> Result<(), String> {
    if !compose_path().exists() {
        return stop_containers(app, running_by_label(None).await).await;
    }
    let path = compose_path_str()?;
    let mut cmd = silent_command("docker");
    cmd.args(["compose", "-p", COMPOSE_PROJECT, "-f", &path, "down"]);
    stream_and_wait_app(cmd, app).await
}

#[cfg(test)]
mod tests {
    #[test]
    fn docker_info_errors_are_classified() {
        use super::{classify_info_error, friendly_error, DockerProblem};
        let perm = "permission denied while trying to connect to the Docker daemon socket at unix:///var/run/docker.sock: Get \"http://%2Fvar%2Frun%2Fdocker.sock/v1.47/info\": dial unix /var/run/docker.sock: connect: permission denied";
        assert_eq!(classify_info_error(perm), DockerProblem::Permission);
        let down = "Cannot connect to the Docker daemon at unix:///var/run/docker.sock. Is the docker daemon running?";
        assert_eq!(classify_info_error(down), DockerProblem::Daemon);
        assert!(friendly_error("docker: 'compose' is not a docker command.\nSee 'docker --help'").unwrap().contains("docker-compose-plugin"));
        assert!(friendly_error(perm).unwrap().contains("usermod"));
        assert_eq!(friendly_error("no such service: mysql"), None);
    }
}
