//! Program yang memakai sebuah port, dan menghentikannya atas permintaan
//! pengguna (setelah konfirmasi di UI).
//!
//! - Windows: `Get-NetTCPConnection` + `Win32_Process` (satu panggilan PowerShell).
//! - Linux: `ss -ltnpH` lalu `/proc/<pid>`.
//! - macOS: `lsof -iTCP -sTCP:LISTEN`, lalu `ps` untuk command.
//!
//! Proses milik pengguna lain tidak terlihat tanpa hak admin; hasilnya `None`
//! dan UI hanya menawarkan port lain.

use serde::{Deserialize, Serialize};

use crate::commands::util::silent_command;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PortOwner {
    pub pid: u32,
    pub name: String,
    pub path: Option<String>,
    pub command: Option<String>,
    /// Waktu mulai menurut OS, supaya Stop tidak mengenai PID yang sudah dipakai ulang.
    pub started: Option<String>,
}

/// `ss -ltnpH`: `LISTEN 0 511 *:3001 *:* users:(("node",pid=1234,fd=21))`.
#[allow(dead_code)] // per OS + tests
pub fn parse_ss(out: &str) -> Option<(u32, String)> {
    let line = out.lines().find(|l| l.contains("pid="))?;
    let name = line.split("((\"").nth(1)?.split('"').next()?.to_string();
    let pid = line.split("pid=").nth(1)?.chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse().ok()?;
    Some((pid, name))
}

/// `lsof -Fpc`: baris `p1234` lalu `cnode`.
#[allow(dead_code)] // per OS + tests
pub fn parse_lsof(out: &str) -> Option<(u32, String)> {
    let mut pid = None;
    let mut name = None;
    for l in out.lines() {
        if let Some(p) = l.strip_prefix('p') {
            if pid.is_some() {
                break;
            }
            pid = p.trim().parse().ok();
        } else if let Some(c) = l.strip_prefix('c') {
            name.get_or_insert_with(|| c.trim().to_string());
        }
    }
    Some((pid?, name.unwrap_or_default()))
}

/// JSON dari script PowerShell di `owner_windows`.
#[allow(dead_code)] // per OS + tests
pub fn parse_ps_json(out: &str) -> Option<PortOwner> {
    let v: serde_json::Value = serde_json::from_str(out.trim()).ok()?;
    let s = |k: &str| v.get(k).and_then(|x| x.as_str()).filter(|x| !x.is_empty()).map(str::to_string);
    Some(PortOwner {
        pid: v.get("pid")?.as_u64()? as u32,
        name: s("name").unwrap_or_else(|| "a program".into()),
        path: s("path"),
        command: s("command"),
        started: None,
    })
}

async fn run(program: &str, args: &[&str]) -> Option<String> {
    let out = silent_command(program).args(args).output().await.ok()?;
    Some(String::from_utf8_lossy(&out.stdout).to_string())
}

#[cfg(target_os = "windows")]
async fn owner_os(port: u16) -> Option<PortOwner> {
    let script = format!(
        "$c = Get-NetTCPConnection -LocalPort {port} -State Listen -ErrorAction SilentlyContinue | Select-Object -First 1; \
         if ($c) {{ $p = Get-CimInstance Win32_Process -Filter \"ProcessId=$($c.OwningProcess)\"; \
         [pscustomobject]@{{pid=[int]$c.OwningProcess; name=$p.Name; path=$p.ExecutablePath; command=$p.CommandLine}} | ConvertTo-Json -Compress }}"
    );
    let out = run("powershell.exe", &["-NoProfile", "-NonInteractive", "-Command", &script]).await?;
    parse_ps_json(&out)
}

#[cfg(target_os = "linux")]
async fn owner_os(port: u16) -> Option<PortOwner> {
    let filter = format!("sport = :{}", port);
    let out = run("ss", &["-ltnpH", &filter]).await?;
    let (pid, name) = parse_ss(&out)?;
    let base = format!("/proc/{}", pid);
    let command = std::fs::read(format!("{}/cmdline", base))
        .ok()
        .map(|b| String::from_utf8_lossy(&b).split('\0').filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" "))
        .filter(|s| !s.is_empty());
    let path = std::fs::read_link(format!("{}/exe", base)).ok().map(|p| p.display().to_string());
    Some(PortOwner { pid, name, path, command, started: None })
}

#[cfg(target_os = "macos")]
async fn owner_os(port: u16) -> Option<PortOwner> {
    let spec = format!("-iTCP:{}", port);
    let out = run("lsof", &["-nP", &spec, "-sTCP:LISTEN", "-Fpc"]).await?;
    let (pid, name) = parse_lsof(&out)?;
    let command = run("ps", &["-o", "command=", "-p", &pid.to_string()]).await.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    Some(PortOwner { pid, name, path: None, command, started: None })
}

pub async fn owner(port: u16) -> Option<PortOwner> {
    let mut o = owner_os(port).await?;
    o.started = crate::sites::process::process_started(o.pid).await;
    Some(o)
}

#[tauri::command]
pub async fn port_owner(port: u16) -> Option<PortOwner> {
    owner(port).await
}

/// Hentikan program pemilik port. `started` harus sama dengan saat ditampilkan:
/// kalau PID sudah dipakai proses lain, tidak ada yang dihentikan.
#[tauri::command]
pub async fn port_owner_stop(pid: u32, started: Option<String>) -> Result<(), String> {
    if pid <= 4 || pid == std::process::id() {
        return Err("Kova will not stop this process".into());
    }
    let now = crate::sites::process::process_started(pid).await;
    if now.is_none() {
        return Ok(()); // sudah berhenti
    }
    if started.is_some() && now != started {
        return Err("that program already stopped; another one now has its process id".into());
    }
    #[cfg(target_os = "windows")]
    {
        let out = silent_command("taskkill").args(["/PID", &pid.to_string(), "/T", "/F"]).output().await.map_err(|e| e.to_string())?;
        if !out.status.success() {
            let msg = String::from_utf8_lossy(&out.stderr).trim().to_string();
            return Err(if msg.is_empty() { "could not stop it (it may need Administrator)".into() } else { msg });
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let p = pid.to_string();
        let _ = silent_command("kill").args(["-TERM", &p]).status().await;
        for _ in 0..30 {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            if crate::sites::process::process_started(pid).await.is_none() {
                return Ok(());
            }
        }
        let st = silent_command("kill").args(["-KILL", &p]).status().await.map_err(|e| e.to_string())?;
        if !st.success() {
            return Err("could not stop it (it may belong to another user)".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ss_lsof_and_powershell() {
        assert_eq!(
            parse_ss("LISTEN 0 511 *:3001 *:* users:((\"node\",pid=18244,fd=21))\n"),
            Some((18244, "node".into()))
        );
        assert_eq!(parse_ss("LISTEN 0 511 *:3001 *:*\n"), None);
        assert_eq!(parse_lsof("p4120\ncmysqld\nf12\n"), Some((4120, "mysqld".into())));
        let o = parse_ps_json(r#"{"pid":18244,"name":"node.exe","path":"C:\\nodejs\\node.exe","command":"node server.js"}"#).unwrap();
        assert_eq!((o.pid, o.name.as_str(), o.command.as_deref()), (18244, "node.exe", Some("node server.js")));
        assert!(parse_ps_json("").is_none());
    }
}
