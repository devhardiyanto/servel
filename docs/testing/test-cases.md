# Test case Kova

Panduan, arti status, dan aturan bukti: [README.md](README.md). Langkah dan hasil yang diharapkan diambil dari artboard flow di canvas *Servel v3 UI* (A1–H6), ditambah kasus khusus OS dan regresi dari rilis 3.10.x.

Kolom **W** = Windows, **L** = Linux. Bukti: `evidence/<versi>/<ID>-<W|L>.<ext>`.

## Catatan putaran

| Putaran | Versi Kova | OS (W / L) | Engine | mise | Tanggal | Penguji |
|---|---|---|---|---|---|---|
| 1 | v3.14.0 | Windows 11 … / Ubuntu … | wslc … / Docker Engine … + Compose … | … | … | … |

## Ringkasan

| Bagian | Jumlah | W ✅ | L ✅ | Milestone yang menutup |
|---|---|---|---|---|
| A · Onboarding | 9 | 0 | 0 | v3.10.x, v3.12.0 |
| B · Add project | 7 | 0 | 0 | v3.10.x |
| C · Project gagal | 6 | 0 | 0 | v3.10.x |
| D · Services | 7 | 0 | 0 | v3.12.0, v3.14.0 |
| E · Runtimes | 8 | 0 | 0 | v3.13.0 |
| F · Update | 6 | 0 | 0 | v3.13.0 |
| G · Logs | 5 | 0 | 0 | v3.11.0 |
| H · Environments | 6 | 0 | 0 | v3.10.x |
| S · Scripts / dev server | 7 | 0 | 0 | v3.11.0 |
| T · Tray & jendela | 5 | 0 | 0 | v3.10.x |
| U · Tampilan & aksesibilitas | 5 | 0 | 0 | v3.14.0 |
| W · Khusus Windows | 4 | 0 | ➖ | v3.10.x |
| L · Khusus Linux | 7 | ➖ | 0 | v3.12.0 |

---

## A · Onboarding (canvas A1–A9)

Persiapan: lihat README → *Cara menyiapkan ulang*.

| ID | Langkah | Hasil yang diharapkan | W | L | Bukti | Catatan |
|---|---|---|---|---|---|---|
| TC-A1 | Buka Kova pertama kali, atau Settings → General → Run setup again. | Onboarding terbuka. Engine diperiksa otomatis tanpa klik; Continue nonaktif sampai hasil keluar. | ⬜ | ⬜ | | |
| TC-A2 | Tunggu hasil cek engine, lalu klik Continue. | **W:** baris WSL containers siap dengan versi WSL ≥ 3.0.1; Docker tidak wajib. **L:** baris Docker siap (Docker Engine, bukan Desktop). Continue aktif. | ⬜ | ⬜ | | |
| TC-A3 | Langkah Runtimes dengan mise belum terpasang. | **W:** tombol Install mise; "I'll install it myself" menampilkan `winget install jdx.mise` + Check again. **L:** perintah `curl https://mise.run \| sh` + Copy + Check again. | ⬜ | ⬜ | | |
| TC-A4 | **W:** klik Install mise. | Output winget tampil baris per baris di kartu, beserta progres; tombol dikunci selama proses, Back tetap ada. Gagal → status *mise install failed* dengan Retry. | ⬜ | ➖ | | |
| TC-A5 | Setelah mise terpasang (W: lewat A4; L: pasang manual lalu Check again). | Versi mise tampil; log tetap terlihat; Continue aktif. | ⬜ | ⬜ | | |
| TC-A6 | Langkah First project: klik Choose folder, lalu ulangi dengan Skip for now. | Dialog folder bawaan OS terbuka. Skip for now → Projects dengan empty state. | ⬜ | ⬜ | | |
| TC-A7 | Pilih folder project Laravel (ada `artisan` dan `composer.json`). | Terdeteksi Laravel; perintah `php artisan serve --host=127.0.0.1 --port={port}`; port bebas terisi; versi PHP dari `mise.toml` bila ada. | ⬜ | ⬜ | | |
| TC-A8 | Klik Add & start. | Onboarding tertutup, Projects menjadi layar pertama, project berstatus Starting. | ⬜ | ⬜ | | |
| TC-A9 | Tunggu sampai berjalan, lalu klik Open localhost:port. | Status Running, toast konfirmasi; browser default membuka `http://localhost:<port>`. Buka ulang Kova → langsung ke Projects, tanpa onboarding. | ⬜ | ⬜ | | |

## B · Add project (canvas B1–B7)

| ID | Langkah | Hasil yang diharapkan | W | L | Bukti | Catatan |
|---|---|---|---|---|---|---|
| TC-B1 | Di Projects klik Add project; ulangi lewat Ctrl K → Add project. | Keduanya membuka alur Add project. | ⬜ | ⬜ | | |
| TC-B2 | Di dialog folder klik Cancel. | Kembali ke Projects tanpa perubahan. | ⬜ | ⬜ | | |
| TC-B3 | Pilih folder project Node (mis. Nuxt/Vite). | Modal Add project dengan skeleton selama deteksi; Save dikunci sampai deteksi selesai. | ⬜ | ⬜ | | |
| TC-B4 | Periksa hasil deteksi, klik Save (tanpa start). Lalu tambah project lain dengan port yang sudah dipakai. | Semua field terisi. Save menyimpan tanpa menjalankan. Port bentrok → pesan jelas sebelum start. | ⬜ | ⬜ | | |
| TC-B5 | Add project dengan pilihan Services = sebuah profil dan *Start with project* dicentang, lalu Save & start. | Modal tertutup, project baru terpilih, output tampil di panel kanan; service di profil itu ikut menyala. | ⬜ | ⬜ | | |
| TC-B6 | Tunggu sampai Running. | Port tampil di kartu dan panel detail; tombol utama Open localhost:port. | ⬜ | ⬜ | | |
| TC-B7 | Klik Open. | Browser default membuka `http://localhost:<port>`; tidak ada domain `.test` atau HTTPS lokal. | ⬜ | ⬜ | | |

## C · Project gagal → perbaiki (canvas C1–C6)

Persiapan: project yang gagal karena env, mis. Laravel tanpa `DB_*` padahal butuh database, atau perintah `php -r "fwrite(STDERR, 'DATABASE_URL missing'); exit(1);"`.

| ID | Langkah | Hasil yang diharapkan | W | L | Bukti | Catatan |
|---|---|---|---|---|---|---|
| TC-C1 | Start project yang pasti gagal. | Kartu menunjukkan `exited (1)` berwarna merah; badge merah di nav Logs. | ⬜ | ⬜ | | |
| TC-C2 | Klik kartunya. | Banner error berisi baris terakhir output; ada View log. | ⬜ | ⬜ | | |
| TC-C3 | Klik View log, lalu Open project. | Logs terbuka dengan sumber project itu terpilih, baris error berwarna merah; Open project kembali ke panel detail. | ⬜ | ⬜ | | |
| TC-C4 | Tab Environment: tambah variabel yang hilang, klik Save & restart. | Tersimpan, project di-restart. | ⬜ | ⬜ | | |
| TC-C5 | Amati saat restart. | Banner error hilang; output baru menggantikan yang lama. Bila gagal lagi → kembali ke C2 dengan baris baru. | ⬜ | ⬜ | | |
| TC-C6 | Tunggu sampai Running. | Status Running; badge Logs berkurang; toast hilang sendiri dan bisa ditutup manual. | ⬜ | ⬜ | | |

## D · Services (canvas D1–D5, ditambah RAM v3.10.1)

| ID | Langkah | Hasil yang diharapkan | W | L | Bukti | Catatan |
|---|---|---|---|---|---|---|
| TC-D1 | Buka Services. | Semua service tampil (grup core/additional) dengan toggle; pemilih profil di header. | ⬜ | ⬜ | | |
| TC-D2 | Ganti profil, lalu Start selected. | Toggle mengikuti profil; service terpilih mulai. *Artboard D2 memakai tombol "Start profile"; keputusan final ada di v3.14.0.* | ⬜ | ⬜ | | v3.14.0: tombol "Start <profil>" selama pilihan = isi profil |
| TC-D3 | Amati saat service mulai (pertama kali: image belum ada). Ulangi dengan port service dipakai program lain. | Tiap baris punya status sendiri; image diunduh dulu. Port bentrok → banner "Port … is already in use". | ⬜ | ⬜ | | |
| TC-D4 | Buka detail MySQL → Connect. Pilih format Laravel, Copy .env, lalu Copy URL. | Host, port, user, password, database tampil; potongan `.env` sesuai format; URL `mysql://root@127.0.0.1:3306/…`. | ⬜ | ⬜ | | |
| TC-D5 | Tempel hasil Copy .env ke project. | Toast menyebut jumlah baris yang disalin; nilai bisa langsung dipakai project (lihat C4). | ⬜ | ⬜ | | |
| TC-D6 | Dengan service berjalan, diamkan di layar Services; arahkan kursor ke angka RAM. Lalu sembunyikan jendela ke tray. | Angka RAM berubah sekitar tiap 2 detik; tooltip "measured at HH:MM:SS". Service berhenti → angka `~` dengan tooltip estimasi. | ⬜ | ⬜ | | v3.10.1 |
| TC-D7 | Stop all. | Semua berhenti; RAM di ringkasan dan status bar menjadi 0. | ⬜ | ⬜ | | v3.10.1 |

## E · Runtimes (canvas E1–E8)

| ID | Langkah | Hasil yang diharapkan | W | L | Bukti | Catatan |
|---|---|---|---|---|---|---|
| TC-E1 | Runtimes → PHP. Ketik versi (mis. 8.4) di Install version, klik Install. | Daftar versi: global, dipakai project, tidak dipakai. Instalasi dimulai. | ⬜ | ⬜ | | |
| TC-E2 | Amati selama instalasi; ulangi lalu klik Cancel. | Baris progres + baris terakhir output mise; navigasi lain tetap bisa dipakai. Cancel menghentikan proses. Gagal → baris merah dengan pesan mise + Retry. | ⬜ | ⬜ | | |
| TC-E3 | Setelah terpasang. | Versi baru muncul dengan status Not used dan chip `new`. | ⬜ | ⬜ | | v3.13.0 |
| TC-E4 | Klik ⋯ pada versi itu. | Menu: Set as global, Use in project…, Remove. | ⬜ | ⬜ | | |
| TC-E5 | Pilih Use in project…, pilih project, konfirmasi. | Hanya project PHP yang muncul. `mise.toml` project berisi `php = "<versi>"` (lampirkan isi file). | ⬜ | ⬜ | | |
| TC-E6 | Nyalakan extension `intl`. | Toast berhasil, menawarkan Restart project. | ⬜ | ⬜ | | |
| TC-E7 | Nyalakan `xdebug`. | Status *checking…* selama backup `php.ini` → tulis → `php -m`; toggle lain tetap bisa dipakai. | ⬜ | ⬜ | | |
| TC-E8 | Nyalakan extension yang tidak cocok dengan versi PHP (bila ada). | Toggle kembali off; `php.ini` dikembalikan; banner "<ext> was not turned on. Nothing changed." berisi penyebab dan cara memperbaikinya. | ⬜ | ⬜ | | v3.13.0 |

## F · Update aplikasi (canvas F1–F5)

Persiapan: pasang versi sebelumnya dari Releases.

| ID | Langkah | Hasil yang diharapkan | W | L | Bukti | Catatan |
|---|---|---|---|---|---|---|
| TC-F1 | Buka Kova versi lama. | Banner "update available" muncul setelah cek saat launch; Release notes membuka halaman rilis; × menyembunyikan banner. | ⬜ | ⬜ | | |
| TC-F2 | Klik Update. | Progres unduhan di banner; aplikasi tetap bisa dipakai. | ⬜ | ⬜ | | |
| TC-F3 | Saat siap, klik Later; cek Settings → About; Quit Kova dari tray; buka lagi. | About menulis "Kova X will be installed when you quit Kova." Setelah Quit, update terpasang; Kova dibuka lagi di versi baru. | ⬜ | ⬜ | | v3.13.0 |
| TC-F4 | Klik Restart now. | Kova terbuka lagi di versi baru; status bar menampilkan versi itu; What's new membuka catatan rilis. | ⬜ | ⬜ | | What's new: v3.13.0 (toast "Kova updated to X") |
| TC-F5 | Putus jaringan, lalu Settings → About → Check for updates. | "Could not check for updates" dengan pesan error, Retry, dan Open release page; tidak tampil sebagai "latest version". | ⬜ | ⬜ | | |
| TC-F6 | Jaringan normal, Check for updates. | "You have the latest version · checked HH:MM", di dalam kartu About tanpa kartu bersarang. | ⬜ | ⬜ | | v3.10.2 |

## G · Logs (canvas G1–G4)

| ID | Langkah | Hasil yang diharapkan | W | L | Bukti | Catatan |
|---|---|---|---|---|---|---|
| TC-G1 | Buka Logs dengan beberapa project dan service berjalan. | Semua sumber digabung dan diurutkan waktu; warna kolom sumber sesuai jenisnya. | ⬜ | ⬜ | | |
| TC-G2 | Tab Projects → chip satu project. | Hanya output project itu; badge di chip = jumlah error; tombol Open project muncul. | ⬜ | ⬜ | | |
| TC-G3 | Pilih Errors (All/Errors). | Baris biasa disembunyikan; footer menandai hasil terfilter. | ⬜ | ⬜ | | |
| TC-G4 | Ketik teks di filter, klik Copy, lalu Clear. | Filter teks dan level bisa digabung; Copy hanya menyalin baris yang terlihat; Clear mengosongkan buffer sumber itu tanpa menghapus file. | ⬜ | ⬜ | | |
| TC-G5 | Matikan Follow lalu scroll ke atas; nyalakan lagi. Export logs. | Follow off = tidak lompat ke bawah; on = ikut baris baru. Footer "Last 500 lines per source". Export menghasilkan file. | ⬜ | ⬜ | | |

## H · Environments (canvas H1–H6)

| ID | Langkah | Hasil yang diharapkan | W | L | Bukti | Catatan |
|---|---|---|---|---|---|---|
| TC-H1 | Detail project → tab Environment. | Pemilih Environment dengan Default; ada New environment. | ⬜ | ⬜ | | |
| TC-H2 | New environment: nama `tenant-astra`, Copy changes from = Nothing, Create environment. Coba juga Esc. | Environment dibuat dan terpilih; Esc/Cancel membatalkan. Nama tidak valid ditolak. | ⬜ | ⬜ | | |
| TC-H3 | Di `tenant-astra` ubah satu nilai dan tambah satu variabel baru; Save & restart. | Nilai Default tampil sebagai placeholder (tanpa badge); hanya yang berbeda disimpan. Mengosongkan override = kembali ke Default. | ⬜ | ⬜ | | |
| TC-H4 | Edit as .env, tempel beberapa baris termasuk `PORT=…` dan `export X=1`; Save. Lalu tempel blok `.env` langsung di kolom Name. | Ringkasan "N added · N replaced · N skipped"; `PORT` dilewati; Edit as rows kembali ke daftar. | ⬜ | ⬜ | | |
| TC-H5 | Saat project berjalan dengan Default, pilih `tenant-astra`. | Banner "Restart to use tenant-astra"; proses tetap memakai Default sampai Restart. | ⬜ | ⬜ | | |
| TC-H6 | Klik Restart. | Kartu project, header detail, dan popup tray menampilkan `tenant-astra`; log mencatat `env (tenant-astra): …`. | ⬜ | ⬜ | | |

## S · Scripts & dev server (regresi v3.10.2, persiapan v3.11.0)

| ID | Langkah | Hasil yang diharapkan | W | L | Bukti | Catatan |
|---|---|---|---|---|---|---|
| TC-S1 | Project Laravel dengan Command `composer run dev`, Port 8001; Start. | Log `[server]` menampilkan `127.0.0.1:8001`; status Running; Open membuka :8001. | ⬜ | ⬜ | | v3.10.2 (`SERVER_PORT`) |
| TC-S2 | Command yang lambat menjawab > 90 detik. **W:** `powershell -c "Start-Sleep 100; php -S 127.0.0.1:{port}"`. **L:** `sh -c "sleep 100; python3 -m http.server {port}"`. | Setelah 90 detik status *not answering*; sekitar 10 detik kemudian berubah sendiri menjadi Running. | ⬜ | ⬜ | | v3.10.2 |
| TC-S3 | Stop project `composer run dev` dari TC-S1. | Semua proses anak berhenti (`php`, `node`/vite, queue, scheduler); port bebas. **W:** tidak ada `php.exe`/`node.exe` sisa di Task Manager. **L:** `pgrep -fa artisan` kosong. | ⬜ | ⬜ | | |
| TC-S4 | Tutup paksa Kova (Task Manager / `kill -9`) saat project berjalan, lalu buka lagi. | Dev server sisa dari sesi sebelumnya dibersihkan; project bisa di-start lagi tanpa "port in use". | ⬜ | ⬜ | | |
| TC-S5 | Project settings project Laravel yang punya script `dev` di `composer.json`. | Di bawah Command tampil Suggestions: perintah deteksi, `composer run dev`, `npm run dev` (bila ada). Klik mengisi field. **W:** bila script memuat `pail`, muncul peringatan pcntl. | ⬜ | ⬜ | | v3.11.0 |
| TC-S6 | Jalankan `composer run dev`, buka Logs → Projects → chip project itu. | Baris chip kedua: All processes, server, queue, vite, … dengan jumlah error; klik satu = hanya baris proses itu. | ⬜ | ⬜ | | v3.11.0 |
| TC-S7 | Command `programtidakada run dev`, Start. | Exit 127 (L) / 9009 (W); banner project dan log: "programtidakada was not found. …". | ⬜ | ⬜ | | v3.11.0 |

## T · Tray & jendela

| ID | Langkah | Hasil yang diharapkan | W | L | Bukti | Catatan |
|---|---|---|---|---|---|---|
| TC-T1 | Tutup jendela dengan "Keep running in the tray" aktif. | Jendela tersembunyi; project dan service tetap jalan. | ⬜ | ⬜ | | |
| TC-T2 | **W:** klik kiri/kanan ikon tray. **L:** klik ikon tray. | **W:** popup berisi project dan service dengan start/stop per baris, versi PHP/Node global. **L:** menu native. | ⬜ | ⬜ | | L: GNOME butuh ekstensi AppIndicator |
| TC-T3 | Quit dari tray dengan "Stop services when Kova quits" aktif. | Service berhenti sebelum Kova keluar; data volume tetap ada. | ⬜ | ⬜ | | |
| TC-T4 | Aktifkan "Start Kova when you sign in", logout/login. | Kova mulai sendiri (di tray bila minimize to tray aktif). | ⬜ | ⬜ | | |
| TC-T5 | Buka Kova untuk kedua kalinya. | Hanya satu instance; jendela yang ada muncul ke depan. | ⬜ | ⬜ | | |

## U · Tampilan & aksesibilitas

| ID | Langkah | Hasil yang diharapkan | W | L | Bukti | Catatan |
|---|---|---|---|---|---|---|
| TC-U1 | Settings → Appearance → Interface size: Compact, Default, Large. | Teks dan kontrol di semua jendela ikut berubah; pilihan tersimpan setelah restart. | ⬜ | ⬜ | | |
| TC-U2 | Buka semua tab Settings (General, Services, About). | Tidak ada kartu bersarang; teks tidak menempel ke tepi kartu. | ⬜ | ⬜ | | v3.10.2–3.10.3 |
| TC-U3 | Ctrl K, ketik nama project / perintah. | Palet terbuka, hasil bisa dipilih dengan keyboard, Esc menutup. | ⬜ | ⬜ | | |
| TC-U4 | Navigasi seluruh aplikasi hanya dengan keyboard (Tab, Shift+Tab, Enter, Space, Esc). | Fokus selalu terlihat; semua tombol dan toggle bisa dijangkau; modal mengunci fokus. | ⬜ | ⬜ | | |
| TC-U5 | **W:** jalankan NVDA, buka Projects, Services, Logs. | Nama tombol, status project, dan badge error dibacakan; perubahan status diumumkan. | ⬜ | ➖ | | v3.14.0 |

## W · Khusus Windows

| ID | Langkah | Hasil yang diharapkan | W | L | Bukti | Catatan |
|---|---|---|---|---|---|---|
| TC-W1 | Settings → Container engine: Auto, lalu WSL containers, lalu Docker (bila Docker Desktop terpasang). | Auto memilih WSL containers; status Running dengan versi; catatan bahwa image/volume WSL containers terpisah dari Docker Desktop. | ⬜ | ➖ | | |
| TC-W2 | Buka terminal baru, jalankan `kova status` dan `kova help`. | Perintah ditemukan di PATH dan menampilkan status service. | ⬜ | ➖ | | |
| TC-W3 | Jalankan Kova sebagai Administrator. | Peringatan Administrator di onboarding. | ⬜ | ➖ | | |
| TC-W4 | Restore database: pilih file `.sql`, MySQL, Restore. | Service dinyalakan bila perlu, menunggu koneksi, dump dijalankan; hasil tampil di kartu. | ⬜ | ➖ | | Ulangi di L sebagai TC-L7 |

## L · Khusus Linux (Docker Engine, tanpa Docker Desktop)

Catat di *Catatan putaran*: distro, desktop environment, `docker version`, `docker compose version`, `docker context ls`.

| ID | Langkah | Hasil yang diharapkan | W | L | Bukti | Catatan |
|---|---|---|---|---|---|---|
| TC-L1 | `chmod +x Kova_*.AppImage` lalu jalankan. | Kova terbuka. Bila gagal, catat pesan error dan paket yang hilang (mis. `libfuse2`). | ➖ | ⬜ | | |
| TC-L2 | Docker Engine aktif (`systemctl is-active docker`), user di grup `docker`. Buka Services, start MySQL. | Engine Docker berjalan; service start lewat `docker compose`; `docker context ls` menunjukkan `default`, bukan `desktop-linux`. | ➖ | ⬜ | | |
| TC-L3 | `sudo systemctl stop docker`, lalu buka Kova / Check again. | Instruksi `sudo systemctl start docker` dengan Copy; setelah daemon dinyalakan, Check again → siap. | ➖ | ⬜ | | |
| TC-L4 | User **tanpa** grup `docker` (`sudo gpasswd -d $USER docker`, login ulang). Buka Kova. | Pesan jelas berisi instruksi `sudo usermod -aG docker $USER` dan perlu login ulang. Status engine "No access" di Settings. | ➖ | ⬜ | | v3.12.0 |
| TC-L5 | Tanpa plugin Compose v2 (`docker compose version` gagal). Start service. | Pesan jelas bahwa plugin Compose v2 dibutuhkan dan cara memasangnya (`sudo apt install docker-compose-plugin`). Start service memberi pesan yang sama. | ➖ | ⬜ | | v3.12.0 |
| TC-L6 | Settings → Command line → Install, lalu buka terminal baru: `kova status`. | `kova` tersalin ke `~/.local/bin/kova` dan berjalan (bila `~/.local/bin` ada di PATH). | ➖ | ⬜ | | |
| TC-L7 | Restore database seperti TC-W4. | Sama dengan TC-W4. | ➖ | ⬜ | | |
