//! Kanal kontrol lokal untuk CLI `servel` (v2.5): start/stop/restart project
//! lewat aplikasi yang sedang berjalan.
//!
//! Dev server adalah proses anak aplikasi (log, status, job object), jadi CLI
//! tidak menjalankannya sendiri; ia meminta aplikasi melakukannya.
//!
//! - Aplikasi mendengarkan di `127.0.0.1:<port acak>` dan menulis
//!   `control.json` (`port`, `token`, `pid`) ke folder data aplikasi.
//! - Satu koneksi = satu baris permintaan `<token> <aksi> [nama project]`,
//!   dibalas satu baris JSON `{ ok, message, projects }`.
//! - Token acak per sesi; hanya proses yang bisa membaca folder data pengguna
//!   yang bisa memerintah. Tidak ada aksi selain yang sudah ada di UI.

use serde::Serialize;
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

use crate::commands::config::{load_config, Project};
use crate::sites::process::{self, SiteState};

const MAX_LINE: usize = 1024;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Action {
    Start,
    Stop,
    Restart,
    Status,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Request {
    pub action: Action,
    /// Nama atau id project; kosong hanya untuk `status`.
    pub target: String,
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectStatus {
    pub name: String,
    pub state: SiteState,
    pub port: u16,
    pub exit_code: Option<i32>,
}

#[derive(Serialize, Debug)]
pub struct Response {
    pub ok: bool,
    pub message: String,
    pub projects: Vec<ProjectStatus>,
}

/// Bandingkan tanpa berhenti di byte pertama yang beda, supaya lama
/// perbandingan tidak membocorkan isi token.
fn same_token(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// `<token> <aksi> [target...]`. Target boleh berisi spasi.
pub fn parse_request(line: &str, token: &str) -> Result<Request, String> {
    let line = line.trim();
    let (given, rest) = line.split_once(' ').unwrap_or((line, ""));
    if !same_token(given, token) {
        return Err("token does not match".to_string());
    }
    let rest = rest.trim();
    let (verb, target) = rest.split_once(' ').unwrap_or((rest, ""));
    let action = match verb {
        "start" => Action::Start,
        "stop" => Action::Stop,
        "restart" => Action::Restart,
        "status" => Action::Status,
        other => return Err(format!("unknown action: {}", other)),
    };
    let target = target.trim().to_string();
    if action != Action::Status && target.is_empty() {
        return Err("project name is missing".to_string());
    }
    Ok(Request { action, target })
}

/// Cari project dari nama (tanpa beda huruf besar/kecil) atau id. Nama yang
/// dipakai lebih dari satu project ditolak supaya tidak salah sasaran.
pub fn find_project<'a>(projects: &'a [Project], target: &str) -> Result<&'a Project, String> {
    if let Some(p) = projects.iter().find(|p| p.id == target) {
        return Ok(p);
    }
    let hits: Vec<&Project> = projects.iter().filter(|p| p.name.eq_ignore_ascii_case(target)).collect();
    match hits.len() {
        0 => Err(format!("no project named \"{}\"", target)),
        1 => Ok(hits[0]),
        n => Err(format!("{} projects are named \"{}\"; rename one of them", n, target)),
    }
}

fn statuses(app: &AppHandle, projects: &[Project]) -> Vec<ProjectStatus> {
    let runtime = process::snapshot(app);
    projects
        .iter()
        .filter(|p| !p.command.trim().is_empty())
        .map(|p| {
            let rt = runtime.iter().find(|r| r.id == p.id);
            ProjectStatus {
                name: p.name.clone(),
                state: rt.map(|r| r.state).unwrap_or(SiteState::Stopped),
                port: p.port,
                exit_code: rt.and_then(|r| r.exit_code),
            }
        })
        .collect()
}

async fn handle(app: &AppHandle, line: &str, token: &str) -> Response {
    let fail = |message: String| Response { ok: false, message, projects: Vec::new() };
    let req = match parse_request(line, token) {
        Ok(r) => r,
        Err(e) => return fail(e),
    };
    let cfg = match load_config(app) {
        Ok(c) => c,
        Err(e) => return fail(e),
    };
    if req.action == Action::Status && req.target.is_empty() {
        return Response { ok: true, message: String::new(), projects: statuses(app, &cfg.projects) };
    }
    let project = match find_project(&cfg.projects, &req.target) {
        Ok(p) => p.clone(),
        Err(e) => return fail(e),
    };
    let result = match req.action {
        Action::Start => crate::sites::start_with_services(app, &project).await.map(|_| "started"),
        Action::Restart => crate::sites::restart_with_services(app, &project).await.map(|_| "restarted"),
        Action::Stop => process::stop(app, &project.id).await.map(|_| "stopped"),
        Action::Status => Ok(""),
    };
    match result {
        Ok(what) => Response {
            ok: true,
            message: if what.is_empty() { String::new() } else { format!("{} {}", project.name, what) },
            projects: statuses(app, std::slice::from_ref(&project)),
        },
        Err(e) => fail(format!("{}: {}", project.name, e)),
    }
}

fn new_token() -> Result<String, String> {
    let mut buf = [0u8; 24];
    getrandom::getrandom(&mut buf).map_err(|e| format!("could not create a token: {}", e))?;
    Ok(buf.iter().map(|b| format!("{:02x}", b)).collect())
}

/// Mulai mendengarkan dan tulis `control.json`. Gagal di sini tidak
/// menghalangi aplikasi; hanya CLI project yang tidak tersedia.
pub fn start(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = serve(app).await {
            eprintln!("[control] {}", e);
        }
    });
}

async fn serve(app: AppHandle) -> Result<(), String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let token = new_token()?;
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let info = serde_json::json!({ "port": port, "token": token, "pid": std::process::id() });
    std::fs::write(dir.join("control.json"), info.to_string()).map_err(|e| e.to_string())?;

    loop {
        let Ok((stream, _)) = listener.accept().await else { continue };
        let app = app.clone();
        let token = token.clone();
        tauri::async_runtime::spawn(async move {
            answer(stream, |line| async move { handle(&app, &line, &token).await }).await;
        });
    }
}

/// Baca satu baris permintaan (dibatasi panjang dan waktunya), balas satu
/// baris JSON, lalu tutup koneksi.
async fn answer<F, Fut>(stream: tokio::net::TcpStream, respond: F)
where
    F: FnOnce(String) -> Fut,
    Fut: std::future::Future<Output = Response>,
{
    let (read, mut write) = stream.into_split();
    let mut line = String::new();
    let mut reader = BufReader::new(read).take(MAX_LINE as u64);
    let got = tokio::time::timeout(Duration::from_secs(5), reader.read_line(&mut line)).await;
    let response = match got {
        Ok(Ok(n)) if n > 0 => respond(line).await,
        _ => Response { ok: false, message: "empty request".to_string(), projects: Vec::new() },
    };
    let mut out = serde_json::to_string(&response).unwrap_or_else(|_| "{\"ok\":false}".to_string());
    out.push('\n');
    let _ = write.write_all(out.as_bytes()).await;
    let _ = write.shutdown().await;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(id: &str, name: &str) -> Project {
        serde_json::from_value(serde_json::json!({ "id": id, "name": name })).unwrap()
    }

    #[test]
    fn parse_request_checks_token_and_action() {
        let t = "abc123";
        assert_eq!(
            parse_request("abc123 start share api\n", t),
            Ok(Request { action: Action::Start, target: "share api".into() })
        );
        assert_eq!(parse_request("abc123 status", t), Ok(Request { action: Action::Status, target: String::new() }));
        assert!(parse_request("abc124 start x", t).unwrap_err().contains("token"));
        assert!(parse_request("abc start x", t).unwrap_err().contains("token"));
        assert!(parse_request("start x", t).unwrap_err().contains("token"));
        assert!(parse_request("abc123 delete x", t).unwrap_err().contains("unknown action"));
        assert!(parse_request("abc123 stop", t).unwrap_err().contains("project name"));
    }

    #[test]
    fn find_project_by_name_or_id() {
        let ps = vec![project("1", "share-api"), project("2", "Event"), project("3", "event")];
        assert_eq!(find_project(&ps, "SHARE-API").unwrap().id, "1");
        assert_eq!(find_project(&ps, "2").unwrap().name, "Event");
        assert!(find_project(&ps, "event").unwrap_err().contains("2 projects"));
        assert!(find_project(&ps, "nope").unwrap_err().contains("no project named"));
    }

    #[test]
    fn tokens_are_long_and_differ() {
        let a = new_token().unwrap();
        assert_eq!(a.len(), 48);
        assert_ne!(a, new_token().unwrap());
    }

    /// Satu koneksi sungguhan: klien menulis satu baris, menerima satu baris JSON.
    #[tokio::test]
    async fn answers_one_line_over_tcp() {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            answer(stream, |line| async move {
                match parse_request(&line, "tok") {
                    Ok(r) => Response {
                        ok: true,
                        message: format!("{:?} {}", r.action, r.target),
                        projects: vec![ProjectStatus { name: r.target, state: SiteState::Running, port: 8001, exit_code: None }],
                    },
                    Err(e) => Response { ok: false, message: e, projects: Vec::new() },
                }
            })
            .await;
        });
        let mut client = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        client.write_all(b"tok restart share-api\n").await.unwrap();
        let mut reply = String::new();
        BufReader::new(&mut client).read_line(&mut reply).await.unwrap();
        server.await.unwrap();
        let v: serde_json::Value = serde_json::from_str(&reply).unwrap();
        assert_eq!(v["ok"], true);
        assert_eq!(v["message"], "Restart share-api");
        assert_eq!(v["projects"][0]["state"], "running");
        assert_eq!(v["projects"][0]["exitCode"], serde_json::Value::Null);
    }
}
