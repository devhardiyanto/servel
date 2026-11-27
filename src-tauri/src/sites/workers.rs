//! Proses tambahan project (queue worker, scheduler, Vite) yang berjalan
//! bersama dev server.
//!
//! - Kunci tiap proses: `<project id>#<nama>`. Disimpan terpisah dari dev
//!   server (`process.rs`), jadi tray, CLI, dan status project tidak berubah.
//! - Output masuk ke Logs dengan sumber nama project dan prefiks `[nama]`,
//!   sehingga chip proses di Logs langsung memisahkannya.
//! - Proses yang berhenti sendiri dijalankan lagi bila `auto_restart`, dengan
//!   jeda yang makin panjang. Setelah [`MAX_RESTARTS`] kali dalam
//!   [`RESTART_WINDOW`], Kova berhenti mencoba dan statusnya `exited`.

use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tokio::io::{AsyncBufReadExt, BufReader};

use super::process::{self, emit_log, kill_tree, line_level, strip_ansi, SiteState};
use crate::commands::config::{Project, ProjectProcess};
use crate::commands::util::silent_command;

const MAX_RESTARTS: usize = 5;
const RESTART_WINDOW: Duration = Duration::from_secs(120);
/// Proses yang hidup selama ini dianggap sudah berjalan normal.
const SETTLE: Duration = Duration::from_secs(2);

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkerRuntime {
    pub project_id: String,
    pub name: String,
    pub state: SiteState,
    pub exit_code: Option<i32>,
    pub last_line: Option<String>,
    /// Berapa kali Kova menjalankan ulang proses ini setelah crash.
    pub restarts: usize,
    /// Kova berhenti mencoba setelah terlalu sering crash.
    pub gave_up: bool,
}

struct Entry {
    pid: Option<u32>,
    state: SiteState,
    exit_code: Option<i32>,
    last_line: Option<String>,
    stopping: bool,
    generation: u64,
    wsl_group: Option<(String, u32)>,
    crashes: Vec<Instant>,
    restarts: usize,
    gave_up: bool,
}

#[derive(Default)]
pub struct Workers {
    entries: Mutex<HashMap<String, Entry>>,
    counter: Mutex<u64>,
}

pub fn key(project_id: &str, name: &str) -> String {
    format!("{}#{}", project_id, name)
}

fn split_key(k: &str) -> (&str, &str) {
    k.split_once('#').unwrap_or((k, ""))
}

impl Workers {
    fn snapshot(&self) -> Vec<WorkerRuntime> {
        let map = self.entries.lock().unwrap();
        let mut out: Vec<WorkerRuntime> = map
            .iter()
            .map(|(k, e)| {
                let (pid, name) = split_key(k);
                WorkerRuntime {
                    project_id: pid.to_string(),
                    name: name.to_string(),
                    state: e.state,
                    exit_code: e.exit_code,
                    last_line: e.last_line.clone(),
                    restarts: e.restarts,
                    gave_up: e.gave_up,
                }
            })
            .collect();
        out.sort_by(|a, b| (&a.project_id, &a.name).cmp(&(&b.project_id, &b.name)));
        out
    }

    fn is_active(&self, k: &str) -> bool {
        self.entries
            .lock()
            .unwrap()
            .get(k)
            .is_some_and(|e| matches!(e.state, SiteState::Starting | SiteState::Running))
    }
}

fn emit_status<R: Runtime>(app: &AppHandle<R>) {
    let _ = app.emit("site-workers-changed", app.state::<Workers>().snapshot());
}

pub fn snapshot<R: Runtime>(app: &AppHandle<R>) -> Vec<WorkerRuntime> {
    app.state::<Workers>().snapshot()
}

/// PID proses yang hidup, untuk RAM project.
pub fn running_pids<R: Runtime>(app: &AppHandle<R>) -> Vec<u32> {
    app.state::<Workers>().entries.lock().unwrap().values().filter_map(|e| e.pid).collect()
}

/// Nama proses: huruf, angka, `-`, `_`, `.`, `:`; 1–24 karakter. Sama dengan
/// prefiks yang dikenali chip proses di Logs.
pub fn validate_name(name: &str) -> Result<(), String> {
    let ok = !name.is_empty()
        && name.len() <= 24
        && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':'));
    if ok {
        Ok(())
    } else {
        Err(format!("process name \"{}\": use 1–24 letters, digits, - _ . or :", name))
    }
}

pub fn validate(site: &Project) -> Result<(), String> {
    let mut seen = std::collections::HashSet::new();
    for p in &site.processes {
        validate_name(&p.name)?;
        if p.command.trim().is_empty() {
            return Err(format!("process {}: the command is empty", p.name));
        }
        if !seen.insert(p.name.as_str()) {
            return Err(format!("process {} is listed twice", p.name));
        }
    }
    Ok(())
}

/// Jalankan semua proses tambahan project yang belum berjalan. Kegagalan satu
/// proses masuk ke Logs dan tidak menghalangi yang lain.
pub async fn start_all<R: Runtime>(app: &AppHandle<R>, site: &Project) {
    if site.processes.is_empty() {
        return;
    }
    if !site.run_mode.is_command() {
        emit_log(app, &site.name, "extra processes only run in Command mode; skipped", Some("warn"));
        return;
    }
    for p in &site.processes {
        if let Err(e) = start_one(app, site, p).await {
            emit_log(app, &site.name, &format!("[{}] could not start: {}", p.name, e), Some("error"));
        }
    }
}

/// Hentikan semua proses tambahan project `project_id`.
pub async fn stop_all_for<R: Runtime>(app: &AppHandle<R>, project_id: &str) {
    let prefix = format!("{}#", project_id);
    let names: Vec<String> = app
        .state::<Workers>()
        .entries
        .lock()
        .unwrap()
        .keys()
        .filter(|k| k.starts_with(&prefix))
        .cloned()
        .collect();
    let handles: Vec<_> = names
        .into_iter()
        .map(|k| {
            let app = app.clone();
            tauri::async_runtime::spawn(async move { stop_key(&app, &k).await })
        })
        .collect();
    for h in handles {
        let _ = h.await;
    }
}

/// Hentikan semua proses tambahan (saat Kova keluar).
pub async fn stop_everything<R: Runtime>(app: &AppHandle<R>) {
    let keys: Vec<String> = app.state::<Workers>().entries.lock().unwrap().keys().cloned().collect();
    let handles: Vec<_> = keys
        .into_iter()
        .map(|k| {
            let app = app.clone();
            tauri::async_runtime::spawn(async move { stop_key(&app, &k).await })
        })
        .collect();
    for h in handles {
        let _ = h.await;
    }
}

pub async fn stop<R: Runtime>(app: &AppHandle<R>, project_id: &str, name: &str) {
    stop_key(app, &key(project_id, name)).await;
}

async fn stop_key<R: Runtime>(app: &AppHandle<R>, k: &str) {
    let target = {
        let workers = app.state::<Workers>();
        let mut map = workers.entries.lock().unwrap();
        match map.get_mut(k) {
            Some(e) => {
                e.stopping = true;
                e.pid.map(|p| (p, e.wsl_group.clone()))
            }
            None => None,
        }
    };
    let Some((pid, group)) = target else { return };
    if let Some((distro, pgid)) = &group {
        crate::wsl::kill_group(distro, *pgid, "TERM").await;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while app.state::<Workers>().entries.lock().unwrap().get(k).is_some_and(|e| e.pid == Some(pid)) {
            if tokio::time::Instant::now() >= deadline {
                crate::wsl::kill_group(distro, *pgid, "KILL").await;
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
    kill_tree(pid).await;
    // Tunggu `wait` mencatat status, supaya restart sesudahnya tidak ditolak.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while app.state::<Workers>().entries.lock().unwrap().get(k).is_some_and(|e| e.pid.is_some()) {
        if tokio::time::Instant::now() >= deadline {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

pub async fn restart<R: Runtime>(app: &AppHandle<R>, site: &Project, name: &str) -> Result<(), String> {
    let p = site
        .processes
        .iter()
        .find(|p| p.name == name)
        .ok_or_else(|| format!("{} has no process named {}", site.name, name))?
        .clone();
    stop(app, &site.id, name).await;
    start_one(app, site, &p).await
}

/// Lupakan proses yang sudah tidak ada di config dan tidak berjalan.
pub fn forget_missing<R: Runtime>(app: &AppHandle<R>, site: &Project) {
    let prefix = format!("{}#", site.id);
    let workers = app.state::<Workers>();
    let mut map = workers.entries.lock().unwrap();
    map.retain(|k, e| {
        !(k.starts_with(&prefix) && e.pid.is_none() && !site.processes.iter().any(|p| key(&site.id, &p.name) == *k))
    });
}

pub fn forget_project<R: Runtime>(app: &AppHandle<R>, project_id: &str) {
    let prefix = format!("{}#", project_id);
    let workers = app.state::<Workers>();
    workers.entries.lock().unwrap().retain(|k, e| !(k.starts_with(&prefix) && e.pid.is_none()));
}

pub async fn start_one<R: Runtime>(app: &AppHandle<R>, site: &Project, p: &ProjectProcess) -> Result<(), String> {
    validate_name(&p.name)?;
    let k = key(&site.id, &p.name);
    let workers = app.state::<Workers>();
    if workers.is_active(&k) {
        return Ok(());
    }
    if !Path::new(&site.path).is_dir() {
        return Err(format!("folder not found: {}", site.path));
    }
    if p.command.trim().is_empty() {
        return Err("the command is empty".to_string());
    }
    let wsl_target = if cfg!(target_os = "windows") && site.wsl {
        Some(crate::wsl::parse(&site.path).ok_or_else(|| "Run inside WSL needs a folder in a WSL distro".to_string())?)
    } else {
        None
    };
    if wsl_target.is_none() && process::is_unc(&site.path) {
        return Err("extra processes are not supported for network folders yet".to_string());
    }
    let line = super::detect::render_command(p.command.trim(), site.port);
    let mut cmd = match &wsl_target {
        Some(wp) => {
            let mut c = silent_command("wsl.exe");
            c.args(crate::wsl::start_args(wp, &line));
            c
        }
        None => process::shell_command(&line),
    };
    cmd.current_dir(if wsl_target.is_some() { std::env::temp_dir() } else { PathBuf::from(&site.path) })
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("PORT", site.port.to_string())
        .env("BROWSER", "none")
        .env("NO_COLOR", "1")
        .env("FORCE_COLOR", "0");
    if wsl_target.is_none() && crate::mise::version().await.is_some() {
        if let Ok(env) = crate::mise::env_for(Path::new(&site.path)).await {
            cmd.envs(env);
        }
    }
    let env = site.effective_env();
    cmd.envs(&env);
    if wsl_target.is_some() {
        let mut names: Vec<&str> = vec!["PORT", "BROWSER", "NO_COLOR", "FORCE_COLOR"];
        names.extend(env.keys().map(String::as_str));
        let existing = std::env::var("WSLENV").ok();
        cmd.env("WSLENV", crate::wsl::wslenv(existing.as_deref(), &names));
    }

    let source = site.name.clone();
    let tag = format!("[{}]", p.name);
    emit_log(app, &source, &format!("{} $ {}", tag, line), None);
    let mut child = cmd.spawn().map_err(|e| format!("could not run the command: {}", e))?;
    let pid = child.id();
    let hold_stdin = child.stdin.take();
    if let Some(pid) = pid {
        #[cfg(target_os = "windows")]
        let _ = process::job::assign(pid);
        process::track_pid(app, &k, pid).await;
    }
    let generation = {
        let mut c = workers.counter.lock().unwrap();
        *c += 1;
        *c
    };
    {
        let mut map = workers.entries.lock().unwrap();
        let prev = map.remove(&k);
        let (crashes, restarts) = prev.map(|e| (e.crashes, e.restarts)).unwrap_or_default();
        map.insert(
            k.clone(),
            Entry {
                pid,
                state: SiteState::Starting,
                exit_code: None,
                last_line: None,
                stopping: false,
                generation,
                wsl_group: None,
                crashes,
                restarts,
                gave_up: false,
            },
        );
    }
    emit_status(app);

    for (stream, capture) in [
        (child.stdout.take().map(Pipe::Out), wsl_target.as_ref().map(|w| w.distro.clone())),
        (child.stderr.take().map(Pipe::Err), None),
    ] {
        let Some(stream) = stream else { continue };
        let (app, k, source, tag) = (app.clone(), k.clone(), source.clone(), tag.clone());
        tauri::async_runtime::spawn(async move {
            match stream {
                Pipe::Out(s) => pump(app, k, source, tag, generation, BufReader::new(s), capture).await,
                Pipe::Err(s) => pump(app, k, source, tag, generation, BufReader::new(s), capture).await,
            }
        });
    }

    // Starting → Running setelah proses bertahan beberapa detik.
    {
        let (app, k) = (app.clone(), k.clone());
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(SETTLE).await;
            let changed = {
                let workers = app.state::<Workers>();
                let mut map = workers.entries.lock().unwrap();
                match map.get_mut(&k) {
                    Some(e) if e.generation == generation && e.state == SiteState::Starting => {
                        e.state = SiteState::Running;
                        true
                    }
                    _ => false,
                }
            };
            if changed {
                emit_status(&app);
            }
        });
    }

    let (wait_app, wait_site, wait_proc) = (app.clone(), site.clone(), p.clone());
    tauri::async_runtime::spawn(async move {
        let code = child.wait().await.ok().and_then(|s| s.code());
        drop(hold_stdin);
        let k = key(&wait_site.id, &wait_proc.name);
        process::untrack_pid(&wait_app, &k);
        let mut retry_after = None;
        let mut gave_up = false;
        let requested;
        {
            let workers = wait_app.state::<Workers>();
            let mut map = workers.entries.lock().unwrap();
            let Some(e) = map.get_mut(&k).filter(|e| e.generation == generation) else { return };
            requested = e.stopping;
            e.pid = None;
            e.exit_code = code;
            if requested {
                e.state = SiteState::Stopped;
                e.last_line = None;
            } else {
                e.state = SiteState::Exited;
                let now = Instant::now();
                e.crashes.retain(|t| now.duration_since(*t) < RESTART_WINDOW);
                e.crashes.push(now);
                if wait_proc.auto_restart {
                    if e.crashes.len() > MAX_RESTARTS {
                        e.gave_up = true;
                        gave_up = true;
                    } else {
                        retry_after = Some(backoff(e.crashes.len()));
                    }
                }
            }
        }
        let tag = format!("[{}]", wait_proc.name);
        if requested {
            emit_log(&wait_app, &wait_site.name, &format!("{} stopped", tag), None);
        } else {
            let ended = match code {
                Some(c) => format!("{} the process ended (exit {})", tag, c),
                None => format!("{} the process ended", tag),
            };
            emit_log(&wait_app, &wait_site.name, &ended, Some("error"));
            if gave_up {
                emit_log(
                    &wait_app,
                    &wait_site.name,
                    &format!("{} crashed {} times in 2 minutes; Kova stopped restarting it", tag, MAX_RESTARTS + 1),
                    Some("error"),
                );
                crate::notify::process_gave_up(&wait_app, &wait_site.name, &wait_proc.name);
            } else if !wait_proc.auto_restart {
                crate::notify::process_exited(&wait_app, &wait_site.name, &wait_proc.name, code);
            }
        }
        emit_status(&wait_app);
        if let Some(delay) = retry_after {
            emit_log(
                &wait_app,
                &wait_site.name,
                &format!("{} restarting in {} s", tag, delay.as_secs()),
                Some("warn"),
            );
            tokio::time::sleep(delay).await;
            // Dihentikan atau dijalankan ulang pengguna selama jeda: jangan ganggu.
            let still_exited = wait_app
                .state::<Workers>()
                .entries
                .lock()
                .unwrap()
                .get(&k)
                .is_some_and(|e| e.generation == generation && e.state == SiteState::Exited && !e.stopping);
            if still_exited {
                {
                    let workers = wait_app.state::<Workers>();
                    let mut map = workers.entries.lock().unwrap();
                    if let Some(e) = map.get_mut(&k) {
                        e.restarts += 1;
                    }
                }
                if let Err(err) = start_boxed(wait_app.clone(), wait_site.clone(), wait_proc.clone()).await {
                    emit_log(&wait_app, &wait_site.name, &format!("{} could not restart: {}", tag, err), Some("error"));
                }
            }
        }
    });
    Ok(())
}

/// `start_one` lewat `Box<dyn Future + Send>`: restart otomatis memanggil
/// `start_one` dari dalam task yang dibuat `start_one` sendiri.
fn start_boxed<R: Runtime>(
    app: AppHandle<R>,
    site: Project,
    p: ProjectProcess,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send>> {
    Box::pin(async move { start_one(&app, &site, &p).await })
}

/// Jeda sebelum restart ke-`n` (1-based): 1, 2, 5, 10, 20 detik.
pub fn backoff(n: usize) -> Duration {
    const STEPS: [u64; 5] = [1, 2, 5, 10, 20];
    Duration::from_secs(STEPS[n.saturating_sub(1).min(STEPS.len() - 1)])
}

enum Pipe {
    Out(tokio::process::ChildStdout),
    Err(tokio::process::ChildStderr),
}

async fn pump<R: Runtime, B: tokio::io::AsyncBufRead + Unpin>(
    app: AppHandle<R>,
    k: String,
    source: String,
    tag: String,
    generation: u64,
    reader: B,
    mut pgid_distro: Option<String>,
) {
    let mut lines = reader.lines();
    while let Ok(Some(raw)) = lines.next_line().await {
        let line = strip_ansi(&raw);
        let line = line.trim_end();
        if line.trim().is_empty() {
            continue;
        }
        if let Some(distro) = pgid_distro.take() {
            if let Some(pgid) = crate::wsl::parse_pgid(line) {
                let workers = app.state::<Workers>();
                let mut map = workers.entries.lock().unwrap();
                if let Some(e) = map.get_mut(&k).filter(|e| e.generation == generation) {
                    e.wsl_group = Some((distro, pgid));
                }
                continue;
            }
        }
        emit_log(&app, &source, &format!("{} {}", tag, line), line_level(line));
        let workers = app.state::<Workers>();
        let mut map = workers.entries.lock().unwrap();
        if let Some(e) = map.get_mut(&k).filter(|e| e.generation == generation) {
            e.last_line = Some(line.to_string());
        }
    }
}

/// Usulan proses tambahan dari isi folder: queue dan scheduler untuk Laravel,
/// Vite bila `package.json` punya script `dev` dan perintah utama bukan itu.
pub fn suggestions(dir: &Path, main_command: &str) -> Vec<ProjectProcess> {
    let mut out = Vec::new();
    let is_laravel = dir.join("artisan").is_file();
    let main = main_command.to_lowercase();
    // `composer run dev` / `artisan dev` sudah menjalankan queue dan Vite sendiri.
    let runs_bundle = main.contains("composer run dev") || main.contains("artisan dev");
    if is_laravel && !runs_bundle {
        out.push(ProjectProcess {
            name: "queue".into(),
            command: "php artisan queue:listen --tries=1".into(),
            auto_restart: true,
        });
    }
    if is_laravel {
        out.push(ProjectProcess { name: "scheduler".into(), command: "php artisan schedule:work".into(), auto_restart: true });
    }
    let has_dev_script = std::fs::read_to_string(dir.join("package.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v.get("scripts").and_then(|s| s.get("dev")).cloned())
        .is_some();
    let main_is_node_dev = main.contains("npm run dev") || main.contains("pnpm dev") || main.contains("yarn dev");
    if is_laravel && has_dev_script && !main_is_node_dev && !runs_bundle {
        out.push(ProjectProcess { name: "vite".into(), command: "npm run dev".into(), auto_restart: true });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_follow_log_prefix_rules() {
        assert!(validate_name("queue").is_ok());
        assert!(validate_name("horizon:main").is_ok());
        assert!(validate_name("").is_err());
        assert!(validate_name("has space").is_err());
        assert!(validate_name(&"x".repeat(25)).is_err());
    }

    #[test]
    fn duplicate_or_empty_processes_are_rejected() {
        let mut p: Project = serde_json::from_str(r#"{"id":"1","name":"api"}"#).unwrap();
        assert!(validate(&p).is_ok());
        p.processes = vec![
            ProjectProcess { name: "queue".into(), command: "a".into(), auto_restart: true },
            ProjectProcess { name: "queue".into(), command: "b".into(), auto_restart: true },
        ];
        assert!(validate(&p).unwrap_err().contains("twice"));
        p.processes.pop();
        p.processes[0].command = " ".into();
        assert!(validate(&p).unwrap_err().contains("empty"));
    }

    #[test]
    fn auto_restart_defaults_on_and_old_configs_load() {
        let p: Project = serde_json::from_str(r#"{"id":"1","name":"api","processes":[{"name":"queue","command":"x"}]}"#).unwrap();
        assert!(p.processes[0].auto_restart);
        let p: Project = serde_json::from_str(r#"{"id":"1","name":"api"}"#).unwrap();
        assert!(p.processes.is_empty());
        // Tanpa proses, field tidak ditulis (config Kova lama tetap sama).
        assert!(!serde_json::to_string(&p).unwrap().contains("processes"));
    }

    #[test]
    fn backoff_grows_and_caps() {
        assert_eq!(backoff(1), Duration::from_secs(1));
        assert_eq!(backoff(3), Duration::from_secs(5));
        assert_eq!(backoff(9), Duration::from_secs(20));
    }

    #[test]
    fn key_round_trips() {
        assert_eq!(split_key(&key("p1", "queue")), ("p1", "queue"));
    }

    #[test]
    fn suggestions_for_laravel() {
        let dir = std::env::temp_dir().join(format!("kova-workers-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("artisan"), "").unwrap();
        std::fs::write(dir.join("package.json"), r#"{"scripts":{"dev":"vite"}}"#).unwrap();
        let names: Vec<String> = suggestions(&dir, "php artisan serve --port {port}").into_iter().map(|p| p.name).collect();
        assert_eq!(names, vec!["queue", "scheduler", "vite"]);
        let names: Vec<String> = suggestions(&dir, "composer run dev").into_iter().map(|p| p.name).collect();
        assert_eq!(names, vec!["scheduler"]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
