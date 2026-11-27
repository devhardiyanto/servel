use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;
use tokio::sync::Notify;

/// Mode polling RAM dari UI. Angka di status bar terlihat di semua layar,
/// jadi `normal` dan `fast` sama-sama tiap 2 detik; jendela tersembunyi = jeda.
pub const MODE_NORMAL: u8 = 0;
pub const MODE_FAST: u8 = 1;
pub const MODE_PAUSED: u8 = 2;
static MODE: AtomicU8 = AtomicU8::new(MODE_NORMAL);

fn wake() -> &'static Notify {
    static N: OnceLock<Notify> = OnceLock::new();
    N.get_or_init(Notify::new)
}

pub fn mode() -> u8 {
    MODE.load(Ordering::Relaxed)
}

/// Jeda antar pengambilan untuk mode saat ini. `None` = jangan ambil.
pub fn interval_for(mode: u8) -> Option<std::time::Duration> {
    match mode {
        MODE_PAUSED => None,
        _ => Some(std::time::Duration::from_secs(2)),
    }
}

/// Tidur sampai `d` habis atau mode diganti (mis. layar Services dibuka).
pub async fn sleep_or_mode_change(d: std::time::Duration) {
    tokio::select! {
        _ = tokio::time::sleep(d) => {}
        _ = wake().notified() => {}
    }
}

#[tauri::command]
pub fn stats_set_mode(mode: String) {
    let m = match mode.as_str() {
        "fast" => MODE_FAST,
        "paused" => MODE_PAUSED,
        _ => MODE_NORMAL,
    };
    if MODE.swap(m, Ordering::Relaxed) != m {
        wake().notify_waiters();
    }
}

/// Memori satu container, entri event `container-stats-changed`.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContainerMemStat {
    pub id: String,
    pub mem_mb: u64,
}

/// Parse string `MemUsage` dari `docker stats --format json` jadi MB (integer, rounded).
///
/// Format input contoh: `"123.4MiB / 1.95GiB"`, `"512KiB / 1GiB"`, `"1.2GiB / 8GiB"`.
/// Ambil bagian sebelum `/`, parse angka + unit. Konversi binary unit (MiB = 2^20 bytes)
/// lalu bagi 1_000_000 untuk dapat MB desimal — ini yang biasa dipakai user-facing display
/// (sama seperti Task Manager). Round ke nearest integer.
pub fn parse_mem_usage_to_mb(raw: &str) -> Option<u64> {
    let usage = raw.split('/').next()?.trim();
    if usage.is_empty() {
        return None;
    }

    // Pisahkan angka dan unit. Unit selalu di belakang dan terdiri dari huruf.
    let split_idx = usage
        .find(|c: char| c.is_alphabetic())
        .unwrap_or(usage.len());
    let (num_part, unit_part) = usage.split_at(split_idx);
    let num: f64 = num_part.trim().parse().ok()?;

    let bytes: f64 = match unit_part.trim() {
        "B" | "" => num,
        "KiB" | "kiB" => num * 1024.0,
        "MiB" => num * 1024.0 * 1024.0,
        "GiB" => num * 1024.0 * 1024.0 * 1024.0,
        "TiB" => num * 1024.0 * 1024.0 * 1024.0 * 1024.0,
        // Decimal unit (jarang muncul dari docker stats, tapi guard untuk amannya).
        "kB" | "KB" => num * 1000.0,
        "MB" => num * 1_000_000.0,
        "GB" => num * 1_000_000_000.0,
        _ => return None,
    };

    let mb = bytes / 1_000_000.0;
    Some(mb.round().max(0.0) as u64)
}

/// Parse line-delimited JSON output dari `docker stats --no-stream --format json`.
/// Filter ke container yang punya prefix `servel_` (selaras dengan filter polling status).
/// Service id dari `Name` lewat `id_from_container_name`.
pub fn parse_docker_stats_json(stdout: &str) -> Vec<ContainerMemStat> {
    let mut result = Vec::new();

    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let Ok(val) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };

        let name = val.get("Name").and_then(|v| v.as_str()).unwrap_or("");
        let Some(id) = crate::commands::services::id_from_container_name(name) else {
            continue;
        };

        let mem_raw = val.get("MemUsage").and_then(|v| v.as_str()).unwrap_or("");
        let Some(mem_mb) = parse_mem_usage_to_mb(mem_raw) else {
            continue;
        };

        result.push(ContainerMemStat {
            id: id.to_string(),
            mem_mb,
        });
    }

    result
}

/// Snapshot memori container dari engine aktif (Docker / WSLC).
pub async fn fetch_container_stats() -> Result<Vec<ContainerMemStat>, String> {
    crate::runtime::stats().await
}

/// Payload event `ram-stats-changed`, dikirim tiap putaran polling.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RamStats {
    /// Jumlah memori container; `None` bila engine tidak bisa ditanya.
    pub services_mb: Option<u64>,
    /// Pohon proses semua dev server yang berjalan.
    pub projects_mb: u64,
    /// Proses Kova sendiri beserta proses WebView-nya.
    pub kova_mb: u64,
    /// Waktu pengukuran, milidetik sejak epoch.
    pub measured_at: u64,
}

/// Satu proses: (pid, parent, byte memori).
pub type ProcInfo = (u32, Option<u32>, u64);

/// Bagi memori pohon proses Kova: subtree yang berakar di PID dev server
/// dihitung sebagai project, sisanya (Kova dan WebView2/WebKit yang
/// diluncurkannya) sebagai Kova. Dev server yang sudah tidak berada di bawah
/// Kova tetap dihitung lewat akarnya. Hasil: (byte project, byte Kova).
pub fn split_process_memory(procs: &[ProcInfo], own: u32, dev_roots: &[u32]) -> (u64, u64) {
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    let mut mem: HashMap<u32, u64> = HashMap::new();
    for &(pid, parent, bytes) in procs {
        mem.insert(pid, bytes);
        if let Some(pp) = parent.filter(|pp| *pp != pid) {
            children.entry(pp).or_default().push(pid);
        }
    }
    let roots: HashSet<u32> = dev_roots.iter().copied().collect();
    let mut seen: HashSet<u32> = HashSet::new();
    let (mut projects, mut kova) = (0u64, 0u64);

    let sum_tree = |start: u32, seen: &mut HashSet<u32>| -> u64 {
        let mut total = 0;
        let mut stack = vec![start];
        while let Some(pid) = stack.pop() {
            if !seen.insert(pid) {
                continue;
            }
            total += mem.get(&pid).copied().unwrap_or(0);
            stack.extend(children.get(&pid).into_iter().flatten().copied());
        }
        total
    };

    let mut stack = vec![own];
    while let Some(pid) = stack.pop() {
        if roots.contains(&pid) {
            projects += sum_tree(pid, &mut seen);
            continue;
        }
        if !seen.insert(pid) {
            continue;
        }
        kova += mem.get(&pid).copied().unwrap_or(0);
        stack.extend(children.get(&pid).into_iter().flatten().copied());
    }
    for &root in dev_roots {
        projects += sum_tree(root, &mut seen);
    }
    (projects, kova)
}

/// Byte ke MB desimal, sama dengan angka container.
pub fn bytes_to_mb(bytes: u64) -> u64 {
    (bytes as f64 / 1_000_000.0).round() as u64
}

/// Semua PID di pohon Kova dan di pohon tiap dev server: hanya proses ini
/// yang memorinya dibaca.
pub fn tree_pids(procs: &[ProcInfo], own: u32, dev_roots: &[u32]) -> HashSet<u32> {
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    for &(pid, parent, _) in procs {
        if let Some(pp) = parent.filter(|pp| *pp != pid) {
            children.entry(pp).or_default().push(pid);
        }
    }
    let mut seen = HashSet::new();
    let mut stack: Vec<u32> = std::iter::once(own).chain(dev_roots.iter().copied()).collect();
    while let Some(pid) = stack.pop() {
        if seen.insert(pid) {
            stack.extend(children.get(&pid).into_iter().flatten().copied());
        }
    }
    seen
}

/// Jumlah `Private_Clean` + `Private_Dirty` (kB) dari `/proc/<pid>/smaps_rollup`, dalam byte.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn parse_smaps_private(text: &str) -> Option<u64> {
    let mut kb = 0u64;
    let mut found = false;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("Private_Clean:").or_else(|| line.strip_prefix("Private_Dirty:")) {
            kb += rest.split_whitespace().next()?.parse::<u64>().ok()?;
            found = true;
        }
    }
    found.then_some(kb * 1024)
}

/// Memori privat satu proses: tanpa halaman yang dibagi dengan proses lain
/// (DLL, shared memory). Working set biasa menghitung halaman bersama itu di
/// setiap proses WebView2, sehingga totalnya jauh di atas Task Manager.
#[cfg(windows)]
fn private_bytes(pid: u32) -> Option<u64> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX2};
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    // SAFETY: handle diperiksa sebelum dipakai dan selalu ditutup; struct
    // diisi nol dengan `cb` sesuai ukurannya, seperti yang diminta API.
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return None;
        }
        let mut c: PROCESS_MEMORY_COUNTERS_EX2 = std::mem::zeroed();
        c.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX2>() as u32;
        let ok = K32GetProcessMemoryInfo(h, &mut c as *mut _ as *mut PROCESS_MEMORY_COUNTERS, c.cb);
        CloseHandle(h);
        (ok != 0).then_some(c.PrivateWorkingSetSize as u64)
    }
}

#[cfg(target_os = "linux")]
fn private_bytes(pid: u32) -> Option<u64> {
    std::fs::read_to_string(format!("/proc/{pid}/smaps_rollup")).ok().and_then(|t| parse_smaps_private(&t))
}

#[cfg(not(any(windows, target_os = "linux")))]
fn private_bytes(_pid: u32) -> Option<u64> {
    None
}

/// Ukur memori project dan Kova. `sys` disimpan antar putaran; dari sysinfo
/// hanya daftar proses dan parent-nya yang dipakai. Memori dibaca per proses
/// di pohon Kova dan dev server saja, sebagai memori privat (fallback: RSS
/// dari sysinfo bila OS tidak memberikannya).
pub fn measure_processes(sys: &mut sysinfo::System, dev_roots: &[u32]) -> (u64, u64) {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate};
    // Tanpa tasks: di Linux sysinfo bawaannya juga mendaftar tiap thread
    // sebagai "proses" dengan memori prosesnya, sehingga Kova (puluhan
    // thread) terhitung puluhan kali.
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing().with_memory().without_tasks());
    let own = std::process::id();
    let all: Vec<ProcInfo> = sys
        .processes()
        .iter()
        .filter(|(_, p)| p.thread_kind().is_none())
        .map(|(pid, p)| (pid.as_u32(), p.parent().map(|pp| pp.as_u32()), p.memory()))
        .collect();
    let wanted = tree_pids(&all, own, dev_roots);
    let procs: Vec<ProcInfo> = all
        .into_iter()
        .map(|(pid, parent, rss)| {
            let bytes = if wanted.contains(&pid) { private_bytes(pid).unwrap_or(rss) } else { 0 };
            (pid, parent, bytes)
        })
        .collect();
    let (projects, kova) = split_process_memory(&procs, own, dev_roots);
    (bytes_to_mb(projects), bytes_to_mb(kova))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polling_interval_follows_mode() {
        assert_eq!(interval_for(MODE_FAST), Some(std::time::Duration::from_secs(2)));
        assert_eq!(interval_for(MODE_NORMAL), Some(std::time::Duration::from_secs(2)));
        assert_eq!(interval_for(MODE_PAUSED), None);
    }

    #[test]
    fn process_memory_is_split_between_dev_servers_and_kova() {
        const MB: u64 = 1_000_000;
        // 1 = Kova, 2 = WebView, 3 = renderer WebView, 10 = shell dev server
        // (anak: 11 npm, 12 node), 20 = dev server yang sudah lepas dari Kova,
        // 99 = proses lain di PC.
        let procs: Vec<ProcInfo> = vec![
            (1, Some(0), 90 * MB),
            (2, Some(1), 60 * MB),
            (3, Some(2), 36 * MB),
            (10, Some(1), 2 * MB),
            (11, Some(10), 50 * MB),
            (12, Some(11), 300 * MB),
            (20, Some(5), 60 * MB),
            (99, Some(0), 999 * MB),
        ];
        let (projects, kova) = split_process_memory(&procs, 1, &[10, 20]);
        assert_eq!((bytes_to_mb(projects), bytes_to_mb(kova)), (412, 186));
        // Tanpa dev server: semua milik Kova; PID yang sudah mati diabaikan.
        assert_eq!(split_process_memory(&procs, 1, &[]), (0, 538 * MB));
        assert_eq!(split_process_memory(&procs, 1, &[777]).0, 0);
        // Siklus parent (data OS yang aneh) tidak membuat loop.
        assert_eq!(split_process_memory(&[(1, Some(2), MB), (2, Some(1), MB)], 1, &[]), (0, 2 * MB));
    }

    #[test]
    fn tree_and_private_memory() {
        let procs: Vec<ProcInfo> = vec![(1, Some(0), 0), (2, Some(1), 0), (3, Some(2), 0), (20, Some(5), 0), (21, Some(20), 0), (99, Some(0), 0)];
        let t = tree_pids(&procs, 1, &[20]);
        assert_eq!(t, [1, 2, 3, 20, 21].into_iter().collect());
        let smaps = "Rss:              120000 kB\nPss:               60000 kB\nPrivate_Clean:      1000 kB\nPrivate_Dirty:     40000 kB\nSwap:                  0 kB\n";
        assert_eq!(parse_smaps_private(smaps), Some(41_000 * 1024));
        assert_eq!(parse_smaps_private("Rss: 1 kB\n"), None);
    }

    #[test]
    fn parse_mib() {
        assert_eq!(parse_mem_usage_to_mb("123.4MiB / 1.95GiB"), Some(129));
    }

    #[test]
    fn parse_gib() {
        // 1.2 GiB = 1.2 * 1024^3 / 1_000_000 = ~1288.49 MB -> 1288
        assert_eq!(parse_mem_usage_to_mb("1.2GiB / 8GiB"), Some(1288));
    }

    #[test]
    fn parse_kib() {
        // 512 KiB = 524288 B = 0.524 MB -> 1 (round)
        assert_eq!(parse_mem_usage_to_mb("512KiB / 1GiB"), Some(1));
    }

    #[test]
    fn parse_with_spaces() {
        assert_eq!(parse_mem_usage_to_mb("  256MiB  /  4GiB  "), Some(268));
    }

    #[test]
    fn parse_bytes() {
        assert_eq!(parse_mem_usage_to_mb("100B / 1MiB"), Some(0));
    }

    #[test]
    fn parse_unknown_unit_returns_none() {
        assert_eq!(parse_mem_usage_to_mb("123XB / 1GiB"), None);
    }

    #[test]
    fn parse_empty_returns_none() {
        assert_eq!(parse_mem_usage_to_mb(""), None);
        assert_eq!(parse_mem_usage_to_mb("/ 1GiB"), None);
    }

    #[test]
    fn parse_stats_lines_filters_prefix_and_strips() {
        let stdout = concat!(
            r#"{"Name":"servel_mysql","MemUsage":"345MiB / 2GiB"}"#,
            "\n",
            r#"{"Name":"other_container","MemUsage":"100MiB / 2GiB"}"#,
            "\n",
            r#"{"Name":"servel_redis","MemUsage":"12.5MiB / 2GiB"}"#,
            "\n",
            "",
            "\n",
            "not-json",
            "\n",
        );

        let parsed = parse_docker_stats_json(stdout);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].id, "mysql");
        assert_eq!(parsed[0].mem_mb, 362); // 345 MiB ~ 361.7 MB -> 362
        assert_eq!(parsed[1].id, "redis");
        assert_eq!(parsed[1].mem_mb, 13);
    }

    #[test]
    fn parse_stats_skips_invalid_mem_usage() {
        let stdout = r#"{"Name":"servel_broken","MemUsage":"garbage"}"#;
        assert!(parse_docker_stats_json(stdout).is_empty());
    }
}
