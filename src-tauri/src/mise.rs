//! Integrasi mise sebagai version manager PHP/Node (v1.7).
//!
//! Prinsip:
//! - **Resolusi selalu oleh mise.** Servel menjalankan `mise` dengan cwd =
//!   folder project aktif (atau home untuk konteks Global) dan membaca output
//!   `--json`. Servel tidak mencari `mise.toml` sendiri, karena config bisa
//!   berada di folder induk (`D:\dev\event\mise.toml` berlaku juga di
//!   `event\source-code\event-backend`) dan sumber bisa berbeda per tool.
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

use serde::Serialize;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Stdio;
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
/// instalasi mise setelah Servel berjalan tetap terdeteksi).
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
/// dipakai tool lain di konteks ini (mis. `D:\dev\event\mise.toml` untuk
/// `event-backend`), supaya config tidak terpecah ke file baru di subfolder.
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
pub fn validate_version(version: &str) -> Result<(), String> {
    let parts: Vec<&str> = version.split('.').collect();
    let ok = !version.is_empty()
        && parts.len() <= 3
        && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()));
    if ok { Ok(()) } else { Err(format!("versi '{}' tidak valid", version)) }
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

fn map_error(stderr: &str, stdout: &str) -> String {
    if let Some(p) = untrusted_path(stderr).or_else(|| untrusted_path(stdout)) {
        return format!("{}|{}", ERR_UNTRUSTED, p);
    }
    // Pesan paling berguna: baris ERROR terakhir. Abaikan WARN (mis. notifikasi
    // "mise version X available") yang sering muncul di stderr.
    let lines: Vec<&str> = stderr.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let msg = lines
        .iter()
        .rev()
        .find(|l| l.contains("ERROR"))
        .or_else(|| lines.iter().rev().find(|l| !l.contains("WARN")))
        .copied()
        .unwrap_or("mise gagal");
    msg.trim_start_matches("mise ERROR").trim().to_string()
}

async fn capture(args: &[String], cwd: &Path) -> Result<String, String> {
    let fut = command(args, cwd).stdout(Stdio::piped()).stderr(Stdio::piped()).output();
    let out = tokio::time::timeout(std::time::Duration::from_secs(30), fut)
        .await
        .map_err(|_| format!("mise {} timeout", args.join(" ")))?
        .map_err(|e| format!("mise tidak bisa dijalankan: {}", e))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(map_error(&String::from_utf8_lossy(&out.stderr), &String::from_utf8_lossy(&out.stdout)))
    }
}

/// Jalankan mise sambil men-stream output ke panel log (source `MISE`).
async fn run_logged(app: &AppHandle, args: &[String], cwd: &Path) -> Result<(), String> {
    let emit = |line: &str, stream: &str| {
        let _ = app.emit("cmd-output", serde_json::json!({ "line": line, "stream": stream, "source": "MISE" }));
    };
    emit(&format!("mise {}", args.join(" ")), "stdout");

    let mut child = command(args, cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("mise tidak bisa dijalankan: {}", e))?;
    let stdout = child.stdout.take().ok_or("stdout mise tidak tersedia")?;
    let stderr = child.stderr.take().ok_or("stderr mise tidak tersedia")?;

    let app_out = app.clone();
    let out_task = tokio::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(l)) = lines.next_line().await {
            if !l.trim().is_empty() {
                let _ = app_out.emit("cmd-output", serde_json::json!({ "line": l, "stream": "stdout", "source": "MISE" }));
            }
        }
    });
    let app_err = app.clone();
    let err_task = tokio::spawn(async move {
        let mut acc = Vec::new();
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(l)) = lines.next_line().await {
            if !l.trim().is_empty() {
                // mise menulis progress install ke stderr — bukan error.
                let _ = app_err.emit("cmd-output", serde_json::json!({ "line": l, "stream": "stdout", "source": "MISE" }));
                acc.push(l);
            }
        }
        acc.join("\n")
    });

    let status = tokio::time::timeout(std::time::Duration::from_secs(20 * 60), child.wait())
        .await
        .map_err(|_| "mise timeout (20 menit)".to_string())?
        .map_err(|e| format!("gagal menunggu mise: {}", e))?;
    let _ = out_task.await;
    let err = err_task.await.unwrap_or_default();
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

/// Provider versi aktif: `mise` bila config `versionProvider` = mise, atau
/// `auto` dan mise terpasang. Selain itu phpvm/fnm (perilaku v1.6).
pub async fn provider_is_mise() -> bool {
    let pref = APP
        .get()
        .and_then(|a| load_config(a).ok())
        .map(|c| c.version_provider)
        .unwrap_or_else(|| "auto".to_string());
    match pref.as_str() {
        "legacy" => false,
        "mise" => true,
        _ => version().await.is_some(),
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
    /// `mise` | `legacy` (phpvm/fnm).
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
        other => return Err(format!("aksi '{}' tidak dikenal", other)),
    };
    // `doctor` exit non-zero bila menemukan masalah — laporannya tetap di log.
    let r = run_logged(&app, &args, &home_dir()).await;
    if action == "doctor" { Ok(()) } else { r }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn win_env(k: &str) -> Option<String> {
        match k {
            "USERPROFILE" => Some(r"C:\Users\devha".to_string()),
            _ => None,
        }
    }

    fn roots() -> Vec<String> {
        global_config_roots(&win_env)
    }

    #[test]
    fn parse_real_ls_current_from_event_backend() {
        // Output asli di D:\dev\event\source-code\event-backend (mise 2026.11.2).
        let tools = parse_current(include_str!("fixtures/mise-2026.11.2-ls-current.json"), &roots());
        let names: Vec<&str> = tools.iter().map(|t| t.tool.as_str()).collect();
        assert_eq!(names, vec!["go", "node", "php", "python"]);

        let node = tools.iter().find(|t| t.tool == "node").unwrap();
        assert_eq!(node.version, "24.21.0");
        assert_eq!(node.requested.as_deref(), Some("24"));
        assert_eq!(node.source.as_deref(), Some(r"D:\dev\event\mise.toml"));
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
        assert_eq!(classify_source(Some(r"C:\Users\devha\.config\mise\config.toml"), &r), SourceScope::Global);
        assert_eq!(classify_source(Some(r"c:/users/DEVHA/.config/mise/conf.d/x.toml"), &r), SourceScope::Global);
        assert_eq!(classify_source(Some(r"C:\Users\devha\.tool-versions"), &r), SourceScope::Global);
        assert_eq!(classify_source(Some(r"D:\dev\astra\mise.toml"), &r), SourceScope::Project);
        // Prefix yang mirip tapi bukan folder yang sama → bukan global.
        assert_eq!(classify_source(Some(r"C:\Users\devha\.config\mise-other\x.toml"), &r), SourceScope::Project);
        assert_eq!(classify_source(None, &r), SourceScope::None);
    }

    #[test]
    fn classify_on_macos_and_linux() {
        // v2.0: integrasi mise juga untuk macOS/Linux (HOME, bukan USERPROFILE).
        let env = |k: &str| (k == "HOME").then(|| "/home/irfan".to_string());
        let r = global_config_roots(&env);
        assert_eq!(classify_source(Some("/home/irfan/.config/mise/config.toml"), &r), SourceScope::Global);
        assert_eq!(classify_source(Some("/home/irfan/code/app/mise.toml"), &r), SourceScope::Project);
        let mac = |k: &str| (k == "HOME").then(|| "/Users/irfan".to_string());
        let r = global_config_roots(&mac);
        assert_eq!(classify_source(Some("/Users/irfan/.tool-versions"), &r), SourceScope::Global);
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

    const GLOBAL: &str = r"C:\Users\devha\.config\mise\config.toml";

    #[test]
    fn plan_use_global_context() {
        assert_eq!(plan_use(None, "php", &[], None), UsePlan::Global);
        assert_eq!(plan_use(None, "php", &[], Some("project")), UsePlan::Global);
    }

    #[test]
    fn plan_use_updates_the_file_that_applies() {
        // event-backend: node dari D:\dev\event\mise.toml (folder induk).
        let root = Path::new(r"D:\dev\event\source-code\event-backend");
        let all = vec![
            tool("node", SourceScope::Project, Some(r"D:\dev\event\mise.toml")),
            tool("php", SourceScope::Global, Some(GLOBAL)),
        ];
        let plan = plan_use(Some(root), "node", &all, None);
        assert_eq!(plan, UsePlan::File(r"D:\dev\event\mise.toml".into()));
        assert_eq!(
            use_args(&plan, "node", "22.23.3").unwrap(),
            vec!["use", "--path", r"D:\dev\event\mise.toml", "node@22.23.3"]
        );
    }

    #[test]
    fn pin_from_global_reuses_existing_project_config() {
        // PHP event-backend masih global; pin harus masuk ke D:\dev\event\mise.toml
        // (config yang sudah dipakai node), bukan file baru di subfolder.
        let root = Path::new(r"D:\dev\event\source-code\event-backend");
        let all = vec![
            tool("node", SourceScope::Project, Some(r"D:\dev\event\mise.toml")),
            tool("php", SourceScope::Global, Some(GLOBAL)),
        ];
        let target = UsePlan::NeedsScope { project_file: r"D:\dev\event\mise.toml".into() };
        assert_eq!(plan_use(Some(root), "php", &all, None), target);
        assert_eq!(plan_use(Some(root), "php", &all, Some("project")), UsePlan::File(r"D:\dev\event\mise.toml".into()));
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
        let root = Path::new("/dev/yhpa");
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
        let e = "mise ERROR Config files in D:\\dev\\astra\\mise.toml are not trusted.\nTrust them with `mise trust`.";
        assert_eq!(untrusted_path(e).as_deref(), Some(r"D:\dev\astra\mise.toml"));
        assert_eq!(map_error(e, ""), format!("{}|{}", ERR_UNTRUSTED, r"D:\dev\astra\mise.toml"));
        assert_eq!(untrusted_path("mise ERROR tool not found"), None);
    }
}
