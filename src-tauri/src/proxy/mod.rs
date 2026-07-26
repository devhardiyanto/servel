//! Lapisan reverse proxy Sites (Phase 12) — engine Caddy sebagai child-process.
//!
//! Pembagian: `binary` = resolusi/instalasi binary caddy, `caddyfile` =
//! proyeksi sites → config (murni, tanpa I/O). Lifecycle proses menyusul.

pub mod binary;
pub mod caddyfile;
