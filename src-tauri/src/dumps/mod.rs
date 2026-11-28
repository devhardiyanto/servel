//! Kova Dumps (spike AQ, Experimental).
//!
//! Alur: proses PHP yang dijalankan Kova (dev server, proses pendamping)
//! mendapat `PHP_INI_SCAN_DIR` berisi ini milik Kova dengan
//! `auto_prepend_file = prepend.php`. File itu mengisi
//! `$_SERVER['VAR_DUMPER_FORMAT'] = tcp://127.0.0.1:<port>`, sehingga
//! `dump()`/`dd()` dikirim ServerDumper Symfony ke penerima per project.
//! Penerima (`receiver.php`) berjalan dengan PHP dan `vendor/autoload.php`
//! milik project, jadi format serialisasi PHP dibuka oleh PHP sendiri; Kova
//! hanya membaca satu baris JSON per dump. File project tidak diubah.
//!
//! Dinyalakan lewat file penanda `<data>/dumps/enabled` (Settings › About ›
//! Diagnostics), supaya project yang auto-start sebelum UI siap ikut terbaca.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::sites::process::emit_log;

const PREPEND: &str = include_str!("prepend.php");
const RECEIVER: &str = include_str!("receiver.php");
/// Dump terbaru yang disimpan per project (di memori saja).
const KEEP: usize = 200;

#[derive(Default)]
pub struct Dumps {
    map: Mutex<HashMap<String, Receiver>>,
    counter: Mutex<u64>,
}

#[derive(Default)]
struct Receiver {
    generation: u64,
    port: u16,
    pid: Option<u32>,
    state: &'static str,
    error: Option<String>,
    php: Option<String>,
    var_dumper: Option<String>,
    items: VecDeque<Value>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    enabled: bool,
    /// Project punya `vendor/symfony/var-dumper` dengan DumpServer.
    supported: bool,
    /// off | starting | listening | failed
    state: &'static str,
    port: Option<u16>,
    error: Option<String>,
    php: Option<String>,
    var_dumper: Option<String>,
    count: usize,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn dir<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join("dumps"))
}

pub fn enabled<R: Runtime>(app: &AppHandle<R>) -> bool {
    dir(app).map(|d| d.join("enabled").is_file()).unwrap_or(false)
}

pub fn supported(project_path: &str) -> bool {
    Path::new(project_path)
        .join("vendor/symfony/var-dumper/Server/DumpServer.php")
        .is_file()
}

/// Kunci daftar project: huruf kecil, garis miring maju, tanpa garis di akhir.
/// Harus sama dengan normalisasi `getcwd()` di `prepend.php`.
fn registry_key(path: &str) -> String {
    path.replace('\\', "/").trim_end_matches('/').to_lowercase()
}

fn write_if_changed(path: &Path, text: &str) -> std::io::Result<()> {
    if std::fs::read_to_string(path).ok().as_deref() == Some(text) {
        return Ok(());
    }
    std::fs::write(path, text)
}

/// Tulis prepend, penerima, dan ini milik Kova. Mengembalikan (folder ini, receiver.php).
fn install(base: &Path) -> std::io::Result<(PathBuf, PathBuf)> {
    let ini_dir = base.join("ini");
    std::fs::create_dir_all(&ini_dir)?;
    let prepend = base.join("prepend.php");
    let receiver = base.join("receiver.php");
    write_if_changed(&prepend, PREPEND)?;
    write_if_changed(&receiver, RECEIVER)?;
    // Garis miring maju: PHP di Windows menerimanya, dan tidak ada escape di ini.
    let ini = format!(
        "; Kova Dumps (Experimental). Hanya untuk proses yang dijalankan Kova.\nauto_prepend_file=\"{}\"\n",
        prepend.to_string_lossy().replace('\\', "/")
    );
    write_if_changed(&ini_dir.join("kova-dumps.ini"), &ini)?;
    Ok((ini_dir, receiver))
}

fn update_registry(base: &Path, project_path: &str, target: Option<String>) {
    let file = base.join("projects.json");
    let mut map: serde_json::Map<String, Value> = std::fs::read_to_string(&file)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    let key = registry_key(project_path);
    match target {
        Some(t) => {
            map.insert(key, Value::String(t));
        }
        None => {
            map.remove(&key);
        }
    }
    let _ = std::fs::write(&file, Value::Object(map).to_string());
}

/// `PHP_INI_SCAN_DIR` yang menambahkan folder Kova tanpa membuang folder
/// bawaan PHP (elemen kosong = folder bawaan saat PHP dikompilasi).
fn scan_dir_value(existing: Option<&str>, ours: &Path) -> String {
    let sep = if cfg!(windows) { ';' } else { ':' };
    format!("{}{}{}", existing.unwrap_or(""), sep, ours.to_string_lossy())
}

fn free_port() -> Option<u16> {
    std::net::TcpListener::bind("127.0.0.1:0").ok()?.local_addr().ok().map(|a| a.port())
}

fn emit_status<R: Runtime>(app: &AppHandle<R>, project_id: &str) {
    let _ = app.emit("dumps://status", serde_json::json!({ "projectId": project_id }));
}

/// Dipanggil sebelum dev server atau proses pendamping PHP dijalankan Kova
/// (bukan WSL, bukan container). Menyalakan penerima project bila perlu dan
/// mengembalikan env untuk proses itu. `None` = Dumps tidak dipakai.
pub async fn prepare<R: Runtime>(
    app: &AppHandle<R>,
    project_id: &str,
    project_name: &str,
    project_path: &str,
) -> Option<Vec<(String, String)>> {
    if !enabled(app) || !supported(project_path) {
        return None;
    }
    let base = dir(app)?;
    let (ini_dir, receiver) = match install(&base) {
        Ok(x) => x,
        Err(e) => {
            emit_log(app, project_name, &format!("[dumps] could not write Kova's PHP files: {}", e), Some("warn"));
            return None;
        }
    };
    let state = app.state::<Dumps>();
    let running = lock(&state.map)
        .get(project_id)
        .filter(|r| r.state == "starting" || r.state == "listening")
        .map(|r| r.port);
    let port = match running {
        Some(p) => p,
        None => spawn(app, project_id, project_name, project_path, &receiver).await?,
    };
    let target = format!("tcp://127.0.0.1:{}", port);
    update_registry(&base, project_path, Some(target.clone()));
    let existing = std::env::var("PHP_INI_SCAN_DIR").ok();
    Some(vec![
        ("PHP_INI_SCAN_DIR".into(), scan_dir_value(existing.as_deref(), &ini_dir)),
        ("KOVA_DUMPS".into(), target),
        ("KOVA_DUMPS_REGISTRY".into(), base.join("projects.json").to_string_lossy().into_owned()),
    ])
}

async fn spawn<R: Runtime>(
    app: &AppHandle<R>,
    project_id: &str,
    project_name: &str,
    project_path: &str,
    receiver: &Path,
) -> Option<u16> {
    let port = free_port()?;
    let state = app.state::<Dumps>();
    let generation = {
        let mut c = lock(&state.counter);
        *c += 1;
        *c
    };
    let line = format!(
        "php \"{}\" \"{}\" 127.0.0.1:{}",
        receiver.to_string_lossy(),
        Path::new("vendor").join("autoload.php").to_string_lossy(),
        port
    );
    let mut cmd = crate::sites::process::shell_command(&line);
    cmd.current_dir(project_path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    // PHP yang sama dengan dev server: versi dari mise.toml project.
    if crate::mise::version().await.is_some() {
        if let Ok(env) = crate::mise::env_for(Path::new(project_path)).await {
            cmd.envs(env);
        }
    }
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            set_failed(app, project_id, generation, port, format!("could not run php: {}", e));
            return None;
        }
    };
    let pid = child.id();
    #[cfg(target_os = "windows")]
    if let Some(pid) = pid {
        let _ = crate::sites::process::job::assign(pid);
    }
    lock(&state.map).insert(
        project_id.to_string(),
        Receiver { generation, port, pid, state: "starting", ..Default::default() },
    );
    emit_status(app, project_id);
    emit_log(app, project_name, &format!("[dumps] receiver on 127.0.0.1:{}", port), None);

    if let Some(out) = child.stdout.take() {
        let app = app.clone();
        let id = project_id.to_string();
        tauri::async_runtime::spawn(async move {
            let mut lines = BufReader::new(out).lines();
            while let Ok(Some(l)) = lines.next_line().await {
                on_line(&app, &id, generation, &l);
            }
        });
    }
    if let Some(err) = child.stderr.take() {
        let app = app.clone();
        let name = project_name.to_string();
        tauri::async_runtime::spawn(async move {
            let mut lines = BufReader::new(err).lines();
            while let Ok(Some(l)) = lines.next_line().await {
                if !l.trim().is_empty() {
                    emit_log(&app, &name, &format!("[dumps] {}", l), Some("warn"));
                }
            }
        });
    }
    {
        let app = app.clone();
        let id = project_id.to_string();
        tauri::async_runtime::spawn(async move {
            let code = child.wait().await.ok().and_then(|s| s.code());
            let state = app.state::<Dumps>();
            let mut map = lock(&state.map);
            if let Some(r) = map.get_mut(&id).filter(|r| r.generation == generation) {
                if r.state != "off" {
                    r.state = "failed";
                    if r.error.is_none() {
                        r.error = Some(format!(
                            "the receiver stopped (exit {})",
                            code.map(|c| c.to_string()).unwrap_or_else(|| "?".into())
                        ));
                    }
                }
                r.pid = None;
            }
            drop(map);
            emit_status(&app, &id);
        });
    }
    Some(port)
}

fn set_failed<R: Runtime>(app: &AppHandle<R>, project_id: &str, generation: u64, port: u16, error: String) {
    let state = app.state::<Dumps>();
    lock(&state.map).insert(
        project_id.to_string(),
        Receiver { generation, port, state: "failed", error: Some(error), ..Default::default() },
    );
    emit_status(app, project_id);
}

fn on_line<R: Runtime>(app: &AppHandle<R>, project_id: &str, generation: u64, line: &str) {
    let Ok(msg) = serde_json::from_str::<Value>(line) else { return };
    let state = app.state::<Dumps>();
    let mut map = lock(&state.map);
    let Some(r) = map.get_mut(project_id).filter(|r| r.generation == generation) else { return };
    match msg.get("type").and_then(Value::as_str) {
        Some("ready") => {
            r.state = "listening";
            r.php = msg.get("php").and_then(Value::as_str).map(str::to_string);
            r.var_dumper = msg.get("varDumper").and_then(Value::as_str).map(str::to_string);
            drop(map);
            emit_status(app, project_id);
        }
        Some("error") => {
            r.state = "failed";
            r.error = msg.get("message").and_then(Value::as_str).map(str::to_string);
            drop(map);
            emit_status(app, project_id);
        }
        Some("dump") => {
            if r.items.len() >= KEEP {
                r.items.pop_front();
            }
            r.items.push_back(msg.clone());
            drop(map);
            let _ = app.emit("dumps://new", serde_json::json!({ "projectId": project_id, "dump": msg }));
        }
        _ => {}
    }
}

/// Matikan semua penerima (Dumps dimatikan, atau Kova keluar).
pub async fn stop_all<R: Runtime>(app: &AppHandle<R>) {
    let pids: Vec<(String, u32)> = {
        let state = app.state::<Dumps>();
        let mut map = lock(&state.map);
        map.iter_mut()
            .filter_map(|(id, r)| {
                r.state = "off";
                r.pid.take().map(|p| (id.clone(), p))
            })
            .collect()
    };
    if let Some(base) = dir(app) {
        let _ = std::fs::remove_file(base.join("projects.json"));
    }
    for (id, pid) in pids {
        crate::sites::process::kill_tree(pid).await;
        emit_status(app, &id);
    }
}

#[tauri::command]
pub fn dumps_enabled(app: AppHandle) -> bool {
    enabled(&app)
}

#[tauri::command]
pub async fn dumps_set_enabled(app: AppHandle, on: bool) -> Result<(), String> {
    let base = dir(&app).ok_or("no data folder")?;
    std::fs::create_dir_all(&base).map_err(|e| e.to_string())?;
    let flag = base.join("enabled");
    if on {
        std::fs::write(&flag, "1").map_err(|e| e.to_string())?;
    } else {
        let _ = std::fs::remove_file(&flag);
        stop_all(&app).await;
    }
    Ok(())
}

#[tauri::command]
pub fn dumps_status(app: AppHandle, project_id: String, project_path: String) -> Status {
    let state = app.state::<Dumps>();
    let map = lock(&state.map);
    let r = map.get(&project_id);
    Status {
        enabled: enabled(&app),
        supported: supported(&project_path),
        state: r.map(|r| r.state).unwrap_or("off"),
        port: r.map(|r| r.port),
        error: r.and_then(|r| r.error.clone()),
        php: r.and_then(|r| r.php.clone()),
        var_dumper: r.and_then(|r| r.var_dumper.clone()),
        count: r.map(|r| r.items.len()).unwrap_or(0),
    }
}

#[tauri::command]
pub fn dumps_list(app: AppHandle, project_id: String) -> Vec<Value> {
    let state = app.state::<Dumps>();
    let map = lock(&state.map);
    map.get(&project_id).map(|r| r.items.iter().cloned().collect()).unwrap_or_default()
}

#[tauri::command]
pub fn dumps_clear(app: AppHandle, project_id: String) {
    let state = app.state::<Dumps>();
    if let Some(r) = lock(&state.map).get_mut(&project_id) {
        r.items.clear();
    }
    emit_status(&app, &project_id);
}

/// Buka file sumber dump di VS Code pada barisnya (`code --goto`). File harus
/// berada di dalam folder project: path datang dari proses PHP, bukan dari Kova.
#[tauri::command]
pub async fn dumps_open_source(project_path: String, file: String, line: Option<u32>) -> Result<(), String> {
    let root = std::fs::canonicalize(&project_path).map_err(|_| format!("Folder not found: {}", project_path))?;
    let path = std::fs::canonicalize(&file).map_err(|_| format!("File not found: {}", file))?;
    if !path.starts_with(&root) || !path.is_file() {
        return Err("This file is outside the project folder.".into());
    }
    let shown = crate::editor::fs::display_path(&path);
    if shown.contains('%') || shown.contains('"') {
        return Err("This file name cannot be passed to VS Code safely.".into());
    }
    let goto = format!("{}:{}", shown, line.unwrap_or(1).max(1));
    let mut cmd = if cfg!(windows) {
        let mut c = crate::commands::util::silent_command("cmd");
        c.args(["/C", "code", "--goto", &goto]);
        c
    } else {
        let mut c = crate::commands::util::silent_command("code");
        c.args(["--goto", &goto]);
        c
    };
    cmd.current_dir(&root).stdin(Stdio::null());
    cmd.spawn().map(|_| ()).map_err(|_| {
        "VS Code was not found. Install it and turn on the `code` command (Shell Command: Install 'code' command in PATH)."
            .to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_key_matches_prepend() {
        assert_eq!(registry_key("C:\\dev\\Acme-Api\\"), "c:/dev/acme-api");
        assert_eq!(registry_key("/home/dev/shop/"), "/home/dev/shop");
    }

    #[test]
    fn scan_dir_keeps_the_default() {
        let ours = Path::new("/data/dumps/ini");
        let sep = if cfg!(windows) { ';' } else { ':' };
        assert_eq!(scan_dir_value(None, ours), format!("{}/data/dumps/ini", sep));
        assert_eq!(scan_dir_value(Some("/etc/php"), ours), format!("/etc/php{}/data/dumps/ini", sep));
    }

    #[test]
    fn install_writes_an_ini_that_points_at_prepend() {
        let base = std::env::temp_dir().join(format!("kova-dumps-test-{}", std::process::id()));
        let (ini_dir, receiver) = install(&base).unwrap();
        let ini = std::fs::read_to_string(ini_dir.join("kova-dumps.ini")).unwrap();
        assert!(ini.contains("auto_prepend_file=\""));
        assert!(ini.contains("prepend.php\""));
        assert!(!ini.contains('\\'));
        assert!(receiver.is_file());
        update_registry(&base, "/home/dev/Shop", Some("tcp://127.0.0.1:9".into()));
        let reg = std::fs::read_to_string(base.join("projects.json")).unwrap();
        assert!(reg.contains("\"/home/dev/shop\":\"tcp://127.0.0.1:9\""));
        update_registry(&base, "/home/dev/Shop", None);
        assert_eq!(std::fs::read_to_string(base.join("projects.json")).unwrap(), "{}");
        let _ = std::fs::remove_dir_all(&base);
    }
}
