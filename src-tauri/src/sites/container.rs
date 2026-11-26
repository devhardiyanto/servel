//! Project mode container (Run with: Dockerfile / Compose).
//!
//! Kova menulis script sementara (PowerShell di Windows, sh di tempat lain)
//! yang menjalankan langkah engine satu per satu, lalu menjalankannya sebagai
//! proses project biasa: log, status, Stop, dan restart memakai mesin yang
//! sama dengan mode Command. Setiap argumen dikutip sendiri-sendiri; nilai
//! ENV tidak pernah ditulis ke script, hanya namanya (`-e NAMA`), karena engine
//! membaca nilainya dari environment proses yang dipasang Kova.
//!
//! Setelah proses selesai (Stop atau berhenti sendiri) container dihapus
//! (`rm -f` / `compose down`), supaya Start berikutnya tidak bentrok nama/port.

use std::path::{Path, PathBuf};

use crate::commands::config::{Project, RunMode};
#[cfg(test)]
use crate::commands::config::ContainerSpec;
use crate::open_in::{ps_quote, sh_quote};
use crate::runtime::RuntimeKind;

/// Network service Kova di Docker (lihat `commands::compose`).
pub const SERVICES_NETWORK: &str = "servel_default";

/// Langkah pembersihan setelah proses project selesai.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cleanup {
    Remove { engine: &'static str, name: String },
    ComposeDown { args: Vec<String> },
}

#[derive(Debug, Clone)]
pub struct Plan {
    pub program: String,
    pub args: Vec<String>,
    /// Ringkasan untuk log (`$ …`).
    pub summary: String,
    pub cleanup: Cleanup,
}

/// Nama aman untuk image/container: huruf kecil, angka, `-`.
pub fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let t = out.trim_matches('-').chars().take(40).collect::<String>();
    if t.is_empty() { "project".into() } else { t }
}

fn engine_bin(kind: RuntimeKind) -> &'static str {
    match kind {
        RuntimeKind::Docker => "docker",
        RuntimeKind::Wslc => "wslc",
    }
}

/// Langkah-langkah (argv per langkah). Langkah pertama boleh gagal (bersih-bersih).
pub fn steps(site: &Project, kind: RuntimeKind, env_names: &[String]) -> Result<(Vec<Vec<String>>, Cleanup), String> {
    let spec = site.container.clone().unwrap_or_default();
    let base = instance_slug(site);
    let name = format!("kova-{base}");
    let bin = engine_bin(kind);
    let owned = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<String>>();
    match site.run_mode {
        RunMode::Command => Err("this project runs a command, not a container".into()),
        RunMode::Dockerfile => {
            let image = format!("kova/{base}");
            let dockerfile = if spec.dockerfile.trim().is_empty() { "Dockerfile".to_string() } else { spec.dockerfile.trim().to_string() };
            if !Path::new(&site.path).join(&dockerfile).is_file() {
                return Err(format!("{} was not found in the project folder", dockerfile));
            }
            let cport = if spec.container_port == 0 { site.port } else { spec.container_port };
            let rm = match kind {
                RuntimeKind::Docker => owned(&[bin, "rm", "-f", &name]),
                RuntimeKind::Wslc => owned(&[bin, "remove", "-f", &name]),
            };
            let mut build = owned(&[bin, "build", "-t", &image]);
            if dockerfile != "Dockerfile" {
                build.push("-f".into());
                build.push(Path::new(&site.path).join(&dockerfile).display().to_string());
            }
            build.push(site.path.clone());
            let mut run = owned(&[bin, "run", "--rm", "--name", &name]);
            if kind == RuntimeKind::Docker && spec.join_services {
                run.push("--network".into());
                run.push(SERVICES_NETWORK.into());
            }
            run.push("-p".into());
            run.push(format!("127.0.0.1:{}:{}", site.port, cport));
            for n in env_names {
                run.push("-e".into());
                run.push(n.clone());
            }
            run.push(image);
            Ok((vec![rm, build, run], Cleanup::Remove { engine: bin, name }))
        }
        RunMode::Compose => {
            if kind != RuntimeKind::Docker {
                return Err("Compose needs the Docker engine; WSL containers has no compose yet. Switch the engine in Settings, or use Run with: Dockerfile.".into());
            }
            let file = if spec.compose_file.trim().is_empty() { default_compose_file(&site.path) } else { spec.compose_file.trim().to_string() };
            if !Path::new(&site.path).join(&file).is_file() {
                return Err(format!("{} was not found in the project folder", file));
            }
            let mut base = owned(&[bin, "compose", "-f"]);
            base.push(Path::new(&site.path).join(&file).display().to_string());
            if spec.join_services {
                base.push("-f".into());
                base.push(join_override()?.display().to_string());
            }
            base.push("-p".into());
            base.push(name.clone());
            let mut up = base.clone();
            up.extend(owned(&["up", "--build"]));
            for s in spec.skip_compose_services.iter().filter(|s| valid_service_name(s)) {
                up.push("--scale".into());
                up.push(format!("{}=0", s));
            }
            let mut down: Vec<String> = base[1..].to_vec();
            down.push("down".into());
            Ok((vec![up], Cleanup::ComposeDown { args: down }))
        }
    }
}

fn valid_service_name(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// `compose.yaml`, `compose.yml`, `docker-compose.yml`, `docker-compose.yaml`.
pub fn default_compose_file(dir: &str) -> String {
    for f in ["compose.yaml", "compose.yml", "docker-compose.yml", "docker-compose.yaml"] {
        if Path::new(dir).join(f).is_file() {
            return f.to_string();
        }
    }
    "compose.yaml".into()
}

/// Slug nama ditambah 8 karakter id project, supaya dua project bernama mirip
/// ("Acme API" dan "acme-api") tidak berbagi container, image, atau project compose.
pub fn instance_slug(site: &Project) -> String {
    let short: String = site.id.chars().filter(|c| c.is_ascii_alphanumeric()).take(8).collect::<String>().to_lowercase();
    if short.is_empty() { slug(&site.name) } else { format!("{}-{short}", slug(&site.name)) }
}

/// Override compose: network default project = network service Kova.
fn join_override() -> Result<PathBuf, String> {
    let p = crate::commands::compose::private_dir().join("compose-join.yaml");
    let body = format!("networks:\n  default:\n    name: {}\n    external: true\n", SERVICES_NETWORK);
    crate::commands::compose::write_private(&p, &body)?;
    Ok(p)
}

/// Script PowerShell: tiap langkah dicetak lalu dijalankan; berhenti di kode
/// keluar pertama yang bukan 0 (kecuali langkah bersih-bersih).
pub fn ps_script(steps: &[Vec<String>], first_may_fail: bool) -> String {
    let mut out = String::from("Remove-Item -LiteralPath $PSCommandPath -ErrorAction SilentlyContinue\n");
    for (i, st) in steps.iter().enumerate() {
        let quoted: Vec<String> = st.iter().map(|a| ps_quote(a)).collect();
        if i == 0 && first_may_fail {
            out.push_str(&format!("& {} *> $null\n", quoted.join(" ")));
            continue;
        }
        out.push_str(&format!("Write-Output {}\n", ps_quote(&format!("$ {}", st.join(" ")))));
        out.push_str(&format!("& {}\n", quoted.join(" ")));
        out.push_str("if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }\n");
    }
    out.push_str("exit 0\n");
    out
}

pub fn sh_script(steps: &[Vec<String>], first_may_fail: bool) -> String {
    let mut out = String::from("#!/bin/sh\nrm -f -- \"$0\"\n");
    for (i, st) in steps.iter().enumerate() {
        let quoted: Vec<String> = st.iter().map(|a| sh_quote(a)).collect();
        if i == 0 && first_may_fail {
            out.push_str(&format!("{} >/dev/null 2>&1\n", quoted.join(" ")));
            continue;
        }
        out.push_str(&format!("printf '%s\\n' {}\n", sh_quote(&format!("$ {}", st.join(" ")))));
        out.push_str(&format!("{} || exit $?\n", quoted.join(" ")));
    }
    out
}

/// Rencana lengkap: tulis script, kembalikan program + argumen untuk proses project.
pub fn plan(site: &Project, kind: RuntimeKind, env_names: &[String]) -> Result<Plan, String> {
    let (steps, cleanup) = steps(site, kind, env_names)?;
    let first_may_fail = site.run_mode == RunMode::Dockerfile;
    let summary = steps.iter().skip(usize::from(first_may_fail)).map(|s| s.join(" ")).collect::<Vec<_>>().join(" && ");
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    if cfg!(windows) {
        let path = std::env::temp_dir().join(format!("kova-run-{}.ps1", nanos));
        std::fs::write(&path, ps_script(&steps, first_may_fail)).map_err(|e| format!("could not write the run script: {}", e))?;
        Ok(Plan {
            program: "powershell.exe".into(),
            args: vec!["-NoProfile".into(), "-NonInteractive".into(), "-ExecutionPolicy".into(), "Bypass".into(), "-File".into(), path.display().to_string()],
            summary,
            cleanup,
        })
    } else {
        let path = std::env::temp_dir().join(format!("kova-run-{}.sh", nanos));
        std::fs::write(&path, sh_script(&steps, first_may_fail)).map_err(|e| format!("could not write the run script: {}", e))?;
        Ok(Plan { program: "sh".into(), args: vec![path.display().to_string()], summary, cleanup })
    }
}

/// Jalankan pembersihan (diabaikan bila gagal: container mungkin sudah hilang).
pub async fn cleanup(c: &Cleanup) {
    use crate::commands::util::silent_command;
    match c {
        Cleanup::Remove { engine, name } => {
            let sub = if *engine == "wslc" { "remove" } else { "rm" };
            let _ = silent_command(engine).args([sub, "-f", name]).output().await;
        }
        Cleanup::ComposeDown { args } => {
            let _ = silent_command("docker").args(args).output().await;
        }
    }
}

/// Isi folder yang relevan untuk Project settings › Run with.
#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ContainerInfo {
    pub dockerfile: bool,
    pub compose_file: Option<String>,
    pub compose_services: Vec<String>,
    /// Port pertama dari `EXPOSE` di Dockerfile, untuk "Port in the container".
    pub expose_port: Option<u16>,
}

/// Port pertama di baris `EXPOSE` (mis. `EXPOSE 8080/tcp 9000` → 8080).
/// Variabel (`EXPOSE $PORT`) dilewati karena nilainya baru ada saat build.
pub fn dockerfile_expose(text: &str) -> Option<u16> {
    text.lines().find_map(|l| {
        let t = l.trim();
        let (kw, rest) = t.split_once(char::is_whitespace)?;
        if !kw.eq_ignore_ascii_case("EXPOSE") {
            return None;
        }
        rest.split_whitespace().find_map(|p| p.split('/').next()?.parse::<u16>().ok().filter(|n| *n > 0))
    })
}

/// Nama service tingkat atas di compose (`services:` lalu kunci berindentasi
/// satu tingkat). Cukup untuk menampilkan pilihan; bukan parser YAML lengkap.
pub fn compose_services(yaml: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut inside = false;
    let mut indent: Option<usize> = None;
    for line in yaml.lines() {
        let t = line.trim_end();
        if t.trim().is_empty() || t.trim_start().starts_with('#') {
            continue;
        }
        let lead = t.len() - t.trim_start().len();
        if lead == 0 {
            inside = t.starts_with("services:");
            indent = None;
            continue;
        }
        if !inside {
            continue;
        }
        let ind = *indent.get_or_insert(lead);
        if lead == ind {
            if let Some(name) = t.trim().strip_suffix(':') {
                let name = name.trim_matches(|c| c == '"' || c == '\'');
                if valid_service_name(name) {
                    out.push(name.to_string());
                }
            }
        }
    }
    out
}

#[tauri::command]
pub fn project_container_info(path: String) -> ContainerInfo {
    let dir = Path::new(&path);
    let file = default_compose_file(&path);
    let compose = dir.join(&file).is_file().then_some(file);
    let compose_services = compose
        .as_ref()
        .and_then(|f| std::fs::read_to_string(dir.join(f)).ok())
        .map(|y| compose_services(&y))
        .unwrap_or_default();
    let dockerfile = dir.join("Dockerfile");
    let expose_port = std::fs::read_to_string(&dockerfile).ok().and_then(|t| dockerfile_expose(&t));
    ContainerInfo { dockerfile: dockerfile.is_file(), compose_file: compose, compose_services, expose_port }
}

/// Satu container project compose (canvas K7).
#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ComposeContainer {
    pub service: String,
    pub state: String,
    /// Port di PC yang dipetakan ke container, bila ada.
    pub ports: Vec<u16>,
}

/// `docker compose ps --format json`: array JSON (Compose lama) atau satu objek per baris.
pub fn parse_compose_ps(out: &str) -> Vec<ComposeContainer> {
    let t = out.trim();
    let items: Vec<serde_json::Value> = if t.starts_with('[') {
        serde_json::from_str(t).unwrap_or_default()
    } else {
        t.lines().filter_map(|l| serde_json::from_str(l.trim()).ok()).collect()
    };
    let mut list: Vec<ComposeContainer> = items
        .iter()
        .filter_map(|v| {
            let service = v.get("Service")?.as_str()?.to_string();
            let state = v.get("State").and_then(|s| s.as_str()).unwrap_or("unknown").to_string();
            let mut ports: Vec<u16> = v
                .get("Publishers")
                .and_then(|p| p.as_array())
                .map(|a| a.iter().filter_map(|p| p.get("PublishedPort")?.as_u64()).filter(|n| *n > 0 && *n <= 65535).map(|n| n as u16).collect())
                .unwrap_or_default();
            ports.sort_unstable();
            ports.dedup();
            Some(ComposeContainer { service, state, ports })
        })
        .collect();
    list.sort_by(|a, b| a.service.cmp(&b.service));
    list
}

#[tauri::command]
pub async fn project_compose_ps(app: tauri::AppHandle, project_id: String) -> Result<Vec<ComposeContainer>, String> {
    let cfg = crate::commands::config::load_config(&app)?;
    let site = cfg.projects.iter().find(|p| p.id == project_id).ok_or("project not found")?;
    if site.run_mode != RunMode::Compose {
        return Ok(Vec::new());
    }
    let name = format!("kova-{}", instance_slug(site));
    let fut = crate::commands::util::silent_command("docker")
        .args(["compose", "-p", &name, "ps", "--all", "--format", "json"])
        .kill_on_drop(true)
        .stdout(std::process::Stdio::piped())
        .output();
    let out = tokio::time::timeout(std::time::Duration::from_secs(20), fut)
        .await
        .map_err(|_| "docker compose ps did not answer within 20 s".to_string())?
        .map_err(|e| format!("could not run docker: {e}"))?;
    Ok(parse_compose_ps(&String::from_utf8_lossy(&out.stdout)))
}

#[cfg(test)]
mod tests {
    #[test]
    fn compose_ps_both_formats() {
        let lines = r#"{"Service":"worker","State":"running","Publishers":[]}
{"Service":"api","State":"running","Publishers":[{"PublishedPort":8080},{"PublishedPort":8080},{"PublishedPort":0}]}"#;
        let l = super::parse_compose_ps(lines);
        assert_eq!(l[0].service, "api");
        assert_eq!(l[0].ports, vec![8080]);
        assert_eq!(l[1].state, "running");
        let arr = r#"[{"Service":"db","State":"exited","Publishers":null}]"#;
        assert_eq!(super::parse_compose_ps(arr)[0].state, "exited");
        assert!(super::parse_compose_ps("").is_empty());
    }

    use super::*;

    fn site(dir: &Path, mode: RunMode) -> Project {
        Project {
            id: "p".into(),
            name: "Go Search API".into(),
            path: dir.display().to_string(),
            port: 8080,
            run_mode: mode,
            container: Some(ContainerSpec { container_port: 9000, join_services: true, ..Default::default() }),
            ..Default::default()
        }
    }

    #[test]
    fn expose_port_from_dockerfile() {
        assert_eq!(dockerfile_expose("FROM golang:1.23\nexpose 8080/tcp 9000\n"), Some(8080));
        assert_eq!(dockerfile_expose("FROM node\nEXPOSE $PORT 3000\n"), Some(3000));
        assert_eq!(dockerfile_expose("FROM node\n# EXPOSE 80\nCMD npm start\n"), None);
        assert_eq!(dockerfile_expose("FROM alpine\n"), None);
    }

    #[test]
    fn dockerfile_steps() {
        let dir = std::env::temp_dir().join("kova-container-test a&b");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("Dockerfile"), "FROM scratch").unwrap();
        let (st, cl) = steps(&site(&dir, RunMode::Dockerfile), RuntimeKind::Docker, &["APP_ENV".into()]).unwrap();
        assert_eq!(st[0], ["docker", "rm", "-f", "kova-go-search-api-p"]);
        assert_eq!(&st[1][..4], ["docker", "build", "-t", "kova/go-search-api-p"]);
        assert_eq!(st[1].last().unwrap(), &dir.display().to_string());
        assert!(st[2].windows(2).any(|w| w == ["--network", SERVICES_NETWORK]));
        assert!(st[2].windows(2).any(|w| w == ["-p", "127.0.0.1:8080:9000"]));
        assert!(st[2].windows(2).any(|w| w == ["-e", "APP_ENV"]));
        assert_eq!(cl, Cleanup::Remove { engine: "docker", name: "kova-go-search-api-p".into() });
        let (w, _) = steps(&site(&dir, RunMode::Dockerfile), RuntimeKind::Wslc, &[]).unwrap();
        assert_eq!(w[0][1], "remove");
        assert!(!w[2].contains(&"--network".to_string()), "WSL containers has no shared network");
        let ps = ps_script(&st, true);
        assert!(ps.contains("'kova-container-test a&b'") || ps.contains("a&b"));
        assert!(ps.contains("if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }"));
        let sh = sh_script(&st, true);
        assert!(sh.contains("|| exit $?"));
        assert!(steps(&site(&dir.join("missing"), RunMode::Dockerfile), RuntimeKind::Docker, &[]).is_err());
    }

    #[test]
    fn compose_needs_docker_and_scales_skipped_services() {
        let dir = std::env::temp_dir().join("kova-compose-test");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("compose.yaml"), "services: {}").unwrap();
        let mut s = site(&dir, RunMode::Compose);
        s.container.as_mut().unwrap().skip_compose_services = vec!["elasticsearch".into(), "bad name".into()];
        assert!(steps(&s, RuntimeKind::Wslc, &[]).is_err());
        let (st, cl) = steps(&s, RuntimeKind::Docker, &[]).unwrap();
        assert!(st[0].windows(2).any(|w| w == ["--scale", "elasticsearch=0"]));
        assert!(!st[0].iter().any(|a| a.contains("bad name")));
        let Cleanup::ComposeDown { args } = cl else { panic!() };
        assert_eq!(args.last().unwrap(), "down");
        assert_eq!(slug("Go Search API!"), "go-search-api");
        let y = "name: x\nservices:\n  api:\n    build: .\n    ports: [\"8080:8080\"]\n  worker:\n    image: w\n  # comment\n  elasticsearch:\n    image: es\nvolumes:\n  data:\n";
        assert_eq!(compose_services(y), vec!["api", "worker", "elasticsearch"]);
    }
}
