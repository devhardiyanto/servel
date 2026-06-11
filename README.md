# Servel

Desktop app untuk orkestrasi local dev environment — satu UI untuk switch PHP (phpvm), switch Node (fnm), dan start/stop Docker services (MySQL, PostgreSQL, Redis, RabbitMQ, MongoDB, MinIO, Mailpit, Gotenberg, SQL Server) via dynamic docker-compose.

## Prerequisites

- **Node.js** 20 LTS atau lebih baru
- **Rust** stable (install via [rustup](https://rustup.rs/))
- **Tauri CLI v2** — terinstall otomatis via `npm run tauri`
- **Visual Studio Build Tools** (Windows) — workload "Desktop development with C++" wajib ada

## Development

```bash
npm install
npm run tauri dev
```
