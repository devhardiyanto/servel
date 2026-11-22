# Kova Roadmap

Kova (dulu **Servel**, berganti nama di v3.0.0). Status per versi. Item yang sudah rilis tetap dicatat supaya keputusan desainnya bisa ditelusuri; item yang sudah selesai atau usang di bagian lama diringkas dan diberi rujukan versi. Pekerjaan yang masih terbuka ada di [Status per 22 November 2026](#status-per-22-november-2026) di akhir file.

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

### Verifikasi di Windows

- Terverifikasi di WSL 3.0.5: bentuk NDJSON `wslc container list --all --format json` dan `wslc stats --format json` (fixture di `src-tauri/src/runtime/fixtures/`); `wslc stop -t 30` (21 November 2026, v3.8.0).
- CLI di PowerShell: dicatat lulus pada uji v2.5–v2.7 dan uji rc.1/rc.2 (alias `servel`, `kova.exe`).
- Masih terbuka (prioritas rendah, lihat status terbaru): `wslc remove -f` saat definisi service berubah, dan apakah `.wslconfig` memengaruhi VM WSLC.

### Batasan yang diketahui (masih berlaku per v3.15.3)

- WSLC: tidak ada network antar-container (Docker memakai `servel_default`). Service diakses dari host lewat `127.0.0.1`. Sejak v3.15.3 Docker juga mem-publish port di `127.0.0.1` saja.
- WSLC: tidak ada restart policy, jadi service hidup kembali saat Kova auto-start, bukan saat Windows boot (Docker memakai `restart: unless-stopped`).
- Data Docker Desktop tidak dimigrasikan otomatis (Import from Docker dihapus di v3.0.0-rc.1).

## v1.7.0 — mise sebagai version manager

Disetujui: cakupan terpisah dari v1.6; Servel boleh mengganti versi per project lewat UI.

### Temuan dari mesin pengguna (Windows, mise 2026.11.2)

- mise mengelola **PHP dan Node** di Windows (php 8.3.33 / 8.3.35 / 8.5.11, node 20 / 22 / 24).
- Sumber versi bisa **berbeda per tool dalam satu project**. Contoh di `acme-api`: node dari `C:\dev\acme\mise.toml`, php dari global `~\.config\mise\config.toml`.
- Config berlaku **ke subfolder**: root project ≠ lokasi config.

### Selesai

- **`src-tauri/src/mise.rs`**:
  - mise dijalankan dengan cwd = project aktif (atau home untuk konteks Global); hanya output `--json` yang di-parse.
  - Fixture output asli: `ls --current`, `ls --installed php`, `--version` dengan banner.
- **Dispatch**: `php_*` / `node_*` (list, active, switch, install, hook) memakai mise bila provider = mise. Kartu Dashboard dan menu tray otomatis ikut.
- **Mengganti versi** (`plan_use`, diuji unit):
  - Global → `mise use -g`.
  - Project → `mise use --path <file yang sedang berlaku>`.
  - Tool yang masih dari global memicu dialog "Pin in this project / Change global". Pin ditulis ke config project yang **sudah** dipakai tool lain (mis. `C:\dev\acme\mise.toml`), bukan file baru di subfolder; `mise.toml` di root project hanya dibuat bila belum ada config project sama sekali.
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

### Verifikasi di Windows

- `mise install` dan `mise use --path` dari aplikasi dicakup test case TC-E1, TC-E2, TC-E5 (`docs/testing/test-cases.md`); belum dijalankan di putaran 1.
- Pesan "not trusted" kini ditangani banner **Trust** di Runtimes (`src/components/RuntimeTools.vue`), tetapi belum punya test case sendiri.

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

### Verifikasi

- Restore `.sql` nyata: dicakup TC-W4 / TC-L7, belum dijalankan.
- Import from Docker: usang, fitur dihapus di v3.0.0-rc.1 (E1).

## v1.9.0 — project sebagai pusat

### Selesai

- `Project.profileId`: memilih project di Dashboard juga menerapkan profil service-nya. Hanya selection yang berubah; container yang jalan tidak di-stop.
- CLI `servel use <tool> <versi> [--global]` (target sama dengan `plan_use`), `servel projects` (read-only).

- Menautkan Sites ke project: selesai di v2.2.0 (keduanya dilebur menjadi Projects).

## v2.0.0 — default baru

### Selesai

- **WSL containers default di Windows** (`runtime::resolve`): Auto memilih WSLC kecuali Docker masih menyimpan container `servel_*`. Daemon Docker yang mati / sisa `docker.exe` tidak lagi menahan pengguna di Docker; data yang mungkin masih ada di sana bisa diambil lewat Import from Docker (fitur itu dihapus di v3.0.0-rc.1). Resolusi ulang hanya saat preferensi berubah, supaya simpan config tidak menunggu `docker ps`.
- **mise di macOS/Linux**: modul `mise` sudah lintas platform (HOME, PATH login shell); ditambah test path Unix.
- Clippy bersih tanpa warning.

### Tidak bisa dikerjakan dari sisi kode (masih berlaku per v3.15.3)

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
- ~~Bila Servel mati paksa (crash/kill), dev server yang sedang jalan tidak ikut dihentikan.~~ Diatasi di v2.3.0 (job object Windows + `running-projects.json`).
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

### D.1 — v2.11.1: perbaikan workflow rilis

- [x] v2.11.0 terbit tanpa aset: `release_id` kosong karena draft baru belum muncul di daftar rilis. ID kini diambil dari respons `POST /releases`; job gagal bila ID bukan angka.
- [x] v2.10.0: build sukses, tetapi pembuatan rilis ditolak GitHub (403, workflow lama per-job). Tidak diterbitkan ulang; isinya tercakup di v2.11.x.

### E — v3.0.0-rc.1: Kova (pra-terbit)

- [x] E3 — nama Kova di productName, judul jendela, tray, installer hook, README, sumber log `KOVA`, judul rilis; bundle id, `servel_*`, kunci updater tetap. Versi 3.0.0-rc.1.
- [x] Logo K: ikon aplikasi semua ukuran (`tauri icon` dari SVG proposal); tray memakai ikon jendela.
- [x] E4a — kerangka baru: navigasi kiri (logo K; Projects, Services, Runtimes, Logs, Settings dengan badge jumlah), status bar (engine, service jalan, RAM, versi), Projects sebagai layar pertama setelah onboarding, Runtimes jadi halaman sendiri, Settings tinggal General/Services/About.
- [x] E4b — kartu versi PHP/Node + Versions for pindah ke Runtimes (`VersionsPanel`), Projects jadi daftar + panel detail kanan (`ProjectsView`), Connect service di panel kanan Services, onboarding dengan logo K. Kartu Session Stats dihapus (angkanya ada di navigasi dan status bar).
- [x] CLI `kova` (shim baru) + alias `servel` (shim memasang `KOVA_CLI_ALIAS`, skrip menulis pemberitahuan ke stderr).
- [x] E1 — hapus sisa `.test` (`hosts.rs`, `legacy_status/cleanup_hosts/remove_cert`, banner di Projects), Import from Docker (`runtime/migrate.rs`, `services_migrate_from_docker`), cadangan cek update via GitHub API (`useUpdateCheck`: hanya updater bawaan). Migrasi config v7 (site lama → project) tetap ada.
- [x] E2 — hapus provider phpvm/fnm: `php.rs`/`node.rs` hanya lewat mise (tanpa mise: daftar kosong, switch/install memberi pesan jelas), deteksi phpvm/fnm di prasyarat, kartu onboarding, banner `phpvm hook`, pilihan "Manage PHP & Node with". Field `versionProvider` di config dibiarkan agar file lama tetap terbaca.
- [x] Uji Windows oleh pemilik (rc.1): 18 lulus, 1 dilewati, 0 gagal. Temuan (dua aplikasi terpasang, alias tanpa pemberitahuan, `servel.exe`) diperbaiki di F0.

### F0 — v3.0.0-rc.2: perbaikan dari uji rc.1

- [x] Uji Windows rc.1: 18 lulus, 1 dilewati, 0 gagal (checklist interaktif).
- [x] Installer mencopot Servel 2.x diam-diam (`NSIS_HOOK_PREINSTALL`); data dan `servel_*` tetap.
- [x] `mainBinaryName: "kova"`; entri start saat login ditulis ulang saat start bila aktif.
- [x] Preview `.env` di Connect; tab Logs membungkus; kolom port disembunyikan saat tabel Services sempit (container query).
- [x] Pesan container adopsi sekali per sesi.

### F — v3.0.0: rilis

- [x] Uji Windows rc.2: installer mencopot Servel, alias, `kova.exe`, entri start saat login, tampilan lulus; folder instalasi lama tertinggal karena `hosts-backups`.
- [x] Installer menghapus `hosts-backups\hosts.servel.*.bak` lalu folder Servel lama (tanpa hapus rekursif).
- [x] Pesan container adopsi sekali per container (`wslc-adopted.json`).
- [x] Catatan rilis `v3.0.0.md`, screenshot baru di `docs/screenshots/`.
- [x] v3.0.0 terbit 15 November 2026 sebagai Latest.
- [x] Uji Windows setelah terbit: Servel 2.11.1 → Check for updates → Kova 3.0.0 lewat updater bawaan; hanya Kova di Apps & features; `%LOCALAPPDATA%\Servel` hilang; `servel status` menampilkan pemberitahuan alias; data terbawa.
- [ ] Ganti nama repo (oleh pemilik; GitHub mengalihkan URL lama; jangan membuat repo baru bernama `servel`). Setelah itu: URL repo di endpoint updater, README, dan tautan catatan rilis diperbarui lewat satu PR. *Per v3.15.3 masih terbuka: endpoint updater di `src-tauri/tauri.conf.json` masih menunjuk repo `…/servel`.*

## Status per 15 November 2026

Rebrand dan tampilan baru selesai dengan v3.0.0. Milestone A–F tertutup kecuali ganti nama repo. Yang belum teruji: build macOS dan Linux v3 (tidak ada mesin uji), dan jalur start saat login dari Servel 2.11 yang sudah aktif (di mesin uji fitur itu tidak pernah dinyalakan). *Pembaruan 22 November: Linux (AppImage + Docker Engine) mulai dipakai di v3.15.2–v3.15.3; macOS masih belum pernah diuji.*

## Setelah v3.0.0

Penomoran: v3.x menambah fitur tanpa memutus apa pun dari Servel. **v4.0 adalah rilis yang membuang lapisan kompatibilitas Servel**; belum dijadwalkan dan baru dikerjakan bila ada alasan (misalnya nama `servel_*` mulai menghalangi fitur).

### G — v3.1: lanjutan

- [x] G1 — Palet perintah Ctrl K / ⌘K (`CommandPalette.vue`, `utils/palette.ts`): pindah layar; start/stop service (sama dengan switch: start ikut memilih, stop melepas pilihan), Start selected, Stop all; project Start / Open / Restart / Stop sesuai keadaan. Tombol "Commands" di navigasi kiri. Pilihan service kini dipulihkan saat aplikasi dibuka, bukan saat layar Services dibuka.
- [x] G2 — CLI `kova` untuk macOS dan Linux (`assets/cli/kova.sh`, POSIX sh). Dipasang dari Settings → General → Terminal ke `~/.local/bin/kova` (`cli_install.rs`; skrip ditanam di biner, file asing bernama `kova` tidak ditimpa). `up`/`down` dan perintah project dijalankan aplikasi lewat kanal kontrol (aksi baru `up`, `down`, `services`, dan balasan teks `plain`); `status`/`logs`/`shell` langsung ke Docker. Tidak ada `restore` dan `list` di CLI ini (ditambah di v3.3.0). Belum diuji di macOS/Linux sungguhan.
- G3 — Code signing: **ditunda**. SignPath Foundation menolak pengajuan (Juni 2026: proyek belum punya cukup adopsi publik). Diajukan ulang setelah ada adopsi, atau dibuka lagi bila ada pengguna yang terhalang peringatan SmartScreen. Pilihan berbayar (Certum Open Source, Azure Trusted Signing, SignPath berlangganan) belum diperiksa syarat dan harganya.

### H — v3.2.0: kelengkapan UI dari canvas

Dua PR menuju satu rilis. Versi dinaikkan di PR 2.

PR 1:

- [x] Pintasan browser bawaan webview dimatikan di build rilis (`utils/webviewGuards.ts`): cetak, muat ulang, cari di halaman, lihat sumber, simpan, zoom (tombol dan Ctrl + roda), devtools, riwayat (Alt ←/→, tombol mouse), F3/F5/F7/F12, menu konteks di luar kolom teks, seret-lepas file, klik tengah. Hanya perilaku bawaan yang dimatikan; Ctrl K, salin/tempel, dan Ctrl A di area log tetap bekerja. Belum diverifikasi di WebView2 sungguhan.
- [x] "View log" di kartu project yang gagal atau tidak menjawab: membuka Logs → Projects dengan filter project itu.
- [x] Filter teks di Logs (tiap kata harus ada di baris atau sumbernya), Ctrl F memfokuskannya, Esc mengosongkan; footer menampilkan "N of M lines".

PR 2:

- [x] Komponen bersama di `src/components/ui/`, mengikuti spesifikasi canvas (Controls, Patterns): `UiInput` (input/textarea/select + label, bantuan, error), `UiSegmented`, `UiTabs`, `StatusDot`, `UiChip` (chip dan badge), `UiBanner`, `ToastHost` + `useToast`, `UiMenu`, `UiModal`, `UiDrawer`, `EmptyState`. Modal dan drawer mengunci fokus, Esc menutup, fokus kembali ke pemicu (`utils/focusTrap.ts`).
- [x] Projects: menu ⋯ per kartu (Open folder, Terminal here, Open in VS Code, Copy URL, Project settings…, Remove project…); form edit pindah ke drawer "Project settings"; hapus lewat dialog konfirmasi dengan fokus awal di Cancel (menggantikan dialog sistem); banner untuk project yang keluar atau tidak menjawab; state kosong, memuat (skeleton), dan error baca config.
- [x] Services: status memakai `StatusDot` (bentuk berbeda per status); engine tidak merespons, konflik port, dan gagal start memakai `UiBanner`; Copy .env / Copy URL memberi toast.
- [x] Logs: tab memakai `UiTabs` (panah kiri/kanan); state kosong dan "tidak ada yang cocok" memakai `EmptyState`.
- [x] Runtimes: dialog scope versi memakai `UiModal`. Settings: pilihan engine dan target restore memakai `UiSegmented`.
- Sisa yang belum memakai komponen bersama (kolom isian Runtimes/PHP extensions, kartu prasyarat, `lv-btn`, `svc-toggle`) dipindah di v3.3.0 (I).

### H.1 — v3.2.1: satu instance

- [x] Kova yang dibuka kedua kali meminta instance pertama menampilkan jendelanya (aksi `show` di kanal kontrol), lalu keluar. Dicek paling awal di `run()`, sebelum Tauri dibangun. Start saat login (`--hidden`) hanya memeriksa, tanpa memunculkan jendela. `control.json` dihapus sebelum restart setelah update supaya proses baru tidak menyerahkan diri ke proses lama.
- [x] v3.2.0: `latest.json` terbit dengan 4 dari 8 platform karena job build menulisnya bergantian; diganti manual di rilis. Workflow diperbaiki di v3.3.0 (`latest.json` disusun sekali).

### I — v3.3.0: Python, CLI Unix, komponen bersama, aksesibilitas

- [x] Deteksi project Python: Django, FastAPI, Flask, skrip biasa; interpreter mengikuti uv, poetry, pipenv, atau `.venv`.
- [x] `kova list` dan `kova restore` di macOS/Linux (aksi `list` baru di kanal kontrol).
- [x] Sisa kolom isian (Runtimes, PHP extensions, profil, filter log), tombol log, kartu prasyarat, dan saklar service dipindah ke komponen bersama.
- [x] Audit aksesibilitas WCAG 2.1 A/AA dengan axe-core di 12 layar: tanpa temuan setelah perbaikan kontras `--faint`, saklar service, pemilihan project lewat keyboard, dan nama aksesibel.
- [x] Workflow rilis: tag dicek terhadap versi, "Run workflow" membuat tag, `latest.json` disusun sekali.
- Uji di perangkat nyata v3.2.x–v3.3.0: digabung ke putaran uji v3.15.3 (lihat status 22 November).

### J — v3.4.0: Projects, Add project, onboarding, update (kesesuaian canvas)

Audit per artboard (17 Nov) menemukan 12 dari 45 langkah flow sesuai canvas, 22 sebagian, 11 berbeda. Fungsinya ada, tetapi tata letak tiap layar belum mengikuti desain. J dan K menutup selisih itu.

- [x] Projects: kartu ringkas, aksi di panel detail, tab Overview / Environment / Logs, editor variabel dengan Save & restart, banner error dengan View log / Environment / Restart, pencarian, strip service.
- [x] Modal Add project: Detecting, hasil deteksi dengan berkas penanda dan versi mise, cek port dengan "Use <port>", Save & start.
- [x] Onboarding tiga langkah, "I'll install it myself", langkah First project.
- [x] Banner update global: unduh di latar, Later / Restart now, Try again.

### K — v3.5.0: Runtimes, Services, Logs, Settings (kesesuaian canvas)

- [x] Runtimes bertab per tool dengan tabel versi (Used by, scope), menu ⋯ per versi, dialog Use in project, progres install dengan Cancel.
- [x] Panel service bertab (Connect, Data, Logs) dengan format .env; state kosong dan error Services.
- [x] Logs: Open project, Copy, state "Lost the log stream".
- [x] Settings: Diagnostics / Copy info.
- Uji di perangkat nyata v3.4.0–v3.5.0: unduh lalu restart dan Logs diuji 21 November (v3.7.x); Cancel install dan mise use/uninstall masuk TC-E2/TC-E5 putaran v3.15.3.

"Lost the log stream" untuk service tidak dibuat: Kova tidak men-stream log container. Padanannya banner project berhenti di Logs.

Sengaja tidak diikuti dari canvas: Settings satu halaman (sub-navigasi dipertahankan), daftar sumber Logs di kiri (tab dipertahankan), tab profil di Services (dropdown dipertahankan), gambar dialog folder (dialog OS dipakai).

### K.1 — v3.5.1: Quit dan onboarding

- [x] Quit Kova menghentikan dev server lalu service (batas 60 detik), dengan layar "Stopping services…"; bisa dimatikan di Settings.
- [x] Onboarding hanya sampai diselesaikan sekali; pengguna lama dianggap sudah. "Run setup again" di Settings.

### L — v3.6.0: tray, ikon, aksesibilitas (penutup revamp canvas)

Canvas disinkronkan ke keputusan 17 Nov dulu (dropdown profil, tab Logs, sub-navigasi Settings); yang tersisa di canvas adalah lingkup ini.

- [x] Menu tray baru: versi, project dengan status (klik = buka atau Start), ringkasan service dengan Start/Stop, PHP/Node global, Open Kova, Quit Kova. Dibangun ulang saat status atau config berubah.
- [x] Ikon di nav rail.
- [x] Perubahan status project diumumkan (`role="status"`, error lewat `role="alert"`); badge nav punya teks untuk pembaca layar.
- [x] Baris `mise ERROR`/`✗` ditandai error dan `mise WARN` peringatan; pesan error tidak lagi "Run with --verbose…"; uninstall versi yang sedang dipakai menjelaskan penyebabnya.
- Uji pembaca layar NVDA: masih terbuka, TC-U5 (lihat status 22 November).

### L.1 — v3.6.1: patch dari pemakaian

- [x] Toggle service cepat (on lalu off, atau sebaliknya) tidak lagi macet di "starting"/"stopping".
- [x] Menu tray tanpa baris abu-abu yang hanya label; versi dan jumlah service di tooltip.
- [x] Settings › About tidak menduplikasi banner update; sub-navigasi Settings tanpa latar sendiri.
- [x] Projects tanpa project: panel detail dan tombol Add project ganda tidak ditampilkan.
- [x] Pemisah panel yang bisa diseret (`col-resize`) di Projects dan Services; seret habis ke kanan menutup panel.

### M — v3.7.0: popup tray

- [x] Klik kiri ikon tray membuka popup seperti canvas (status berwarna, port rata kanan, Start/Stop); klik kanan tetap menu native.
- [x] Posisi dihitung dari kotak ikon dan work area monitornya: taskbar bawah, atas, kiri, kanan, monitor kedua (diuji unit).
- [x] Hilang fokus = tertutup; klik ikon yang menutupnya tidak langsung membukanya lagi (jeda 300 ms).
- [x] Menutup jendela utama dengan "Minimize to tray" mati tetap keluar lewat jalur Quit (popup tersembunyi tidak menahan aplikasi).
- [x] Badge "active" di Settings › Services memakai chip standar.
- Uji di Windows: posisi, DPI 125%, klik ulang ikon lolos 21 November (v3.7.3–v3.7.4). Multi-monitor masih terbuka.

### M.1 — v3.7.1: toggle service saat hidup

- [x] Start/stop service berjalan dalam satu antrean berurutan; stop yang datang selagi start berjalan tidak lagi jadi no-op.
- [x] Jaring pengaman UI: status transisi yang bertentangan dengan kenyataan lama setelah perintahnya selesai (stop 15 s, start 120 s) diganti status sebenarnya.

### M.2 — v3.7.2: deteksi engine tanpa restart, klik kanan tray, Logs

- [x] Windows: folder baru di PATH sistem/user (mis. `C:\Program Files\WSL` setelah `wsl --update`) ditambahkan ke PATH proses saat memeriksa engine, jadi `wslc` atau `docker` yang baru dipasang terdeteksi tanpa menutup paksa Kova.
- [x] Preferensi engine Auto diperiksa ulang bila engine yang terpilih tidak terpasang.
- [x] Klik kanan ikon tray juga membuka popup (Windows, macOS). Menu native hanya di Linux.
- [x] Popup tray: bagian Global versions (PHP, Node) dengan `mise use -g`; Runtimes memuat ulang lewat event `runtime-global-changed`.
- [x] Submenu PHP/Node di menu tray native kini berefek (dulu event-nya tidak ditangani; `usePhp`/`useNode` yang tidak terpakai dihapus).
- [x] PATH dibaca ulang sebelum setiap perintah di Windows (registry, maks. sekali per 5 s) dan saat memeriksa engine di Linux/macOS; jendela utama memeriksa ulang engine dan mise saat kembali difokuskan.
- [x] Logs: banner update tidak lagi memotong baris terbaru; latar banner menyatu dengan header Logs.

### M.3 — v3.7.3: onboarding saat WSL containers sudah ada

- [x] Docker tidak terpasang + WSL containers tersedia: tanpa `wsl --update`, tawarkan Use WSL containers atau Docker Desktop.
- [x] Layar onboarding menutupi seluruh jendela (`position: fixed`).

### M.4 — v3.7.4: tautan keluar

- [x] `utils/openExternal` (plugin opener, `window.open` hanya cadangan) untuk onboarding, kartu prasyarat, panduan mise, dan Release notes.

## Status per 20 November 2026

Revamp canvas selesai dengan v3.6.0 (L): tray, ikon, dan aksesibilitas menutup selisih terakhir antara canvas dan aplikasi. v3.6.1–v3.7.4 (L.1, M–M.4) adalah lanjutan dari pemakaian: toggle service cepat yang tidak lagi macet, panel detail yang bisa diseret dan ditutup, popup tray (klik kiri dan kanan di Windows/macOS, menu native hanya di Linux) dengan versi global PHP/Node, engine dan mise yang baru dipasang terdeteksi tanpa restart, onboarding yang tidak meminta `wsl --update` bila WSL containers sudah ada, dan tautan keluar yang membuka browser sistem.

Diuji di Windows (21 November 2026, v3.7.3–v3.7.4): restart setelah update, popup tray pada skala 125%, klik ulang / klik di luar / Esc menutup popup, klik kanan membuka popup, mengganti versi global PHP/Node dari popup (Runtimes ikut berubah), toggle service cepat saat hidup, wslc terdeteksi tanpa menutup Kova, tombol Docker Desktop ↗ membuka browser, dan baris terakhir Logs saat banner update. Semua lolos.

Belum diuji: popup tray di multi-monitor, pembaca layar NVDA.

### M.5 — v3.7.5: auto-start yang jujur

Ditemukan saat uji: setelah restart, toggle Redis menyala dan log menulis `auto-started: redis`, tetapi Redis tidak berjalan.

- [x] Auto-start memulihkan pilihan service sendiri sebelum start (dulu bergantung pada pemulihan di App.vue yang berjalan bersamaan; bila kalah cepat, `start()` tidak melakukan apa-apa).
- [x] `start()` mengembalikan hasil (started / already running / port conflict / failed); log auto-start mencatat hasil sebenarnya.
- [x] Engine yang baru menyala ditunggu sampai 60 detik, bukan langsung dilewati.
- [x] Banner hasil auto-start (menunggu engine, sedang start, dilewati, gagal) dengan Retry, Open Services, atau Engine settings; berhasil = toast.
- [x] Status engine `not installed` (bukan "checking" atau "not responding") di status bar dan Services; status bar bisa diklik ke Settings saat engine bermasalah.

### N — v3.8.0: keadaan UI yang jelas

- [x] C4 `UiStatusNotice`: satu pola untuk proses yang sedang berjalan, berhasil, dilewati, atau gagal (Retry, tutup, aksi tambahan). `UiBanner` mendapat tone `pending` (ikon berputar).
- [x] C1 (kandidat a + b) Settings → Container engine: **Check again** dengan waktu pemeriksaan terakhir; saran "WSL containers are available on this PC" saat Docker dipakai, beserta penjelasan data volume dan tombol **Use WSL containers**.
- [x] S1 Baris service yang dipilih tetapi tidak berjalan: label "not running" dan tombol **Start**.
- [x] S5 Start/stop yang gagal: pesan disimpan per service, baris menampilkan "failed" dan **Retry** (pesan di tooltip); banner error Services mendapat Retry dan tutup.
- [x] C2 Popup tray: baris per service (yang dipilih atau berjalan) dengan status berwarna; klik = start/stop service itu saja, dengan "starting…"/"stopping…" sampai statusnya berubah. Start mengirim service yang sedang berjalan + service itu, supaya backend Docker tidak menghentikan yang lain dan pilihan lain tidak ikut menyala.
- [x] C3 Logs: kata filter disorot; hanya 1.000 baris terbaru yang dirender, dengan "Show N older lines" (posisi baca dipertahankan). Buffer tetap 500 baris per sumber, maks. 4.000.
- [x] S4 Popup tray: "none selected" dan petunjuk bila belum ada service yang dipilih (bukan bug; pilihan sudah tersinkron, hanya labelnya kurang jelas).

### N.1 — v3.8.1: log wslc dan test flaky

Diuji di Windows (21 November 2026, v3.8.0): start/stop satu service dari popup tray (status "starting…"/"stopping…", service lain tidak ikut berubah) dan `wslc stop -t 30`. Lolos.

- [x] Logs: output wslc yang hanya mengulang nama container setelah start/stop/remove tidak ditampilkan.
- [x] Test `port_checks_see_a_listener` tidak lagi bergantung pada port yang baru dilepas (bisa direbut test paralel).

### N.2 — v3.8.2: onboarding sesuai canvas, pengecekan update yang jujur

Audit ulang canvas (21 November 2026, per artboard, di level markup): selisih besar hanya di onboarding A1–A5 dan konsep daftar Services. Services tetap memakai model toggle (canvas yang diperbarui).

- [x] Onboarding satu kartu: dua baris engine dengan hasil pemeriksaan, pilihan Engine (WSL containers / Docker / Auto), banner WSL terlalu lama, Docker belum ada/berhenti, WSL tidak merespons, Administrator.
- [x] Runtimes: judul dan state A3–A5, winget tidak ada, pasang sendiri dengan Copy.
- [x] Pengecekan update gagal tampil sebagai error dengan Retry (dulu "You have the latest version"); hasil lama disembunyikan saat memeriksa; cek berkala 6 jam dan saat fokus.

### N.3 — v3.8.3: output winget langsung di onboarding

- [x] Output winget tampil baris per baris di kartu Install mise (A4), progres unduhan di bawah bar; baris yang sama masuk Logs.

### N.4 — v3.8.4: Logs, Services, Settings sesuai canvas

- [x] Logs: All / Errors & warnings (segmented), switch Follow, footer "Last 500 lines per source".
- [x] Services: profil kosong dengan Choose services.
- [x] Settings: WINDOW & QUIT, COMMAND LINE, "Keep running in the tray when the window closes".
- Tidak diubah, karena label di aplikasi lebih tepat: "Start Kova when you sign in" (berlaku lintas OS) dan "Start services" (memulai pilihan tersimpan). Canvas yang diperbarui.

### N.5 — v3.8.5: paste bulk dan Edit as .env

- [x] Paste teks `.env` ke kolom nama/nilai dipecah menjadi baris; nama yang ada diganti; ringkasan added/replaced/skipped dengan alasan.
- [x] Edit as .env / Edit as rows; Save dari mode teks. Parser: komentar, `export`, kutip, `=` pertama, `PORT` dilewati.
- [x] Canvas Flow H (Environments, model B: Default + override) digambar; H3 memakai placeholder nilai Default tanpa chip (masukan 21 November).

### N.6 — v3.8.6: tata letak sesuai artboard

Masukan 21 November: fungsi sudah sesuai canvas, tetapi susunan layar terlalu polos. Audit sebelumnya membandingkan markup dan teks, sehingga kepadatan visual (ikon di tombol, chip, kotak output, kartu pengaturan) tidak tertangkap.

- [x] `UiIcon` (ikon stroke canvas) di tombol utama semua layar.
- [x] Projects: pencarian selalu ada, titik status di kartu; detail: runtime + chip sumber, chip service, kotak output, menu ⋯ di judul, footer Open folder / Terminal / VS Code.
- [x] Services, Logs, Settings: judul layar; Settings berupa kartu per bagian dalam dua kolom. Runtimes: PHP extensions sebagai panel kanan.

### O — v3.9.0: Environments per project

Keputusan 21 November: model B (Default + override), nama "Environments", H3 memakai placeholder nilai Default tanpa chip.

- [x] Config: `envs` (nama + nilai yang berbeda dari Default) dan `activeEnv`; ditulis hanya bila dipakai. `Project::effective_env` = Default + override; nama usang = Default.
- [x] Start/restart memakai environment aktif; log `env (<nama>): …`; status proses membawa `envName`.
- [x] Tab Environment: pemilih environment, New environment (salin perubahan dari environment lain), baris Default dengan placeholder, × = kembali ke Default, Delete environment, paste bulk dan Edit as .env per environment.
- [x] Banner "Running with … · Restart to use …"; environment aktif di kartu project, status detail, Overview, dan popup tray.

### P — v3.10.0: komponen, font, dan penempatan sesuai canvas

Masukan 21 November: teks terasa terlalu besar, dan audit harus mencakup komponen UI, font, serta penempatan layout di lima layar. Ukuran yang dirender diukur langsung terhadap `kova.css`.

- [x] Komponen: kartu project 56 px/r8, tombol sm 26 px/500 dan md 32 px, navigasi 34 px, toggle 34×20, tab 36 px, panel detail `surface` padding 16, RAM abu.
- [x] Font: body 13 px/1.45 (dulu 14/1.5); nama project/service 14 px, versi runtime 14/600, mono detail 12 px, sub-navigasi Settings 13 px.
- [x] Penempatan: padding Projects 18/20; footer detail satu baris; ringkasan Services; judul + pilihan tool Runtimes satu baris; ikon search Logs; Settings dua kolom seperti canvas.
- [x] Interface size (Compact / Default / Large) lewat zoom webview.

### P.1 — v3.10.1: RAM realtime yang hemat

- [x] Polling `stats` per mode: 2 s saat layar Services terlihat, 5 s di layar lain, jeda saat jendela tersembunyi (`stats_set_mode`, bangun lebih awal lewat `Notify`).
- [x] Snapshot kosong ikut dikirim, sehingga nilai lama terhapus setelah semua service berhenti.
- [x] Tooltip RAM: "measured at …" untuk nilai asli, "Estimate …" untuk perkiraan.

### P.2 — v3.10.2: `composer run dev` dan start yang lambat

- [x] Port tetap dicek setelah batas 90 detik (tiap 2 s); `unresponsive` naik ke `running` begitu app menjawab.
- [x] `SERVER_PORT` dan `SERVER_HOST` disuntik bersama `PORT`, sehingga `php artisan serve` di dalam `composer run dev` mengikuti port project.
- [x] Settings → About: kartu bersarang dihapus, baris update diberi ruang bawah.
- [x] v3.10.3: Settings → Services: teks pembuka Profiles/Restore database tidak menempel ke tepi kartu, baris profil tanpa kartu bersarang, ruang bawah form profil.
- [x] Kandidat minor (pilih script dari `composer.json`/`package.json`, chip sumber log `[server]`/`[queue]`/`[vite]`, petunjuk bila `composer` tidak ditemukan): selesai di v3.11.0.

## Berikutnya

### Q — Penutup revamp: milestone per minor (disepakati 21 November 2026)

Dasar: audit flow A–H canvas terhadap aplikasi (v3.10.3). Semua minor, tanpa perubahan yang memutus kompatibilitas; yang memutus kompatibilitas tetap di v4.0. Bukti uji di [docs/testing/test-cases.md](docs/testing/test-cases.md).

- [ ] **Baseline uji.** Semula dengan v3.10.3; dipindah ke putaran 1 dengan **v3.15.3** di Windows (wslc) dan Linux (Docker Engine, tanpa Docker Desktop). Temuan Linux awal sudah menjadi v3.15.2–v3.15.3; tabel test case masih kosong.
- [x] **v3.11.0 — Scripts & `composer run dev`.** Pilih script dari `composer.json`/`package.json`; chip sumber log dari prefiks `concurrently` (`[server]`/`[queue]`/`[vite]`); petunjuk bila `composer`/`npm` tidak ditemukan; keputusan default Laravel. Menutup TC-S*, TC-G*.
- [x] **v3.12.0 — Linux Docker.** Pesan jelas untuk `docker.sock` permission denied (`usermod -aG docker`), plugin Compose v2 tidak ada, daemon mati; kartu onboarding Linux menyesuaikan. Menutup TC-L*, TC-A2/L, TC-D*/L.
- [x] **v3.13.0 — Runtimes & Update.** Chip `new` setelah install (E3), teks rollback extension (E8), Later = pasang saat Kova ditutup (F3), What's new setelah update (F4). Menutup TC-E*, TC-F*.
- [x] **v3.14.0 — Penutup revamp.** Keputusan final D2 (Start profile vs toggle), semua ◐ di canvas diselesaikan, canvas final, uji NVDA, regresi penuh A–H di dua OS. Selesai bila tidak ada ◐ tersisa dan semua baris test case ✅/➖. *Dirilis bersama v3.11.0–v3.13.0 sebagai satu tag v3.14.0; draft rilis menggabungkan catatannya (`scripts/release-notes.sh`). Uji di perangkat menyusul (lihat docs/testing).*

### v3.x — kandidat, belum diputuskan

- [x] Catatan status canvas diperbarui per v3.8.4 (21 November 2026), termasuk penyimpangan yang memang benar.
- [x] v3.9.0 — Environments per project (canvas Flow H): lihat O di atas.
- [x] **v3.15.0 — Kelompok project dan logo framework** (canvas B8–B12, tray v3.15). Deteksi Go, React, Hono, Express, AdonisJS, Fastify, NestJS (backend menang atas frontend) dengan alasan deteksi; chip berlogo dari simple-icons (CC0, dibundel); kelompok project (lipat, Start all, Rename, Stop all, Delete); urutan lewat seret dan Alt+↑/↓; klik kartu terpilih menutup panel detail; tray dan menu Linux mengikuti kelompok; palet perintah per kelompok; RAM Kova dan dev server di status bar tiap 2 detik. Uji di perangkat: TC-B8–B10, TC-P*, TC-T7–T9, TC-U6–U9.

### Ditunda — sampai ada kebutuhan pengguna

- Dukungan Dockerfile/compose per project. Ditunda per 20 November 2026: belum ada kebutuhan dari pengguna; dibuka lagi bila permintaannya banyak. Bila dibuka, mulai dengan spike: verifikasi `wslc build`, bind mount folder project, dan jaringan ke service. Docker jelas bisa; dukungan wslc belum pasti dan `wslc compose` belum ada.

### v4.0 — pemutusan kompatibilitas Servel (belum dijadwalkan)

- Perintah terminal `servel` dihapus; hanya `kova`. Pemberitahuan di terminal ("keeps working until v4.0") merujuk ke rilis ini.
- Kandidat, masing-masing butuh migrasi dan keputusan terpisah: nama container/volume `servel_*` → `kova_*`, folder data dengan bundle id lama (`com.<pemilik>.servel`), field config lama (`versionProvider`), migrasi config v7.
