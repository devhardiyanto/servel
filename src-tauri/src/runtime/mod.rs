//! Abstraksi container runtime.
//!
//! Servel mendukung dua engine:
//! - **Docker** (`docker compose`) — semua OS. Perilaku v1.5 ke bawah.
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
pub mod readiness;
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

/// Preferensi user. `Auto` = Servel memilih (lihat [`resolve`]).
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
/// - `Auto`: WSLC bila `wslc` ada **dan** `docker` tidak ada. Bila keduanya
///   ada, tetap Docker: storage WSLC terpisah dari Docker Desktop, jadi pindah
///   diam-diam akan membuat database user tampak kosong. User pindah lewat
///   Settings secara sadar.
pub fn resolve(
    pref: RuntimePreference,
    is_windows: bool,
    wslc_available: bool,
    docker_available: bool,
) -> RuntimeKind {
    if !is_windows {
        return RuntimeKind::Docker;
    }
    match pref {
        RuntimePreference::Docker => RuntimeKind::Docker,
        RuntimePreference::Wslc => RuntimeKind::Wslc,
        RuntimePreference::Auto => {
            if wslc_available && !docker_available {
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
                resolve(pref, is_windows, wslc_ok, docker_ok)
            }
        }
    };

    set_active(kind);
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

pub async fn stats() -> Result<Vec<ContainerMemStat>, String> {
    match ensure_resolved().await {
        RuntimeKind::Docker => docker::stats().await,
        RuntimeKind::Wslc => wslc::stats().await,
    }
}

/// Start `services` (id). Service lain yang sedang running tetap dibiarkan.
pub async fn start(app: &AppHandle, defs: &[ServiceDef], services: &[String]) -> Result<(), String> {
    match ensure_resolved().await {
        RuntimeKind::Docker => docker::start(app, defs, services).await,
        RuntimeKind::Wslc => wslc::start(app, defs, services).await,
    }
}

pub async fn stop(app: &AppHandle, defs: &[ServiceDef], services: &[String]) -> Result<(), String> {
    match ensure_resolved().await {
        RuntimeKind::Docker => docker::stop(app, services).await,
        RuntimeKind::Wslc => wslc::stop(app, defs, services).await,
    }
}

pub async fn stop_all(app: &AppHandle, defs: &[ServiceDef]) -> Result<(), String> {
    match ensure_resolved().await {
        RuntimeKind::Docker => docker::stop_all(app).await,
        RuntimeKind::Wslc => wslc::stop_all(app, defs).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_windows_always_docker() {
        for pref in [RuntimePreference::Auto, RuntimePreference::Docker, RuntimePreference::Wslc] {
            assert_eq!(resolve(pref, false, true, false), RuntimeKind::Docker);
        }
    }

    #[test]
    fn explicit_preference_wins_on_windows() {
        assert_eq!(resolve(RuntimePreference::Wslc, true, false, true), RuntimeKind::Wslc);
        assert_eq!(resolve(RuntimePreference::Docker, true, true, false), RuntimeKind::Docker);
    }

    #[test]
    fn auto_picks_wslc_only_without_docker() {
        assert_eq!(resolve(RuntimePreference::Auto, true, true, false), RuntimeKind::Wslc);
        // Keduanya ada → tetap Docker (hindari data "hilang" diam-diam).
        assert_eq!(resolve(RuntimePreference::Auto, true, true, true), RuntimeKind::Docker);
        assert_eq!(resolve(RuntimePreference::Auto, true, false, true), RuntimeKind::Docker);
        // Tidak ada keduanya → Docker (onboarding menampilkan cara install).
        assert_eq!(resolve(RuntimePreference::Auto, true, false, false), RuntimeKind::Docker);
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
