//! Lapisan reverse proxy Sites (Phase 12) — engine Caddy sebagai child-process.
//!
//! Pembagian: `binary` = resolusi/instalasi binary caddy, `caddyfile` =
//! proyeksi sites → config (murni, tanpa I/O), `lifecycle` = start/stop/reload
//! proses caddy + pemasangan sertifikat.

pub mod binary;
pub mod caddyfile;
pub mod lifecycle;
