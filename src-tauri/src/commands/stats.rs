use serde::Serialize;
use std::process::Stdio;
use tokio::process::Command;

/// Single container memory snapshot — payload entry untuk event `container-stats-changed`.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContainerMemStat {
    pub id: String,
    pub mem_mb: u64,
}

/// Parse string `MemUsage` dari `docker stats --format json` jadi MB (integer, rounded).
///
/// Format input contoh: `"123.4MiB / 1.95GiB"`, `"512KiB / 1GiB"`, `"1.2GiB / 8GiB"`.
/// Ambil bagian sebelum `/`, parse angka + unit. Konversi binary unit (MiB = 2^20 bytes)
/// lalu bagi 1_000_000 untuk dapat MB desimal — ini yang biasa dipakai user-facing display
/// (sama seperti Task Manager). Round ke nearest integer.
pub fn parse_mem_usage_to_mb(raw: &str) -> Option<u64> {
    let usage = raw.split('/').next()?.trim();
    if usage.is_empty() {
        return None;
    }

    // Pisahkan angka dan unit. Unit selalu di belakang dan terdiri dari huruf.
    let split_idx = usage
        .find(|c: char| c.is_alphabetic())
        .unwrap_or(usage.len());
    let (num_part, unit_part) = usage.split_at(split_idx);
    let num: f64 = num_part.trim().parse().ok()?;

    let bytes: f64 = match unit_part.trim() {
        "B" | "" => num,
        "KiB" | "kiB" => num * 1024.0,
        "MiB" => num * 1024.0 * 1024.0,
        "GiB" => num * 1024.0 * 1024.0 * 1024.0,
        "TiB" => num * 1024.0 * 1024.0 * 1024.0 * 1024.0,
        // Decimal unit (jarang muncul dari docker stats, tapi guard untuk amannya).
        "kB" | "KB" => num * 1000.0,
        "MB" => num * 1_000_000.0,
        "GB" => num * 1_000_000_000.0,
        _ => return None,
    };

    let mb = bytes / 1_000_000.0;
    Some(mb.round().max(0.0) as u64)
}

/// Parse line-delimited JSON output dari `docker stats --no-stream --format json`.
/// Filter ke container yang punya prefix `servel_` (selaras dengan filter polling status).
/// Strip prefix dari `Name` untuk dapat service id.
pub fn parse_docker_stats_json(stdout: &str) -> Vec<ContainerMemStat> {
    let mut result = Vec::new();

    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let Ok(val) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };

        let name = val.get("Name").and_then(|v| v.as_str()).unwrap_or("");
        let Some(id) = name.strip_prefix("servel_") else {
            continue;
        };

        let mem_raw = val.get("MemUsage").and_then(|v| v.as_str()).unwrap_or("");
        let Some(mem_mb) = parse_mem_usage_to_mb(mem_raw) else {
            continue;
        };

        result.push(ContainerMemStat {
            id: id.to_string(),
            mem_mb,
        });
    }

    result
}

/// Panggil `docker stats --no-stream --format json` (tanpa `--filter` agar kompatibel
/// di Docker lama). Filter prefix `servel_` dilakukan di Rust saat parse.
pub async fn fetch_container_stats() -> Result<Vec<ContainerMemStat>, String> {
    #[cfg_attr(not(target_os = "windows"), allow(unused_mut))]
    let mut cmd = Command::new("docker");
    cmd.args(["stats", "--no-stream", "--format", "json"])
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
        .map_err(|e| format!("Gagal jalankan docker stats: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("docker stats gagal: {}", stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(parse_docker_stats_json(&stdout))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_mib() {
        assert_eq!(parse_mem_usage_to_mb("123.4MiB / 1.95GiB"), Some(129));
    }

    #[test]
    fn parse_gib() {
        // 1.2 GiB = 1.2 * 1024^3 / 1_000_000 = ~1288.49 MB -> 1288
        assert_eq!(parse_mem_usage_to_mb("1.2GiB / 8GiB"), Some(1288));
    }

    #[test]
    fn parse_kib() {
        // 512 KiB = 524288 B = 0.524 MB -> 1 (round)
        assert_eq!(parse_mem_usage_to_mb("512KiB / 1GiB"), Some(1));
    }

    #[test]
    fn parse_with_spaces() {
        assert_eq!(parse_mem_usage_to_mb("  256MiB  /  4GiB  "), Some(268));
    }

    #[test]
    fn parse_bytes() {
        assert_eq!(parse_mem_usage_to_mb("100B / 1MiB"), Some(0));
    }

    #[test]
    fn parse_unknown_unit_returns_none() {
        assert_eq!(parse_mem_usage_to_mb("123XB / 1GiB"), None);
    }

    #[test]
    fn parse_empty_returns_none() {
        assert_eq!(parse_mem_usage_to_mb(""), None);
        assert_eq!(parse_mem_usage_to_mb("/ 1GiB"), None);
    }

    #[test]
    fn parse_stats_lines_filters_prefix_and_strips() {
        let stdout = concat!(
            r#"{"Name":"servel_mysql","MemUsage":"345MiB / 2GiB"}"#,
            "\n",
            r#"{"Name":"other_container","MemUsage":"100MiB / 2GiB"}"#,
            "\n",
            r#"{"Name":"servel_redis","MemUsage":"12.5MiB / 2GiB"}"#,
            "\n",
            "",
            "\n",
            "not-json",
            "\n",
        );

        let parsed = parse_docker_stats_json(stdout);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].id, "mysql");
        assert_eq!(parsed[0].mem_mb, 362); // 345 MiB ~ 361.7 MB -> 362
        assert_eq!(parsed[1].id, "redis");
        assert_eq!(parsed[1].mem_mb, 13);
    }

    #[test]
    fn parse_stats_skips_invalid_mem_usage() {
        let stdout = r#"{"Name":"servel_broken","MemUsage":"garbage"}"#;
        assert!(parse_docker_stats_json(stdout).is_empty());
    }
}
