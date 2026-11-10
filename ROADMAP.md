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
- **Prasyarat**: deteksi versi WSL, versi wslc, dan peringatan bila Servel berjalan elevated (sesi WSLC terpisah per elevation).
- **UI**:
  - Onboarding, banner Dashboard, dan Settings → Container engine menjadi engine-neutral, termasuk tombol pindah ke WSL containers satu klik.
  - Tema "soft": netral kebiruan, border translucent, aksen pastel, skala radius, Inter untuk UI dan mono hanya untuk data.

### Perlu verifikasi di mesin Windows

Hal-hal berikut tidak bisa diuji tanpa `wslc` sungguhan. Parser sudah dibuat toleran terhadap beberapa bentuk output.

- Bentuk JSON `wslc container list --all --format json`. Diasumsikan `State` numerik (1 created, 2 running, 3 exited), mengikuti proyek komunitas.
- Bentuk JSON `wslc stats`. Bila tidak dikenali atau gagal tiga kali berturut-turut, kolom RAM jatuh ke estimasi dan stats dimatikan untuk sesi itu.
- `wslc stop -t 30` dan `wslc remove -f`.
- Apakah `.wslconfig` memengaruhi VM WSLC.

### Batasan yang diketahui

- Tidak ada network antar-container (`servel_default`). Service diakses dari host lewat `127.0.0.1`; dev lokal Servel tidak membutuhkan komunikasi antar-container.
- Tidak ada restart policy, jadi service hidup kembali saat Servel auto-start, bukan saat Windows boot.
- Data Docker Desktop tidak dimigrasikan otomatis.

## Berikutnya (kandidat)

- **v1.7** — Wizard migrasi data Docker → WSLC: dump dari volume Docker lalu restore ke container WSLC, dengan pola `servel.ps1 restore`.
- **v1.7** — Readiness check per service (mis. `mysqladmin ping`, `pg_isready`) sebelum status dianggap "up".
- **v2.0** — WSLC menjadi default di Windows setelah `wslc compose` resmi tersedia ([microsoft/WSL#40948](https://github.com/microsoft/WSL/issues/40948)) atau endpoint Docker API ([microsoft/WSL#40976](https://github.com/microsoft/WSL/issues/40976)).
- Code signing Windows dan notarization macOS (tertunda sejak v1.0).
