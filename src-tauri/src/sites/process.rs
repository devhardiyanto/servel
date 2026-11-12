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
}

#[derive(Default)]
pub struct SiteProcs {
    entries: Mutex<HashMap<String, Entry>>,
    counter: Mutex<u64>,
    /// Catatan dev server yang sedang jalan, disimpan ke disk supaya sisa
    /// dari sesi yang mati paksa bisa dibersihkan saat Servel dibuka lagi.
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
            })
            .collect();
        out.sort_by(|a, b| a.id.cmp(&b.id));
        out
    }

    fn is_active(&self, id: &str) -> bool {
        self.entries.lock().unwrap().get(id).is_some_and(|e| {
            matches!(
                e.state,
                SiteState::Starting | SiteState::Running | SiteState::Unresponsive
            )
        })
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
        let attempt = tokio::time::timeout(
            Duration::from_millis(400),
            tokio::net::TcpStream::connect(addr),
        )
        .await;
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

async fn kill_tree(pid: u32) {
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
        let _ = silent_command("kill")
            .args(["-TERM", "--", &group])
            .status()
            .await;
        // Beri waktu berhenti baik-baik, lalu paksa bila masih hidup.
        tokio::time::sleep(Duration::from_secs(3)).await;
        let _ = silent_command("kill")
            .args(["-KILL", "--", &group])
            .status()
            .await;
    }
}

/// Windows: dev server dimasukkan ke job object dengan
/// `KILL_ON_JOB_CLOSE`. Handle job hanya dipegang Servel, jadi saat Servel
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
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_SET_QUOTA, PROCESS_TERMINATE,
    };

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
        let job = job().ok_or("job object tidak bisa dibuat")?;
        unsafe {
            let proc = OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, 0, pid);
            if proc.is_null() {
                return Err(format!("proses {} tidak bisa dibuka", pid));
            }
            let ok = AssignProcessToJobObject(job, proc);
            CloseHandle(proc);
            if ok == 0 {
                return Err("proses tidak bisa dimasukkan ke job object".to_string());
            }
        }
        Ok(())
    }
}

/// Waktu mulai proses menurut OS, sebagai teks pembanding. `None` bila proses
/// tidak ada. Unix: `ps -o lstart=`; Windows: tidak dipakai (job object).
async fn process_started(pid: u32) -> Option<String> {
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
                &format!(
                    "(Get-Process -Id {} -ErrorAction Stop).StartTime.ToFileTimeUtc()",
                    pid
                ),
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
    let Some(path) = procs.records_path.lock().unwrap().clone() else {
        return;
    };
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

/// Aktifkan penyimpanan catatan di `path` (dipanggil sekali saat Servel start).
pub fn init_records<R: Runtime>(app: &AppHandle<R>, path: std::path::PathBuf) {
    *app.state::<SiteProcs>().records_path.lock().unwrap() = Some(path);
}

/// Matikan dev server sisa sesi sebelumnya yang tidak berhenti bersih
/// (Servel crash atau dimatikan paksa). PID hanya dimatikan bila waktu mulai
/// prosesnya sama dengan catatan, jadi proses lain yang kebetulan mendapat
/// PID yang sama tidak tersentuh. Mengembalikan jumlah proses yang dimatikan.
pub async fn cleanup_leftovers<R: Runtime>(app: &AppHandle<R>) -> usize {
    let procs = app.state::<SiteProcs>();
    let Some(path) = procs.records_path.lock().unwrap().clone() else {
        return 0;
    };
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return 0;
    };
    let _ = std::fs::remove_file(&path);
    let records: HashMap<String, Record> = serde_json::from_str(&raw).unwrap_or_default();
    let mut killed = 0;
    for (id, rec) in records {
        if process_started(rec.pid).await.as_deref() == Some(rec.started.as_str()) {
            kill_tree(rec.pid).await;
            killed += 1;
            emit_log(
                app,
                "SERVEL",
                &format!(
                    "menghentikan dev server sisa sesi sebelumnya (project {}, pid {})",
                    id, rec.pid
                ),
                Some("warn"),
            );
        }
    }
    killed
}

pub fn validate(site: &Project) -> Result<(), String> {
    if site.path.trim().is_empty() {
        return Err("folder project belum dipilih".to_string());
    }
    if !Path::new(&site.path).is_dir() {
        return Err(format!("folder tidak ditemukan: {}", site.path));
    }
    if site.command.trim().is_empty() {
        return Err("perintah belum diisi".to_string());
    }
    if site.port < 1024 {
        return Err("port harus 1024–65535".to_string());
    }
    for (key, value) in &site.env {
        validate_env(key, value)?;
    }
    Ok(())
}

/// Nama variabel: tanpa spasi, `=`, atau NUL. `PORT` milik Servel (port site).
pub fn validate_env(key: &str, value: &str) -> Result<(), String> {
    if key.is_empty()
        || key
            .chars()
            .any(|c| c == '=' || c == '\0' || c.is_whitespace())
    {
        return Err(format!(
            "nama environment variable tidak valid: \"{}\"",
            key
        ));
    }
    if key.eq_ignore_ascii_case("PORT") {
        return Err("PORT diatur dari kolom Port, bukan dari Environment".to_string());
    }
    if value.contains('\0') {
        return Err(format!("nilai {} mengandung karakter NUL", key));
    }
    Ok(())
}

pub async fn start<R: Runtime>(app: &AppHandle<R>, site: &Project) -> Result<(), String> {
    validate(site)?;
    let procs = app.state::<SiteProcs>();
    if procs.is_active(&site.id) {
        return Err(format!("{} sudah berjalan", site.name));
    }
    if !port_free(site.port).await {
        return Err(format!(
            "port {} sedang dipakai program lain. Ganti port site ini atau hentikan program itu.",
            site.port
        ));
    }

    let source = site.name.clone();
    let line = render_command(site.command.trim(), site.port);
    let mut cmd = shell_command(&line);
    cmd.current_dir(&site.path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("PORT", site.port.to_string())
        // Jangan buka browser sendiri (CRA) dan jangan beri warna di log.
        .env("BROWSER", "none")
        .env("NO_COLOR", "1")
        .env("FORCE_COLOR", "0");

    // Versi PHP/Node/Go mengikuti mise.toml project bila mise terpasang.
    if crate::mise::version().await.is_some() {
        match crate::mise::env_for(Path::new(&site.path)).await {
            Ok(env) => {
                cmd.envs(env);
            }
            Err(e) => emit_log(
                app,
                &source,
                &format!("mise env gagal ({}); memakai versi dari PATH", e),
                Some("warn"),
            ),
        }
    }
    // Environment milik project dipasang paling akhir, jadi menimpa nilai mise.
    if !site.env.is_empty() {
        cmd.envs(&site.env);
        let keys: Vec<&str> = site.env.keys().map(String::as_str).collect();
        emit_log(app, &source, &format!("env: {}", keys.join(", ")), None);
    }

    emit_log(
        app,
        &source,
        &format!("$ {}  (di {})", line, site.path),
        None,
    );
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("gagal menjalankan perintah: {}", e))?;
    let pid = child.id();

    // Pastikan dev server tidak tertinggal bila Servel mati paksa.
    if let Some(pid) = pid {
        #[cfg(target_os = "windows")]
        if let Err(e) = job::assign(pid) {
            emit_log(
                app,
                &source,
                &format!("{}; proses mungkin tertinggal bila Servel crash", e),
                Some("warn"),
            );
        }
        if let Some(started) = process_started(pid).await {
            procs
                .records
                .lock()
                .unwrap()
                .insert(site.id.clone(), Record { pid, started });
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
        },
    );
    emit_status(app);

    // Output → panel log. stderr diperlakukan sebagai output biasa: `php -S`
    // dan banyak dev server menulis log normalnya ke stderr.
    for (reader_app, id, src, stream) in [
        (
            app.clone(),
            site.id.clone(),
            source.clone(),
            child.stdout.take().map(Pipe::Out),
        ),
        (
            app.clone(),
            site.id.clone(),
            source.clone(),
            child.stderr.take().map(Pipe::Err),
        ),
    ] {
        let Some(stream) = stream else { continue };
        tauri::async_runtime::spawn(async move {
            match stream {
                Pipe::Out(s) => pump(reader_app, id, src, generation, BufReader::new(s)).await,
                Pipe::Err(s) => pump(reader_app, id, src, generation, BufReader::new(s)).await,
            }
        });
    }

    // Tunggu proses selesai → Stopped (diminta) atau Exited (berhenti sendiri).
    let wait_app = app.clone();
    let wait_id = site.id.clone();
    let wait_src = source.clone();
    tauri::async_runtime::spawn(async move {
        let code = child.wait().await.ok().and_then(|s| s.code());
        let procs = wait_app.state::<SiteProcs>();
        record_remove(&procs, &wait_id);
        let mut requested = false;
        {
            let mut map = procs.entries.lock().unwrap();
            if let Some(e) = map.get_mut(&wait_id) {
                if e.generation != generation {
                    return;
                }
                requested = e.stopping;
                e.pid = None;
                e.exit_code = code;
                e.state = if requested {
                    SiteState::Stopped
                } else {
                    SiteState::Exited
                };
                if requested {
                    e.last_line = None;
                }
            }
        }
        if requested {
            emit_log(&wait_app, &wait_src, "dihentikan", None);
        } else {
            let msg = match code {
                Some(c) => format!("proses berhenti (exit {})", c),
                None => "proses berhenti".to_string(),
            };
            emit_log(&wait_app, &wait_src, &msg, Some("error"));
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
            tokio::time::sleep(READY_INTERVAL).await;
            let procs = ready_app.state::<SiteProcs>();
            let still_starting = procs
                .entries
                .lock()
                .unwrap()
                .get(&ready_id)
                .is_some_and(|e| e.generation == generation && e.state == SiteState::Starting);
            if !still_starting {
                return;
            }
            let open = port_open(port).await;
            let timed_out = tokio::time::Instant::now() >= deadline;
            if !open && !timed_out {
                continue;
            }
            {
                let mut map = procs.entries.lock().unwrap();
                if let Some(e) = map.get_mut(&ready_id) {
                    if e.generation == generation && e.state == SiteState::Starting {
                        e.state = if open {
                            SiteState::Running
                        } else {
                            SiteState::Unresponsive
                        };
                    }
                }
            }
            emit_status(&ready_app);
            return;
        }
    });

    Ok(())
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
) {
    let mut lines = reader.lines();
    while let Ok(Some(raw)) = lines.next_line().await {
        let line = strip_ansi(&raw);
        let line = line.trim_end();
        if line.trim().is_empty() {
            continue;
        }
        emit_log(&app, &source, line, None);
        let procs = app.state::<SiteProcs>();
        let mut map = procs.entries.lock().unwrap();
        if let Some(e) = map.get_mut(&id) {
            if e.generation == generation {
                e.last_line = Some(line.to_string());
            }
        }
    }
}

pub async fn stop<R: Runtime>(app: &AppHandle<R>, id: &str) -> Result<(), String> {
    let procs = app.state::<SiteProcs>();
    let pid = {
        let mut map = procs.entries.lock().unwrap();
        match map.get_mut(id) {
            Some(e) if e.pid.is_some() => {
                e.stopping = true;
                e.pid
            }
            _ => None,
        }
    };
    if let Some(pid) = pid {
        kill_tree(pid).await;
    }
    Ok(())
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
                "{} belum berhenti setelah 15 dtk; port {} masih dipakai",
                site.name, site.port
            ));
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    start(app, site).await
}

/// Hentikan semua dev server (dipanggil saat Servel keluar).
pub async fn stop_all<R: Runtime>(app: &AppHandle<R>) {
    let procs = app.state::<SiteProcs>();
    let pids: Vec<u32> = {
        let mut map = procs.entries.lock().unwrap();
        map.values_mut()
            .filter_map(|e| {
                e.stopping = true;
                e.pid
            })
            .collect()
    };
    for pid in pids {
        kill_tree(pid).await;
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

#[cfg(test)]
mod tests {
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
        }
    }

    #[test]
    fn strip_ansi_removes_colors() {
        assert_eq!(
            strip_ansi("\u{1b}[32m  VITE v5.4.0\u{1b}[39m  ready"),
            "  VITE v5.4.0  ready"
        );
        assert_eq!(strip_ansi("\u{1b}[2K\u{1b}[1Gdone"), "done");
        assert_eq!(strip_ansi("plain"), "plain");
        assert_eq!(strip_ansi("\u{1b}]0;mise\u{7}installing"), "installing");
    }

    #[test]
    fn validate_rejects_incomplete_sites() {
        let dir = std::env::temp_dir();
        let dir = dir.to_string_lossy();
        assert!(validate(&site("", "npm run dev", 3000))
            .unwrap_err()
            .contains("folder"));
        assert!(validate(&site("/definitely/not/here", "npm run dev", 3000)).is_err());
        assert!(validate(&site(&dir, "  ", 3000))
            .unwrap_err()
            .contains("perintah"));
        assert!(validate(&site(&dir, "npm run dev", 80))
            .unwrap_err()
            .contains("port"));
        assert!(validate(&site(&dir, "npm run dev", 3000)).is_ok());

        let mut p = site(&dir, "npm run dev", 3000);
        p.env.insert(
            "SSL_CERT_FILE".into(),
            r"C:\Program Files\Git\usr\ssl\certs\ca-bundle.crt".into(),
        );
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
        let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(port_open(port).await);
        assert!(!port_free(port).await);
        drop(listener);
        assert!(!port_open(port).await);
        assert!(port_free(port).await);
    }

    /// Uji ujung ke ujung di Unix: jalankan server sungguhan lewat shell,
    /// tunggu port menjawab, lalu Stop harus mematikan seluruh pohon proses
    /// (induk `sh` + anak) dan membebaskan port.
    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn start_then_stop_frees_the_port() {
        if std::process::Command::new("python3")
            .arg("--version")
            .output()
            .is_err()
        {
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
        // npm → node: Stop harus mematikan keduanya.
        let mut p = site(
            &dir.to_string_lossy(),
            "python3 -m http.server {port} --bind 127.0.0.1 & wait",
            port,
        );
        p.id = "e2e".into();

        start(&handle, &p).await.expect("start gagal");
        assert!(
            start(&handle, &p).await.is_err(),
            "start kedua harus ditolak"
        );

        let mut state = SiteState::Starting;
        for _ in 0..60 {
            tokio::time::sleep(Duration::from_millis(250)).await;
            state = snapshot(&handle)
                .iter()
                .find(|r| r.id == "e2e")
                .map(|r| r.state)
                .unwrap();
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
            let st = snapshot(&handle)
                .iter()
                .find(|r| r.id == "e2e")
                .map(|r| r.state)
                .unwrap();
            if st == SiteState::Stopped && port_free(port).await {
                stopped = true;
                break;
            }
        }
        assert!(stopped, "setelah Stop, status harus stopped dan port bebas");

        // Restart dari keadaan berhenti = start; dari keadaan jalan = stop + start.
        restart(&handle, &p)
            .await
            .expect("restart (dari berhenti) gagal");
        restart(&handle, &p)
            .await
            .expect("restart (dari jalan) gagal");
        let mut running = false;
        for _ in 0..60 {
            tokio::time::sleep(Duration::from_millis(250)).await;
            let st = snapshot(&handle)
                .iter()
                .find(|r| r.id == "e2e")
                .map(|r| r.state)
                .unwrap();
            if st == SiteState::Running {
                running = true;
                break;
            }
        }
        assert!(
            running && port_open(port).await,
            "setelah Restart, server harus jalan lagi"
        );
        stop(&handle, "e2e").await.unwrap();
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
        let mut p = site(
            &dir.to_string_lossy(),
            "echo \"V=$SERVEL_TEST_VAR\"; exit 3",
            port,
        );
        p.id = "crash".into();
        p.env
            .insert("SERVEL_TEST_VAR".into(), "dari-project".into());
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
        let mut orphan = std::process::Command::new("sleep")
            .arg("60")
            .process_group(0)
            .spawn()
            .unwrap();
        let mut other = std::process::Command::new("sleep")
            .arg("60")
            .process_group(0)
            .spawn()
            .unwrap();
        let started = process_started(orphan.id()).await.unwrap();
        let records: HashMap<String, Record> = [
            (
                "a".to_string(),
                Record {
                    pid: orphan.id(),
                    started,
                },
            ),
            (
                "b".to_string(),
                Record {
                    pid: other.id(),
                    started: "Thu Jan  1 00:00:00 1970".into(),
                },
            ),
        ]
        .into();
        std::fs::write(&path, serde_json::to_string(&records).unwrap()).unwrap();

        assert_eq!(cleanup_leftovers(&handle).await, 1);
        let status = tokio::task::spawn_blocking(move || orphan.wait())
            .await
            .unwrap()
            .unwrap();
        assert!(!status.success(), "proses sisa harus dimatikan");
        assert!(
            other.try_wait().unwrap().is_none(),
            "PID dengan waktu mulai berbeda tidak boleh disentuh"
        );
        assert!(!path.exists(), "catatan dihapus setelah dibersihkan");
        let _ = other.kill();
        let _ = other.wait();
    }
}
