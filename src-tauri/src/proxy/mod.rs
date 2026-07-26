//! Lapisan reverse proxy Sites (Phase 12) — engine Caddy sebagai child-process.
//!
//! Pembagian: `binary` = resolusi/instalasi binary caddy. Generator Caddyfile
//! dan lifecycle proses menyusul di modul terpisah.

pub mod binary;
