//! Proyeksi `sites` → Caddyfile. Murni: tanpa I/O, tanpa state, hasilnya hanya
//! bergantung pada input — jadi bisa diuji tanpa menjalankan Caddy.
//!
//! Config yang dihasilkan mengikuti bentuk yang sudah divalidasi di spike T0
//! (`docs/reports/impl/phase-12-t0-spike.md` §3).

use crate::commands::config::{Site, SiteTarget};

/// Port admin API Caddy — dipakai untuk hot-reload tanpa mematikan proses.
pub const ADMIN_ADDR: &str = "localhost:2019";

/// Upstream selalu loopback: v1.5 hanya melayani dev server di mesin yang sama
/// (keputusan A7 grooming Phase 12).
const UPSTREAM_HOST: &str = "127.0.0.1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TlsMode {
    /// HTTPS dengan CA lokal Caddy. `skip_install_trust` menahan Caddy memasang
    /// root CA sendiri saat start — pemasangan jadi langkah eksplisit dari UI,
    /// supaya dialog sertifikat tidak muncul tiba-tiba (temuan F3 spike T0).
    Https,
    /// Fallback saat user menolak/gagal memasang sertifikat. `auto_https off`
    /// mematikan redirect ke HTTPS, jadi domain tetap bisa dipakai lewat HTTP.
    HttpOnly,
}

/// Site yang benar-benar diproyeksikan ke Caddyfile: aktif, ber-target, domain
/// aman, dan port masuk akal.
fn routable(site: &Site) -> Option<(&str, u16)> {
    if !site.enabled {
        return None;
    }
    let SiteTarget::Port { value } = site.target.as_ref()?;
    if *value == 0 || !is_safe_domain(&site.domain) {
        return None;
    }
    Some((site.domain.as_str(), *value))
}

/// Domain hanya boleh berisi karakter yang tak bisa keluar dari konteksnya di
/// Caddyfile. Bukan validasi DNS penuh — ini pagar supaya isi config.json tak
/// pernah bisa menyuntikkan direktif.
fn is_safe_domain(domain: &str) -> bool {
    !domain.is_empty()
        && domain.len() <= 253
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !domain.contains("..")
        && domain
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
}

/// Render Caddyfile untuk seluruh site yang routable.
///
/// Urutan blok di-sort per domain, jadi konfigurasi yang sama secara semantik
/// selalu menghasilkan teks yang sama — reload hanya perlu dilakukan kalau
/// isinya benar-benar berubah.
pub fn render(sites: &[Site], mode: TlsMode) -> String {
    let mut routes: Vec<(&str, u16)> = sites.iter().filter_map(routable).collect();
    routes.sort_by(|a, b| a.0.cmp(b.0));
    routes.dedup_by(|a, b| a.0 == b.0);

    let mut out = String::new();
    out.push_str("# File ini di-generate oleh Servel. Perubahan manual akan ditimpa.\n");
    out.push_str("{\n");
    out.push_str(&format!("\tadmin {}\n", ADMIN_ADDR));
    match mode {
        TlsMode::Https => out.push_str("\tskip_install_trust\n"),
        TlsMode::HttpOnly => out.push_str("\tauto_https off\n"),
    }
    out.push_str("}\n");

    for (domain, port) in routes {
        out.push('\n');
        match mode {
            TlsMode::Https => {
                out.push_str(&format!("{} {{\n", domain));
                out.push_str("\ttls internal\n");
            }
            TlsMode::HttpOnly => out.push_str(&format!("http://{} {{\n", domain)),
        }
        out.push_str(&format!(
            "\treverse_proxy {}:{}\n",
            UPSTREAM_HOST, port
        ));
        out.push_str("}\n");
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn site(domain: &str, enabled: bool, target: Option<SiteTarget>) -> Site {
        Site {
            id: domain.to_string(),
            domain: domain.to_string(),
            ip: "127.0.0.1".to_string(),
            enabled,
            target,
        }
    }

    fn port(domain: &str, value: u16) -> Site {
        site(domain, true, Some(SiteTarget::Port { value }))
    }

    #[test]
    fn renders_https_block_per_site() {
        let out = render(&[port("myapp.test", 5173)], TlsMode::Https);
        assert!(out.contains("admin localhost:2019"));
        assert!(out.contains("skip_install_trust"));
        assert!(out.contains("myapp.test {"));
        assert!(out.contains("tls internal"));
        assert!(out.contains("reverse_proxy 127.0.0.1:5173"));
        assert!(!out.contains("auto_https off"));
    }

    #[test]
    fn http_only_mode_disables_auto_https() {
        let out = render(&[port("myapp.test", 5173)], TlsMode::HttpOnly);
        assert!(out.contains("auto_https off"));
        assert!(out.contains("http://myapp.test {"));
        assert!(!out.contains("tls internal"));
        assert!(!out.contains("skip_install_trust"));
    }

    #[test]
    fn skips_disabled_and_targetless_sites() {
        let sites = vec![
            site("off.test", false, Some(SiteTarget::Port { value: 3000 })),
            site("hosts-only.test", true, None),
            port("ok.test", 8000),
        ];
        let out = render(&sites, TlsMode::Https);
        assert!(out.contains("ok.test {"));
        assert!(!out.contains("off.test"));
        assert!(!out.contains("hosts-only.test"));
    }

    #[test]
    fn skips_invalid_port_and_unsafe_domain() {
        let sites = vec![
            port("zero.test", 0),
            port("spasi buruk.test", 3000),
            port("brace{.test", 3000),
            port(".leading.test", 3000),
            port("double..dot.test", 3000),
            port("ok.test", 3000),
        ];
        let out = render(&sites, TlsMode::Https);
        assert!(out.contains("ok.test {"));
        assert!(!out.contains("zero.test"));
        assert!(!out.contains("spasi"));
        assert!(!out.contains("brace"));
        assert!(!out.contains("leading.test"));
        assert!(!out.contains("double"));
    }

    #[test]
    fn zero_routable_sites_still_yields_valid_global_block() {
        let out = render(&[], TlsMode::Https);
        assert!(out.contains("admin localhost:2019"));
        assert!(!out.contains("reverse_proxy"));
    }

    #[test]
    fn output_is_stable_regardless_of_input_order() {
        let a = render(
            &[port("b.test", 2), port("a.test", 1), port("c.test", 3)],
            TlsMode::Https,
        );
        let b = render(
            &[port("c.test", 3), port("b.test", 2), port("a.test", 1)],
            TlsMode::Https,
        );
        assert_eq!(a, b);

        // Sekaligus kunci urutannya: a → b → c.
        let pos = |needle: &str| a.find(needle).expect("blok domain harus ada");
        assert!(pos("a.test {") < pos("b.test {"));
        assert!(pos("b.test {") < pos("c.test {"));
    }

    #[test]
    fn duplicate_domain_yields_single_block() {
        // Caddy menolak site-address kembar; config.json bisa saja punya dua
        // entri domain sama, jadi generator yang membereskan.
        let out = render(&[port("dup.test", 1111), port("dup.test", 2222)], TlsMode::Https);
        assert_eq!(out.matches("dup.test {").count(), 1);
    }
}
