//! Migrasi data Docker → WSL containers (v1.8).
//!
//! Storage Docker Desktop dan WSLC terpisah, jadi pindah engine membuat
//! database tampak kosong. Wizard ini: dump database user dari container
//! Docker `servel_<id>` ke file sementara di host, lalu restore ke container
//! WSLC dengan jalur yang sama seperti "Restore database".
//!
//! - MySQL: semua database **kecuali** database sistem (`mysql`, `sys`,
//!   `*_schema`) supaya akun/privilege di target tidak tertimpa.
//! - PostgreSQL: `pg_dump -C` per database (bukan `pg_dumpall`, yang memuat
//!   `CREATE ROLE postgres` dan gagal di target karena role sudah ada).

use std::process::Stdio;
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;

use super::{ensure_resolved, RuntimeKind};
use crate::commands::services::ServiceDef;
use crate::commands::util::silent_command;

/// Perintah dump di dalam container Docker. Output = SQL di stdout.
pub fn dump_command(service_id: &str) -> Option<&'static str> {
    match service_id {
        "mysql" => Some(concat!(
            r#"export MYSQL_PWD="${MYSQL_ROOT_PASSWORD:-}"; "#,
            r#"dbs=$(mysql -N -uroot -e 'SHOW DATABASES' | grep -Ev '^(information_schema|performance_schema|mysql|sys)$'); "#,
            r#"[ -z "$dbs" ] && { echo 'tidak ada database user' >&2; exit 3; }; "#,
            r#"mysqldump -uroot --single-transaction --routines --events --triggers --databases $dbs"#
        )),
        "postgres" => Some(concat!(
            r#"U="${POSTGRES_USER:-postgres}"; "#,
            r#"dbs=$(psql -U "$U" -d postgres -Atc "SELECT datname FROM pg_database WHERE NOT datistemplate AND datname <> 'postgres'"); "#,
            r#"[ -z "$dbs" ] && { echo 'tidak ada database user' >&2; exit 3; }; "#,
            r#"for db in $dbs; do pg_dump -U "$U" -C "$db" || exit 1; done"#
        )),
        _ => None,
    }
}

fn emit(app: &AppHandle, source: &str, stream: &str, line: &str) {
    let _ = app.emit("cmd-output", serde_json::json!({ "line": line, "stream": stream, "source": source }));
}

pub async fn migrate_from_docker(app: &AppHandle, defs: &[ServiceDef], service_id: &str) -> Result<(), String> {
    if ensure_resolved().await != RuntimeKind::Wslc {
        return Err("pindahkan engine ke WSL containers dulu (Settings → Container engine)".to_string());
    }
    let cmd = dump_command(service_id)
        .ok_or_else(|| format!("migrasi hanya untuk mysql dan postgres, bukan '{}'", service_id))?;
    let def = defs
        .iter()
        .find(|d| d.id == service_id)
        .ok_or_else(|| format!("service '{}' tidak dikenal", service_id))?;
    let name = &def.container_name;
    let source = service_id.to_uppercase();

    if !super::docker::is_running().await {
        return Err("Docker tidak berjalan — start Docker Desktop dulu untuk mengambil data lama".to_string());
    }

    emit(app, &source, "stdout", &format!("docker start {} (sumber data lama)", name));
    let started = silent_command("docker").args(["start", name]).status().await;
    if !matches!(started, Ok(s) if s.success()) {
        return Err(format!("container Docker {} tidak ditemukan", name));
    }
    // Beri waktu server DB siap di sisi Docker.
    let mut ready = false;
    if let Some(probe) = super::readiness::probe_command(service_id) {
        for _ in 0..60 {
            if super::docker::exec_status(name, probe, std::time::Duration::from_secs(10)).await == Some(0) {
                ready = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
    }
    if !ready {
        return Err(format!("{} di Docker belum siap setelah 120 dtk", name));
    }

    let dump_path = std::env::temp_dir().join(format!("servel-migrate-{}.sql", service_id));
    emit(app, &source, "stdout", &format!("dump dari Docker → {}", dump_path.display()));
    let mut child = silent_command("docker")
        .args(["exec", name, "sh", "-c", cmd])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Gagal menjalankan docker exec: {}", e))?;
    let mut stdout = child.stdout.take().ok_or("stdout docker tidak tersedia")?;
    let mut file = tokio::fs::File::create(&dump_path)
        .await
        .map_err(|e| format!("gagal membuat {}: {}", dump_path.display(), e))?;
    tokio::io::copy(&mut stdout, &mut file)
        .await
        .map_err(|e| format!("gagal menulis dump: {}", e))?;
    file.flush().await.ok();
    drop(file);
    let out = child.wait_with_output().await.map_err(|e| format!("gagal menunggu docker: {}", e))?;
    if !out.status.success() {
        let _ = tokio::fs::remove_file(&dump_path).await;
        return Err(format!("dump gagal: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }

    let result = super::restore::restore(app, defs, service_id, &dump_path.to_string_lossy()).await;
    // Dump bisa berisi data sensitif — jangan ditinggal di temp.
    let _ = tokio::fs::remove_file(&dump_path).await;
    if result.is_ok() {
        emit(app, &source, "stdout", "migrasi selesai. Container Docker lama tidak dihapus — hapus manual bila sudah yakin.");
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dump_commands_exclude_system_databases() {
        let m = dump_command("mysql").unwrap();
        assert!(m.contains("information_schema|performance_schema|mysql|sys"));
        assert!(m.contains("--single-transaction") && m.contains("--databases $dbs"));
        let p = dump_command("postgres").unwrap();
        assert!(p.contains("pg_dump -U \"$U\" -C"));
        assert!(!p.contains("pg_dumpall"), "pg_dumpall memuat CREATE ROLE postgres");
        assert!(p.contains("datname <> 'postgres'"));
        assert!(dump_command("redis").is_none());
    }
}
