//! PHP extensions dan sertifikat HTTPS per versi PHP (v2.3).
//!
//! `php.ini` berlaku per instalasi PHP, jadi perubahan di sini berlaku untuk
//! semua project yang memakai versi itu, di Servel maupun di terminal.
//!
//! Setiap penulisan:
//! 1. mem-backup `php.ini` (`php.ini.servel-<epoch>.bak`, 5 terakhir),
//! 2. menulis perubahan,
//! 3. menjalankan `php -m` lagi. Bila PHP gagal start atau extension yang
//!    diminta tidak berubah status, `php.ini` dikembalikan dari backup.
//!
//! Hanya extension yang file-nya sudah ada di folder extension yang bisa
//! diaktifkan; Servel tidak mengunduh extension (PECL).

use serde::Serialize;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::sync::Mutex;

use crate::commands::util::silent_command;

/// Satu operasi tulis `php.ini` pada satu waktu.
static WRITE_LOCK: Mutex<()> = Mutex::const_new(());

const BACKUP_PREFIX: &str = "php.ini.servel-";
const BACKUP_SUFFIX: &str = ".bak";
const BACKUP_KEEP: usize = 5;

/// Extension yang dimuat lewat `zend_extension=`, bukan `extension=`.
const ZEND_EXTENSIONS: &[&str] = &["opcache", "xdebug"];

// ---------------------------------------------------------------------------
// Bagian murni: parse dan ubah teks php.ini / output php
// ---------------------------------------------------------------------------

fn newline_of(s: &str) -> &'static str {
    if s.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}

/// Nama extension dari nilai `extension=` / nama file:
/// `php_curl.dll`, `"C:\php\ext\php_xdebug.dll"`, `curl.so`, `curl` → `curl`.
pub fn ext_name(value: &str) -> String {
    let v = value.trim().trim_matches(['"', '\'']);
    let base = v.rsplit(['/', '\\']).next().unwrap_or(v);
    let base = base
        .strip_suffix(".dll")
        .or_else(|| base.strip_suffix(".so"))
        .unwrap_or(base);
    base.strip_prefix("php_")
        .unwrap_or(base)
        .to_ascii_lowercase()
}

/// Nama modul dari `php -m` → nama extension. `Zend OPcache` → `opcache`.
pub fn module_name(line: &str) -> String {
    let l = line.trim().to_ascii_lowercase();
    match l.as_str() {
        "zend opcache" => "opcache".to_string(),
        _ => l,
    }
}

/// Modul yang dimuat menurut `php -m`.
pub fn parse_modules(stdout: &str) -> BTreeSet<String> {
    stdout
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('['))
        .map(module_name)
        .collect()
}

/// Peringatan startup PHP (extension gagal dimuat dan sejenisnya).
pub fn startup_warning(output: &str) -> Option<String> {
    output
        .lines()
        .map(str::trim)
        .find(|l| l.contains("PHP Startup:") || l.contains("Unable to load dynamic library"))
        .map(|l| l.to_string())
}

/// Baris `extension=` / `zend_extension=` (aktif atau dikomentari dengan `;`).
/// Mengembalikan (aktif?, zend?, nama).
fn parse_ext_line(line: &str) -> Option<(bool, bool, String)> {
    let t = line.trim_start();
    let (active, rest) = match t.strip_prefix(';') {
        Some(r) => (false, r.trim_start()),
        None => (true, t),
    };
    let (zend, rest) = match rest.strip_prefix("zend_extension") {
        Some(r) => (true, r),
        None => (false, rest.strip_prefix("extension")?),
    };
    let value = rest.trim_start().strip_prefix('=')?;
    // Buang komentar di ujung baris (`extension=curl ; catatan`).
    let value = value.split(';').next().unwrap_or("").trim();
    if value.is_empty() {
        return None;
    }
    Some((active, zend, ext_name(value)))
}

/// Aktifkan atau nonaktifkan extension di teks `php.ini`.
///
/// - Aktifkan: baris komentar yang sesuai di-uncomment (satu saja); kalau tidak
///   ada, `extension=<nama>` ditambahkan di akhir.
/// - Nonaktifkan: semua baris aktif yang sesuai diberi `;`.
///
/// Baris lain tidak disentuh dan gaya baris (CRLF/LF) dipertahankan.
pub fn set_extension(ini: &str, name: &str, enable: bool) -> String {
    let nl = newline_of(ini);
    let name = name.to_ascii_lowercase();
    let mut lines: Vec<String> = ini.lines().map(str::to_string).collect();
    let matches: Vec<(usize, bool, bool)> = lines
        .iter()
        .enumerate()
        .filter_map(|(i, l)| {
            parse_ext_line(l)
                .filter(|(_, _, n)| *n == name)
                .map(|(a, z, _)| (i, a, z))
        })
        .collect();

    if enable {
        if matches.iter().any(|(_, active, _)| *active) {
            return ini.to_string();
        }
        if let Some((i, _, _)) = matches.first() {
            let line = &lines[*i];
            let indent_len = line.len() - line.trim_start().len();
            let body = line.trim_start().trim_start_matches(';').trim_start();
            lines[*i] = format!("{}{}", &line[..indent_len], body);
        } else {
            let key = if ZEND_EXTENSIONS.contains(&name.as_str()) {
                "zend_extension"
            } else {
                "extension"
            };
            lines.push(format!("{}={}", key, name));
        }
    } else {
        for (i, active, _) in &matches {
            if *active {
                lines[*i] = format!(";{}", lines[*i]);
            }
        }
    }
    let mut out = lines.join(nl);
    out.push_str(nl);
    out
}

/// Set `key = "value"` di `php.ini`: baris aktif diganti, kalau tidak ada maka
/// baris komentar pertama di-uncomment, kalau tidak ada juga ditambahkan.
pub fn set_directive(ini: &str, key: &str, value: &str) -> String {
    let nl = newline_of(ini);
    let wanted = format!("{} = \"{}\"", key, value);
    let mut lines: Vec<String> = ini.lines().map(str::to_string).collect();
    let is_key = |l: &str, commented: bool| -> bool {
        let t = l.trim_start();
        let t = match (commented, t.strip_prefix(';')) {
            (true, Some(r)) => r.trim_start(),
            (true, None) => return false,
            (false, Some(_)) => return false,
            (false, None) => t,
        };
        t.strip_prefix(key)
            .is_some_and(|r| r.trim_start().starts_with('='))
    };
    if let Some(i) = lines.iter().position(|l| is_key(l, false)) {
        lines[i] = wanted;
    } else if let Some(i) = lines.iter().position(|l| is_key(l, true)) {
        lines[i] = wanted;
    } else {
        lines.push(wanted);
    }
    let mut out = lines.join(nl);
    out.push_str(nl);
    out
}

/// Nama extension dari isi folder extension (`php_*.dll` atau `*.so`).
pub fn ext_files(names: &[String]) -> BTreeSet<String> {
    names
        .iter()
        .filter(|n| {
            let l = n.to_ascii_lowercase();
            (l.starts_with("php_") && l.ends_with(".dll")) || l.ends_with(".so")
        })
        .map(|n| ext_name(n))
        .collect()
}

/// Nilai dari output `php --ini`, mis. `Loaded Configuration File`.
/// PHP 8.5 menulis path di dalam tanda kutip (`"C:\php\php.ini"`); kutipnya
/// dibuang, kalau tidak Windows menolak path itu (os error 123).
pub fn ini_field(output: &str, label: &str) -> Option<String> {
    output
        .lines()
        .find_map(|l| l.trim().strip_prefix(label))
        .map(|r| unquote(r.trim_start_matches(':').trim()).to_string())
        .filter(|v| !v.is_empty() && v != "(none)")
}

fn unquote(v: &str) -> &str {
    for q in ['"', '\''] {
        if let Some(inner) = v.strip_prefix(q).and_then(|r| r.strip_suffix(q)) {
            return inner.trim();
        }
    }
    v
}

// ---------------------------------------------------------------------------
// Bagian yang menjalankan PHP dan menulis file
// ---------------------------------------------------------------------------

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PhpExt {
    pub name: String,
    pub loaded: bool,
    /// Ada file-nya di folder extension (bisa diaktifkan/nonaktifkan).
    pub available: bool,
    /// Dimuat tanpa file terpisah (dikompilasi ke dalam PHP); tidak bisa diubah.
    pub builtin: bool,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct HttpsStatus {
    pub ok: bool,
    pub cainfo: Option<String>,
    pub cafile: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PhpExtStatus {
    pub version: String,
    pub binary: String,
    pub ini_path: String,
    pub ini_exists: bool,
    pub extension_dir: String,
    pub extensions: Vec<PhpExt>,
    /// Hanya Windows: PHP di Windows tidak membawa daftar CA sendiri.
    pub https: Option<HttpsStatus>,
}

struct PhpInstall {
    version: String,
    binary: PathBuf,
}

/// Lokasi `php` untuk versi tertentu. Dengan mise: `mise where php@<v>`.
/// Tanpa mise (phpvm): `php` di PATH, versinya apa pun yang aktif.
async fn locate(version: Option<&str>) -> Result<PhpInstall, String> {
    if crate::mise::provider_is_mise().await {
        let version = match version {
            Some(v) => v.to_string(),
            None => crate::mise::get_active("php")
                .await?
                .ok_or_else(|| "belum ada versi PHP aktif".to_string())?,
        };
        let dir = crate::mise::install_dir("php", &version).await?;
        let candidates = [
            dir.join("php.exe"),
            dir.join("bin").join("php"),
            dir.join("php"),
        ];
        let binary = candidates
            .into_iter()
            .find(|p| p.is_file())
            .ok_or_else(|| format!("php tidak ditemukan di {}", dir.display()))?;
        return Ok(PhpInstall { version, binary });
    }
    let out = run(
        Path::new("php"),
        &["-r", "echo PHP_BINARY, \"\\n\", PHP_VERSION;"],
    )
    .await?;
    let mut lines = out.0.lines();
    let binary = lines.next().unwrap_or("").trim().to_string();
    let version = lines.next().unwrap_or("").trim().to_string();
    if binary.is_empty() {
        return Err("php tidak ditemukan di PATH".to_string());
    }
    Ok(PhpInstall {
        version,
        binary: PathBuf::from(binary),
    })
}

/// Jalankan php → (stdout, stderr, sukses).
async fn run(binary: &Path, args: &[&str]) -> Result<(String, String, bool), String> {
    let mut cmd = silent_command(&binary.to_string_lossy());
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let out = tokio::time::timeout(Duration::from_secs(20), cmd.output())
        .await
        .map_err(|_| "php tidak merespons (timeout)".to_string())?
        .map_err(|e| format!("php tidak bisa dijalankan: {}", e))?;
    Ok((
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    ))
}

/// `php.ini` yang dipakai, atau lokasi yang akan dibuat bila belum ada.
async fn ini_path(install: &PhpInstall) -> Result<PathBuf, String> {
    let (out, _, _) = run(&install.binary, &["--ini"]).await?;
    if let Some(p) = ini_field(&out, "Loaded Configuration File") {
        return Ok(PathBuf::from(p));
    }
    // Windows mencari php.ini di samping php.exe; di Unix pakai folder
    // konfigurasi yang dilaporkan PHP.
    if cfg!(windows) {
        if let Some(dir) = install.binary.parent() {
            return Ok(dir.join("php.ini"));
        }
    }
    let dir = ini_field(&out, "Configuration File (php.ini) Path")
        .ok_or_else(|| "lokasi php.ini tidak diketahui".to_string())?;
    Ok(PathBuf::from(dir).join("php.ini"))
}

/// Folder extension yang benar-benar dipakai. Di Windows `extension_dir`
/// bawaan bisa relatif ("ext") atau menunjuk folder yang tidak ada; dalam
/// kasus itu dipakai `<folder php>\ext`.
fn resolve_ext_dir(reported: &str, binary: &Path) -> (PathBuf, bool) {
    let p = PathBuf::from(reported.trim());
    if p.is_absolute() && p.is_dir() {
        return (p, false);
    }
    let fallback = binary.parent().map(|d| d.join("ext")).unwrap_or_default();
    (fallback, true)
}

async fn modules(install: &PhpInstall) -> Result<(BTreeSet<String>, Option<String>, bool), String> {
    let (out, err, ok) = run(&install.binary, &["-m"]).await?;
    let warning = startup_warning(&err).or_else(|| startup_warning(&out));
    Ok((parse_modules(&out), warning, ok))
}

async fn status_of(install: &PhpInstall) -> Result<PhpExtStatus, String> {
    let ini = ini_path(install).await?;
    let (info, _, _) = run(
        &install.binary,
        &["-r", "echo ini_get('extension_dir'), \"\\n\", ini_get('curl.cainfo'), \"\\n\", ini_get('openssl.cafile');"],
    )
    .await?;
    let mut info = info.lines();
    let reported_dir = info.next().unwrap_or("").to_string();
    let cainfo = info
        .next()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let cafile = info
        .next()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let (ext_dir, _) = resolve_ext_dir(&reported_dir, &install.binary);

    let names: Vec<String> = std::fs::read_dir(&ext_dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    let files = ext_files(&names);
    let (loaded, _, _) = modules(install).await?;

    let mut all: BTreeSet<String> = files.clone();
    all.extend(loaded.iter().cloned());
    let extensions = all
        .into_iter()
        .map(|name| PhpExt {
            loaded: loaded.contains(&name),
            available: files.contains(&name),
            builtin: !files.contains(&name),
            name,
        })
        .collect();

    let https = cfg!(windows).then(|| {
        let exists = |v: &Option<String>| v.as_ref().is_some_and(|p| Path::new(p).is_file());
        HttpsStatus {
            ok: exists(&cainfo) && exists(&cafile),
            cainfo,
            cafile,
        }
    });

    Ok(PhpExtStatus {
        version: install.version.clone(),
        binary: install.binary.to_string_lossy().into_owned(),
        ini_exists: ini.is_file(),
        ini_path: ini.to_string_lossy().into_owned(),
        extension_dir: ext_dir.to_string_lossy().into_owned(),
        extensions,
        https,
    })
}

fn epoch() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Backup php.ini → `<dir>/php.ini.servel-<epoch>.bak`, simpan 5 terakhir.
fn backup(ini: &Path) -> Result<Option<PathBuf>, String> {
    if !ini.is_file() {
        return Ok(None);
    }
    let dir = ini.parent().ok_or("folder php.ini tidak diketahui")?;
    let target = dir.join(format!("{}{}{}", BACKUP_PREFIX, epoch(), BACKUP_SUFFIX));
    std::fs::copy(ini, &target).map_err(|e| format!("gagal mem-backup php.ini: {}", e))?;
    let mut olds: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .map(|n| n.to_string_lossy())
                        .is_some_and(|n| n.starts_with(BACKUP_PREFIX) && n.ends_with(BACKUP_SUFFIX))
                })
                .collect()
        })
        .unwrap_or_default();
    olds.sort();
    while olds.len() > BACKUP_KEEP {
        let _ = std::fs::remove_file(olds.remove(0));
    }
    Ok(Some(target))
}

/// Isi awal php.ini yang belum ada: salinan `php.ini-development` (bawaan
/// paket PHP Windows) bila tersedia.
fn initial_ini(ini: &Path) -> String {
    let template = ini.parent().map(|d| d.join("php.ini-development"));
    template
        .and_then(|t| std::fs::read_to_string(t).ok())
        .unwrap_or_default()
}

/// Tulis php.ini lalu pastikan PHP masih start dan `check` terpenuhi;
/// kalau tidak, kembalikan isi lama.
async fn write_checked(
    install: &PhpInstall,
    ini: &Path,
    new_text: &str,
    check: impl Fn(&BTreeSet<String>) -> Result<(), String>,
) -> Result<(), String> {
    let existed = ini.is_file();
    let old = std::fs::read_to_string(ini).unwrap_or_default();
    let backup = backup(ini)?;
    std::fs::write(ini, new_text).map_err(|e| format!("gagal menulis {}: {}", ini.display(), e))?;

    let result = match modules(install).await {
        Ok((_, Some(warning), _)) => Err(format!("PHP menolak perubahan: {}", warning)),
        Ok((_, None, false)) => Err("PHP gagal start setelah perubahan".to_string()),
        Ok((loaded, None, true)) => check(&loaded),
        Err(e) => Err(e),
    };
    if let Err(e) = result {
        let restored = if existed {
            std::fs::write(ini, &old)
        } else {
            std::fs::remove_file(ini)
        };
        let note = match (restored, backup) {
            (Ok(()), _) => "php.ini dikembalikan seperti semula".to_string(),
            (Err(re), Some(b)) => format!(
                "php.ini gagal dikembalikan ({}); backup ada di {}",
                re,
                b.display()
            ),
            (Err(re), None) => format!("php.ini gagal dikembalikan ({})", re),
        };
        return Err(format!("{}. {}.", e, note));
    }
    Ok(())
}

#[tauri::command]
pub async fn php_ext_status(version: Option<String>) -> Result<PhpExtStatus, String> {
    let install = locate(version.as_deref()).await?;
    status_of(&install).await
}

#[tauri::command]
pub async fn php_ext_set(
    version: Option<String>,
    name: String,
    enable: bool,
) -> Result<PhpExtStatus, String> {
    let _guard = WRITE_LOCK.lock().await;
    let install = locate(version.as_deref()).await?;
    set_ext(&install, &name, enable).await
}

async fn set_ext(install: &PhpInstall, name: &str, enable: bool) -> Result<PhpExtStatus, String> {
    let name = ext_name(name);
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(format!("nama extension tidak valid: {}", name));
    }
    let before = status_of(install).await?;
    let ext = before
        .extensions
        .iter()
        .find(|e| e.name == name)
        .ok_or_else(|| format!("extension {} tidak ada di {}", name, before.extension_dir))?;
    if ext.builtin {
        return Err(format!(
            "{} sudah tertanam di PHP ini dan tidak bisa dimatikan",
            name
        ));
    }

    let ini = PathBuf::from(&before.ini_path);
    let mut text = if ini.is_file() {
        std::fs::read_to_string(&ini).map_err(|e| format!("gagal membaca php.ini: {}", e))?
    } else {
        initial_ini(&ini)
    };
    if enable {
        // extension_dir bawaan Windows bisa relatif ("ext") → extension gagal
        // dimuat bila PHP dijalankan dari folder lain. Tetapkan path absolut.
        let (info, _, _) = run(&install.binary, &["-r", "echo ini_get('extension_dir');"]).await?;
        let (dir, needs_fix) = resolve_ext_dir(&info, &install.binary);
        if needs_fix && dir.is_dir() {
            text = set_directive(&text, "extension_dir", &dir.to_string_lossy());
        }
    }
    let text = set_extension(&text, &name, enable);
    let wanted = name.clone();
    write_checked(install, &ini, &text, move |loaded| {
        match (enable, loaded.contains(&wanted)) {
            (true, false) => Err(format!("{} tetap tidak termuat", wanted)),
            (false, true) => Err(format!(
                "{} masih termuat (mungkin diaktifkan di file .ini lain)",
                wanted
            )),
            _ => Ok(()),
        }
    })
    .await?;
    status_of(install).await
}

/// File CA bundle yang sudah ada di mesin (Git for Windows), atau `None`.
#[cfg(target_os = "windows")]
fn find_ca_bundle() -> Option<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    for var in ["ProgramFiles", "ProgramW6432", "LOCALAPPDATA"] {
        if let Some(v) = std::env::var_os(var) {
            let base = PathBuf::from(v);
            roots.push(base.join("Git"));
            roots.push(base.join("Programs").join("Git"));
        }
    }
    let rel = [
        "mingw64/etc/ssl/certs/ca-bundle.crt",
        "usr/ssl/certs/ca-bundle.crt",
        "mingw64/ssl/certs/ca-bundle.crt",
    ];
    roots
        .iter()
        .flat_map(|r| rel.iter().map(move |p| r.join(p)))
        .find(|p| p.is_file())
}

/// Tanpa Git: ekspor sertifikat root Windows ke PEM di folder data Servel.
/// Windows mengunduh sebagian root hanya saat dibutuhkan, jadi daftar ini bisa
/// kurang lengkap dibanding bundle Git; dipakai hanya sebagai cadangan.
#[cfg(target_os = "windows")]
fn export_windows_roots(target: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    if let Some(dir) = target.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("gagal membuat {}: {}", dir.display(), e))?;
    }
    let script = format!(
        "$c = @(Get-ChildItem Cert:\\LocalMachine\\Root) + @(Get-ChildItem Cert:\\CurrentUser\\Root) | Sort-Object Thumbprint -Unique; \
         $o = foreach ($x in $c) {{ '-----BEGIN CERTIFICATE-----'; [Convert]::ToBase64String($x.RawData, 'InsertLineBreaks'); '-----END CERTIFICATE-----' }}; \
         [IO.File]::WriteAllLines('{}', [string[]]$o)",
        target.to_string_lossy().replace('\'', "''")
    );
    let ok = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .creation_flags(0x08000000)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if ok && target.is_file() {
        Ok(())
    } else {
        Err("gagal mengekspor sertifikat Windows".to_string())
    }
}

/// Isi `curl.cainfo` dan `openssl.cafile` di php.ini versi ini (Windows).
#[tauri::command]
pub async fn php_ext_fix_https(
    app: tauri::AppHandle,
    version: Option<String>,
) -> Result<PhpExtStatus, String> {
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (&app, &version);
        Err("hanya diperlukan di Windows; PHP di macOS/Linux memakai sertifikat sistem".to_string())
    }
    #[cfg(target_os = "windows")]
    {
        use tauri::Manager;
        let _guard = WRITE_LOCK.lock().await;
        let install = locate(version.as_deref()).await?;
        let bundle = match find_ca_bundle() {
            Some(b) => b,
            None => {
                let target = app
                    .path()
                    .app_data_dir()
                    .map_err(|e| format!("folder data Servel tidak diketahui: {}", e))?
                    .join("certs")
                    .join("ca-bundle.pem");
                export_windows_roots(&target)?;
                target
            }
        };
        // Garis miring maju: aman di php.ini tanpa escape.
        let bundle_str = bundle.to_string_lossy().replace('\\', "/");
        let ini = ini_path(&install).await?;
        let text = if ini.is_file() {
            std::fs::read_to_string(&ini).map_err(|e| format!("gagal membaca php.ini: {}", e))?
        } else {
            initial_ini(&ini)
        };
        let text = set_directive(&text, "curl.cainfo", &bundle_str);
        let text = set_directive(&text, "openssl.cafile", &bundle_str);
        write_checked(&install, &ini, &text, |_| Ok(())).await?;
        status_of(&install).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIN_INI: &str = "[PHP]\r\n;extension=curl\r\n;extension=intl\r\nextension=mbstring\r\n;zend_extension=opcache\r\n";

    #[test]
    fn names_from_values_and_files() {
        assert_eq!(ext_name("php_curl.dll"), "curl");
        assert_eq!(ext_name(r#""C:\php\ext\php_xdebug.dll""#), "xdebug");
        assert_eq!(ext_name("pdo_mysql.so"), "pdo_mysql");
        assert_eq!(ext_name(" Intl "), "intl");
        assert_eq!(module_name("Zend OPcache"), "opcache");
        let files = ext_files(&[
            "php_curl.dll".into(),
            "php_pdo_pgsql.dll".into(),
            "libsodium.dll".into(),
            "redis.so".into(),
        ]);
        assert_eq!(
            files.into_iter().collect::<Vec<_>>(),
            vec!["curl", "pdo_pgsql", "redis"]
        );
    }

    #[test]
    fn parse_php_m_output() {
        let out =
            "[PHP Modules]\nCore\ncurl\nmbstring\nZend OPcache\n\n[Zend Modules]\nZend OPcache\n";
        let m = parse_modules(out);
        assert!(m.contains("core") && m.contains("curl") && m.contains("opcache"));
        assert!(!m.iter().any(|x| x.starts_with('[')));
    }

    #[test]
    fn enable_uncomments_existing_line_and_keeps_crlf() {
        let out = set_extension(WIN_INI, "intl", true);
        assert!(out.contains("\r\nextension=intl\r\n"));
        assert!(
            out.contains(";extension=curl\r\n"),
            "baris lain tidak berubah"
        );
        assert_eq!(out.matches("intl").count(), 1);
        // Idempotent.
        assert_eq!(set_extension(&out, "intl", true), out);
    }

    #[test]
    fn enable_appends_when_missing_with_zend_for_xdebug() {
        let out = set_extension("[PHP]\n", "sodium", true);
        assert!(out.ends_with("extension=sodium\n"));
        let out = set_extension("[PHP]\n", "xdebug", true);
        assert!(out.ends_with("zend_extension=xdebug\n"));
        let out = set_extension(WIN_INI, "opcache", true);
        assert!(out.contains("\r\nzend_extension=opcache\r\n"));
    }

    #[test]
    fn disable_comments_every_active_line() {
        let ini = "extension=php_gd.dll\nextension = gd ; dobel\n;extension=gd\n";
        let out = set_extension(ini, "gd", false);
        assert_eq!(
            out,
            ";extension=php_gd.dll\n;extension = gd ; dobel\n;extension=gd\n"
        );
        assert_eq!(set_extension(&out, "gd", false), out);
    }

    #[test]
    fn similar_names_are_not_confused() {
        let ini = ";extension=pdo_pgsql\n;extension=pgsql\n";
        let out = set_extension(ini, "pgsql", true);
        assert_eq!(out, ";extension=pdo_pgsql\nextension=pgsql\n");
    }

    #[test]
    fn directives_replace_uncomment_or_append() {
        let ini = "[curl]\n;curl.cainfo =\n[openssl]\nopenssl.cafile=\"old\"\n";
        let out = set_directive(ini, "curl.cainfo", "C:/Git/ca.crt");
        assert!(out.contains("\ncurl.cainfo = \"C:/Git/ca.crt\"\n"));
        let out = set_directive(&out, "openssl.cafile", "C:/Git/ca.crt");
        assert!(out.contains("openssl.cafile = \"C:/Git/ca.crt\"") && !out.contains("old"));
        let out = set_directive("", "extension_dir", "C:/php/ext");
        assert_eq!(out, "extension_dir = \"C:/php/ext\"\n");
        // `curl.cainfo_x` bukan `curl.cainfo`.
        let out = set_directive("curl.cainfo_x = 1\n", "curl.cainfo", "a");
        assert_eq!(out, "curl.cainfo_x = 1\ncurl.cainfo = \"a\"\n");
    }

    #[test]
    fn php_ini_fields_and_startup_warnings() {
        let out = "Configuration File (php.ini) Path: \nLoaded Configuration File:         C:\\php\\8.3.35\\php.ini\nScan for additional .ini files in: (none)\n";
        assert_eq!(
            ini_field(out, "Loaded Configuration File").as_deref(),
            Some(r"C:\php\8.3.35\php.ini")
        );
        assert_eq!(
            ini_field(
                "Loaded Configuration File:         (none)\n",
                "Loaded Configuration File"
            ),
            None
        );
        // Format PHP 8.5: path dikutip.
        let out85 = "Configuration File (php.ini) Path: \"C:\\php\\8.5.11\"\nLoaded Configuration File:         \"C:\\php\\8.5.11\\php.ini\"\n";
        assert_eq!(
            ini_field(out85, "Loaded Configuration File").as_deref(),
            Some(r"C:\php\8.5.11\php.ini")
        );
        assert_eq!(
            ini_field(out85, "Configuration File (php.ini) Path").as_deref(),
            Some(r"C:\php\8.5.11")
        );
        let err = "PHP Warning:  PHP Startup: Unable to load dynamic library 'redis' (tried: C:\\ext\\redis)";
        assert!(startup_warning(err).is_some());
        assert!(startup_warning("[PHP Modules]\ncurl\n").is_none());
    }

    /// Uji ujung ke ujung dengan PHP sungguhan (dilewati bila tidak ada).
    /// `php` dibungkus skrip yang memakai php.ini dan folder extension
    /// sementara, jadi konfigurasi PHP sistem tidak tersentuh.
    #[cfg(unix)]
    #[tokio::test]
    async fn real_php_enable_disable_and_rollback() {
        use std::os::unix::fs::PermissionsExt;
        let Ok(out) = std::process::Command::new("php")
            .args(["-r", "echo ini_get('extension_dir');"])
            .output()
        else {
            eprintln!("php tidak ada; uji dilewati");
            return;
        };
        let sys_ext = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim().to_string());
        if !sys_ext.join("calendar.so").is_file() {
            eprintln!("calendar.so tidak ada; uji dilewati");
            return;
        }
        let root = std::env::temp_dir().join(format!("servel-phpext-{}", epoch_nanos()));
        let ext = root.join("ext");
        std::fs::create_dir_all(&ext).unwrap();
        std::fs::copy(sys_ext.join("calendar.so"), ext.join("calendar.so")).unwrap();
        std::fs::write(ext.join("broken.so"), b"bukan library").unwrap();
        let ini = root.join("php.ini");
        std::fs::write(&ini, "[PHP]\n;extension=calendar\n").unwrap();
        let wrapper = root.join("php");
        std::fs::write(
            &wrapper,
            format!(
                "#!/bin/sh\nexec php -n -c '{}' -d extension_dir='{}' \"$@\"\n",
                ini.display(),
                ext.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755)).unwrap();
        let install = PhpInstall {
            version: "test".into(),
            binary: wrapper,
        };

        let st = status_of(&install).await.unwrap();
        assert_eq!(PathBuf::from(&st.ini_path), ini);
        let cal = st.extensions.iter().find(|e| e.name == "calendar").unwrap();
        assert!(cal.available && !cal.loaded && !cal.builtin);
        assert!(st
            .extensions
            .iter()
            .any(|e| e.name == "core" && e.builtin && e.loaded));

        // Aktifkan: baris komentar di-uncomment, modul termuat, backup dibuat.
        let st = set_ext(&install, "calendar", true).await.unwrap();
        assert!(st
            .extensions
            .iter()
            .any(|e| e.name == "calendar" && e.loaded));
        assert!(std::fs::read_to_string(&ini)
            .unwrap()
            .contains("\nextension=calendar\n"));
        let backups = std::fs::read_dir(&root)
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(BACKUP_PREFIX)
            })
            .count();
        assert_eq!(backups, 1);

        // Extension rusak: PHP memberi peringatan startup → php.ini dikembalikan.
        let before = std::fs::read_to_string(&ini).unwrap();
        let err = set_ext(&install, "broken", true).await.unwrap_err();
        assert!(err.contains("dikembalikan"), "{}", err);
        assert_eq!(std::fs::read_to_string(&ini).unwrap(), before);

        // Nonaktifkan.
        let st = set_ext(&install, "calendar", false).await.unwrap();
        assert!(st
            .extensions
            .iter()
            .any(|e| e.name == "calendar" && !e.loaded));
        assert!(std::fs::read_to_string(&ini)
            .unwrap()
            .contains(";extension=calendar"));

        // Modul tertanam tidak bisa diubah.
        assert!(set_ext(&install, "core", false)
            .await
            .unwrap_err()
            .contains("tertanam"));

        let _ = std::fs::remove_dir_all(&root);
    }

    fn epoch_nanos() -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    }
}
