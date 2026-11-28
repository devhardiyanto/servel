# Kova Editor · checklist spike

Spike = prototipe yang tidak dirilis. Tujuannya menjawab satu pertanyaan sebelum MVP dibangun: apakah Kova Editor (Tauri + CodeMirror 6) benar-benar lebih hemat RAM dari VS Code dan tetap enak dipakai di Windows. Hasilnya keputusan **lanjut** atau **evaluasi ulang** (target ada di canvas, papan W8).

## Cara menyalakan

1. Pasang build dari CI pull request spike (artifact `servel-windows-*`), bukan dari rilis.
2. Settings › About › Diagnostics › **Kova Editor (spike)** → nyalakan.
3. Buka detail project → tombol **Kova Editor** di bawah panel.

## Status per tahap

| Tahap | Isi | Uji |
|---|---|---|
| Spike 1 | Jendela editor per folder, Explorer, tab dengan preview, CodeMirror 6, simpan atomik dengan deteksi konflik, file berubah/terhapus di disk, Quick Open (Ctrl P), palet (Ctrl Shift P), pintasan browser WebView2 dimatikan | S1, S4 (sebagian), S5, S6 (tanpa LSP), S8, S12 |
| Spike 2 | Jembatan LSP di Rust, Phpactor, Vue (vue_ls + vtsls dalam satu file) | S6 (dengan LSP), S9, S11 |
| Spike 3 | Terminal PowerShell (ConPTY), pulih saat WebView2 crash, IME, seret file dari Explorer Windows, laporan hasil | S2, S3, S4, S7, S10 |

## Uji (isi kolom Hasil)

| No | Uji | Cara | Lulus bila | Hasil |
|---|---|---|---|---|
| S1 | Pintasan | Di editor: Ctrl F, Ctrl H, Ctrl D, Ctrl G, Ctrl P, Ctrl Shift P, Ctrl W, Ctrl S, Alt ↑/↓, Ctrl /, F5, Ctrl R, F12, Alt ← | Semua menjalankan perintah editor atau tidak terjadi apa-apa. Tidak ada UI browser (cari di halaman, cetak, muat ulang, devtools) | ⬜ |
| S2 | Crash WebView2 | Task Manager › akhiri proses `msedgewebview2.exe` (renderer) saat ada file belum disimpan | Jendela pulih kurang dari 3 detik, isi kembali (spike 3) | ⬜ |
| S3 | IME | Microsoft IME Jepang, Pinyin, Korea di file teks | Tidak ada huruf ganda atau hilang (spike 3) | ⬜ |
| S4 | Seret | Seret file dari Windows Explorer ke editor; seret teks di dalam editor | Teks bisa dipindah (spike 1); file terbuka (spike 3) | ⬜ |
| S5 | Buka/tutup | Buka dan tutup jendela editor 20 kali dari detail project | Tidak ada jendela putih, tidak macet; jendela utama tetap hidup | ⬜ |
| S6 | RAM | Lihat bagian RAM di bawah | Angka tercatat | ⬜ |
| S7 | Versi WebView2 | `edge://version` tidak tersedia di app: catat dari Settings › About › Copy info | Versi tercatat (spike 3 menambah info ini) | ⬜ |
| S8 | Clipboard dan akhir baris | Salin/tempel teks besar (sekitar 10 MB); buka file CRLF, edit, simpan, cek dengan `git diff` | Isi utuh; file tetap CRLF | ⬜ |
| S9 | Vue | File `.vue`: completion dari Vue dan TypeScript sekaligus | Spike 2 | ⬜ |
| S10 | Terminal | `php artisan`, `npm run dev`, Ctrl C, ubah ukuran | Spike 3 | ⬜ |
| S11 | Phpactor | Project Laravel: indexing, completion, go to definition, rename | Completion pertama tidak lebih lambat dari VS Code (spike 2) | ⬜ |
| S12 | Folder besar | Ctrl P di project dengan `vendor` dan `node_modules` | Daftar file muncul kurang dari 2 detik; `vendor` dan `node_modules` ikut .gitignore | ⬜ |

Tambahan untuk spike 1:

- Simpan saat file sudah diubah program lain (mis. `git checkout` di terminal) → banner "changed on disk" dengan Overwrite with mine / Use disk version, bukan menimpa diam-diam.
- Hapus file dari terminal lalu kembali ke jendela editor → nama tab dicoret, Save to restore menulis ulang.
- File lebih dari 5 MB (mis. `storage/logs/laravel.log`) → terbuka tanpa highlight, jendela tetap lancar.
- Menutup jendela editor dengan file belum disimpan → dialog Save / Don't save / Cancel. Jendela utama tidak ikut tertutup.

## RAM (S6)

Ukur dengan angka RAM di status bar Kova (private working set, sama dengan Task Manager) dan Task Manager untuk VS Code (jumlah semua proses `Code.exe`).

| Skenario | VS Code | Kova + editor |
|---|---|---|
| Jendela kosong (tanpa folder) | | |
| Project Laravel, 5 file terbuka, tunggu 10 menit | | |
| Workspace terbesar, 5 file per stack, tunggu 10 menit | | |
| File 5.000 baris, mengetik 1 menit | | |

Spike 1 belum menjalankan language server, jadi bandingkan dulu dengan VS Code yang extension-nya dimatikan (`code --disable-extensions`). Perbandingan dengan extension (Intelephense, Vue) setelah spike 2.
