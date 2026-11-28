//! Jembatan language server untuk Kova Editor (spike 2).
//!
//! Halaman editor memakai `@codemirror/lsp-client`; Rust menjalankan server
//! (stdio), membingkai pesan JSON-RPC, dan mengirim pesan dari server ke
//! halaman lewat `ipc::Channel`. Dua hal sengaja dikerjakan di sini:
//!
//! - Request dari server yang ditolak `lsp-client` (`workspace/configuration`,
//!   `client/registerCapability`, `window/workDoneProgress/create`,
//!   `workspace/workspaceFolders`) dijawab langsung, supaya server tidak macet.
//! - File Vue dilayani dua server: `vue-language-server` (utama) dan `vtsls`
//!   (TypeScript dengan `@vue/typescript-plugin`). Sinkronisasi dokumen
//!   dikirim ke keduanya, `tsserver/request` dari server Vue diteruskan ke
//!   vtsls, diagnostics digabung, dan completion/hover/definition diminta ke
//!   keduanya lalu digabung. CodeMirror hanya melihat satu server.

use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager};
#[cfg(test)]
use tauri::ipc::InvokeResponseBody;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;

use super::EditorRoots;

/// Request dari editor yang di file Vue diminta ke kedua server.
const MERGED: [&str; 3] = ["textDocument/completion", "textDocument/hover", "textDocument/definition"];
/// Notifikasi sinkronisasi dokumen yang dikirim ke kedua server.
const MIRRORED: [&str; 5] = [
    "textDocument/didOpen",
    "textDocument/didChange",
    "textDocument/didClose",
    "textDocument/didSave",
    "workspace/didChangeConfiguration",
];

#[derive(Clone, Copy, PartialEq, Debug)]
enum Side {
    Primary,
    Secondary,
}

/// Request ke server kedua yang menunggu balasan.
enum SecWait {
    Init,
    /// `tsserver/request` dari server Vue: id-nya dikirim balik di `tsserver/response`.
    TsRelay(Value),
    /// Bagian dari request editor yang digabung (id request editor).
    Merge(String),
}

#[derive(Default)]
struct MergeWait {
    id: Value,
    method: String,
    primary: Option<Value>,
    secondary: Option<Value>,
    got_primary: bool,
    got_secondary: bool,
}

struct Proc {
    tx: mpsc::UnboundedSender<Vec<u8>>,
    pid: Option<u32>,
}

struct State {
    primary: Proc,
    secondary: Option<Proc>,
    /// Pengaturan per section untuk `workspace/configuration`.
    settings: Value,
    /// Diagnostics terakhir per URI dari tiap server, untuk digabung.
    diags: HashMap<String, (Vec<Value>, Vec<Value>)>,
    sec_wait: HashMap<String, SecWait>,
    merges: HashMap<String, MergeWait>,
    /// Pesan untuk server kedua sebelum ia selesai `initialize`.
    sec_ready: bool,
    sec_queue: Vec<Value>,
}

pub struct Session {
    window: String,
    root: PathBuf,
    out: Channel<String>,
    /// Baris stderr server (ke Logs Kova di aplikasi).
    log: Arc<dyn Fn(String) + Send + Sync>,
    state: Mutex<State>,
}

#[derive(Default)]
pub struct LspSessions {
    next: AtomicU64,
    map: Mutex<HashMap<u64, Arc<Session>>>,
}

// ---------- membingkai pesan

pub fn frame(body: &[u8]) -> Vec<u8> {
    let mut out = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
    out.extend_from_slice(body);
    out
}

/// Baca satu pesan (header + isi). `None` saat stream selesai.
async fn read_message<R: tokio::io::AsyncBufRead + Unpin>(r: &mut R) -> Option<Vec<u8>> {
    let mut len: Option<usize> = None;
    loop {
        let mut line = String::new();
        if r.read_line(&mut line).await.ok()? == 0 {
            return None;
        }
        let line = line.trim_end();
        if line.is_empty() {
            if len.is_some() {
                break;
            }
            continue;
        }
        if let Some(v) = line.strip_prefix("Content-Length:").or_else(|| line.strip_prefix("content-length:")) {
            len = v.trim().parse().ok();
        }
    }
    let mut buf = vec![0u8; len?];
    r.read_exact(&mut buf).await.ok()?;
    Some(buf)
}

// ---------- menemukan server

/// Cari program di PATH (Windows: ikut PATHEXT, mis. `.cmd` dari npm global).
pub fn which(name: &str) -> Option<PathBuf> {
    crate::commands::util::refresh_windows_path();
    let path = std::env::var_os("PATH")?;
    let exts: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into()).split(';').map(|e| e.to_lowercase()).collect()
    } else {
        vec![String::new()]
    };
    for dir in std::env::split_paths(&path) {
        for ext in &exts {
            let p = dir.join(format!("{}{}", name, ext));
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

/// Folder paket npm global dari lokasi shim-nya (`…\npm\vue-language-server.cmd`
/// → `…\npm\node_modules\@vue\language-server`; Unix: `prefix/bin` → `prefix/lib/node_modules`).
pub fn npm_package_dir(bin: &Path, package: &str) -> Option<PathBuf> {
    let dir = bin.parent()?;
    let candidates = [dir.join("node_modules").join(package), dir.parent()?.join("lib").join("node_modules").join(package)];
    candidates.into_iter().find(|p| p.is_dir())
}

struct Spec {
    program: PathBuf,
    args: Vec<String>,
}

fn find(name: &str, args: &[&str], hint: &str) -> Result<Spec, String> {
    let program = which(name).ok_or_else(|| format!("missing: {} was not found. {}", name, hint))?;
    Ok(Spec { program, args: args.iter().map(|s| s.to_string()).collect() })
}

/// Program untuk tiap server. Pemasangan dari Kova (dengan checksum) datang
/// di MVP; di spike server dipasang sendiri.
fn specs(server: &str) -> Result<(Spec, Option<Spec>, Value), String> {
    let npm = "Install it with npm: npm install -g @vtsls/language-server @vue/language-server";
    match server {
        "php" => Ok((find("phpactor", &["language-server"], "Install Phpactor (phpactor.github.io), then try again.")?, None, json!({}))),
        "typescript" => Ok((find("vtsls", &["--stdio"], npm)?, None, json!({}))),
        "go" => Ok((find("gopls", &[], "Install it with: go install golang.org/x/tools/gopls@latest")?, None, json!({}))),
        "vue" => {
            let vue = find("vue-language-server", &["--stdio"], npm)?;
            let ts = find("vtsls", &["--stdio"], npm)?;
            let plugin = npm_package_dir(&vue.program, "@vue/language-server")
                .ok_or("missing: @vue/language-server was found on PATH but not its package folder")?;
            let settings = json!({
                "vtsls": { "tsserver": { "globalPlugins": [{
                    "name": "@vue/typescript-plugin",
                    "location": super::fs::display_path(&plugin),
                    "languages": ["vue"],
                    "configNamespace": "typescript",
                    "enableForWorkspaceTypeScriptVersions": true
                }]}}
            });
            Ok((vue, Some(ts), settings))
        }
        other => Err(format!("unknown language server {}", other)),
    }
}

// ---------- menjalankan proses

fn spawn(sess: &Arc<Session>, spec: &Spec, side: Side) -> Result<Proc, String> {
    let mut cmd = crate::commands::util::silent_command(&spec.program.to_string_lossy());
    cmd.args(&spec.args).current_dir(&sess.root).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
    #[cfg(unix)]
    cmd.process_group(0);
    let mut child = cmd.spawn().map_err(|e| format!("{}: {}", spec.program.display(), e))?;
    let pid = child.id();
    #[cfg(windows)]
    if let Some(p) = pid {
        // Ikut mati bila Kova dimatikan paksa.
        let _ = crate::sites::process::job::assign(p);
    }
    let mut stdin = child.stdin.take().ok_or("no stdin")?;
    let stdout = child.stdout.take().ok_or("no stdout")?;
    let stderr = child.stderr.take().ok_or("no stderr")?;
    let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();
    tokio::spawn(async move {
        while let Some(m) = rx.recv().await {
            if stdin.write_all(&frame(&m)).await.is_err() || stdin.flush().await.is_err() {
                break;
            }
        }
    });
    let s = sess.clone();
    tokio::spawn(async move {
        let mut r = BufReader::new(stdout);
        while let Some(body) = read_message(&mut r).await {
            if let Ok(v) = serde_json::from_slice::<Value>(&body) {
                s.from_server(side, v);
            }
        }
        if side == Side::Primary {
            let _ = s.out.send(json!({ "jsonrpc": "2.0", "method": "kova/exit", "params": {} }).to_string());
        }
        let _ = child.wait().await;
    });
    let log = sess.log.clone();
    tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(l)) = lines.next_line().await {
            log(l);
        }
    });
    Ok(Proc { tx, pid })
}

impl Session {
    fn send(&self, side: Side, v: &Value) {
        let st = self.state.lock().unwrap();
        let p = match side {
            Side::Primary => Some(&st.primary),
            Side::Secondary => st.secondary.as_ref(),
        };
        if let Some(p) = p {
            let _ = p.tx.send(v.to_string().into_bytes());
        }
    }

    fn to_editor(&self, v: &Value) {
        let _ = self.out.send(v.to_string());
    }

    fn reply(&self, side: Side, id: &Value, result: Value) {
        self.send(side, &json!({ "jsonrpc": "2.0", "id": id, "result": result }));
    }

    /// Jawaban Kova untuk request dari server yang tidak ditangani editor.
    fn answer_server(&self, method: &str, params: &Value) -> Option<Value> {
        match method {
            "client/registerCapability" | "client/unregisterCapability" | "window/workDoneProgress/create" => Some(Value::Null),
            "workspace/workspaceFolders" => {
                let uri = path_uri(&self.root);
                let name = self.root.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                Some(json!([{ "uri": uri, "name": name }]))
            }
            "workspace/configuration" => {
                let st = self.state.lock().unwrap();
                let items = params.get("items").and_then(|i| i.as_array()).cloned().unwrap_or_default();
                Some(Value::Array(items.iter().map(|it| section(&st.settings, it.get("section").and_then(|s| s.as_str()))).collect()))
            }
            _ => None,
        }
    }

    fn from_server(self: &Arc<Self>, side: Side, msg: Value) {
        let method = msg.get("method").and_then(|m| m.as_str()).map(str::to_string);
        let id = msg.get("id").cloned();
        match (method, id) {
            // Request dari server.
            (Some(m), Some(id)) => {
                // Diagnosis: KOVA_LSPDEBUG=1 menulis request server ke Logs.
                if std::env::var_os("KOVA_LSPDEBUG").is_some() {
                    (self.log)(format!("{:?} asks {} {}", side, m, msg.get("params").map(|p| p.to_string()).unwrap_or_default()));
                }
                if let Some(r) = self.answer_server(&m, msg.get("params").unwrap_or(&Value::Null)) {
                    self.reply(side, &id, r);
                } else if side == Side::Primary {
                    self.to_editor(&msg);
                } else {
                    self.send(side, &json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32601, "message": "not handled by Kova" } }));
                }
            }
            // Notifikasi dari server.
            (Some(m), None) => {
                let params = msg.get("params").cloned().unwrap_or(Value::Null);
                if m == "textDocument/publishDiagnostics" && self.has_secondary() {
                    self.merge_diagnostics(side, params);
                } else if m == "tsserver/request" && side == Side::Primary {
                    self.relay_tsserver(params);
                } else if side == Side::Primary {
                    self.to_editor(&msg);
                } else if m == "window/logMessage" || m == "window/showMessage" {
                    // Server kedua tidak terlihat oleh editor: pesannya ke Logs.
                    if let Some(t) = params.get("message").and_then(|t| t.as_str()) {
                        (self.log)(format!("vtsls: {}", t));
                    }
                }
            }
            // Balasan.
            (None, Some(id)) => {
                let key = id.to_string();
                if side == Side::Secondary {
                    let wait = self.state.lock().unwrap().sec_wait.remove(&key);
                    match wait {
                        Some(SecWait::Init) => self.secondary_ready(),
                        Some(SecWait::TsRelay(vue_id)) => {
                            let body = msg.get("result").and_then(|r| r.get("body")).cloned().unwrap_or(Value::Null);
                            self.send(Side::Primary, &json!({ "jsonrpc": "2.0", "method": "tsserver/response", "params": [[vue_id, body]] }));
                        }
                        Some(SecWait::Merge(k)) => self.merge_part(&k, Side::Secondary, msg.get("result").cloned()),
                        None => {}
                    }
                } else if self.state.lock().unwrap().merges.contains_key(&key) {
                    self.merge_part(&key, Side::Primary, msg.get("result").cloned());
                } else {
                    self.to_editor(&msg);
                }
            }
            _ => {}
        }
    }

    fn has_secondary(&self) -> bool {
        self.state.lock().unwrap().secondary.is_some()
    }

    fn merge_diagnostics(&self, side: Side, params: Value) {
        let uri = params.get("uri").and_then(|u| u.as_str()).unwrap_or_default().to_string();
        let list = params.get("diagnostics").and_then(|d| d.as_array()).cloned().unwrap_or_default();
        let all = {
            let mut st = self.state.lock().unwrap();
            let e = st.diags.entry(uri.clone()).or_default();
            match side {
                Side::Primary => e.0 = list,
                Side::Secondary => e.1 = list,
            }
            e.0.iter().chain(e.1.iter()).cloned().collect::<Vec<_>>()
        };
        self.to_editor(&json!({ "jsonrpc": "2.0", "method": "textDocument/publishDiagnostics", "params": { "uri": uri, "diagnostics": all } }));
    }

    /// vue-language-server 3 meminta tsserver lewat klien: `[[id, command, args]]`.
    fn relay_tsserver(&self, params: Value) {
        let items: Vec<Value> = match params.as_array() {
            Some(a) if a.first().is_some_and(|x| x.is_array()) => a.clone(),
            Some(a) => vec![Value::Array(a.clone())],
            None => return,
        };
        for it in items {
            let Some(a) = it.as_array() else { continue };
            let (Some(vue_id), Some(command)) = (a.first().cloned(), a.get(1).cloned()) else { continue };
            let args = a.get(2).cloned().unwrap_or(Value::Null);
            let key = format!("\"kova-ts-{}\"", next_id());
            let id: Value = serde_json::from_str(&key).unwrap();
            self.state.lock().unwrap().sec_wait.insert(key, SecWait::TsRelay(vue_id));
            self.to_secondary(json!({
                "jsonrpc": "2.0", "id": id, "method": "workspace/executeCommand",
                "params": { "command": "typescript.tsserverRequest", "arguments": [command, args, { "isAsync": true, "lowPriority": true }] }
            }));
        }
    }

    /// Kirim ke server kedua, atau antrekan sampai ia siap.
    fn to_secondary(&self, v: Value) {
        let mut st = self.state.lock().unwrap();
        if st.sec_ready {
            if let Some(p) = &st.secondary {
                let _ = p.tx.send(v.to_string().into_bytes());
            }
        } else {
            st.sec_queue.push(v);
        }
    }

    fn secondary_ready(&self) {
        let queue = {
            let mut st = self.state.lock().unwrap();
            st.sec_ready = true;
            std::mem::take(&mut st.sec_queue)
        };
        self.send(Side::Secondary, &json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }));
        let settings = self.state.lock().unwrap().settings.clone();
        self.send(Side::Secondary, &json!({ "jsonrpc": "2.0", "method": "workspace/didChangeConfiguration", "params": { "settings": settings } }));
        for v in queue {
            self.send(Side::Secondary, &v);
        }
    }

    fn merge_part(self: &Arc<Self>, key: &str, side: Side, result: Option<Value>) {
        let done = {
            let mut st = self.state.lock().unwrap();
            let Some(m) = st.merges.get_mut(key) else { return };
            match side {
                Side::Primary => {
                    m.primary = result;
                    m.got_primary = true;
                }
                Side::Secondary => {
                    m.secondary = result;
                    m.got_secondary = true;
                }
            }
            if m.got_primary && m.got_secondary { st.merges.remove(key) } else { None }
        };
        if let Some(m) = done {
            self.finish_merge(m);
        }
    }

    fn finish_merge(&self, m: MergeWait) {
        let result = merge_results(&m.method, m.primary, m.secondary);
        self.to_editor(&json!({ "jsonrpc": "2.0", "id": m.id, "result": result }));
    }

    /// Pesan dari halaman editor.
    fn from_editor(self: &Arc<Self>, msg: Value) {
        if !self.has_secondary() {
            self.send(Side::Primary, &msg);
            return;
        }
        let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or_default().to_string();
        let id = msg.get("id").cloned();
        if method == "initialize" {
            let mut p = msg.get("params").cloned().unwrap_or(Value::Null);
            // vtsls membaca pengaturan (globalPlugins) lewat workspace/configuration,
            // dan hanya memintanya bila klien mengiklankan kemampuan itu.
            p["initializationOptions"] = self.state.lock().unwrap().settings.clone();
            if !p["capabilities"].is_object() {
                p["capabilities"] = json!({});
            }
            if !p["capabilities"]["workspace"].is_object() {
                p["capabilities"]["workspace"] = json!({});
            }
            p["capabilities"]["workspace"]["configuration"] = json!(true);
            let key = format!("\"kova-init-{}\"", next_id());
            let sid: Value = serde_json::from_str(&key).unwrap();
            self.state.lock().unwrap().sec_wait.insert(key, SecWait::Init);
            self.send(Side::Secondary, &json!({ "jsonrpc": "2.0", "id": sid, "method": "initialize", "params": p }));
            self.send(Side::Primary, &msg);
            return;
        }
        if id.is_none() && MIRRORED.contains(&method.as_str()) {
            self.send(Side::Primary, &msg);
            self.to_secondary(msg);
            return;
        }
        if let (Some(id), true) = (id.clone(), MERGED.contains(&method.as_str())) {
            let key = id.to_string();
            let skey = format!("\"kova-m-{}\"", next_id());
            let sid: Value = serde_json::from_str(&skey).unwrap();
            {
                let mut st = self.state.lock().unwrap();
                st.merges.insert(key.clone(), MergeWait { id: id.clone(), method: method.clone(), ..Default::default() });
                st.sec_wait.insert(skey, SecWait::Merge(key.clone()));
            }
            self.send(Side::Primary, &msg);
            let mut sm = msg.clone();
            sm["id"] = sid;
            self.to_secondary(sm);
            // Server yang lambat tidak boleh menahan yang lain: setelah 2 detik
            // kirim yang sudah ada.
            let s = self.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(Duration::from_secs(2)).await;
                let m = s.state.lock().unwrap().merges.remove(&key);
                if let Some(m) = m {
                    s.finish_merge(m);
                }
            });
            return;
        }
        if method == "shutdown" || method == "exit" {
            let mut sm = msg.clone();
            if sm.get("id").is_some() {
                sm["id"] = json!(format!("kova-x-{}", next_id()));
            }
            self.to_secondary(sm);
        }
        self.send(Side::Primary, &msg);
    }

    async fn kill(&self) {
        let pids: Vec<u32> = {
            let st = self.state.lock().unwrap();
            [Some(&st.primary), st.secondary.as_ref()].into_iter().flatten().filter_map(|p| p.pid).collect()
        };
        for pid in pids {
            crate::sites::process::kill_tree(pid).await;
        }
    }
}

fn next_id() -> u64 {
    static N: AtomicU64 = AtomicU64::new(1);
    N.fetch_add(1, Ordering::Relaxed)
}

/// Nilai section bertitik (`vtsls.tsserver`) dari pengaturan, atau null.
fn section(settings: &Value, name: Option<&str>) -> Value {
    let Some(name) = name else { return settings.clone() };
    let mut cur = settings;
    for part in name.split('.') {
        match cur.get(part) {
            Some(v) => cur = v,
            None => return Value::Null,
        }
    }
    cur.clone()
}

fn completion_items(v: Option<Value>) -> (Vec<Value>, bool) {
    match v {
        Some(Value::Array(a)) => (a, false),
        Some(Value::Object(o)) => (
            o.get("items").and_then(|i| i.as_array()).cloned().unwrap_or_default(),
            o.get("isIncomplete").and_then(|b| b.as_bool()).unwrap_or(false),
        ),
        _ => (vec![], false),
    }
}

fn locations(v: Option<Value>) -> Vec<Value> {
    match v {
        Some(Value::Array(a)) => a,
        Some(Value::Null) | None => vec![],
        Some(o) => vec![o],
    }
}

/// Gabungkan jawaban dua server untuk satu request editor.
pub fn merge_results(method: &str, primary: Option<Value>, secondary: Option<Value>) -> Value {
    match method {
        "textDocument/completion" => {
            let (mut a, ia) = completion_items(primary);
            let (b, ib) = completion_items(secondary);
            a.extend(b);
            json!({ "isIncomplete": ia || ib, "items": a })
        }
        "textDocument/definition" => {
            let mut a = locations(primary);
            a.extend(locations(secondary));
            Value::Array(a)
        }
        _ => match primary {
            Some(v) if !v.is_null() => v,
            _ => secondary.unwrap_or(Value::Null),
        },
    }
}

/// `file:///C:/dev/x` dari path Windows atau Unix.
pub fn path_uri(p: &Path) -> String {
    let s = super::fs::display_path(p).replace('\\', "/");
    let s = if s.starts_with('/') { s } else { format!("/{}", s) };
    let mut out = String::from("file://");
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => out.push(b as char),
            b':' if out.len() <= 10 => out.push(':'),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

// ---------- perintah

/// Mulai server untuk jendela ini. Pesan dari server dikirim ke `on_message`.
#[tauri::command]
pub async fn lsp_start(
    app: AppHandle,
    window: tauri::WebviewWindow,
    roots: tauri::State<'_, EditorRoots>,
    sessions: tauri::State<'_, LspSessions>,
    server: String,
    on_message: Channel<String>,
) -> Result<u64, String> {
    let root = roots.0.lock().unwrap().get(window.label()).cloned().ok_or("this window has no folder")?;
    let tag = format!("LSP {}", server);
    let log_app = app.clone();
    let log: Arc<dyn Fn(String) + Send + Sync> = Arc::new(move |l: String| crate::commands::util::emit_log_line(&log_app, &tag, &l));
    let sess = start_session(window.label(), &server, root, on_message, log).await?;
    crate::commands::util::emit_log_line(&app, &format!("LSP {}", server), &format!("started for {}", super::fs::display_path(&sess.root)));
    let id = sessions.next.fetch_add(1, Ordering::Relaxed) + 1;
    sessions.map.lock().unwrap().insert(id, sess);
    Ok(id)
}

async fn start_session(window: &str, server: &str, root: PathBuf, out: Channel<String>, log: Arc<dyn Fn(String) + Send + Sync>) -> Result<Arc<Session>, String> {
    let (primary, secondary, settings) = specs(server)?;
    let (dummy, _) = mpsc::unbounded_channel();
    let sess = Arc::new(Session {
        window: window.to_string(),
        root,
        out,
        log,
        state: Mutex::new(State {
            primary: Proc { tx: dummy, pid: None },
            secondary: None,
            settings,
            diags: HashMap::new(),
            sec_wait: HashMap::new(),
            merges: HashMap::new(),
            sec_ready: false,
            sec_queue: Vec::new(),
        }),
    });
    let p = spawn(&sess, &primary, Side::Primary)?;
    sess.state.lock().unwrap().primary = p;
    if let Some(sec) = secondary {
        match spawn(&sess, &sec, Side::Secondary) {
            Ok(p) => sess.state.lock().unwrap().secondary = Some(p),
            Err(e) => {
                sess.kill().await;
                return Err(e);
            }
        }
    }
    Ok(sess)
}

/// Pesan JSON dari `lsp-client` ke server.
#[tauri::command]
pub fn lsp_send(window: tauri::WebviewWindow, sessions: tauri::State<'_, LspSessions>, id: u64, message: String) -> Result<(), String> {
    let sess = sessions.map.lock().unwrap().get(&id).cloned().ok_or("language server is not running")?;
    if sess.window != window.label() {
        return Err("not your language server".into());
    }
    let v: Value = serde_json::from_str(&message).map_err(|e| e.to_string())?;
    sess.from_editor(v);
    Ok(())
}

#[tauri::command]
pub async fn lsp_stop(window: tauri::WebviewWindow, sessions: tauri::State<'_, LspSessions>, id: u64) -> Result<(), String> {
    let sess = {
        let mut map = sessions.map.lock().unwrap();
        match map.get(&id) {
            Some(s) if s.window == window.label() => map.remove(&id),
            _ => None,
        }
    };
    if let Some(s) = sess {
        s.kill().await;
    }
    Ok(())
}

/// Hentikan semua server milik jendela yang ditutup.
pub fn stop_window(app: &AppHandle, label: &str) {
    let Some(sessions) = app.try_state::<LspSessions>() else { return };
    let gone: Vec<Arc<Session>> = {
        let mut map = sessions.map.lock().unwrap();
        let ids: Vec<u64> = map.iter().filter(|(_, s)| s.window == label).map(|(k, _)| *k).collect();
        ids.into_iter().filter_map(|k| map.remove(&k)).collect()
    };
    if gone.is_empty() {
        return;
    }
    tauri::async_runtime::spawn(async move {
        for s in gone {
            s.kill().await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn frames_round_trip() {
        let mut data = frame(br#"{"a":1}"#);
        data.extend(frame("{\"b\":\"é\"}".as_bytes()));
        let mut r = BufReader::new(&data[..]);
        assert_eq!(read_message(&mut r).await.unwrap(), br#"{"a":1}"#);
        assert_eq!(String::from_utf8(read_message(&mut r).await.unwrap()).unwrap(), "{\"b\":\"é\"}");
        assert!(read_message(&mut r).await.is_none());
    }

    #[test]
    fn merges_completion_definition_and_hover() {
        let c = merge_results("textDocument/completion", Some(json!([{ "label": "a" }])), Some(json!({ "isIncomplete": true, "items": [{ "label": "b" }] })));
        assert_eq!(c, json!({ "isIncomplete": true, "items": [{ "label": "a" }, { "label": "b" }] }));
        let d = merge_results("textDocument/definition", Some(json!({ "uri": "x" })), Some(Value::Null));
        assert_eq!(d, json!([{ "uri": "x" }]));
        let h = merge_results("textDocument/hover", Some(Value::Null), Some(json!({ "contents": "t" })));
        assert_eq!(h, json!({ "contents": "t" }));
    }

    #[test]
    fn section_lookup() {
        let s = json!({ "vtsls": { "tsserver": { "x": 1 } } });
        assert_eq!(section(&s, Some("vtsls.tsserver")), json!({ "x": 1 }));
        assert_eq!(section(&s, Some("php")), Value::Null);
    }

    #[test]
    fn uri_from_windows_and_unix_paths() {
        assert_eq!(path_uri(Path::new(r"C:\dev\acme api")), "file:///C:/dev/acme%20api");
        assert_eq!(path_uri(Path::new("/home/dev/x")), "file:///home/dev/x");
    }

    #[test]
    fn npm_package_dir_next_to_shim() {
        let dir = std::env::temp_dir().join(format!("kova-npm-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("node_modules/@vue/language-server")).unwrap();
        let bin = dir.join("vue-language-server.cmd");
        std::fs::write(&bin, "").unwrap();
        assert_eq!(npm_package_dir(&bin, "@vue/language-server").unwrap(), dir.join("node_modules/@vue/language-server"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Uji nyata dengan vue-language-server dan vtsls dari npm global.
    /// Jalankan manual: `cargo test vue_hybrid -- --ignored --nocapture`.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    async fn vue_hybrid_end_to_end() {
        // KOVA_VUE_FIXTURE: folder dengan `npm install vue` supaya tipe Vue ada.
        let dir = std::env::var_os("KOVA_VUE_FIXTURE").map(PathBuf::from).unwrap_or_else(|| std::env::temp_dir().join(format!("kova-vue-{}", std::process::id())));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        if !dir.join("package.json").exists() {
            std::fs::write(dir.join("package.json"), r#"{"name":"t","private":true}"#).unwrap();
        }
        std::fs::write(dir.join("tsconfig.json"), r#"{"compilerOptions":{"strict":true,"target":"ES2022","module":"ESNext","moduleResolution":"Bundler"},"include":["src/**/*"]}"#).unwrap();
        let text = "<script setup lang=\"ts\">\nconst count: number = 'x'\nconst label = 'total'\n</script>\n\n<template>\n  <p>{{ label.toUpperCase() }} {{ count }}</p>\n</template>\n";
        std::fs::write(dir.join("src/App.vue"), text).unwrap();
        let root = std::fs::canonicalize(&dir).unwrap();
        let got: Arc<Mutex<Vec<Value>>> = Arc::default();
        let g = got.clone();
        let out = Channel::<String>::new(move |b| {
            if let InvokeResponseBody::Json(s) = b {
                if let Ok(inner) = serde_json::from_str::<String>(&s) {
                    if let Ok(v) = serde_json::from_str::<Value>(&inner) {
                        g.lock().unwrap().push(v);
                    }
                }
            }
            Ok(())
        });
        let sess = start_session("t", "vue", root.clone(), out, Arc::new(|l| eprintln!("[stderr] {}", l))).await.expect("servers on PATH");
        let wait = |pred: Box<dyn Fn(&Value) -> bool + Send>, secs: u64| {
            let got = got.clone();
            async move {
                for _ in 0..secs * 10 {
                    if let Some(v) = got.lock().unwrap().iter().find(|v| pred(v)).cloned() {
                        return Some(v);
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                None
            }
        };
        let root_uri = path_uri(&root);
        sess.from_editor(json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "processId": null, "rootUri": root_uri, "workspaceFolders": [{ "uri": root_uri, "name": "t" }],
            "capabilities": { "textDocument": { "completion": { "completionItem": { "snippetSupport": true } }, "hover": { "contentFormat": ["markdown", "plaintext"] }, "publishDiagnostics": {} } }
        }}));
        assert!(wait(Box::new(|v| v["id"] == 1 && v.get("result").is_some()), 30).await.is_some(), "initialize");
        sess.from_editor(json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }));
        let uri = path_uri(&root.join("src/App.vue"));
        sess.from_editor(json!({ "jsonrpc": "2.0", "method": "textDocument/didOpen", "params": { "textDocument": { "uri": uri, "languageId": "vue", "version": 0, "text": text } } }));
        let u = uri.clone();
        let diag = wait(Box::new(move |v| v["method"] == "textDocument/publishDiagnostics" && v["params"]["uri"] == u.as_str()
            && v["params"]["diagnostics"].as_array().is_some_and(|d| d.iter().any(|x| x["code"] == 2322))), 60).await;
        eprintln!("diagnostics: {}", diag.as_ref().map(|d| d["params"]["diagnostics"].to_string()).unwrap_or_default());
        // Completion di template setelah `label.` (baris 6, kolom 14).
        sess.from_editor(json!({ "jsonrpc": "2.0", "id": 2, "method": "textDocument/completion", "params": { "textDocument": { "uri": uri }, "position": { "line": 6, "character": 14 } } }));
        let comp = wait(Box::new(|v| v["id"] == 2), 30).await;
        let labels: Vec<String> = comp.as_ref().and_then(|c| c["result"]["items"].as_array().cloned()).unwrap_or_default().iter().filter_map(|i| i["label"].as_str().map(str::to_string)).collect();
        eprintln!("completion: {} items, has toUpperCase: {}", labels.len(), labels.iter().any(|l| l == "toUpperCase"));
        for v in got.lock().unwrap().iter() { eprintln!("[got] {}", v.to_string().chars().take(300).collect::<String>()); }
        sess.kill().await;
        if std::env::var_os("KOVA_VUE_FIXTURE").is_none() {
            let _ = std::fs::remove_dir_all(&dir);
        }
        assert!(diag.is_some(), "TS error 2322 from vtsls in the .vue file");
        assert!(labels.iter().any(|l| l == "toUpperCase"), "template completion through the tsserver relay");
    }

    /// Jalur satu server (sama dengan Phpactor dan gopls) dengan vtsls.
    /// Jalankan manual: `cargo test single_server -- --ignored --nocapture`.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    async fn single_server_end_to_end() {
        let dir = std::env::temp_dir().join(format!("kova-ts-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("tsconfig.json"), r#"{"compilerOptions":{"strict":true}}"#).unwrap();
        let text = "const n: number = 'x'\nconst s = 'abc'\ns.\n";
        std::fs::write(dir.join("a.ts"), text).unwrap();
        let root = std::fs::canonicalize(&dir).unwrap();
        let got: Arc<Mutex<Vec<Value>>> = Arc::default();
        let g = got.clone();
        let out = Channel::<String>::new(move |b| {
            if let InvokeResponseBody::Json(s) = b {
                if let Some(v) = serde_json::from_str::<String>(&s).ok().and_then(|i| serde_json::from_str::<Value>(&i).ok()) {
                    g.lock().unwrap().push(v);
                }
            }
            Ok(())
        });
        let sess = start_session("t", "typescript", root.clone(), out, Arc::new(|l| eprintln!("[stderr] {}", l))).await.expect("vtsls on PATH");
        let find = |pred: &dyn Fn(&Value) -> bool| got.lock().unwrap().iter().find(|v| pred(v)).cloned();
        let root_uri = path_uri(&root);
        sess.from_editor(json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "processId": null, "rootUri": root_uri, "capabilities": { "textDocument": { "publishDiagnostics": {}, "completion": {} } } } }));
        let uri = path_uri(&root.join("a.ts"));
        let mut init = false;
        for _ in 0..300 {
            if !init && find(&|v| v["id"] == 1).is_some() {
                init = true;
                sess.from_editor(json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }));
                sess.from_editor(json!({ "jsonrpc": "2.0", "method": "textDocument/didOpen", "params": { "textDocument": { "uri": uri, "languageId": "typescript", "version": 0, "text": text } } }));
                sess.from_editor(json!({ "jsonrpc": "2.0", "id": 2, "method": "textDocument/completion", "params": { "textDocument": { "uri": uri }, "position": { "line": 2, "character": 2 } } }));
            }
            if init && find(&|v| v["id"] == 2).is_some() && find(&|v| v["method"] == "textDocument/publishDiagnostics" && v["params"]["diagnostics"].as_array().is_some_and(|d| !d.is_empty())).is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let diag = find(&|v| v["method"] == "textDocument/publishDiagnostics" && v["params"]["diagnostics"].as_array().is_some_and(|d| d.iter().any(|x| x["code"] == 2322)));
        let comp = find(&|v| v["id"] == 2);
        let has = comp.as_ref().and_then(|c| c["result"]["items"].as_array().cloned()).unwrap_or_default().iter().any(|i| i["label"] == "toUpperCase");
        sess.kill().await;
        let _ = std::fs::remove_dir_all(&dir);
        assert!(diag.is_some(), "diagnostics passed through");
        assert!(has, "completion passed through");
    }
}
