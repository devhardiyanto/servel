//! Restore dump `.sql` ke MySQL / PostgreSQL (v1.8) — padanan GUI dari
//! `servel restore`. Engine-agnostik: alur sama untuk WSLC dan Docker, hanya
//! cara menyalin file dan exec yang berbeda (lihat `wslc.rs` / `docker.rs`).
//!
//! Alur: start service → tunggu readiness → salin file ke
//! `/tmp/servel-restore.sql` di container → jalankan client SQL → hapus file.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

use super::{ensure_resolved, readiness, RuntimeKind};
use crate::commands::services::ServiceDef;

pub const CONTAINER_FILE: &str = "/tmp/servel-restore.sql";

/// Perintah restore di dalam container. `None` = service tidak didukung.
pub fn restore_command(service_id: &str) -> Option<String> {
    let f = CONTAINER_FILE;
    match service_id {
        // MYSQL_PWD kosong = tanpa password (default Servel: MYSQL_ALLOW_EMPTY_PASSWORD).
        "mysql" => Some(format!(
            r#"MYSQL_PWD="${{MYSQL_ROOT_PASSWORD:-}}" mysql -uroot < {f} && rm -f {f}"#
        )),
        // ON_ERROR_STOP: psql default tetap exit 0 walau ada error SQL.
        "postgres" => Some(format!(
            r#"psql -v ON_ERROR_STOP=1 -U "${{POSTGRES_USER:-postgres}}" -d postgres -f {f} && rm -f {f}"#
        )),
        _ => None,
    }
}

/// Validasi file host: harus ada, berupa file, dan berekstensi `.sql`.
pub fn validate_dump(path: &Path) -> Result<(), String> {
    let is_sql = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("sql"));
    if !is_sql {
        return Err("file restore harus berekstensi .sql".to_string());
    }
    if !path.is_file() {
        return Err(format!("file tidak ditemukan: {}", path.display()));
    }
    Ok(())
}

fn emit(app: &AppHandle, source: &str, stream: &str, line: &str) {
    let _ = app.emit(
        "cmd-output",
        serde_json::json!({ "line": line, "stream": stream, "source": source }),
    );
}

pub async fn restore(
    app: &AppHandle,
    defs: &[ServiceDef],
    service_id: &str,
    file: &str,
) -> Result<(), String> {
    let cmd = restore_command(service_id).ok_or_else(|| {
        format!(
            "restore hanya untuk mysql dan postgres, bukan '{}'",
            service_id
        )
    })?;
    let path = PathBuf::from(file);
    validate_dump(&path)?;
    let def = defs
        .iter()
        .find(|d| d.id == service_id)
        .ok_or_else(|| format!("service '{}' tidak dikenal", service_id))?;
    let source = service_id.to_uppercase();
    let name = def.container_name.clone();

    emit(
        app,
        &source,
        "stdout",
        &format!("restore {} → {}", path.display(), name),
    );
    super::start(app, defs, &[service_id.to_string()]).await?;

    // Tunggu service siap (probe yang sama dengan status "up").
    let deadline = Instant::now() + readiness::READY_TIMEOUT;
    loop {
        if super::probe(service_id, &name).await != readiness::ProbeOutcome::NotReady {
            break;
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "{} belum siap setelah 120 dtk — cek tab Logs",
                name
            ));
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    emit(app, &source, "stdout", "menyalin file ke container…");
    match ensure_resolved().await {
        RuntimeKind::Wslc => super::wslc::copy_file_into(&name, &path, CONTAINER_FILE).await?,
        RuntimeKind::Docker => super::docker::copy_file_into(&name, &path, CONTAINER_FILE).await?,
    }

    emit(app, &source, "stdout", "menjalankan restore…");
    let r = match ensure_resolved().await {
        RuntimeKind::Wslc => super::wslc::exec_streamed(app, &name, &cmd, &source).await,
        RuntimeKind::Docker => super::docker::exec_streamed(app, &name, &cmd).await,
    };
    match &r {
        Ok(()) => emit(
            app,
            &source,
            "stdout",
            &format!("restore selesai: {}", path.display()),
        ),
        Err(e) => emit(app, &source, "stderr", &format!("restore gagal: {}", e)),
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restore_commands() {
        let m = restore_command("mysql").unwrap();
        assert!(m.contains("mysql -uroot < /tmp/servel-restore.sql"));
        assert!(m.contains(r#"MYSQL_PWD="${MYSQL_ROOT_PASSWORD:-}""#));
        let p = restore_command("postgres").unwrap();
        assert!(p.contains("ON_ERROR_STOP=1") && p.contains("-f /tmp/servel-restore.sql"));
        assert!(restore_command("redis").is_none());
    }

    #[test]
    fn dump_validation() {
        let dir = std::env::temp_dir().join("servel-restore-test");
        std::fs::create_dir_all(&dir).unwrap();
        let ok = dir.join("dump.SQL");
        std::fs::write(&ok, "select 1;").unwrap();
        assert!(validate_dump(&ok).is_ok());
        assert!(validate_dump(&dir.join("missing.sql")).is_err());
        let txt = dir.join("dump.txt");
        std::fs::write(&txt, "x").unwrap();
        assert!(validate_dump(&txt).is_err());
    }
}
