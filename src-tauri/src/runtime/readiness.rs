//! Readiness check (v1.8): status "up" baru dilaporkan setelah service di dalam
//! container benar-benar menerima koneksi, bukan sekadar container running.
//!
//! Probe dijalankan di dalam container (`exec sh -c <cmd>`) hanya selama fase
//! "starting" (running tapi belum ready). Setelah lolos sekali, hasilnya
//! di-cache sampai container berhenti, jadi polling normal tidak menambah exec.
//!
//! Pengaman supaya status tidak macet di "starting":
//! - exit 126/127 (tool probe tidak ada di image, mis. image pengganti)
//!   → dianggap ready;
//! - service tanpa probe → ready begitu running;
//! - lewat [`READY_TIMEOUT`] tanpa lolos → dianggap ready + peringatan di log.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

/// Batas tunggu fase starting sebelum dianggap ready (mis. auth probe tidak cocok
/// pada container adopsi).
pub const READY_TIMEOUT: Duration = Duration::from_secs(120);

/// Perintah probe per service id. `None` = tidak ada probe (ready saat running).
pub fn probe_command(service_id: &str) -> Option<&'static str> {
    match service_id {
        // TCP 127.0.0.1 (bukan socket) supaya server sementara saat first-init
        // image resmi tidak dianggap siap.
        "mysql" => Some(
            r#"MYSQL_PWD="${MYSQL_ROOT_PASSWORD:-}" mysqladmin ping -h 127.0.0.1 -uroot --silent"#,
        ),
        "postgres" => Some(r#"pg_isready -h 127.0.0.1 -U "${POSTGRES_USER:-postgres}" -q"#),
        "redis" => Some("redis-cli -h 127.0.0.1 ping | grep -q PONG"),
        "mongodb" => Some(r#"mongosh --quiet --eval "db.adminCommand('ping').ok" | grep -q 1"#),
        "rabbitmq" => Some("rabbitmq-diagnostics -q ping"),
        "minio" => Some("curl -fsS http://127.0.0.1:9000/minio/health/ready >/dev/null"),
        _ => None,
    }
}

/// Hasil satu probe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeOutcome {
    Ready,
    NotReady,
    /// Tool probe tidak tersedia di image (exit 126/127).
    Unsupported,
}

pub fn classify_exit(code: Option<i32>) -> ProbeOutcome {
    match code {
        Some(0) => ProbeOutcome::Ready,
        Some(126) | Some(127) => ProbeOutcome::Unsupported,
        _ => ProbeOutcome::NotReady,
    }
}

/// Keputusan untuk satu service running pada satu tick polling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Sudah ready (cache) — tidak perlu probe.
    Ready,
    /// Perlu probe sekarang.
    Probe,
    /// Melewati batas tunggu → dianggap ready, beri peringatan.
    TimedOut,
}

/// State readiness lintas tick polling.
#[derive(Default)]
pub struct Tracker {
    ready: HashSet<String>,
    starting_since: HashMap<String, Instant>,
}

impl Tracker {
    /// Panggil sekali per tick dengan id yang sedang running. Membersihkan state
    /// service yang berhenti (supaya start berikutnya di-probe lagi).
    pub fn retain_running(&mut self, running: &HashSet<String>) {
        self.ready.retain(|id| running.contains(id));
        self.starting_since.retain(|id, _| running.contains(id));
    }

    pub fn decide(&mut self, id: &str, now: Instant) -> Decision {
        if self.ready.contains(id) {
            return Decision::Ready;
        }
        if probe_command(id).is_none() {
            self.ready.insert(id.to_string());
            return Decision::Ready;
        }
        let since = *self.starting_since.entry(id.to_string()).or_insert(now);
        if now.duration_since(since) >= READY_TIMEOUT {
            self.ready.insert(id.to_string());
            return Decision::TimedOut;
        }
        Decision::Probe
    }

    pub fn record(&mut self, id: &str, outcome: ProbeOutcome) -> bool {
        if outcome != ProbeOutcome::NotReady {
            self.ready.insert(id.to_string());
            self.starting_since.remove(id);
            return true;
        }
        false
    }

    pub fn is_ready(&self, id: &str) -> bool {
        self.ready.contains(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(ids: &[&str]) -> HashSet<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn services_without_probe_are_ready_immediately() {
        let mut t = Tracker::default();
        let now = Instant::now();
        assert_eq!(t.decide("mailpit", now), Decision::Ready);
        assert_eq!(t.decide("gotenberg", now), Decision::Ready);
        assert!(t.is_ready("mailpit"));
    }

    #[test]
    fn probe_until_ready_then_cached() {
        let mut t = Tracker::default();
        let now = Instant::now();
        assert_eq!(t.decide("mysql", now), Decision::Probe);
        assert!(!t.record("mysql", ProbeOutcome::NotReady));
        assert_eq!(
            t.decide("mysql", now + Duration::from_secs(3)),
            Decision::Probe
        );
        assert!(t.record("mysql", ProbeOutcome::Ready));
        assert_eq!(
            t.decide("mysql", now + Duration::from_secs(6)),
            Decision::Ready
        );
    }

    #[test]
    fn unsupported_probe_counts_as_ready() {
        // mis. image MinIO pengganti tanpa curl → exit 127.
        let mut t = Tracker::default();
        assert_eq!(classify_exit(Some(127)), ProbeOutcome::Unsupported);
        assert!(t.record("minio", classify_exit(Some(127))));
        assert!(t.is_ready("minio"));
    }

    #[test]
    fn timeout_marks_ready() {
        let mut t = Tracker::default();
        let start = Instant::now();
        assert_eq!(t.decide("postgres", start), Decision::Probe);
        t.record("postgres", ProbeOutcome::NotReady);
        assert_eq!(
            t.decide("postgres", start + READY_TIMEOUT),
            Decision::TimedOut
        );
        assert!(t.is_ready("postgres"));
    }

    #[test]
    fn stopping_resets_readiness() {
        let mut t = Tracker::default();
        let now = Instant::now();
        t.decide("redis", now);
        t.record("redis", ProbeOutcome::Ready);
        t.retain_running(&set(&[]));
        assert!(!t.is_ready("redis"));
        assert_eq!(
            t.decide("redis", now),
            Decision::Probe,
            "start berikutnya di-probe lagi"
        );
    }

    #[test]
    fn exit_codes() {
        assert_eq!(classify_exit(Some(0)), ProbeOutcome::Ready);
        assert_eq!(classify_exit(Some(1)), ProbeOutcome::NotReady);
        assert_eq!(classify_exit(None), ProbeOutcome::NotReady);
        assert_eq!(classify_exit(Some(126)), ProbeOutcome::Unsupported);
    }
}
