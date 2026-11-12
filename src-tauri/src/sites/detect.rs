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
}

fn det(kind: &str, label: &str, command: impl Into<String>, default_port: u16) -> Detection {
    Detection {
        kind: kind.to_string(),
        label: label.to_string(),
        command: command.into(),
        default_port,
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
    has_key(pkg, "dependencies", name) || has_key(pkg, "devDependencies", name)
}

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
    let requires = |name: &str| {
        composer
            .as_ref()
            .is_some_and(|c| has_key(c, "require", name))
    };

    if requires("laravel/framework") && dir.join("artisan").is_file() {
        return Some(det(
            "laravel",
            "Laravel",
            "php artisan serve --host=127.0.0.1 --port={port}",
            8000,
        ));
    }
    if requires("laravel/lumen-framework") {
        return Some(det(
            "lumen",
            "Lumen",
            "php -S 127.0.0.1:{port} -t public",
            8000,
        ));
    }
    // Drupal: router bawaan ada di docroot (`.ht.router.php`). Project composer
    // menaruh docroot di `web/`.
    if dir.join("core/lib/Drupal.php").is_file() {
        return Some(det(
            "drupal",
            "Drupal",
            "php -S 127.0.0.1:{port} .ht.router.php",
            8000,
        ));
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
        return Some(det(
            "wordpress",
            "WordPress",
            "php -S 127.0.0.1:{port}",
            8000,
        ));
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

    if has_dep(&pkg, "next") {
        let cmd = script
            .map(|s| run_script(pm, s, "-p {port}"))
            .unwrap_or_default();
        return Some(det("next", "Next.js", cmd, 3000));
    }
    if has_dep(&pkg, "nuxt") {
        let cmd = script
            .map(|s| run_script(pm, s, "--port {port}"))
            .unwrap_or_default();
        return Some(det("nuxt", "Nuxt", cmd, 3000));
    }
    if has_dep(&pkg, "vite") {
        let flavor = if has_dep(&pkg, "vue") {
            "Vite · Vue"
        } else if has_dep(&pkg, "react") {
            "Vite · React"
        } else if has_dep(&pkg, "svelte") {
            "Vite · Svelte"
        } else {
            "Vite"
        };
        let cmd = script
            .map(|s| run_script(pm, s, "--port {port} --strictPort"))
            .unwrap_or_default();
        return Some(det("vite", flavor, cmd, 5173));
    }
    if has_dep(&pkg, "@angular/core") {
        let cmd = script
            .map(|s| run_script(pm, s, "--port {port}"))
            .unwrap_or_default();
        return Some(det("angular", "Angular", cmd, 4200));
    }

    // Node lain (Express, Nest, CRA, ...): port lewat env PORT.
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
    let cmd = if dir.join(".air.toml").is_file() {
        "air"
    } else {
        "go run ."
    };
    Some(det("go", "Go", cmd, 8080))
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
        .or_else(|| detect_node(dir))
        .or_else(|| detect_go(dir))
        .or_else(|| detect_plain_php(dir))
        .unwrap_or_else(|| det("unknown", "", "", 8000))
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

    impl Drop for Proj {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn laravel_wins_over_its_own_package_json() {
        let p = Proj::new("laravel");
        p.file("artisan", "#!/usr/bin/env php")
            .file(
                "composer.json",
                r#"{"require":{"php":"^8.2","laravel/framework":"^11.0"}}"#,
            )
            .file(
                "package.json",
                r#"{"scripts":{"dev":"vite"},"devDependencies":{"vite":"^5"}}"#,
            );
        let d = p.detect();
        assert_eq!(d.kind, "laravel");
        assert_eq!(
            d.command,
            "php artisan serve --host=127.0.0.1 --port={port}"
        );
        assert_eq!(d.default_port, 8000);
    }

    #[test]
    fn lumen_wordpress_drupal() {
        let p = Proj::new("lumen");
        p.file(
            "composer.json",
            r#"{"require":{"laravel/lumen-framework":"^10.0"}}"#,
        );
        assert_eq!(p.detect().command, "php -S 127.0.0.1:{port} -t public");

        let p = Proj::new("wp");
        p.file("wp-load.php", "<?php");
        assert_eq!(p.detect().kind, "wordpress");
        assert_eq!(p.detect().command, "php -S 127.0.0.1:{port}");

        let p = Proj::new("drupal");
        p.file("core/lib/Drupal.php", "<?php");
        assert_eq!(p.detect().command, "php -S 127.0.0.1:{port} .ht.router.php");

        let p = Proj::new("drupal-composer");
        p.file("web/core/lib/Drupal.php", "<?php")
            .file("composer.json", "{}");
        assert_eq!(
            p.detect().command,
            "php -S 127.0.0.1:{port} -t web web/.ht.router.php"
        );
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
        assert_eq!(
            p.detect().command,
            "pnpm run dev --port {port} --strictPort"
        );

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
        p.file(
            "package.json",
            r#"{"scripts":{"dev":"next dev"},"dependencies":{"next":"14"}}"#,
        );
        assert_eq!(p.detect().command, "npm run dev -- -p {port}");

        let p = Proj::new("nuxt");
        p.file(
            "package.json",
            r#"{"scripts":{"dev":"nuxt dev"},"devDependencies":{"nuxt":"3"}}"#,
        );
        assert_eq!(p.detect().command, "npm run dev -- --port {port}");

        // Backend biasa: tanpa argumen port, mengandalkan env PORT.
        let p = Proj::new("express");
        p.file(
            "package.json",
            r#"{"scripts":{"start":"node server.js"},"dependencies":{"express":"4"}}"#,
        );
        let d = p.detect();
        assert_eq!(
            (d.kind.as_str(), d.command.as_str()),
            ("node", "npm run start")
        );
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
    fn go_plain_php_and_unknown() {
        let p = Proj::new("go");
        p.file("go.mod", "module x");
        assert_eq!(p.detect().command, "go run .");
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
    fn broken_json_does_not_panic() {
        let p = Proj::new("broken");
        p.file("package.json", "{ not json")
            .file("composer.json", "[[");
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
        assert_eq!(
            render_command("php -S 127.0.0.1:{port} -t public", 8001),
            "php -S 127.0.0.1:8001 -t public"
        );
        assert_eq!(render_command("npm run dev", 3000), "npm run dev");
        assert_eq!(name_from_path(r"D:\dev\astra\share-api"), "share-api");
        assert_eq!(name_from_path("/home/u/app/"), "app");
    }
}
