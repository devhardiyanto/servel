//! Resolusi & instalasi binary Caddy — engine reverse proxy Sites.
//!
//! Strategi: **download-on-first-run** dari GitHub release resmi. Versi dipin
//! (`CADDY_VERSION`) dan digest **SHA-512** di-hard-code per-target, jadi
//! verifikasi bukan trust-on-first-use — file yang tidak cocok dibuang sebelum
//! menyentuh disk. Rilis Caddy memublikasikan SHA-512, bukan SHA-256 (temuan F4
//! di `docs/reports/impl/phase-12-t0-spike.md`).

use std::io::{Cursor, Read, Write};
use std::path::PathBuf;
use std::process::Stdio;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha512};
use tauri::{AppHandle, Emitter, Manager};
use tokio::process::Command;

pub const CADDY_VERSION: &str = "2.11.4";

const RELEASE_BASE: &str = "https://github.com/caddyserver/caddy/releases/download";

/// Arsip rilis untuk satu target OS+arch, beserta digest SHA-512 resminya.
struct Target {
    /// Nama file arsip di GitHub release.
    asset: &'static str,
    /// Digest SHA-512 lowercase-hex dari `caddy_<ver>_checksums.txt`.
    sha512: &'static str,
}

/// Target arsip untuk platform yang sedang dikompilasi.
///
/// Sengaja tidak menebak arsitektur lain: kalau target belum didukung, gagal
/// eksplisit supaya tidak ada unduhan yang tak bisa diverifikasi.
fn target() -> Result<Target, String> {
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    return Ok(Target {
        asset: "caddy_2.11.4_windows_amd64.zip",
        sha512: "cd5ccfd86a4b40732cf715890d0dca5bf3f63adefec5a7914de85adf240c60ce7e5d2791631b88ef9758e46b23bb1730e020b9c5d696889740b284ffd4788e35",
    });

    #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
    return Ok(Target {
        asset: "caddy_2.11.4_windows_arm64.zip",
        sha512: "582ad4657223ecd52a627d88b9d5a0cc051a0289546427659e878c57db3b4f44cedf9edd8ad6efcf29aa20a156d72626db00dfb64caccfe207bbebea2a0773c4",
    });

    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    return Ok(Target {
        asset: "caddy_2.11.4_mac_amd64.tar.gz",
        sha512: "e04eb10f9ce7e2e079bc9bff1bd5d3a3164888d1edbb1a49e5d15be4eab691b57e89ed36bb29c65ba43f1ba8d9279e0967b1003991c13fe4cb78384c3caf25de",
    });

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    return Ok(Target {
        asset: "caddy_2.11.4_mac_arm64.tar.gz",
        sha512: "3190ae0df98b59ab4b6021556fa35adc3c526a4f3e138776b0eaec8a037cc26121cbbb1ad53453f565551b47d37d5ba4755e2c2c3652256737fe2ce9e53c8ec0",
    });

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    return Ok(Target {
        asset: "caddy_2.11.4_linux_amd64.tar.gz",
        sha512: "8220d1f013b6f27510247b2360c9e0ca9f018feebd82515f07635318b34ff9777ccc8fd0b6e6f2486ce3a33fe389fbb7db12d05baa474f4587509fb4f5ebf1c9",
    });

    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    return Ok(Target {
        asset: "caddy_2.11.4_linux_arm64.tar.gz",
        sha512: "d5a7c423853c24a799765e0e8210d5c7c22a8f56ed37a3cae2fb9f58be138853c02b4efd6b59d576e6d8c7c0d30b9c1592deeaa6a536ff69bcca23b8c1ea709c",
    });

    #[allow(unreachable_code)]
    Err(format!(
        "Platform ini belum didukung untuk reverse proxy ({} {}).",
        std::env::consts::OS,
        std::env::consts::ARCH
    ))
}

/// Direktori tempat binary caddy disimpan: `<app_data_dir>/bin`.
fn caddy_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("gagal resolve app_data_dir: {}", e))?;
    Ok(dir.join("bin"))
}

/// Path lengkap binary caddy. File-nya belum tentu ada — cek `.is_file()`.
pub fn caddy_path(app: &AppHandle) -> Result<PathBuf, String> {
    let name = if cfg!(target_os = "windows") {
        "caddy.exe"
    } else {
        "caddy"
    };
    Ok(caddy_dir(app)?.join(name))
}

/// `Command` siap pakai untuk binary caddy.
///
/// `JAVA_HOME` **dibuang**: kalau ter-set, pustaka truststore Caddy mencoba
/// menulis ke `cacerts` JDK lewat `keytool.exe` (butuh admin), gagal, lalu
/// membatalkan seluruh instalasi root CA — termasuk ke trust store OS
/// (temuan F1 di `docs/reports/impl/phase-12-t0-spike.md`).
pub fn caddy_command(bin: &PathBuf) -> Command {
    let mut cmd = Command::new(bin);
    cmd.env_remove("JAVA_HOME");

    #[cfg(target_os = "windows")]
    {
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    #[cfg(not(target_os = "windows"))]
    {
        // `caddy trust` memanggil tool OS (security / update-ca-certificates);
        // app yang dilaunch dari GUI tak mewarisi PATH shell interaktif.
        cmd.env("PATH", crate::commands::util::resolved_path());
    }

    cmd
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub struct ProxyBinaryStatus {
    pub installed: bool,
    /// Versi hasil `caddy version`; `None` bila binary belum ada atau gagal diprobe.
    pub version: Option<String>,
    /// Versi yang diharapkan Servel (dipin).
    pub expected_version: String,
    pub path: String,
}

#[derive(Serialize, Clone)]
struct DownloadProgress {
    downloaded: u64,
    /// 0 bila server tak mengirim `Content-Length`.
    total: u64,
}

async fn probe_version(bin: &PathBuf) -> Option<String> {
    let output = caddy_command(bin)
        .arg("version")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await
        .ok()?;

    if !output.status.success() {
        return None;
    }
    crate::commands::util::extract_semver(&String::from_utf8_lossy(&output.stdout))
}

#[tauri::command]
pub async fn proxy_binary_status(app: AppHandle) -> Result<ProxyBinaryStatus, String> {
    let path = caddy_path(&app)?;
    let installed = path.is_file();
    let version = if installed {
        probe_version(&path).await
    } else {
        None
    };

    Ok(ProxyBinaryStatus {
        installed,
        version,
        expected_version: CADDY_VERSION.to_string(),
        path: path.to_string_lossy().to_string(),
    })
}

/// Unduh arsip rilis ke memori sambil meng-emit progres.
async fn download(app: &AppHandle, url: &str) -> Result<Vec<u8>, String> {
    let response = reqwest::get(url)
        .await
        .map_err(|e| format!("Gagal mengunduh Caddy: {}", e))?;

    if !response.status().is_success() {
        return Err(format!(
            "Gagal mengunduh Caddy: server menjawab {}.",
            response.status()
        ));
    }

    let total = response.content_length().unwrap_or(0);
    let mut buf: Vec<u8> = Vec::with_capacity(total as usize);
    let mut response = response;

    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| format!("Unduhan Caddy terputus: {}", e))?
    {
        buf.extend_from_slice(&chunk);
        let _ = app.emit(
            "proxy-binary-progress",
            DownloadProgress {
                downloaded: buf.len() as u64,
                total,
            },
        );
    }

    Ok(buf)
}

fn verify(bytes: &[u8], expected: &str) -> Result<(), String> {
    let mut hasher = Sha512::new();
    hasher.update(bytes);
    let actual = hex(&hasher.finalize());

    if actual != expected {
        return Err(
            "Verifikasi Caddy gagal: sidik jari file tidak cocok. Unduhan dibatalkan.".to_string(),
        );
    }
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{:02x}", b));
    }
    out
}

/// Ambil binary `caddy` dari arsip. Layout rilis Caddy: binary berada di root arsip.
#[cfg(target_os = "windows")]
fn extract(archive: &[u8]) -> Result<Vec<u8>, String> {
    let mut zip = zip::ZipArchive::new(Cursor::new(archive))
        .map_err(|e| format!("Arsip Caddy rusak: {}", e))?;
    let mut entry = zip
        .by_name("caddy.exe")
        .map_err(|_| "Arsip Caddy tidak berisi caddy.exe.".to_string())?;

    let mut out = Vec::with_capacity(entry.size() as usize);
    entry
        .read_to_end(&mut out)
        .map_err(|e| format!("Gagal membaca caddy.exe dari arsip: {}", e))?;
    Ok(out)
}

#[cfg(not(target_os = "windows"))]
fn extract(archive: &[u8]) -> Result<Vec<u8>, String> {
    let decoder = flate2::read::GzDecoder::new(Cursor::new(archive));
    let mut tar = tar::Archive::new(decoder);

    for entry in tar
        .entries()
        .map_err(|e| format!("Arsip Caddy rusak: {}", e))?
    {
        let mut entry = entry.map_err(|e| format!("Arsip Caddy rusak: {}", e))?;
        let is_caddy = entry
            .path()
            .map(|p| p.as_os_str() == "caddy")
            .unwrap_or(false);

        if is_caddy {
            let mut out = Vec::new();
            entry
                .read_to_end(&mut out)
                .map_err(|e| format!("Gagal membaca caddy dari arsip: {}", e))?;
            return Ok(out);
        }
    }

    Err("Arsip Caddy tidak berisi binary caddy.".to_string())
}

/// Tulis binary ke disk lewat file sementara lalu rename — supaya tak pernah ada
/// caddy setengah-tertulis yang terlihat "installed" oleh [`is_installed`].
fn install_to_disk(dest: &PathBuf, bytes: &[u8]) -> Result<(), String> {
    let dir = dest
        .parent()
        .ok_or_else(|| "path binary Caddy tidak valid".to_string())?;
    std::fs::create_dir_all(dir).map_err(|e| format!("gagal membuat direktori bin: {}", e))?;

    let tmp = dest.with_extension("download");
    {
        let mut file =
            std::fs::File::create(&tmp).map_err(|e| format!("gagal menulis binary Caddy: {}", e))?;
        file.write_all(bytes)
            .map_err(|e| format!("gagal menulis binary Caddy: {}", e))?;
    }

    #[cfg(not(target_os = "windows"))]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("gagal set permission binary Caddy: {}", e))?;
    }

    // Windows menolak rename ke file yang sudah ada.
    if dest.exists() {
        let _ = std::fs::remove_file(dest);
    }
    std::fs::rename(&tmp, dest).map_err(|e| format!("gagal memasang binary Caddy: {}", e))?;
    Ok(())
}

/// Unduh + verifikasi + pasang binary Caddy. Idempoten: kalau versi yang dipin
/// sudah terpasang, tidak mengunduh ulang.
#[tauri::command]
pub async fn proxy_binary_install(app: AppHandle) -> Result<ProxyBinaryStatus, String> {
    let dest = caddy_path(&app)?;

    if dest.is_file() && probe_version(&dest).await.as_deref() == Some(CADDY_VERSION) {
        return proxy_binary_status(app).await;
    }

    let target = target()?;
    let url = format!("{}/v{}/{}", RELEASE_BASE, CADDY_VERSION, target.asset);

    let archive = download(&app, &url).await?;
    verify(&archive, target.sha512)?;
    let binary = extract(&archive)?;
    install_to_disk(&dest, &binary)?;

    proxy_binary_status(app).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_is_lowercase_and_padded() {
        assert_eq!(hex(&[0x00, 0x0f, 0xff]), "000fff");
    }

    #[test]
    fn verify_rejects_mismatch() {
        let err = verify(b"halo", "deadbeef").unwrap_err();
        assert!(err.contains("Verifikasi Caddy gagal"));
    }

    #[test]
    fn verify_accepts_known_digest() {
        // SHA-512 dari string kosong.
        let empty = "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce\
                     47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e";
        verify(b"", empty).unwrap();
    }

    #[test]
    fn target_is_defined_for_this_platform() {
        // Build host selalu salah satu target yang didukung; kalau tidak, ini
        // memberi sinyal lebih awal daripada gagal saat runtime.
        assert!(target().is_ok());
    }
}
