//! Kanal kontrol lokal untuk CLI `kova`: start/stop/restart project
//! lewat aplikasi yang sedang berjalan.
//!
//! Dev server adalah proses anak aplikasi (log, status, job object), jadi CLI
//! tidak menjalankannya sendiri; ia meminta aplikasi melakukannya.
//!
//! - Aplikasi mendengarkan di `127.0.0.1:<port acak>` dan menulis
//!   `control.json` (`port`, `token`, `pid`) ke folder data aplikasi.
//! - Satu koneksi = satu baris permintaan `<token> <aksi> [nama project]`,
//!   dibalas satu baris JSON `{ ok, message, projects }`.
//! - `<token> plain <aksi> ...` dibalas teks baris-per-baris (lihat
//!   [`to_plain`]) untuk CLI shell di macOS/Linux, dan aksi service
//!   `up [id...]`, `down [id...]`, `services`.
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
    /// Service: start `target` (id dipisah spasi; kosong = pilihan tersimpan).
    Up,
    /// Service: stop `target` (kosong = semua).
    Down,
    /// Service: daftar status.
    Services,
    /// Service: daftar yang tersedia beserta image dan port (`kova list`).
    Catalog,
    /// Tampilkan jendela utama. Dikirim oleh Kova yang dibuka kedua kali,
    /// supaya hanya ada satu Kova yang berjalan.
    Show,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Request {
    pub action: Action,
    /// Nama atau id project; kosong hanya untuk `status` dan aksi service.
    pub target: String,
    /// Balasan teks, bukan JSON.
    pub plain: bool,
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
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub services: Vec<ServiceRow>,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct ServiceRow {
    pub id: String,
    pub state: String,
    /// Hanya terisi untuk `list`: image dan pemetaan port `host->container`.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub image: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub ports: String,
}

impl Response {
    fn fail(message: String) -> Self {
        Response { ok: false, message, projects: Vec::new(), services: Vec::new() }
    }
}

fn one_line(s: &str) -> String {
    s.replace(['\n', '\r', '\t'], " ")
}

/// Balasan untuk klien shell: baris 1 `ok` atau `error`, baris 2 pesan (boleh
/// kosong), lalu satu baris per entri dengan kolom dipisah tab:
/// `project<TAB>nama<TAB>status<TAB>url`, `service<TAB>id<TAB>status`, atau
/// untuk `list`: `catalog<TAB>id<TAB>image<TAB>port`.
pub fn to_plain(r: &Response) -> String {
    let mut out = format!("{}\n{}\n", if r.ok { "ok" } else { "error" }, one_line(&r.message));
    for p in &r.projects {
        let state = serde_json::to_value(p.state).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default();
        let state = match p.exit_code {
            Some(code) if state == "exited" => format!("exited ({})", code),
            _ => state,
        };
        let url = if p.port > 0 { format!("http://localhost:{}", p.port) } else { String::new() };
        out.push_str(&format!("project\t{}\t{}\t{}\n", one_line(&p.name), state, url));
    }
    for s in &r.services {
        if !s.image.is_empty() {
            out.push_str(&format!("catalog\t{}\t{}\t{}\n", one_line(&s.id), one_line(&s.image), one_line(&s.ports)));
            continue;
        }
        out.push_str(&format!("service\t{}\t{}\n", one_line(&s.id), one_line(&s.state)));
    }
    out
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
    let (plain, rest) = match rest.split_once(' ') {
        Some(("plain", more)) => (true, more.trim()),
        _ if rest == "plain" => (true, ""),
        _ => (false, rest),
    };
    let (verb, target) = rest.split_once(' ').unwrap_or((rest, ""));
    let action = match verb {
        "start" => Action::Start,
        "stop" => Action::Stop,
        "restart" => Action::Restart,
        "status" => Action::Status,
        "up" => Action::Up,
        "down" => Action::Down,
        "services" => Action::Services,
        "list" => Action::Catalog,
        "show" => Action::Show,
        other => return Err(format!("unknown action: {}", other)),
    };
    let target = target.trim().to_string();
    let needs_project = matches!(action, Action::Start | Action::Stop | Action::Restart);
    if needs_project && target.is_empty() {
        return Err("project name is missing".to_string());
    }
    Ok(Request { action, target, plain })
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

async fn service_rows() -> Result<Vec<ServiceRow>, String> {
    let mut rows: Vec<ServiceRow> = crate::runtime::status()
        .await?
        .into_iter()
        .map(|s| ServiceRow { id: s.id, state: s.state, image: String::new(), ports: String::new() })
        .collect();
    rows.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(rows)
}

/// Id service dari `target`; id yang tidak dikenal ditolak dengan daftar yang ada.
pub fn service_ids(target: &str, known: &[String]) -> Result<Vec<String>, String> {
    let ids: Vec<String> = target.split_whitespace().map(|s| s.to_ascii_lowercase()).collect();
    if let Some(bad) = ids.iter().find(|id| !known.contains(id)) {
        return Err(format!("no service named \"{}\" (available: {})", bad, known.join(", ")));
    }
    Ok(ids)
}

/// Baris `list` dari definisi service. Tidak menyentuh engine container,
/// jadi tetap menjawab saat Docker mati.
pub fn catalog_rows(defs: &[crate::commands::services::ServiceDef]) -> Vec<ServiceRow> {
    defs.iter()
        .map(|d| ServiceRow {
            id: d.id.clone(),
            state: String::new(),
            image: d.image.clone(),
            ports: d.ports.iter().map(|p| format!("{}->{}", p.host, p.container)).collect::<Vec<_>>().join(", "),
        })
        .collect()
}

async fn handle_services(app: &AppHandle, req: &Request) -> Result<Response, String> {
    use crate::commands::services::{services_start, services_stop, services_stop_all};
    if req.action == Action::Catalog {
        let defs = crate::commands::services::load_services(app.clone()).await?;
        return Ok(Response { ok: true, message: String::new(), projects: Vec::new(), services: catalog_rows(&defs) });
    }
    let message = match req.action {
        Action::Services => String::new(),
        Action::Up | Action::Down => {
            let defs = crate::commands::services::load_services(app.clone()).await?;
            let known: Vec<String> = defs.iter().map(|d| d.id.clone()).collect();
            let mut ids = service_ids(&req.target, &known)?;
            if req.action == Action::Up {
                if ids.is_empty() {
                    ids = load_config(app)?.selected_service_ids;
                }
                if ids.is_empty() {
                    return Err("no services are selected in Kova; name them: kova up mysql redis".to_string());
                }
                services_start(app.clone(), ids.clone()).await?;
                format!("started {}", ids.join(", "))
            } else if ids.is_empty() {
                services_stop_all(app.clone()).await?;
                "stopped every service".to_string()
            } else {
                services_stop(app.clone(), ids.clone()).await?;
                format!("stopped {}", ids.join(", "))
            }
        }
        _ => unreachable!("handle_services is only called for service actions"),
    };
    Ok(Response { ok: true, message, projects: Vec::new(), services: service_rows().await? })
}

async fn handle(app: &AppHandle, req: &Request) -> Response {
    if req.action == Action::Show {
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.unminimize();
            let _ = w.show();
            let _ = w.set_focus();
        }
        return Response { ok: true, message: "shown".to_string(), projects: Vec::new(), services: Vec::new() };
    }
    if matches!(req.action, Action::Up | Action::Down | Action::Services | Action::Catalog) {
        return handle_services(app, req).await.unwrap_or_else(Response::fail);
    }
    let cfg = match load_config(app) {
        Ok(c) => c,
        Err(e) => return Response::fail(e),
    };
    if req.action == Action::Status && req.target.is_empty() {
        return Response { ok: true, message: String::new(), projects: statuses(app, &cfg.projects), services: Vec::new() };
    }
    let project = match find_project(&cfg.projects, &req.target) {
        Ok(p) => p.clone(),
        Err(e) => return Response::fail(e),
    };
    let result = match req.action {
        Action::Start => crate::sites::start_with_services(app, &project).await.map(|_| "started"),
        Action::Restart => crate::sites::restart_with_services(app, &project).await.map(|_| "restarted"),
        Action::Stop => crate::sites::stop_project(app, &project.id).await.map(|_| "stopped"),
        _ => Ok(""),
    };
    match result {
        Ok(what) => Response {
            ok: true,
            message: if what.is_empty() { String::new() } else { format!("{} {}", project.name, what) },
            projects: statuses(app, std::slice::from_ref(&project)),
            services: Vec::new(),
        },
        Err(e) => Response::fail(format!("{}: {}", project.name, e)),
    }
}

/// Parse lalu jalankan; mengembalikan balasan dan apakah klien minta teks.
async fn respond(app: &AppHandle, line: &str, token: &str) -> (Response, bool) {
    match parse_request(line, token) {
        Ok(req) => (handle(app, &req).await, req.plain),
        Err(e) => (Response::fail(e), false),
    }
}

// ---------------------------------------------------------------------------
// Satu instance: Kova yang dibuka kedua kali menyerahkan diri ke yang pertama
// ---------------------------------------------------------------------------

/// Folder data aplikasi tanpa `AppHandle` (dipakai sebelum Tauri dibangun).
/// Sama dengan `app_data_dir()` untuk identifier ini.
fn data_dir_early() -> Option<std::path::PathBuf> {
    const ID: &str = "com.devhardiyanto.servel";
    #[cfg(windows)]
    let base = std::env::var_os("APPDATA").map(std::path::PathBuf::from);
    #[cfg(target_os = "macos")]
    let base = std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join("Library/Application Support"));
    #[cfg(all(unix, not(target_os = "macos")))]
    let base = std::env::var_os("XDG_DATA_HOME")
        .filter(|v| !v.is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".local/share")));
    base.map(|b| b.join(ID))
}

/// Kirim satu permintaan ke Kova di `port`; `true` hanya bila ia menjawab
/// `ok`. Token yang cocok membuktikan penjawabnya memang Kova, bukan program
/// lain yang kebetulan memakai port dari `control.json` lama.
pub fn ask_instance(port: u16, token: &str, request: &str) -> bool {
    use std::io::{BufRead, Write};
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let Ok(mut stream) = std::net::TcpStream::connect_timeout(&addr, Duration::from_millis(700)) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(3)));
    if stream.write_all(format!("{} {}\n", token, request).as_bytes()).is_err() {
        return false;
    }
    let mut line = String::new();
    if std::io::BufReader::new(stream).read_line(&mut line).is_err() {
        return false;
    }
    serde_json::from_str::<serde_json::Value>(&line).map(|v| v["ok"] == true).unwrap_or(false)
}

/// Dipanggil paling awal saat start. `true` = Kova lain sudah berjalan dan
/// sudah diminta menampilkan jendelanya; proses ini harus keluar. `hidden`
/// (start saat login) hanya memeriksa, tanpa memunculkan jendela.
pub fn hand_off_to_running_instance(hidden: bool) -> bool {
    let Some(file) = data_dir_early().map(|d| d.join("control.json")) else { return false };
    let Ok(text) = std::fs::read_to_string(file) else { return false };
    let Ok(info) = serde_json::from_str::<serde_json::Value>(&text) else { return false };
    let (Some(port), Some(token)) = (info["port"].as_u64(), info["token"].as_str()) else { return false };
    let Ok(port) = u16::try_from(port) else { return false };
    ask_instance(port, token, if hidden { "status" } else { "show" })
}

/// Hapus `control.json` sebelum restart setelah update: proses baru dimulai
/// saat proses ini masih hidup sebentar, dan tidak boleh mengira Kova sudah berjalan.
pub fn forget() {
    if let Some(dir) = data_dir_early() {
        let _ = std::fs::remove_file(dir.join("control.json"));
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
    // Token kontrol: hanya pemilik akun yang boleh membacanya (0600 di Unix).
    crate::commands::compose::write_private(&dir.join("control.json"), &info.to_string())?;

    loop {
        let Ok((stream, _)) = listener.accept().await else { continue };
        let app = app.clone();
        let token = token.clone();
        tauri::async_runtime::spawn(async move {
            answer(stream, |line| async move { respond(&app, &line, &token).await }).await;
        });
    }
}

/// Baca satu baris permintaan (dibatasi panjang dan waktunya), balas satu
/// baris JSON (atau teks bila diminta), lalu tutup koneksi.
async fn answer<F, Fut>(stream: tokio::net::TcpStream, respond: F)
where
    F: FnOnce(String) -> Fut,
    Fut: std::future::Future<Output = (Response, bool)>,
{
    let (read, mut write) = stream.into_split();
    let mut line = String::new();
    let mut reader = BufReader::new(read).take(MAX_LINE as u64);
    let got = tokio::time::timeout(Duration::from_secs(5), reader.read_line(&mut line)).await;
    let (response, plain) = match got {
        Ok(Ok(n)) if n > 0 => respond(line).await,
        _ => (Response::fail("empty request".to_string()), false),
    };
    let out = if plain {
        to_plain(&response)
    } else {
        let mut json = serde_json::to_string(&response).unwrap_or_else(|_| "{\"ok\":false}".to_string());
        json.push('\n');
        json
    };
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
            parse_request("abc123 start shop api\n", t),
            Ok(Request { action: Action::Start, target: "shop api".into(), plain: false })
        );
        assert_eq!(
            parse_request("abc123 status", t),
            Ok(Request { action: Action::Status, target: String::new(), plain: false })
        );
        assert_eq!(
            parse_request("abc123 plain up mysql redis", t),
            Ok(Request { action: Action::Up, target: "mysql redis".into(), plain: true })
        );
        assert_eq!(
            parse_request("abc123 plain services", t),
            Ok(Request { action: Action::Services, target: String::new(), plain: true })
        );
        assert_eq!(parse_request("abc123 plain list", t).unwrap().action, Action::Catalog);
        assert_eq!(parse_request("abc123 down", t), Ok(Request { action: Action::Down, target: String::new(), plain: false }));
        // Project bernama "plain" tetap bisa dituju lewat JSON.
        assert_eq!(parse_request("abc123 start plain", t).unwrap().target, "plain");
        assert!(parse_request("abc123 plain", t).unwrap_err().contains("unknown action"));
        assert!(parse_request("abc123 plain stop", t).unwrap_err().contains("project name"));
        assert!(parse_request("abc124 start x", t).unwrap_err().contains("token"));
        assert!(parse_request("abc start x", t).unwrap_err().contains("token"));
        assert!(parse_request("start x", t).unwrap_err().contains("token"));
        assert!(parse_request("abc123 delete x", t).unwrap_err().contains("unknown action"));
        assert!(parse_request("abc123 stop", t).unwrap_err().contains("project name"));
    }

    #[test]
    fn find_project_by_name_or_id() {
        let ps = vec![project("1", "shop-api"), project("2", "Acme"), project("3", "acme")];
        assert_eq!(find_project(&ps, "SHOP-API").unwrap().id, "1");
        assert_eq!(find_project(&ps, "2").unwrap().name, "Acme");
        assert!(find_project(&ps, "acme").unwrap_err().contains("2 projects"));
        assert!(find_project(&ps, "nope").unwrap_err().contains("no project named"));
    }

    #[test]
    fn plain_reply_is_tab_separated_lines() {
        let r = Response {
            ok: true,
            message: "started\nmysql".into(),
            projects: vec![
                ProjectStatus { name: "shop api".into(), state: SiteState::Running, port: 8001, exit_code: None },
                ProjectStatus { name: "lib".into(), state: SiteState::Exited, port: 0, exit_code: Some(1) },
            ],
            services: vec![
                ServiceRow { id: "mysql".into(), state: "running".into(), image: String::new(), ports: String::new() },
                ServiceRow { id: "redis".into(), state: String::new(), image: "redis:7".into(), ports: "6379->6379".into() },
            ],
        };
        assert_eq!(
            to_plain(&r),
            "ok\nstarted mysql\nproject\tshop api\trunning\thttp://localhost:8001\nproject\tlib\texited (1)\t\nservice\tmysql\trunning\ncatalog\tredis\tredis:7\t6379->6379\n"
        );
        assert_eq!(to_plain(&Response::fail("nope".into())), "error\nnope\n");
    }

    #[test]
    fn service_ids_are_validated() {
        let known = vec!["mysql".to_string(), "redis".to_string()];
        assert_eq!(service_ids("MySQL  redis", &known).unwrap(), vec!["mysql", "redis"]);
        assert!(service_ids("", &known).unwrap().is_empty());
        assert!(service_ids("mysql nope", &known).unwrap_err().contains("available: mysql, redis"));
    }

    #[test]
    fn show_is_parsed() {
        assert_eq!(
            parse_request("abc123 show", "abc123"),
            Ok(Request { action: Action::Show, target: String::new(), plain: false })
        );
    }

    /// Instance kedua: `true` hanya bila penjawabnya menerima token dan membalas ok.
    #[test]
    fn ask_instance_needs_an_ok_reply() {
        use std::io::{BufRead, Write};
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            for _ in 0..2 {
                let (stream, _) = listener.accept().unwrap();
                let mut line = String::new();
                std::io::BufReader::new(&stream).read_line(&mut line).unwrap();
                let ok = parse_request(&line, "tok").map(|r| r.action == Action::Show).unwrap_or(false);
                (&stream).write_all(format!("{{\"ok\":{}}}\n", ok).as_bytes()).unwrap();
            }
        });
        assert!(ask_instance(port, "tok", "show"));
        assert!(!ask_instance(port, "wrong", "show"));
        server.join().unwrap();
        // Tidak ada yang mendengarkan (control.json tertinggal dari sesi yang mati).
        assert!(!ask_instance(port, "tok", "show"));
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
                    Ok(r) => (
                        Response {
                            ok: true,
                            message: format!("{:?} {}", r.action, r.target),
                            projects: vec![ProjectStatus { name: r.target, state: SiteState::Running, port: 8001, exit_code: None }],
                            services: Vec::new(),
                        },
                        r.plain,
                    ),
                    Err(e) => (Response::fail(e), false),
                }
            })
            .await;
        });
        let mut client = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        client.write_all(b"tok restart shop-api\n").await.unwrap();
        let mut reply = String::new();
        BufReader::new(&mut client).read_line(&mut reply).await.unwrap();
        server.await.unwrap();
        let v: serde_json::Value = serde_json::from_str(&reply).unwrap();
        assert_eq!(v["ok"], true);
        assert_eq!(v["message"], "Restart shop-api");
        assert_eq!(v["projects"][0]["state"], "running");
        assert_eq!(v["projects"][0]["exitCode"], serde_json::Value::Null);
    }
}
