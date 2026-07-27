//! Manajemen file hosts sistem untuk fitur Sites (domain lokal → IP).
//!
//! Bagian **pure** (parse / generate / diff) di file ini **tidak** menyentuh
//! privilege — semua write elevated diisolasi di primitive terpisah (T3).
//! Prinsip keamanan inti (lihat `docs/reports/arch/phase-11-sites-tasks.md` §A2):
//! Servel **hanya** menyentuh baris di dalam blok bertanda sentinel; segala isi
//! hosts di luar blok dipertahankan apa adanya.

use super::config::{self, Site};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};

/// Marker awal blok terkelola Servel. Baris di antara marker = milik Servel.
const BEGIN_MARKER: &str = "# >>> servel managed block >>>";
/// Marker akhir blok terkelola Servel.
const END_MARKER: &str = "# <<< servel managed block <<<";

/// Satu entri hosts di dalam managed block: `ip  domain`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HostEntry {
    pub ip: String,
    pub domain: String,
}

/// Hasil diff blok lama vs baru untuk preview konfirmasi di UI.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct DiffResult {
    /// Baris `ip  domain` yang ada di blok baru tapi tidak di lama.
    pub added: Vec<String>,
    /// Baris yang ada di blok lama tapi hilang di baru.
    pub removed: Vec<String>,
    /// Baris yang sama di kedua blok.
    pub unchanged: Vec<String>,
}

/// Path absolut file hosts sistem sesuai OS.
pub fn hosts_path() -> PathBuf {
    #[cfg(windows)]
    {
        // Hormati %SystemRoot% (biasanya C:\Windows) daripada hard-code.
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
        PathBuf::from(root).join("System32\\drivers\\etc\\hosts")
    }
    #[cfg(not(windows))]
    {
        PathBuf::from("/etc/hosts")
    }
}

/// Deteksi gaya newline dominan agar seam yang kita sisipkan konsisten dengan
/// file existing (Windows hosts umumnya CRLF).
fn detect_newline(s: &str) -> &'static str {
    if s.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}

/// Cari span byte `[start, end)` yang menutupi seluruh baris BEGIN..END marker
/// (termasuk newline penutup baris END). `None` bila blok tak ada.
fn find_block_span(s: &str) -> Option<(usize, usize)> {
    let begin = s.find(BEGIN_MARKER)?;
    // Mundur ke awal baris BEGIN (setelah newline sebelumnya, atau awal string).
    let start = s[..begin].rfind('\n').map(|i| i + 1).unwrap_or(0);

    // END harus muncul setelah BEGIN.
    let end_marker = s[begin..].find(END_MARKER).map(|i| begin + i)?;
    let after_marker = end_marker + END_MARKER.len();
    // Sertakan newline penutup baris END (untuk CRLF, '\n' terakhir → '\r' ikut).
    let end = match s[after_marker..].find('\n') {
        Some(i) => after_marker + i + 1,
        None => s.len(),
    };
    Some((start, end))
}

/// Render isi blok (tanpa newline penutup akhir; caller yang menambah).
fn render_block(entries: &[HostEntry], nl: &str) -> String {
    let mut out = String::from(BEGIN_MARKER);
    for e in entries {
        out.push_str(nl);
        out.push_str(&e.ip);
        out.push_str("  ");
        out.push_str(&e.domain);
    }
    out.push_str(nl);
    out.push_str(END_MARKER);
    out
}

fn trim_eol(s: &str) -> &str {
    s.trim_end_matches(['\n', '\r'])
}

/// Ekstrak entri hosts di dalam managed block. Baris marker, komentar, dan
/// baris kosong di-skip. Baris tak ada di dalam blok diabaikan sepenuhnya.
pub fn parse_managed_block(content: &str) -> Vec<HostEntry> {
    let (start, end) = match find_block_span(content) {
        Some(span) => span,
        None => return Vec::new(),
    };

    let mut out = Vec::new();
    for line in content[start..end].lines() {
        let t = line.trim();
        // Skip marker & komentar (keduanya diawali '#') dan baris kosong.
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        let mut parts = t.split_whitespace();
        if let (Some(ip), Some(domain)) = (parts.next(), parts.next()) {
            out.push(HostEntry {
                ip: ip.to_string(),
                domain: domain.to_string(),
            });
        }
    }
    out
}

/// Hasilkan konten hosts baru = hosts existing dengan managed block Servel
/// di-replace/inject dari `entries`. Semua byte di luar blok dipertahankan.
///
/// Aturan:
/// - Blok ada + `entries` non-kosong → replace **in-place** (byte luar identik).
/// - Blok ada + `entries` kosong → hapus blok bersih (marker ikut hilang).
/// - Blok tak ada + `entries` non-kosong → append blok di akhir file.
/// - Blok tak ada + `entries` kosong → kembalikan existing apa adanya (no-op).
///
/// Idempoten: `generate_hosts(generate_hosts(e, x), x) == generate_hosts(e, x)`.
pub fn generate_hosts(existing: &str, entries: &[HostEntry]) -> String {
    let nl = detect_newline(existing);

    match find_block_span(existing) {
        Some((start, end)) => {
            let before = &existing[..start];
            let after = &existing[end..];

            if entries.is_empty() {
                // Hapus blok + kolaps whitespace milik kita di kedua seam.
                let mut out = String::from(trim_eol(before));
                let after_trimmed = after.trim_start_matches(['\n', '\r']);
                if !after_trimmed.is_empty() {
                    if !out.is_empty() {
                        out.push_str(nl);
                        out.push_str(nl);
                    }
                    out.push_str(after_trimmed);
                }
                if !out.is_empty() {
                    out.push_str(nl);
                }
                out
            } else {
                // Replace in-place: `before` diakhiri newline (start = awal baris),
                // blok baru menggantikan span lama termasuk newline penutupnya.
                let block = render_block(entries, nl);
                format!("{before}{block}{nl}{after}")
            }
        }
        None => {
            if entries.is_empty() {
                existing.to_string()
            } else {
                let block = render_block(entries, nl);
                let base = trim_eol(existing);
                if base.is_empty() {
                    format!("{block}{nl}")
                } else {
                    // Satu baris kosong pemisah antara konten user & blok kita.
                    format!("{base}{nl}{nl}{block}{nl}")
                }
            }
        }
    }
}

fn fmt_entry(e: &HostEntry) -> String {
    format!("{}  {}", e.ip, e.domain)
}

// --- Backup (T2) -------------------------------------------------------------
// Sebelum tiap write ke hosts, salin hosts sekarang → app-data. Simpan 5
// terakhir, rotasi otomatis. Backup disimpan di app-data (bukan system dir).

const BACKUP_PREFIX: &str = "hosts.servel.";
const BACKUP_SUFFIX: &str = ".bak";
const BACKUP_KEEP: usize = 5;

/// Timestamp epoch nanodetik — fixed-width untuk era sekarang → sorting leksikal
/// nama file = urutan kronologis. Nanos meminimalkan tabrakan antar-panggilan.
fn epoch_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

/// Daftar file backup di `dir`, terurut kronologis (lama → baru).
pub fn list_backups(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = match std::fs::read_dir(dir) {
        Ok(rd) => rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with(BACKUP_PREFIX) && n.ends_with(BACKUP_SUFFIX))
                    .unwrap_or(false)
            })
            .collect(),
        Err(_) => Vec::new(),
    };
    v.sort();
    v
}

/// Sisakan `keep` backup terbaru; hapus sisanya (yang terlama).
fn rotate_backups(dir: &Path, keep: usize) -> Result<(), String> {
    let all = list_backups(dir);
    if all.len() <= keep {
        return Ok(());
    }
    for p in &all[..all.len() - keep] {
        std::fs::remove_file(p).map_err(|e| format!("gagal rotasi backup: {e}"))?;
    }
    Ok(())
}

/// Backup `hosts` ke `backup_dir` (timestamped) lalu rotasi simpan `BACKUP_KEEP`.
/// Return path backup yang dibuat.
pub fn backup_hosts_to_dir(hosts: &Path, backup_dir: &Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(backup_dir).map_err(|e| format!("gagal buat backup dir: {e}"))?;
    let name = format!("{BACKUP_PREFIX}{}{BACKUP_SUFFIX}", epoch_nanos());
    let dest = backup_dir.join(name);
    std::fs::copy(hosts, &dest).map_err(|e| format!("gagal backup hosts: {e}"))?;
    rotate_backups(backup_dir, BACKUP_KEEP)?;
    Ok(dest)
}

/// Diff managed block antara dua konten hosts (lama vs baru) untuk preview UI.
pub fn diff_block(old_hosts: &str, new_hosts: &str) -> DiffResult {
    let old: Vec<String> = parse_managed_block(old_hosts)
        .iter()
        .map(fmt_entry)
        .collect();
    let new: Vec<String> = parse_managed_block(new_hosts)
        .iter()
        .map(fmt_entry)
        .collect();

    let added = new.iter().filter(|l| !old.contains(l)).cloned().collect();
    let removed = old.iter().filter(|l| !new.contains(l)).cloned().collect();
    let unchanged = old.iter().filter(|l| new.contains(l)).cloned().collect();

    DiffResult {
        added,
        removed,
        unchanged,
    }
}

// --- Elevated write (T3) -----------------------------------------------------
// Satu-satunya titik yang butuh privilege. Diisolasi di sini; sisa modul murni.
// Jalur Windows terbukti lewat spike T0 (lihat arch doc §B).
//
// Alur umum: tulis konten final ke temp non-privileged → jalankan copy temp→hosts
// dengan elevation OS-spesifik → hosts hanya tersentuh oleh langkah elevated,
// sehingga cancel/tolak = no-op (hosts utuh).

/// Sentinel yang dikembalikan di Linux tanpa `pkexec` — frontend memicu fallback
/// instruksi manual (paste konten hosts). Lihat arch doc §C O4.
#[cfg(target_os = "linux")]
pub const PKEXEC_UNAVAILABLE: &str = "PKEXEC_UNAVAILABLE";

/// Tulis konten ke file temp (UTF-8, tanpa BOM) di luar area privileged.
fn write_temp_content(content: &str) -> Result<PathBuf, String> {
    let temp = std::env::temp_dir().join(format!("servel_hosts_{}.new", epoch_nanos()));
    // std::fs::write menulis byte mentah → tak ada BOM (hosts harus polos).
    std::fs::write(&temp, content.as_bytes())
        .map_err(|e| format!("gagal tulis temp hosts: {e}"))?;
    Ok(temp)
}

/// Escape untuk string ber-quote-tunggal PowerShell (`'` → `''`).
#[cfg(windows)]
fn ps_single_quote(s: &str) -> String {
    s.replace('\'', "''")
}

/// Bangun perintah outer PowerShell yang memicu UAC via `Start-Process -Verb RunAs`
/// dan mem-propagate exit code. Cancel UAC melempar → di-catch → exit 1223
/// (ERROR_CANCELLED) sehingga bisa dibedakan dari kegagalan lain.
#[cfg(windows)]
fn build_runas_command(worker: &str, temp: &str, hosts: &str) -> String {
    format!(
        "try {{ $p = Start-Process -FilePath 'powershell.exe' -ArgumentList \
         '-NoProfile','-ExecutionPolicy','Bypass','-File','{}','{}','{}' \
         -Verb RunAs -Wait -PassThru; exit $p.ExitCode }} catch {{ exit 1223 }}",
        ps_single_quote(worker),
        ps_single_quote(temp),
        ps_single_quote(hosts),
    )
}

/// Tulis `content` ke file hosts sistem dengan elevation. Windows-first (terbukti);
/// macOS/Linux best-effort. Cancel/tolak elevation → `Err` bersih, hosts tak tersentuh.
#[cfg(windows)]
pub fn write_hosts_elevated(content: &str) -> Result<(), String> {
    let hosts = hosts_path();
    let temp = write_temp_content(content)?;

    // Worker elevated = copy temp→hosts (persis jalur spike T0).
    let worker = std::env::temp_dir().join(format!("servel_elev_{}.ps1", epoch_nanos()));
    let worker_body = "param([string]$Src,[string]$Dst)\r\n\
        try { Copy-Item -LiteralPath $Src -Destination $Dst -Force -ErrorAction Stop; exit 0 } \
        catch { exit 1 }\r\n";
    std::fs::write(&worker, worker_body).map_err(|e| {
        let _ = std::fs::remove_file(&temp);
        format!("gagal tulis worker elevation: {e}")
    })?;

    let cmd = build_runas_command(
        &worker.to_string_lossy(),
        &temp.to_string_lossy(),
        &hosts.to_string_lossy(),
    );
    let status = Command::new("powershell.exe")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &cmd])
        .status();

    // Bersihkan artefak temp apa pun hasilnya.
    let _ = std::fs::remove_file(&temp);
    let _ = std::fs::remove_file(&worker);

    let status = status.map_err(|e| format!("gagal menjalankan powershell: {e}"))?;
    match status.code() {
        Some(0) => Ok(()),
        Some(1223) => Err("Penulisan hosts dibatalkan (UAC ditolak).".to_string()),
        Some(1) => Err("Gagal menyalin ke file hosts (proses elevated error).".to_string()),
        Some(c) => Err(format!("Penulisan hosts gagal (kode {c}).")),
        None => Err("Proses elevation dihentikan tanpa kode keluar.".to_string()),
    }
}

/// macOS: dialog auth native via `osascript`. Cancel → non-zero → `Err`.
#[cfg(target_os = "macos")]
pub fn write_hosts_elevated(content: &str) -> Result<(), String> {
    let hosts = hosts_path();
    let temp = write_temp_content(content)?;
    // AppleScript double-quoted; escape backslash & double-quote pada path.
    let esc = |s: String| s.replace('\\', "\\\\").replace('"', "\\\"");
    let script = format!(
        "do shell script \"cp '{}' '{}'\" with administrator privileges",
        esc(temp.to_string_lossy().into_owned()),
        esc(hosts.to_string_lossy().into_owned()),
    );
    let status = Command::new("osascript").arg("-e").arg(&script).status();
    let _ = std::fs::remove_file(&temp);
    let status = status.map_err(|e| format!("gagal menjalankan osascript: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err("Penulisan hosts dibatalkan atau gagal (macOS).".to_string())
    }
}

/// Linux: `pkexec` (polkit) bila tersedia; jika tidak → `Err(PKEXEC_UNAVAILABLE)`
/// agar frontend menampilkan fallback instruksi manual (arch doc §C O4).
#[cfg(target_os = "linux")]
pub fn write_hosts_elevated(content: &str) -> Result<(), String> {
    let hosts = hosts_path();
    let has_pkexec = Command::new("sh")
        .arg("-c")
        .arg("command -v pkexec")
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !has_pkexec {
        return Err(PKEXEC_UNAVAILABLE.to_string());
    }
    let temp = write_temp_content(content)?;
    let status = Command::new("pkexec")
        .arg("cp")
        .arg(&temp)
        .arg(&hosts)
        .status();
    let _ = std::fs::remove_file(&temp);
    let status = status.map_err(|e| format!("gagal menjalankan pkexec: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err("Penulisan hosts dibatalkan atau gagal (Linux).".to_string())
    }
}

// --- Commands + event (T4) ---------------------------------------------------
// Event `sites-changed` (kebab-case) di-emit setelah hosts berubah.

/// Status sinkronisasi Sites untuk UI: apakah blok hosts == proyeksi sites,
/// diff yang akan diterapkan, dan daftar backup untuk aksi restore.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SitesStatus {
    /// `true` bila hosts sudah cocok dengan proyeksi sites (tak perlu apply).
    pub in_sync: bool,
    /// Perubahan blok (current hosts → hasil generate) untuk preview konfirmasi.
    pub diff: DiffResult,
    /// Nama file backup, terbaru dulu (untuk dropdown restore).
    pub backups: Vec<String>,
}

/// Proyeksi sites `enabled` → entri hosts. Sites disabled tidak masuk blok.
fn project_sites(sites: &[Site]) -> Vec<HostEntry> {
    sites
        .iter()
        .filter(|s| s.enabled)
        .map(|s| HostEntry {
            ip: s.ip.clone(),
            domain: s.domain.clone(),
        })
        .collect()
}

/// Direktori backup hosts (co-located dengan config.json di app-config dir).
fn backups_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("gagal resolve app_config_dir: {e}"))?;
    Ok(dir.join("hosts-backups"))
}

/// Nama backup dianggap valid bila tanpa separator path & sesuai pola penamaan
/// (cegah path traversal saat restore).
fn is_valid_backup_name(name: &str) -> bool {
    !name.contains('/')
        && !name.contains('\\')
        && !name.contains("..")
        && name.starts_with(BACKUP_PREFIX)
        && name.ends_with(BACKUP_SUFFIX)
}

fn backup_names_newest_first(app: &AppHandle) -> Vec<String> {
    let dir = match backups_dir(app) {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };
    let mut names: Vec<String> = list_backups(&dir)
        .iter()
        .filter_map(|p| p.file_name().and_then(|n| n.to_str()).map(String::from))
        .collect();
    names.reverse(); // list_backups: lama→baru; UI mau baru dulu.
    names
}

#[tauri::command]
pub async fn sites_status(app: AppHandle) -> Result<SitesStatus, String> {
    let cfg = config::load_config(&app)?;
    let desired = project_sites(&cfg.sites);
    let existing = std::fs::read_to_string(hosts_path()).unwrap_or_default();
    let generated = generate_hosts(&existing, &desired);
    Ok(SitesStatus {
        in_sync: existing == generated,
        diff: diff_block(&existing, &generated),
        backups: backup_names_newest_first(&app),
    })
}

#[tauri::command]
pub async fn sites_apply(app: AppHandle) -> Result<DiffResult, String> {
    let cfg = config::load_config(&app)?;
    let desired = project_sites(&cfg.sites);
    let hosts = hosts_path();
    let existing = std::fs::read_to_string(&hosts).unwrap_or_default();
    let generated = generate_hosts(&existing, &desired);
    let diff = diff_block(&existing, &generated);

    // Sudah sinkron → tak perlu elevation (UAC) sama sekali.
    if existing == generated {
        return Ok(diff);
    }

    // Backup hosts sekarang SEBELUM write (rotasi 5) lalu tulis elevated.
    backup_hosts_to_dir(&hosts, &backups_dir(&app)?)?;
    write_hosts_elevated(&generated)?;
    let _ = app.emit("sites-changed", ());
    Ok(diff)
}

#[tauri::command]
pub async fn sites_restore(app: AppHandle, backup_id: String) -> Result<(), String> {
    if !is_valid_backup_name(&backup_id) {
        return Err("Nama backup tidak valid.".to_string());
    }
    let path = backups_dir(&app)?.join(&backup_id);
    let content = std::fs::read_to_string(&path).map_err(|e| format!("gagal baca backup: {e}"))?;
    write_hosts_elevated(&content)?;
    let _ = app.emit("sites-changed", ());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(pairs: &[(&str, &str)]) -> Vec<HostEntry> {
        pairs
            .iter()
            .map(|(ip, d)| HostEntry {
                ip: ip.to_string(),
                domain: d.to_string(),
            })
            .collect()
    }

    const SAMPLE: &str = "# Copyright\n127.0.0.1  localhost\n::1  localhost\n";

    #[test]
    fn inject_when_no_marker() {
        let out = generate_hosts(SAMPLE, &entries(&[("127.0.0.1", "myapp.test")]));
        // Konten existing utuh di awal.
        assert!(out.starts_with(SAMPLE));
        assert!(out.contains(BEGIN_MARKER));
        assert!(out.contains(END_MARKER));
        assert!(out.contains("127.0.0.1  myapp.test"));
        assert!(out.ends_with('\n'));
    }

    #[test]
    fn replace_when_marker_exists() {
        let first = generate_hosts(SAMPLE, &entries(&[("127.0.0.1", "old.test")]));
        let second = generate_hosts(&first, &entries(&[("127.0.0.1", "new.test")]));
        assert!(second.contains("new.test"));
        assert!(!second.contains("old.test"));
        // Hanya ada satu blok.
        assert_eq!(second.matches(BEGIN_MARKER).count(), 1);
        assert_eq!(second.matches(END_MARKER).count(), 1);
    }

    #[test]
    fn preserves_lines_outside_block() {
        let existing = "# header\n10.0.0.5  custom.host\n# footer comment\n";
        let out = generate_hosts(existing, &entries(&[("127.0.0.1", "app.test")]));
        assert!(out.contains("10.0.0.5  custom.host"));
        assert!(out.contains("# footer comment"));
        assert!(out.contains("# header"));
    }

    #[test]
    fn idempotent_repeated_apply() {
        let e = entries(&[("127.0.0.1", "a.test"), ("127.0.0.1", "b.test")]);
        let once = generate_hosts(SAMPLE, &e);
        let twice = generate_hosts(&once, &e);
        assert_eq!(once, twice);
    }

    #[test]
    fn empty_entries_removes_block_cleanly() {
        let with_block = generate_hosts(SAMPLE, &entries(&[("127.0.0.1", "gone.test")]));
        assert!(with_block.contains(BEGIN_MARKER));
        let removed = generate_hosts(&with_block, &[]);
        assert!(!removed.contains(BEGIN_MARKER));
        assert!(!removed.contains(END_MARKER));
        assert!(!removed.contains("gone.test"));
        // Konten user tetap ada.
        assert!(removed.contains("127.0.0.1  localhost"));
    }

    #[test]
    fn empty_entries_no_block_is_noop() {
        assert_eq!(generate_hosts(SAMPLE, &[]), SAMPLE);
    }

    #[test]
    fn crlf_preserved() {
        let crlf = "# head\r\n127.0.0.1  localhost\r\n";
        let out = generate_hosts(crlf, &entries(&[("127.0.0.1", "win.test")]));
        assert!(out.contains("\r\n"));
        // Tidak menyisipkan bare LF di seam blok.
        assert!(!out.contains("test\n#") || out.contains("test\r\n#"));
        // Round-trip parse tetap benar meski CRLF.
        let parsed = parse_managed_block(&out);
        assert_eq!(parsed, entries(&[("127.0.0.1", "win.test")]));
    }

    #[test]
    fn lf_stays_lf() {
        let out = generate_hosts(SAMPLE, &entries(&[("127.0.0.1", "nix.test")]));
        assert!(!out.contains("\r\n"));
    }

    #[test]
    fn parse_extracts_only_block_entries() {
        let existing = "1.2.3.4  outside.host\n";
        let out = generate_hosts(
            existing,
            &entries(&[("127.0.0.1", "app.test"), ("192.168.1.10", "vm.test")]),
        );
        let parsed = parse_managed_block(&out);
        assert_eq!(
            parsed,
            entries(&[("127.0.0.1", "app.test"), ("192.168.1.10", "vm.test")])
        );
        // Baris di luar blok tidak ikut ter-parse.
        assert!(!parsed.iter().any(|e| e.domain == "outside.host"));
    }

    #[test]
    fn parse_empty_when_no_block() {
        assert!(parse_managed_block(SAMPLE).is_empty());
    }

    #[test]
    fn diff_reports_added_removed_unchanged() {
        let old = generate_hosts(
            SAMPLE,
            &entries(&[("127.0.0.1", "keep.test"), ("127.0.0.1", "drop.test")]),
        );
        let new = generate_hosts(
            SAMPLE,
            &entries(&[("127.0.0.1", "keep.test"), ("127.0.0.1", "fresh.test")]),
        );
        let d = diff_block(&old, &new);
        assert_eq!(d.added, vec!["127.0.0.1  fresh.test"]);
        assert_eq!(d.removed, vec!["127.0.0.1  drop.test"]);
        assert_eq!(d.unchanged, vec!["127.0.0.1  keep.test"]);
    }

    #[test]
    fn hosts_path_is_absolute() {
        assert!(hosts_path().is_absolute());
    }

    fn site(domain: &str, ip: &str, enabled: bool) -> Site {
        Site {
            id: domain.to_string(),
            domain: domain.to_string(),
            ip: ip.to_string(),
            enabled,
            // Hosts tak peduli target proxy — itu urusan lapisan proxy.
            target: None,
        }
    }

    #[test]
    fn project_sites_only_enabled() {
        let sites = vec![
            site("on.test", "127.0.0.1", true),
            site("off.test", "127.0.0.1", false),
            site("vm.test", "192.168.1.5", true),
        ];
        let projected = project_sites(&sites);
        assert_eq!(
            projected,
            entries(&[("127.0.0.1", "on.test"), ("192.168.1.5", "vm.test")])
        );
    }

    #[test]
    fn valid_backup_name_guards_traversal() {
        assert!(is_valid_backup_name("hosts.servel.123.bak"));
        assert!(!is_valid_backup_name("../hosts.servel.123.bak"));
        assert!(!is_valid_backup_name("hosts.servel.123.bak/../evil"));
        assert!(!is_valid_backup_name("random.txt"));
        assert!(!is_valid_backup_name("..\\windows\\system32\\hosts"));
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let mut d = std::env::temp_dir();
        d.push(format!("servel_hosts_test_{}_{}", tag, epoch_nanos()));
        d
    }

    #[test]
    fn backup_copies_hosts_content() {
        let dir = temp_dir("copy");
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("hosts");
        std::fs::write(&src, "127.0.0.1  localhost\n").unwrap();
        let backup_dir = dir.join("backups");

        let dest = backup_hosts_to_dir(&src, &backup_dir).expect("backup gagal");
        assert!(dest.exists());
        assert_eq!(
            std::fs::read_to_string(&dest).unwrap(),
            "127.0.0.1  localhost\n"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_temp_content_has_no_bom() {
        let temp = write_temp_content("127.0.0.1  a.test\n").expect("tulis temp gagal");
        let bytes = std::fs::read(&temp).unwrap();
        // Tak boleh diawali UTF-8 BOM (EF BB BF).
        assert_ne!(&bytes[..3.min(bytes.len())], &[0xEF, 0xBB, 0xBF][..]);
        assert_eq!(String::from_utf8(bytes).unwrap(), "127.0.0.1  a.test\n");
        let _ = std::fs::remove_file(&temp);
    }

    #[cfg(windows)]
    #[test]
    fn ps_single_quote_escapes() {
        assert_eq!(ps_single_quote("a'b"), "a''b");
        assert_eq!(ps_single_quote("plain"), "plain");
    }

    #[cfg(windows)]
    #[test]
    fn runas_command_shape() {
        let cmd = build_runas_command("C:\\w.ps1", "C:\\t.new", "C:\\Windows\\hosts");
        assert!(cmd.contains("-Verb RunAs"));
        assert!(cmd.contains("-Wait -PassThru"));
        // Cancel UAC → exit 1223 (ERROR_CANCELLED).
        assert!(cmd.contains("catch { exit 1223 }"));
        assert!(cmd.contains("C:\\w.ps1"));
        assert!(cmd.contains("C:\\Windows\\hosts"));
    }

    #[test]
    fn rotation_keeps_only_newest() {
        let dir = temp_dir("rotate");
        std::fs::create_dir_all(&dir).unwrap();
        // Buat 8 backup ber-timestamp menaik (fixed width) → terlama 3 harus dihapus.
        for i in 0..8u32 {
            let name = format!("{BACKUP_PREFIX}{:020}{BACKUP_SUFFIX}", i);
            std::fs::write(dir.join(name), format!("v{i}")).unwrap();
        }
        rotate_backups(&dir, BACKUP_KEEP).expect("rotasi gagal");

        let remaining = list_backups(&dir);
        assert_eq!(remaining.len(), BACKUP_KEEP);
        // Yang tersisa = 5 terbaru (index 3..8).
        let names: Vec<String> = remaining
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert!(names[0].contains(&format!("{:020}", 3)));
        assert!(names[4].contains(&format!("{:020}", 7)));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
