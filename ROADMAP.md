# Servel Roadmap

Status per versi. Item yang sudah rilis tetap dicatat supaya keputusan desainnya bisa ditelusuri.

## v1.6.0 — WSL containers (Windows)

Tujuan: di Windows, Servel menjalankan service tanpa Docker Desktop dan tanpa distro Linux tambahan, memakai engine container bawaan WSL (`wslc.exe`, WSL ≥ 3.0.1).

### Selesai

- **Abstraksi runtime** (`src-tauri/src/runtime/`): satu pintu untuk status, stats, start, stop. Command Tauri, tray, dan loop polling tidak lagi memanggil `docker` langsung.
- **Backend WSLC** (`runtime/wslc.rs`):
  - Orkestrasi imperatif per service: `volume create` → `run -d` / `start` / `stop`, karena WSLC belum punya `compose` maupun endpoint Docker API.
  - Semua pemanggilan `wslc` diserialkan dan error transien (`ERROR_SHARING_VIOLATION`, `ERROR_ALREADY_EXISTS`) di-retry.
  - Spec hash per container disimpan di `wslc-state.json`; container di-recreate hanya bila definisinya berubah. Data tetap aman di named volume.
  - Container `servel_*` yang dibuat di luar Servel diadopsi apa adanya, tidak di-recreate.
  - Stop tidak menghapus container (beda dengan `compose down`).
  - Port di-bind ke `127.0.0.1`.
- **Pemilihan engine**: config `containerRuntime` = `auto` | `wslc` | `docker` (config v5). `auto` memilih WSLC hanya bila Docker tidak terpasang, supaya instalasi lama tidak pindah storage diam-diam.
- **CLI `servel`** (`assets/cli/`): versi umum dari skrip manual `servel.ps1`. Berisi `preflight`, `list`, `up`, `down`, `status`, `logs`, `shell`, dan `restore`.
  - Dipasang ke PATH user oleh installer NSIS lewat `install-path.ps1`, yang menulis registry dengan nilai yang tidak di-expand. Saat uninstall, entri PATH dihapus lagi.
  - Spec hash identik dengan aplikasi (dikunci oleh golden test), dan state `wslc-state.json` dipakai bersama, jadi CLI dan aplikasi bisa dipakai bergantian.
  - Kompatibel dengan Windows PowerShell 5.1 (escape kutip ganda untuk mode legacy) dan pwsh 7.
- **phpvm/fnm opsional**: Onboarding hanya mewajibkan engine container; mise dideteksi dan ditampilkan (integrasi penuh di v1.7).
- **Prasyarat**: deteksi versi WSL, versi wslc, dan peringatan bila Servel berjalan elevated (sesi WSLC terpisah per elevation).
- **UI**:
  - Onboarding, banner Dashboard, dan Settings → Container engine menjadi engine-neutral, termasuk tombol pindah ke WSL containers satu klik.
  - Tema "soft": netral kebiruan, border translucent, aksen pastel, skala radius, Inter untuk UI dan mono hanya untuk data.

### Perlu verifikasi di mesin Windows

Hal-hal berikut tidak bisa diuji tanpa `wslc` sungguhan. Parser sudah dibuat toleran terhadap beberapa bentuk output.

- ~~Bentuk JSON `wslc container list --all --format json`~~ — **terverifikasi di WSL 3.0.5**: NDJSON bergaya Docker (`Names` string, `State` teks, exit code hanya di `Status`). Fixture: `src-tauri/src/runtime/fixtures/`.
- ~~Bentuk JSON `wslc stats --format json`~~ — **terverifikasi di WSL 3.0.5**: NDJSON bergaya `docker stats` (`Name`, `MemUsage`); output kosong bila tidak ada container running; `--no-stream` tidak dikenali (tidak dipakai).
- `wslc stop -t 30` dan `wslc remove -f`.
- Apakah `.wslconfig` memengaruhi VM WSLC.
- CLI `servel`: belum pernah dijalankan di PowerShell sungguhan (lingkungan pengembangan tanpa pwsh). Algoritma hash sudah dicocokkan dengan Rust; sintaks skrip perlu diuji di Windows.

### Batasan yang diketahui

- Tidak ada network antar-container (`servel_default`). Service diakses dari host lewat `127.0.0.1`; dev lokal Servel tidak membutuhkan komunikasi antar-container.
- Tidak ada restart policy, jadi service hidup kembali saat Servel auto-start, bukan saat Windows boot.
- Data Docker Desktop tidak dimigrasikan otomatis.

## v1.7.0 — mise sebagai version manager

Disetujui: cakupan terpisah dari v1.6; Servel boleh mengganti versi per project lewat UI.

### Temuan dari mesin pengguna (Windows, mise 2026.11.2)

- mise mengelola **PHP dan Node** di Windows (php 8.3.33 / 8.3.35 / 8.5.11, node 20 / 22 / 24).
- Sumber versi bisa **berbeda per tool dalam satu project**. Contoh di `event-backend`: node dari `D:\dev\event\mise.toml`, php dari global `~\.config\mise\config.toml`.
- Config berlaku **ke subfolder**: root project ≠ lokasi config.

### Selesai

- **`src-tauri/src/mise.rs`**:
  - mise dijalankan dengan cwd = project aktif (atau home untuk konteks Global); hanya output `--json` yang di-parse.
  - Fixture output asli: `ls --current`, `ls --installed php`, `--version` dengan banner.
- **Dispatch**: `php_*` / `node_*` (list, active, switch, install, hook) memakai mise bila provider = mise. Kartu Dashboard dan menu tray otomatis ikut.
- **Mengganti versi** (`plan_use`, diuji unit):
  - Global → `mise use -g`.
  - Project → `mise use --path <file yang sedang berlaku>`.
  - Tool yang masih dari global memicu dialog "Pin in this project / Change global". Pin ditulis ke config project yang **sudah** dipakai tool lain (mis. `D:\dev\event\mise.toml`), bukan file baru di subfolder; `mise.toml` di root project hanya dibuat bila belum ada config project sama sekali.
- **Ketahanan**:
  - Error "not trusted" memunculkan banner dengan tombol Trust (`mise trust <file>`).
  - Settings menyediakan Reshim / Clear cache / Doctor.
  - Error mise tampil apa adanya; baris WARN (mis. notifikasi update) diabaikan.
  - Validasi versi menolak apa pun selain angka bertitik.
- **Config v6**: `projects`, `activeProjectId`, `versionProvider` (`auto` | `mise` | `legacy`). `watchedPath` lama dimigrasikan menjadi project pertama.
- **UI**:
  - Dashboard: bar "Versions for" (Global / project) dan label sumber versi di kartu PHP/Node.
  - Settings → PHP & Node: pemilih provider, info mise, aksi pemulihan, daftar project (tambah lewat dialog folder).
  - Onboarding: satu kartu mise menggantikan phpvm + fnm.

### Perlu verifikasi di Windows

- `mise use --path <file>` benar-benar memperbarui file yang ditunjuk (bukan membuat baru) di mise 2026.11.x.
- Format pesan error "not trusted" (pola yang dikenali: `Config files in <path> are not trusted`).
- `mise install` dari Servel untuk versi yang belum terpasang.

## Berikutnya (kandidat)

- **v1.8** — Wizard migrasi data Docker → WSLC: dump dari volume Docker lalu restore ke container WSLC, dengan pola `servel.ps1 restore`.
- **v1.8** — Readiness check per service (mis. `mysqladmin ping`, `pg_isready`) sebelum status dianggap "up".
- **v2.0** — WSLC menjadi default di Windows setelah `wslc compose` resmi tersedia ([microsoft/WSL#40948](https://github.com/microsoft/WSL/issues/40948)) atau endpoint Docker API ([microsoft/WSL#40976](https://github.com/microsoft/WSL/issues/40976)).
- Code signing Windows dan notarization macOS (tertunda sejak v1.0).
