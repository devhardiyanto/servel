# Test case Kova

Panduan, arti status, dan aturan bukti: [README.md](README.md). Langkah dan hasil yang diharapkan diambil dari artboard flow di canvas desain *Servel v3 UI* (A1–H6; nama canvas dari sebelum ganti nama ke Kova), ditambah kasus khusus OS dan regresi dari rilis 3.10.x sampai v3.16.0.

Kolom **W** = Windows, **L** = Linux. Bukti: `evidence/<versi>/<ID>-<W|L>.<ext>`.

## Catatan putaran

| Putaran | Versi Kova | OS (W / L) | Engine | mise | Tanggal | Penguji |
|---|---|---|---|---|---|---|
| 1 | v3.15.3 | Windows 11 … / Ubuntu … | wslc … / Docker Engine … + Compose … | … | … | … |

## Ringkasan

| Bagian | Jumlah | W ✅ | L ✅ | Milestone yang menutup |
|---|---|---|---|---|
| A · Onboarding | 9 | 0 | 0 | v3.10.x, v3.12.0 |
| B · Add project | 13 | 0 | 0 | v3.10.x, v3.15.0, v3.24.0, v3.25.1 |
| C · Project gagal | 10 | 0 | 0 | v3.10.x, v3.21.1–v3.21.3 |
| D · Services | 9 | 0 | 0 | v3.12.0, v3.14.0, v3.15.3, v3.25.1 |
| E · Runtimes | 14 | 0 | 0 | v3.13.0, v3.15.4, v3.16.0, v3.24.0, v3.26.0 |
| F · Update | 7 | 0 | 0 | v3.13.0, v3.16.2 |
| G · Logs | 10 | 0 | 0 | v3.11.0, v3.16.0, v3.22.0, v3.25.1, v3.26.0 |
| H · Environments | 10 | 0 | 0 | v3.10.x, v3.16.1, v3.22.0 |
| I · Port & Terminal | 7 | 0 | 0 | v3.18.0, v3.26.0 |
| J · Custom service & menu | 8 | 0 | 0 | v3.19.0, v3.24.0, v3.25.1 |
| K · Container mode | 7 | 0 | 0 | v3.20.0, v3.25.1, v3.26.0 |
| L2 · Runtimes WSL & Add tool | 7 | 0 | 0 | v3.21.0, v3.25.0, v3.26.0 |
| S · Scripts / dev server | 7 | 0 | 0 | v3.11.0 |
| P · Kelompok & urutan project | 10 | 0 | 0 | v3.15.0 |
| T · Tray & jendela | 9 | 0 | 0 | v3.10.x, v3.14.1, v3.15.0 |
| U · Tampilan & aksesibilitas | 17 | 0 | 0 | v3.14.0, v3.15.0, v3.15.1, v3.16.0, v3.22.0, v3.23.0, v3.26.0 |
| W · Khusus Windows | 12 | 0 | ➖ | v3.10.x, v3.16.2, v3.17.0, v3.21.2, v3.24.0 |
| L · Khusus Linux | 12 | ➖ | 0 | v3.12.0, v3.15.2, v3.15.3, v3.25.1 |

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
| TC-B1 | Di Projects klik Add project; ulangi dari empty state Projects (tanpa project). | Keduanya membuka dialog pilih folder, lalu alur Add project. | ⬜ | ⬜ | | |
| TC-B2 | Di dialog folder klik Cancel. | Kembali ke Projects tanpa perubahan. | ⬜ | ⬜ | | |
| TC-B3 | Pilih folder project Node (mis. Nuxt/Vite). | Modal Add project dengan skeleton selama deteksi; Save dikunci sampai deteksi selesai. | ⬜ | ⬜ | | |
| TC-B4 | Periksa hasil deteksi, klik Save (tanpa start). Lalu tambah project lain dengan port yang sudah dipakai. | Semua field terisi. Save menyimpan tanpa menjalankan. Port bentrok → pesan jelas sebelum start. | ⬜ | ⬜ | | |
| TC-B5 | Add project dengan pilihan Services = sebuah profil dan *Start services with project* dinyalakan, lalu Save & start. | Modal tertutup, project baru terpilih, output tampil di panel kanan; service di profil itu ikut menyala. | ⬜ | ⬜ | | |
| TC-B6 | Tunggu sampai Running. | Port tampil di kartu dan panel detail; tombol utama Open localhost:port. | ⬜ | ⬜ | | |
| TC-B7 | Klik Open. | Browser default membuka `http://localhost:<port>`; tidak ada domain `.test` atau HTTPS lokal. | ⬜ | ⬜ | | |
| TC-B8 | Add project untuk folder backend Node: `shop-api` (Hono), `acme-api` (Express), `billing-worker` (Fastify), `acme-admin` (NestJS), `docs-api` (AdonisJS, ada `ace`), masing-masing dengan script `dev`. | Terdeteksi Hono / Express / Fastify / NestJS / AdonisJS dengan logo 28 px; perintah `npm run dev` (pnpm/yarn/bun mengikuti lockfile); alasan mis. "hono in package.json dependencies · src/index.ts · Node … from mise.toml". | ⬜ | ⬜ | | v3.15.0 |
| TC-B9 | Add project `acme-web` yang `package.json`-nya memuat `react` + `vite` + `express` dan script `dev`. Ulangi dengan React tanpa backend (Vite, lalu `react-scripts`), dan dengan folder Go (`go.mod`). | Campuran React + Express → **Express** (backend menang), `npm run dev`. React + Vite → "Vite · React" dengan logo React, `--port {port} --strictPort`. `react-scripts` → "React", `npm run start`. Go → "Go", `go run .` (atau `air`). | ⬜ | ⬜ | | v3.15.0 |
| TC-B10 | Lihat chip jenis di kartu, panel detail (Type), dan Project settings → Detect pada project lama. | Chip berlogo dengan garis/latar warna merek dan teks normal; Express/Fastify/Next.js terlihat jelas (logo warna teks). Jenis tanpa logo = chip abu-abu. Detect memperbarui jenis setelah Save. Tidak ada permintaan jaringan untuk ikon. | ⬜ | ⬜ | | v3.15.0 |
| TC-B11 | Add project, pilih folder yang sudah menjadi project (juga dengan garis miring di akhir atau huruf besar-kecil berbeda). | Dialog *<project> already uses this folder*; Show <project> menutup dialog dan memilih project itu; Choose another folder membuka pemilih folder lagi; tidak ada project ganda. | ⬜ | ⬜ | | v3.24.0 (canvas B13) |
| TC-B12 | Add project untuk folder dengan `Dockerfile` (`EXPOSE 9000`) dan `compose.yaml`. | *Run with* (Command / Dockerfile / Compose) dan *Found in the folder: Dockerfile compose.yaml*. Dockerfile: *Port in the container* = 9000 dengan keterangan *From EXPOSE in the Dockerfile*; Save & start menjalankan container. Folder tanpa keduanya tidak menampilkan Run with. | ⬜ | ⬜ | | v3.24.0 (canvas K1) |
| TC-B13 | Add project dengan port `80`; lalu ketik port yang dipakai MySQL dan langsung Save. Di Project settings, Change folder ke folder project lain. | Port 80 ditolak (1024–65535). Save cepat menunggu hasil cek port ("Still checking…"), lalu menolak port yang terpakai. Change folder ke folder milik project lain ditolak dengan nama project-nya. | ⬜ | ⬜ | | v3.25.1 |

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
| TC-C7 | Project yang membaca stdin sebagai penahan (mis. command `echo started; read x`, atau app Go dengan `fmt.Scanln()`). Start, tunggu 10 detik. | Proses tetap hidup sampai Stop; tidak ada `the process ended (exit 0)` tepat setelah start. | ⬜ | ⬜ | | v3.21.1 |
| TC-C9 | Tab Environment dengan 90+ variabel: ubah satu nilai di bawah, scroll; klik **?**. | Save / Save & start tetap terlihat di atas sebaris dengan pencarian; **?** membuka dan menutup penjelasan (Esc juga menutup). | ⬜ | ⬜ | | v3.21.3 |
| TC-C10 | Klik dua kali garis pemisah di Projects dan di Services (panel detail terbuka). | Panel detail tertutup; tombol ‹ di tepi kanan membukanya lagi dengan lebar sebelumnya. | ⬜ | ⬜ | | v3.21.3 |
| TC-C8 | Project yang keluar dengan exit 0 tanpa listen (mis. Go `http.Server{Addr: os.Getenv("PORT")}` dengan command tanpa `PORT=":{port}"`, atau `echo "Server started on $PORT"`). | Log: `the process ended (exit 0)` lalu petunjuk "The app ended without listening on port …" dengan saran `PORT=":{port}"`; petunjuk yang sama di kartu. Dengan `PORT=":{port}" <command>` project Running. | ⬜ | ⬜ | | v3.21.2 |

## D · Services (canvas D1–D5, ditambah RAM v3.10.1)

| ID | Langkah | Hasil yang diharapkan | W | L | Bukti | Catatan |
|---|---|---|---|---|---|---|
| TC-D1 | Buka Services. | Semua service tampil (grup core/additional) dengan toggle; pemilih profil di header. | ⬜ | ⬜ | | |
| TC-D2 | Ganti profil, lalu klik tombol utama; setelah semua berjalan klik lagi. | Toggle mengikuti profil; tombol *Start <profil>* dengan jumlah yang belum berjalan; setelah semua berjalan tombolnya *Stop <profil>* dan hanya menghentikan pilihan itu. Pilihan berbeda dari profil = *Start selected* / *Stop selected*. | ⬜ | ⬜ | | v3.23.0 (canvas D2) |
| TC-D3 | Amati saat service mulai (pertama kali: image belum ada). Ulangi dengan port service dipakai program lain. | Tiap baris punya status sendiri; image diunduh dulu. Port bentrok → banner "Port … is already in use" di detail service; bila start tetap gagal, pesan menyebut port dan service-nya ("Ports 3306 (mysql) … already used by another program"), bukan "docker compose failed (exit …)". | ⬜ | ⬜ | | |
| TC-D4 | Buka detail MySQL → Connect. Pilih format Laravel, Copy .env, lalu Copy URL. | Host, port, user, password, database tampil; potongan `.env` sesuai format; URL `mysql://root@127.0.0.1:3306/…`. | ⬜ | ⬜ | | |
| TC-D5 | Tempel hasil Copy .env ke project. | Toast menyebut jumlah baris yang disalin; nilai bisa langsung dipakai project (lihat C4). | ⬜ | ⬜ | | |
| TC-D6 | Dengan service berjalan, diamkan di layar Services; arahkan kursor ke angka RAM. Lalu sembunyikan jendela ke tray. | Angka RAM berubah sekitar tiap 2 detik; tooltip "measured at HH:MM:SS". Service berhenti → angka `~` dengan tooltip estimasi. | ⬜ | ⬜ | | v3.10.1 |
| TC-D7 | Menu ⋯ di samping tombol utama → Stop all services. | Semua berhenti, termasuk yang tidak dipilih; RAM di ringkasan dan status bar menjadi 0. Item nonaktif bila tidak ada yang berjalan. | ⬜ | ⬜ | | v3.10.1, v3.23.0 |
| TC-D8 | Engine Docker (**W:** Docker Desktop, **L:** Docker Engine). Pasang v3.15.2 atau lebih lama, start MySQL dan isi satu tabel; update ke v3.15.3+, Start lagi. Lalu `docker port servel_mysql`. Dari perangkat lain di LAN coba sambung ke port 3306. | Container dibuat ulang sekali saat start; tabel tetap ada (volume sama). `docker port` menunjukkan `127.0.0.1:3306`, bukan `0.0.0.0`; dari LAN tidak bisa tersambung. Log pull image hanya berisi baris Pulling/Pulled dan status container. | ⬜ | ⬜ | | v3.15.3 |
| TC-D9 | Engine Docker: nyalakan MySQL, hapus folder `~/.local/state/kova` (atau reboot pada versi lama yang memakai `/tmp`), lalu matikan switch MySQL dan Stop all services. | Keduanya menghentikan container (tidak ada pesan "compose file not found"). File compose baru berada di `~/.local/state/kova/servel/` dengan izin 0600, bukan di `/tmp`. | ⬜ | ⬜ | | v3.25.1 |

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
| TC-E9 | Runtimes → PHP → Install 8.4 (atau Node 22 bila PHP lama). Saat banner "Installing …" tampil, buka Logs, tunggu beberapa detik, kembali ke Runtimes. Ulangi, tetapi tetap di Logs sampai install selesai. | Kembali saat install berjalan: banner, baris progres mise, dan Cancel masih tampil; field Install nonaktif. Selesai saat di Logs: toast tetap muncul, dan saat kembali versi baru tampil dengan chip `new`. | ⬜ | ⬜ | | v3.15.4 |
| TC-E10 | (a) Tool tanpa versi global (`mise ls --current` tidak menampilkannya), Install satu versi. (b) Tool yang sudah punya versi global, Install versi lain. | (a) Versi baru bertanda Global; toast "… and set it as the global version"; `php -v` (atau tool itu) di terminal baru langsung jalan; tray ikut menampilkan versi itu. (b) Versi global lama tidak berubah; toast hanya "Installed …". | ⬜ | ⬜ | | v3.15.4 |
| TC-E11 | Runtimes → PHP → ⋯ pada satu versi → Use in project…, dengan project Laravel, project Vite, dan project tanpa jenis. | Hanya project PHP (atau yang mise.toml-nya sudah menyebut PHP) yang tampil sebagai radio; **Show all projects** menampilkan semuanya; tombol **Use PHP x** menulis mise.toml; panel extension pindah ke versi itu. | ⬜ | ⬜ | | v3.16.0 |
| TC-E12 | Nyalakan dua extension berurutan dengan cepat. | Baris pertama menampilkan *checking…* dan terkunci; toggle kedua tetap bisa diklik; keduanya selesai dengan hasil benar. | ⬜ | ⬜ | | v3.16.0 |
| TC-E13 | Project dengan `mise.toml` yang meminta versi PHP/Node belum terpasang; buka Runtimes. | Baris versi itu bertanda *not installed* dengan tombol Install langsung (bukan menu ⋯); Install menampilkan banner progres. Saat memuat tertulis *Loading installed … versions…*. | ⬜ | ⬜ | | v3.24.0 |
| TC-E14 | Install versi yang tidak ada (mis. PHP 9.9). | Banner *Could not install PHP 9.9* dengan pesan mise dan tombol Retry; Retry mengulang install. | ⬜ | ⬜ | | v3.26.0 |

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
| TC-F7 | Jalankan satu project dan satu service, lalu update dengan Restart now (Stop services when Kova quits menyala). Ulangi dengan setelan itu mati. | Log mencatat "update: stopping services" / "services stopped"; setelah Kova terbuka lagi, port project bebas dan Start langsung berhasil. Dengan setelan mati, service tetap berjalan. | ⬜ | ⬜ | | v3.16.2 |

## G · Logs (canvas G1–G4)

| ID | Langkah | Hasil yang diharapkan | W | L | Bukti | Catatan |
|---|---|---|---|---|---|---|
| TC-G1 | Buka Logs dengan beberapa project dan service berjalan. | Semua sumber digabung dan diurutkan waktu; warna kolom sumber sesuai jenisnya. | ⬜ | ⬜ | | |
| TC-G2 | Tab Projects → chip satu project. | Hanya output project itu; badge di chip = jumlah error; tombol Open project muncul. | ⬜ | ⬜ | | |
| TC-G3 | Pilih Errors & warnings (All / Errors & warnings). | Baris biasa disembunyikan; footer menandai hasil terfilter. | ⬜ | ⬜ | | |
| TC-G4 | Ketik teks di filter, klik Copy, lalu Clear. | Filter teks dan level bisa digabung; Copy hanya menyalin baris yang terlihat; di tab All tombolnya *Clear all* dan mengosongkan semua sumber. Project dan service tetap berjalan. | ⬜ | ⬜ | | v3.22.0 |
| TC-G5 | Matikan Follow lalu scroll ke atas; nyalakan lagi. Export logs. | Follow off = tidak lompat ke bawah; on = ikut baris baru. Footer "Last 500 lines per source". Export menghasilkan file. | ⬜ | ⬜ | | |
| TC-G7 | Pilih MySQL dan Mailpit di Services, tambah satu custom service, jalankan semuanya; buka Logs. | Tab: All, Projects, MySQL, Mailpit, nama custom service, mise (bila ada baris mise), Kova. Tidak ada tab PHP atau Node. Tiap tab hanya berisi baris sumber itu. Hapus custom service saat tabnya terbuka → kembali ke All. | ⬜ | ⬜ | | v3.22.0 |
| TC-G8 | Tab MySQL → **Clear MySQL**; lalu Undo. Ulangi di tab Projects dengan chip satu project. | Hanya baris MySQL hilang; toast "Cleared MySQL logs (N lines)" dengan Undo mengembalikannya, baris baru yang masuk sesudahnya tetap ada. Di chip project tombolnya *Clear <project>*. | ⬜ | ⬜ | | v3.22.0 |
| TC-G9 | Clear MySQL saat sebuah project terus mencetak log; tunggu ±10 baris baru; Undo. Ulangi: Clear MySQL, Clear Kova, lalu Undo pada toast pertama. | Baris MySQL kembali di posisi waktunya, baris project yang baru tetap ada. Undo pada toast lama menampilkan "Only the latest Clear can be undone." dan tidak mengubah daftar. Badge error Logs tidak naik lagi untuk error yang sudah dilihat. | ⬜ | ⬜ | | v3.25.1 |
| TC-G6 | Buat satu project gagal start (error di Logs), lihat badge Logs. Perbaiki dan start lagi sampai running. Lalu buka Logs dan kembali. | Badge naik saat error; turun setelah project running kembali; nol setelah Logs dibuka. Saat project running muncul toast "… is running on localhost:PORT" dengan tombol Open yang membuka browser. | ⬜ | ⬜ | | v3.16.0 |
| TC-G10 | Logs dengan banyak baris; scroll ke atas, tunggu baris baru, lalu scroll ke paling bawah. | Follow mati saat scroll ke atas (baris baru tidak menarik ke bawah), menyala lagi di dasar. Menyalakan Follow langsung lompat ke baris terbaru. | ⬜ | ⬜ | | v3.26.0 |

## H · Environments (canvas H1–H6)

| ID | Langkah | Hasil yang diharapkan | W | L | Bukti | Catatan |
|---|---|---|---|---|---|---|
| TC-H1 | Detail project → tab Environment. | Pemilih Environment dengan Default; ada New environment. | ⬜ | ⬜ | | |
| TC-H2 | New environment: nama `tenant-acme`, Copy changes from = Nothing, Create environment. Coba juga Esc. | Environment dibuat dan terpilih; Esc/Cancel membatalkan. Nama tidak valid ditolak. | ⬜ | ⬜ | | |
| TC-H3 | Di `tenant-acme` ubah satu nilai dan tambah satu variabel baru; Save & restart. | Nilai Default tampil sebagai placeholder (tanpa badge); hanya yang berbeda disimpan. Mengosongkan override = kembali ke Default. | ⬜ | ⬜ | | |
| TC-H4 | Edit as .env, tempel beberapa baris termasuk `PORT=…` dan `export X=1`; Save. Lalu tempel blok `.env` langsung di kolom Name. | Ringkasan "N added · N replaced · N skipped"; `PORT` dilewati; Edit as rows kembali ke daftar. | ⬜ | ⬜ | | |
| TC-H5 | Saat project berjalan dengan Default, pilih `tenant-acme`. | Banner "Running with Default · Restart <project> to use tenant-acme. Nothing changes until then." dengan tombol Restart; proses tetap memakai Default sampai Restart. | ⬜ | ⬜ | | |
| TC-H6 | Klik Restart. | Kartu project, header detail, dan popup tray menampilkan `tenant-acme`; log mencatat `env (tenant-acme): …`. | ⬜ | ⬜ | | |
| TC-H7 | Isi `APP_ENV=local` di Default, sementara `.env` project berisi `APP_ENV=production` dan `DB_HOST=…`. Start project (Laravel), cek `env('APP_ENV')` dan `env('DB_HOST')`. | `APP_ENV` = `local` (dari Kova); `DB_HOST` tetap dari `.env`. Teks tab menjelaskan aturan ini. | ⬜ | ⬜ | | v3.16.1 |
| TC-H8 | Default dengan lebih dari 10 variabel; ketik sebagian nama atau nilai di Search. Lalu Add variable saat Search terisi. | Search hanya muncul di atas 10 baris; daftar tersaring; "No variable matches" bila kosong; baris baru tetap tampil. | ⬜ | ⬜ | | v3.16.1 |
| TC-H10 | Ubah satu nilai di tab Environment tanpa Save, lalu: klik project lain; klik kartu yang sama; klik dua kali pemisah; tekan Esc di daftar; buka Services; ganti environment. | Setiap kali muncul *Save changes to Default?* (atau nama environment). Stay here = tetap di tempat dengan isian utuh; Discard changes = isian kembali ke yang tersimpan lalu lanjut; Save = tersimpan lalu lanjut. Nama variabel tidak valid saat Save = dialog tertutup, tetap di tab Environment dengan pesan error. | ⬜ | ⬜ | | v3.22.0 |
| TC-H9 | Di environment bernama, override dua nilai Default dan tambah satu variabel baru; Save, pindah ke Default lalu kembali. | Kelompok *Set in <env>* berisi dua override lalu variabel baru; kelompok *From Default* berisi baris Default lain. Tidak ada garis accent per baris. Baris tidak berpindah saat mengetik; Add variable menambah baris di kelompok pertama dan memfokuskannya. | ⬜ | ⬜ | | v3.16.1, v3.23.0 |

## I · Port & Terminal (canvas Flow I1–I11)

| ID | Langkah | Hasil yang diharapkan | W | L | Bukti | Catatan |
|---|---|---|---|---|---|---|
| TC-I1 | Jalankan `npx http-server -p 3001` di terminal, lalu Start project yang memakai 3001. | Banner "Port 3001 is used by another program" menyebut `node.exe`/`node` dan PID-nya; tombol Use 3002, Stop node…, Check again. | ⬜ | ⬜ | | v3.18.0 |
| TC-I2 | Dari TC-I1 klik Stop node… lalu Stop program and start. Ulangi dengan Use 3002. | Dialog menampilkan program, command, PID; program berhenti dan project jalan di 3001. Use 3002: port tersimpan, project jalan di 3002, program lain tetap jalan. | ⬜ | ⬜ | | v3.18.0 |
| TC-I3 | Project Vite dengan command `npm run dev` (tanpa `--port`), port project 5174. | Status not answering; banner "Nothing answers on :5174 · The app printed http://localhost:5173/"; Use 5173 mengganti port dan restart → running. | ⬜ | ⬜ | | v3.18.0 |
| TC-I4 | Jalankan MySQL lokal (atau program lain) di 3306, Start service MySQL. | Banner menyebut program + PID; Change port… membuka dialog dengan port bebas; Change port and start → MySQL jalan di port baru, Connect dan Copy .env memakai port baru. | ⬜ | ⬜ | | v3.18.0 |
| TC-I5 | Dari TC-I4 buka Change port… lagi, klik Use the default port. | MySQL kembali ke 3306 (container dibuat ulang, data tetap). | ⬜ | ⬜ | | v3.18.0 |
| TC-I6 | Project dengan variabel di tab Environment: Terminal here (panel dan menu ⋯). Cek `echo $env:APP_ENV` / `echo $APP_ENV`, `php -v`. Ulangi untuk project Run inside WSL. | Baris pertama "Kova: … from <project> > Default"; nilai sama dengan tab Environment; versi PHP/Node sesuai project; script sementara sudah terhapus. WSL: terminal di distro dengan variabel yang sama. | ⬜ | ⬜ | | v3.18.0 |
| TC-I12 | Isi `DB_PORT=3306` di Environment dua project; Services › MySQL › Change port… ke 3307. Ulangi lewat menu ⋯ di daftar Services. | Connect menampilkan *2 projects use 3306* dengan nama keduanya dan ajakan memperbarui ke 3307; Done menutupnya. Dari menu ⋯, panel MySQL terbuka dengan pesan yang sama. | ⬜ | ⬜ | | v3.26.0 |

## J · Custom service & menu ⋯ (canvas Flow J1–J8)

| ID | Langkah | Hasil yang diharapkan | W | L | Bukti | Catatan |
|---|---|---|---|---|---|---|
| TC-J1 | Services › Add service: Elasticsearch, `elasticsearch:8.15.0`, port 9200/9200, ENV `discovery.type=single-node` + `xpack.security.enabled=false`, data `/usr/share/elasticsearch/data`, Ready when HTTP `/_cluster/health`, Connect `ELASTICSEARCH_URL=http://localhost:{port}`. Add and start. | Grup *custom* berisi Elasticsearch (chip custom); status starting → running setelah HTTP menjawab; Connect menampilkan URL dan nama network; `curl localhost:9200` menjawab. | ⬜ | ⬜ | | v3.19.0 |
| TC-J2 | Tambahkan Elasticsearch ke profil, Stop all, Start profil; restart Kova. | Ikut start bersama profil; definisinya tetap ada setelah restart Kova; data index tetap ada. | ⬜ | ⬜ | | v3.19.0 |
| TC-J3 | Menu ⋯ MySQL › Open shell. Juga pada Elasticsearch. | Terminal baru di dalam container (prompt bash/sh); `mysql -uroot` jalan. | ⬜ | ⬜ | | v3.19.0 |
| TC-J4 | Menu ⋯ MySQL › Delete data… dengan backup dicentang. | Fokus awal Cancel; setelah Delete data: file `kova-mysql-<waktu>.sql` di Downloads, MySQL start lagi kosong (database lama hilang); toast menyebut lokasi backup. | ⬜ | ⬜ | | v3.19.0 |
| TC-J5 | Menu ⋯ Elasticsearch › Edit service… ganti port di PC ke 9201, Save and restart. Lalu Remove service… tanpa dan dengan Also delete its data. | Port baru dipakai; Remove menghapus container dan definisi; dengan centang, volume `servel_elasticsearch_data` ikut terhapus. | ⬜ | ⬜ | | v3.19.0 |
| TC-J6 | Menu ⋯ › Restart dan View logs pada service yang berjalan. | Restart = stop lalu start; View logs membuka panel service di tab Logs. | ⬜ | ⬜ | | v3.19.0 |
| TC-J7 | Add service dengan image yang lama siap (mis. `postgres:17` sebagai custom), Ready when: **Log line** = `ready to accept connections`; Add and start. Coba juga teks yang tidak pernah muncul. | Status *starting* sampai teks muncul di log container, lalu running. Teks yang tidak muncul: setelah 120 detik baris log "readiness check has not passed…" dan service dianggap siap. Field kosong ditolak. | ⬜ | ⬜ | | v3.24.0 |
| TC-J8 | Add service dengan nama `Acme Search` (id `acme-search`) dan env `PASSWORD=Pa$sw0rd #1`; Add and start. Edit service, lalu nyalakan Redis. | Baris tampil running dan RAM terukur; `docker exec servel_acme_search env` menulis `PASSWORD=Pa$sw0rd #1` utuh; Edit menampilkan nilai yang sama; menyalakan Redis tidak menghapus container acme-search. Hanya satu start di log. | ⬜ | ⬜ | | v3.25.1 |

## K · Project container mode (canvas Flow K1–K7)

| ID | Langkah | Hasil yang diharapkan | W | L | Bukti | Catatan |
|---|---|---|---|---|---|---|
| TC-K1 | Project Go dengan Dockerfile (app listen di 8080). Project settings › Run with: Dockerfile, Save, Start. | Log menampilkan `$ … build -t kova/<nama> …` lalu `$ … run --rm --name kova-<nama> …`; status Running; `localhost:8080` menjawab; Overview: Runs in Container. | ⬜ | ⬜ | | v3.20.0 |
| TC-K2 | Isi `APP_ENV=local` di tab Environment; cetak dari app di container. | Nilai terbaca di container; `PORT` = port project. | ⬜ | ⬜ | | v3.20.0 |
| TC-K3 | Stop, lalu Start lagi; ulangi 3 kali. Cek `docker ps -a` / `wslc list`. | Tidak ada container `kova-<nama>` yang tertinggal setelah Stop; Start berikutnya langsung jalan. | ⬜ | ⬜ | | v3.20.0 |
| TC-K4 | Buat error kompilasi di kode, Start. | Status exited; banner dengan baris error terakhir; View log menampilkan output build. Perbaiki, Restart → jalan. | ⬜ | ⬜ | | v3.20.0 |
| TC-K5 | Engine Docker: Run with: Compose pada folder dengan `compose.yaml` (api + redis), nyalakan *Use Kova's redis*. Start, lalu Stop. Ganti engine ke WSL containers dan buka Run with: Compose. | `compose up --build --scale redis=0`; api menjangkau Kova redis sebagai `servel_redis`; Stop = compose down. Di WSL containers muncul peringatan "Compose needs the Docker engine". | ⬜ | ⬜ | | v3.20.0 |
| TC-K6 | Dua project Dockerfile bernama `Acme API` dan `acme-api`; start keduanya (port berbeda), stop yang pertama. | Container `kova-acme-api-<id>` berbeda untuk tiap project; keduanya berjalan bersamaan; menghentikan satu tidak memengaruhi yang lain. | ⬜ | ⬜ | | v3.25.1 |
| TC-K7 | Project Dockerfile: Start dan amati status saat build; buat error kompilasi lalu Start. Project Compose: Start dan buka Overview. | Status *building image · step N of M*; build gagal = banner *The image did not build* dengan langkah + View log, Open in VS Code, Retry. Overview menulis Network; compose mendaftar container dengan port dan status. | ⬜ | ⬜ | | v3.26.0 |

## L2 · Runtimes WSL & Add tool (canvas Flow L1–L6)

| ID | Langkah | Hasil yang diharapkan | W | L | Bukti | Catatan |
|---|---|---|---|---|---|---|
| TC-RW1 | Windows dengan distro Ubuntu-24.04 tanpa mise: Runtimes › Go › WSL · Ubuntu-24.04. | Empty state "mise is not installed in Ubuntu-24.04"; Install mise memasang mise (log di Logs › MISE); halaman lalu menampilkan versi Go di distro. | ⬜ | ➖ | | v3.21.0 |
| TC-RW2 | Di WSL · Ubuntu-24.04, Install Go 1.23; Set as global; Use in project (project WSL). | Versi terpasang di distro (`go version` di terminal Ubuntu); global di `~/.config/mise`; `mise.toml` project di distro berisi go. | ⬜ | ➖ | | v3.21.0 |
| TC-RW3 | Add tool › Bun › Install on Windows. Ulangi dengan Install on WSL. | Tab Bun muncul dan aktif; Bun latest terpasang dan jadi global; `bun --version` jalan di terminal baru. | ⬜ | ⬜ | | v3.21.0 |
| TC-RW4 | Add tool: ketik `java`, pilih; install versi `temurin-21`. | Diterima (bukan "version is not valid"); terpasang. | ⬜ | ⬜ | | v3.21.0 |
| TC-RW5 | Tab Bun › Remove tab; restart Kova. | Tab hilang dan tetap hilang; versi Bun yang terpasang masih ada di `mise ls`. | ⬜ | ⬜ | | v3.21.0 |
| TC-RW6 | Runtimes › PHP › WSL · Ubuntu-24.04 (mise terpasang di distro). Klik Reshim lalu Doctor. Kembali ke Windows. | Seksi menjadi *Version manager in Ubuntu-24.04* dengan versi mise distro; log Logs › mise menulis `mise reshim  (in Ubuntu-24.04)` dan laporan doctor distro. Panel samping *PHP extensions in Ubuntu-24.04* menjelaskan `php --ini`, tanpa daftar extension Windows. Di Windows, seksi dan panel kembali ke milik Windows. | ⬜ | ➖ | | v3.25.0 (canvas L8) |
| TC-RW7 | Distro tanpa mise yang punya `go`/`node` dari apt: Runtimes › WSL. Lalu distro dengan PHP: tab PHP › Open php.ini folder dan Copy php --ini. | Empty state menulis *Found on PATH in <distro>: go … · node …*. Open php.ini folder membuka folder php.ini distro di Explorer; tanpa PHP muncul toast error. Copy menyalin `php --ini`. | ⬜ | ➖ | | v3.26.0 |

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

## P · Kelompok & urutan project

Persiapan: 6+ project, misalnya `acme-api`, `acme-web`, `shop-api`, `shop-web`, `billing-worker`, `docs-site`.

| ID | Langkah | Hasil yang diharapkan | W | L | Bukti | Catatan |
|---|---|---|---|---|---|---|
| TC-P1 | Projects → New group, ketik "Acme", Enter. Ulangi, lalu Esc tanpa mengetik. | Kelompok Acme muncul dengan petunjuk putus-putus "Drag projects here, or use ⋯ → Move to group." Esc pada nama kosong membatalkan tanpa menyimpan. | ⬜ | ⬜ | | v3.15.0 |
| TC-P2 | ⋯ kelompok → Rename, ubah nama, Enter. Restart Kova. | Nama baru tampil dan bertahan setelah restart. | ⬜ | ⬜ | | v3.15.0 |
| TC-P3 | Lipat kelompok dengan panah, restart Kova. | Kelompok tetap terlipat; panah punya aria-expanded yang benar. | ⬜ | ⬜ | | v3.15.0 |
| TC-P4 | ⋯ kelompok berisi project → Delete group. | Kelompok hilang; project-nya pindah ke Ungrouped (terakhir). Ungrouped tidak punya Start all, Rename, atau Delete. | ⬜ | ⬜ | | v3.15.0 |
| TC-P5 | Kelompok dengan sebagian project berhenti: Start all; lalu ⋯ → Stop all. | Start all menjalankan yang bisa dijalankan dan hilang saat semua berjalan; meta "N · M running" berubah; Stop all menghentikan semuanya. | ⬜ | ⬜ | | v3.15.0 |
| TC-P6 | Seret pegangan ⋮⋮ `acme-web` ke atas `acme-api` di kelompok yang sama, lalu restart. | Kartu terangkat (latar lebih terang, garis aksen, bayangan), garis aksen 2 px di posisi tujuan; urutan baru bertahan. **W:** WebView2. **L:** WebKitGTK. | ⬜ | ⬜ | | v3.15.0 |
| TC-P7 | Seret `shop-web` dari Ungrouped ke kelompok Shop; tekan Esc di tengah seretan lain. | Project masuk ke Shop di posisi garis. Esc membatalkan seretan tanpa perubahan. | ⬜ | ⬜ | | v3.15.0 |
| TC-P8 | Tab ke pegangan `shop-api` ("Reorder shop-api. Alt+Up or Alt+Down to move"), tekan Alt+↓ dan Alt+↑. Coba juga ⋯ panel detail → Move up / Move down / Move to group › Shop, Ungrouped, New group…. | Project bergeser satu langkah di kelompoknya, fokus tetap di pegangan, pembaca layar mengumumkan "shop-api moved to position 2 of 3 in Shop". Item menu nonaktif di ujung. New group… membuat kelompok berisi project itu. | ⬜ | ⬜ | | v3.15.0 |
| TC-P9 | Ketik nama project dari kelompok yang terlipat di pencarian; lalu ketik nama kelompok. | Kelompok berisi hasil tampil terbuka, kelompok tanpa hasil tersembunyi; nama kelompok menampilkan project-nya. Menghapus pencarian mengembalikan status lipat semula. | ⬜ | ⬜ | | v3.15.0 |
| TC-P10 | Klik kartu yang sedang terpilih; klik lagi. Lalu Esc saat fokus di daftar. | Klik kedua menutup panel detail dan daftar melebar (kartu aria-pressed=false); tombol ‹ di tepi kanan membuka project yang sama. Esc juga menutup panel. | ⬜ | ⬜ | | v3.15.0 |

## T · Tray & jendela

| ID | Langkah | Hasil yang diharapkan | W | L | Bukti | Catatan |
|---|---|---|---|---|---|---|
| TC-T1 | Tutup jendela dengan "Keep running in the tray" aktif. | Jendela tersembunyi; project dan service tetap jalan. | ⬜ | ⬜ | | |
| TC-T2 | **W:** klik kiri/kanan ikon tray. **L:** klik ikon tray. | **W:** popup berisi project dan service dengan start/stop per baris, versi PHP/Node global. **L:** menu native. | ⬜ | ⬜ | | L: GNOME butuh ekstensi AppIndicator |
| TC-T3 | Quit dari tray dengan "Stop services when Kova quits" aktif. | Service berhenti sebelum Kova keluar; data volume tetap ada. | ⬜ | ⬜ | | |
| TC-T4 | Aktifkan "Start Kova when you sign in", logout/login. | Kova mulai sendiri (di tray bila minimize to tray aktif). | ⬜ | ⬜ | | |
| TC-T5 | Buka Kova untuk kedua kalinya. | Hanya satu instance; jendela yang ada muncul ke depan. | ⬜ | ⬜ | | |
| TC-T6 | Tambahkan lebih dari 5 project (idealnya 10+), jalankan 1–2 di antaranya; buka tray. | **W:** popup hanya menampilkan 5 baris project, yang berjalan/keluar di atas, lalu "Show all N projects". Klik → daftar lengkap bergulir di dalam kotak, kolom "Find a project" ikut muncul; Esc mengosongkan pencarian. Services, versi global, Open/Quit Kova tetap terlihat tanpa menggulir popup. **L:** 5 project di menu, sisanya di submenu "More projects (N)" (tanpa kelompok; dengan kelompok lihat TC-T9). | ⬜ | ⬜ | | v3.14.1 |
| TC-T7 | Dengan kelompok Acme dan Shop serta 10+ project, buka popup tray. | Tiap baris: titik status, logo framework 13 px, nama, petunjuk. 5 baris: yang aktif dulu, sisanya urut kelompok lalu Ungrouped. "Show all" menampilkan judul "ACME · 4 · 1 running", …, Ungrouped terakhir; kelompok yang terlipat di Kova tetap terbuka di tray. Tanpa kelompok tidak ada judul. | ⬜ | ➖ | | v3.15.0 |
| TC-T8 | Di Show all ketik "shop" di Find a project; Esc dua kali. NVDA: baca satu baris. | Semua project Shop tampil, judul tanpa hasil disembunyikan. Esc pertama mengosongkan, kedua menutup. NVDA membaca mis. "shop-api, running, Shop. Open localhost:3000". | ⬜ | ➖ | | v3.15.0 |
| TC-T9 | **L:** buat 2 kelompok dan 10+ project, buka menu tray. Hapus semua kelompok, buka lagi. | 5 project teratas langsung di menu; sisanya di submenu "Acme (N)", "Shop (N)", "Ungrouped (N)". Tanpa kelompok: satu submenu "More projects (N)". | ➖ | ⬜ | | v3.15.0 |

## U · Tampilan & aksesibilitas

| ID | Langkah | Hasil yang diharapkan | W | L | Bukti | Catatan |
|---|---|---|---|---|---|---|
| TC-U1 | Settings → General → Appearance → Interface size: Compact, Default, Large. | Teks dan kontrol di semua jendela ikut berubah; pilihan tersimpan setelah restart. | ⬜ | ⬜ | | |
| TC-U2 | Buka semua tab Settings (General, Services, About). | Tidak ada kartu bersarang; teks tidak menempel ke tepi kartu. | ⬜ | ⬜ | | v3.10.2–3.10.3 |
| TC-U3 | Ctrl K, ketik nama project / perintah. | Palet terbuka, hasil bisa dipilih dengan keyboard, Esc menutup. | ⬜ | ⬜ | | |
| TC-U4 | Navigasi seluruh aplikasi hanya dengan keyboard (Tab, Shift+Tab, Enter, Space, Esc). | Fokus selalu terlihat; semua tombol dan toggle bisa dijangkau; modal mengunci fokus. | ⬜ | ⬜ | | |
| TC-U5 | **W:** jalankan NVDA, buka Projects, Services, Logs. | Nama tombol, status project, dan badge error dibacakan; perubahan status diumumkan. | ⬜ | ➖ | | v3.14.0 |
| TC-U6 | Ctrl K, ketik nama kelompok (mis. "shop"). | Project di kelompok itu tampil dengan logo 13 px dan label kanan "Shop"; Start group Shop / Stop group Shop muncul sesuai status. New group membuka Projects dengan kolom nama terfokus. Di Projects ada Hide/Show project details. | ⬜ | ⬜ | | v3.15.0 |
| TC-U7 | Jalankan 1 service dan 1 project; amati status bar di Projects dan Settings selama 10 detik. | "RAM <total> (services … + projects … + Kova …)" berubah kira-kira tiap 2 detik tanpa teks bergeser; tooltip "Services … · Projects … · Kova … — measured at HH:MM:SS". Angka Kova tetap ada tanpa service. | ⬜ | ⬜ | | v3.15.0 |
| TC-U8 | Sembunyikan jendela ke tray 30 detik, lalu buka lagi (pantau `docker stats`/`wslc stats` dan CPU Kova). | Saat tersembunyi tidak ada pengukuran; begitu jendela terlihat angka diperbarui lagi dalam ±2 detik. | ⬜ | ⬜ | | v3.15.0 |
| TC-U9 | **W:** NVDA aktif di Projects selama 30 detik tanpa interaksi. | NVDA tidak membacakan angka RAM berulang; perubahan status engine tetap diumumkan. | ⬜ | ➖ | | v3.15.0 |
| TC-U10 | Buka Projects dengan panel detail terbuka di jendela ±1100 px, project bernama panjang (≥ 24 karakter). | Tidak ada scroll horizontal; New group tampil sebagai ikon; nama project satu baris dengan elipsis (tooltip nama lengkap); port tetap terlihat di panel detail. Tidak ada kotak putih di pojok scrollbar. | ⬜ | ⬜ | | v3.15.1 |
| TC-U11 | Jalankan beberapa service, lihat footer Services dan status bar. | Status bar menulis `RAM <total> (services <X> + Kova <Y>)`; X sama dengan angka di footer Services. Y kira-kira sama dengan jumlah grup Kova + WebView2 Manager di Task Manager (W) atau Private di `smem`/smaps_rollup (L), selisih ≤ 15%. | ⬜ | ⬜ | | v3.15.1 |
| TC-U12 | (a) Ctrl K → "Add project". (b) Settings → Engine, pilih engine lain. (c) Services › MySQL › Data → Restore database, pilih file. (d) Settings → On launch, matikan "Start projects marked start on launch", tutup dan buka Kova. (e) Rusak config.json (mis. hapus satu kurung), buka Kova. | (a) Dialog Add project terbuka. (b) Dialog konfirmasi; Cancel tidak mengubah engine. (c) Dialog konfirmasi menyebut nama file sebelum restore berjalan. (d) Project bertanda tidak start; deskripsi menyebut nama project-nya. (e) Banner menyebut path config.json dan tombol Open folder; Services dan Runtimes tetap bisa dibuka. | ⬜ | ⬜ | | v3.16.0 |
| TC-U14 | Picu error: Services › ⋯ › Delete data… pada service yang sedang dipakai, atau Settings › About › Data folder › Open setelah folder dihapus. | Toast error bergaris merah, dibacakan NVDA sebagai alert, tetap tampil lebih dari 10 detik, Copy menyalin pesannya, × menutupnya. | ⬜ | ⬜ | | v3.22.0 |
| TC-U15 | Keyboard dan pembaca layar di Services dan Settings: Tab ke baris service lalu Enter; navigasi heading (NVDA: H) di Settings; tab di detail project. | Enter di nama service membuka detail; heading *On launch*, *Container engine*, dst. terbaca sebagai heading level 2; tab mengumumkan panelnya. Service yang berhenti menulis "–" di kolom RAM, tooltip menyebut perkiraan. | ⬜ | ⬜ | | v3.22.0 |
| TC-U16 | Perkecil jendela ke 900 px. Projects: klik kartu lain. Services: buka detail MySQL. | Panel detail Projects tetap di kanan dan menampilkan project yang diklik; Add project jadi ikon, pencarian di baris sendiri, tidak ada scroll horizontal. Services: status dan ⋯ setiap baris terlihat utuh; tombol header terbungkus, tidak terpotong. Label kelompok dan judul bagian ditulis huruf biasa. | ⬜ | ⬜ | | v3.23.0 |
| TC-U13 | Buka Services, panel detail satu service, popup tray, dan Ctrl K → ketik nama service. | MySQL, PostgreSQL, Redis, RabbitMQ, MongoDB, MinIO memakai logo resminya (bukan emoji); Mailpit dan Gotenberg ikon garis; logo jelas terbaca di tema gelap dan terang; nama service tetap tertulis di sampingnya. | ⬜ | ⬜ | | v3.16.0 |
| TC-U17 | Buat config.json read-only, lalu ubah satu toggle di Settings. | Setelah ±0,5 detik toggle kembali ke nilai semula dan toast error *Could not save the setting, so it was changed back…* muncul. | ⬜ | ⬜ | | v3.26.0 |

## W · Khusus Windows

| ID | Langkah | Hasil yang diharapkan | W | L | Bukti | Catatan |
|---|---|---|---|---|---|---|
| TC-W1 | Settings → General → Container engine: Auto, lalu WSL containers, lalu Docker (bila Docker Desktop terpasang). | Auto memilih WSL containers; status Running dengan versi; catatan bahwa image/volume WSL containers terpisah dari Docker Desktop. | ⬜ | ➖ | | |
| TC-W2 | Buka terminal baru, jalankan `kova status` dan `kova help`. | Perintah ditemukan di PATH dan menampilkan status service. | ⬜ | ➖ | | |
| TC-W3 | Jalankan Kova sebagai Administrator. | Peringatan Administrator di onboarding. | ⬜ | ➖ | | |
| TC-W4 | Restore database: pilih file `.sql`, MySQL, Restore. | Service dinyalakan bila perlu, menunggu koneksi, dump dijalankan; hasil tampil di kartu. | ⬜ | ➖ | | Ulangi di L sebagai TC-L7 |
| TC-W5 | Project dengan folder `\\wsl.localhost\<distro>\…` dan command relatif (mis. `go run .`). Start, Stop, ulangi 3 kali; cek `net use`. | Tidak ada peringatan UNC; command berjalan di folder project; setelah Stop tidak ada drive sementara yang tertinggal di `net use`. | ⬜ | ➖ | | v3.16.2 |
| TC-W6 | Command yang mencetak `RPC_S_SERVER_UNAVAILABLE` (mis. `wslc` saat WSL bermasalah). | Log menampilkan petunjuk "WSL did not answer. Run `wsl --shutdown`…"; kartu project menampilkan petunjuk itu. | ⬜ | ➖ | | v3.16.2 |
| TC-W7 | Add project dari `\\wsl.localhost\Ubuntu-24.04\home\…` (project Go), command `go run .`, port 8080. | Switch *Run inside WSL (Ubuntu-24.04)* menyala; Overview menampilkan *Runs in: WSL · Ubuntu-24.04* dan versi Go dari distro; Start → log `(in Ubuntu-24.04:/home/… in WSL)`, status Running, `localhost:8080` terbuka dari browser Windows. | ⬜ | ➖ | | v3.17.0 |
| TC-W8 | Isi `HTTP_ADDR` dan satu variabel lain di tab Environment; Start; cetak variabelnya dari aplikasi (atau command `env \| grep HTTP_ADDR`). | Nilai dari Kova terbaca di dalam distro; `PORT` = port project. | ⬜ | ➖ | | v3.17.0 |
| TC-W9 | Stop project WSL yang sedang jalan, lalu Start lagi; ulangi 3 kali. Cek `ps -ef` di distro. | Tidak ada proses yang tertinggal; port bebas; Start berikutnya langsung berhasil. | ⬜ | ➖ | | v3.17.0 |
| TC-W10 | Project WSL: Terminal here, Open in VS Code. Project lama dengan command `wslc …`: switch mati, Start. | Terminal terbuka di distro pada folder project; VS Code membuka Remote WSL; project `wslc` tetap jalan dengan program Windows. | ⬜ | ➖ | | v3.17.0 |
| TC-W11 | Project WSL dengan variabel di tab Environment: Terminal here, lalu `echo $PORT` dan satu variabel project. Ulangi untuk service: menu ⋯ → Open shell tanpa Windows Terminal. | Jendela tetap terbuka di folder project dan bisa diketik; `PORT` dan variabel project terbaca; jendela shell service juga tetap terbuka. | ⬜ | ➖ | | v3.21.2 |
| TC-W12 | Hapus `…\Kova\cli\bin` dari PATH user (Environment Variables), buka Settings › General › Command line. Klik Add to PATH, buka terminal baru, `kova help`. | Baris kova command menulis *Not on your PATH* dan tombol *Add to PATH*; setelah diklik toast muncul, deskripsi menjadi *On your PATH…*, `kova help` jalan di terminal baru. Hanya ada satu penjelasan perintah kova di Settings, tanpa menyebut servel. | ⬜ | ➖ | | v3.24.0 |

## L · Khusus Linux (Docker Engine, tanpa Docker Desktop)

Catat di *Catatan putaran*: distro, desktop environment, `docker version`, `docker compose version`, `docker context ls`.

| ID | Langkah | Hasil yang diharapkan | W | L | Bukti | Catatan |
|---|---|---|---|---|---|---|
| TC-L1 | `chmod +x Kova_*.AppImage` lalu jalankan. | Kova terbuka. Bila gagal, catat pesan error dan paket yang hilang (mis. `libfuse2`). | ➖ | ⬜ | | |
| TC-L2 | Docker Engine aktif (`systemctl is-active docker`), user di grup `docker`. Buka Services, start MySQL. | Engine Docker berjalan; service start lewat `docker compose`; `docker context ls` menunjukkan `default`, bukan `desktop-linux`. | ➖ | ⬜ | | |
| TC-L3 | `sudo systemctl stop docker`, lalu buka Kova / Check again. | Instruksi `sudo systemctl start docker` dengan Copy; setelah daemon dinyalakan, Check again → siap. | ➖ | ⬜ | | |
| TC-L4 | User **tanpa** grup `docker` (`sudo gpasswd -d $USER docker`, login ulang). Buka Kova. | Pesan jelas berisi instruksi `sudo usermod -aG docker $USER` dan perlu login ulang. Status engine "No access" di Settings. | ➖ | ⬜ | | v3.12.0 |
| TC-L5 | Tanpa plugin Compose v2 (`docker compose version` gagal). Start service. | Pesan jelas bahwa plugin Compose v2 dibutuhkan dan cara memasangnya (`sudo apt install docker-compose-plugin`). Start service memberi pesan yang sama. | ➖ | ⬜ | | v3.12.0 |
| TC-L6 | Settings → General → Command line → kova command → Install, lalu buka terminal baru: `kova status`. | `kova` tersalin ke `~/.local/bin/kova` dan berjalan. Bila `~/.local/bin` belum ada di PATH, Settings menampilkan baris `export PATH=…` untuk profil shell. | ➖ | ⬜ | | |
| TC-L7 | Restore database seperti TC-W4. | Sama dengan TC-W4. | ➖ | ⬜ | | |
| TC-L8 | Jalankan `Kova_<versi>_amd64.AppImage` di distro dengan glibc 2.35 (mis. Ubuntu 22.04) dan di distro terbaru. | Aplikasi terbuka di keduanya, tanpa `GLIBC_x.y not found`. Bila gagal, kirim output `ldd --version`. | ➖ | ⬜ | | v3.15.2 |
| TC-L9 | (a) Dengan tailscaled mendengarkan 3306/6379/5432/5672 di IP Tailscale, Start profil berisi MySQL, Redis, PostgreSQL, RabbitMQ. (b) Dengan MySQL bawaan Linux di 127.0.0.1:3306, Start MySQL. | (a) Keempat service running; `ss -ltnp` menunjukkan docker-proxy di 127.0.0.1. (b) Peringatan port dipakai program lain sebelum start; bila dipaksa, pesan menyebut port dan service-nya, bukan "exit status". Log pull tanpa ratusan baris "Downloading". | ➖ | ⬜ | | v3.15.3 |
| TC-L10 | Lihat status bar setelah Kova terbuka ±1 menit. | Angka Kova ratusan MB, bukan GB; kira-kira sama dengan `ps`/System Monitor untuk proses kova + WebKit. | ➖ | ⬜ | | v3.15.3 |
| TC-L11 | (a) Runtimes → PHP → Install 8.4 dari AppImage, dengan paket build terpasang (dari terminal `mise install php@8.4` berhasil). (b) Ulangi tanpa paket build. (c) Buka Terminal here dari Kova, jalankan `echo $LD_LIBRARY_PATH $APPDIR`. | (a) Berhasil, sama dengan dari terminal. (b) Gagal dengan pesan yang menyertakan daftar paket apt/dnf. (c) Keduanya kosong. | ➖ | ⬜ | | v3.15.3 |
| TC-L12 | Rusak config.json (hapus satu kurung), buka Kova, lalu ubah satu setting. Juga Terminal here tanpa terminal yang dikenal. | Ada `config.json.corrupt-<waktu>` berisi isi lama sebelum config ditimpa. Terminal here gagal dengan pesan, tanpa skrip `kova-term-*` tertinggal; `control.json` berizin 0600. | ➖ | ⬜ | | v3.25.1 |
