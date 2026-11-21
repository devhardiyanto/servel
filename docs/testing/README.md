# Pengujian manual Kova

Dokumen ini dipakai untuk menguji Kova di perangkat sungguhan (Windows dan Linux) mengikuti flow A–H di canvas *Servel v3 UI*. Daftar kasusnya ada di [test-cases.md](test-cases.md).

Unit test dan build di CI tidak menggantikan uji ini. CI tidak menjalankan WSL containers, Docker Engine, winget, tray, atau browser sungguhan.

## Lingkungan yang diuji

| Kode | Sistem | Engine | Catatan |
|---|---|---|---|
| **W** | Windows 11 | WSL containers (`wslc`, WSL ≥ 3.0.1) | Docker Desktop opsional, hanya untuk kasus pindah engine. |
| **L** | Linux desktop (mis. Ubuntu 24.04, GNOME) | Docker Engine (`dockerd` lewat systemd) + plugin Compose v2 | Tanpa Docker Desktop. User ada di grup `docker`, kecuali di kasus yang sengaja menguji hal itu. |

Docker Desktop for Linux tidak wajib. Kova hanya memanggil CLI `docker` (`docker info`, `docker ps`, `docker compose …`), jadi daemon apa pun yang dituju CLI itu akan dipakai.

## Status

| Tanda | Arti |
|---|---|
| ⬜ | Belum diuji |
| ✅ | Lulus: hasil sesuai "Hasil yang diharapkan" |
| ❌ | Gagal: tulis apa yang terjadi di kolom Catatan, beserta nomor issue/PR bila ada |
| ⚠️ | Lulus sebagian, atau lulus dengan catatan |
| 🔜 | Celah yang sudah diketahui. Fiturnya dijadwalkan di milestone yang disebut, jadi gagal di sini belum dianggap regresi. |
| ➖ | Tidak berlaku untuk OS ini |

## Bukti

- Setiap status selain ⬜ dan ➖ harus punya bukti: screenshot, potongan log, atau output perintah.
- File bukti disimpan di `docs/testing/evidence/<versi>/<ID>-<W|L>.<ext>`, misalnya `evidence/v3.14.0/TC-A2-W.png`. Log pendek boleh langsung ditempel di Catatan.
- Kolom **Bukti** berisi tautan relatif ke file itu.
- Isi lembar **Catatan putaran** di bagian atas test-cases.md untuk setiap putaran: versi Kova, versi OS, engine, versi mise, dan tanggal.

Alurnya: penguji mengirim bukti (di chat atau di PR), lalu baris yang bersangkutan diperbarui dalam PR berikutnya. Satu milestone dianggap selesai bila semua baris miliknya ✅ atau ➖ di kedua OS.

## Cara menyiapkan ulang

- **Onboarding (flow A):** Settings → General → **Run setup again**. Untuk benar-benar seperti instalasi baru, tutup Kova lalu ganti nama folder data:
  - Windows: `%APPDATA%\com.devhardiyanto.servel`
  - Linux: `~/.config/com.devhardiyanto.servel` (config) dan `~/.local/share/com.devhardiyanto.servel` (data)
- **mise belum terpasang (A3–A5):** uji di user atau VM yang bersih. Jangan menghapus mise yang sedang dipakai project.
- **Update (flow F):** pasang versi sebelumnya dari halaman Releases, lalu buka Kova.
