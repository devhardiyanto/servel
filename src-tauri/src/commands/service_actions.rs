//! Menu ⋯ per service: Open shell, Delete data (dengan backup .sql), Remove
//! service (custom). Restart dan Change port memakai start/stop biasa.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager};
use tokio::io::AsyncReadExt;

use crate::commands::services::{load_services_internal, ServiceDef};
use crate::commands::util::emit_log_line;
use crate::runtime::{self, readiness, RuntimeKind};

/// Perintah dump untuk backup sebelum Delete data. `None` = tidak didukung.
pub fn dump_command(service_id: &str) -> Option<&'static str> {
    match service_id {
        "mysql" => Some(r#"MYSQL_PWD="${MYSQL_ROOT_PASSWORD:-}" mysqldump -uroot --all-databases --single-transaction --routines --events --triggers"#),
        "postgres" => Some(r#"pg_dumpall -U "${POSTGRES_USER:-postgres}""#),
        _ => None,
    }
}

/// Nama volume dari entri `nama:/path`.
pub fn volume_names(def: &ServiceDef) -> Vec<String> {
    def.volumes
        .clone()
        .unwrap_or_default()
        .iter()
        .filter_map(|v| v.split_once(':').map(|(n, _)| n.to_string()))
        .filter(|n| !n.is_empty() && !n.contains('/') && !n.contains('\\'))
        .collect()
}

async fn def_of(app: &AppHandle, id: &str) -> Result<(Vec<ServiceDef>, ServiceDef), String> {
    let defs = load_services_internal(app).await?;
    let def = defs.iter().find(|d| d.id == id).cloned().ok_or_else(|| format!("unknown service '{}'", id))?;
    Ok((defs, def))
}

/// Terminal baru di dalam container service (bash bila ada, selain itu sh).
#[tauri::command]
pub async fn service_shell(app: AppHandle, id: String) -> Result<(), String> {
    let (_, def) = def_of(&app, &id).await?;
    let engine = match runtime::ensure_resolved().await {
        RuntimeKind::Wslc => "wslc",
        RuntimeKind::Docker => "docker",
    };
    let args = vec![
        "exec".to_string(),
        "-it".to_string(),
        def.container_name.clone(),
        "sh".to_string(),
        "-c".to_string(),
        "command -v bash >/dev/null && exec bash || exec sh".to_string(),
    ];
    crate::open_in::open_terminal_running(engine, &args, &format!("{} ({})", def.name, def.container_name))
}

async fn wait_ready(def: &ServiceDef) -> Result<(), String> {
    let deadline = Instant::now() + readiness::READY_TIMEOUT;
    loop {
        if runtime::probe(&def.id, &def.container_name).await != readiness::ProbeOutcome::NotReady {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!("{} is not ready after 120 s", def.name));
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

async fn backup(app: &AppHandle, defs: &[ServiceDef], def: &ServiceDef) -> Result<PathBuf, String> {
    let cmd = dump_command(&def.id).ok_or_else(|| format!("{} has no backup", def.name))?;
    let source = def.id.to_uppercase();
    runtime::start(app, defs, std::slice::from_ref(&def.id)).await?;
    wait_ready(def).await?;
    let dir = app.path().download_dir().map_err(|e| format!("no Downloads folder: {}", e))?;
    let stamp = chrono_like_stamp();
    let file = dir.join(format!("kova-{}-{}.sql", def.id, stamp));
    emit_log_line(app, &source, &format!("backup → {}", file.display()));
    let mut child = match runtime::ensure_resolved().await {
        RuntimeKind::Wslc => runtime::wslc::exec_command(&def.container_name, cmd),
        RuntimeKind::Docker => runtime::docker::exec_command(&def.container_name, cmd),
    }
    .spawn()
    .map_err(|e| format!("could not run the backup: {}", e))?;
    let mut out = child.stdout.take().ok_or("no output from the backup")?;
    let mut f = tokio::fs::File::create(&file).await.map_err(|e| format!("could not write {}: {}", file.display(), e))?;
    let copied = tokio::io::copy(&mut out, &mut f).await.map_err(|e| e.to_string())?;
    let mut err = String::new();
    if let Some(mut e) = child.stderr.take() {
        let _ = e.read_to_string(&mut err).await;
    }
    let status = child.wait().await.map_err(|e| e.to_string())?;
    if !status.success() || copied == 0 {
        let _ = tokio::fs::remove_file(&file).await;
        return Err(format!("the backup failed: {}", err.lines().last().unwrap_or("no output")));
    }
    Ok(file)
}

/// `YYYYMMDD-HHMMSS` (UTC) untuk nama file backup, tanpa dependensi tanggal.
pub fn chrono_like_stamp() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    stamp_from(secs)
}

pub fn stamp_from(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    // Civil-from-days (Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{:04}{:02}{:02}-{:02}{:02}{:02}", y, m, d, rem / 3600, (rem % 3600) / 60, rem % 60)
}

async fn wipe(app: &AppHandle, defs: &[ServiceDef], def: &ServiceDef) -> Result<(), String> {
    let source = def.id.to_uppercase();
    let _ = runtime::stop(app, defs, std::slice::from_ref(&def.id)).await;
    let kind = runtime::ensure_resolved().await;
    match kind {
        RuntimeKind::Wslc => runtime::wslc::remove_container(&def.container_name).await?,
        RuntimeKind::Docker => runtime::docker::remove_container(&def.container_name).await?,
    }
    for v in volume_names(def) {
        emit_log_line(app, &source, &format!("deleting volume {}", v));
        match kind {
            RuntimeKind::Wslc => runtime::wslc::remove_volume(&v).await?,
            RuntimeKind::Docker => runtime::docker::remove_volume(&v).await?,
        }
    }
    Ok(())
}

/// Delete data: (backup), stop, hapus container dan volume, start lagi kosong.
/// Mengembalikan path backup bila dibuat.
#[tauri::command]
pub async fn service_delete_data(app: AppHandle, id: String, backup_first: bool) -> Result<Option<String>, String> {
    let (defs, def) = def_of(&app, &id).await?;
    let saved = if backup_first && dump_command(&id).is_some() {
        Some(backup(&app, &defs, &def).await?.display().to_string())
    } else {
        None
    };
    wipe(&app, &defs, &def).await?;
    emit_log_line(&app, &def.id.to_uppercase(), "data deleted; starting with empty volumes");
    runtime::start(&app, &defs, std::slice::from_ref(&def.id)).await?;
    Ok(saved)
}

/// Remove service (custom): hentikan dan hapus container; volume ikut dihapus
/// bila `delete_data`. Definisinya dihapus frontend dari config.
#[tauri::command]
pub async fn service_remove(app: AppHandle, id: String, delete_data: bool) -> Result<(), String> {
    let (defs, def) = def_of(&app, &id).await?;
    if !def.custom {
        return Err(format!("{} is a built-in service and cannot be removed", def.name));
    }
    if delete_data {
        wipe(&app, &defs, &def).await
    } else {
        let _ = runtime::stop(&app, &defs, std::slice::from_ref(&def.id)).await;
        match runtime::ensure_resolved().await {
            RuntimeKind::Wslc => runtime::wslc::remove_container(&def.container_name).await,
            RuntimeKind::Docker => runtime::docker::remove_container(&def.container_name).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dumps_and_volume_names() {
        assert!(dump_command("mysql").unwrap().contains("mysqldump -uroot --all-databases"));
        assert!(dump_command("postgres").unwrap().starts_with("pg_dumpall"));
        assert!(dump_command("redis").is_none());
        let def: ServiceDef = serde_json::from_str(
            r#"{"id":"mysql","name":"MySQL","category":"core","image":"mysql:8.0","containerName":"servel_mysql","ports":[],"volumes":["servel_mysql_data:/var/lib/mysql","/host:/x"],"ramEstimateMb":80}"#,
        )
        .unwrap();
        assert_eq!(volume_names(&def), vec!["servel_mysql_data".to_string()]);
        assert_eq!(stamp_from(1_795_449_600), "20261123-160000");
        assert_eq!(stamp_from(0), "19700101-000000");
    }
}
