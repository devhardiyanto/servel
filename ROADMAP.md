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

- **SQL Server dihapus** dari daftar service (image ~1.5 GB, jarang dipakai); id `sqlserver` dibuang dari profil tersimpan saat migrasi config.
- **Log tidak lagi menandai progress sebagai error**: `wslc`, `mise`, dan `docker compose` menulis progress (pull, install) ke stderr. Baris itu kini tampil sebagai output biasa; hanya pesan kegagalan final yang ditandai error.
- Tab Logs: PHP / Node / mise (menggantikan phpvm / fnm).

### Terverifikasi di Windows (2026-11-10)

- Onboarding: WSL containers + mise terdeteksi (2/2 ready).
- Container buatan `servel.ps1` diadopsi apa adanya dan di-start lewat `wslc start`; service baru dibuat dengan `volume create` + `run` (pull image berjalan).

### Perlu verifikasi di Windows

- `mise use --path <file>` benar-benar memperbarui file yang ditunjuk (bukan membuat baru) di mise 2026.11.x.
- Format pesan error "not trusted" (pola yang dikenali: `Config files in <path> are not trusted`).
- `mise install` dari Servel untuk versi yang belum terpasang.

## v1.8.0 — data & kesehatan service

### Selesai

- **Readiness check** (`runtime/readiness.rs`):
  - Probe di dalam container: `mysqladmin ping`, `pg_isready`, `redis-cli ping`, `mongosh` ping, `rabbitmq-diagnostics ping`, health MinIO.
  - Status "up" baru dikirim setelah probe lolos; hasilnya di-cache sampai container berhenti.
  - Exit 126/127 (tool tidak ada di image) dan batas 120 dtk dianggap siap.
- **Restore database** (`runtime/restore.rs`, Settings → Services):
  - Start service, tunggu readiness, salin dump, lalu jalankan `mysql` / `psql -v ON_ERROR_STOP=1`.
  - WSLC: `container cp` ke `/tmp/` dengan cwd folder file, fallback stdin. Docker: `docker cp`.
- **Import from Docker** (`runtime/migrate.rs`):
  - MySQL: dump semua database kecuali database sistem.
  - PostgreSQL: `pg_dump -C` per database (bukan `pg_dumpall`, yang memuat `CREATE ROLE postgres`).
  - Hasil dump di-restore ke WSLC; file dump sementara dihapus.

### Perlu verifikasi di Windows

- Restore `.sql` nyata ke MySQL dan PostgreSQL di WSLC.
- Import from Docker (butuh mesin yang masih punya Docker Desktop + data Servel).

## v1.9.0 — project sebagai pusat

### Selesai

- `Project.profileId`: memilih project di Dashboard juga menerapkan profil service-nya. Hanya selection yang berubah; container yang jalan tidak di-stop.
- CLI `servel use <tool> <versi> [--global]` (target sama dengan `plan_use`), `servel projects` (read-only).

### Ditunda

- Menautkan Sites ke project (dikerjakan di v2.2 dengan melebur keduanya).

## v2.0.0 — default baru

### Selesai

- **WSL containers default di Windows** (`runtime::resolve`): Auto memilih WSLC kecuali Docker masih menyimpan container `servel_*`. Daemon Docker yang mati / sisa `docker.exe` tidak lagi menahan pengguna di Docker; data yang mungkin masih ada di sana bisa diambil lewat Import from Docker. Resolusi ulang hanya saat preferensi berubah, supaya simpan config tidak menunggu `docker ps`.
- **mise di macOS/Linux**: modul `mise` sudah lintas platform (HOME, PATH login shell); ditambah test path Unix.
- Clippy bersih tanpa warning.

### Tidak bisa dikerjakan dari sisi kode

- **Code signing Windows / notarization macOS**: butuh sertifikat Authenticode dan akun Apple Developer milik maintainer. Setelah tersedia, rahasianya cukup ditambahkan ke workflow `release`.
- **`wslc compose` / endpoint Docker API**: menunggu Microsoft ([microsoft/WSL#40948](https://github.com/microsoft/WSL/issues/40948), [#40976](https://github.com/microsoft/WSL/issues/40976)). Servel tidak bergantung pada keduanya.

## Rilis

v1.6–v1.9 tidak dirilis terpisah. Tag `v2.0.0` menghasilkan satu rilis yang mencakup semuanya; changelog otomatis disusun dari `v1.5.0..v2.0.0`. Catatan rilis: `docs/releases/v2.0.0.md`.

## v2.1.0 — update dari dalam aplikasi

**Selesai**

- `tauri-plugin-updater`: command `update_check` / `update_install`, progress lewat event `update-progress`, restart setelah pasang.
- Settings → About: tombol **Update sekarang** dengan progress. Pengecekan GitHub API lama dipertahankan sebagai cadangan (banner "Buka halaman rilis") bila updater gagal.
- `release.yml`: tanda tangan file update + `latest.json` diunggah ke release; gagal lebih awal bila pubkey masih penanda atau secret belum ada.
- `build.yml`: build PR tidak menandatangani (`createUpdaterArtifacts: false`).
- Target bundle `app` ditambahkan supaya macOS menghasilkan `.app.tar.gz` untuk updater.

**Perlu verifikasi (hanya bisa lewat rilis nyata)**

- Alur ujung ke ujung: v2.1.0 terpasang → rilis berikutnya dipublish → Update sekarang → restart ke versi baru (Windows, macOS, AppImage).
- `latest.json` memuat keempat target (Windows, macOS arm64/x64, Linux) setelah matrix release selesai.
- Hook NSIS (PATH `servel`) tetap berjalan saat installer dipanggil oleh updater.

**Batasan**

- v2.0.0 → v2.1.0 tetap install manual; updater berlaku mulai v2.1.
- Private key updater hilang = pengguna terpasang tidak bisa auto-update lagi.
- Bukan code signing: SmartScreen/Gatekeeper tetap muncul.

## v2.2.0 — project dijalankan di localhost

**Selesai**

- Satu daftar **Projects** (Settings → Projects): folder, profil service, perintah dev server, port tetap, auto-start. Daftar "Projects" di PHP & Node dilebur ke sini.
- Deteksi jenis project → usulan perintah dan port: Laravel, Lumen, WordPress, Drupal, PHP polos, Vite (Vue/React/Svelte), Next, Nuxt, Angular, Node lain, Go (`air` bila ada). Package manager dari lockfile.
- Start/Stop/Open/Log per project. Perintah dijalankan lewat shell di folder project dengan environment `mise env --json`, jadi versi mengikuti `mise.toml`. Env `PORT` selalu di-set.
- Status: starting → running (port menjawab) / port tidak menjawab setelah 90 dtk / exited (exit code + baris output terakhir).
- Stop mematikan seluruh pohon proses (`taskkill /T /F` di Windows, process group di Unix). Semua dev server dihentikan saat Servel keluar.
- **Fitur domain `.test` dihapus** (hosts, reverse proxy Caddy, sertifikat lokal). Site lama dipindah ke Projects (domain menjadi nama, port dipertahankan, folder dipilih ulang). Banner di Projects menawarkan pembersihan blok hosts dan sertifikat lama.
- Config v7; dependensi unduh/ekstrak Caddy dicabut.
- CLI: `servel projects` menampilkan URL localhost tiap project.

**Perlu verifikasi di Windows**

- Start/Stop sungguhan: `cmd /S /C` dengan perintah berisi tanda kutip, dan `taskkill /T /F` membebaskan port (`npm` → `node` → `vite`). Di Linux sudah diuji ujung ke ujung lewat unit test.
- `mise env --json` di mise 2026.11.2: bentuk output dan PATH yang dihasilkan.
- Pembersihan sisa lama: hapus blok hosts (UAC) dan hapus sertifikat Caddy dari `CurrentUser\Root` (dialog konfirmasi Windows).
- Updater v2.1.0 → v2.2.0 (uji pertama alur update).

**Batasan**

- Node backend dan Go hanya mengikuti port bila aplikasinya membaca `PORT`; kalau tidak, isi port aplikasinya di Edit.
- `php -S` / `artisan serve` melayani satu request pada satu waktu di Windows.
- Dependensi project (`vendor`, `node_modules`) tidak dipasang otomatis.
- Bila Servel mati paksa (crash/kill), dev server yang sedang jalan tidak ikut dihentikan.
- `%AppData%\Caddy` (data Caddy, termasuk kunci CA lama) tidak dihapus karena bisa dipakai instalasi Caddy lain.

## Setelah v2.2 (kandidat)

- Patch dari pemakaian lapangan (item "Perlu verifikasi" di atas).
- Code signing begitu sertifikat tersedia.
