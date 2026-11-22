//! Deteksi jenis project dari isi folder → usulan perintah dev server.
//!
//! Hasilnya **usulan**: pengguna bisa mengubah perintah dan port per site.
//! `{port}` di perintah diganti port site saat dijalankan; env `PORT` juga
//! selalu di-set, untuk aplikasi yang membaca port dari environment.

use serde::Serialize;
use serde_json::Value;
use std::collections::HashSet;
use std::path::Path;

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Detection {
    /// Id jenis: `laravel`, `vite`, `go`, ... `unknown` bila tak dikenali.
    pub kind: String,
    /// Label tampilan, mis. "Vite · Vue". Kosong bila tak dikenali.
    pub label: String,
    /// Perintah usulan; kosong bila tak dikenali.
    pub command: String,
    /// Port awal yang lazim untuk jenis ini.
    pub default_port: u16,
    /// Alasan singkat untuk dialog Add project, mis. "hono in package.json
    /// dependencies · src/index.ts". Kosong = frontend memakai `evidence`.
    pub reason: String,
}

fn det(kind: &str, label: &str, command: impl Into<String>, default_port: u16) -> Detection {
    Detection {
        kind: kind.to_string(),
        label: label.to_string(),
        command: command.into(),
        default_port,
        reason: String::new(),
    }
}

impl Detection {
    fn why(mut self, parts: &[&str]) -> Self {
        self.reason = parts.iter().filter(|p| !p.is_empty()).copied().collect::<Vec<_>>().join(" · ");
        self
    }
}

fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

fn has_key(v: &Value, section: &str, key: &str) -> bool {
    v.get(section).and_then(|s| s.get(key)).is_some()
}

/// Dependency ada di `dependencies` atau `devDependencies`.
fn has_dep(pkg: &Value, name: &str) -> bool {
    dep_section(pkg, name).is_some()
}

fn dep_section(pkg: &Value, name: &str) -> Option<&'static str> {
    ["dependencies", "devDependencies"].into_iter().find(|s| has_key(pkg, s, name))
}

/// "hono in package.json dependencies".
fn dep_reason(pkg: &Value, name: &str) -> String {
    dep_section(pkg, name).map(|s| format!("{name} in package.json {s}")).unwrap_or_default()
}

/// Berkas masuk server Node yang lazim, untuk alasan deteksi.
fn node_entry(dir: &Path) -> Option<&'static str> {
    const ENTRIES: [&str; 10] = [
        "src/index.ts", "src/main.ts", "src/server.ts", "src/app.ts", "server.ts", "index.ts",
        "src/index.js", "server.js", "index.js", "app.js",
    ];
    ENTRIES.into_iter().find(|f| dir.join(f).is_file())
}

/// Framework backend Node: (paket, kind, label, port lazim). Didahulukan dari
/// framework frontend, karena project yang memuat keduanya biasanya dijalankan
/// lewat server backend-nya.
const NODE_BACKENDS: [(&str, &str, &str, u16); 5] = [
    ("@nestjs/core", "nest", "NestJS", 3000),
    ("@adonisjs/core", "adonis", "AdonisJS", 3333),
    ("hono", "hono", "Hono", 3000),
    ("fastify", "fastify", "Fastify", 3000),
    ("express", "express", "Express", 3000),
];

fn has_script(pkg: &Value, name: &str) -> bool {
    has_key(pkg, "scripts", name)
}

/// Package manager dari lockfile; default npm.
pub fn package_manager(dir: &Path) -> &'static str {
    if dir.join("pnpm-lock.yaml").is_file() {
        "pnpm"
    } else if dir.join("yarn.lock").is_file() {
        "yarn"
    } else if dir.join("bun.lockb").is_file() || dir.join("bun.lock").is_file() {
        "bun"
    } else {
        "npm"
    }
}

/// `<pm> run <script> [-- ]<args>`. Hanya npm yang butuh `--` untuk meneruskan
/// argumen ke script; pnpm/yarn/bun meneruskannya langsung (dan pnpm justru
/// ikut meneruskan `--` secara harfiah).
pub fn run_script(pm: &str, script: &str, args: &str) -> String {
    let base = format!("{} run {}", pm, script);
    if args.is_empty() {
        base
    } else if pm == "npm" {
        format!("{} -- {}", base, args)
    } else {
        format!("{} {}", base, args)
    }
}

fn detect_php_framework(dir: &Path) -> Option<Detection> {
    let composer = read_json(&dir.join("composer.json"));
    let requires = |name: &str| composer.as_ref().is_some_and(|c| has_key(c, "require", name));

    if requires("laravel/framework") && dir.join("artisan").is_file() {
        return Some(
            det("laravel", "Laravel", "php artisan serve --host=127.0.0.1 --port={port}", 8000)
                .why(&["laravel/framework in composer.json", "artisan"]),
        );
    }
    if requires("laravel/lumen-framework") {
        return Some(det("lumen", "Lumen", "php -S 127.0.0.1:{port} -t public", 8000));
    }
    // Drupal: router bawaan ada di docroot (`.ht.router.php`). Project composer
    // menaruh docroot di `web/`.
    if dir.join("core/lib/Drupal.php").is_file() {
        return Some(det("drupal", "Drupal", "php -S 127.0.0.1:{port} .ht.router.php", 8000));
    }
    if dir.join("web/core/lib/Drupal.php").is_file() {
        return Some(det(
            "drupal",
            "Drupal",
            "php -S 127.0.0.1:{port} -t web web/.ht.router.php",
            8000,
        ));
    }
    if dir.join("wp-load.php").is_file() || dir.join("wp-config.php").is_file() {
        return Some(det("wordpress", "WordPress", "php -S 127.0.0.1:{port}", 8000));
    }
    None
}

fn detect_node(dir: &Path) -> Option<Detection> {
    let pkg = read_json(&dir.join("package.json"))?;
    let pm = package_manager(dir);
    // Script yang dipakai: `dev` bila ada, kalau tidak `start`.
    let script = if has_script(&pkg, "dev") {
        Some("dev")
    } else if has_script(&pkg, "start") {
        Some("start")
    } else {
        None
    };

    // Port backend lewat env PORT. Tanpa script dev/start perintah dikosongkan,
    // kecuali AdonisJS yang punya `node ace serve`.
    let backend_cmd = |fallback: &str| match script {
        Some(s) => run_script(pm, s, ""),
        None => fallback.to_string(),
    };
    let has_ace = dir.join("ace").is_file();
    for (dep, kind, label, port) in NODE_BACKENDS {
        if has_dep(&pkg, dep) {
            let fallback = if kind == "adonis" && has_ace { "node ace serve --watch" } else { "" };
            let entry = node_entry(dir).unwrap_or("");
            return Some(det(kind, label, backend_cmd(fallback), port).why(&[&dep_reason(&pkg, dep), entry]));
        }
    }
    if has_ace {
        return Some(det("adonis", "AdonisJS", backend_cmd("node ace serve --watch"), 3333).why(&["ace"]));
    }

    if has_dep(&pkg, "next") {
        let cmd = script.map(|s| run_script(pm, s, "-p {port}")).unwrap_or_default();
        return Some(det("next", "Next.js", cmd, 3000).why(&[&dep_reason(&pkg, "next")]));
    }
    if has_dep(&pkg, "nuxt") {
        let cmd = script.map(|s| run_script(pm, s, "--port {port}")).unwrap_or_default();
        return Some(det("nuxt", "Nuxt", cmd, 3000).why(&[&dep_reason(&pkg, "nuxt")]));
    }
    if has_dep(&pkg, "vite") {
        let (flavor, lib) = if has_dep(&pkg, "vue") {
            ("Vite · Vue", "vue")
        } else if has_dep(&pkg, "react") {
            ("Vite · React", "react")
        } else if has_dep(&pkg, "svelte") {
            ("Vite · Svelte", "svelte")
        } else {
            ("Vite", "")
        };
        let cmd = script
            .map(|s| run_script(pm, s, "--port {port} --strictPort"))
            .unwrap_or_default();
        let lib_reason = if lib.is_empty() { String::new() } else { dep_reason(&pkg, lib) };
        return Some(det("vite", flavor, cmd, 5173).why(&[&lib_reason, &dep_reason(&pkg, "vite")]));
    }
    if has_dep(&pkg, "@angular/core") {
        let cmd = script.map(|s| run_script(pm, s, "--port {port}")).unwrap_or_default();
        return Some(det("angular", "Angular", cmd, 4200).why(&[&dep_reason(&pkg, "@angular/core")]));
    }
    // React tanpa Vite (mis. react-scripts) membaca port dari env PORT.
    if has_dep(&pkg, "react") {
        let cmd = script.map(|s| run_script(pm, s, "")).unwrap_or_default();
        return Some(det("react", "React", cmd, 3000).why(&[&dep_reason(&pkg, "react")]));
    }

    // Node lain: port lewat env PORT.
    let cmd = match script {
        Some(s) => run_script(pm, s, ""),
        None => match pkg.get("main").and_then(Value::as_str) {
            Some(main) if !main.trim().is_empty() => format!("node {}", main.trim()),
            _ => String::new(),
        },
    };
    Some(det("node", "Node", cmd, 3000))
}

fn detect_go(dir: &Path) -> Option<Detection> {
    if !dir.join("go.mod").is_file() {
        return None;
    }
    let cmd = if dir.join(".air.toml").is_file() { "air" } else { "go run ." };
    let entry = ["main.go", "cmd/server/main.go"].into_iter().find(|f| dir.join(f).is_file()).unwrap_or("");
    Some(det("go", "Go", cmd, 8080).why(&["go.mod", entry]))
}

/// Interpreter Python bawaan platform. Dengan mise aktif, keduanya ada.
const PYTHON: &str = if cfg!(windows) { "python" } else { "python3" };

/// Awalan untuk menjalankan Python di environment milik project. Tanpa ini
/// `python` yang terpakai adalah interpreter global, yang tidak punya
/// dependency project.
fn python_runner(dir: &Path) -> String {
    if dir.join("uv.lock").is_file() {
        return "uv run python".into();
    }
    if dir.join("poetry.lock").is_file() {
        return "poetry run python".into();
    }
    if dir.join("Pipfile").is_file() {
        return "pipenv run python".into();
    }
    for venv in [".venv", "venv"] {
        let (rel, shown) = if cfg!(windows) {
            (format!("{venv}/Scripts/python.exe"), format!("{venv}\\Scripts\\python"))
        } else {
            (format!("{venv}/bin/python"), format!("{venv}/bin/python"))
        };
        if dir.join(rel).is_file() {
            return shown;
        }
    }
    PYTHON.into()
}

/// Nama paket yang disebut di berkas dependency Python, huruf kecil.
/// Pencocokan per token, jadi `flask-cors` tidak dihitung sebagai `flask`.
fn python_packages(dir: &Path) -> HashSet<String> {
    let mut out = HashSet::new();
    for file in ["requirements.txt", "pyproject.toml", "Pipfile", "setup.py", "setup.cfg"] {
        let Ok(text) = std::fs::read_to_string(dir.join(file)) else { continue };
        for token in text
            .to_ascii_lowercase()
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
        {
            if !token.is_empty() {
                out.insert(token.to_string());
            }
        }
    }
    out
}

/// Cari `<nama> = <Kelas>(` di berkas masuk yang lazim. Hasil: (modul, nama
/// variabel), mis. `("app.main", "app")` untuk `app/main.py`.
fn python_app(dir: &Path, class: &str) -> Option<(String, String)> {
    let call = format!("{class}(");
    for rel in ["main.py", "app.py", "server.py", "api.py", "wsgi.py", "asgi.py", "app/main.py", "src/main.py"] {
        let Ok(text) = std::fs::read_to_string(dir.join(rel)) else { continue };
        for line in text.lines() {
            let Some((left, right)) = line.split_once('=') else { continue };
            if !right.trim_start().starts_with(&call) {
                continue;
            }
            // `app: FastAPI = FastAPI()` → `app`. Baris berindentasi dilewati:
            // itu variabel lokal di dalam fungsi, bukan objek tingkat modul.
            let var = left.split(':').next().unwrap_or("").trim_end();
            let is_ident = !var.is_empty()
                && var.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                && !var.starts_with(|c: char| c.is_ascii_digit());
            if is_ident {
                let module = rel.trim_end_matches(".py").replace('/', ".");
                return Some((module, var.to_string()));
            }
        }
    }
    None
}

/// Django dikenali dari `manage.py`, dan didahulukan dari Node karena project
/// Django sering punya `package.json` untuk aset.
fn detect_django(dir: &Path) -> Option<Detection> {
    if !dir.join("manage.py").is_file() {
        return None;
    }
    Some(det(
        "django",
        "Django",
        format!("{} manage.py runserver 127.0.0.1:{{port}}", python_runner(dir)),
        8000,
    ))
}

fn detect_python(dir: &Path) -> Option<Detection> {
    const MARKERS: [&str; 7] =
        ["pyproject.toml", "requirements.txt", "Pipfile", "uv.lock", "poetry.lock", "setup.py", "setup.cfg"];
    let has_marker = MARKERS.iter().any(|f| dir.join(f).is_file());
    let entry = ["main.py", "app.py"].into_iter().find(|f| dir.join(f).is_file());
    if !has_marker && entry.is_none() {
        return None;
    }
    let py = python_runner(dir);
    let packages = python_packages(dir);

    // Objek aplikasi di kode sumber lebih kuat daripada daftar dependency:
    // dari situ modul dan nama variabelnya diketahui.
    if let Some((module, var)) = python_app(dir, "FastAPI") {
        return Some(det(
            "fastapi",
            "FastAPI",
            format!("{py} -m uvicorn {module}:{var} --reload --host 127.0.0.1 --port {{port}}"),
            8000,
        ));
    }
    if let Some((module, var)) = python_app(dir, "Flask") {
        return Some(det(
            "flask",
            "Flask",
            format!("{py} -m flask --app {module}:{var} run --debug --host 127.0.0.1 --port {{port}}"),
            5000,
        ));
    }
    // Framework dideklarasikan tetapi objek aplikasinya tidak ditemukan
    // (application factory, struktur paket lain): jenis dikenali, perintah
    // dikosongkan agar pengguna mengisinya, bukan diberi tebakan yang salah.
    if packages.contains("fastapi") {
        return Some(det("fastapi", "FastAPI", "", 8000));
    }
    if packages.contains("flask") {
        return Some(det("flask", "Flask", "", 5000));
    }
    if packages.contains("django") {
        return Some(det("django", "Django", "", 8000));
    }
    // Skrip biasa: port lewat env PORT.
    let cmd = entry.map(|f| format!("{py} {f}")).unwrap_or_default();
    Some(det("python", "Python", cmd, 8000))
}

fn detect_plain_php(dir: &Path) -> Option<Detection> {
    if dir.join("public/index.php").is_file() {
        return Some(det("php", "PHP", "php -S 127.0.0.1:{port} -t public", 8000));
    }
    if dir.join("index.php").is_file() || dir.join("composer.json").is_file() {
        return Some(det("php", "PHP", "php -S 127.0.0.1:{port}", 8000));
    }
    None
}

/// Urutan penting: framework PHP didahulukan karena project Laravel juga
/// punya `package.json` (Vite untuk aset), tetapi yang dibuka adalah PHP-nya.
pub fn detect(dir: &Path) -> Detection {
    detect_php_framework(dir)
        .or_else(|| detect_django(dir))
        .or_else(|| detect_node(dir))
        .or_else(|| detect_go(dir))
        .or_else(|| detect_python(dir))
        .or_else(|| detect_plain_php(dir))
        .unwrap_or_else(|| det("unknown", "", "", 8000))
}

/// Berkas penanda yang ada di folder, untuk ditampilkan sebagai alasan hasil
/// deteksi ("artisan, composer.json"). Urutan tetap; paling banyak empat.
pub fn evidence(dir: &Path) -> Vec<String> {
    const MARKERS: [&str; 20] = [
        "artisan", "manage.py", "composer.json", "package.json", "go.mod", "pyproject.toml",
        "requirements.txt", "Pipfile", "nuxt.config.ts", "nuxt.config.js", "next.config.js",
        "next.config.mjs", "vite.config.ts", "vite.config.js", "angular.json", "wp-config.php",
        "wp-load.php", "index.php", "main.py", "app.py",
    ];
    MARKERS.iter().filter(|f| dir.join(f).is_file()).take(4).map(|f| f.to_string()).collect()
}

/// Pilihan perintah untuk field Command: hasil deteksi, lalu script
/// dari `composer.json` dan `package.json`.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ScriptOption {
    /// Teks tombol, mis. "composer run dev".
    pub label: String,
    pub command: String,
    /// `detected`, `composer.json` atau `package.json`.
    pub source: String,
    /// Peringatan singkat bila script kemungkinan bermasalah di OS ini.
    pub note: Option<String>,
}

/// Isi script composer: string atau array perintah.
fn composer_script_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(items) => items.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(" "),
        _ => String::new(),
    }
}

/// `php artisan pail` butuh ekstensi `pcntl`, yang tidak ada di PHP Windows.
/// Dengan `concurrently --kill-others`, pail yang mati menghentikan semuanya.
fn composer_note(text: &str, windows: bool) -> Option<String> {
    if windows && text.contains("artisan pail") {
        return Some(
            "Runs php artisan pail, which needs the pcntl extension that PHP on Windows does not have. The script may stop right after it starts."
                .to_string(),
        );
    }
    if !text.contains("disableProcessTimeout") && (text.contains("concurrently") || text.contains("artisan serve")) {
        return Some(
            "Composer stops scripts after 300 seconds unless the script starts with Composer\\Config::disableProcessTimeout."
                .to_string(),
        );
    }
    None
}

pub fn script_options(dir: &Path, windows: bool) -> Vec<ScriptOption> {
    let mut out: Vec<ScriptOption> = Vec::new();
    let mut push = |o: ScriptOption| {
        if !o.command.is_empty() && !out.iter().any(|e| e.command == o.command) {
            out.push(o);
        }
    };
    let d = detect(dir);
    push(ScriptOption { label: d.command.clone(), command: d.command.clone(), source: "detected".into(), note: None });

    if let Some(scripts) = read_json(&dir.join("composer.json")).and_then(|c| c.get("scripts").cloned()) {
        for key in ["dev", "serve", "start"] {
            if let Some(v) = scripts.get(key) {
                let command = format!("composer run {}", key);
                push(ScriptOption {
                    label: command.clone(),
                    command,
                    source: "composer.json".into(),
                    note: composer_note(&composer_script_text(v), windows),
                });
            }
        }
    }
    if let Some(pkg) = read_json(&dir.join("package.json")) {
        let pm = package_manager(dir);
        for key in ["dev", "start", "serve", "preview"] {
            if has_script(&pkg, key) {
                let command = run_script(pm, key, "");
                // Hasil deteksi sudah memuat script yang sama dengan `--port`.
                if d.command.starts_with(&command) {
                    continue;
                }
                push(ScriptOption { label: command.clone(), command, source: "package.json".into(), note: None });
            }
        }
    }
    out
}

/// Port pertama ≥ `start` yang tidak ada di `taken` dan lolos `is_free`.
pub fn pick_port(start: u16, taken: &HashSet<u16>, is_free: impl Fn(u16) -> bool) -> Option<u16> {
    (start.max(1024)..=u16::MAX).find(|p| !taken.contains(p) && is_free(*p))
}

/// Ganti `{port}` di perintah.
pub fn render_command(template: &str, port: u16) -> String {
    template.replace("{port}", &port.to_string())
}

/// Nama tampilan = nama folder terakhir.
pub fn name_from_path(path: &str) -> String {
    path.trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(path)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct Proj(PathBuf);

    impl Proj {
        fn new(tag: &str) -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let dir = std::env::temp_dir().join(format!("servel-detect-{}-{}", tag, nanos));
            std::fs::create_dir_all(&dir).unwrap();
            Proj(dir)
        }
        fn file(&self, rel: &str, content: &str) -> &Self {
            let p = self.0.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, content).unwrap();
            self
        }
        fn detect(&self) -> Detection {
            detect(&self.0)
        }
    }

    impl Proj {
        fn scripts(&self, windows: bool) -> Vec<(String, String, bool)> {
            script_options(&self.0, windows)
                .into_iter()
                .map(|o| (o.command, o.source, o.note.is_some()))
                .collect()
        }
    }

    #[test]
    fn laravel_offers_artisan_serve_then_composer_and_npm_scripts() {
        let p = Proj::new("scripts-laravel");
        p.file("artisan", "")
            .file(
                "composer.json",
                r#"{"require":{"laravel/framework":"^12.0"},"scripts":{"dev":["Composer\\Config::disableProcessTimeout","npx concurrently \"php artisan serve\" \"php artisan pail --timeout=0\" \"npm run dev\""]}}"#,
            )
            .file("package.json", r#"{"scripts":{"dev":"vite","build":"vite build"}}"#);
        let s = p.scripts(false);
        assert_eq!(s[0].0, "php artisan serve --host=127.0.0.1 --port={port}");
        assert_eq!(s[0].1, "detected");
        assert_eq!((s[1].0.as_str(), s[1].1.as_str(), s[1].2), ("composer run dev", "composer.json", false));
        assert_eq!(s[2].0, "npm run dev");
        assert_eq!(s.len(), 3, "build bukan dev server");
        // Di Windows, pail memberi peringatan.
        assert!(p.scripts(true)[1].2);
    }

    #[test]
    fn composer_script_without_disabled_timeout_gets_a_note() {
        let p = Proj::new("scripts-timeout");
        p.file("composer.json", r#"{"scripts":{"dev":"npx concurrently \"php artisan serve\" \"npm run dev\""}}"#);
        let s = p.scripts(false);
        let dev = s.iter().find(|o| o.0 == "composer run dev").unwrap();
        assert!(dev.2);
    }

    #[test]
    fn node_script_already_in_detection_is_not_repeated() {
        let p = Proj::new("scripts-vite");
        p.file("package.json", r#"{"devDependencies":{"vite":"^5"},"scripts":{"dev":"vite","preview":"vite preview"}}"#);
        let s = p.scripts(false);
        assert_eq!(s[0].0, "npm run dev -- --port {port} --strictPort");
        assert_eq!(s.iter().map(|o| o.0.as_str()).collect::<Vec<_>>(), ["npm run dev -- --port {port} --strictPort", "npm run preview"]);
    }

    impl Drop for Proj {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn laravel_wins_over_its_own_package_json() {
        let p = Proj::new("laravel");
        p.file("artisan", "#!/usr/bin/env php")
            .file("composer.json", r#"{"require":{"php":"^8.2","laravel/framework":"^11.0"}}"#)
            .file("package.json", r#"{"scripts":{"dev":"vite"},"devDependencies":{"vite":"^5"}}"#);
        let d = p.detect();
        assert_eq!(d.kind, "laravel");
        assert_eq!(d.command, "php artisan serve --host=127.0.0.1 --port={port}");
        assert_eq!(d.default_port, 8000);
    }

    #[test]
    fn lumen_wordpress_drupal() {
        let p = Proj::new("lumen");
        p.file("composer.json", r#"{"require":{"laravel/lumen-framework":"^10.0"}}"#);
        assert_eq!(p.detect().command, "php -S 127.0.0.1:{port} -t public");

        let p = Proj::new("wp");
        p.file("wp-load.php", "<?php");
        assert_eq!(p.detect().kind, "wordpress");
        assert_eq!(p.detect().command, "php -S 127.0.0.1:{port}");

        let p = Proj::new("drupal");
        p.file("core/lib/Drupal.php", "<?php");
        assert_eq!(p.detect().command, "php -S 127.0.0.1:{port} .ht.router.php");

        let p = Proj::new("drupal-composer");
        p.file("web/core/lib/Drupal.php", "<?php").file("composer.json", "{}");
        assert_eq!(p.detect().command, "php -S 127.0.0.1:{port} -t web web/.ht.router.php");
    }

    #[test]
    fn vite_flavors_and_package_managers() {
        let p = Proj::new("vue");
        p.file(
            "package.json",
            r#"{"scripts":{"dev":"vite"},"dependencies":{"vue":"^3"},"devDependencies":{"vite":"^5"}}"#,
        );
        let d = p.detect();
        assert_eq!((d.kind.as_str(), d.label.as_str()), ("vite", "Vite · Vue"));
        assert_eq!(d.command, "npm run dev -- --port {port} --strictPort");
        assert_eq!(d.default_port, 5173);

        // pnpm meneruskan argumen tanpa `--`.
        p.file("pnpm-lock.yaml", "");
        assert_eq!(p.detect().command, "pnpm run dev --port {port} --strictPort");

        let p = Proj::new("react");
        p.file(
            "package.json",
            r#"{"scripts":{"dev":"vite"},"dependencies":{"react":"^18"},"devDependencies":{"vite":"^5"}}"#,
        )
        .file("yarn.lock", "");
        let d = p.detect();
        assert_eq!(d.label, "Vite · React");
        assert_eq!(d.command, "yarn run dev --port {port} --strictPort");
    }

    #[test]
    fn next_nuxt_and_plain_node() {
        let p = Proj::new("next");
        p.file("package.json", r#"{"scripts":{"dev":"next dev"},"dependencies":{"next":"14"}}"#);
        assert_eq!(p.detect().command, "npm run dev -- -p {port}");

        let p = Proj::new("nuxt");
        p.file("package.json", r#"{"scripts":{"dev":"nuxt dev"},"devDependencies":{"nuxt":"3"}}"#);
        assert_eq!(p.detect().command, "npm run dev -- --port {port}");

        // Backend biasa: tanpa argumen port, mengandalkan env PORT.
        let p = Proj::new("express");
        p.file("package.json", r#"{"scripts":{"start":"node server.js"},"dependencies":{"express":"4"}}"#);
        let d = p.detect();
        assert_eq!((d.kind.as_str(), d.command.as_str()), ("express", "npm run start"));
        assert_eq!(d.default_port, 3000);

        // Tanpa script: pakai `main`.
        let p = Proj::new("main");
        p.file("package.json", r#"{"main":"index.js"}"#);
        assert_eq!(p.detect().command, "node index.js");

        // Tanpa script dan tanpa main: jenis dikenali, perintah dikosongkan.
        let p = Proj::new("bare");
        p.file("package.json", "{}");
        let d = p.detect();
        assert_eq!((d.kind.as_str(), d.command.as_str()), ("node", ""));
    }

    #[test]
    fn node_backend_frameworks_use_the_dev_script() {
        for (dep, kind, label, port) in NODE_BACKENDS {
            let p = Proj::new(kind);
            p.file("package.json", &format!(r#"{{"scripts":{{"dev":"x"}},"dependencies":{{"{dep}":"1"}}}}"#))
                .file("src/index.ts", "");
            let d = p.detect();
            assert_eq!((d.kind.as_str(), d.label.as_str(), d.default_port), (kind, label, port));
            assert_eq!(d.command, "npm run dev");
            assert_eq!(d.reason, format!("{dep} in package.json dependencies · src/index.ts"));
        }
    }

    #[test]
    fn backend_wins_over_frontend_in_the_same_package_json() {
        let p = Proj::new("react-express");
        p.file(
            "package.json",
            r#"{"scripts":{"dev":"concurrently vite \"node server.js\""},"dependencies":{"react":"^18","express":"^4"},"devDependencies":{"vite":"^5"}}"#,
        )
        .file("server.js", "");
        let d = p.detect();
        assert_eq!((d.kind.as_str(), d.command.as_str()), ("express", "npm run dev"));
        assert_eq!(d.reason, "express in package.json dependencies · server.js");

        // Hono sebagai devDependency tetap dikenali, dengan bagian yang benar.
        let p = Proj::new("vue-hono");
        p.file("package.json", r#"{"scripts":{"dev":"x"},"dependencies":{"vue":"^3"},"devDependencies":{"hono":"^4","vite":"^5"}}"#);
        let d = p.detect();
        assert_eq!(d.kind, "hono");
        assert_eq!(d.reason, "hono in package.json devDependencies");
    }

    #[test]
    fn react_without_vite_and_adonis_without_script() {
        let p = Proj::new("cra");
        p.file("package.json", r#"{"scripts":{"start":"react-scripts start"},"dependencies":{"react":"^18","react-scripts":"5"}}"#);
        let d = p.detect();
        assert_eq!((d.kind.as_str(), d.label.as_str(), d.command.as_str()), ("react", "React", "npm run start"));
        assert_eq!(d.reason, "react in package.json dependencies");

        // Vite + React tetap jenis vite, alasannya menyebut keduanya.
        let p = Proj::new("vite-react-reason");
        p.file("package.json", r#"{"scripts":{"dev":"vite"},"dependencies":{"react":"^18"},"devDependencies":{"vite":"^5"}}"#);
        assert_eq!(p.detect().reason, "react in package.json dependencies · vite in package.json devDependencies");

        let p = Proj::new("adonis-ace");
        p.file("package.json", "{}").file("ace", "");
        let d = p.detect();
        assert_eq!((d.kind.as_str(), d.command.as_str(), d.default_port), ("adonis", "node ace serve --watch", 3333));
    }

    #[test]
    fn go_plain_php_and_unknown() {
        let p = Proj::new("go");
        p.file("go.mod", "module x").file("main.go", "package main");
        assert_eq!(p.detect().command, "go run .");
        assert_eq!(p.detect().reason, "go.mod · main.go");
        assert_eq!(p.detect().default_port, 8080);
        p.file(".air.toml", "");
        assert_eq!(p.detect().command, "air");

        let p = Proj::new("php-public");
        p.file("public/index.php", "<?php");
        assert_eq!(p.detect().command, "php -S 127.0.0.1:{port} -t public");

        let p = Proj::new("php-root");
        p.file("index.php", "<?php");
        assert_eq!(p.detect().command, "php -S 127.0.0.1:{port}");

        let p = Proj::new("empty");
        let d = p.detect();
        assert_eq!((d.kind.as_str(), d.command.as_str()), ("unknown", ""));
    }

    #[test]
    fn django_wins_over_its_own_package_json() {
        let p = Proj::new("django");
        p.file("manage.py", "#!/usr/bin/env python")
            .file("requirements.txt", "Django>=5.0\n")
            .file("package.json", r#"{"scripts":{"dev":"vite"},"devDependencies":{"vite":"^5"}}"#);
        let d = p.detect();
        assert_eq!((d.kind.as_str(), d.default_port), ("django", 8000));
        assert_eq!(d.command, format!("{PYTHON} manage.py runserver 127.0.0.1:{{port}}"));
    }

    #[test]
    fn fastapi_and_flask_use_the_app_object_found_in_source() {
        let p = Proj::new("fastapi");
        p.file("requirements.txt", "fastapi[standard]==0.115\n")
            .file("main.py", "from fastapi import FastAPI\n\napp = FastAPI()\n");
        let d = p.detect();
        assert_eq!(d.kind, "fastapi");
        assert_eq!(d.command, format!("{PYTHON} -m uvicorn main:app --reload --host 127.0.0.1 --port {{port}}"));

        // Anotasi tipe, nama variabel lain, dan berkas di dalam paket.
        let p = Proj::new("fastapi-pkg");
        p.file("pyproject.toml", "[project]\ndependencies = [\"fastapi\"]\n")
            .file("app/main.py", "api: FastAPI = FastAPI(title='x')\n");
        assert!(p.detect().command.contains(" -m uvicorn app.main:api "));

        let p = Proj::new("flask");
        p.file("requirements.txt", "flask\n").file("app.py", "app = Flask(__name__)\n");
        let d = p.detect();
        assert_eq!((d.kind.as_str(), d.default_port), ("flask", 5000));
        assert_eq!(
            d.command,
            format!("{PYTHON} -m flask --app app:app run --debug --host 127.0.0.1 --port {{port}}")
        );
    }

    #[test]
    fn python_framework_without_a_visible_app_object_leaves_the_command_empty() {
        // Application factory: objek dibuat di dalam fungsi (berindentasi).
        let p = Proj::new("flask-factory");
        p.file("requirements.txt", "Flask==3.0\nflask-cors\n")
            .file("app.py", "def create_app():\n    app = Flask(__name__)\n    return app\n");
        let d = p.detect();
        assert_eq!((d.kind.as_str(), d.command.as_str()), ("flask", ""));

        // `flask-cors` saja bukan Flask.
        let p = Proj::new("not-flask");
        p.file("requirements.txt", "flask-cors\n");
        assert_eq!(p.detect().kind, "python");
    }

    #[test]
    fn python_runner_follows_the_project_environment() {
        let p = Proj::new("uv");
        p.file("uv.lock", "").file("pyproject.toml", "[project]\n").file("main.py", "print(1)\n");
        let d = p.detect();
        assert_eq!((d.kind.as_str(), d.command.as_str()), ("python", "uv run python main.py"));

        let p = Proj::new("poetry");
        p.file("poetry.lock", "").file("manage.py", "");
        assert_eq!(p.detect().command, "poetry run python manage.py runserver 127.0.0.1:{port}");

        let p = Proj::new("venv");
        let (file, shown) = if cfg!(windows) {
            (".venv/Scripts/python.exe", ".venv\\Scripts\\python")
        } else {
            (".venv/bin/python", ".venv/bin/python")
        };
        p.file(file, "").file("requirements.txt", "requests\n").file("app.py", "print(1)\n");
        assert_eq!(p.detect().command, format!("{shown} app.py"));

        // Hanya berkas dependency, tanpa berkas masuk: dikenali, tanpa perintah.
        let p = Proj::new("py-bare");
        p.file("requirements.txt", "requests\n");
        let d = p.detect();
        assert_eq!((d.kind.as_str(), d.command.as_str()), ("python", ""));
    }

    #[test]
    fn node_and_go_still_win_over_stray_python_files() {
        let p = Proj::new("node-with-py");
        p.file("package.json", r#"{"scripts":{"dev":"vite"},"devDependencies":{"vite":"^5"}}"#)
            .file("requirements.txt", "flask\n");
        assert_eq!(p.detect().kind, "vite");

        let p = Proj::new("go-with-py");
        p.file("go.mod", "module x").file("main.py", "print(1)\n");
        assert_eq!(p.detect().kind, "go");
    }

    #[test]
    fn evidence_lists_marker_files_in_a_fixed_order() {
        let p = Proj::new("evidence");
        p.file("composer.json", "{}").file("artisan", "").file("package.json", "{}");
        assert_eq!(evidence(&p.0), vec!["artisan", "composer.json", "package.json"]);
        assert!(evidence(&Proj::new("evidence-empty").0).is_empty());
    }

    #[test]
    fn broken_json_does_not_panic() {
        let p = Proj::new("broken");
        p.file("package.json", "{ not json").file("composer.json", "[[");
        // composer.json ada → PHP polos; package.json rusak diabaikan.
        assert_eq!(p.detect().kind, "php");
    }

    #[test]
    fn pick_port_skips_taken_and_busy() {
        let taken: HashSet<u16> = [8000, 8001].into_iter().collect();
        assert_eq!(pick_port(8000, &taken, |p| p != 8002), Some(8003));
        assert_eq!(pick_port(5173, &HashSet::new(), |_| true), Some(5173));
        // Port di bawah 1024 tidak pernah diusulkan.
        assert_eq!(pick_port(80, &HashSet::new(), |_| true), Some(1024));
        assert_eq!(pick_port(65535, &HashSet::new(), |_| false), None);
    }

    #[test]
    fn render_and_name() {
        assert_eq!(render_command("php -S 127.0.0.1:{port} -t public", 8001), "php -S 127.0.0.1:8001 -t public");
        assert_eq!(render_command("npm run dev", 3000), "npm run dev");
        assert_eq!(name_from_path(r"C:\dev\acme\acme-api"), "acme-api");
        assert_eq!(name_from_path("/home/u/app/"), "app");
    }
}
