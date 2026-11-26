use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tauri::{AppHandle, Manager};

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
    /// Ditambah pengguna (Services › Add service), bukan dari katalog.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub custom: bool,
    /// Kapan custom service dianggap siap. `None` = saat container running.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ready: Option<ReadyCheck>,
    /// Baris .env untuk Connect, `{port}` = port di PC (custom service).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connect_env: Option<String>,
}

/// Cek siap untuk custom service, dari PC (bukan di dalam container).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ReadyCheck {
    /// Port pertama menerima koneksi TCP.
    Port,
    /// `GET http://127.0.0.1:<port><path>` menjawab 2xx/3xx/401/403.
    Http { path: String },
    /// Baris log container (200 baris terakhir) memuat `text`.
    Log { text: String },
}

/// Id custom service: huruf kecil, angka, `-`; 1–32 karakter.
pub fn valid_custom_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 32 && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !id.starts_with('-')
}

/// Rapikan custom service dari config: container `servel_<id>`, hanya volume
/// bernama `servel_<id>_…` (tanpa bind mount ke folder PC), port angka.
pub fn sanitize_custom(def: &ServiceDef, builtin: &[ServiceDef]) -> Option<ServiceDef> {
    if !valid_custom_id(&def.id) || builtin.iter().any(|b| b.id == def.id) || def.image.trim().is_empty() {
        return None;
    }
    if def.image.chars().any(|c| c.is_whitespace()) {
        return None;
    }
    // Baris baru di nilai mana pun bisa menyisipkan kunci YAML di compose.
    let multiline = |s: &str| s.contains(['\n', '\r']);
    if def.environment.iter().any(|(k, v)| multiline(k) || multiline(v) || k.contains('='))
        || def.volumes.iter().flatten().any(|v| multiline(v))
        || def.command.as_deref().is_some_and(multiline)
    {
        return None;
    }
    let prefix = format!("servel_{}_", def.id.replace('-', "_"));
    let volumes: Vec<String> = def
        .volumes
        .clone()
        .unwrap_or_default()
        .into_iter()
        .filter(|v| v.split_once(':').is_some_and(|(n, path)| n.starts_with(&prefix) && path.starts_with('/')))
        .collect();
    let ports: Vec<PortMap> = def
        .ports
        .iter()
        .filter(|p| p.host.parse::<u16>().is_ok_and(|n| n >= 1024) && p.container.parse::<u16>().is_ok())
        .cloned()
        .collect();
    Some(ServiceDef {
        id: def.id.clone(),
        name: if def.name.trim().is_empty() { def.id.clone() } else { def.name.trim().to_string() },
        category: "custom".into(),
        image: def.image.trim().to_string(),
        container_name: format!("servel_{}", def.id.replace('-', "_")),
        ports,
        environment: def.environment.clone(),
        volumes: (!volumes.is_empty()).then_some(volumes),
        command: def.command.clone().filter(|c| !c.trim().is_empty()),
        ram_estimate_mb: if def.ram_estimate_mb == 0 { 200 } else { def.ram_estimate_mb },
        custom: true,
        ready: def.ready.clone(),
        connect_env: def.connect_env.clone(),
    })
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

/// Id service dari nama container `servel_<id>`. Custom service `acme-search`
/// memakai container `servel_acme_search` (lihat `sanitize_custom`); id bawaan
/// tidak memakai `_`, jadi `_` dikembalikan menjadi `-`.
pub fn id_from_container_name(name: &str) -> Option<String> {
    name.strip_prefix("servel_").filter(|s| !s.is_empty()).map(|s| s.replace('_', "-"))
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
/// Cukup untuk Docker (compose Kova mem-publish di 127.0.0.1) maupun WSLC (Kova bind
/// eksplisit ke `127.0.0.1`, dan WSLC mem-publish di loopback Windows).
pub fn probe_host_port(port: u16) -> bool {
    std::net::TcpListener::bind(("127.0.0.1", port)).is_err()
}


/// Inti load_services tanpa #[tauri::command], bisa dipanggil internal.
pub(crate) async fn load_services_internal(app: &AppHandle) -> Result<Vec<ServiceDef>, String> {
    let mut defs = load_builtin_services(app)?;
    if let Some(state) = app.try_state::<std::sync::Mutex<crate::commands::config::ConfigState>>() {
        if let Ok(cfg) = state.lock() {
            let custom: Vec<ServiceDef> = cfg.custom_services.iter().filter_map(|c| sanitize_custom(c, &defs)).collect();
            defs.extend(custom);
            apply_port_overrides(&mut defs, &cfg.service_ports);
        }
    }
    Ok(defs)
}

/// Services › Change port: ganti port di PC pada port pertama service.
pub fn apply_port_overrides(defs: &mut [ServiceDef], ports: &std::collections::BTreeMap<String, u16>) {
    for d in defs.iter_mut() {
        if let (Some(p), Some(first)) = (ports.get(&d.id), d.ports.first_mut()) {
            if *p >= 1024 {
                first.host = p.to_string();
            }
        }
    }
}

fn load_builtin_services(app: &AppHandle) -> Result<Vec<ServiceDef>, String> {
    let resource_path = app
        .path()
        .resource_dir()
        .map_err(|e| format!("Could not find the resource folder: {}", e))?
        .join("services")
        .join("services.json");

    #[cfg(debug_assertions)]
    if !resource_path.exists() {
        let dev_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../assets/services/services.json");
        let content = std::fs::read_to_string(&dev_path)
            .map_err(|e| format!("Could not read services.json (dev fallback, {}): {}", dev_path.display(), e))?;
        return serde_json::from_str(&content)
            .map_err(|e| format!("Could not parse services.json: {}", e));
    }

    let content = std::fs::read_to_string(&resource_path)
        .map_err(|e| format!("Could not read services.json ({}): {}", resource_path.display(), e))?;

    serde_json::from_str(&content).map_err(|e| format!("Could not parse services.json: {}", e))
}

/// Inti services_status tanpa #[tauri::command], bisa dipanggil dari polling.
/// Dispatch ke engine aktif (Docker / WSLC) — lihat `crate::runtime`.
pub(crate) async fn services_status_internal() -> Result<Vec<ServiceStatus>, String> {
    crate::runtime::status().await
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
    crate::runtime::start(&app, &defs, &services).await
}

#[tauri::command]
pub async fn services_stop(app: AppHandle, services: Vec<String>) -> Result<(), String> {
    let defs = load_services_internal(&app).await?;
    crate::runtime::stop(&app, &defs, &services).await
}

/// Restore dump `.sql` ke `service` (mysql | postgres). Lihat `runtime::restore`.
#[tauri::command]
pub async fn services_restore(app: AppHandle, service: String, file: String) -> Result<(), String> {
    let defs = load_services_internal(&app).await?;
    crate::runtime::restore::restore(&app, &defs, &service, &file).await
}

#[tauri::command]
pub async fn services_stop_all(app: AppHandle) -> Result<(), String> {
    let defs = load_services_internal(&app).await?;
    crate::runtime::stop_all(&app, &defs).await
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
        // Toggle mongodb saja, tapi 4 lain sedang running → semua ikut compose.
        let requested = vec!["mongodb".to_string()];
        let running = vec![
            status("mysql", true),
            status("postgres", true),
            status("minio", true),
            status("redis", true),
        ];
        let effective = union_with_running(&requested, &running);
        assert_eq!(effective.len(), 5);
        assert_eq!(effective[0], "mongodb", "requested harus tetap di depan");
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

        assert_eq!(defs.len(), 8, "harus ada 8 service (SQL Server dihapus di v1.7)");

        let core_count = defs.iter().filter(|d| d.category == "core").count();
        let additional_count = defs.iter().filter(|d| d.category == "additional").count();
        assert_eq!(core_count, 4, "harus ada 4 core service");
        assert_eq!(additional_count, 4, "harus ada 4 additional service");

        for def in &defs {
            assert!(
                def.container_name.starts_with("servel_"),
                "containerName '{}' harus prefix servel_",
                def.container_name
            );
        }
    }
}

#[cfg(test)]
mod custom_tests {
    use super::*;

    fn es() -> ServiceDef {
        ServiceDef {
            id: "elasticsearch".into(),
            name: " Elasticsearch ".into(),
            category: "core".into(),
            image: "elasticsearch:8.15.0".into(),
            container_name: "evil".into(),
            ports: vec![PortMap { host: "9200".into(), container: "9200".into() }, PortMap { host: "80".into(), container: "80".into() }],
            environment: HashMap::from([("discovery.type".into(), "single-node".into())]),
            volumes: Some(vec!["servel_elasticsearch_data:/usr/share/elasticsearch/data".into(), "C:/Users:/host".into()]),
            command: None,
            ram_estimate_mb: 0,
            custom: false,
            ready: Some(ReadyCheck::Http { path: "/_cluster/health".into() }),
            connect_env: Some("ELASTICSEARCH_URL=http://localhost:{port}".into()),
        }
    }

    #[test]
    fn custom_services_are_sanitized() {
        let d = sanitize_custom(&es(), &[]).unwrap();
        assert_eq!(d.container_name, "servel_elasticsearch");
        assert_eq!(d.category, "custom");
        assert!(d.custom);
        assert_eq!(d.name, "Elasticsearch");
        assert_eq!(d.ports.len(), 1, "ports under 1024 are dropped");
        assert_eq!(d.volumes.as_ref().unwrap().len(), 1, "host bind mounts are dropped");
        assert_eq!(d.ram_estimate_mb, 200);
        let mut bad = es();
        bad.id = "Bad Id".into();
        assert!(sanitize_custom(&bad, &[]).is_none());
        let builtin = vec![ServiceDef { id: "elasticsearch".into(), ..es() }];
        assert!(sanitize_custom(&es(), &builtin).is_none(), "cannot shadow a built-in id");
    }
}
