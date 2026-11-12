//! Sisa fitur domain `.test` (v1.4–v2.1), yang dihapus di v2.2. Yang
//! dipertahankan hanya yang dibutuhkan untuk **membersihkan** blok terkelola
//! Servel dari file hosts pengguna lama.
//!
//! Bagian **pure** (parse / generate / diff) di file ini **tidak** menyentuh
//! privilege — semua write elevated diisolasi di primitive terpisah (T3).
//! Prinsip keamanan inti (lihat `docs/reports/arch/phase-11-sites-tasks.md` §A2):
//! Servel **hanya** menyentuh baris di dalam blok bertanda sentinel; segala isi
//! hosts di luar blok dipertahankan apa adanya.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

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

/// Timestamp epoch nanodetik — fixed-width untuk era sekarang → sorting leksikal
/// nama file = urutan kronologis. Nanos meminimalkan tabrakan antar-panggilan.
fn epoch_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

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

/// Blok terkelola Servel masih ada di file hosts?
pub fn has_managed_block(content: &str) -> bool {
    find_block_span(content).is_some()
}

/// Hapus blok terkelola Servel dari file hosts (butuh izin admin). Baris di
/// luar blok tidak disentuh. Tidak melakukan apa-apa bila blok sudah tidak ada.
pub fn remove_managed_block() -> Result<(), String> {
    let path = hosts_path();
    let current =
        std::fs::read_to_string(&path).map_err(|e| format!("gagal membaca file hosts: {e}"))?;
    if !has_managed_block(&current) {
        return Ok(());
    }
    write_hosts_elevated(&generate_hosts(&current, &[]))
}

/// Blok terkelola masih ada? `false` juga bila file hosts tidak terbaca.
pub fn managed_block_present() -> bool {
    std::fs::read_to_string(hosts_path())
        .map(|c| has_managed_block(&c))
        .unwrap_or(false)
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
        assert!(out.contains("127.0.0.1  win.test\r\n"));
    }

    #[test]
    fn lf_stays_lf() {
        let out = generate_hosts(SAMPLE, &entries(&[("127.0.0.1", "nix.test")]));
        assert!(!out.contains("\r\n"));
    }

    #[test]
    fn detects_and_removes_managed_block() {
        let hosts = "127.0.0.1 localhost\n# >>> servel managed block >>>\n127.0.0.1  myapp.test\n# <<< servel managed block <<<\n10.0.0.2 nas\n";
        assert!(has_managed_block(hosts));
        let cleaned = generate_hosts(hosts, &[]);
        assert!(!has_managed_block(&cleaned));
        assert!(cleaned.contains("127.0.0.1 localhost") && cleaned.contains("10.0.0.2 nas"));
        assert!(!cleaned.contains("myapp.test"));
        assert!(!has_managed_block("127.0.0.1 localhost\n"));
    }

    #[test]
    fn hosts_path_is_absolute() {
        assert!(hosts_path().is_absolute());
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
}
