//! Menjalankan dan menghentikan dev server tiap site.
//!
//! - Perintah dijalankan lewat shell (`cmd /S /C` di Windows, `sh -c` di
//!   Unix) di folder project, supaya `npm run dev -- --port N` dan sejenisnya
//!   bekerja apa adanya.
//! - Bila mise terpasang, environment hasil `mise env --json` di folder itu
//!   dipakai, jadi PHP/Node/Go mengikuti `mise.toml` project.
//! - Stop mematikan seluruh pohon proses (`taskkill /T` / process group),
//!   karena `npm` → `node` → `vite` adalah tiga proses.

use serde::Serialize;
use std::collections::HashMap;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::Path;
use std::process::Stdio;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tokio::io::{AsyncBufReadExt, BufReader};

use super::detect::render_command;
use crate::commands::config::Project;
use crate::commands::util::silent_command;

/// Batas menunggu port menerima koneksi sebelum status menjadi `unresponsive`.
const READY_TIMEOUT: Duration = Duration::from_secs(90);
const READY_INTERVAL: Duration = Duration::from_millis(500);
/// Setelah batas lewat, port tetap dicek (lebih jarang) supaya start
/// yang lambat (mis. `composer run dev` di Windows) naik ke `running` begitu
/// server akhirnya menjawab, bukan macet di `unresponsive`.
const LATE_INTERVAL: Duration = Duration::from_secs(2);

/// Shell keluar dengan 127 (`sh`) atau 9009 (`cmd`) bila program
/// pertama tidak ditemukan. Pesan shell berbeda per OS, jadi Kova menulis
/// petunjuknya sendiri, dengan nama programnya.
fn not_found_hint(code: Option<i32>, command: &str) -> Option<String> {
    if !matches!(code, Some(127) | Some(9009)) {
        return None;
    }
    let program = command.split_whitespace().next()?.trim_matches('"');
    let how = match program {
        "composer" => " Install Composer (getcomposer.org) and make sure it is on PATH, then restart Kova so the new PATH is used.",
        "npm" | "npx" | "node" => " Install Node with Runtimes → Node, or set a Node version for this project.",
        "pnpm" | "yarn" | "bun" => " Install it (for example with corepack enable or mise), then restart Kova.",
        "php" => " Install PHP with Runtimes → PHP, or set a PHP version for this project.",
        _ => " Install it or add it to PATH, then restart Kova.",
    };
    Some(format!("{} was not found.{}", program, how))
}

/// Program tidak ditemukan di distro (bash keluar 127): tool Windows dan
/// Runtimes di Kova tidak berlaku di sana.
fn wsl_not_found_hint(code: Option<i32>, command: &str, distro: &str) -> Option<String> {
    if code != Some(127) {
        return None;
    }
    let program = command.split_whitespace().next()?.trim_matches('"');
    Some(format!(
        "{} was not found in {}. Install it inside the distro (or with mise there), or turn off Run inside WSL in Project settings to use the Windows tools.",
        program, distro
    ))
}

/// Proses selesai sendiri dengan exit 0 tanpa port pernah menjawab: biasanya
/// server gagal listen tetapi error-nya diabaikan. Penyebab yang paling sering:
/// app memakai PORT sebagai alamat (Go `http.Server{Addr: os.Getenv("PORT")}`)
/// dan butuh `:8081`, bukan `8081`.
fn no_listen_hint(code: Option<i32>, ever_ready: bool, port: u16) -> Option<String> {
    if code != Some(0) || ever_ready {
        return None;
    }
    Some(format!(
        "The app ended without listening on port {port}. If it uses PORT as an address (like Go's http.Server{{Addr: os.Getenv(\"PORT\")}}), put a colon in front in the command: PORT=\":{{port}}\" <command>. Otherwise check that the app starts its server and does not stop right after."
    ))
}

/// ENV untuk Terminal here: sama dengan Start, tanpa variabel tampilan log.
pub fn terminal_env(site: &Project) -> Vec<(String, String)> {
    let mut out = vec![
        ("PORT".to_string(), site.port.to_string()),
        ("SERVER_PORT".to_string(), site.port.to_string()),
        ("SERVER_HOST".to_string(), "127.0.0.1".to_string()),
    ];
    for (k, v) in site.effective_env() {
        out.retain(|(x, _)| *x != k);
        out.push((k, v));
    }
    out
}

/// Petunjuk untuk baris output yang dikenali. WSL menulis
/// RPC_S_SERVER_UNAVAILABLE bila service-nya tidak bisa dihubungi.
fn output_hint(line: &str) -> Option<&'static str> {
    if line.contains("RPC_S_SERVER_UNAVAILABLE") {
        return Some(
            "WSL did not answer. Run `wsl --shutdown`, then Start again. If it keeps happening, run the same command in a terminal: if it fails there too, the problem is in WSL, not in Kova.",
        );
    }
    None
}

/// Port yang dicetak dev server: URL lokal (`http://localhost:3000/`,
/// `http://127.0.0.1:8000`, `http://0.0.0.0:5173`, `http://[::1]:4000`) atau `listening on :8080` /
/// `listening on port 8080`. Port < 1024 diabaikan.
pub fn printed_port(line: &str) -> Option<u16> {
    let lower = line.to_ascii_lowercase();
    let digits = |s: &str| -> Option<u16> {
        let d: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
        d.parse::<u16>().ok().filter(|p| *p >= 1024)
    };
    let mut found = None;
    // Hanya URL http(s): baris koneksi database (`localhost:5432`) bukan port app.
    for scheme in ["http://", "https://"] {
        let mut rest = lower.as_str();
        while let Some(i) = rest.find(scheme) {
            rest = &rest[i + scheme.len()..];
            for host in ["localhost:", "127.0.0.1:", "0.0.0.0:", "[::1]:", "[::]:"] {
                if let Some(after) = rest.strip_prefix(host) {
                    if let Some(p) = digits(after) {
                        found = Some(p);
                    }
                }
            }
        }
    }
    if found.is_none() {
        for key in ["listening on port ", "listening on :", "listening at :", "running on port ", "started on port "] {
            if let Some(i) = lower.find(key) {
                found = digits(&lower[i + key.len()..]);
                if found.is_some() {
                    break;
                }
            }
        }
    }
    found
}

/// Folder UNC (`\\wsl.localhost\…`, `\\server\share`). `cmd` tidak bisa
/// memakainya sebagai folder kerja dan pindah ke C:\Windows.
fn is_unc(path: &str) -> bool {
    path.starts_with(r"\\")
}

/// Di folder UNC, `pushd` memetakan drive sementara lalu masuk ke folder itu;
/// `cd` mencetak folder barunya (dibaca Kova untuk melepas drive setelah
/// proses selesai). Gagal `pushd` = keluar dengan 1, command tidak dijalankan.
/// `&&`/`||` mengikat lebih kuat dari `&`, jadi `&` di command pengguna aman.
fn unc_wrap(path: &str, line: &str) -> String {
    format!("pushd \"{}\" || exit /b 1 & cd & {}", path, line)
}

/// Drive dari baris `cd` pertama, mis. `Z:\home\dev\app` → `Z:`.
fn mapped_drive(line: &str) -> Option<String> {
    let b = line.as_bytes();
    (b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && b[2] == b'\\').then(|| line[..2].to_uppercase())
}

/// Status berikutnya dari pengecekan port. `None` = tidak berubah.
fn ready_transition(state: SiteState, open: bool, timed_out: bool) -> Option<SiteState> {
    match (state, open, timed_out) {
        (SiteState::Starting | SiteState::Unresponsive, true, _) => Some(SiteState::Running),
        (SiteState::Starting, false, true) => Some(SiteState::Unresponsive),
        _ => None,
    }
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SiteState {
    Stopped,
    Starting,
    Running,
    /// Proses hidup, tetapi port belum menerima koneksi setelah batas waktu.
    Unresponsive,
    /// Proses berhenti sendiri (crash atau selesai).
    Exited,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SiteRuntime {
    pub id: String,
    pub state: SiteState,
    pub exit_code: Option<i32>,
    /// Baris output terakhir sebelum proses berhenti.
    pub last_line: Option<String>,
    /// Environment yang dipakai saat proses ini dimulai. `None` =
    /// Default. Dipakai UI untuk "Restart to use …".
    pub env_name: Option<String>,
    /// Port terakhir yang dicetak app di output (`http://localhost:3000`,
    /// `listening on :8080`). UI membandingkannya dengan port project.
    pub printed_port: Option<u16>,
}

struct Entry {
    pid: Option<u32>,
    state: SiteState,
    exit_code: Option<i32>,
    last_line: Option<String>,
    /// Stop diminta pengguna → exit berikutnya adalah `Stopped`, bukan `Exited`.
    stopping: bool,
    /// Membedakan start lama dan baru untuk site yang sama.
    generation: u64,
    env_name: Option<String>,
    /// Project WSL: (distro, PGID) proses di dalam distro, untuk Stop.
    wsl_group: Option<(String, u32)>,
    printed_port: Option<u16>,
}

#[derive(Default)]
pub struct SiteProcs {
    entries: Mutex<HashMap<String, Entry>>,
    counter: Mutex<u64>,
    /// Catatan dev server yang sedang jalan, disimpan ke disk supaya sisa
    /// dari sesi yang mati paksa bisa dibersihkan saat Kova dibuka lagi.
    /// `None` = tidak disimpan (mis. di unit test).
    records_path: Mutex<Option<std::path::PathBuf>>,
    records: Mutex<HashMap<String, Record>>,
}

/// Dev server yang sedang jalan. `started` = waktu mulai proses menurut OS,
/// untuk memastikan PID belum dipakai ulang proses lain sebelum dimatikan.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct Record {
    pub pid: u32,
    pub started: String,
}

impl SiteProcs {
    fn snapshot(&self) -> Vec<SiteRuntime> {
        let map = self.entries.lock().unwrap();
        let mut out: Vec<SiteRuntime> = map
            .iter()
            .map(|(id, e)| SiteRuntime {
                id: id.clone(),
                state: e.state,
                exit_code: e.exit_code,
                last_line: e.last_line.clone(),
                env_name: e.env_name.clone(),
                printed_port: e.printed_port,
            })
            .collect();
        out.sort_by(|a, b| a.id.cmp(&b.id));
        out
    }

    fn is_active(&self, id: &str) -> bool {
        self.entries
            .lock()
            .unwrap()
            .get(id)
            .is_some_and(|e| matches!(e.state, SiteState::Starting | SiteState::Running | SiteState::Unresponsive))
    }
}

fn emit_status<R: Runtime>(app: &AppHandle<R>) {
    let procs = app.state::<SiteProcs>();
    let _ = app.emit("site-status-changed", procs.snapshot());
}

fn emit_log<R: Runtime>(app: &AppHandle<R>, source: &str, line: &str, level: Option<&str>) {
    let mut payload = serde_json::json!({ "line": line, "stream": "stdout", "source": source });
    if let Some(l) = level {
        payload["level"] = serde_json::Value::String(l.to_string());
    }
    let _ = app.emit("cmd-output", payload);
}

/// Tebak tingkat sebuah baris output dev server dari isinya, untuk filter
/// "Errors only" di Logs. Sengaja sempit: hanya kata yang jelas menandai
/// masalah, dan ringkasan "0 errors" tidak dihitung.
pub fn line_level(line: &str) -> Option<&'static str> {
    let l = line.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| l.contains(w));
    if has(&["0 error", "no error", "without error"]) {
        return None;
    }
    if has(&["error", "exception", "fatal", "panic:", "traceback", "unhandled", "econnrefused", "eaddrinuse"]) {
        return Some("error");
    }
    if has(&["warn", "deprecated"]) {
        return Some("warn");
    }
    None
}

/// Buang escape ANSI (warna) dari output dev server supaya log terbaca.
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                // CSI: parameter sampai byte akhir 0x40–0x7E.
                for n in chars.by_ref() {
                    if ('\u{40}'..='\u{7e}').contains(&n) {
                        break;
                    }
                }
            } else if chars.peek() == Some(&']') {
                // OSC (judul terminal, hyperlink): sampai BEL atau ESC \.
                chars.next();
                while let Some(n) = chars.next() {
                    if n == '\u{7}' {
                        break;
                    }
                    if n == '\u{1b}' && chars.peek() == Some(&'\\') {
                        chars.next();
                        break;
                    }
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

/// Ada yang menerima koneksi di port ini (IPv4 atau IPv6 loopback)? Vite di
/// Node baru bisa hanya mendengarkan `::1`, jadi keduanya dicek.
pub async fn port_open(port: u16) -> bool {
    let addrs = [
        SocketAddr::from((Ipv4Addr::LOCALHOST, port)),
        SocketAddr::from((Ipv6Addr::LOCALHOST, port)),
    ];
    for addr in addrs {
        let attempt = tokio::time::timeout(Duration::from_millis(400), tokio::net::TcpStream::connect(addr)).await;
        if matches!(attempt, Ok(Ok(_))) {
            return true;
        }
    }
    false
}

/// Port bisa dipakai: tidak ada yang mendengarkan dan bind berhasil.
pub async fn port_free(port: u16) -> bool {
    if port_open(port).await {
        return false;
    }
    std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, port)).is_ok()
}

/// Versi sinkron untuk `pick_port` (hanya bind; cukup untuk usulan port).
pub fn port_bindable(port: u16) -> bool {
    std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, port)).is_ok()
}

fn shell_command(line: &str) -> tokio::process::Command {
    #[cfg(target_os = "windows")]
    {
        // `/S /C "<line>"`: cmd membuang tanda kutip terluar dan menjalankan
        // sisanya apa adanya, jadi kutip di dalam perintah pengguna aman.
        let mut cmd = silent_command("cmd");
        cmd.raw_arg(format!("/S /C \"{}\"", line));
        cmd
    }
    #[cfg(not(target_os = "windows"))]
    {
        let mut cmd = silent_command("sh");
        cmd.args(["-c", line]);
        // Group sendiri supaya Stop bisa mematikan seluruh pohon proses.
        cmd.process_group(0);
        cmd
    }
}

pub(crate) async fn kill_tree(pid: u32) {
    #[cfg(target_os = "windows")]
    {
        let _ = silent_command("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .status()
            .await;
    }
    #[cfg(not(target_os = "windows"))]
    {
        let group = format!("-{}", pid);
        let _ = silent_command("kill").args(["-TERM", "--", &group]).status().await;
        // Beri waktu berhenti baik-baik, lalu paksa bila masih hidup.
        tokio::time::sleep(Duration::from_secs(3)).await;
        let _ = silent_command("kill").args(["-KILL", "--", &group]).status().await;
    }
}

/// Windows: dev server dimasukkan ke job object dengan
/// `KILL_ON_JOB_CLOSE`. Handle job hanya dipegang Kova, jadi saat Kova
/// berakhir dengan cara apa pun (termasuk crash atau dimatikan dari Task
/// Manager), Windows menutup handle itu dan mematikan seluruh pohon proses.
/// Proses anak yang dibuat sebelum penugasan selesai (beberapa milidetik
/// pertama) bisa lolos; catatan di disk menjadi cadangannya.
#[cfg(target_os = "windows")]
mod job {
    use std::sync::OnceLock;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_SET_QUOTA, PROCESS_TERMINATE};

    static JOB: OnceLock<usize> = OnceLock::new();

    fn job() -> Option<HANDLE> {
        let h = *JOB.get_or_init(|| unsafe {
            let h = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if h.is_null() {
                return 0;
            }
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let ok = SetInformationJobObject(
                h,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const core::ffi::c_void,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            if ok == 0 {
                CloseHandle(h);
                return 0;
            }
            h as usize
        });
        (h != 0).then_some(h as HANDLE)
    }

    pub fn assign(pid: u32) -> Result<(), String> {
        let job = job().ok_or("could not create the job object")?;
        unsafe {
            let proc = OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, 0, pid);
            if proc.is_null() {
                return Err(format!("could not open process {}", pid));
            }
            let ok = AssignProcessToJobObject(job, proc);
            CloseHandle(proc);
            if ok == 0 {
                return Err("could not add the process to the job object".to_string());
            }
        }
        Ok(())
    }
}

/// Waktu mulai proses menurut OS, sebagai teks pembanding. `None` bila proses
/// tidak ada. Unix: `ps -o lstart=`; Windows: tidak dipakai (job object).
pub(crate) async fn process_started(pid: u32) -> Option<String> {
    #[cfg(not(target_os = "windows"))]
    {
        let out = silent_command("ps")
            .args(["-o", "lstart=", "-p", &pid.to_string()])
            .stdout(Stdio::piped())
            .output()
            .await
            .ok()?;
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        (out.status.success() && !s.is_empty()).then_some(s)
    }
    #[cfg(target_os = "windows")]
    {
        let out = silent_command("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                &format!("(Get-Process -Id {} -ErrorAction Stop).StartTime.ToFileTimeUtc()", pid),
            ])
            .stdout(Stdio::piped())
            .output()
            .await
            .ok()?;
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        (out.status.success() && !s.is_empty()).then_some(s)
    }
}

fn persist_records(procs: &SiteProcs) {
    let Some(path) = procs.records_path.lock().unwrap().clone() else { return };
    let records = procs.records.lock().unwrap().clone();
    if records.is_empty() {
        let _ = std::fs::remove_file(&path);
        return;
    }
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_string_pretty(&records) {
        let _ = std::fs::write(&path, json);
    }
}

fn record_remove(procs: &SiteProcs, id: &str) {
    let removed = procs.records.lock().unwrap().remove(id).is_some();
    if removed {
        persist_records(procs);
    }
}

/// Aktifkan penyimpanan catatan di `path` (dipanggil sekali saat Kova start).
pub fn init_records<R: Runtime>(app: &AppHandle<R>, path: std::path::PathBuf) {
    *app.state::<SiteProcs>().records_path.lock().unwrap() = Some(path);
}

/// Matikan dev server sisa sesi sebelumnya yang tidak berhenti bersih
/// (Kova crash atau dimatikan paksa). PID hanya dimatikan bila waktu mulai
/// prosesnya sama dengan catatan, jadi proses lain yang kebetulan mendapat
/// PID yang sama tidak tersentuh. Mengembalikan jumlah proses yang dimatikan.
pub async fn cleanup_leftovers<R: Runtime>(app: &AppHandle<R>) -> usize {
    let procs = app.state::<SiteProcs>();
    let Some(path) = procs.records_path.lock().unwrap().clone() else { return 0 };
    let Ok(raw) = std::fs::read_to_string(&path) else { return 0 };
    let _ = std::fs::remove_file(&path);
    let records: HashMap<String, Record> = serde_json::from_str(&raw).unwrap_or_default();
    let mut killed = 0;
    for (id, rec) in records {
        if process_started(rec.pid).await.as_deref() == Some(rec.started.as_str()) {
            kill_tree(rec.pid).await;
            killed += 1;
            emit_log(app, "KOVA", &format!("stopping a dev server left from the previous session (project {}, pid {})", id, rec.pid), Some("warn"));
        }
    }
    killed
}

pub fn validate(site: &Project) -> Result<(), String> {
    if site.path.trim().is_empty() {
        return Err("the project folder is not chosen yet".to_string());
    }
    if !Path::new(&site.path).is_dir() {
        return Err(format!("folder not found: {}", site.path));
    }
    if site.command.trim().is_empty() && site.run_mode.is_command() {
        return Err("the command is empty".to_string());
    }
    if site.port < 1024 {
        return Err("port must be 1024–65535".to_string());
    }
    for (key, value) in &site.env {
        validate_env(key, value)?;
    }
    for e in &site.envs {
        for (key, value) in &e.vars {
            validate_env(key, value).map_err(|err| format!("{} ({})", err, e.name))?;
        }
    }
    Ok(())
}

/// Nama variabel: tanpa spasi, `=`, atau NUL. `PORT` milik Kova (port site).
pub fn validate_env(key: &str, value: &str) -> Result<(), String> {
    if key.is_empty() || key.chars().any(|c| c == '=' || c == '\0' || c.is_whitespace()) {
        return Err(format!("environment variable name is not valid: \"{}\"", key));
    }
    if key.eq_ignore_ascii_case("PORT") {
        return Err("PORT is set in the Port field, not in Environment".to_string());
    }
    if value.contains('\0') {
        return Err(format!("the value of {} contains a NUL character", key));
    }
    Ok(())
}

pub async fn start<R: Runtime>(app: &AppHandle<R>, site: &Project) -> Result<(), String> {
    validate(site)?;
    let procs = app.state::<SiteProcs>();
    if procs.is_active(&site.id) {
        return Err(format!("{} is already running", site.name));
    }
    if !port_free(site.port).await {
        return Err(format!(
            "port {} is used by another program. Change this project's port or stop that program.",
            site.port
        ));
    }

    let source = site.name.clone();
    // Run with: Dockerfile / Compose → script engine (lihat `container.rs`).
    let cplan = if site.run_mode.is_command() {
        None
    } else {
        let mut names: Vec<String> = vec!["PORT".into()];
        names.extend(site.effective_env().into_keys());
        Some(super::container::plan(site, crate::runtime::ensure_resolved().await, &names)?)
    };
    let line = match &cplan {
        Some(c) => c.summary.clone(),
        None => render_command(site.command.trim(), site.port),
    };
    // "Run inside WSL": command dijalankan di distro, bukan oleh cmd Windows.
    let wsl_target = if cfg!(target_os = "windows") && site.wsl && cplan.is_none() {
        Some(crate::wsl::parse(&site.path).ok_or_else(|| {
            "Run inside WSL needs a folder in a WSL distro (\\\\wsl.localhost\\<distro>\\…)".to_string()
        })?)
    } else {
        None
    };
    let unc = cfg!(target_os = "windows") && wsl_target.is_none() && cplan.is_none() && is_unc(&site.path);
    let mut cmd = match (&cplan, &wsl_target) {
        (Some(c), _) => {
            let mut x = silent_command(&c.program);
            x.args(&c.args);
            #[cfg(unix)]
            x.process_group(0);
            x
        }
        (None, Some(wp)) => {
            let mut c = silent_command("wsl.exe");
            c.args(crate::wsl::start_args(wp, &line));
            c
        }
        (None, None) => shell_command(&if unc { unc_wrap(&site.path, &line) } else { line.clone() }),
    };
    // Folder UNC dimasuki lewat `pushd` (atau `--cd` di WSL); folder kerja
    // proses Windows-nya sendiri harus lokal.
    cmd.current_dir(if unc || wsl_target.is_some() || (cplan.is_some() && is_unc(&site.path)) {
        std::env::temp_dir()
    } else {
        std::path::PathBuf::from(&site.path)
    })
        // stdin = pipe yang tetap terbuka (tidak pernah ditulis), seperti
        // terminal yang diam. Dengan `null`, app yang membaca stdin (mis.
        // `fmt.Scanln()` sebagai penahan, atau goroutine pembaca perintah)
        // langsung dapat EOF dan keluar dengan exit 0 tepat setelah start.
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("PORT", site.port.to_string())
        // `php artisan serve` membaca SERVER_PORT/SERVER_HOST sebagai
        // default, jadi `composer run dev` (artisan serve di dalam
        // concurrently) ikut port project. Environment project tetap menang.
        .env("SERVER_PORT", site.port.to_string())
        .env("SERVER_HOST", "127.0.0.1")
        // Jangan buka browser sendiri (CRA) dan jangan beri warna di log.
        .env("BROWSER", "none")
        .env("NO_COLOR", "1")
        .env("FORCE_COLOR", "0");

    // Versi PHP/Node/Go mengikuti mise.toml project bila mise terpasang.
    // Di WSL, mise milik distro yang dipakai (lihat `wsl::start_script`).
    if wsl_target.is_none() && cplan.is_none() && crate::mise::version().await.is_some() {
        match crate::mise::env_for(Path::new(&site.path)).await {
            Ok(env) => {
                cmd.envs(env);
            }
            Err(e) => emit_log(
                app,
                &source,
                &format!("mise env failed ({}); using the versions on PATH", e),
                Some("warn"),
            ),
        }
    }
    // Environment milik project dipasang paling akhir, jadi menimpa nilai mise.
    // Default lalu override environment aktif.
    let env = site.effective_env();
    let env_label = site.active_env_entry().map(|e| e.name.as_str()).unwrap_or("Default");
    if !env.is_empty() {
        cmd.envs(&env);
        let keys: Vec<&str> = env.keys().map(String::as_str).collect();
        emit_log(app, &source, &format!("env ({}): {}", env_label, keys.join(", ")), None);
    } else if site.active_env_entry().is_some() {
        emit_log(app, &source, &format!("env ({}): none", env_label), None);
    }

    // WSL hanya meneruskan variabel yang namanya ada di WSLENV.
    if wsl_target.is_some() {
        let mut names: Vec<&str> = vec!["PORT", "SERVER_PORT", "SERVER_HOST", "BROWSER", "NO_COLOR", "FORCE_COLOR"];
        names.extend(env.keys().map(String::as_str));
        let existing = std::env::var("WSLENV").ok();
        cmd.env("WSLENV", crate::wsl::wslenv(existing.as_deref(), &names));
    }

    let place = match (&cplan, &wsl_target) {
        (Some(_), _) => format!("{} · container", site.path),
        (None, Some(wp)) => format!("{}:{} in WSL", wp.distro, wp.path),
        (None, None) => site.path.clone(),
    };
    emit_log(app, &source, &format!("$ {}  (in {})", line, place), None);
    let mut child = cmd.spawn().map_err(|e| format!("could not run the command: {}", e))?;
    let pid = child.id();
    // Dipegang sampai proses selesai supaya stdin-nya tidak pernah EOF.
    let hold_stdin = child.stdin.take();

    // Pastikan dev server tidak tertinggal bila Kova mati paksa.
    if let Some(pid) = pid {
        #[cfg(target_os = "windows")]
        if let Err(e) = job::assign(pid) {
            emit_log(app, &source, &format!("{}; the process may be left running if Kova crashes", e), Some("warn"));
        }
        if let Some(started) = process_started(pid).await {
            procs.records.lock().unwrap().insert(site.id.clone(), Record { pid, started });
            persist_records(&procs);
        }
    }

    let generation = {
        let mut c = procs.counter.lock().unwrap();
        *c += 1;
        *c
    };
    procs.entries.lock().unwrap().insert(
        site.id.clone(),
        Entry {
            pid,
            state: SiteState::Starting,
            exit_code: None,
            last_line: None,
            stopping: false,
            generation,
            env_name: site.active_env_entry().map(|e| e.name.clone()),
            wsl_group: None,
            printed_port: None,
        },
    );
    emit_status(app);

    // Drive sementara dari `pushd` (folder UNC), dilepas setelah proses selesai.
    let drive: std::sync::Arc<Mutex<Option<String>>> = Default::default();

    // Output → panel log. stderr diperlakukan sebagai output biasa: `php -S`
    // dan banyak dev server menulis log normalnya ke stderr.
    for (reader_app, id, src, stream, capture) in [
        (
            app.clone(),
            site.id.clone(),
            source.clone(),
            child.stdout.take().map(Pipe::Out),
            match &wsl_target {
                Some(wp) => Some(Capture::Pgid(wp.distro.clone())),
                None => unc.then(|| Capture::Drive(drive.clone())),
            },
        ),
        (app.clone(), site.id.clone(), source.clone(), child.stderr.take().map(Pipe::Err), None),
    ] {
        let Some(stream) = stream else { continue };
        tauri::async_runtime::spawn(async move {
            match stream {
                Pipe::Out(s) => pump(reader_app, id, src, generation, BufReader::new(s), capture).await,
                Pipe::Err(s) => pump(reader_app, id, src, generation, BufReader::new(s), capture).await,
            }
        });
    }

    // Tunggu proses selesai → Stopped (diminta) atau Exited (berhenti sendiri).
    let wait_app = app.clone();
    let wait_id = site.id.clone();
    let wait_src = source.clone();
    let wait_cmd = line.clone();
    let wait_drive = drive.clone();
    let wait_distro = wsl_target.as_ref().map(|w| w.distro.clone());
    let wait_port = site.port;
    let wait_cleanup = cplan.as_ref().map(|c| c.cleanup.clone());
    tauri::async_runtime::spawn(async move {
        let code = child.wait().await.ok().and_then(|s| s.code());
        drop(hold_stdin);
        if unc {
            release_drive(&wait_drive).await;
        }
        // Mode container: hapus container / compose down supaya nama dan port bebas.
        if let Some(c) = &wait_cleanup {
            super::container::cleanup(c).await;
        }
        let procs = wait_app.state::<SiteProcs>();
        record_remove(&procs, &wait_id);
        let mut requested = false;
        let mut ever_ready = false;
        {
            let mut map = procs.entries.lock().unwrap();
            if let Some(e) = map.get_mut(&wait_id) {
                if e.generation != generation {
                    return;
                }
                requested = e.stopping;
                ever_ready = e.state == SiteState::Running;
                e.pid = None;
                e.exit_code = code;
                e.state = if requested { SiteState::Stopped } else { SiteState::Exited };
                if requested {
                    e.last_line = None;
                }
            }
        }
        if requested {
            emit_log(&wait_app, &wait_src, "stopped", None);
        } else {
            let msg = match code {
                Some(c) => format!("the process ended (exit {})", c),
                None => "the process ended".to_string(),
            };
            emit_log(&wait_app, &wait_src, &msg, Some("error"));
            let hint = match &wait_distro {
                Some(d) => wsl_not_found_hint(code, &wait_cmd, d),
                None => not_found_hint(code, &wait_cmd),
            }
            .or_else(|| no_listen_hint(code, ever_ready, wait_port));
            if let Some(hint) = hint {
                emit_log(&wait_app, &wait_src, &hint, Some("error"));
                let mut map = procs.entries.lock().unwrap();
                if let Some(e) = map.get_mut(&wait_id).filter(|e| e.generation == generation) {
                    e.last_line = Some(hint);
                }
            }
        }
        emit_status(&wait_app);
    });

    // Tunggu port menerima koneksi.
    let ready_app = app.clone();
    let ready_id = site.id.clone();
    let port = site.port;
    tauri::async_runtime::spawn(async move {
        let deadline = tokio::time::Instant::now() + READY_TIMEOUT;
        loop {
            let timed_out = tokio::time::Instant::now() >= deadline;
            tokio::time::sleep(if timed_out { LATE_INTERVAL } else { READY_INTERVAL }).await;
            let procs = ready_app.state::<SiteProcs>();
            let current = procs
                .entries
                .lock()
                .unwrap()
                .get(&ready_id)
                .filter(|e| e.generation == generation)
                .map(|e| e.state);
            // Berhenti bila proses sudah diganti, dihentikan, atau keluar.
            let Some(current) = current.filter(|s| matches!(s, SiteState::Starting | SiteState::Unresponsive)) else {
                return;
            };
            let open = port_open(port).await;
            let timed_out = tokio::time::Instant::now() >= deadline;
            let Some(next) = ready_transition(current, open, timed_out) else {
                continue;
            };
            let changed = {
                let mut map = procs.entries.lock().unwrap();
                match map.get_mut(&ready_id) {
                    Some(e) if e.generation == generation && e.state == current => {
                        e.state = next;
                        true
                    }
                    _ => false,
                }
            };
            if changed {
                emit_status(&ready_app);
            }
            if next == SiteState::Running {
                return;
            }
        }
    });

    Ok(())
}

/// Baris pertama stdout yang dibaca Kova sendiri, tidak ditampilkan.
enum Capture {
    /// `cd` setelah `pushd`: drive sementara folder UNC.
    Drive(std::sync::Arc<Mutex<Option<String>>>),
    /// Penanda PGID dari `wsl::start_script`, untuk Stop di dalam distro.
    Pgid(String),
}

enum Pipe {
    Out(tokio::process::ChildStdout),
    Err(tokio::process::ChildStderr),
}

async fn pump<R: Runtime, B: tokio::io::AsyncBufRead + Unpin>(
    app: AppHandle<R>,
    id: String,
    source: String,
    generation: u64,
    reader: B,
    mut capture: Option<Capture>,
) {
    let mut lines = reader.lines();
    let mut hinted = false;
    while let Ok(Some(raw)) = lines.next_line().await {
        let line = strip_ansi(&raw);
        let line = line.trim_end();
        if line.trim().is_empty() {
            continue;
        }
        // Baris pertama dari `cd` (folder UNC): simpan drive-nya, jangan tampilkan.
        match capture.take() {
            Some(Capture::Drive(slot)) => {
                if let Some(d) = mapped_drive(line) {
                    *slot.lock().unwrap() = Some(d);
                    continue;
                }
            }
            Some(Capture::Pgid(distro)) => {
                if let Some(pgid) = crate::wsl::parse_pgid(line) {
                    let procs = app.state::<SiteProcs>();
                    if let Some(e) = procs.entries.lock().unwrap().get_mut(&id).filter(|e| e.generation == generation) {
                        e.wsl_group = Some((distro, pgid));
                    }
                    continue;
                }
            }
            None => {}
        }
        emit_log(&app, &source, line, line_level(line));
        let hint = if hinted { None } else { output_hint(line) };
        if let Some(h) = hint {
            hinted = true;
            emit_log(&app, &source, h, Some("error"));
        }
        let procs = app.state::<SiteProcs>();
        let mut map = procs.entries.lock().unwrap();
        if let Some(e) = map.get_mut(&id) {
            if e.generation == generation {
                e.last_line = Some(hint.unwrap_or(line).to_string());
                if let Some(port) = printed_port(line) {
                    e.printed_port = Some(port);
                }
            }
        }
    }
}

/// Lepas drive sementara dari `pushd`. Proses yang dihentikan paksa tidak
/// sempat menjalankan `popd`, jadi Kova melepasnya sendiri. Baris `cd` bisa
/// baru terbaca sesaat setelah proses selesai, jadi tunggu sebentar.
async fn release_drive(slot: &std::sync::Arc<Mutex<Option<String>>>) {
    let mut d = slot.lock().unwrap().take();
    if d.is_none() {
        tokio::time::sleep(Duration::from_millis(300)).await;
        d = slot.lock().unwrap().take();
    }
    if let Some(d) = d {
        let _ = silent_command("net").args(["use", &d, "/delete", "/y"]).output().await;
    }
}

pub async fn stop<R: Runtime>(app: &AppHandle<R>, id: &str) -> Result<(), String> {
    let procs = app.state::<SiteProcs>();
    let target = {
        let mut map = procs.entries.lock().unwrap();
        match map.get_mut(id) {
            Some(e) if e.pid.is_some() => {
                e.stopping = true;
                e.pid.map(|p| (p, e.wsl_group.clone()))
            }
            _ => None,
        }
    };
    if let Some((pid, group)) = target {
        stop_one(app, id, pid, group).await;
    }
    Ok(())
}

/// Hentikan satu dev server. Di WSL, grup proses di distro diberi TERM dulu
/// (dev server sempat menutup port), lalu KILL bila belum selesai dalam 5 dtk.
/// `wsl.exe` di Windows dimatikan terakhir.
async fn stop_one<R: Runtime>(app: &AppHandle<R>, id: &str, pid: u32, group: Option<(String, u32)>) {
    if let Some((distro, pgid)) = &group {
        crate::wsl::kill_group(distro, *pgid, "TERM").await;
        let procs = app.state::<SiteProcs>();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while procs.entries.lock().unwrap().get(id).is_some_and(|e| e.pid == Some(pid)) {
            if tokio::time::Instant::now() >= deadline {
                crate::wsl::kill_group(distro, *pgid, "KILL").await;
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
    kill_tree(pid).await;
}

/// Stop lalu start lagi. Menunggu proses lama benar-benar selesai dan port
/// bebas, supaya start baru tidak ditolak dengan "port sedang dipakai".
pub async fn restart<R: Runtime>(app: &AppHandle<R>, site: &Project) -> Result<(), String> {
    validate(site)?;
    stop(app, &site.id).await?;
    let procs = app.state::<SiteProcs>();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    while procs.is_active(&site.id) || !port_free(site.port).await {
        if tokio::time::Instant::now() >= deadline {
            return Err(format!(
                "{} has not stopped after 15 s; port {} is still in use",
                site.name, site.port
            ));
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    start(app, site).await
}

/// Hentikan semua dev server (dipanggil saat Kova keluar).
pub async fn stop_all<R: Runtime>(app: &AppHandle<R>) {
    let procs = app.state::<SiteProcs>();
    let targets: Vec<(String, u32, Option<(String, u32)>)> = {
        let mut map = procs.entries.lock().unwrap();
        map.iter_mut()
            .filter_map(|(id, e)| {
                e.stopping = true;
                e.pid.map(|p| (id.clone(), p, e.wsl_group.clone()))
            })
            .collect()
    };
    // Semua sekaligus: tiap project WSL bisa menunggu sampai 5 dtk.
    let handles: Vec<_> = targets
        .into_iter()
        .map(|(id, pid, group)| {
            let app = app.clone();
            tauri::async_runtime::spawn(async move { stop_one(&app, &id, pid, group).await })
        })
        .collect();
    for h in handles {
        let _ = h.await;
    }
    // Tunggu proses benar-benar selesai (maks 10 dtk), supaya port dan drive
    // sementara sudah lepas sebelum Kova keluar atau installer berjalan.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while procs.entries.lock().unwrap().values().any(|e| e.pid.is_some()) {
        if tokio::time::Instant::now() >= deadline {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Hapus catatan site yang sudah tidak berjalan (mis. setelah site dihapus).
pub fn forget<R: Runtime>(app: &AppHandle<R>, id: &str) {
    let procs = app.state::<SiteProcs>();
    let mut map = procs.entries.lock().unwrap();
    if map.get(id).is_some_and(|e| e.pid.is_none()) {
        map.remove(id);
    }
}

pub fn snapshot<R: Runtime>(app: &AppHandle<R>) -> Vec<SiteRuntime> {
    app.state::<SiteProcs>().snapshot()
}

/// PID akar dev server yang sedang hidup, untuk menghitung RAM project.
pub fn running_pids<R: Runtime>(app: &AppHandle<R>) -> Vec<u32> {
    app.state::<SiteProcs>().entries.lock().unwrap().values().filter_map(|e| e.pid).collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn finds_the_printed_port() {
        use super::printed_port;
        assert_eq!(printed_port("  ➜  Local:   http://localhost:3000/"), Some(3000));
        assert_eq!(printed_port("INFO  Server running on [http://127.0.0.1:8001]."), Some(8001));
        assert_eq!(printed_port("listening on :8080"), Some(8080));
        assert_eq!(printed_port("Listening on port 4000"), Some(4000));
        assert_eq!(printed_port("Network: http://0.0.0.0:5173/"), Some(5173));
        assert_eq!(printed_port("connected to postgres at localhost:5432 then http://localhost:3001"), Some(3001));
        assert_eq!(printed_port("connected to localhost:5432"), None);
        assert_eq!(printed_port("GET /api 200 in 12ms"), None);
        assert_eq!(printed_port("http://localhost:80/"), None);
    }

    #[test]
    fn unc_folder_runs_through_pushd() {
        use super::{is_unc, mapped_drive, output_hint, unc_wrap};
        assert!(is_unc(r"\\wsl.localhost\Ubuntu-24.04\home\dev\app"));
        assert!(!is_unc(r"D:\dev\app"));
        assert_eq!(
            unc_wrap(r"\\wsl.localhost\U\app", "go run . & echo x"),
            r#"pushd "\\wsl.localhost\U\app" || exit /b 1 & cd & go run . & echo x"#
        );
        assert_eq!(mapped_drive(r"z:\home\dev\app").as_deref(), Some("Z:"));
        assert_eq!(mapped_drive("listening on :8080"), None);
        assert!(output_hint("Error code: RPC_S_SERVER_UNAVAILABLE").unwrap().contains("wsl --shutdown"));
        assert_eq!(output_hint("ok"), None);
    }

    #[test]
    fn missing_program_gets_a_named_hint() {
        use super::not_found_hint;
        assert!(not_found_hint(Some(127), "composer run dev").unwrap().starts_with("composer was not found."));
        assert!(not_found_hint(Some(9009), "pnpm run dev --port 3000").unwrap().contains("corepack"));
        assert!(not_found_hint(Some(9009), "\"my tool\" x").is_some());
        assert_eq!(not_found_hint(Some(1), "composer run dev"), None);
        assert_eq!(not_found_hint(None, "composer run dev"), None);
    }

    #[test]
    fn exit_zero_without_listening_gets_a_hint() {
        let h = no_listen_hint(Some(0), false, 8081).unwrap();
        assert!(h.contains("port 8081") && h.contains("PORT=\":{port}\""));
        assert!(no_listen_hint(Some(0), true, 8081).is_none());
        assert!(no_listen_hint(Some(1), false, 8081).is_none());
        assert!(no_listen_hint(None, false, 8081).is_none());
    }

    #[test]
    fn late_answer_moves_unresponsive_to_running() {
        use super::{ready_transition, SiteState::*};
        assert_eq!(ready_transition(Starting, false, false), None);
        assert_eq!(ready_transition(Starting, true, false), Some(Running));
        assert_eq!(ready_transition(Starting, false, true), Some(Unresponsive));
        // Tetap dicek setelah batas; jawaban terlambat = running.
        assert_eq!(ready_transition(Unresponsive, false, true), None);
        assert_eq!(ready_transition(Unresponsive, true, true), Some(Running));
        assert_eq!(ready_transition(Exited, true, true), None);
        assert_eq!(ready_transition(Stopped, true, true), None);
    }

    use super::*;

    fn site(path: &str, command: &str, port: u16) -> Project {
        Project {
            id: "s".into(),
            name: "app".into(),
            path: path.into(),
            profile_id: None,
            kind: "node".into(),
            command: command.into(),
            port,
            auto_start: false,
            env: Default::default(),
            envs: Vec::new(),
            active_env: None,
            start_services: false,
            group_id: None,
            wsl: false,
            run_mode: Default::default(),
            container: None,
        }
    }

    #[test]
    fn line_level_marks_obvious_problems() {
        assert_eq!(line_level("Error: Cannot find module 'dotenv'"), Some("error"));
        assert_eq!(line_level("PHP Fatal error:  Uncaught Exception in /app/x.php:3"), Some("error"));
        assert_eq!(line_level("[nuxt] WARN  Deprecated option"), Some("warn"));
        assert_eq!(line_level("✓ built in 533ms, 0 errors"), None);
        assert_eq!(line_level("[Thu Nov 12] 127.0.0.1:54729 Accepted"), None);
    }

    #[test]
    fn strip_ansi_removes_colors() {
        assert_eq!(strip_ansi("\u{1b}[32m  VITE v5.4.0\u{1b}[39m  ready"), "  VITE v5.4.0  ready");
        assert_eq!(strip_ansi("\u{1b}[2K\u{1b}[1Gdone"), "done");
        assert_eq!(strip_ansi("plain"), "plain");
        assert_eq!(strip_ansi("\u{1b}]0;mise\u{7}installing"), "installing");
    }

    #[test]
    fn validate_rejects_incomplete_sites() {
        let dir = std::env::temp_dir();
        let dir = dir.to_string_lossy();
        assert!(validate(&site("", "npm run dev", 3000)).unwrap_err().contains("folder"));
        assert!(validate(&site("/definitely/not/here", "npm run dev", 3000)).is_err());
        assert!(validate(&site(&dir, "  ", 3000)).unwrap_err().contains("command"));
        assert!(validate(&site(&dir, "npm run dev", 80)).unwrap_err().contains("port"));
        assert!(validate(&site(&dir, "npm run dev", 3000)).is_ok());

        let mut p = site(&dir, "npm run dev", 3000);
        p.env.insert("SSL_CERT_FILE".into(), r"C:\Program Files\Git\usr\ssl\certs\ca-bundle.crt".into());
        assert!(validate(&p).is_ok());
        p.env.insert("port".into(), "1".into());
        assert!(validate(&p).unwrap_err().contains("PORT"));
        assert!(validate_env("A B", "x").is_err());
        assert!(validate_env("A=B", "x").is_err());
        assert!(validate_env("", "x").is_err());
        assert!(validate_env("NODE_OPTIONS", "--max-old-space-size=4096").is_ok());
    }

    #[tokio::test]
    async fn port_checks_see_a_listener() {
        let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(port_open(port).await);
        assert!(!port_free(port).await);
        drop(listener);

        // Port yang baru dilepas bisa langsung diambil test lain yang berjalan
        // paralel (mis. `start_then_stop_frees_the_port` memilih port lewat
        // bind ke 0), sehingga "tidak ada yang mendengarkan" di port itu tidak
        // bisa dijamin. Untuk kasus tertutup, port dipegang socket yang di-bind
        // tetapi tidak listen (tanpa SO_REUSEADDR): koneksi ditolak dan tidak
        // ada test lain yang bisa memakainya.
        let held = tokio::net::TcpSocket::new_v4().unwrap();
        held.bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))).unwrap();
        let held_port = held.local_addr().unwrap().port();
        assert!(!port_open(held_port).await);
        assert!(!port_free(held_port).await);
        drop(held);

        // Kasus bebas: tetap bisa direbut test lain di sela waktu, jadi coba
        // beberapa port baru; gagal hanya bila semuanya bentrok.
        let mut found_free = false;
        for _ in 0..5 {
            let p = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap().local_addr().unwrap().port();
            if port_free(p).await {
                found_free = true;
                break;
            }
        }
        assert!(found_free, "port_free harus true untuk port yang tidak dipakai");
    }

    /// Uji ujung ke ujung di Unix: jalankan server sungguhan lewat shell,
    /// tunggu port menjawab, lalu Stop harus mematikan seluruh pohon proses
    /// (induk `sh` + anak) dan membebaskan port.
    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn start_then_stop_frees_the_port() {
        if std::process::Command::new("python3").arg("--version").output().is_err() {
            eprintln!("python3 tidak ada; uji dilewati");
            return;
        }
        let app = tauri::test::mock_app();
        app.manage(SiteProcs::default());
        let handle = app.handle().clone();

        let port = {
            let l = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
            l.local_addr().unwrap().port()
        };
        let dir = std::env::temp_dir();
        // `sh -c "<cmd> & wait"` membuat anak terpisah dari shell, seperti
        // npm → node: Stop harus mematikan keduanya. Port dari SERVER_PORT
        // seperti `php artisan serve` di dalam `composer run dev`.
        let mut p = site(&dir.to_string_lossy(), "python3 -m http.server $SERVER_PORT --bind $SERVER_HOST & wait", port);
        p.id = "e2e".into();

        start(&handle, &p).await.expect("start gagal");
        assert!(start(&handle, &p).await.is_err(), "start kedua harus ditolak");

        let mut state = SiteState::Starting;
        for _ in 0..60 {
            tokio::time::sleep(Duration::from_millis(250)).await;
            state = snapshot(&handle).iter().find(|r| r.id == "e2e").map(|r| r.state).unwrap();
            if state != SiteState::Starting {
                break;
            }
        }
        assert_eq!(state, SiteState::Running);
        assert!(port_open(port).await);

        stop(&handle, "e2e").await.unwrap();
        let mut stopped = false;
        for _ in 0..40 {
            tokio::time::sleep(Duration::from_millis(250)).await;
            let st = snapshot(&handle).iter().find(|r| r.id == "e2e").map(|r| r.state).unwrap();
            if st == SiteState::Stopped && port_free(port).await {
                stopped = true;
                break;
            }
        }
        assert!(stopped, "setelah Stop, status harus stopped dan port bebas");

        // Restart dari keadaan berhenti = start; dari keadaan jalan = stop + start.
        restart(&handle, &p).await.expect("restart (dari berhenti) gagal");
        restart(&handle, &p).await.expect("restart (dari jalan) gagal");
        let mut running = false;
        for _ in 0..60 {
            tokio::time::sleep(Duration::from_millis(250)).await;
            let st = snapshot(&handle).iter().find(|r| r.id == "e2e").map(|r| r.state).unwrap();
            if st == SiteState::Running {
                running = true;
                break;
            }
        }
        assert!(running && port_open(port).await, "setelah Restart, server harus jalan lagi");
        stop(&handle, "e2e").await.unwrap();
    }

    /// App yang membaca stdin (mis. Go dengan `fmt.Scanln()` sebagai penahan)
    /// tidak boleh langsung keluar: stdin-nya tetap terbuka sampai Stop.
    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn stdin_stays_open_until_stop() {
        let app = tauri::test::mock_app();
        app.manage(SiteProcs::default());
        let handle = app.handle().clone();
        let port = {
            let l = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
            l.local_addr().unwrap().port()
        };
        let dir = std::env::temp_dir();
        let mut p = site(&dir.to_string_lossy(), "echo started; read x; echo eof", port);
        p.id = "stdin".into();
        start(&handle, &p).await.expect("start gagal");
        tokio::time::sleep(Duration::from_millis(800)).await;
        let st = snapshot(&handle).into_iter().find(|r| r.id == "stdin").unwrap();
        assert_ne!(st.state, SiteState::Exited, "proses keluar karena stdin EOF");
        stop(&handle, "stdin").await.unwrap();
    }

    /// Exit 0 tanpa port pernah menjawab → petunjuk soal PORT jadi baris terakhir.
    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn clean_exit_without_listening_shows_the_hint() {
        let app = tauri::test::mock_app();
        app.manage(SiteProcs::default());
        let handle = app.handle().clone();
        let port = {
            let l = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
            l.local_addr().unwrap().port()
        };
        let dir = std::env::temp_dir();
        let mut p = site(&dir.to_string_lossy(), "echo \"Server started on $PORT\"", port);
        p.id = "nolisten".into();
        start(&handle, &p).await.expect("start gagal");
        let mut info = None;
        for _ in 0..40 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            info = snapshot(&handle).into_iter().find(|r| r.id == "nolisten");
            if info.as_ref().is_some_and(|r| r.state == SiteState::Exited) {
                break;
            }
        }
        let info = info.unwrap();
        assert_eq!((info.state, info.exit_code), (SiteState::Exited, Some(0)));
        assert!(info.last_line.unwrap().contains("without listening on port"));
    }

    /// Proses yang berhenti sendiri dilaporkan `exited` dengan exit code dan
    /// baris output terakhirnya.
    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn crash_is_reported_with_code_and_last_line() {
        let app = tauri::test::mock_app();
        app.manage(SiteProcs::default());
        let handle = app.handle().clone();
        let port = {
            let l = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
            l.local_addr().unwrap().port()
        };
        let dir = std::env::temp_dir();
        let mut p = site(&dir.to_string_lossy(), "echo \"V=$SERVEL_TEST_VAR\"; exit 3", port);
        p.id = "crash".into();
        p.env.insert("SERVEL_TEST_VAR".into(), "dari-project".into());
        start(&handle, &p).await.expect("start gagal");

        let mut info = None;
        for _ in 0..40 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            info = snapshot(&handle).into_iter().find(|r| r.id == "crash");
            if info.as_ref().is_some_and(|r| r.state == SiteState::Exited) {
                break;
            }
        }
        let info = info.unwrap();
        assert_eq!(info.state, SiteState::Exited);
        assert_eq!(info.exit_code, Some(3));
        // Environment milik project sampai ke proses.
        assert_eq!(info.last_line.as_deref(), Some("V=dari-project"));
    }

    /// Sisa sesi sebelumnya: proses yang masih hidup dengan waktu mulai yang
    /// cocok dimatikan; catatan dengan waktu mulai berbeda (PID dipakai ulang)
    /// dibiarkan.
    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn leftovers_are_killed_only_when_start_time_matches() {
        use std::os::unix::process::CommandExt;
        let app = tauri::test::mock_app();
        app.manage(SiteProcs::default());
        let handle = app.handle().clone();
        let path = std::env::temp_dir().join(format!("servel-records-{}.json", std::process::id()));
        init_records(&handle, path.clone());

        // "Sisa" dev server: proses dengan group sendiri, seperti yang dibuat start().
        let mut orphan = std::process::Command::new("sleep").arg("60").process_group(0).spawn().unwrap();
        let mut other = std::process::Command::new("sleep").arg("60").process_group(0).spawn().unwrap();
        let started = process_started(orphan.id()).await.unwrap();
        let records: HashMap<String, Record> = [
            ("a".to_string(), Record { pid: orphan.id(), started }),
            ("b".to_string(), Record { pid: other.id(), started: "Thu Jan  1 00:00:00 1970".into() }),
        ]
        .into();
        std::fs::write(&path, serde_json::to_string(&records).unwrap()).unwrap();

        assert_eq!(cleanup_leftovers(&handle).await, 1);
        let status = tokio::task::spawn_blocking(move || orphan.wait()).await.unwrap().unwrap();
        assert!(!status.success(), "proses sisa harus dimatikan");
        assert!(other.try_wait().unwrap().is_none(), "PID dengan waktu mulai berbeda tidak boleh disentuh");
        assert!(!path.exists(), "catatan dihapus setelah dibersihkan");
        let _ = other.kill();
        let _ = other.wait();
    }
}
