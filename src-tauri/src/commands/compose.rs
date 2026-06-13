use std::collections::HashSet;
use std::fmt::Write as FmtWrite;
use std::path::PathBuf;

use super::services::ServiceDef;

/// Resolve path ke docker-compose.yml di temp dir.
/// Parent dir dibuat jika belum ada.
pub fn compose_path() -> PathBuf {
    let base = std::env::temp_dir().join("servel");
    std::fs::create_dir_all(&base).ok();
    base.join("docker-compose.yml")
}

#[tauri::command]
pub fn get_compose_path() -> Result<String, String> {
    Ok(compose_path().to_string_lossy().to_string())
}

/// Heuristic: apakah volume string ini adalah named volume?
/// Named = bagian sebelum ':' pertama tidak mengandung '/' atau '.'.
fn is_named_volume(vol: &str) -> bool {
    let name_part = vol.split(':').next().unwrap_or("");
    !name_part.contains('/') && !name_part.contains('.')
}

/// Generate full docker-compose YAML string dari defs, filter by selected_ids.
/// Order output mengikuti urutan selected_ids.
pub(crate) fn generate_compose(defs: &[ServiceDef], selected_ids: &[String]) -> String {
    let selected: Vec<&ServiceDef> = selected_ids
        .iter()
        .filter_map(|id| defs.iter().find(|d| &d.id == id))
        .collect();

    let mut out = String::new();

    writeln!(out, "services:").ok();

    for def in &selected {
        writeln!(out, "  {}:", def.id).ok();
        writeln!(out, "    image: {}", def.image).ok();
        writeln!(out, "    container_name: {}", def.container_name).ok();
        writeln!(out, "    restart: unless-stopped").ok();

        if !def.ports.is_empty() {
            writeln!(out, "    ports:").ok();
            for p in &def.ports {
                writeln!(out, "      - \"{}:{}\"", p.host, p.container).ok();
            }
        }

        if !def.environment.is_empty() {
            writeln!(out, "    environment:").ok();
            // Sort untuk output deterministik
            let mut envs: Vec<(&String, &String)> = def.environment.iter().collect();
            envs.sort_by_key(|(k, _)| k.as_str());
            for (k, v) in envs {
                writeln!(out, "      - {}={}", k, v).ok();
            }
        }

        if let Some(volumes) = &def.volumes {
            if !volumes.is_empty() {
                writeln!(out, "    volumes:").ok();
                for vol in volumes {
                    writeln!(out, "      - {}", vol).ok();
                }
            }
        }

        if let Some(cmd) = &def.command {
            writeln!(out, "    command: {}", cmd).ok();
        }

        writeln!(out, "    networks:").ok();
        writeln!(out, "      - servel_default").ok();
    }

    // Kumpulkan semua named volumes (de-dup)
    let named_volumes: Vec<String> = {
        let mut set = HashSet::new();
        for def in &selected {
            if let Some(volumes) = &def.volumes {
                for vol in volumes {
                    if is_named_volume(vol) {
                        let vol_name = vol.split(':').next().unwrap_or("").to_string();
                        set.insert(vol_name);
                    }
                }
            }
        }
        let mut v: Vec<String> = set.into_iter().collect();
        v.sort();
        v
    };

    if !named_volumes.is_empty() {
        writeln!(out).ok();
        writeln!(out, "volumes:").ok();
        for vol_name in &named_volumes {
            // `name:` override mencegah Docker Compose nge-prefix dengan project name
            // (yang bikin nama jadi `servel_servel_<vol>`).
            writeln!(out, "  {}:", vol_name).ok();
            writeln!(out, "    name: {}", vol_name).ok();
        }
    }

    writeln!(out).ok();
    writeln!(out, "networks:").ok();
    writeln!(out, "  servel_default:").ok();
    writeln!(out, "    name: servel_default").ok();
    writeln!(out, "    driver: bridge").ok();

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use super::super::services::{PortMap, ServiceDef};

    fn mysql_def() -> ServiceDef {
        ServiceDef {
            id: "mysql".to_string(),
            name: "MySQL".to_string(),
            category: "core".to_string(),
            image: "mysql:8.0".to_string(),
            container_name: "servel_mysql".to_string(),
            ports: vec![PortMap { host: "3306".to_string(), container: "3306".to_string() }],
            environment: {
                let mut m = HashMap::new();
                m.insert("MYSQL_ROOT_PASSWORD".to_string(), "root".to_string());
                m
            },
            volumes: Some(vec!["servel_mysql_data:/var/lib/mysql".to_string()]),
            command: None,
            ram_estimate_mb: 350,
        }
    }

    fn redis_def() -> ServiceDef {
        ServiceDef {
            id: "redis".to_string(),
            name: "Redis".to_string(),
            category: "core".to_string(),
            image: "redis:7-alpine".to_string(),
            container_name: "servel_redis".to_string(),
            ports: vec![PortMap { host: "6379".to_string(), container: "6379".to_string() }],
            environment: HashMap::new(),
            volumes: Some(vec!["servel_redis_data:/data".to_string()]),
            command: None,
            ram_estimate_mb: 64,
        }
    }

    fn mailpit_def() -> ServiceDef {
        ServiceDef {
            id: "mailpit".to_string(),
            name: "Mailpit".to_string(),
            category: "additional".to_string(),
            image: "axllent/mailpit:latest".to_string(),
            container_name: "servel_mailpit".to_string(),
            ports: vec![
                PortMap { host: "1025".to_string(), container: "1025".to_string() },
                PortMap { host: "8025".to_string(), container: "8025".to_string() },
            ],
            environment: HashMap::new(),
            volumes: None,
            command: None,
            ram_estimate_mb: 32,
        }
    }

    #[test]
    fn test_empty_selection() {
        let defs = vec![mysql_def(), redis_def()];
        let yaml = generate_compose(&defs, &[]);

        assert!(yaml.contains("services:"), "harus ada services: block");
        assert!(yaml.contains("networks:"), "harus ada networks: block");
        assert!(yaml.contains("servel_default:"), "harus ada servel_default network");
        assert!(yaml.contains("driver: bridge"), "network harus driver bridge");
        // Tidak ada service yang di-render
        assert!(!yaml.contains("mysql:"), "tidak boleh ada mysql saat empty");
        // Tidak ada volumes top-level
        assert!(!yaml.contains("\nvolumes:"), "tidak boleh ada volumes section saat empty");
    }

    #[test]
    fn test_single_mysql() {
        let defs = vec![mysql_def(), redis_def(), mailpit_def()];
        let selected = vec!["mysql".to_string()];
        let yaml = generate_compose(&defs, &selected);

        // Service block
        assert!(yaml.contains("services:"));
        assert!(yaml.contains("  mysql:"), "mysql service harus ada");
        assert!(yaml.contains("image: mysql:8.0"), "image mysql");
        assert!(yaml.contains("container_name: servel_mysql"), "container_name");
        assert!(yaml.contains("restart: unless-stopped"), "restart policy");

        // Port di-quote
        assert!(yaml.contains("- \"3306:3306\""), "port harus di-quote");

        // Environment list format
        assert!(yaml.contains("- MYSQL_ROOT_PASSWORD=root"), "env KEY=value format");

        // Volume service
        assert!(yaml.contains("- servel_mysql_data:/var/lib/mysql"), "volume service entry");

        // Named volume top-level
        assert!(yaml.contains("\nvolumes:"), "top-level volumes section");
        assert!(yaml.contains("  servel_mysql_data:"), "named volume declaration");

        // Network
        assert!(yaml.contains("networks:"));
        assert!(yaml.contains("  servel_default:"));
        assert!(yaml.contains("    driver: bridge"));

        // Redis tidak ikut
        assert!(!yaml.contains("  redis:"), "redis tidak boleh ter-render");
    }

    #[test]
    fn test_mysql_redis_mailpit_order() {
        let defs = vec![mysql_def(), redis_def(), mailpit_def()];
        let selected = vec![
            "mysql".to_string(),
            "redis".to_string(),
            "mailpit".to_string(),
        ];
        let yaml = generate_compose(&defs, &selected);

        // Semua 3 service ada
        assert!(yaml.contains("  mysql:"), "mysql harus ada");
        assert!(yaml.contains("  redis:"), "redis harus ada");
        assert!(yaml.contains("  mailpit:"), "mailpit harus ada");

        // Order preserved: mysql sebelum redis sebelum mailpit
        let pos_mysql = yaml.find("  mysql:").unwrap();
        let pos_redis = yaml.find("  redis:").unwrap();
        let pos_mailpit = yaml.find("  mailpit:").unwrap();
        assert!(pos_mysql < pos_redis, "mysql harus sebelum redis");
        assert!(pos_redis < pos_mailpit, "redis harus sebelum mailpit");

        // 2 named volumes (mysql + redis), mailpit tidak punya volume
        assert!(yaml.contains("  servel_mysql_data:"), "mysql volume top-level");
        assert!(yaml.contains("  servel_redis_data:"), "redis volume top-level");

        // Env mysql ter-render
        assert!(yaml.contains("- MYSQL_ROOT_PASSWORD=root"), "env mysql");

        // Mailpit punya 2 port
        assert!(yaml.contains("- \"1025:1025\""), "mailpit port smtp");
        assert!(yaml.contains("- \"8025:8025\""), "mailpit port web");

        // Network section
        assert!(yaml.contains("  servel_default:"));
        assert!(yaml.contains("    driver: bridge"));
    }
}
