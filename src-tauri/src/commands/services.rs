use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::process::Stdio;
use tauri::{AppHandle, Manager};
use tokio::process::Command;

use crate::commands::util::{silent_command, stream_and_wait_app};



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

/// Satu konflik port: host `port` service `service_id` sudah dipakai proses lain.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PortConflict {
    pub service_id: String,
    pub service_name: String,
    pub port: u16,
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

/// Gabungkan `requested` dengan id container yang sedang running (dari status),
/// jaga urutan `requested` dulu lalu running yang belum ada. Dipakai
/// `services_start` agar `--remove-orphans` tak menghancurkan service running
/// yang tidak ikut dikirim frontend.
pub fn union_with_running(requested: &[String], statuses: &[ServiceStatus]) -> Vec<String> {
    let mut effective = requested.to_vec();
    for s in statuses {
        if s.running && !effective.contains(&s.id) {
            effective.push(s.id.clone());
        }
    }
    effective
}

/// True bila host `port` sudah dipakai proses lain (bind gagal = terpakai).
/// Bind ke `127.0.0.1` — portable lintas-OS (std::net), tak perlu parse `netstat`.
/// Cukup untuk mapping default docker (host port ter-expose di loopback).
pub fn probe_host_port(port: u16) -> bool {
    std::net::TcpListener::bind(("127.0.0.1", port)).is_err()
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
    let mut cmd = silent_command("docker");
    cmd.args(["ps", "-a", "--filter", "name=servel_", "--format", "json"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

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
    silent_command("docker")
}


#[tauri::command]
pub async fn load_services(app: AppHandle) -> Result<Vec<ServiceDef>, String> {
    load_services_internal(&app).await
}

#[tauri::command]
pub async fn services_status() -> Result<Vec<ServiceStatus>, String> {
    services_status_internal().await
}

/// Cek apakah host port service yang akan di-start sudah dipakai proses lain.
/// Service yang SEDANG running di-exclude (port-nya dipegang container servel
/// sendiri — bukan konflik). Jika docker down, `services_status` gagal →
/// anggap tak ada yang running, semua port di-probe apa adanya.
#[tauri::command]
pub async fn check_port_conflicts(
    app: AppHandle,
    services: Vec<String>,
) -> Result<Vec<PortConflict>, String> {
    let defs = load_services_internal(&app).await?;

    let running: std::collections::HashSet<String> = services_status_internal()
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|s| s.running)
        .map(|s| s.id)
        .collect();

    let mut conflicts = Vec::new();
    for id in &services {
        if running.contains(id) {
            continue;
        }
        let Some(def) = defs.iter().find(|d| &d.id == id) else {
            continue;
        };
        for pm in &def.ports {
            let Ok(port) = pm.host.parse::<u16>() else {
                continue;
            };
            if probe_host_port(port) {
                conflicts.push(PortConflict {
                    service_id: def.id.clone(),
                    service_name: def.name.clone(),
                    port,
                });
            }
        }
    }

    Ok(conflicts)
}

#[tauri::command]
pub async fn services_start(app: AppHandle, services: Vec<String>) -> Result<(), String> {
    let defs = load_services_internal(&app).await?;

    // Safety net: gabungkan container servel_ yang SEDANG running ke compose.
    // Tanpa ini, `up --remove-orphans` (di bawah) menghancurkan service yang
    // tidak ada di `services` — mis. saat frontend mengirim subset karena race
    // state setelah boot. Union hanya melindungi yang running; yang sudah exited
    // (di-deselect) tetap boleh dibersihkan orphan. Jika docker belum ready,
    // status gagal → pakai `services` apa adanya (tak ada yang running dilindungi).
    let running = services_status_internal().await.unwrap_or_default();
    let effective = union_with_running(&services, &running);

    let yaml = crate::commands::compose::generate_compose(&defs, &effective);

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
    stream_and_wait_app(cmd, &app).await?;

    Ok(())
}

#[tauri::command]
pub async fn services_stop(app: AppHandle, services: Vec<String>) -> Result<(), String> {
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
    stream_and_wait_app(cmd, &app).await?;

    Ok(())
}

#[tauri::command]
pub async fn services_stop_all(app: AppHandle) -> Result<(), String> {
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
    stream_and_wait_app(cmd, &app).await?;

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

    fn status(id: &str, running: bool) -> ServiceStatus {
        ServiceStatus {
            id: id.to_string(),
            container_name: format!("servel_{}", id),
            running,
            state: if running { "running" } else { "exited" }.to_string(),
            exit_code: if running { None } else { Some(0) },
        }
    }

    #[test]
    fn test_union_preserves_running_not_requested() {
        // Toggle sqlserver saja, tapi 4 lain sedang running → semua ikut compose.
        let requested = vec!["sqlserver".to_string()];
        let running = vec![
            status("mysql", true),
            status("postgres", true),
            status("minio", true),
            status("redis", true),
        ];
        let effective = union_with_running(&requested, &running);
        assert_eq!(effective.len(), 5);
        assert_eq!(effective[0], "sqlserver", "requested harus tetap di depan");
        for id in ["mysql", "postgres", "minio", "redis"] {
            assert!(effective.contains(&id.to_string()), "{} harus terlindungi", id);
        }
    }

    #[test]
    fn test_union_ignores_stopped_and_dedupes() {
        // exited (di-deselect) tidak ikut; running yang sudah diminta tidak dobel.
        let requested = vec!["mysql".to_string(), "redis".to_string()];
        let running = vec![status("mysql", true), status("postgres", false)];
        let effective = union_with_running(&requested, &running);
        assert_eq!(effective, vec!["mysql".to_string(), "redis".to_string()]);
    }

    #[test]
    fn test_probe_host_port_detects_in_use() {
        // Bind port ephemeral → probe pada port itu harus true (terpakai).
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(probe_host_port(port), "port yang sedang di-bind harus terdeteksi terpakai");
    }

    #[test]
    fn test_probe_host_port_free_after_release() {
        // Bind lalu drop listener → port kembali bebas → probe false.
        let port = {
            let l = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
            l.local_addr().unwrap().port()
        };
        assert!(!probe_host_port(port), "port yang sudah dilepas harus terdeteksi bebas");
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
