//! Integrasi mise sebagai version manager PHP/Node.
//!
//! Prinsip:
//! - **Resolusi selalu oleh mise.** Kova menjalankan `mise` dengan cwd =
//!   folder project aktif (atau home untuk konteks Global) dan membaca output
//!   `--json`. Kova tidak mencari `mise.toml` sendiri, karena config bisa
//!   berada di folder induk (`D:\dev\acme\mise.toml` berlaku juga di
//!   `acme\source-code\acme-api`) dan sumber bisa berbeda per tool.
//! - **Tulis ke file yang memang berlaku.** Mengganti versi di project memakai
//!   `mise use --path <file sumber>`. Tanpa `--path`, `mise use` di subfolder
//!   akan membuat `mise.toml` baru di subfolder itu.
//! - **Pin dari global butuh keputusan user.** Bila tool di project masih
//!   bersumber dari config global, backend mengembalikan `MISE_NEEDS_SCOPE`
//!   dan UI menanyakan: pin di project, atau ubah global.
//! - **Error mise diteruskan apa adanya** ke panel log; error "not trusted"
//!   dikenali dan diberi kode `MISE_UNTRUSTED` supaya UI bisa menawarkan
//!   tombol Trust.
//!
//! Format output yang di-parse diverifikasi terhadap mise 2026.11.2 (Windows);
//! fixture ada di `src/fixtures/mise-*`.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::commands::config::{load_config, Project};
use crate::commands::util::{extract_semver, silent_command};

const BIN: &str = "mise";
/// Prefix error yang dikenali frontend.
pub const ERR_UNTRUSTED: &str = "MISE_UNTRUSTED";
pub const ERR_NEEDS_SCOPE: &str = "MISE_NEEDS_SCOPE";

static APP: OnceLock<AppHandle> = OnceLock::new();
/// Versi mise yang terdeteksi (hanya hasil positif yang di-cache, supaya
/// instalasi mise setelah Kova berjalan tetap terdeteksi).
static VERSION: OnceLock<String> = OnceLock::new();

pub fn init(app: &AppHandle) {
    let _ = APP.set(app.clone());
}

// ---------------------------------------------------------------------------
// Parsing (murni — diuji dengan output asli)
// ---------------------------------------------------------------------------

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SourceScope {
    /// Dari config global user (`~/.config/mise/config.toml`, dsb).
    Global,
    /// Dari config project (`mise.toml` di folder project atau induknya).
    Project,
    /// Tidak diminta oleh config mana pun.
    None,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ToolCurrent {
    pub tool: String,
    pub version: String,
    pub requested: Option<String>,
    /// Path file config sumber versi ini.
    pub source: Option<String>,
    pub scope: SourceScope,
    pub installed: bool,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InstalledVersion {
    pub version: String,
    pub active: bool,
}

fn str_field(m: &serde_json::Map<String, Value>, key: &str) -> Option<String> {
    m.get(key).and_then(Value::as_str).map(str::to_string).filter(|s| !s.is_empty())
}

fn source_path(m: &serde_json::Map<String, Value>) -> Option<String> {
    m.get("source")
        .and_then(Value::as_object)
        .and_then(|s| str_field(s, "path"))
}

/// Normalisasi path untuk perbandingan: `/` → `\`-agnostik, lowercase
/// (Windows case-insensitive), tanpa separator di ujung.
fn norm(p: &str) -> String {
    p.replace('\\', "/").trim_end_matches('/').to_lowercase()
}

/// Lokasi config global mise. Urutan mengikuti dokumentasi mise: variabel
/// override, lalu `$XDG_CONFIG_HOME/mise`, lalu `~/.config/mise`, plus
/// `~/.tool-versions`.
pub fn global_config_roots(env: &dyn Fn(&str) -> Option<String>) -> Vec<String> {
    let mut roots = Vec::new();
    for key in ["MISE_GLOBAL_CONFIG_FILE", "MISE_CONFIG_DIR"] {
        if let Some(v) = env(key).filter(|v| !v.is_empty()) {
            roots.push(norm(&v));
        }
    }
    if let Some(x) = env("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
        roots.push(norm(&format!("{}/mise", x)));
    }
    if let Some(home) = env("USERPROFILE").or_else(|| env("HOME")).filter(|v| !v.is_empty()) {
        roots.push(norm(&format!("{}/.config/mise", home)));
        roots.push(norm(&format!("{}/.tool-versions", home)));
    }
    roots
}

pub fn classify_source(source: Option<&str>, global_roots: &[String]) -> SourceScope {
    let Some(src) = source else { return SourceScope::None };
    let s = norm(src);
    let is_global = global_roots
        .iter()
        .any(|r| s == *r || s.starts_with(&format!("{}/", r)));
    if is_global {
        SourceScope::Global
    } else {
        SourceScope::Project
    }
}

/// Parse `mise ls --current --json`: objek `{ "<tool>": [ {...}, ... ] }`.
/// Per tool diambil entri `active` (fallback: entri pertama).
pub fn parse_current(stdout: &str, global_roots: &[String]) -> Vec<ToolCurrent> {
    let Ok(Value::Object(tools)) = serde_json::from_str::<Value>(stdout.trim()) else {
        return Vec::new();
    };
    let mut out: Vec<ToolCurrent> = tools
        .iter()
        .filter_map(|(tool, entries)| {
            let list = entries.as_array()?;
            let pick = list
                .iter()
                .filter_map(Value::as_object)
                .find(|e| e.get("active").and_then(Value::as_bool) == Some(true))
                .or_else(|| list.iter().filter_map(Value::as_object).next())?;
            let source = source_path(pick);
            Some(ToolCurrent {
                tool: tool.clone(),
                version: str_field(pick, "version")?,
                requested: str_field(pick, "requested_version"),
                scope: classify_source(source.as_deref(), global_roots),
                source,
                installed: pick.get("installed").and_then(Value::as_bool).unwrap_or(false),
            })
        })
        .collect();
    out.sort_by(|a, b| a.tool.cmp(&b.tool));
    out
}

/// Parse `mise ls --installed --json <tool>`: array entri versi.
pub fn parse_installed(stdout: &str) -> Vec<InstalledVersion> {
    let Ok(Value::Array(items)) = serde_json::from_str::<Value>(stdout.trim()) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(Value::as_object)
        .filter(|e| e.get("installed").and_then(Value::as_bool).unwrap_or(true))
        .filter_map(|e| {
            Some(InstalledVersion {
                version: str_field(e, "version")?,
                active: e.get("active").and_then(Value::as_bool).unwrap_or(false),
            })
        })
        .collect()
}

/// Versi dari `mise --version` (output bisa diawali banner ASCII).
pub fn parse_version(stdout: &str) -> Option<String> {
    stdout.lines().rev().find_map(|l| {
        let t = l.trim();
        t.chars().next().filter(char::is_ascii_digit)?;
        extract_semver(t)
    })
}

/// Path config dari error "Config files in <path> are not trusted".
pub fn untrusted_path(stderr: &str) -> Option<String> {
    let lower = stderr.to_lowercase();
    if !lower.contains("not trusted") {
        return None;
    }
    let line = stderr.lines().find(|l| l.to_lowercase().contains("not trusted"))?;
    let start = line.to_lowercase().find(" in ")? + 4;
    let end = line.to_lowercase().find(" are not trusted").or_else(|| line.to_lowercase().find(" is not trusted"))?;
    (end > start).then(|| line[start..end].trim().trim_matches('"').to_string())
}

/// Rencana perintah `mise use` untuk mengganti versi.
#[derive(Debug, PartialEq, Eq)]
pub enum UsePlan {
    Global,
    /// Tulis ke file config tertentu (sudah ada, atau `mise.toml` baru di root project).
    File(String),
    /// Tool di project masih dari global — tanyakan user dulu.
    NeedsScope { project_file: String },
}

/// `requested_scope`: `None` (otomatis), `Some("global")`, `Some("project")`.
/// `all` = semua tool hasil `mise ls --current` di konteks yang sama.
///
/// Target pin untuk tool yang masih dari global: config project yang **sudah**
/// dipakai tool lain di konteks ini (mis. `D:\dev\acme\mise.toml` untuk
/// `acme-api`), supaya config tidak terpecah ke file baru di subfolder.
/// Bila belum ada config project sama sekali → `mise.toml` di root project.
pub fn plan_use(
    project: Option<&Path>,
    tool: &str,
    all: &[ToolCurrent],
    requested_scope: Option<&str>,
) -> UsePlan {
    let Some(root) = project else { return UsePlan::Global };
    if requested_scope == Some("global") {
        return UsePlan::Global;
    }
    let project_source = |t: &ToolCurrent| {
        (t.scope == SourceScope::Project).then(|| t.source.clone()).flatten()
    };
    if let Some(src) = all.iter().find(|t| t.tool == tool).and_then(project_source) {
        return UsePlan::File(src);
    }
    let project_file = all
        .iter()
        .filter_map(project_source)
        // Paling dekat ke folder project = path terpanjang.
        .max_by_key(|p| p.len())
        .unwrap_or_else(|| root.join("mise.toml").to_string_lossy().into_owned());
    if requested_scope == Some("project") {
        UsePlan::File(project_file)
    } else {
        UsePlan::NeedsScope { project_file }
    }
}

/// Versi yang boleh diteruskan ke mise: 1–3 segmen angka (`24`, `8.3`, `22.23.3`).
/// Menolak apa pun yang bisa disalahartikan sebagai flag atau spec lain.
/// Versi untuk halaman Runtimes: `latest`, angka (`8.4.14`), atau untuk tool
/// tambahan bentuk lain yang dipakai mise (`temurin-21`, `1.2.4-beta`).
pub fn validate_tool_version(tool: &str, version: &str) -> Result<(), String> {
    if version == "latest" || validate_version(version).is_ok() {
        return Ok(());
    }
    let extra = !ALL_TOOLS.contains(&tool);
    let shape = !version.is_empty()
        && version.len() <= 64
        && version.chars().next().is_some_and(|c| c.is_ascii_alphanumeric())
        && version.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+'));
    if extra && shape { Ok(()) } else { Err(format!("version '{}' is not valid", version)) }
}

pub fn validate_version(version: &str) -> Result<(), String> {
    let parts: Vec<&str> = version.split('.').collect();
    let ok = !version.is_empty()
        && parts.len() <= 3
        && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()));
    if ok { Ok(()) } else { Err(format!("version '{}' is not valid", version)) }
}

pub fn use_args(plan: &UsePlan, tool: &str, version: &str) -> Option<Vec<String>> {
    let spec = format!("{}@{}", tool, version);
    match plan {
        UsePlan::Global => Some(vec!["use".into(), "-g".into(), spec]),
        UsePlan::File(f) => Some(vec!["use".into(), "--path".into(), f.clone(), spec]),
        UsePlan::NeedsScope { .. } => None,
    }
}

// ---------------------------------------------------------------------------
// Eksekusi
// ---------------------------------------------------------------------------

fn home_dir() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
}

/// Tempat mise dijalankan: PC ini, atau distro WSL (Runtimes › WSL · distro).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Host {
    Local,
    Wsl(String),
}

/// `distro` dari frontend → Host. Nama distro dibatasi ke karakter WSL.
pub fn host_of(distro: &Option<String>) -> Result<Host, String> {
    match distro.as_deref().map(str::trim).filter(|d| !d.is_empty()) {
        None => Ok(Host::Local),
        Some(d) if d.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_')) => Ok(Host::Wsl(d.to_string())),
        Some(d) => Err(format!("'{d}' is not a WSL distro name")),
    }
}

/// Argumen `wsl.exe` untuk `mise <args>` di distro. Argumen diteruskan lewat
/// `"$@"`, bukan disusun ke string shell. `linux_cwd` = folder kerja di distro.
pub fn wsl_mise_args(distro: &str, linux_cwd: Option<&str>, args: &[String]) -> Vec<String> {
    let mut a = vec!["-d".to_string(), distro.to_string()];
    if let Some(c) = linux_cwd {
        a.push("--cd".into());
        a.push(c.to_string());
    } else {
        a.push("--cd".into());
        a.push("~".into());
    }
    a.extend(["--exec", "bash", "-lc", "exec mise \"$@\"", "mise"].map(String::from));
    a.extend(args.iter().cloned());
    a
}

fn command_on(host: &Host, args: &[String], cwd: &Path) -> tokio::process::Command {
    match host {
        Host::Local => command(args, cwd),
        Host::Wsl(d) => {
            let linux = crate::wsl::parse(&cwd.display().to_string()).map(|w| w.path);
            let mut cmd = silent_command("wsl.exe");
            cmd.args(wsl_mise_args(d, linux.as_deref(), args))
                .env("NO_COLOR", "1")
                .env("MISE_YES", "1")
                .env("WSLENV", crate::wsl::wslenv(std::env::var("WSLENV").ok().as_deref(), &["NO_COLOR", "MISE_YES"]))
                .env("WSL_UTF8", "1")
                .stdin(Stdio::null())
                .kill_on_drop(true);
            cmd
        }
    }
}

fn command(args: &[String], cwd: &Path) -> tokio::process::Command {
    let mut cmd = silent_command(BIN);
    cmd.args(args)
        .current_dir(cwd)
        // Output stabil untuk di-parse & tanpa prompt interaktif.
        .env("NO_COLOR", "1")
        .env("MISE_YES", "1")
        .stdin(Stdio::null())
        .kill_on_drop(true);
    cmd
}

/// Baris ERROR penutup mise yang tidak menjelaskan apa pun.
fn is_error_boilerplate(line: &str) -> bool {
    let l = line.trim_start_matches("mise ERROR").trim();
    l.starts_with("Run with --verbose") || l.starts_with("Version:") || l.is_empty()
}

fn map_error(stderr: &str, stdout: &str) -> String {
    if let Some(p) = untrusted_path(stderr).or_else(|| untrusted_path(stdout)) {
        return format!("{}|{}", ERR_UNTRUSTED, p);
    }
    // Pesan paling berguna: baris ERROR terakhir yang bukan penutup ("Run with
    // --verbose…", "Version: …"). Abaikan WARN (mis. notifikasi "mise version
    // X available") yang sering muncul di stderr.
    let lines: Vec<&str> = stderr.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let msg = lines
        .iter()
        .rev()
        .find(|l| l.contains("ERROR") && !is_error_boilerplate(l))
        .or_else(|| lines.iter().rev().find(|l| !l.contains("WARN") && !l.contains("ERROR")))
        .copied()
        .unwrap_or("mise failed");
    let msg = msg.trim_start_matches("mise ERROR").trim().to_string();
    if cfg!(target_os = "linux") && (msg.contains("Failed to configure PHP") || msg.contains("Failed to build PHP")) {
        return format!("{}. {}", msg, PHP_LINUX_DEPS_HINT);
    }
    msg
}

/// Di Linux mise meng-compile PHP dari source; gagal configure hampir selalu
/// karena paket build belum terpasang.
pub const PHP_LINUX_DEPS_HINT: &str = "On Linux, mise builds PHP from source and needs build packages first. Debian/Ubuntu: sudo apt install build-essential autoconf bison re2c pkg-config libxml2-dev libsqlite3-dev libssl-dev libcurl4-openssl-dev libonig-dev libzip-dev libreadline-dev libpng-dev libjpeg-dev libfreetype-dev libicu-dev libgd-dev. Fedora: sudo dnf install gcc make autoconf bison re2c pkgconf libxml2-devel sqlite-devel openssl-devel libcurl-devel oniguruma-devel libzip-devel readline-devel libpng-devel libjpeg-turbo-devel freetype-devel libicu-devel gd-devel. Then install again.";

/// Level baris output mise untuk Logs: `mise ERROR …` dan `✗` adalah
/// error, `mise WARN …` peringatan. Progres (yang juga ditulis ke stderr) tidak.
pub fn line_level(line: &str) -> Option<&'static str> {
    let has = |w: &str| line.split_whitespace().take(3).any(|t| t == w);
    if has("ERROR") || has("✗") {
        Some("error")
    } else if has("WARN") {
        Some("warn")
    } else {
        None
    }
}

/// Uninstall gagal karena folder versi itu sedang dipakai proses lain
/// (Windows: "Access is denied (os error 5)" / "used by another process").
fn uninstall_in_use(err: &str) -> bool {
    let e = err.to_lowercase();
    e.contains("in use") || e.contains("os error 5") || e.contains("os error 32") || e.contains("failed to remove")
}

async fn capture(args: &[String], cwd: &Path) -> Result<String, String> {
    capture_on(&Host::Local, args, cwd).await
}

async fn capture_on(host: &Host, args: &[String], cwd: &Path) -> Result<String, String> {
    let fut = command_on(host, args, cwd).stdout(Stdio::piped()).stderr(Stdio::piped()).output();
    let out = tokio::time::timeout(std::time::Duration::from_secs(30), fut)
        .await
        .map_err(|_| format!("mise {} timeout", args.join(" ")))?
        .map_err(|e| format!("could not run mise: {}", e))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(map_error(&String::from_utf8_lossy(&out.stderr), &String::from_utf8_lossy(&out.stdout)))
    }
}

/// Parse `mise env --json`: objek `{ "NAMA": "nilai", ... }`. Nilai non-string
/// diabaikan.
pub fn parse_env(stdout: &str) -> Result<std::collections::HashMap<String, String>, String> {
    let v: Value = serde_json::from_str(stdout.trim()).map_err(|e| format!("mise env output is not valid: {}", e))?;
    let obj = v.as_object().ok_or_else(|| "mise env output is not an object".to_string())?;
    Ok(obj
        .iter()
        .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
        .collect())
}

/// Environment (PATH dan variabel lain) yang berlaku di `dir` menurut mise,
/// untuk menjalankan dev server project dengan versi tool yang benar.
pub async fn env_for(dir: &Path) -> Result<std::collections::HashMap<String, String>, String> {
    let out = capture(&["env".into(), "--json".into()], dir).await?;
    parse_env(&out)
}

/// Rapikan satu baris output mise untuk panel log. mise menggambar progress
/// bar dengan `\r`, kode ANSI, dan karakter blok/spinner walau tanpa terminal;
/// yang disimpan hanya keadaan terakhir baris itu, tanpa gambar bar-nya.
pub fn clean_log_line(raw: &str) -> Option<String> {
    let last = raw.rsplit('\r').find(|seg| !seg.trim().is_empty())?;
    let text: String = crate::sites::process::strip_ansi(last)
        .chars()
        .filter(|c| {
            let u = *c as u32;
            // Block Elements, Braille (spinner), dan karakter kontrol lain.
            !(0x2580..=0x259F).contains(&u) && !(0x2800..=0x28FF).contains(&u) && (!c.is_control() || *c == '\t')
        })
        .collect();
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() { None } else { Some(text) }
}

fn log_payload(line: &str) -> serde_json::Value {
    let mut v = serde_json::json!({ "line": line, "stream": "stdout", "source": "MISE" });
    if let Some(level) = line_level(line) {
        v["level"] = serde_json::Value::String(level.into());
    }
    v
}

/// `mise install` yang sedang berjalan, untuk tombol Cancel.
static INSTALL_PID: std::sync::Mutex<Option<u32>> = std::sync::Mutex::new(None);
static INSTALL_CANCELLED: AtomicBool = AtomicBool::new(false);

/// Jalankan mise sambil men-stream output ke panel log (source `MISE`).
async fn run_logged(app: &AppHandle, args: &[String], cwd: &Path) -> Result<(), String> {
    run_logged_on(app, &Host::Local, args, cwd).await
}

async fn run_logged_on(app: &AppHandle, host: &Host, args: &[String], cwd: &Path) -> Result<(), String> {
    let emit = |line: &str, stream: &str| {
        let _ = app.emit("cmd-output", serde_json::json!({ "line": line, "stream": stream, "source": "MISE" }));
    };
    let where_ = match host {
        Host::Local => String::new(),
        Host::Wsl(d) => format!("  (in {})", d),
    };
    emit(&format!("mise {}{}", args.join(" "), where_), "stdout");

    let mut child = command_on(host, args, cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("could not run mise: {}", e))?;
    let is_install = args.first().map(String::as_str) == Some("install");
    if is_install {
        INSTALL_CANCELLED.store(false, Ordering::SeqCst);
        *INSTALL_PID.lock().unwrap_or_else(|e| e.into_inner()) = child.id();
    }
    let stdout = child.stdout.take().ok_or("mise stdout not available")?;
    let stderr = child.stderr.take().ok_or("mise stderr not available")?;

    let app_out = app.clone();
    let out_task = tokio::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(l)) = lines.next_line().await {
            if let Some(l) = clean_log_line(&l) {
                let _ = app_out.emit("cmd-output", log_payload(&l));
            }
        }
    });
    let app_err = app.clone();
    let err_task = tokio::spawn(async move {
        let mut acc = Vec::new();
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(l)) = lines.next_line().await {
            if let Some(l) = clean_log_line(&l) {
                // mise menulis progress install ke stderr — bukan error; level
                // diambil dari isi baris.
                let _ = app_err.emit("cmd-output", log_payload(&l));
                acc.push(l);
            }
        }
        acc.join("\n")
    });

    let status = tokio::time::timeout(std::time::Duration::from_secs(20 * 60), child.wait())
        .await
        .map_err(|_| "mise timeout (20 menit)".to_string())?
        .map_err(|e| format!("waiting for mise failed: {}", e))?;
    let _ = out_task.await;
    let err = err_task.await.unwrap_or_default();
    if is_install {
        *INSTALL_PID.lock().unwrap_or_else(|e| e.into_inner()) = None;
        if INSTALL_CANCELLED.swap(false, Ordering::SeqCst) {
            emit("install cancelled", "stderr");
            return Err("Install cancelled.".to_string());
        }
    }
    if status.success() {
        Ok(())
    } else {
        let e = map_error(&err, "");
        emit(&e, "stderr");
        Err(e)
    }
}

pub async fn version() -> Option<String> {
    if let Some(v) = VERSION.get() {
        return Some(v.clone());
    }
    let out = command(&["--version".to_string()], &home_dir())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let v = parse_version(&String::from_utf8_lossy(&out.stdout))?;
    let _ = VERSION.set(v.clone());
    Some(v)
}

/// mise terpasang dan bisa dijalankan.
pub async fn provider_is_mise() -> bool {
    // mise satu-satunya pengelola versi; `version_provider` di config tidak
    // dibaca (field dibiarkan untuk kompatibilitas file config).
    version().await.is_some()
}

pub const ERR_NO_MISE: &str =
    "mise is not installed. Install it from Runtimes or mise.jdx.dev.";

pub async fn require_mise() -> Result<(), String> {
    if version().await.is_some() {
        Ok(())
    } else {
        Err(ERR_NO_MISE.to_string())
    }
}

fn active_project() -> Option<Project> {
    let cfg = load_config(APP.get()?).ok()?;
    let id = cfg.active_project_id?;
    cfg.projects.into_iter().find(|p| p.id == id)
}

fn context_dir(project: Option<&Project>) -> PathBuf {
    project
        .map(|p| PathBuf::from(&p.path))
        .filter(|p| p.is_dir())
        .unwrap_or_else(home_dir)
}

fn env_lookup(k: &str) -> Option<String> {
    std::env::var(k).ok()
}

async fn current_tools(project: Option<&Project>) -> Result<Vec<ToolCurrent>, String> {
    let out = capture(&["ls".into(), "--current".into(), "--json".into()], &context_dir(project)).await?;
    Ok(parse_current(&out, &global_config_roots(&env_lookup)))
}

pub async fn list_installed(tool: &str) -> Result<Vec<InstalledVersion>, String> {
    let project = active_project();
    let out = capture(
        &["ls".into(), "--installed".into(), "--json".into(), tool.into()],
        &context_dir(project.as_ref()),
    )
    .await?;
    Ok(parse_installed(&out))
}

/// Folder instalasi `tool@version` (`mise where`), mis. folder PHP 8.3.35.
pub async fn install_dir(tool: &str, version: &str) -> Result<PathBuf, String> {
    validate_version(version)?;
    let out = capture(&["where".into(), format!("{}@{}", tool, version)], &home_dir()).await?;
    let dir = out.lines().map(str::trim).rfind(|l| !l.is_empty()).unwrap_or("");
    if dir.is_empty() {
        return Err(format!("mise could not find {}@{}", tool, version));
    }
    Ok(PathBuf::from(dir))
}

pub async fn get_active(tool: &str) -> Result<Option<String>, String> {
    let project = active_project();
    Ok(current_tools(project.as_ref())
        .await?
        .into_iter()
        .find(|t| t.tool == tool)
        .map(|t| t.version))
}

pub async fn use_version(
    app: &AppHandle,
    tool: &str,
    version: &str,
    scope: Option<&str>,
) -> Result<(), String> {
    let project = active_project();
    let dir = context_dir(project.as_ref());
    let current = current_tools(project.as_ref()).await?;
    let root = project.as_ref().map(|p| PathBuf::from(&p.path));
    let plan = plan_use(root.as_deref(), tool, &current, scope);

    let Some(args) = use_args(&plan, tool, version) else {
        let UsePlan::NeedsScope { project_file } = plan else { unreachable!() };
        return Err(format!("{}|{}|{}", ERR_NEEDS_SCOPE, tool, project_file));
    };
    run_logged(app, &args, &dir).await
}

pub async fn install(app: &AppHandle, tool: &str, version: &str) -> Result<(), String> {
    let dir = context_dir(active_project().as_ref());
    run_logged(app, &["install".into(), format!("{}@{}", tool, version)], &dir).await
}

// ---------------------------------------------------------------------------
// Command Tauri
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MiseContext {
    /// `mise` | `legacy` (mise tidak terpasang).
    pub provider: String,
    pub mise_version: Option<String>,
    pub project: Option<Project>,
    /// Folder project aktif tidak ada lagi di disk.
    pub project_missing: bool,
    pub php: Option<ToolCurrent>,
    pub node: Option<ToolCurrent>,
    /// Error dari mise (mis. `MISE_UNTRUSTED|<path>`), bila ada.
    pub error: Option<String>,
}

// ---------------------------------------------------------------------------
// Runtimes lain lewat mise: Go dan Python
// ---------------------------------------------------------------------------

/// Tool yang boleh diatur dari panel Runtimes. PHP/Node punya panel sendiri.
pub const RUNTIME_TOOLS: &[&str] = &["go", "python"];

pub fn check_runtime_tool(tool: &str) -> Result<(), String> {
    if RUNTIME_TOOLS.contains(&tool) {
        Ok(())
    } else {
        Err(format!("'{tool}' is not a supported runtime"))
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeVersions {
    pub tool: String,
    pub installed: Vec<InstalledVersion>,
    /// Versi yang berlaku di konteks aktif (project terpilih atau global).
    pub current: Option<ToolCurrent>,
}

#[tauri::command]
pub async fn runtime_versions(tool: String) -> Result<RuntimeVersions, String> {
    check_runtime_tool(&tool)?;
    let installed = list_installed(&tool).await?;
    let current = current_tools(active_project().as_ref()).await?.into_iter().find(|t| t.tool == tool);
    Ok(RuntimeVersions { tool, installed, current })
}

#[tauri::command]
pub async fn runtime_install(app: AppHandle, tool: String, version: String) -> Result<(), String> {
    check_runtime_tool(&tool)?;
    validate_version(&version)?;
    install(&app, &tool, &version).await
}

/// `scope`: `global` atau `project` (project aktif, lihat `active_project`).
#[tauri::command]
pub async fn runtime_use(app: AppHandle, tool: String, version: String, scope: String) -> Result<(), String> {
    check_runtime_tool(&tool)?;
    validate_version(&version)?;
    if scope != "global" && scope != "project" {
        return Err(format!("scope '{scope}' is not valid"));
    }
    use_version(&app, &tool, &version, Some(&scope)).await
}

// ---------------------------------------------------------------------------
// Halaman Runtimes per tool
// ---------------------------------------------------------------------------

/// Tool yang punya tab di Runtimes.
pub const ALL_TOOLS: &[&str] = &["php", "node", "go", "python"];

/// Tool bawaan atau tool mise yang ditambah lewat Add tool (`bun`,
/// `rust`, `java`, `aqua:cli/cli`…). Tanpa spasi atau `@`, tidak diawali `-`.
pub fn valid_tool_name(tool: &str) -> bool {
    !tool.is_empty()
        && tool.len() <= 64
        && !tool.starts_with('-')
        && tool.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '_' | '.' | ':' | '/'))
}

fn check_any_tool(tool: &str) -> Result<(), String> {
    if ALL_TOOLS.contains(&tool) || valid_tool_name(tool) {
        Ok(())
    } else {
        Err(format!("'{tool}' is not a supported runtime"))
    }
}

#[derive(Deserialize, Clone, Debug)]
pub struct ProjectRef {
    pub name: String,
    pub path: String,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VersionRow {
    pub version: String,
    pub installed: bool,
    /// Versi global (di luar project mana pun).
    pub global: bool,
    /// Nama project yang `mise.toml`-nya meminta versi ini.
    pub used_by: Vec<String>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ToolOverview {
    pub tool: String,
    pub global: Option<String>,
    pub versions: Vec<VersionRow>,
}

/// Gabungkan versi terpasang, versi global, dan pemakaian per project.
/// Versi yang diminta project tetapi belum terpasang ikut ditampilkan.
/// Urutan: versi terbaru di atas.
pub fn build_rows(installed: &[InstalledVersion], global: Option<&str>, uses: &[(String, String)]) -> Vec<VersionRow> {
    let mut rows: Vec<VersionRow> = installed
        .iter()
        .map(|v| VersionRow { version: v.version.clone(), installed: true, global: false, used_by: Vec::new() })
        .collect();
    let row_for = |rows: &mut Vec<VersionRow>, version: &str| -> usize {
        if let Some(i) = rows.iter().position(|r| r.version == version) {
            return i;
        }
        rows.push(VersionRow { version: version.to_string(), installed: false, global: false, used_by: Vec::new() });
        rows.len() - 1
    };
    if let Some(g) = global {
        let i = row_for(&mut rows, g);
        rows[i].global = true;
    }
    for (name, version) in uses {
        let i = row_for(&mut rows, version);
        if !rows[i].used_by.contains(name) {
            rows[i].used_by.push(name.clone());
        }
    }
    let key = |v: &str| -> Vec<u64> { v.split(|c: char| !c.is_ascii_digit()).filter_map(|n| n.parse().ok()).collect() };
    rows.sort_by(|a, b| key(&b.version).cmp(&key(&a.version)));
    rows
}

#[tauri::command]
pub async fn runtime_overview(tool: String, projects: Vec<ProjectRef>, distro: Option<String>) -> Result<ToolOverview, String> {
    check_any_tool(&tool)?;
    let host = host_of(&distro)?;
    let home = home_dir();
    let installed =
        parse_installed(&capture_on(&host, &["ls".into(), "--installed".into(), "--json".into(), tool.clone()], &home).await?);
    let roots = match &host {
        Host::Local => global_config_roots(&env_lookup),
        Host::Wsl(d) => wsl_global_roots(d).await,
    };
    let global = global_version_on(&host, &tool, &home, &roots).await?;
    let mut uses = Vec::new();
    for p in &projects {
        let dir = PathBuf::from(&p.path);
        if !dir.is_dir() {
            continue;
        }
        // Di WSL hanya project di distro yang sama.
        if let Host::Wsl(d) = &host {
            if crate::wsl::parse(&p.path).is_none_or(|w| &w.distro != d) {
                continue;
            }
        }
        // Project yang mise.toml-nya belum dipercaya membuat mise gagal; lewati saja.
        let Ok(out) = capture_on(&host, &["ls".into(), "--current".into(), "--json".into()], &dir).await else { continue };
        if let Some(t) = parse_current(&out, &roots).into_iter().find(|t| t.tool == tool && t.scope == SourceScope::Project) {
            uses.push((p.name.clone(), t.version));
        }
    }
    Ok(ToolOverview { versions: build_rows(&installed, global.as_deref(), &uses), global, tool })
}

/// Pasang versi untuk tool apa pun di Runtimes. Bisa dibatalkan dengan
/// `runtime_install_cancel`.
#[tauri::command]
pub async fn runtime_install_version(app: AppHandle, tool: String, version: String, distro: Option<String>) -> Result<bool, String> {
    check_any_tool(&tool)?;
    validate_tool_version(&tool, &version)?;
    let host = host_of(&distro)?;
    let home = home_dir();
    run_logged_on(&app, &host, &["install".into(), format!("{}@{}", tool, version)], &home).await?;
    // The first version of a tool becomes the global one, so `php` works in a
    // terminal right away. A failure here does not undo the install.
    let roots = match &host {
        Host::Local => global_config_roots(&env_lookup),
        Host::Wsl(d) => wsl_global_roots(d).await,
    };
    if !matches!(global_version_on(&host, &tool, &home, &roots).await, Ok(None)) {
        return Ok(false);
    }
    if run_logged_on(&app, &host, &["use".into(), "-g".into(), format!("{}@{}", tool, version)], &home).await.is_err() {
        return Ok(false);
    }
    let _ = app.emit("runtime-global-changed", serde_json::json!({ "tool": tool, "version": version }));
    Ok(true)
}

/// Version set in the global mise config for `tool`, if any.
async fn global_version_on(host: &Host, tool: &str, home: &Path, roots: &[String]) -> Result<Option<String>, String> {
    let out = capture_on(host, &["ls".into(), "--current".into(), "--json".into()], home).await?;
    Ok(parse_current(&out, roots).into_iter().find(|t| t.tool == tool && t.scope == SourceScope::Global).map(|t| t.version))
}

#[tauri::command]
pub async fn runtime_install_cancel() -> Result<(), String> {
    let pid = *INSTALL_PID.lock().map_err(|e| e.to_string())?;
    let Some(pid) = pid else { return Ok(()) };
    INSTALL_CANCELLED.store(true, Ordering::SeqCst);
    crate::sites::process::kill_tree(pid).await;
    Ok(())
}

/// `mise use -g tool@version`.
#[tauri::command]
pub async fn runtime_set_global(app: AppHandle, tool: String, version: String, distro: Option<String>) -> Result<(), String> {
    check_any_tool(&tool)?;
    validate_tool_version(&tool, &version)?;
    let host = host_of(&distro)?;
    run_logged_on(&app, &host, &["use".into(), "-g".into(), format!("{}@{}", tool, version)], &home_dir()).await
}

/// `mise use tool@version` di folder project: menulis `mise.toml` project itu.
#[tauri::command]
pub async fn runtime_use_in(app: AppHandle, tool: String, version: String, path: String, distro: Option<String>) -> Result<(), String> {
    check_any_tool(&tool)?;
    validate_tool_version(&tool, &version)?;
    let host = host_of(&distro)?;
    let dir = PathBuf::from(&path);
    if !dir.is_dir() {
        return Err(format!("Folder not found: {path}"));
    }
    run_logged_on(&app, &host, &["use".into(), format!("{}@{}", tool, version)], &dir).await
}

/// `mise uninstall tool@version`. Frontend hanya menawarkannya untuk versi
/// yang bukan global dan tidak dipakai project.
#[tauri::command]
pub async fn runtime_uninstall(app: AppHandle, tool: String, version: String, distro: Option<String>) -> Result<(), String> {
    check_any_tool(&tool)?;
    validate_tool_version(&tool, &version)?;
    let host = host_of(&distro)?;
    run_logged_on(&app, &host, &["uninstall".into(), format!("{}@{}", tool, version)], &home_dir())
        .await
        .map_err(|e| {
            if uninstall_in_use(&e) {
                format!(
                    "{} {} is in use by a running program, such as a project, a terminal or an editor. Stop it and try again.",
                    tool_label(&tool),
                    version
                )
            } else {
                e
            }
        })
}

fn tool_label(tool: &str) -> String {
    match tool {
        "php" => "PHP".into(),
        "node" => "Node".into(),
        "go" => "Go".into(),
        "python" => "Python".into(),
        t => t.to_string(),
    }
}

/// Argumen winget untuk memasang mise tanpa dialog.
pub fn winget_install_args() -> Vec<&'static str> {
    vec![
        "install",
        "--id",
        "jdx.mise",
        "--exact",
        "--source",
        "winget",
        "--accept-source-agreements",
        "--accept-package-agreements",
        "--disable-interactivity",
    ]
}

/// Potongan output winget: baris selesai (`\n`) masuk log; potongan yang
/// diakhiri `\r` adalah gambar progres yang ditimpa winget di tempat.
#[derive(Debug, PartialEq)]
pub enum WingetSeg {
    Line(String),
    Progress(String),
}

/// Memecah output winget yang datang sepotong-sepotong menjadi baris dan
/// progres, supaya tampil langsung di kartu onboarding saat winget berjalan.
#[derive(Default)]
pub struct WingetSplitter {
    buf: Vec<u8>,
    /// Potongan sebelum `\r` terakhir: baris bila diikuti `\n` (CRLF),
    /// selain itu progres yang akan ditimpa.
    pending_cr: Option<Vec<u8>>,
}

fn clean_bytes(b: &[u8]) -> Option<String> {
    clean_log_line(&String::from_utf8_lossy(b))
}

impl WingetSplitter {
    pub fn push(&mut self, bytes: &[u8]) -> Vec<WingetSeg> {
        let mut out = Vec::new();
        for &b in bytes {
            if let Some(prev) = self.pending_cr.take() {
                if b == b'\n' {
                    out.extend(clean_bytes(&prev).map(WingetSeg::Line));
                    continue;
                }
                out.extend(clean_bytes(&prev).map(WingetSeg::Progress));
            }
            match b {
                b'\r' => self.pending_cr = Some(std::mem::take(&mut self.buf)),
                b'\n' => out.extend(clean_bytes(&std::mem::take(&mut self.buf)).map(WingetSeg::Line)),
                _ => self.buf.push(b),
            }
        }
        out
    }

    pub fn finish(&mut self) -> Option<WingetSeg> {
        let mut rest = self.pending_cr.take().unwrap_or_default();
        rest.extend(std::mem::take(&mut self.buf));
        clean_bytes(&rest).map(WingetSeg::Line)
    }
}

/// Pasang mise satu klik lewat winget (Windows). Output masuk ke Logs
/// (sumber MISE) baris per baris saat winget berjalan, dan progresnya lewat
/// event `mise-install-progress`. Setelah berhasil, folder shim winget
/// ditambahkan ke PATH proses ini supaya mise langsung terdeteksi tanpa restart.
#[tauri::command]
pub async fn mise_install_winget(app: AppHandle) -> Result<String, String> {
    use tokio::io::AsyncReadExt;
    if !cfg!(windows) {
        return Err("One-click install is available on Windows only. See mise.jdx.dev for macOS and Linux.".into());
    }
    let emit = |app: &AppHandle, line: &str| {
        let _ = app.emit("cmd-output", serde_json::json!({ "line": line, "stream": "stdout", "source": "MISE" }));
    };
    let args = winget_install_args();
    emit(&app, &format!("winget {}", args.join(" ")));
    let mut cmd = crate::commands::util::silent_command("winget");
    cmd.args(&args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
    let mut child = cmd.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            "winget is not available on this PC. Install \"App Installer\" from the Microsoft Store, or install mise from mise.jdx.dev.".to_string()
        } else {
            format!("could not run winget: {e}")
        }
    })?;

    let stdout = child.stdout.take().ok_or("winget stdout not available")?;
    let stderr = child.stderr.take().ok_or("winget stderr not available")?;
    let read = |mut pipe: Box<dyn tokio::io::AsyncRead + Unpin + Send>, app: AppHandle| {
        tokio::spawn(async move {
            let mut split = WingetSplitter::default();
            let mut text = String::new();
            let mut chunk = [0u8; 4096];
            let mut last_progress = String::new();
            loop {
                let n = match pipe.read(&mut chunk).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => n,
                };
                for seg in split.push(&chunk[..n]) {
                    match seg {
                        WingetSeg::Line(l) => {
                            emit(&app, &l);
                            text.push_str(&l);
                            text.push('\n');
                        }
                        WingetSeg::Progress(p) => {
                            if p != last_progress {
                                let _ = app.emit("mise-install-progress", serde_json::json!({ "text": p }));
                                last_progress = p;
                            }
                        }
                    }
                }
            }
            if let Some(WingetSeg::Line(l)) = split.finish() {
                emit(&app, &l);
                text.push_str(&l);
            }
            text
        })
    };
    let out_task = read(Box::new(stdout), app.clone());
    let err_task = read(Box::new(stderr), app.clone());

    let status = match tokio::time::timeout(std::time::Duration::from_secs(600), child.wait()).await {
        Ok(r) => r.map_err(|e| format!("could not run winget: {e}"))?,
        Err(_) => {
            let _ = child.kill().await;
            return Err("winget did not finish within 10 minutes".to_string());
        }
    };
    let text = format!("{}\n{}", out_task.await.unwrap_or_default(), err_task.await.unwrap_or_default());
    // winget memberi kode bukan nol bila paket sudah terpasang; anggap sukses.
    let already = text.contains("already installed") || text.contains("No newer package");
    if !status.success() && !already {
        return Err(format!("winget failed (exit {}). See Logs for details.", status.code().unwrap_or(-1)));
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        let links = PathBuf::from(local).join("Microsoft").join("WinGet").join("Links");
        let cur = std::env::var_os("PATH").unwrap_or_default();
        let mut parts: Vec<PathBuf> = std::env::split_paths(&cur).collect();
        if !parts.contains(&links) {
            parts.insert(0, links);
            if let Ok(joined) = std::env::join_paths(parts) {
                std::env::set_var("PATH", joined);
            }
        }
    }
    Ok(match version().await {
        Some(v) => format!("mise {v} is installed."),
        None => "mise is installed. Restart Kova if it is not detected yet.".into(),
    })
}

/// Versi tool yang berlaku di folder project: dipakai panel detail
/// project. Kosong bila mise tidak ada atau folder tidak ditemukan.
#[tauri::command]
pub async fn mise_project_tools(path: String) -> Result<Vec<ToolCurrent>, String> {
    let dir = PathBuf::from(&path);
    if !dir.is_dir() {
        return Err(format!("Folder not found: {path}"));
    }
    let out = capture(&["ls".into(), "--current".into(), "--json".into()], &dir).await?;
    Ok(parse_current(&out, &global_config_roots(&env_lookup)))
}

#[tauri::command]
pub async fn mise_context() -> MiseContext {
    let mise_version = version().await;
    let is_mise = provider_is_mise().await;
    let project = active_project();
    let project_missing = project.as_ref().is_some_and(|p| !Path::new(&p.path).is_dir());

    let mut ctx = MiseContext {
        provider: if is_mise { "mise" } else { "legacy" }.to_string(),
        mise_version,
        project: project.clone(),
        project_missing,
        php: None,
        node: None,
        error: None,
    };
    if !is_mise {
        return ctx;
    }
    match current_tools(project.as_ref()).await {
        Ok(tools) => {
            ctx.php = tools.iter().find(|t| t.tool == "php").cloned();
            ctx.node = tools.iter().find(|t| t.tool == "node").cloned();
        }
        Err(e) => ctx.error = Some(e),
    }
    ctx
}

/// `mise trust <path>` — dipicu tombol Trust setelah error MISE_UNTRUSTED.
#[tauri::command]
pub async fn mise_trust(app: AppHandle, path: String) -> Result<(), String> {
    let p = PathBuf::from(&path);
    let dir = p.parent().filter(|d| d.is_dir()).map(Path::to_path_buf).unwrap_or_else(home_dir);
    run_logged(&app, &["trust".into(), path], &dir).await
}

/// Aksi pemulihan: `reshim` (shim rusak / tool baru tak terdeteksi) dan
/// `cache-clear` (daftar versi remote basi).
#[tauri::command]
pub async fn mise_repair(app: AppHandle, action: String) -> Result<(), String> {
    let args: Vec<String> = match action.as_str() {
        "reshim" => vec!["reshim".into()],
        "cache-clear" => vec!["cache".into(), "clear".into()],
        "doctor" => vec!["doctor".into()],
        other => return Err(format!("unknown action '{}'", other)),
    };
    // `doctor` exit non-zero bila menemukan masalah — laporannya tetap di log.
    let r = run_logged(&app, &args, &home_dir()).await;
    if action == "doctor" { Ok(()) } else { r }
}

// ---------------------------------------------------------------------------
// Runtimes di distro WSL
// ---------------------------------------------------------------------------

/// `~/.config/mise` dan `~/.tool-versions` milik pengguna di distro.
async fn wsl_global_roots(distro: &str) -> Vec<String> {
    let out = silent_command("wsl.exe")
        .args(["-d", distro, "--exec", "sh", "-c", "printf %s \"$HOME\""])
        .env("WSL_UTF8", "1")
        .output()
        .await;
    let home = out.ok().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
    if home.is_empty() {
        return Vec::new();
    }
    vec![format!("{}/.config/mise", home), format!("{}/.tool-versions", home)]
}

/// Nama distro dari `wsl.exe -l -q` (UTF-16 tanpa WSL_UTF8; dibersihkan dari NUL).
pub fn parse_distros(out: &[u8]) -> Vec<String> {
    let text: String = if out.len() >= 2 && out[1] == 0 {
        let u16s: Vec<u16> = out.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        String::from_utf16_lossy(&u16s)
    } else {
        String::from_utf8_lossy(out).into_owned()
    };
    text.lines()
        .map(|l| l.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}' || c == '\0').to_string())
        .filter(|l| !l.is_empty() && !l.starts_with("docker-desktop") && l.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_')))
        .collect()
}

/// Distro WSL yang terpasang (Windows saja; di OS lain kosong).
#[tauri::command]
pub async fn wsl_distros() -> Vec<String> {
    if !cfg!(windows) {
        return Vec::new();
    }
    match silent_command("wsl.exe").args(["-l", "-q"]).output().await {
        Ok(o) if o.status.success() => parse_distros(&o.stdout),
        _ => Vec::new(),
    }
}

/// Versi mise di distro, `None` bila belum terpasang.
#[tauri::command]
pub async fn wsl_mise_version(distro: String) -> Result<Option<String>, String> {
    let host = host_of(&Some(distro))?;
    Ok(capture_on(&host, &["--version".into()], &home_dir()).await.ok().and_then(|o| parse_version(&o)))
}

/// Pasang mise di distro dengan installer resminya (`curl https://mise.run | sh`),
/// ke `~/.local/bin` pengguna di distro. Output ke panel log.
#[tauri::command]
pub async fn wsl_mise_install(app: AppHandle, distro: String) -> Result<(), String> {
    let Host::Wsl(d) = host_of(&Some(distro))? else { return Err("no distro".into()) };
    let emit = |line: &str, stream: &str| {
        let _ = app.emit("cmd-output", serde_json::json!({ "line": line, "stream": stream, "source": "MISE" }));
    };
    emit(&format!("curl https://mise.run | sh  (in {})", d), "stdout");
    let out = silent_command("wsl.exe")
        .args(["-d", &d, "--cd", "~", "--exec", "bash", "-lc", "curl -fsSL https://mise.run | sh"])
        .env("WSL_UTF8", "1")
        .output()
        .await
        .map_err(|e| format!("could not run wsl.exe: {}", e))?;
    for l in String::from_utf8_lossy(&out.stdout).lines().chain(String::from_utf8_lossy(&out.stderr).lines()) {
        if let Some(l) = clean_log_line(l) {
            emit(&l, "stdout");
        }
    }
    if out.status.success() {
        Ok(())
    } else {
        Err(format!("installing mise in {} failed (is curl installed there?)", d))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn runtime_tools_are_allowlisted() {
        assert!(check_runtime_tool("go").is_ok());
        assert!(check_runtime_tool("python").is_ok());
        assert!(check_runtime_tool("php").is_err());
        assert!(check_runtime_tool("--yes").is_err());
    }

    #[test]
    fn winget_installs_exact_mise_without_prompts() {
        let a = winget_install_args();
        assert_eq!(&a[..4], &["install", "--id", "jdx.mise", "--exact"]);
        assert!(a.contains(&"--disable-interactivity"));
        assert!(a.contains(&"--accept-package-agreements"));
    }

    use super::*;

    #[test]
    fn winget_splitter_separates_lines_and_progress() {
        let mut s = WingetSplitter::default();
        let mut segs = s.push(b"Found mise [jdx.mise] Version 2026.11.2\r\nDownloading https://x/mise.zip\n  \xe2\x96\x88\xe2\x96\x88  1.00 MB / 6.40 MB\r  \xe2\x96\x88\xe2\x96\x88\xe2\x96\x88  6.40 MB / 6.40 MB\r");
        segs.extend(s.push(b"\nSuccessfully ins"));
        segs.extend(s.push(b"talled\n"));
        assert_eq!(
            segs,
            vec![
                WingetSeg::Line("Found mise [jdx.mise] Version 2026.11.2".into()),
                WingetSeg::Line("Downloading https://x/mise.zip".into()),
                WingetSeg::Progress("1.00 MB / 6.40 MB".into()),
                WingetSeg::Line("6.40 MB / 6.40 MB".into()),
                WingetSeg::Line("Successfully installed".into()),
            ]
        );
        // `\r` di ujung potongan: belum bisa diputuskan sampai byte berikutnya.
        assert_eq!(s.push(b"50%\r"), vec![]);
        assert_eq!(s.push(b"\n"), vec![WingetSeg::Line("50%".into())]);
        assert_eq!(s.push(b"tail"), vec![]);
        assert_eq!(s.finish(), Some(WingetSeg::Line("tail".into())));
    }

    #[test]
    fn version_rows_merge_installed_global_and_projects() {
        let installed = vec![
            InstalledVersion { version: "8.3.35".into(), active: true },
            InstalledVersion { version: "8.4.14".into(), active: false },
            InstalledVersion { version: "8.2.9".into(), active: false },
        ];
        let uses = vec![
            ("acme-api".to_string(), "8.3.35".to_string()),
            ("shop-api".to_string(), "8.3.35".to_string()),
            ("legacy".to_string(), "7.4.33".to_string()),
        ];
        let rows = build_rows(&installed, Some("8.4.14"), &uses);
        let versions: Vec<&str> = rows.iter().map(|r| r.version.as_str()).collect();
        assert_eq!(versions, vec!["8.4.14", "8.3.35", "8.2.9", "7.4.33"]);
        assert!(rows[0].global && rows[0].used_by.is_empty());
        assert_eq!(rows[1].used_by, vec!["acme-api", "shop-api"]);
        assert!(!rows[3].installed, "requested but not installed");
        // 10 > 9 secara numerik, bukan teks.
        let rows = build_rows(
            &[InstalledVersion { version: "1.9.0".into(), active: false }, InstalledVersion { version: "1.10.0".into(), active: false }],
            None,
            &[],
        );
        assert_eq!(rows[0].version, "1.10.0");
    }

    fn win_env(k: &str) -> Option<String> {
        match k {
            "USERPROFILE" => Some(r"C:\Users\dev".to_string()),
            _ => None,
        }
    }

    fn roots() -> Vec<String> {
        global_config_roots(&win_env)
    }

    #[test]
    fn parse_real_ls_current_from_project() {
        // Output asli mise 2026.11.2 di folder project Windows.
        let tools = parse_current(include_str!("fixtures/mise-2026.11.2-ls-current.json"), &roots());
        let names: Vec<&str> = tools.iter().map(|t| t.tool.as_str()).collect();
        assert_eq!(names, vec!["go", "node", "php", "python"]);

        let node = tools.iter().find(|t| t.tool == "node").unwrap();
        assert_eq!(node.version, "24.21.0");
        assert_eq!(node.requested.as_deref(), Some("24"));
        assert_eq!(node.source.as_deref(), Some(r"D:\dev\acme\mise.toml"));
        assert_eq!(node.scope, SourceScope::Project, "config di folder induk tetap 'project'");

        let php = tools.iter().find(|t| t.tool == "php").unwrap();
        assert_eq!(php.version, "8.5.11");
        assert_eq!(php.scope, SourceScope::Global);
        assert!(php.installed);
    }

    #[test]
    fn parse_real_ls_installed_php() {
        let v = parse_installed(include_str!("fixtures/mise-2026.11.2-ls-installed-php.json"));
        let got: Vec<(&str, bool)> = v.iter().map(|x| (x.version.as_str(), x.active)).collect();
        assert_eq!(got, vec![("8.3.33", false), ("8.3.35", false), ("8.5.11", true)]);
    }

    #[test]
    fn parse_env_keeps_string_values() {
        let env = parse_env(r#"{"PATH":"C:\\mise\\php\\8.3;C:\\Windows","APP_ENV":"local","N":1}"#).unwrap();
        assert_eq!(env.get("APP_ENV").map(String::as_str), Some("local"));
        assert!(env.get("PATH").unwrap().starts_with(r"C:\mise\php\8.3"));
        assert!(!env.contains_key("N"));
        assert!(parse_env("WARN something").is_err());
        assert!(parse_env("[]").is_err());
    }

    #[test]
    fn parse_real_version_with_banner() {
        assert_eq!(
            parse_version(include_str!("fixtures/mise-2026.11.2-version.txt")).as_deref(),
            Some("2026.11.2")
        );
        assert_eq!(parse_version("2026.11.5 linux-x64 (2026-11-09)").as_deref(), Some("2026.11.5"));
        assert_eq!(parse_version("no version here"), None);
    }

    #[test]
    fn classify_global_variants() {
        let r = roots();
        assert_eq!(classify_source(Some(r"C:\Users\dev\.config\mise\config.toml"), &r), SourceScope::Global);
        assert_eq!(classify_source(Some(r"c:/users/DEV/.config/mise/conf.d/x.toml"), &r), SourceScope::Global);
        assert_eq!(classify_source(Some(r"C:\Users\dev\.tool-versions"), &r), SourceScope::Global);
        assert_eq!(classify_source(Some(r"D:\dev\shop\mise.toml"), &r), SourceScope::Project);
        // Prefix yang mirip tapi bukan folder yang sama → bukan global.
        assert_eq!(classify_source(Some(r"C:\Users\dev\.config\mise-other\x.toml"), &r), SourceScope::Project);
        assert_eq!(classify_source(None, &r), SourceScope::None);
    }

    #[test]
    fn classify_on_macos_and_linux() {
        // macOS/Linux: HOME, bukan USERPROFILE.
        let env = |k: &str| (k == "HOME").then(|| "/home/dev".to_string());
        let r = global_config_roots(&env);
        assert_eq!(classify_source(Some("/home/dev/.config/mise/config.toml"), &r), SourceScope::Global);
        assert_eq!(classify_source(Some("/home/dev/code/app/mise.toml"), &r), SourceScope::Project);
        let mac = |k: &str| (k == "HOME").then(|| "/Users/dev".to_string());
        let r = global_config_roots(&mac);
        assert_eq!(classify_source(Some("/Users/dev/.tool-versions"), &r), SourceScope::Global);
    }

    #[test]
    fn global_roots_respect_overrides() {
        let env = |k: &str| match k {
            "MISE_GLOBAL_CONFIG_FILE" => Some(r"E:\cfg\mise.toml".to_string()),
            "XDG_CONFIG_HOME" => Some(r"E:\xdg".to_string()),
            "USERPROFILE" => Some(r"C:\Users\a".to_string()),
            _ => None,
        };
        let r = global_config_roots(&env);
        assert_eq!(classify_source(Some(r"E:\cfg\mise.toml"), &r), SourceScope::Global);
        assert_eq!(classify_source(Some(r"E:\xdg\mise\config.toml"), &r), SourceScope::Global);
    }

    fn tool(name: &str, scope: SourceScope, source: Option<&str>) -> ToolCurrent {
        ToolCurrent {
            tool: name.into(),
            version: "1.0.0".into(),
            requested: None,
            source: source.map(str::to_string),
            scope,
            installed: true,
        }
    }

    const GLOBAL: &str = r"C:\Users\dev\.config\mise\config.toml";

    #[test]
    fn plan_use_global_context() {
        assert_eq!(plan_use(None, "php", &[], None), UsePlan::Global);
        assert_eq!(plan_use(None, "php", &[], Some("project")), UsePlan::Global);
    }

    #[test]
    fn plan_use_updates_the_file_that_applies() {
        // acme-api: node dari D:\dev\acme\mise.toml (folder induk).
        let root = Path::new(r"D:\dev\acme\source-code\acme-api");
        let all = vec![
            tool("node", SourceScope::Project, Some(r"D:\dev\acme\mise.toml")),
            tool("php", SourceScope::Global, Some(GLOBAL)),
        ];
        let plan = plan_use(Some(root), "node", &all, None);
        assert_eq!(plan, UsePlan::File(r"D:\dev\acme\mise.toml".into()));
        assert_eq!(
            use_args(&plan, "node", "22.23.3").unwrap(),
            vec!["use", "--path", r"D:\dev\acme\mise.toml", "node@22.23.3"]
        );
    }

    #[test]
    fn pin_from_global_reuses_existing_project_config() {
        // PHP acme-api masih global; pin harus masuk ke D:\dev\acme\mise.toml
        // (config yang sudah dipakai node), bukan file baru di subfolder.
        let root = Path::new(r"D:\dev\acme\source-code\acme-api");
        let all = vec![
            tool("node", SourceScope::Project, Some(r"D:\dev\acme\mise.toml")),
            tool("php", SourceScope::Global, Some(GLOBAL)),
        ];
        let target = UsePlan::NeedsScope { project_file: r"D:\dev\acme\mise.toml".into() };
        assert_eq!(plan_use(Some(root), "php", &all, None), target);
        assert_eq!(plan_use(Some(root), "php", &all, Some("project")), UsePlan::File(r"D:\dev\acme\mise.toml".into()));
        assert_eq!(plan_use(Some(root), "php", &all, Some("global")), UsePlan::Global);
    }

    #[test]
    fn pin_picks_nearest_project_config() {
        let root = Path::new("/w/mono/apps/api");
        let all = vec![
            tool("node", SourceScope::Project, Some("/w/mono/mise.toml")),
            tool("go", SourceScope::Project, Some("/w/mono/apps/api/mise.toml")),
            tool("php", SourceScope::Global, Some("/home/u/.config/mise/config.toml")),
        ];
        assert_eq!(
            plan_use(Some(root), "php", &all, Some("project")),
            UsePlan::File("/w/mono/apps/api/mise.toml".into())
        );
    }

    #[test]
    fn pin_without_any_project_config_creates_root_file() {
        let root = Path::new("/dev/shop-web");
        let all = vec![tool("php", SourceScope::Global, Some("/home/u/.config/mise/config.toml"))];
        let expected = root.join("mise.toml").to_string_lossy().into_owned();
        assert_eq!(plan_use(Some(root), "php", &all, None), UsePlan::NeedsScope { project_file: expected.clone() });
        // Tool belum diminta di mana pun → sama: tanya dulu.
        assert!(matches!(plan_use(Some(root), "deno", &[], None), UsePlan::NeedsScope { .. }));
    }

    #[test]
    fn version_validation() {
        for ok in ["24", "8.3", "22.23.3"] {
            assert!(validate_version(ok).is_ok(), "{ok}");
        }
        for bad in ["", "latest", "-g", "8.3.1.2", "8..3", "8.3;rm", "lts"] {
            assert!(validate_version(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn error_message_skips_warn_noise() {
        let e = "mise WARN  mise version 2026.11.5 available\nmise ERROR no version found for php@9.9";
        assert_eq!(map_error(e, ""), "no version found for php@9.9");
    }

    #[test]
    fn untrusted_error_detection() {
        let e = "mise ERROR Config files in D:\\dev\\shop\\mise.toml are not trusted.\nTrust them with `mise trust`.";
        assert_eq!(untrusted_path(e).as_deref(), Some(r"D:\dev\shop\mise.toml"));
        assert_eq!(map_error(e, ""), format!("{}|{}", ERR_UNTRUSTED, r"D:\dev\shop\mise.toml"));
        assert_eq!(untrusted_path("mise ERROR tool not found"), None);
    }

    #[test]
    fn error_message_skips_boilerplate() {
        let e = "mise ERROR failed to remove: C:\\mise\\installs\\node\\22.23.3\nmise ERROR Access is denied. (os error 5)\nmise ERROR Version: 2026.11.2 windows-x64 (2026-11-05)\nmise ERROR Run with --verbose or MISE_VERBOSE=1 for more information";
        assert_eq!(map_error(e, ""), "Access is denied. (os error 5)");
        assert!(uninstall_in_use(&map_error(e, "")));
        assert!(!uninstall_in_use("no version found for php@9.9"));
    }

    #[test]
    fn log_levels_follow_mise_prefixes() {
        assert_eq!(line_level("mise ERROR failed to uninstall core:node@22.23.3"), Some("error"));
        assert_eq!(line_level("mise ✗ node@22.23.3 18ms · failed: failed to remove"), Some("error"));
        assert_eq!(line_level("mise WARN failed to remove C:\\x"), Some("warn"));
        assert_eq!(line_level("mise ✓ node@20.20.2 589ms"), None);
        assert_eq!(line_level("go@1.26.0 removing 3.0s"), None);
        assert_eq!(line_level("A file under it is in use. ERROR handling is fine"), None);
    }

    #[test]
    fn clean_log_line_drops_progress_drawing() {
        assert_eq!(
            clean_log_line("\u{1b}[2K\u{1b}[1Gmise \u{2588}\u{2588}\u{2591}\u{2591} 0/1 \u{b7} 3.0s").as_deref(),
            Some("mise 0/1 \u{b7} 3.0s")
        );
        assert_eq!(
            clean_log_line("\u{2807} php@8.3.35 resolving\r\u{1b}[2Kmise \u{2192} php@8.3.35 already installed\r").as_deref(),
            Some("mise \u{2192} php@8.3.35 already installed")
        );
        assert_eq!(clean_log_line("\u{1b}[2K\r  "), None);
        assert_eq!(clean_log_line("mise use -g php@8.3.35").as_deref(), Some("mise use -g php@8.3.35"));
    }
}

#[cfg(test)]
mod wsl_host_tests {
    use super::*;

    #[test]
    fn wsl_args_pass_mise_arguments_as_argv() {
        let a = wsl_mise_args("Ubuntu-24.04", Some("/home/dev/a b"), &["install".into(), "go@1.23 ; rm".into()]);
        assert_eq!(&a[..4], ["-d", "Ubuntu-24.04", "--cd", "/home/dev/a b"]);
        assert_eq!(&a[4..9], ["--exec", "bash", "-lc", "exec mise \"$@\"", "mise"]);
        assert_eq!(a.last().unwrap(), "go@1.23 ; rm", "one argument, not shell text");
        assert_eq!(wsl_mise_args("U", None, &[])[3], "~");
        assert_eq!(host_of(&None).unwrap(), Host::Local);
        assert!(host_of(&Some("bad name".into())).is_err());
        assert!(valid_tool_name("bun") && valid_tool_name("aqua:cli/cli") && !valid_tool_name("-x") && !valid_tool_name("go@1"));
        assert!(validate_tool_version("php", "latest").is_ok());
        assert!(validate_tool_version("php", "temurin-21").is_err());
        assert!(validate_tool_version("java", "temurin-21").is_ok());
        assert!(validate_tool_version("java", "-rf").is_err());
    }

    #[test]
    fn distro_list_from_utf16() {
        let s = "Ubuntu-24.04\r\ndocker-desktop\r\nDebian\r\n";
        let bytes: Vec<u8> = s.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        assert_eq!(parse_distros(&bytes), vec!["Ubuntu-24.04", "Debian"]);
        assert_eq!(parse_distros(b"Ubuntu\n"), vec!["Ubuntu"]);
    }
}
