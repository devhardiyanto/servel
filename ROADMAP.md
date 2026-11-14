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

**Verifikasi di Windows** — dicatat selesai; akan dipantau lagi ketika aku sedang menjalankannya

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

**Verifikasi di Windows** — dicatat selesai; akan dipantau lagi ketika aku sedang menjalankannya

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

## v2.2.1 — patch dari pemakaian

**Selesai**

- Tombol Restart per project (menunggu port bebas, maks 15 dtk).
- Environment per project (`KEY=VALUE`), dipasang di atas environment mise; `PORT` dicadangkan.
- Updater menghentikan dev server setelah unduh dan sebelum install (installer Windows menutup Servel tanpa menjalankan hook keluar).
- Remove certificate: pisahkan store pengguna (`certutil -user -delstore`, dialog konfirmasi) dan store komputer (certutil lewat UAC); pesan error menyebut store dan alasannya.
- Onboarding: label "checking…" dengan 2 kartu skeleton; konten di tengah vertikal.

**Verifikasi di Windows** — dicatat selesai; akan dipantau lagi ketika aku sedang menjalankannya

- Remove certificate (dialog konfirmasi / UAC) sampai banner hilang.
- Update sekarang v2.2.0 → v2.2.1 dengan project berjalan: setelah Servel terbuka lagi, Start tidak gagal "port sedang dipakai".
- `SSL_CERT_FILE` lewat Environment menyelesaikan error 60 di PHP.

## v2.3.0 — PHP extensions, sertifikat HTTPS, dev server yatim

**Selesai**

- `php_ext`: daftar extension per versi PHP (file di folder extension + `php -m`), toggle lewat `php.ini` (uncomment baris yang ada atau tambah `extension=`/`zend_extension=`). Backup 5 terakhir; setelah menulis, PHP dijalankan untuk verifikasi dan `php.ini` dikembalikan bila ada peringatan startup atau status tidak berubah. `extension_dir` absolut di Windows bila bawaannya relatif.
- Fix HTTPS certificates (Windows): `curl.cainfo` + `openssl.cafile` ke bundle Git for Windows, atau ekspor root Windows ke folder data Servel bila Git tidak ada.
- Dev server yatim: Windows memakai job object `KILL_ON_JOB_CLOSE`; semua platform mencatat PID + waktu mulai proses ke `running-projects.json` dan mematikan sisa sesi saat Servel dibuka (hanya bila waktu mulai cocok).
- Uji ujung ke ujung dengan PHP sungguhan (aktifkan, nonaktifkan, rollback extension rusak) dan dengan proses sisa (dimatikan vs PID dipakai ulang).

**Verifikasi di Windows** — dicatat selesai; akan dipantau lagi ketika aku sedang menjalankannya

- Toggle extension pada PHP dari mise (mis. intl, pdo_pgsql), termasuk saat `php.ini` belum ada.
- Fix HTTPS certificates lalu panggilan HTTPS dari PHP berhasil tanpa `SSL_CERT_FILE`.
- Job object: matikan Servel dari Task Manager saat project berjalan → port project bebas.

**Batasan**

- Ekspor root Windows bisa kurang lengkap (Windows mengunduh sebagian root hanya saat dibutuhkan); bundle Git lebih lengkap.
- Proses anak yang dibuat dalam milidetik pertama sebelum masuk job object bisa lolos; catatan di disk menjadi cadangannya.

## v2.3.1 — patch dari uji Windows

**Selesai**

- Path `php.ini` dari `php --ini` PHP 8.5 dikutip; kutip dibuang (di v2.3.0 simpan gagal dengan os error 123 dan php.ini dianggap belum ada).
- Output progress mise di Logs dibersihkan dari bar, spinner, dan kode ANSI/OSC.

**Verifikasi di Windows** — dicatat selesai; akan dipantau lagi ketika aku sedang menjalankannya

- Toggle extension dan Fix HTTPS certificates pada PHP 8.5.

## v2.4.0 — log per project, start project + service

**Selesai**

- Logs: chip filter per project di tab Projects (dengan jumlah error), tombol "Errors & warnings" di semua tab.
- `line_level`: baris output dev server ditandai error/warn dari isinya (heuristik kata kunci; "0 errors" tidak dihitung).
- Buffer log per sumber (500 baris per sumber, pengaman total 4000) menggantikan 400 baris bersama.
- `Project.start_services`: Start/Restart/auto-start ikut menyalakan service di profil project, di latar belakang; service lain tidak disentuh dan Stop project tidak mematikan service. Default mati.

**Verifikasi di Windows** — dicatat selesai; akan dipantau lagi ketika aku sedang menjalankannya

- Start project dengan opsi service aktif saat container belum jalan (WSL containers).
- Filter per project dan "Errors & warnings" dengan dua dev server berjalan.

## v2.5.0 — start/stop/restart project dari CLI

**Selesai**

- `control`: kanal kontrol lokal. Aplikasi mendengarkan di `127.0.0.1:<port acak>`, menulis `control.json` (port, token acak per sesi, pid) ke folder data. Satu baris permintaan `<token> <aksi> [project]`, satu baris JSON balasan. Aksi: start, stop, restart, status.
- CLI: `servel start|stop|restart <project>` dan `servel ps`. Project dicari dari nama (tanpa beda huruf besar/kecil) atau id; nama ganda ditolak.
- Start/restart dari CLI memakai jalur yang sama dengan UI, termasuk opsi service profil.

**Verifikasi di Windows** — dicatat selesai; akan dipantau lagi ketika aku sedang menjalankannya

- Keempat perintah dari PowerShell 5.1, pwsh 7, cmd, dan Git Bash (skrip CLI tidak bisa dijalankan di lingkungan pengembangan; hanya protokolnya yang diuji).
- `control.json` sisa sesi yang mati paksa → pesan "aplikasi Servel tidak berjalan".

## v2.6.0 — unduh extension PECL (Windows)

**Selesai**

- `pecl`: cari rilis stabil terbaru di downloads.php.net yang punya build untuk PHP ini (mayor.minor, TS/NTS, x64/x86; compiler diambil dari nama file, tidak ditebak), unduh, ekstrak `php_<ext>.dll` ke folder extension dan DLL pendukung ke folder `php.exe`.
- `php_ext_install`: pasang lalu aktifkan lewat `set_ext`; bila PHP menolak, DLL dihapus dan `php.ini` dikembalikan.
- UI: "Install another extension" di panel PHP extensions (hanya Windows).
- Diuji dengan unduhan sungguhan: redis 6.3.0 (PHP 8.5 NTS), xdebug 3.5.3 (PHP 8.3 TS), imagick 3.8.1 (PHP 8.3 NTS, 167 DLL pendukung).

**Verifikasi di Windows** — dicatat selesai; akan dipantau lagi ketika aku sedang menjalankannya

- Download & enable redis dan xdebug pada PHP dari mise; `php -m` memuatnya.
- imagick: DLL pendukung di folder `php.exe` cukup untuk memuat extension.

**Batasan**

- Tanpa checksum dari sumber; integritas hanya lewat HTTPS ke php.net.

## v2.7.0 — satu bahasa (English)

**Selesai**

- Semua teks yang dibaca pengguna menjadi English: label UI yang masih Indonesia (About/updater, profil, banner konflik port), seluruh pesan log dan error backend (±240 pesan), serta pesan dan bantuan CLI.
- Komentar kode dan pesan assert di test tetap Indonesia (bukan teks pengguna).
- Tidak ada perubahan perilaku, format file, atau nama perintah.

**Verifikasi di Windows** — dicatat selesai; akan dipantau lagi ketika aku sedang menjalankannya

- CLI: `servel help` dan pesan error tampil benar di PowerShell 5.1 (skrip tidak bisa dijalankan di lingkungan pengembangan).

## v2.7.1 — jendela putih saat start

**Selesai**

- Jendela mulai tersembunyi (`visible: false`) dengan latar gelap, dan ditampilkan lewat command `app_ready` setelah UI ter-mount. Pengaman: tampil sendiri setelah 8 detik bila UI gagal dimuat. `index.html` diberi latar gelap inline.

**Verifikasi di Windows** — dicatat selesai; akan dipantau lagi ketika aku sedang menjalankannya

- Start biasa dan start setelah update: tidak ada jendela putih; jendela muncul segera setelah UI siap.
- Tutup ke tray lalu Show dari tray tetap bekerja.

## Status per 14 November 2026

Semua item fitur v2.1–v2.7.1 dicatat selesai. Item verifikasi Windows di tiap versi tidak lagi menjadi pekerjaan terbuka: akan dipantau lagi ketika aku sedang menjalankannya, dan temuan masuk sebagai patch.

## Milestone menuju v3.0.0 (Kova)

Keputusan 14 November 2026: nama **Kova**, logo tanda K tiga garis, UI di canvas "Servel v3 UI" disetujui, token baru (`faint`, `surface3`, `--src-*`) disetujui, dialog yang belum digambar mengikuti pola modal yang ada, Ctrl K dan CLI lintas platform di v3.1, alias `servel` hidup selama v3.x, "Start Kova when Windows starts" masuk v3.0 (default off), uji v3.0.0-rc oleh pemilik repo. Rencana rinci: PRD dan dokumen Rencana & Milestone (privat).

Urutan: A → B → C → D → E → F → G. Tiap milestone satu PR; tag dan publish oleh pemilik repo. Tampilan tetap Servel sampai E.

### A — v2.8.0: fondasi dan pembersihan aman (PR ini)

- [x] Token baru di `tokens.css`: `--surface3`, `--faint`, skala tipe `--text-xs…xl`, `--dur-fast/base` (+ reduced motion), `--src-*`.
- [x] Satu peta warna log: `src/utils/logColors.ts` dipakai Logs dan LogTail.
- [x] Hapus wrapper `src/views/Logs.vue`.
- [x] Hapus file watcher `.phpvmrc`/`.nvmrc` (`watch_project` tidak pernah dipanggil; banner "Project suggests …" tidak pernah muncul), dependency `notify`.
- [x] CI: job `test` (vue-tsc + vite build + `cargo test`) sebelum build tiga OS.

### B — v2.9.0: komponen bersama

- [x] `src/components/ui/UiButton.vue`: varian default/accent/primary/success/danger/quiet, ukuran sm/md/lg, loading. Menggantikan 8 keluarga class tombol (`pp-btn`, `px-btn`, `action-btn`, `update-check-btn`, `sw-btn`, `ctx-btn`, `ob-btn`, `prereq-card__action-btn`) di 6 file.
- [x] `src/components/ui/UiToggle.vue` (role="switch", label aksesibel) menggantikan switch di Settings dan PHP extensions.
- [x] 131 `font-size` px dipetakan ke skala `--text-*` (9–12.5 px → 12, 13 → 13, 14 → 14, 16/18 → 17, 22 → 22).
- [x] Teks redup memakai `--faint` (kontras 5.1–5.6:1) menggantikan `--dim` (3.5–3.8:1).
- Dipindah ke E (belum ada pemakainya sekarang, dibuat bersama layar baru): Input/Select, SegmentedControl, Tabs, StatusDot, Chip/Badge, Banner, Toast, Menu ⋯, Modal, Drawer, EmptyState. Tombol log (`lv-btn`) dan pemilih service (`svc-toggle`) ikut diganti di E karena layarnya dirancang ulang.

### C — v2.10.0: fitur project dan service

- [x] Panel Connect per service (`ServiceConnect.vue`, `utils/connect.ts`): host, port, database, user, password, URL; Copy .env (key Laravel), Copy URL, Open web UI (RabbitMQ, MinIO, Mailpit).
- [x] RAM per service: sudah tampil di daftar service sejak v1.x (angka live saat running, estimasi saat off). Ringkasan di status bar ikut layar baru di E.
- [x] Project: Open folder, Terminal here (Windows Terminal, cadangan PowerShell), Open in VS Code (`open_in.rs`; path selalu jadi current_dir/argumen utuh, tidak pernah masuk string shell).
- [x] Panel Details project (`ProjectDetail.vue`): versi runtime dari mise untuk folder itu (`mise_project_tools`), profil service, 20 baris output terakhir.

### D — v2.11.0: runtimes, on launch, CLI

- [x] Go dan Python lewat mise di Settings → Runtimes (versi berlaku, versi terpasang, Install, Use globally / Use in project). Command `runtime_versions/install/use` dengan allowlist tool. Deteksi project Python belum masuk (asumsi; bisa ditambah bila diminta).
- [x] Install mise satu klik di onboarding (Windows, `winget install --id jdx.mise --exact`); folder shim winget ditambahkan ke PATH proses supaya mise langsung terdeteksi. macOS/Linux: perintah resmi (`brew install mise` / `curl https://mise.run | sh`).
- [x] Settings → General disusun ulang: bagian **On launch** (Start Servel when you sign in, Start services, Remember last session, tautan ke opsi per project) dan **Window** (Minimize to tray). Start when you sign in: registry Run (Windows), LaunchAgent (macOS), autostart .desktop (Linux), default off; dibuka dengan `--hidden` sehingga tetap di tray bila tray tersedia. Ditulis tanpa tauri-plugin-autostart karena plugin itu menuntut upgrade tauri/wry.
- [x] CLI: `servel use php@8.4` (bentuk lama tetap jalan), alias `servel service up|down|status|list|logs|shell`.
- [x] `release.yml`: draft dibuat satu kali di job awal (`gh release create --draft`), job build mengunggah ke `releaseId` itu. Re-run memakai draft yang sudah ada.
- Nav "PHP & Node" berganti nama menjadi "Runtimes".

### E — v3.0.0-rc.1: Kova (pra-terbit)

- [ ] Nama Kova di productName, judul jendela, tray, installer, README; bundle id, `servel_*`, kunci updater tetap.
- [ ] Logo K: ikon aplikasi semua ukuran, tray, favicon.
- [ ] UI baru sesuai canvas: navigasi kiri (Projects pertama), panel detail kanan, status bar, onboarding baru.
- [ ] CLI `kova` + alias `servel` dengan pemberitahuan.
- [ ] Hapus: sisa `.test` (legacy_*, hosts.rs), Import from Docker, cadangan cek update via GitHub API, provider phpvm/fnm.
- [ ] Uji Windows oleh pemilik: update dari Servel 2.x (build uji khusus, karena `releases/latest` melewati pra-terbit), folder NSIS, Apps & features, PATH CLI, data volume.

### F — v3.0.0: rilis

- [ ] Perbaikan dari uji rc, catatan rilis, screenshot baru di `docs/screenshots/`.
- [ ] Setelah rilis: ganti nama repo (GitHub mengalihkan URL lama; jangan membuat repo baru bernama `servel`).

### G — v3.1: lanjutan

- [ ] Palet perintah Ctrl K.
- [ ] CLI lintas platform (macOS/Linux).
- [ ] Code signing begitu sertifikat tersedia.
