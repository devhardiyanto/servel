use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, Window};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

const SERVICE_IDS: &[&str] = &[
    "mysql", "postgres", "redis", "rabbitmq", "mongodb",
    "minio", "mailpit", "gotenberg", "sqlserver",
];

fn src_from_line(line: &str) -> String {
    for id in SERVICE_IDS {
        if line.contains(&format!("servel_{}", id)) {
            return id.to_uppercase();
        }
    }
    "SERVEL".to_string()
}

fn parse_docker_error(stderr_acc: &str) -> Option<String> {
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


#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PortMap {
    pub host: String,
    pub container: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ServiceDef {
    pub id: String,
    pub name: String,
    pub category: String,
    pub image: String,
    pub container_name: String,
    pub ports: Vec<PortMap>,
    #[serde(default)]
    pub environment: HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volumes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    pub ram_estimate_mb: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ServiceStatus {
    pub id: String,
    pub container_name: String,
    pub running: bool,
    pub state: String,
    pub exit_code: Option<i32>,
}

/// Parse line-delimited JSON output dari `docker ps -a --format json`.
/// Docker mengembalikan satu objek JSON per baris (bukan JSON array).
pub fn parse_docker_ps_json(stdout: &str) -> Vec<ServiceStatus> {
    let mut result = Vec::new();

    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let Ok(val) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };

        let container_name = val
            .get("Names")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim_start_matches('/')
            .to_string();

        if container_name.is_empty() {
            continue;
        }

        let Some(id) = id_from_container_name(&container_name) else {
            continue;
        };

        let state = val
            .get("State")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let running = state == "running";

        let exit_code = val
            .get("ExitCode")
            .and_then(|v| v.as_i64())
            .map(|v| v as i32);

        result.push(ServiceStatus {
            id,
            container_name,
            running,
            state,
            exit_code,
        });
    }

    result
}

/// Strip prefix `servel_` dari container name, return service id.
/// Container di luar prefix servel_ diabaikan.
pub fn id_from_container_name(name: &str) -> Option<String> {
    name.strip_prefix("servel_").map(|s| s.to_string())
}

fn emit_line(window: &Window, line: &str, stream: &str) {
    let source = src_from_line(line);
    let _ = window.emit(
        "cmd-output",
        serde_json::json!({ "line": line, "stream": stream, "source": source }),
    );
}

/// Inti load_services tanpa #[tauri::command], bisa dipanggil internal.
pub(crate) async fn load_services_internal(app: &AppHandle) -> Result<Vec<ServiceDef>, String> {
    let resource_path = app
        .path()
        .resource_dir()
        .map_err(|e| format!("Gagal resolve resource dir: {}", e))?
        .join("services")
        .join("services.json");

    #[cfg(debug_assertions)]
    if !resource_path.exists() {
        let dev_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../assets/services/services.json");
        let content = std::fs::read_to_string(&dev_path)
            .map_err(|e| format!("Gagal baca services.json (dev fallback, {}): {}", dev_path.display(), e))?;
        return serde_json::from_str(&content)
            .map_err(|e| format!("Gagal parse services.json: {}", e));
    }

    let content = std::fs::read_to_string(&resource_path)
        .map_err(|e| format!("Gagal baca services.json ({}): {}", resource_path.display(), e))?;

    serde_json::from_str(&content).map_err(|e| format!("Gagal parse services.json: {}", e))
}

/// Inti services_status tanpa #[tauri::command], bisa dipanggil dari polling.
pub(crate) async fn services_status_internal() -> Result<Vec<ServiceStatus>, String> {
    let mut cmd = Command::new("docker");
    cmd.args(["ps", "-a", "--filter", "name=servel_", "--format", "json"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(target_os = "windows")]
    {
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let output = cmd
        .output()
        .await
        .map_err(|e| format!("Gagal jalankan docker ps: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("docker ps gagal: {}", stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(parse_docker_ps_json(&stdout))
}

fn new_docker_cmd() -> Command {
    let mut cmd = Command::new("docker");
    #[cfg(target_os = "windows")]
    {
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

async fn stream_and_wait(mut cmd: Command, window: &Window) -> Result<(), String> {
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| format!("Gagal spawn docker: {}", e))?;

    let stdout = child.stdout.take().ok_or("stdout tidak tersedia")?;
    let stderr = child.stderr.take().ok_or("stderr tidak tersedia")?;

    let stderr_buf: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    let window_out = window.clone();
    let stdout_task = tokio::spawn(async move {
        let mut reader = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            emit_line(&window_out, &line, "stdout");
        }
    });

    let window_err = window.clone();
    let stderr_buf_clone = Arc::clone(&stderr_buf);
    let stderr_task = tokio::spawn(async move {
        let mut reader = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            if let Ok(mut buf) = stderr_buf_clone.lock() {
                buf.push(line.clone());
            }
            emit_line(&window_err, &line, "stderr");
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

#[tauri::command]
pub async fn load_services(app: AppHandle) -> Result<Vec<ServiceDef>, String> {
    load_services_internal(&app).await
}

#[tauri::command]
pub async fn services_status() -> Result<Vec<ServiceStatus>, String> {
    services_status_internal().await
}

#[tauri::command]
pub async fn services_start(
    window: Window,
    app: AppHandle,
    services: Vec<String>,
) -> Result<(), String> {
    let defs = load_services_internal(&app).await?;

    let yaml = crate::commands::compose::generate_compose(&defs, &services);

    let compose_file = crate::commands::compose::compose_path();
    if let Some(parent) = compose_file.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Gagal buat dir compose: {}", e))?;
    }
    std::fs::write(&compose_file, yaml)
        .map_err(|e| format!("Gagal tulis compose file: {}", e))?;

    let path_str = compose_file
        .to_str()
        .ok_or("compose path tidak valid UTF-8")?
        .to_string();

    let mut cmd = new_docker_cmd();
    cmd.args(["compose", "-f", &path_str, "up", "-d", "--remove-orphans"]);
    stream_and_wait(cmd, &window).await?;

    Ok(())
}

#[tauri::command]
pub async fn services_stop(window: Window, services: Vec<String>) -> Result<(), String> {
    let compose_file = crate::commands::compose::compose_path();
    if !compose_file.exists() {
        return Err("compose file tidak ditemukan, jalankan Start terlebih dahulu".to_string());
    }

    let path_str = compose_file
        .to_str()
        .ok_or("compose path tidak valid UTF-8")?
        .to_string();

    let mut cmd = new_docker_cmd();
    cmd.args(["compose", "-f", &path_str, "stop"]);
    cmd.args(&services);
    stream_and_wait(cmd, &window).await?;

    Ok(())
}

#[tauri::command]
pub async fn services_stop_all(window: Window) -> Result<(), String> {
    let compose_file = crate::commands::compose::compose_path();
    if !compose_file.exists() {
        return Ok(());
    }

    let path_str = compose_file
        .to_str()
        .ok_or("compose path tidak valid UTF-8")?
        .to_string();

    let mut cmd = new_docker_cmd();
    cmd.args(["compose", "-f", &path_str, "down"]);
    stream_and_wait(cmd, &window).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_running_container() {
        let line = r#"{"Command":"\"docker-entrypoint.s\"","CreatedAt":"2026-06-12 10:00:00 +0000 UTC","ID":"abc123","Image":"mysql:8.0","Labels":"","LocalVolumes":"1","Mounts":"servel_mysql_data","Names":"servel_mysql","Networks":"servel_default","Ports":"0.0.0.0:3306->3306/tcp","RunningFor":"2 hours ago","Size":"0B","State":"running","Status":"Up 2 hours","ExitCode":0}"#;

        let statuses = parse_docker_ps_json(line);
        assert_eq!(statuses.len(), 1);
        let s = &statuses[0];
        assert_eq!(s.id, "mysql");
        assert_eq!(s.container_name, "servel_mysql");
        assert!(s.running);
        assert_eq!(s.state, "running");
        assert_eq!(s.exit_code, Some(0));
    }

    #[test]
    fn test_parse_stopped_exit_0() {
        let line = r#"{"Command":"\"docker-entrypoint.s\"","CreatedAt":"2026-06-12 09:00:00 +0000 UTC","ID":"def456","Image":"redis:7-alpine","Labels":"","LocalVolumes":"1","Mounts":"servel_redis_data","Names":"servel_redis","Networks":"servel_default","Ports":"","RunningFor":"3 hours ago","Size":"0B","State":"exited","Status":"Exited (0) 1 hour ago","ExitCode":0}"#;

        let statuses = parse_docker_ps_json(line);
        assert_eq!(statuses.len(), 1);
        let s = &statuses[0];
        assert_eq!(s.id, "redis");
        assert_eq!(s.container_name, "servel_redis");
        assert!(!s.running);
        assert_eq!(s.state, "exited");
        assert_eq!(s.exit_code, Some(0));
    }

    #[test]
    fn test_parse_stopped_exit_nonzero() {
        let line = r#"{"Command":"\"docker-entrypoint.s\"","CreatedAt":"2026-06-12 08:00:00 +0000 UTC","ID":"ghi789","Image":"postgres:16-alpine","Labels":"","LocalVolumes":"1","Mounts":"servel_postgres_data","Names":"servel_postgres","Networks":"servel_default","Ports":"","RunningFor":"4 hours ago","Size":"0B","State":"exited","Status":"Exited (1) 30 minutes ago","ExitCode":1}"#;

        let statuses = parse_docker_ps_json(line);
        assert_eq!(statuses.len(), 1);
        let s = &statuses[0];
        assert_eq!(s.id, "postgres");
        assert_eq!(s.container_name, "servel_postgres");
        assert!(!s.running);
        assert_eq!(s.state, "exited");
        assert_eq!(s.exit_code, Some(1));
    }

    #[test]
    fn test_load_services_shape() {
        let json = include_str!("../../../assets/services/services.json");
        let defs: Vec<ServiceDef> = serde_json::from_str(json).expect("services.json harus valid");

        assert_eq!(defs.len(), 9, "harus ada 9 service");

        let core_count = defs.iter().filter(|d| d.category == "core").count();
        let additional_count = defs.iter().filter(|d| d.category == "additional").count();
        assert_eq!(core_count, 4, "harus ada 4 core service");
        assert_eq!(additional_count, 5, "harus ada 5 additional service");

        for def in &defs {
            assert!(
                def.container_name.starts_with("servel_"),
                "containerName '{}' harus prefix servel_",
                def.container_name
            );
        }
    }
}
