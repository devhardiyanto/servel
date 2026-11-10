//! Backend WSLC (`wslc.exe`, WSL containers — Windows, WSL >= 3.0.1).
//!
//! WSLC belum punya `compose` maupun endpoint Docker API, jadi orkestrasi
//! dilakukan imperatif per service: `volume create` → `run -d` / `start` /
//! `stop`. Desainnya mengikuti perilaku yang sudah terbukti di skrip manual
//! `servel.ps1` (v1.5 era) dan catatan proyek komunitas wslc-compose:
//!
//! - **Serialisasi.** WSLC bisa mengembalikan `ERROR_SHARING_VIOLATION` saat
//!   dipanggil paralel, jadi semua pemanggilan lewat satu mutex [`LOCK`] dan
//!   error transien di-retry.
//! - **Flag minimal.** Hanya `run -d --name -p -v -e <image> <cmd...>` — flag yang
//!   sudah terbukti. Tidak pakai label: sidik definisi (spec hash) disimpan di
//!   file state lokal (`wslc-state.json`), bukan di container.
//! - **Adopsi.** Container `servel_<id>` yang ada tapi tidak tercatat di state
//!   (mis. dibuat skrip manual) dipakai apa adanya — tidak di-recreate, supaya
//!   env/volume buatan user tidak tertimpa.
//! - **Recreate saat definisi berubah.** Container tercatat dengan hash berbeda
//!   dihapus lalu dibuat ulang. Data aman di named volume.
//! - **Stop tidak menghapus container** (beda dengan `compose down`), supaya
//!   container adopsi tidak hilang.
//! - Port di-bind ke `127.0.0.1` (dev lokal tidak perlu terbuka ke LAN).
//!
//! Catatan sesi: sesi WSLC terpisah per user **dan per elevation**. Servel yang
//! dijalankan "as Administrator" tidak melihat container dari sesi biasa.

use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::process::{Output, Stdio};
use std::sync::atomic::{AtomicU32, AtomicU8, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::Mutex;

use crate::commands::services::{id_from_container_name, ServiceDef, ServiceStatus};
use crate::commands::stats::{parse_mem_usage_to_mb, ContainerMemStat};
use crate::commands::util::{extract_semver, silent_command};

const BIN: &str = "wslc";

/// Penanda error transien WSLC yang aman di-retry.
const TRANSIENT_MARKERS: &[&str] = &["ERROR_SHARING_VIOLATION", "ERROR_ALREADY_EXISTS"];
const MAX_ATTEMPTS: u32 = 4;
const RETRY_DELAY: Duration = Duration::from_millis(1500);

const QUERY_TIMEOUT: Duration = Duration::from_secs(30);
const STATS_TIMEOUT: Duration = Duration::from_secs(10);
const STOP_TIMEOUT: Duration = Duration::from_secs(90);
/// `run` bisa menarik image besar (SQL Server ~1.5 GB).
const RUN_TIMEOUT: Duration = Duration::from_secs(30 * 60);

static LOCK: Mutex<()> = Mutex::const_new(());

fn command(args: &[String]) -> tokio::process::Command {
    let mut cmd = silent_command(BIN);
    cmd.args(args)
        // Paksa output WSL tooling ke UTF-8 (default wsl.exe = UTF-16LE).
        .env("WSL_UTF8", "1")
        .stdin(Stdio::null())
        .kill_on_drop(true);
    cmd
}

fn owned(args: &[&str]) -> Vec<String> {
    args.iter().map(|s| s.to_string()).collect()
}

pub fn is_transient(stderr: &str) -> bool {
    TRANSIENT_MARKERS.iter().any(|m| stderr.contains(m))
}

/// Jalankan wslc dan tangkap output. Serial + retry error transien.
async fn capture(args: &[String], timeout: Duration) -> Result<Output, String> {
    let mut attempt = 0;
    loop {
        attempt += 1;
        let result = {
            let _guard = LOCK.lock().await;
            let fut = command(args).stdout(Stdio::piped()).stderr(Stdio::piped()).output();
            tokio::time::timeout(timeout, fut).await
        };

        let output = match result {
            Err(_) => return Err(format!("wslc {} timeout ({} dtk)", args.join(" "), timeout.as_secs())),
            Ok(Err(e)) => return Err(format!("Gagal menjalankan wslc: {}", e)),
            Ok(Ok(o)) => o,
        };

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if is_transient(&stderr) && attempt < MAX_ATTEMPTS {
                tokio::time::sleep(RETRY_DELAY).await;
                continue;
            }
        }
        return Ok(output);
    }
}

/// Seperti [`capture`] tapi exit != 0 → `Err(stderr)`.
async fn capture_ok(args: &[String], timeout: Duration) -> Result<String, String> {
    let out = capture(args, timeout).await?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
        Err(if stderr.is_empty() { stdout } else { stderr })
    }
}

fn emit(app: &AppHandle, source: &str, stream: &str, line: &str) {
    let _ = app.emit(
        "cmd-output",
        serde_json::json!({ "line": line, "stream": stream, "source": source }),
    );
}

/// Jalankan wslc sambil men-stream stdout/stderr ke panel log. Serial + retry.
async fn run_streamed(
    app: &AppHandle,
    args: &[String],
    source: &str,
    timeout: Duration,
) -> Result<(), String> {
    let mut attempt = 0;
    loop {
        attempt += 1;
        let stderr_acc = {
            let _guard = LOCK.lock().await;
            let mut child = command(args)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|e| format!("Gagal menjalankan wslc: {}", e))?;

            let stdout = child.stdout.take().ok_or("stdout wslc tidak tersedia")?;
            let stderr = child.stderr.take().ok_or("stderr wslc tidak tersedia")?;

            let app_out = app.clone();
            let src_out = source.to_string();
            let out_task = tokio::spawn(async move {
                let mut lines = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if !line.trim().is_empty() {
                        emit(&app_out, &src_out, "stdout", &line);
                    }
                }
            });

            let app_err = app.clone();
            let src_err = source.to_string();
            let err_task = tokio::spawn(async move {
                let mut acc = Vec::new();
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if !line.trim().is_empty() {
                        emit(&app_err, &src_err, "stderr", &line);
                        acc.push(line);
                    }
                }
                acc.join("\n")
            });

            let waited = tokio::time::timeout(timeout, child.wait()).await;
            let status = match waited {
                Err(_) => {
                    let _ = child.kill().await;
                    return Err(format!(
                        "wslc {} timeout ({} menit)",
                        args.first().map(String::as_str).unwrap_or(""),
                        timeout.as_secs() / 60
                    ));
                }
                Ok(Err(e)) => return Err(format!("Gagal menunggu wslc: {}", e)),
                Ok(Ok(s)) => s,
            };
            let _ = out_task.await;
            let acc = err_task.await.unwrap_or_default();

            if status.success() {
                return Ok(());
            }
            acc
        };

        if is_transient(&stderr_acc) && attempt < MAX_ATTEMPTS {
            tokio::time::sleep(RETRY_DELAY).await;
            continue;
        }
        return Err(friendly_error(&stderr_acc)
            .unwrap_or_else(|| last_line(&stderr_acc).unwrap_or_else(|| "wslc gagal".to_string())));
    }
}

fn last_line(s: &str) -> Option<String> {
    s.lines().rev().find(|l| !l.trim().is_empty()).map(|l| l.trim().to_string())
}

/// Terjemahkan stderr WSLC ke pesan yang bisa ditindaklanjuti.
pub fn friendly_error(stderr: &str) -> Option<String> {
    let lower = stderr.to_lowercase();
    if lower.contains("too many volumes") {
        return Some(
            "Batas mount volume sesi WSLC tercapai — jalankan `wsl --shutdown` lalu coba lagi".to_string(),
        );
    }
    if lower.contains("error_sharing_violation") {
        return Some(
            "Sesi WSLC sedang terkunci — tunggu sebentar; bila berulang jalankan `wsl --shutdown`".to_string(),
        );
    }
    let port_hint = lower.contains("address already in use")
        || lower.contains("port is already allocated")
        || lower.contains("error_address_already_associated")
        || (lower.contains("port") && lower.contains("in use"));
    if port_hint {
        return Some("Port sudah dipakai aplikasi lain — tutup aplikasi tersebut lalu Start ulang".to_string());
    }
    None
}

// ---------------------------------------------------------------------------
// Deteksi
// ---------------------------------------------------------------------------

pub async fn is_installed() -> bool {
    if !cfg!(target_os = "windows") {
        return false;
    }
    version().await.is_some()
}

/// Sesi WSLC bisa dipakai (perintah list berhasil). Memicu start sesi bila perlu.
pub async fn is_running() -> bool {
    capture_ok(&owned(&["container", "list", "--all", "--format", "json"]), QUERY_TIMEOUT)
        .await
        .is_ok()
}

pub async fn version() -> Option<String> {
    for args in [["--version"], ["version"]] {
        if let Ok(out) = capture_ok(&owned(&args), Duration::from_secs(15)).await {
            if let Some(v) = extract_semver(&out) {
                return Some(v);
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Parsing JSON (toleran: array, objek tunggal, atau NDJSON; key case-insensitive)
// ---------------------------------------------------------------------------

/// Ambil semua objek JSON dari output: `[...]`, `{...}`, atau satu objek per baris.
pub fn json_objects(stdout: &str) -> Vec<serde_json::Map<String, Value>> {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
        return match v {
            Value::Array(items) => items
                .into_iter()
                .filter_map(|i| match i {
                    Value::Object(m) => Some(m),
                    _ => None,
                })
                .collect(),
            Value::Object(m) => vec![m],
            _ => Vec::new(),
        };
    }
    trimmed
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l.trim()).ok())
        .filter_map(|v| match v {
            Value::Object(m) => Some(m),
            _ => None,
        })
        .collect()
}

/// Cari key secara case-insensitive.
fn get<'a>(m: &'a serde_json::Map<String, Value>, key: &str) -> Option<&'a Value> {
    m.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v)
}

fn first_get<'a>(m: &'a serde_json::Map<String, Value>, keys: &[&str]) -> Option<&'a Value> {
    keys.iter().find_map(|k| get(m, k))
}

/// Nama container dari `Name` / `Names` (string atau array), tanpa `/` di depan.
fn container_name(m: &serde_json::Map<String, Value>) -> Option<String> {
    let raw = match first_get(m, &["Name", "Names"])? {
        Value::String(s) => s.split(',').next().unwrap_or("").to_string(),
        Value::Array(a) => a.iter().find_map(|v| v.as_str()).unwrap_or("").to_string(),
        _ => return None,
    };
    let name = raw.trim().trim_start_matches('/').to_string();
    (!name.is_empty()).then_some(name)
}

/// Normalisasi state WSLC/Docker → (`state` lowercase, `exit_code`).
///
/// WSLC `list --format json` memakai enum numerik: 1 created, 2 running,
/// 3 exited, 4 deleted. `inspect` memakai objek bergaya Docker
/// (`{"Status":"running","Running":true,"ExitCode":0}`). Fallback ke `Status`
/// teks ("Up 3 minutes" / "Exited (1) ...").
fn container_state(m: &serde_json::Map<String, Value>) -> (String, Option<i32>) {
    let mut exit_code = first_get(m, &["ExitCode"]).and_then(Value::as_i64).map(|v| v as i32);

    let state = match get(m, "State") {
        Some(Value::Number(n)) => match n.as_i64() {
            Some(1) => "created",
            Some(2) => "running",
            Some(3) => "exited",
            Some(4) => "removed",
            _ => "unknown",
        }
        .to_string(),
        Some(Value::String(s)) if !s.trim().is_empty() => s.trim().to_lowercase(),
        Some(Value::Object(st)) => {
            if exit_code.is_none() {
                exit_code = get(st, "ExitCode").and_then(Value::as_i64).map(|v| v as i32);
            }
            match get(st, "Status").and_then(Value::as_str) {
                Some(s) if !s.is_empty() => s.to_lowercase(),
                _ => match get(st, "Running").and_then(Value::as_bool) {
                    Some(true) => "running".to_string(),
                    Some(false) => "exited".to_string(),
                    None => "unknown".to_string(),
                },
            }
        }
        _ => String::new(),
    };

    if !state.is_empty() {
        return (state, exit_code);
    }

    let status = get(m, "Status").and_then(Value::as_str).unwrap_or("").trim().to_lowercase();
    let state = if status.starts_with("up") || status == "running" {
        "running".to_string()
    } else if status.starts_with("exited") {
        if exit_code.is_none() {
            exit_code = status
                .split(['(', ')'])
                .nth(1)
                .and_then(|s| s.trim().parse().ok());
        }
        "exited".to_string()
    } else if status.starts_with("created") {
        "created".to_string()
    } else {
        "unknown".to_string()
    };
    (state, exit_code)
}

/// Parse `wslc container list --all --format json` → status container `servel_*`.
pub fn parse_list_json(stdout: &str) -> Vec<ServiceStatus> {
    json_objects(stdout)
        .iter()
        .filter_map(|m| {
            let name = container_name(m)?;
            let id = id_from_container_name(&name)?;
            let (state, exit_code) = container_state(m);
            Some(ServiceStatus {
                id,
                container_name: name,
                running: state == "running",
                state,
                exit_code,
            })
        })
        .collect()
}

/// Nilai memori → MB. Terima string bergaya Docker (`"123.4MiB / 1.9GiB"`),
/// angka (byte), atau objek (`{"Usage": <byte>}`).
fn mem_value_to_mb(v: &Value) -> Option<u64> {
    match v {
        Value::String(s) => parse_mem_usage_to_mb(s).or_else(|| {
            s.trim().parse::<f64>().ok().map(|b| (b / 1_000_000.0).round() as u64)
        }),
        Value::Number(n) => n.as_f64().map(|b| (b / 1_000_000.0).round().max(0.0) as u64),
        Value::Object(o) => first_get(o, &["Usage", "Current", "UsageBytes", "Used"]).and_then(mem_value_to_mb),
        _ => None,
    }
}

/// Parse `wslc stats --format json` (format belum terdokumentasi resmi —
/// sengaja toleran terhadap beberapa bentuk yang masuk akal).
pub fn parse_stats_json(stdout: &str) -> Vec<ContainerMemStat> {
    json_objects(stdout)
        .iter()
        .filter_map(|m| {
            let name = container_name(m).or_else(|| {
                first_get(m, &["Container"]).and_then(Value::as_str).map(|s| s.trim_start_matches('/').to_string())
            })?;
            let id = id_from_container_name(&name)?;
            let mem = first_get(
                m,
                &["MemUsage", "MemoryUsage", "Memory", "MemoryUsageBytes", "MemUsageBytes", "Mem"],
            )
            .and_then(mem_value_to_mb)?;
            Some(ContainerMemStat { id, mem_mb: mem })
        })
        .collect()
}

/// Nama volume dari `wslc volume list --format json`.
pub fn parse_names_json(stdout: &str) -> HashSet<String> {
    json_objects(stdout).iter().filter_map(container_name).collect()
}

// ---------------------------------------------------------------------------
// Penyusun argumen (murni — diuji unit)
// ---------------------------------------------------------------------------

/// Pecah string command bergaya shell: spasi memisahkan, kutip tunggal/ganda
/// mengelompokkan, `\` meng-escape karakter berikut (di luar kutip tunggal).
pub fn split_command(input: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut cur = String::new();
    let mut in_token = false;
    let mut quote: Option<char> = None;
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        match quote {
            Some(q) if c == q => quote = None,
            Some('\'') => cur.push(c),
            Some(_) if c == '\\' => {
                if let Some(&n) = chars.peek() {
                    if n == '"' || n == '\\' {
                        cur.push(n);
                        chars.next();
                        continue;
                    }
                }
                cur.push(c);
            }
            Some(_) => cur.push(c),
            None if c == '"' || c == '\'' => {
                quote = Some(c);
                in_token = true;
            }
            None if c == '\\' => {
                if let Some(n) = chars.next() {
                    cur.push(n);
                }
                in_token = true;
            }
            None if c.is_whitespace() => {
                if in_token {
                    args.push(std::mem::take(&mut cur));
                    in_token = false;
                }
            }
            None => {
                cur.push(c);
                in_token = true;
            }
        }
    }
    if in_token {
        args.push(cur);
    }
    args
}

/// Named volume = bagian sebelum `:` tanpa `/`, `\` atau `.` (bukan path host).
pub fn named_volume(spec: &str) -> Option<String> {
    // Path drive Windows (`C:\data:/data`) bukan named volume.
    let b = spec.as_bytes();
    if b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/') {
        return None;
    }
    let name = spec.split(':').next()?.trim();
    let is_named = !name.is_empty() && !name.contains(['/', '\\', '.']);
    is_named.then(|| name.to_string())
}

/// Argumen `wslc run` untuk satu service.
pub fn build_run_args(def: &ServiceDef) -> Vec<String> {
    let mut a = owned(&["run", "-d", "--name", &def.container_name]);

    for p in &def.ports {
        a.push("-p".into());
        a.push(format!("127.0.0.1:{}:{}", p.host, p.container));
    }
    for v in def.volumes.iter().flatten() {
        a.push("-v".into());
        a.push(v.clone());
    }
    let env: BTreeMap<_, _> = def.environment.iter().collect();
    for (k, v) in env {
        a.push("-e".into());
        a.push(format!("{}={}", k, v));
    }
    a.push(def.image.clone());
    if let Some(cmd) = &def.command {
        a.extend(split_command(cmd));
    }
    a
}

/// Sidik definisi service. Berubah → container di-recreate saat Start.
pub fn spec_hash(def: &ServiceDef) -> String {
    let mut h = Sha256::new();
    for arg in build_run_args(def) {
        h.update(arg.as_bytes());
        h.update([0u8]);
    }
    h.finalize().iter().take(8).map(|b| format!("{:02x}", b)).collect()
}

/// Apa yang harus dilakukan Start terhadap satu service.
#[derive(Debug, PartialEq, Eq)]
pub enum StartAction {
    /// Container belum ada → `run`.
    Create,
    /// Ada, sesuai definisi (atau adopsi), sedang running → tidak ada.
    AlreadyRunning,
    /// Ada, sesuai definisi (atau adopsi), berhenti → `start`.
    Start,
    /// Ada tapi definisi berubah → `remove -f` lalu `run`.
    Recreate,
}

pub fn plan_start(
    existing: Option<&ServiceStatus>,
    recorded_hash: Option<&str>,
    desired_hash: &str,
) -> StartAction {
    match existing {
        None => StartAction::Create,
        Some(s) => {
            // Tidak tercatat = container adopsi → pakai apa adanya.
            let up_to_date = recorded_hash.is_none_or(|h| h == desired_hash);
            match (up_to_date, s.running) {
                (false, _) => StartAction::Recreate,
                (true, true) => StartAction::AlreadyRunning,
                (true, false) => StartAction::Start,
            }
        }
    }
}

// ---------------------------------------------------------------------------
// State lokal: container yang dibuat Servel + hash definisinya
// ---------------------------------------------------------------------------

fn state_path(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join("wslc-state.json"))
}

fn load_state(app: &AppHandle) -> HashMap<String, String> {
    state_path(app)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_state(app: &AppHandle, state: &HashMap<String, String>) {
    let Some(path) = state_path(app) else { return };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(state) {
        let tmp = path.with_extension("tmp");
        if std::fs::write(&tmp, json).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    }
}

// ---------------------------------------------------------------------------
// Operasi
// ---------------------------------------------------------------------------

pub async fn status() -> Result<Vec<ServiceStatus>, String> {
    let out = capture_ok(&owned(&["container", "list", "--all", "--format", "json"]), QUERY_TIMEOUT)
        .await
        .map_err(|e| format!("wslc container list gagal: {}", e))?;
    Ok(parse_list_json(&out))
}

/// Varian argumen `stats` yang berhasil: 0 = belum dicoba, 1 = `--no-stream`,
/// 2 = tanpa flag. WSLC stats berupa snapshot; `--no-stream` mungkin tak dikenal.
static STATS_VARIANT: AtomicU8 = AtomicU8::new(0);
static STATS_FAILURES: AtomicU32 = AtomicU32::new(0);
/// Setelah gagal beruntun sebanyak ini, stats dimatikan untuk sesi app ini
/// supaya tidak terus memegang lock WSLC.
const STATS_MAX_FAILURES: u32 = 3;

pub async fn stats() -> Result<Vec<ContainerMemStat>, String> {
    if STATS_FAILURES.load(Ordering::Relaxed) >= STATS_MAX_FAILURES {
        return Err("wslc stats dinonaktifkan untuk sesi ini".to_string());
    }

    let variants: &[u8] = match STATS_VARIANT.load(Ordering::Relaxed) {
        1 => &[1],
        2 => &[2],
        _ => &[1, 2],
    };

    let mut last_err = String::new();
    for &v in variants {
        let args = if v == 1 {
            owned(&["stats", "--no-stream", "--format", "json"])
        } else {
            owned(&["stats", "--format", "json"])
        };
        match capture_ok(&args, STATS_TIMEOUT).await {
            Ok(out) => {
                STATS_VARIANT.store(v, Ordering::Relaxed);
                STATS_FAILURES.store(0, Ordering::Relaxed);
                return Ok(parse_stats_json(&out));
            }
            Err(e) => last_err = e,
        }
    }

    STATS_FAILURES.fetch_add(1, Ordering::Relaxed);
    Err(format!("wslc stats gagal: {}", last_err))
}

async fn existing_volumes() -> Option<HashSet<String>> {
    capture_ok(&owned(&["volume", "list", "--format", "json"]), QUERY_TIMEOUT)
        .await
        .ok()
        .map(|out| parse_names_json(&out))
}

async fn ensure_volumes(app: &AppHandle, def: &ServiceDef, known: &mut Option<HashSet<String>>) {
    let source = def.id.to_uppercase();
    for spec in def.volumes.iter().flatten() {
        let Some(name) = named_volume(spec) else { continue };
        if known.as_ref().is_some_and(|k| k.contains(&name)) {
            continue;
        }
        // Gagal create (mis. sudah ada tapi list gagal dibaca) tidak fatal —
        // `run` yang akan melaporkan bila volume benar-benar bermasalah.
        if capture_ok(&owned(&["volume", "create", &name]), QUERY_TIMEOUT).await.is_ok() {
            emit(app, &source, "stdout", &format!("volume {} dibuat", name));
        }
        if let Some(k) = known.as_mut() {
            k.insert(name);
        }
    }
}

async fn start_one(
    app: &AppHandle,
    def: &ServiceDef,
    existing: Option<&ServiceStatus>,
    state: &mut HashMap<String, String>,
    volumes: &mut Option<HashSet<String>>,
) -> Result<(), String> {
    let source = def.id.to_uppercase();
    let name = &def.container_name;
    let desired = spec_hash(def);
    let recorded = state.get(name).map(String::as_str);
    let action = plan_start(existing, recorded, &desired);

    if existing.is_some() && recorded.is_none() {
        emit(
            app,
            &source,
            "stdout",
            &format!("{} sudah ada (dibuat di luar Servel) — dipakai apa adanya", name),
        );
    }

    match action {
        StartAction::AlreadyRunning => Ok(()),
        StartAction::Start => {
            emit(app, &source, "stdout", &format!("wslc start {}", name));
            run_streamed(app, &owned(&["start", name]), &source, STOP_TIMEOUT).await
        }
        StartAction::Recreate | StartAction::Create => {
            if action == StartAction::Recreate {
                emit(app, &source, "stdout", &format!("definisi {} berubah — membuat ulang container (data volume tetap)", name));
                run_streamed(app, &owned(&["remove", "-f", name]), &source, STOP_TIMEOUT).await?;
                state.remove(name);
            }
            ensure_volumes(app, def, volumes).await;
            emit(app, &source, "stdout", &format!("wslc run {} ({})", name, def.image));
            run_streamed(app, &build_run_args(def), &source, RUN_TIMEOUT).await?;
            state.insert(name.clone(), desired);
            Ok(())
        }
    }
}

pub async fn start(app: &AppHandle, defs: &[ServiceDef], services: &[String]) -> Result<(), String> {
    let current = status().await?;
    let by_name: HashMap<&str, &ServiceStatus> =
        current.iter().map(|s| (s.container_name.as_str(), s)).collect();

    let mut state = load_state(app);
    let mut volumes = existing_volumes().await;
    let mut failed: Vec<String> = Vec::new();

    for id in services {
        let Some(def) = defs.iter().find(|d| &d.id == id) else {
            emit(app, "SERVEL", "stderr", &format!("service '{}' tidak dikenal — dilewati", id));
            continue;
        };
        let existing = by_name.get(def.container_name.as_str()).copied();
        let result = start_one(app, def, existing, &mut state, &mut volumes).await;
        // Simpan setiap langkah: kegagalan service berikutnya tidak menghapus catatan.
        save_state(app, &state);
        if let Err(e) = result {
            emit(app, &def.id.to_uppercase(), "stderr", &e);
            failed.push(format!("{}: {}", def.name, e));
        }
    }

    if failed.is_empty() {
        Ok(())
    } else {
        Err(format!("Gagal start {}", failed.join("; ")))
    }
}

async fn stop_names(app: &AppHandle, targets: Vec<(String, String)>) -> Result<(), String> {
    let mut failed = Vec::new();
    for (source, name) in targets {
        emit(app, &source, "stdout", &format!("wslc stop {}", name));
        if let Err(e) = run_streamed(app, &owned(&["stop", "-t", "30", &name]), &source, STOP_TIMEOUT).await {
            failed.push(format!("{}: {}", name, e));
        }
    }
    if failed.is_empty() {
        Ok(())
    } else {
        Err(format!("Gagal stop {}", failed.join("; ")))
    }
}

pub async fn stop(app: &AppHandle, defs: &[ServiceDef], services: &[String]) -> Result<(), String> {
    let running: HashSet<String> = status()
        .await?
        .into_iter()
        .filter(|s| s.running)
        .map(|s| s.container_name)
        .collect();

    let targets = services
        .iter()
        .filter_map(|id| {
            let name = defs
                .iter()
                .find(|d| &d.id == id)
                .map(|d| d.container_name.clone())
                .unwrap_or_else(|| format!("servel_{}", id));
            running.contains(&name).then(|| (id.to_uppercase(), name))
        })
        .collect();
    stop_names(app, targets).await
}

/// Stop semua container `servel_*` yang running (urutan terbalik dari services.json).
/// Container tidak dihapus — lihat catatan modul.
pub async fn stop_all(app: &AppHandle, defs: &[ServiceDef]) -> Result<(), String> {
    let current = status().await?;
    let order: HashMap<&str, usize> =
        defs.iter().enumerate().map(|(i, d)| (d.container_name.as_str(), i)).collect();

    let mut running: Vec<&ServiceStatus> = current.iter().filter(|s| s.running).collect();
    running.sort_by_key(|s| std::cmp::Reverse(order.get(s.container_name.as_str()).copied().unwrap_or(0)));

    let targets = running
        .into_iter()
        .map(|s| (s.id.to_uppercase(), s.container_name.clone()))
        .collect();
    stop_names(app, targets).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::services::PortMap;

    fn def(id: &str) -> ServiceDef {
        ServiceDef {
            id: id.to_string(),
            name: id.to_string(),
            category: "core".to_string(),
            image: "mysql:8.0".to_string(),
            container_name: format!("servel_{}", id),
            ports: vec![PortMap { host: "3306".into(), container: "3306".into() }],
            environment: HashMap::from([
                ("MYSQL_ROOT_HOST".to_string(), "%".to_string()),
                ("MYSQL_ALLOW_EMPTY_PASSWORD".to_string(), "yes".to_string()),
            ]),
            volumes: Some(vec!["servel_mysql_data:/var/lib/mysql".to_string()]),
            command: None,
            ram_estimate_mb: 80,
        }
    }

    fn status(id: &str, running: bool) -> ServiceStatus {
        ServiceStatus {
            id: id.into(),
            container_name: format!("servel_{}", id),
            running,
            state: if running { "running" } else { "exited" }.into(),
            exit_code: None,
        }
    }

    #[test]
    fn run_args_are_deterministic_and_loopback_bound() {
        let args = build_run_args(&def("mysql"));
        assert_eq!(
            args,
            vec![
                "run", "-d", "--name", "servel_mysql",
                "-p", "127.0.0.1:3306:3306",
                "-v", "servel_mysql_data:/var/lib/mysql",
                // env diurutkan → hash stabil walau HashMap acak
                "-e", "MYSQL_ALLOW_EMPTY_PASSWORD=yes",
                "-e", "MYSQL_ROOT_HOST=%",
                "mysql:8.0",
            ]
        );
    }

    #[test]
    fn run_args_split_command_string() {
        let mut d = def("minio");
        d.image = "minio/minio".into();
        d.command = Some("server /data --console-address \":9001\"".into());
        let args = build_run_args(&d);
        let tail: Vec<&str> = args.iter().rev().take(5).rev().map(String::as_str).collect();
        assert_eq!(tail, vec!["minio/minio", "server", "/data", "--console-address", ":9001"]);
    }

    #[test]
    fn split_command_handles_quotes_and_escapes() {
        assert_eq!(split_command("a  b\tc"), vec!["a", "b", "c"]);
        assert_eq!(split_command("x 'a b' \"c d\""), vec!["x", "a b", "c d"]);
        assert_eq!(split_command(r#"say "he said \"hi\"""#), vec!["say", r#"he said "hi""#]);
        assert_eq!(split_command("--opt=''"), vec!["--opt="]);
        assert_eq!(split_command(r"a\ b"), vec!["a b"]);
        assert!(split_command("   ").is_empty());
    }

    #[test]
    fn spec_hash_changes_with_definition() {
        let a = def("mysql");
        let mut b = def("mysql");
        assert_eq!(spec_hash(&a), spec_hash(&b));
        b.image = "mysql:8.4".into();
        assert_ne!(spec_hash(&a), spec_hash(&b));
        assert_eq!(spec_hash(&a).len(), 16);
    }

    #[test]
    fn named_volume_detection() {
        assert_eq!(named_volume("servel_mysql_data:/var/lib/mysql").as_deref(), Some("servel_mysql_data"));
        assert_eq!(named_volume("./data:/data"), None);
        assert_eq!(named_volume("/abs:/data"), None);
        assert_eq!(named_volume(r"C:\data:/data"), None);
    }

    #[test]
    fn plan_start_cases() {
        let up = status("mysql", true);
        let down = status("mysql", false);
        assert_eq!(plan_start(None, None, "h"), StartAction::Create);
        assert_eq!(plan_start(None, Some("old"), "h"), StartAction::Create);
        assert_eq!(plan_start(Some(&up), Some("h"), "h"), StartAction::AlreadyRunning);
        assert_eq!(plan_start(Some(&down), Some("h"), "h"), StartAction::Start);
        assert_eq!(plan_start(Some(&up), Some("old"), "h"), StartAction::Recreate);
        assert_eq!(plan_start(Some(&down), Some("old"), "h"), StartAction::Recreate);
        // Adopsi: tidak tercatat → tidak pernah recreate.
        assert_eq!(plan_start(Some(&up), None, "h"), StartAction::AlreadyRunning);
        assert_eq!(plan_start(Some(&down), None, "h"), StartAction::Start);
    }

    #[test]
    fn parse_list_wslc_numeric_state_array() {
        let json = r#"[
            {"Id":"abc","Name":"servel_mysql","Image":"mysql:8.0","State":2,"Ports":[]},
            {"Id":"def","Name":"servel_redis","Image":"redis","State":3},
            {"Id":"ghi","Name":"other_app","Image":"nginx","State":2},
            {"Id":"jkl","Name":"servel_minio","Image":"minio","State":1}
        ]"#;
        let s = parse_list_json(json);
        assert_eq!(s.len(), 3);
        assert_eq!((s[0].id.as_str(), s[0].running, s[0].state.as_str()), ("mysql", true, "running"));
        assert_eq!((s[1].id.as_str(), s[1].running, s[1].state.as_str()), ("redis", false, "exited"));
        assert_eq!((s[2].id.as_str(), s[2].state.as_str()), ("minio", "created"));
    }

    #[test]
    fn parse_list_docker_like_ndjson_and_objects() {
        let ndjson = concat!(
            r#"{"ID":"1","Names":"servel_postgres","State":"running","Status":"Up 2 hours"}"#,
            "\n",
            r#"{"ID":"2","Names":["/servel_mailpit"],"Status":"Exited (137) 3 minutes ago"}"#,
            "\n",
            r#"{"Name":"/servel_mongodb","State":{"Status":"exited","Running":false,"ExitCode":1}}"#,
        );
        let s = parse_list_json(ndjson);
        assert_eq!(s.len(), 3);
        assert!(s[0].running);
        assert_eq!((s[1].id.as_str(), s[1].state.as_str(), s[1].exit_code), ("mailpit", "exited", Some(137)));
        assert_eq!((s[2].id.as_str(), s[2].exit_code), ("mongodb", Some(1)));
    }

    #[test]
    fn parse_list_ignores_garbage() {
        assert!(parse_list_json("").is_empty());
        assert!(parse_list_json("CONTAINER ID   NAME\n").is_empty());
        assert!(parse_list_json("[]").is_empty());
    }

    #[test]
    fn parse_stats_shapes() {
        let docker_like = r#"[{"Name":"servel_mysql","MemUsage":"123.4MiB / 1.95GiB"}]"#;
        assert_eq!(parse_stats_json(docker_like)[0].mem_mb, 129);

        let bytes = r#"{"Name":"servel_redis","MemoryUsage":8000000}"#;
        assert_eq!(parse_stats_json(bytes)[0].mem_mb, 8);

        let nested = r#"[{"Container":"servel_minio","Memory":{"Usage":50000000,"Limit":1}}]"#;
        let s = parse_stats_json(nested);
        assert_eq!((s[0].id.as_str(), s[0].mem_mb), ("minio", 50));

        let foreign = r#"[{"Name":"nginx","MemUsage":"1MiB / 1GiB"}]"#;
        assert!(parse_stats_json(foreign).is_empty());
    }

    #[test]
    fn parse_volume_names() {
        let v = parse_names_json(r#"[{"Name":"servel_mysql_data","Driver":"local"},{"name":"x"}]"#);
        assert!(v.contains("servel_mysql_data") && v.contains("x"));
    }

    #[test]
    fn transient_and_friendly_errors() {
        assert!(is_transient("HRESULT ERROR_SHARING_VIOLATION"));
        assert!(!is_transient("no such container"));
        assert!(friendly_error("Too many volumes have been mounted (limit: 15)").unwrap().contains("wsl --shutdown"));
        assert!(friendly_error("bind: address already in use").unwrap().contains("Port"));
        assert!(friendly_error("image not found").is_none());
    }
}
