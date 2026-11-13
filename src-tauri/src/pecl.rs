//! Unduh extension PECL untuk PHP di Windows (v2.6): redis, xdebug, imagick,
//! dan extension lain yang punya build di downloads.php.net.
//!
//! Sumber: `https://downloads.php.net/~windows/pecl/releases/<ext>/<versi>/`
//! berisi `php_<ext>-<versi>-<php>-<ts|nts>-<compiler>-<x64|x86>.zip`.
//!
//! - Build dipilih dari daftar file yang benar-benar ada, dicocokkan dengan
//!   versi PHP (mayor.minor), thread safety, dan arsitektur. Compiler tidak
//!   ditebak; diambil dari nama file.
//! - Versi extension: rilis stabil terbaru yang punya build untuk PHP ini.
//! - `php_<ext>.dll` masuk folder extension; DLL lain di arsip (pustaka
//!   pendukung, mis. ImageMagick untuk imagick) masuk folder `php.exe`.
//! - Situs sumber tidak menyediakan checksum; pengaman integritas hanya
//!   HTTPS ke php.net. Setelah dipasang, extension diaktifkan lewat jalur
//!   `php_ext` biasa, yang mengembalikan `php.ini` bila PHP menolak DLL-nya.

use std::path::{Path, PathBuf};
use std::time::Duration;

const BASE: &str = "https://downloads.php.net/~windows/pecl/releases";
/// Berapa versi terbaru yang diperiksa sebelum menyerah.
const MAX_VERSIONS: usize = 12;
const MAX_ZIP_BYTES: u64 = 200 * 1024 * 1024;

/// Ciri build PHP yang menentukan DLL mana yang cocok.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhpBuild {
    /// `8.3`
    pub minor: String,
    pub thread_safe: bool,
    pub x64: bool,
}

/// Parse output `echo PHP_MAJOR_VERSION.'.'.PHP_MINOR_VERSION, "\n", PHP_ZTS, "\n", PHP_INT_SIZE;`.
pub fn parse_build(out: &str) -> Option<PhpBuild> {
    let mut l = out.lines().map(str::trim);
    let minor = l.next()?.to_string();
    let zts = l.next()?;
    let int_size = l.next()?;
    let valid = minor.split('.').count() == 2 && minor.split('.').all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()));
    if !valid {
        return None;
    }
    Some(PhpBuild { minor, thread_safe: zts == "1", x64: int_size == "8" })
}

/// Nama extension yang aman dipakai di URL dan nama file.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 40 && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Nilai `href` dari halaman indeks direktori.
pub fn parse_links(html: &str) -> Vec<String> {
    html.split("href=\"").skip(1).filter_map(|s| s.split('"').next()).map(str::to_string).collect()
}

fn version_key(v: &str) -> Option<Vec<u32>> {
    v.split('.').map(|p| p.parse::<u32>().ok()).collect()
}

/// Versi stabil (hanya angka dan titik; RC/alpha/beta dibuang), terbaru dulu.
pub fn stable_versions(links: &[String]) -> Vec<String> {
    let mut v: Vec<(Vec<u32>, String)> = links
        .iter()
        .filter_map(|l| l.strip_suffix('/'))
        .filter_map(|l| version_key(l).map(|k| (k, l.to_string())))
        .collect();
    v.sort_by(|a, b| b.0.cmp(&a.0));
    v.into_iter().map(|(_, s)| s).collect()
}

/// File zip yang cocok dengan build PHP ini, dari daftar file satu versi.
pub fn pick_build(files: &[String], ext: &str, version: &str, php: &PhpBuild) -> Option<String> {
    let prefix = format!("php_{}-{}-{}-{}-", ext, version, php.minor, if php.thread_safe { "ts" } else { "nts" });
    let suffix = format!("-{}.zip", if php.x64 { "x64" } else { "x86" });
    files
        .iter()
        .find(|f| {
            f.strip_prefix(&prefix)
                .and_then(|r| r.strip_suffix(&suffix))
                // Sisanya hanya nama compiler (vs16, vs17, vc15).
                .is_some_and(|c| !c.is_empty() && c.chars().all(|ch| ch.is_ascii_alphanumeric()))
        })
        .cloned()
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Dest {
    /// Folder extension PHP.
    Ext,
    /// Folder `php.exe` (pustaka pendukung).
    Php,
}

/// Ke mana sebuah entri arsip dipasang; `None` = dilewati. Hanya DLL di akar
/// arsip yang dipasang, jadi nama entri tidak pernah membawa path.
pub fn entry_dest(entry: &str, ext: &str) -> Option<Dest> {
    if entry.contains('/') || entry.contains('\\') || entry.contains("..") {
        return None;
    }
    let lower = entry.to_ascii_lowercase();
    if !lower.ends_with(".dll") {
        return None;
    }
    if lower == format!("php_{}.dll", ext) {
        Some(Dest::Ext)
    } else if lower.starts_with("php_") {
        // Extension lain di arsip yang sama: bukan yang diminta.
        None
    } else {
        Some(Dest::Php)
    }
}

fn client() -> Result<reqwest::Client, String> {
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
    reqwest::Client::builder()
        .user_agent("servel")
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(300))
        .build()
        .map_err(|e| format!("could not set up the download: {}", e))
}

async fn get_text(c: &reqwest::Client, url: &str) -> Result<Option<String>, String> {
    let r = c.get(url).send().await.map_err(|e| format!("could not reach downloads.php.net: {}", e))?;
    if r.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !r.status().is_success() {
        return Err(format!("downloads.php.net answered {}", r.status()));
    }
    r.text().await.map(Some).map_err(|e| e.to_string())
}

/// Hasil pemasangan, untuk pesan di UI.
#[derive(Debug, Clone)]
pub struct Installed {
    pub version: String,
    pub file: String,
    /// Pustaka pendukung yang ditaruh di folder `php.exe`.
    pub support_dlls: usize,
}

/// Cari build yang cocok lalu unduh → `(versi, nama file, isi zip)`.
async fn fetch(ext: &str, php: &PhpBuild) -> Result<(String, String, Vec<u8>), String> {
    let c = client()?;
    let index = get_text(&c, &format!("{}/{}/", BASE, ext))
        .await?
        .ok_or_else(|| format!("extension \"{}\" has no Windows build on downloads.php.net", ext))?;
    let versions = stable_versions(&parse_links(&index));
    if versions.is_empty() {
        return Err(format!("{} has no stable release for Windows yet", ext));
    }
    for version in versions.iter().take(MAX_VERSIONS) {
        let Some(listing) = get_text(&c, &format!("{}/{}/{}/", BASE, ext, version)).await? else { continue };
        let Some(file) = pick_build(&parse_links(&listing), ext, version, php) else { continue };
        let url = format!("{}/{}/{}/{}", BASE, ext, version, file);
        let r = c.get(&url).send().await.map_err(|e| format!("download failed: {}", e))?;
        if !r.status().is_success() {
            return Err(format!("download of {} failed: {}", file, r.status()));
        }
        if r.content_length().is_some_and(|n| n > MAX_ZIP_BYTES) {
            return Err(format!("{} is too large", file));
        }
        let bytes = r.bytes().await.map_err(|e| format!("download was interrupted: {}", e))?;
        if bytes.len() as u64 > MAX_ZIP_BYTES {
            return Err(format!("{} is too large", file));
        }
        return Ok((version.clone(), file, bytes.to_vec()));
    }
    Err(format!(
        "no {} build for PHP {} {} {} in the {} newest releases",
        ext,
        php.minor,
        if php.thread_safe { "TS" } else { "NTS" },
        if php.x64 { "x64" } else { "x86" },
        versions.len().min(MAX_VERSIONS)
    ))
}

/// Ekstrak DLL dari `zip` ke `ext_dir` / `php_dir`. File pendukung yang sudah
/// ada tidak ditimpa (bisa sedang dipakai, atau milik extension lain).
pub fn extract(zip: &[u8], ext: &str, ext_dir: &Path, php_dir: &Path) -> Result<usize, String> {
    use std::io::Read;
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip)).map_err(|e| format!("archive is damaged: {}", e))?;
    let mut main: Option<Vec<u8>> = None;
    let mut support: Vec<(String, Vec<u8>)> = Vec::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| format!("archive is damaged: {}", e))?;
        let name = entry.name().to_string();
        let Some(dest) = entry_dest(&name, ext) else { continue };
        let mut data = Vec::new();
        entry.read_to_end(&mut data).map_err(|e| format!("could not read {}: {}", name, e))?;
        match dest {
            Dest::Ext => main = Some(data),
            Dest::Php => support.push((name, data)),
        }
    }
    let main = main.ok_or_else(|| format!("archive does not contain php_{}.dll", ext))?;
    std::fs::create_dir_all(ext_dir).map_err(|e| format!("could not create {}: {}", ext_dir.display(), e))?;
    let target: PathBuf = ext_dir.join(format!("php_{}.dll", ext));
    std::fs::write(&target, main).map_err(|e| format!("could not write {}: {}", target.display(), e))?;
    let mut written = 0;
    for (name, data) in support {
        let path = php_dir.join(&name);
        if path.exists() {
            continue;
        }
        std::fs::write(&path, data).map_err(|e| format!("could not write {}: {}", path.display(), e))?;
        written += 1;
    }
    Ok(written)
}

/// Unduh dan pasang `ext` untuk build PHP ini.
pub async fn install(ext: &str, php: &PhpBuild, ext_dir: &Path, php_dir: &Path) -> Result<Installed, String> {
    if !valid_name(ext) {
        return Err(format!("extension name is not valid: {}", ext));
    }
    let (version, file, bytes) = fetch(ext, php).await?;
    let (ext_owned, ext_dir, php_dir) = (ext.to_string(), ext_dir.to_path_buf(), php_dir.to_path_buf());
    let support_dlls = tokio::task::spawn_blocking(move || extract(&bytes, &ext_owned, &ext_dir, &php_dir))
        .await
        .map_err(|e| e.to_string())??;
    Ok(Installed { version, file, support_dlls })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn php(minor: &str, ts: bool) -> PhpBuild {
        PhpBuild { minor: minor.into(), thread_safe: ts, x64: true }
    }

    #[test]
    fn parse_build_reads_three_lines() {
        assert_eq!(parse_build("8.3\n0\n8\n"), Some(php("8.3", false)));
        assert_eq!(parse_build("8.5\r\n1\r\n4\r\n"), Some(PhpBuild { minor: "8.5".into(), thread_safe: true, x64: false }));
        assert_eq!(parse_build("PHP Warning: x\n0\n8"), None);
        assert_eq!(parse_build("8.3"), None);
    }

    #[test]
    fn versions_newest_stable_first() {
        // Potongan indeks asli downloads.php.net (redis).
        let html = r#"<a href="/~windows/pecl/releases/">Parent Directory</a> <a href="?C=N;O=D">Name</a>
            <a href="5.3.7RC2/">5.3.7RC2/</a> <a href="6.0.2/">6.0.2/</a> <a href="6.10.0/">6.10.0/</a>
            <a href="6.2.0/">6.2.0/</a> <a href="6.3.0/">6.3.0/</a>"#;
        assert_eq!(stable_versions(&parse_links(html)), vec!["6.10.0", "6.3.0", "6.2.0", "6.0.2"]);
    }

    #[test]
    fn pick_build_matches_php_ts_and_arch() {
        let files: Vec<String> = [
            "php_redis-6.3.0-8.3-nts-vs16-x64.zip",
            "php_redis-6.3.0-8.3-nts-vs16-x86.zip",
            "php_redis-6.3.0-8.3-ts-vs16-x64.zip",
            "php_redis-6.3.0-8.5-nts-vs17-x64.zip",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(pick_build(&files, "redis", "6.3.0", &php("8.3", false)).as_deref(), Some("php_redis-6.3.0-8.3-nts-vs16-x64.zip"));
        assert_eq!(pick_build(&files, "redis", "6.3.0", &php("8.3", true)).as_deref(), Some("php_redis-6.3.0-8.3-ts-vs16-x64.zip"));
        assert_eq!(pick_build(&files, "redis", "6.3.0", &php("8.5", false)).as_deref(), Some("php_redis-6.3.0-8.5-nts-vs17-x64.zip"));
        // Tidak ada build TS untuk 8.5, dan 8.30 tidak boleh cocok dengan 8.3.
        assert_eq!(pick_build(&files, "redis", "6.3.0", &php("8.5", true)), None);
        assert_eq!(pick_build(&files, "redis", "6.3.0", &php("8.30", false)), None);
        assert_eq!(pick_build(&files, "redis", "6.3.0", &php("8.4", false)), None);
    }

    #[test]
    fn entry_dest_only_root_dlls() {
        assert_eq!(entry_dest("php_imagick.dll", "imagick"), Some(Dest::Ext));
        assert_eq!(entry_dest("CORE_RL_bzlib_.dll", "imagick"), Some(Dest::Php));
        assert_eq!(entry_dest("CORE_RL_bzlib_.pdb", "imagick"), None);
        assert_eq!(entry_dest("config/policy.xml", "imagick"), None);
        assert_eq!(entry_dest("php_other.dll", "imagick"), None);
        assert_eq!(entry_dest("../evil.dll", "imagick"), None);
        assert_eq!(entry_dest("sub/evil.dll", "imagick"), None);
        assert_eq!(entry_dest("..\\evil.dll", "imagick"), None);
    }

    #[test]
    fn valid_name_rejects_path_like_input() {
        assert!(valid_name("redis"));
        assert!(valid_name("pdo_sqlsrv"));
        assert!(!valid_name("../redis"));
        assert!(!valid_name("Redis"));
        assert!(!valid_name(""));
    }

    fn zip_of(entries: &[(&str, &[u8])]) -> Vec<u8> {
        use std::io::Write;
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, data) in entries {
            w.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
            w.write_all(data).unwrap();
        }
        w.finish().unwrap().into_inner()
    }

    #[test]
    fn extract_places_dlls_and_keeps_existing_support_files() {
        let root = std::env::temp_dir().join(format!("servel-pecl-{}", std::process::id()));
        let (ext_dir, php_dir) = (root.join("ext"), root.join("php"));
        std::fs::create_dir_all(&php_dir).unwrap();
        std::fs::write(php_dir.join("CORE_RL_a_.dll"), b"old").unwrap();
        let zip = zip_of(&[
            ("php_imagick.dll", b"main"),
            ("php_imagick.pdb", b"x"),
            ("CORE_RL_a_.dll", b"new"),
            ("CORE_RL_b_.dll", b"b"),
            ("config/policy.xml", b"x"),
        ]);
        assert_eq!(extract(&zip, "imagick", &ext_dir, &php_dir).unwrap(), 1);
        assert_eq!(std::fs::read(ext_dir.join("php_imagick.dll")).unwrap(), b"main");
        assert_eq!(std::fs::read(php_dir.join("CORE_RL_a_.dll")).unwrap(), b"old");
        assert_eq!(std::fs::read(php_dir.join("CORE_RL_b_.dll")).unwrap(), b"b");
        assert!(!php_dir.join("config").exists());
        // Arsip tanpa DLL yang diminta ditolak.
        assert!(extract(&zip, "redis", &ext_dir, &php_dir).unwrap_err().contains("php_redis.dll"));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Unduhan sungguhan dari downloads.php.net. Butuh jaringan, jadi tidak
    /// ikut CI: `cargo test pecl -- --ignored`.
    #[tokio::test]
    #[ignore]
    async fn downloads_real_builds() {
        let root = std::env::temp_dir().join(format!("servel-pecl-net-{}", std::process::id()));
        for (ext, minor, ts) in [("redis", "8.5", false), ("xdebug", "8.3", true), ("imagick", "8.3", false)] {
            let (ext_dir, php_dir) = (root.join(ext).join("ext"), root.join(ext).join("php"));
            std::fs::create_dir_all(&php_dir).unwrap();
            let got = install(ext, &php(minor, ts), &ext_dir, &php_dir).await.unwrap();
            let size = std::fs::metadata(ext_dir.join(format!("php_{}.dll", ext))).unwrap().len();
            println!("{} {} {} dll={}B support={}", ext, got.version, got.file, size, got.support_dlls);
            assert!(size > 10_000);
            assert_eq!(got.support_dlls > 0, ext == "imagick");
        }
        assert!(install("redis", &php("5.2", false), &root, &root).await.unwrap_err().contains("build for PHP"));
        assert!(install("no_such_ext_xyz", &php("8.3", false), &root, &root).await.unwrap_err().contains("no Windows build"));
        let _ = std::fs::remove_dir_all(&root);
    }
}
