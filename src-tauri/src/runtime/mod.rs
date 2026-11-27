//! Abstraksi container runtime.
//!
//! Kova mendukung dua engine:
//! - **Docker** (`docker compose`) — semua OS.
//! - **WSLC** (`wslc.exe`, WSL containers) — Windows saja. Tidak butuh Docker
//!   Desktop maupun distro WSL; engine bawaan WSL >= 3.0.1.
//!
//! Semua pemanggilan engine (status, stats, start, stop) lewat modul ini supaya
//! command Tauri, tray, dan loop polling tidak perlu tahu engine mana yang aktif.
//!
//! Engine aktif disimpan di global [`ACTIVE`] dan di-resolve dari preferensi
//! user (`ConfigState.container_runtime`: `auto` | `docker` | `wslc`) lewat
//! [`resolve`]. Resolusi ulang terjadi saat config dibaca/ditulis.

pub mod docker;
pub mod follow;
pub mod readiness;
pub mod restore;
pub mod wslc;

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{OnceLock, RwLock};
use tauri::AppHandle;

use crate::commands::services::{ServiceDef, ServiceStatus};
use crate::commands::stats::ContainerMemStat;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeKind {
    Docker,
    Wslc,
}

impl RuntimeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            RuntimeKind::Docker => "docker",
            RuntimeKind::Wslc => "wslc",
        }
    }

    /// Nama yang ditampilkan ke user (log, pesan error).
    pub fn label(self) -> &'static str {
        match self {
            RuntimeKind::Docker => "Docker",
            RuntimeKind::Wslc => "WSL containers",
        }
    }
}

/// Preferensi user. `Auto` = Kova memilih (lihat [`resolve`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimePreference {
    Auto,
    Docker,
    Wslc,
}

impl RuntimePreference {
    /// Parse string dari config. Nilai tak dikenal → `Auto` (forward-compatible).
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "docker" => RuntimePreference::Docker,
            "wslc" => RuntimePreference::Wslc,
            _ => RuntimePreference::Auto,
        }
    }
}

/// Pilih engine dari preferensi + ketersediaan tool.
///
/// Aturan:
/// - Di luar Windows selalu Docker (WSLC hanya ada di Windows).
/// - Preferensi eksplisit dihormati, walau tool-nya belum terpasang — UI yang
///   menampilkan "not installed", bukan diam-diam pindah engine.
/// - `Auto`: WSL containers adalah default Windows. Docker hanya dipilih
///   bila `wslc` tidak ada, atau Docker **masih menyimpan container Kova**
///   (`docker_has_data == Some(true)`) — storage WSLC terpisah, jadi pindah
///   diam-diam akan membuat database user tampak kosong.
///   `docker_has_data == None` (daemon mati / tak bisa ditanya, mis. sisa
///   `docker.exe` tanpa Docker Desktop) → WSLC; data yang mungkin masih ada di
///   Docker tetap utuh (pindahkan lewat dump .sql + Restore).
pub fn resolve(
    pref: RuntimePreference,
    is_windows: bool,
    wslc_available: bool,
    docker_available: bool,
    docker_has_data: Option<bool>,
) -> RuntimeKind {
    if !is_windows {
        return RuntimeKind::Docker;
    }
    match pref {
        RuntimePreference::Docker => RuntimeKind::Docker,
        RuntimePreference::Wslc => RuntimeKind::Wslc,
        RuntimePreference::Auto => {
            let docker_holds_data = docker_available && docker_has_data == Some(true);
            if wslc_available && !docker_holds_data {
                RuntimeKind::Wslc
            } else {
                RuntimeKind::Docker
            }
        }
    }
}

static ACTIVE: RwLock<RuntimeKind> = RwLock::new(RuntimeKind::Docker);
/// True setelah preferensi pertama kali di-resolve. Sebelum itu `ACTIVE` masih
/// nilai bawaan (Docker) — jangan dipakai mentah, lihat [`ensure_resolved`].
static RESOLVED: AtomicBool = AtomicBool::new(false);
static RESOLVE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static APP: OnceLock<AppHandle> = OnceLock::new();
/// Preferensi terakhir yang sudah di-resolve. `config_write` memanggil
/// `apply_preference` di setiap simpan (mis. toggle service); probe ulang
/// (yang bisa menjalankan `docker ps`) hanya saat preferensinya berubah.
static LAST_PREF: RwLock<Option<String>> = RwLock::new(None);

/// Simpan AppHandle (untuk membaca config saat resolusi lazy). Panggil di setup.
pub fn init(app: &AppHandle) {
    let _ = APP.set(app.clone());
}

/// Pastikan engine sudah di-resolve dari config sebelum dipakai.
///
/// Tanpa ini ada race saat boot: frontend (Onboarding → `check_prerequisites`)
/// bisa berjalan sebelum task setup selesai me-resolve, sehingga membaca nilai
/// bawaan Docker dan menampilkan "Docker NOT FOUND" padahal Auto seharusnya
/// memilih WSL containers.
pub async fn ensure_resolved() -> RuntimeKind {
    if RESOLVED.load(Ordering::Acquire) {
        return active();
    }
    let _guard = RESOLVE_LOCK.lock().await;
    if RESOLVED.load(Ordering::Acquire) {
        return active();
    }
    let pref = APP
        .get()
        .and_then(|app| crate::commands::config::load_config(app).ok())
        .map(|c| c.container_runtime)
        .unwrap_or_else(|| "auto".to_string());
    apply_preference(&pref).await
}

/// Preferensi Auto dan engine terpilih tidak terpasang: probe ulang.
///
/// Auto di-resolve sekali lalu disimpan, jadi tanpa ini memasang WSL
/// containers (atau Docker) setelah Kova dibuka baru terlihat setelah Kova
/// ditutup paksa. Dipanggil dari `check_prerequisites` (Settings, onboarding,
/// status engine). Preferensi eksplisit tidak diubah.
pub async fn reprobe_auto_if_missing(engine: RuntimeKind, installed: bool) -> RuntimeKind {
    if installed {
        return engine;
    }
    let pref = LAST_PREF.read().ok().and_then(|g| g.clone());
    if pref.as_deref() != Some("auto") {
        return engine;
    }
    if let Ok(mut g) = LAST_PREF.write() {
        *g = None;
    }
    apply_preference("auto").await
}

/// Engine yang sedang aktif (nilai terakhir). Caller async sebaiknya memakai
/// [`ensure_resolved`] supaya tidak membaca nilai bawaan sebelum boot selesai.
pub fn active() -> RuntimeKind {
    ACTIVE.read().map(|g| *g).unwrap_or(RuntimeKind::Docker)
}

fn set_active(kind: RuntimeKind) {
    if let Ok(mut g) = ACTIVE.write() {
        *g = kind;
    }
}

/// Probe ketersediaan tool lalu set engine aktif dari `pref_raw`.
/// Dipanggil saat boot dan setiap config ditulis. Return engine terpilih.
pub async fn apply_preference(pref_raw: &str) -> RuntimeKind {
    crate::commands::util::refresh_windows_path();
    let normalized = pref_raw.trim().to_ascii_lowercase();
    if RESOLVED.load(Ordering::Acquire)
        && LAST_PREF.read().ok().and_then(|g| g.clone()).as_deref() == Some(normalized.as_str())
    {
        return active();
    }
    let pref = RuntimePreference::parse(pref_raw);
    let is_windows = cfg!(target_os = "windows");

    let kind = if !is_windows {
        RuntimeKind::Docker
    } else {
        match pref {
            // Preferensi eksplisit tidak perlu probe.
            RuntimePreference::Docker => RuntimeKind::Docker,
            RuntimePreference::Wslc => RuntimeKind::Wslc,
            RuntimePreference::Auto => {
                let (wslc_ok, docker_ok) =
                    tokio::join!(wslc::is_installed(), docker::is_installed());
                // Hanya tanya Docker bila keputusan bergantung padanya.
                let has_data = if wslc_ok && docker_ok { docker::has_servel_containers().await } else { None };
                resolve(pref, is_windows, wslc_ok, docker_ok, has_data)
            }
        }
    };

    set_active(kind);
    if let Ok(mut g) = LAST_PREF.write() {
        *g = Some(normalized);
    }
    RESOLVED.store(true, Ordering::Release);
    kind
}

// ---------------------------------------------------------------------------
// Dispatch — satu pintu untuk semua caller.
// ---------------------------------------------------------------------------

pub async fn status() -> Result<Vec<ServiceStatus>, String> {
    match ensure_resolved().await {
        RuntimeKind::Docker => docker::status().await,
        RuntimeKind::Wslc => wslc::status().await,
    }
}

/// Jalankan probe readiness `service_id` di dalam `container_name`.
pub async fn probe(service_id: &str, container_name: &str) -> readiness::ProbeOutcome {
    let Some(cmd) = readiness::probe_command(service_id) else {
        return readiness::ProbeOutcome::Ready;
    };
    let timeout = std::time::Duration::from_secs(10);
    let code = match ensure_resolved().await {
        RuntimeKind::Docker => docker::exec_status(container_name, cmd, timeout).await,
        RuntimeKind::Wslc => wslc::exec_status(container_name, cmd, timeout).await,
    };
    readiness::classify_exit(code)
}

/// Baris log yang dicari sudah muncul. Spasi di tepi teks diabaikan; teks
/// kosong tidak pernah cocok (kalau tidak, service langsung dianggap siap).
pub fn log_has(logs: &str, text: &str) -> bool {
    let t = text.trim();
    !t.is_empty() && logs.lines().any(|l| l.contains(t))
}

/// Readiness custom service dari PC: port TCP, HTTP GET dengan status
/// 2xx/3xx/401/403 (server menjawab, autentikasi bukan urusan readiness),
/// atau teks tertentu di log container.
pub async fn probe_custom(check: &crate::commands::services::ReadyCheck, host_port: u16, container_name: &str) -> readiness::ProbeOutcome {
    use crate::commands::services::ReadyCheck;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    if let ReadyCheck::Log { text } = check {
        let logs = match ensure_resolved().await {
            RuntimeKind::Docker => docker::logs_tail(container_name, 200).await,
            RuntimeKind::Wslc => wslc::logs_tail(container_name, 200).await,
        };
        return if logs.is_some_and(|l| log_has(&l, text)) {
            readiness::ProbeOutcome::Ready
        } else {
            readiness::ProbeOutcome::NotReady
        };
    }
    if host_port == 0 {
        return readiness::ProbeOutcome::Ready;
    }
    let addr = ("127.0.0.1", host_port);
    let connect = tokio::time::timeout(std::time::Duration::from_secs(3), tokio::net::TcpStream::connect(addr)).await;
    let Ok(Ok(mut stream)) = connect else {
        return readiness::ProbeOutcome::NotReady;
    };
    match check {
        ReadyCheck::Port | ReadyCheck::Log { .. } => readiness::ProbeOutcome::Ready,
        ReadyCheck::Http { path } => {
            let path = if path.starts_with('/') { path.clone() } else { format!("/{}", path) };
            let path: String = path.chars().filter(|c| !c.is_control() && *c != ' ').collect();
            let req = format!("GET {} HTTP/1.0\r\nHost: localhost:{}\r\nConnection: close\r\n\r\n", path, host_port);
            if stream.write_all(req.as_bytes()).await.is_err() {
                return readiness::ProbeOutcome::NotReady;
            }
            let mut buf = [0u8; 64];
            let n = tokio::time::timeout(std::time::Duration::from_secs(5), stream.read(&mut buf)).await;
            let Ok(Ok(n)) = n else { return readiness::ProbeOutcome::NotReady };
            if http_ok(&String::from_utf8_lossy(&buf[..n])) {
                readiness::ProbeOutcome::Ready
            } else {
                readiness::ProbeOutcome::NotReady
            }
        }
    }
}

/// Baris status HTTP yang menandakan server sudah menjawab.
pub fn http_ok(head: &str) -> bool {
    let code: u16 = head.split_whitespace().nth(1).and_then(|c| c.parse().ok()).unwrap_or(0);
    (200..400).contains(&code) || code == 401 || code == 403
}

pub async fn stats() -> Result<Vec<ContainerMemStat>, String> {
    match ensure_resolved().await {
        RuntimeKind::Docker => docker::stats().await,
        RuntimeKind::Wslc => wslc::stats().await,
    }
}

/// Antrean operasi start/stop. `wslc::stop` hanya menghentikan
/// container yang running saat dicek; tanpa antrean, stop yang datang selagi
/// start masih berjalan melihat container belum running, tidak melakukan
/// apa-apa, lalu start selesai dan container tetap hidup dengan toggle mati.
/// `tokio::sync::Mutex` melayani penunggu sesuai urutan datang, jadi urutan
/// toggle pengguna dipertahankan.
static OPS: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Start `services` (id). Service lain yang sedang running tetap dibiarkan.
pub async fn start(app: &AppHandle, defs: &[ServiceDef], services: &[String]) -> Result<(), String> {
    let _turn = OPS.lock().await;
    match ensure_resolved().await {
        RuntimeKind::Docker => docker::start(app, defs, services).await,
        RuntimeKind::Wslc => wslc::start(app, defs, services).await,
    }
}

/// Service yang dihentikan atas permintaan (UI, tray, CLI, keluar). Saat
/// statusnya turun, Kova tidak mengirim notifikasi "berhenti sendiri".
static STOP_REQUESTS: std::sync::Mutex<Option<std::collections::HashMap<String, std::time::Instant>>> = std::sync::Mutex::new(None);
const STOP_REQUEST_TTL: std::time::Duration = std::time::Duration::from_secs(300);

fn mark_stop_requested<'a>(ids: impl IntoIterator<Item = &'a String>) {
    let mut guard = STOP_REQUESTS.lock().unwrap_or_else(|e| e.into_inner());
    let map = guard.get_or_insert_with(std::collections::HashMap::new);
    let now = std::time::Instant::now();
    map.retain(|_, t| now.duration_since(*t) < STOP_REQUEST_TTL);
    for id in ids {
        map.insert(id.clone(), now);
    }
}

/// `true` bila `id` baru saja dihentikan atas permintaan. Catatannya dipakai sekali.
pub fn take_stop_request(id: &str) -> bool {
    let mut guard = STOP_REQUESTS.lock().unwrap_or_else(|e| e.into_inner());
    guard
        .as_mut()
        .and_then(|m| m.remove(id))
        .is_some_and(|t| t.elapsed() < STOP_REQUEST_TTL)
}

pub async fn stop(app: &AppHandle, defs: &[ServiceDef], services: &[String]) -> Result<(), String> {
    mark_stop_requested(services);
    let _turn = OPS.lock().await;
    match ensure_resolved().await {
        RuntimeKind::Docker => docker::stop(app, services).await,
        RuntimeKind::Wslc => wslc::stop(app, defs, services).await,
    }
}

pub async fn stop_all(app: &AppHandle, defs: &[ServiceDef]) -> Result<(), String> {
    mark_stop_requested(defs.iter().map(|d| &d.id));
    let _turn = OPS.lock().await;
    match ensure_resolved().await {
        RuntimeKind::Docker => docker::stop_all(app).await,
        RuntimeKind::Wslc => wslc::stop_all(app, defs).await,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn log_line_ready_check() {
        let logs = "starting\n2026-11-24 ready to accept connections tcp\n";
        assert!(super::log_has(logs, "ready to accept connections"));
        assert!(super::log_has(logs, "  ready to accept connections  "));
        assert!(!super::log_has(logs, "listening on"));
        assert!(!super::log_has(logs, "   "));
        let check: crate::commands::services::ReadyCheck = serde_json::from_str(r#"{"kind":"log","text":"ready"}"#).unwrap();
        assert_eq!(check, crate::commands::services::ReadyCheck::Log { text: "ready".into() });
    }

    use super::*;

    /// Antrean melayani sesuai urutan datang: stop yang datang selagi start
    /// berjalan baru jalan setelah start selesai.
    #[tokio::test]
    async fn ops_queue_keeps_arrival_order() {
        use std::sync::{Arc, Mutex as StdMutex};
        let log = Arc::new(StdMutex::new(Vec::<&str>::new()));
        let first = OPS.lock().await; // "start" sedang berjalan
        let l = log.clone();
        let stop = tokio::spawn(async move {
            let _t = OPS.lock().await;
            l.lock().unwrap().push("stop");
        });
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        log.lock().unwrap().push("start done");
        drop(first);
        stop.await.unwrap();
        assert_eq!(*log.lock().unwrap(), vec!["start done", "stop"]);
    }

    #[test]
    fn non_windows_always_docker() {
        for pref in [RuntimePreference::Auto, RuntimePreference::Docker, RuntimePreference::Wslc] {
            assert_eq!(resolve(pref, false, true, false, None), RuntimeKind::Docker);
        }
    }

    #[test]
    fn explicit_preference_wins_on_windows() {
        assert_eq!(resolve(RuntimePreference::Wslc, true, false, true, Some(true)), RuntimeKind::Wslc);
        assert_eq!(resolve(RuntimePreference::Docker, true, true, false, None), RuntimeKind::Docker);
    }

    #[test]
    fn auto_v2_prefers_wslc_but_protects_docker_data() {
        use RuntimePreference::Auto;
        // Hanya wslc → WSLC.
        assert_eq!(resolve(Auto, true, true, false, None), RuntimeKind::Wslc);
        // Keduanya ada, Docker kosong / daemon mati (sisa docker.exe) → WSLC.
        assert_eq!(resolve(Auto, true, true, true, Some(false)), RuntimeKind::Wslc);
        assert_eq!(resolve(Auto, true, true, true, None), RuntimeKind::Wslc);
        // Docker masih menyimpan container Kova → tetap Docker.
        assert_eq!(resolve(Auto, true, true, true, Some(true)), RuntimeKind::Docker);
        // Tanpa wslc → Docker (onboarding menampilkan cara install).
        assert_eq!(resolve(Auto, true, false, true, Some(false)), RuntimeKind::Docker);
        assert_eq!(resolve(Auto, true, false, false, None), RuntimeKind::Docker);
    }

    #[test]
    fn parse_preference_is_lenient() {
        assert_eq!(RuntimePreference::parse("WSLC"), RuntimePreference::Wslc);
        assert_eq!(RuntimePreference::parse(" docker "), RuntimePreference::Docker);
        assert_eq!(RuntimePreference::parse("auto"), RuntimePreference::Auto);
        assert_eq!(RuntimePreference::parse(""), RuntimePreference::Auto);
        assert_eq!(RuntimePreference::parse("podman"), RuntimePreference::Auto);
    }
}

#[cfg(test)]
mod probe_tests {
    #[test]
    fn http_status_lines() {
        assert!(super::http_ok("HTTP/1.1 200 OK\r\n"));
        assert!(super::http_ok("HTTP/1.0 401 Unauthorized"));
        assert!(!super::http_ok("HTTP/1.1 503 Service Unavailable"));
        assert!(!super::http_ok("garbage"));
    }
}
