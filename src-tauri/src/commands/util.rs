use std::process::Stdio;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

/// Build a `tokio::process::Command` pre-configured with:
/// - CREATE_NO_WINDOW flag on Windows (prevents terminal flicker)
/// - stdout / stderr set to `Stdio::null()` by default
///
/// Caller may override stdio (e.g. `.stdout(Stdio::piped())`) before spawning.
pub fn silent_command(program: &str) -> Command {
    let mut cmd = Command::new(program);
    cmd.stdout(Stdio::null()).stderr(Stdio::null());

    #[cfg(target_os = "windows")]
    {
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
        // Tool yang dipasang setelah Kova dibuka (wslc, docker, mise, git…)
        // langsung terlihat oleh perintah berikutnya, tanpa restart.
        refresh_windows_path();
    }

    // GUI-launched apps (AppImage / .app dari launcher) TIDAK mewarisi PATH shell
    // interaktif, jadi tool yang di-install via shell rc (mise → ~/.local/bin,
    // dll.) tak ketemu. Inject PATH hasil resolusi login-shell + dir umum.
    #[cfg(not(target_os = "windows"))]
    {
        apply_host_env(&mut cmd);
        cmd.env("PATH", resolved_path().as_str());
    }

    cmd
}

/// Perubahan env agar program yang dijalankan Kova melihat lingkungan sistem,
/// bukan lingkungan AppImage. AppRun mengarahkan `LD_LIBRARY_PATH`, `PATH`,
/// `XDG_DATA_DIRS`, variabel GTK/GIO, dan lain-lain ke `$APPDIR` agar Kova
/// memakai library bawaannya; diwariskan ke mise, `configure`, compiler, atau
/// dev server, library itu tidak cocok dengan sistem (mis. `./configure` PHP
/// gagal walau sama persis berhasil dari terminal). Entri di `$APPDIR`
/// dibuang; variabel yang tinggal kosong dihapus. Di luar AppImage: kosong.
#[cfg_attr(target_os = "windows", allow(dead_code))]
pub fn host_env_changes(vars: &[(String, String)]) -> Vec<(String, Option<String>)> {
    let Some(appdir) = vars.iter().find(|(k, _)| k == "APPDIR").map(|(_, v)| v.trim_end_matches('/').to_string()) else {
        return Vec::new();
    };
    if appdir.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (k, v) in vars {
        // GDK_BACKEND=x11 dan GTK_THEME kosong dipasang hook GTK AppImage untuk
        // WebKit Kova; terminal/editor yang dibuka dari Kova memakai default-nya.
        let gtk_hook = (k == "GDK_BACKEND" && v == "x11") || (k == "GTK_THEME" && v.is_empty());
        if matches!(k.as_str(), "APPDIR" | "APPIMAGE" | "ARGV0" | "OWD") || gtk_hook {
            out.push((k.clone(), None));
        } else if v.contains(&appdir) {
            let kept: Vec<&str> = v.split(':').filter(|p| !p.is_empty() && !p.starts_with(&appdir)).collect();
            out.push((k.clone(), if kept.is_empty() { None } else { Some(kept.join(":")) }));
        }
    }
    out
}

#[cfg(not(target_os = "windows"))]
fn host_env() -> &'static [(String, Option<String>)] {
    static ENV: std::sync::OnceLock<Vec<(String, Option<String>)>> = std::sync::OnceLock::new();
    ENV.get_or_init(|| host_env_changes(&std::env::vars().collect::<Vec<_>>()))
}

/// Terapkan `host_env_changes` ke perintah tokio.
#[cfg(not(target_os = "windows"))]
pub fn apply_host_env(cmd: &mut Command) {
    for (k, v) in host_env() {
        match v {
            Some(v) => cmd.env(k, v),
            None => cmd.env_remove(k),
        };
    }
}

/// Terapkan `host_env_changes` ke perintah std (terminal, editor, shell login).
#[cfg(not(target_os = "windows"))]
pub fn apply_host_env_std(cmd: &mut std::process::Command) {
    for (k, v) in host_env() {
        match v {
            Some(v) => cmd.env(k, v),
            None => cmd.env_remove(k),
        };
    }
}

/// Windows: tambahkan ke PATH proses ini folder yang muncul di PATH sistem atau
/// user sejak Kova dibuka (mis. `C:\Program Files\WSL` setelah `wsl --update`).
///
/// Proses Windows mewarisi PATH saat dijalankan dan tidak pernah melihat
/// perubahan sesudahnya, jadi tanpa ini `wslc` atau `docker` yang baru dipasang
/// tetap "not installed" sampai Kova ditutup paksa. Urutan PATH lama tidak
/// diubah; folder baru ditambahkan di belakang. Dibaca dari registry paling
/// sering sekali per 5 detik.
#[cfg(target_os = "windows")]
pub fn refresh_windows_path() {
    use std::sync::Mutex;
    use std::time::{Duration, Instant};
    static LAST: Mutex<Option<Instant>> = Mutex::new(None);
    {
        let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
        if last.is_some_and(|t| t.elapsed() < Duration::from_secs(5)) {
            return;
        }
        *last = Some(Instant::now());
    }
    let registry = [
        win_reg::read_path(true).unwrap_or_default(),
        win_reg::read_path(false).unwrap_or_default(),
    ]
    .join(";");
    let current = std::env::var("PATH").unwrap_or_default();
    if let Some(merged) = merge_path(&current, &registry) {
        std::env::set_var("PATH", merged);
    }
}

#[cfg(not(target_os = "windows"))]
pub fn refresh_windows_path() {}

/// `current` ditambah segmen `extra` yang belum ada (Windows: tanpa beda huruf
/// besar/kecil, tanpa `\` di akhir). `None` bila tidak ada yang baru.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn merge_path(current: &str, extra: &str) -> Option<String> {
    let norm = |s: &str| s.trim().trim_end_matches('\\').to_ascii_lowercase();
    let mut seen: std::collections::HashSet<String> =
        current.split(';').map(norm).filter(|s| !s.is_empty()).collect();
    let added: Vec<&str> = extra
        .split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty() && seen.insert(norm(s)))
        .collect();
    if added.is_empty() {
        return None;
    }
    let mut out = current.trim_end_matches(';').to_string();
    for a in added {
        if !out.is_empty() {
            out.push(';');
        }
        out.push_str(a);
    }
    Some(out)
}

#[cfg(target_os = "windows")]
mod win_reg {
    use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// Nilai `Path` dari registry, dengan `%VAR%` sudah diekspansi
    /// (RegGetValueW mengekspansi REG_EXPAND_SZ bila diminta RRF_RT_REG_SZ).
    pub fn read_path(machine: bool) -> Option<String> {
        let (root, key) = if machine {
            (HKEY_LOCAL_MACHINE, r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment")
        } else {
            (HKEY_CURRENT_USER, "Environment")
        };
        let key = wide(key);
        let name = wide("Path");
        let mut size: u32 = 0;
        // SAFETY: pointer ke buffer milik fungsi ini; ukuran dalam byte.
        unsafe {
            if RegGetValueW(root, key.as_ptr(), name.as_ptr(), RRF_RT_REG_SZ, std::ptr::null_mut(), std::ptr::null_mut(), &mut size) != 0 {
                return None;
            }
            let mut buf: Vec<u16> = vec![0; (size as usize).div_ceil(2) + 1];
            let mut bytes = (buf.len() * 2) as u32;
            if RegGetValueW(root, key.as_ptr(), name.as_ptr(), RRF_RT_REG_SZ, std::ptr::null_mut(), buf.as_mut_ptr().cast(), &mut bytes) != 0 {
                return None;
            }
            let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
            Some(String::from_utf16_lossy(&buf[..len]))
        }
    }
}

/// PATH yang sudah di-augment untuk spawn tool eksternal di Linux/macOS.
/// Di-resolve sekali (login-interactive shell → source .bashrc/.zshrc) lalu di-cache.
#[cfg(not(target_os = "windows"))]
static UNIX_PATH: std::sync::RwLock<Option<String>> = std::sync::RwLock::new(None);

#[cfg(not(target_os = "windows"))]
pub fn resolved_path() -> String {
    if let Some(p) = UNIX_PATH.read().ok().and_then(|g| g.clone()) {
        return p;
    }
    let p = build_resolved_path();
    if let Ok(mut g) = UNIX_PATH.write() {
        *g = Some(p.clone());
    }
    p
}

/// Linux/macOS: baca ulang PATH login shell (mis. setelah memasang mise atau
/// Docker sambil Kova terbuka). Menjalankan shell, jadi hanya dipanggil dari
/// pemeriksaan eksplisit (`check_prerequisites`), paling sering sekali per 30 detik.
#[cfg(not(target_os = "windows"))]
pub async fn refresh_unix_path() {
    use std::sync::Mutex;
    use std::time::{Duration, Instant};
    static LAST: Mutex<Option<Instant>> = Mutex::new(None);
    {
        let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
        if last.is_some_and(|t| t.elapsed() < Duration::from_secs(30)) {
            return;
        }
        *last = Some(Instant::now());
    }
    if let Ok(p) = tokio::task::spawn_blocking(build_resolved_path).await {
        if let Ok(mut g) = UNIX_PATH.write() {
            *g = Some(p);
        }
    }
}

#[cfg(target_os = "windows")]
pub async fn refresh_unix_path() {}

/// Susun PATH: PATH login-shell (bila terbaca) ∪ PATH proses ∪ dir install umum.
/// Dedup jaga urutan; buang segmen kosong.
#[cfg(not(target_os = "windows"))]
fn build_resolved_path() -> String {
    use std::collections::HashSet;

    let mut parts: Vec<String> = Vec::new();

    if let Some(shell_path) = capture_login_shell_path() {
        parts.extend(shell_path.split(':').map(str::to_string));
    }
    if let Ok(current) = std::env::var("PATH") {
        parts.extend(current.split(':').map(str::to_string));
    }

    let home = std::env::var("HOME").unwrap_or_default();
    if !home.is_empty() {
        parts.push(format!("{home}/.local/bin"));
        parts.push(format!("{home}/.fnm"));
        parts.push(format!("{home}/.local/share/fnm"));
        parts.push(format!("{home}/.cargo/bin"));
    }
    parts.push("/usr/local/bin".to_string());
    parts.push("/opt/homebrew/bin".to_string());

    // Direktori di dalam AppImage hanya untuk Kova sendiri.
    let appdir = std::env::var("APPDIR").ok().filter(|d| !d.is_empty());
    let mut seen = HashSet::new();
    parts
        .into_iter()
        .filter(|p| !appdir.as_deref().is_some_and(|d| p.starts_with(d)))
        .filter(|p| !p.is_empty() && seen.insert(p.clone()))
        .collect::<Vec<_>>()
        .join(":")
}

/// Jalankan login-interactive shell user untuk membaca PATH final-nya (setelah
/// .bashrc/.zshrc di-source). Timeout 5s + stdin null supaya tak pernah hang.
/// Return None kalau shell gagal / tak ada / kosong (caller fallback ke dir umum).
#[cfg(not(target_os = "windows"))]
fn capture_login_shell_path() -> Option<String> {
    use std::process::{Command as StdCommand, Stdio as StdStdio};
    use std::sync::mpsc;
    use std::time::Duration;

    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
    let (tx, rx) = mpsc::channel();

    std::thread::spawn(move || {
        let mut c = StdCommand::new(&shell);
        apply_host_env_std(&mut c);
        let out = c
            .args(["-lic", "printf '%s' \"$PATH\""])
            .stdin(StdStdio::null())
            .stderr(StdStdio::null())
            .output();
        let _ = tx.send(out);
    });

    match rx.recv_timeout(Duration::from_secs(5)) {
        Ok(Ok(out)) if out.status.success() => {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if s.is_empty() {
                None
            } else {
                Some(s)
            }
        }
        _ => None,
    }
}

/// Emit a log line dengan source tag arbitrary ke frontend via `cmd-output`.
/// Menerima `AppHandle` — dipakai oleh polling task untuk emit `[UP]`/`[DOWN]` log lines.
pub fn emit_log_line(app: &tauri::AppHandle, source: &str, message: &str) {
    let _ = app.emit(
        "cmd-output",
        serde_json::json!({
            "line": message,
            "stream": "stdout",
            "source": source,
        }),
    );
}

/// Extract the first semver-like token (`major.minor[.patch]`) from an arbitrary string.
///
/// Strips a leading `v` prefix before scanning. Returns `None` if no digit-dot-digit
/// sequence is found (e.g. warn messages, alias strings, empty input).
///
/// Examples:
///   "PHP 8.3.31 (cli) ..."        → Some("8.3.31")
///   "Active: 8.3 .31 PHP 8.3.31"  → Some("8.3")   ← first occurrence wins
///   "v20.14.0\n"                   → Some("20.14.0")
///   "[warn] No PHP version active" → None
pub fn extract_semver(input: &str) -> Option<String> {
    let s = input.trim().trim_start_matches('v');
    let chars: Vec<char> = s.chars().collect();
    let n = chars.len();
    let mut i = 0;

    while i < n {
        // Find start of a digit run
        if !chars[i].is_ascii_digit() {
            i += 1;
            continue;
        }

        // Collect major digits
        let start = i;
        while i < n && chars[i].is_ascii_digit() {
            i += 1;
        }

        // Must be followed by '.'
        if i >= n || chars[i] != '.' {
            continue;
        }
        i += 1; // consume '.'

        // Must be followed by at least one digit (minor)
        if i >= n || !chars[i].is_ascii_digit() {
            continue;
        }
        while i < n && chars[i].is_ascii_digit() {
            i += 1;
        }

        // Optional: '.patch'
        if i < n && chars[i] == '.' {
            let dot_pos = i;
            i += 1;
            if i < n && chars[i].is_ascii_digit() {
                while i < n && chars[i].is_ascii_digit() {
                    i += 1;
                }
            } else {
                // dot not followed by digit — rewind, don't include it
                i = dot_pos;
            }
        }

        return Some(chars[start..i].iter().collect());
    }

    None
}

const SERVICE_IDS: &[&str] = &[
    "mysql", "postgres", "redis", "rabbitmq", "mongodb",
    "minio", "mailpit", "gotenberg",
];

/// Derive log source tag dari docker compose output line.
/// Scan `servel_<id>` substring — return `<ID>` uppercase. Fallback "KOVA".
pub fn src_from_line(line: &str) -> String {
    for id in SERVICE_IDS {
        if line.contains(&format!("servel_{}", id)) {
            return id.to_uppercase();
        }
    }
    "KOVA".to_string()
}

/// Port yang gagal di-bind Docker ("failed to bind host port 0.0.0.0:3306/tcp:
/// address already in use"), beserta service-nya bila ada di baris yang sama.
pub fn bind_conflicts(stderr_acc: &str) -> Vec<(String, Option<String>)> {
    let mut out: Vec<(String, Option<String>)> = Vec::new();
    for line in stderr_acc.lines() {
        let Some(idx) = line.find("failed to bind host port ") else { continue };
        let after = &line[idx + "failed to bind host port ".len()..];
        let addr = after.split('/').next().unwrap_or("");
        let port: String = addr.rsplit(':').next().unwrap_or("").chars().take_while(|c| c.is_ascii_digit()).collect();
        if port.is_empty() || out.iter().any(|(p, _)| *p == port) {
            continue;
        }
        let svc = SERVICE_IDS.iter().find(|id| line.contains(&format!("servel_{}", id))).map(|s| s.to_string());
        out.push((port, svc));
    }
    out
}

/// Baris progres per layer image ("49ec2dab01d9 Downloading 93.32MB"). Ratusan
/// baris per image dan tidak berguna di log; ringkasan "Image … Pulling/Pulled"
/// tetap tampil.
pub fn is_layer_progress(line: &str) -> bool {
    let t = line.trim_start();
    let id: String = t.chars().take_while(|c| c.is_ascii_hexdigit()).collect();
    id.len() == 12 && t[12..].starts_with(' ')
}

/// Parse accumulated stderr dari docker compose untuk pesan error friendly.
pub fn parse_docker_error(stderr_acc: &str) -> Option<String> {
    let lower = stderr_acc.to_lowercase();
    let binds = bind_conflicts(stderr_acc);
    if !binds.is_empty() {
        let list = binds
            .iter()
            .map(|(p, s)| match s {
                Some(s) => format!("{} ({})", p, s),
                None => p.clone(),
            })
            .collect::<Vec<_>>()
            .join(", ");
        let (word, verb) = if binds.len() == 1 { ("Port", "is") } else { ("Ports", "are") };
        return Some(format!(
            "{} {} {} already used by another program on this computer, for example a database installed outside Docker. Stop that program, then press Retry. Services in the same start were not started either.",
            word, list, verb
        ));
    }
    if lower.contains("port is already allocated") {
        if let Some(idx) = stderr_acc.find("Bind for 0.0.0.0:") {
            let after = &stderr_acc[idx + "Bind for 0.0.0.0:".len()..];
            let port: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
            if !port.is_empty() {
                return Some(format!(
                    "Port {} is used by another program. Close it before starting this service",
                    port
                ));
            }
        }
        return Some("Port conflict: one of the service ports is used by another program".to_string());
    }
    if lower.contains("network") && lower.contains("already exists") {
        return Some("Network conflict — restart Docker Desktop".to_string());
    }
    if lower.contains("cannot connect to the docker daemon") {
        return Some("Docker Desktop is not running. Start Docker first".to_string());
    }
    None
}

/// Stream stdout/stderr dari `cmd` ke frontend via `cmd-output` event, pakai `AppHandle`.
/// Source tag di-derive dari konten line via `src_from_line` (per-service tagging).
/// Tunggu sampai proses selesai; return Err kalau exit code != 0.
pub async fn stream_and_wait_app(mut cmd: Command, app: &AppHandle) -> Result<(), String> {
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| format!("Could not run docker: {}", e))?;

    let stdout = child.stdout.take().ok_or("stdout not available")?;
    let stderr = child.stderr.take().ok_or("stderr not available")?;

    let stderr_buf: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    let app_out = app.clone();
    let stdout_task = tokio::spawn(async move {
        let mut reader = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            if is_layer_progress(&line) {
                continue;
            }
            let source = src_from_line(&line);
            let _ = app_out.emit(
                "cmd-output",
                serde_json::json!({ "line": line, "stream": "stdout", "source": source }),
            );
        }
    });

    let app_err = app.clone();
    let stderr_buf_clone = Arc::clone(&stderr_buf);
    let stderr_task = tokio::spawn(async move {
        let mut reader = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            if let Ok(mut buf) = stderr_buf_clone.lock() {
                buf.push(line.clone());
            }
            if is_layer_progress(&line) {
                continue;
            }
            let source = src_from_line(&line);
            // docker compose menulis progress (Pulling, Created, Started) ke
            // stderr — tampilkan sebagai output biasa. Pesan error final
            // di-emit di bawah bila perintah gagal.
            let _ = app_err.emit(
                "cmd-output",
                serde_json::json!({ "line": line, "stream": "stdout", "source": source }),
            );
        }
    });

    let _ = tokio::join!(stdout_task, stderr_task);

    let status = child
        .wait()
        .await
        .map_err(|e| format!("Waiting for docker failed: {}", e))?;

    if !status.success() {
        let acc = stderr_buf
            .lock()
            .map(|b| b.join("\n"))
            .unwrap_or_default();
        // Compose tidak ada / socket ditolak → pesan dengan langkah perbaikan.
        let msg = crate::runtime::docker::friendly_error(&acc)
            .or_else(|| parse_docker_error(&acc))
            // `ExitStatus` sudah menulis "exit status: 1".
            .unwrap_or_else(|| format!("docker compose failed ({})", status));
        let _ = app.emit(
            "cmd-output",
            serde_json::json!({ "line": msg, "stream": "stderr", "source": "KOVA" }),
        );
        return Err(msg);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appimage_env_is_stripped_for_children() {
        let v = |k: &str, x: &str| (k.to_string(), x.to_string());
        let vars = vec![
            v("APPDIR", "/tmp/.mount_KovaAB"),
            v("APPIMAGE", "/home/dev/Kova.AppImage"),
            v("LD_LIBRARY_PATH", "/tmp/.mount_KovaAB/usr/lib:/tmp/.mount_KovaAB/usr/lib64"),
            v("PATH", "/tmp/.mount_KovaAB/usr/bin:/usr/local/bin:/usr/bin"),
            v("XDG_DATA_DIRS", "/tmp/.mount_KovaAB/usr/share:/usr/share"),
            v("GDK_PIXBUF_MODULE_FILE", "/tmp/.mount_KovaAB/usr/lib/gdk-pixbuf/loaders.cache"),
            v("GSETTINGS_SCHEMA_DIR", "/tmp/.mount_KovaAB//usr/share/glib-2.0/schemas"),
            v("GDK_BACKEND", "x11"),
            v("GTK_THEME", ""),
            v("HOME", "/home/dev"),
        ];
        let got = host_env_changes(&vars);
        let get = |k: &str| got.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
        assert_eq!(get("LD_LIBRARY_PATH"), Some(None));
        assert_eq!(get("PATH"), Some(Some("/usr/local/bin:/usr/bin".into())));
        assert_eq!(get("XDG_DATA_DIRS"), Some(Some("/usr/share".into())));
        assert_eq!(get("GDK_PIXBUF_MODULE_FILE"), Some(None));
        assert_eq!(get("APPIMAGE"), Some(None));
        assert_eq!(get("GSETTINGS_SCHEMA_DIR"), Some(None));
        assert_eq!(get("GDK_BACKEND"), Some(None));
        assert_eq!(get("GTK_THEME"), Some(None));
        assert_eq!(get("HOME"), None);
        // Di luar AppImage tidak ada yang diubah.
        assert!(host_env_changes(&[v("PATH", "/usr/bin")]).is_empty());
    }

    #[test]
    fn docker_bind_conflicts_and_layer_progress() {
        let acc = " Container servel_mysql Error response from daemon: failed to set up container networking: driver failed programming external connectivity on endpoint servel_mysql (461f): failed to bind host port 0.0.0.0:3306/tcp: address already in use \n Container servel_postgres interrupted by the failure of Container servel_mysql \nError response from daemon: failed to set up container networking: driver failed programming external connectivity on endpoint servel_redis (6eb8): failed to bind host port 0.0.0.0:6379/tcp: address already in use";
        assert_eq!(bind_conflicts(acc), vec![("3306".to_string(), Some("mysql".to_string())), ("6379".to_string(), Some("redis".to_string()))]);
        let msg = parse_docker_error(acc).unwrap();
        assert!(msg.starts_with("Ports 3306 (mysql), 6379 (redis) are already used"), "{msg}");
        assert!(is_layer_progress(" 49ec2dab01d9 Downloading 93.32MB"));
        assert!(is_layer_progress("576cb8ef4643 Pull complete 0B"));
        assert!(!is_layer_progress(" Image mysql:8.0 Pulled "));
        assert!(!is_layer_progress(" Container servel_mysql Created"));
    }

    #[test]
    fn test_extract_semver_plain() {
        assert_eq!(extract_semver("8.3.31"), Some("8.3.31".to_string()));
    }

    #[test]
    fn test_extract_semver_v_prefix() {
        assert_eq!(extract_semver("v8.3.31"), Some("8.3.31".to_string()));
    }

    #[test]
    fn test_extract_semver_php_verbose() {
        let s = "PHP 8.3.31 (cli) (built: May 5 2026 17:23:04) (NTS Visual C++ 2019 x64)";
        assert_eq!(extract_semver(s), Some("8.3.31".to_string()));
    }

    #[test]
    fn test_extract_semver_active_prefix() {
        assert_eq!(extract_semver("Active: 8.3.31"), Some("8.3.31".to_string()));
    }

    #[test]
    fn test_extract_semver_warn_message() {
        assert_eq!(
            extract_semver("[warn] No PHP version active. Run: phpvm use <version>"),
            None
        );
    }

    #[test]
    fn test_extract_semver_empty() {
        assert_eq!(extract_semver(""), None);
    }

    #[test]
    fn test_extract_semver_major_minor_only() {
        assert_eq!(extract_semver("8.3"), Some("8.3".to_string()));
    }

    #[test]
    fn test_extract_semver_node_version() {
        assert_eq!(extract_semver("v20.14.0\n"), Some("20.14.0".to_string()));
        assert_eq!(extract_semver("20.14.0"), Some("20.14.0".to_string()));
    }

    #[test]
    fn merge_path_appends_only_new_folders() {
        let cur = r"C:\Windows\system32;C:\Program Files\Git\cmd\;";
        let reg = r"C:\WINDOWS\System32;C:\Program Files\WSL\;C:\Program Files\Git\cmd";
        assert_eq!(
            merge_path(cur, reg).as_deref(),
            Some(r"C:\Windows\system32;C:\Program Files\Git\cmd\;C:\Program Files\WSL\")
        );
        assert_eq!(merge_path(cur, r"c:\windows\system32\"), None);
        assert_eq!(merge_path("", r"C:\a;;C:\a").as_deref(), Some(r"C:\a"));
    }
}
