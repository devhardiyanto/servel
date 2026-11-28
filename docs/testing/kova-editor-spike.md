# Kova Editor · checklist spike

Spike = prototipe yang tidak dirilis. Tujuannya menjawab satu pertanyaan sebelum MVP dibangun: apakah Kova Editor (Tauri + CodeMirror 6) benar-benar lebih hemat RAM dari VS Code dan tetap enak dipakai di Windows. Hasilnya keputusan **lanjut** atau **evaluasi ulang** (target ada di canvas, papan W8).

## Cara menyalakan

1. Pasang build dari CI pull request spike (artifact `servel-windows-*`), bukan dari rilis.
2. Settings › About › Diagnostics › **Kova Editor (spike)** → nyalakan.
3. Buka detail project → tombol **Kova Editor** di bawah panel.

## Language server (spike 2)

Di spike, server dipasang sendiri (MVP akan memasangnya dari Kova dengan checksum). Kova mencarinya di PATH:

| Bahasa | Program | Pasang |
|---|---|---|
| PHP | `phpactor` | Ikuti petunjuk Windows di phpactor.github.io (butuh PHP 8.1 ke atas di PATH) |
| TypeScript, JavaScript | `vtsls` | `npm install -g @vtsls/language-server` |
| Vue | `vue-language-server` dan `vtsls` | `npm install -g @vue/language-server @vtsls/language-server` |
| Go | `gopls` | `go install golang.org/x/tools/gopls@latest` |

Bila server tidak ada, editor tetap jalan; status bar menulis "Language server not installed". Output server masuk ke Logs dengan sumber `LSP php`, `LSP vue`, dan seterusnya.

Sudah dibuktikan di Linux dengan server asli (test `cargo test vue_hybrid single_server -- --ignored`): file `.vue` mendapat error TypeScript dari vtsls dan completion di template lewat relay `tsserver/request`; jalur satu server (vtsls untuk `.ts`) meneruskan diagnostics dan completion. Phpactor belum diuji dengan server asli.

## Status per tahap

| Tahap | Isi | Uji |
|---|---|---|
| Spike 1 | Jendela editor per folder, Explorer, tab dengan preview, CodeMirror 6, simpan atomik dengan deteksi konflik, file berubah/terhapus di disk, Quick Open (Ctrl P), palet (Ctrl Shift P), pintasan browser WebView2 dimatikan | S1, S4 (sebagian), S5, S6 (tanpa LSP), S8, S12 |
| Spike 2 | Jembatan LSP di Rust: satu server per bahasa (Phpactor, vtsls, gopls), Vue dengan dua server digabung, request server dijawab Kova; diagnostics, completion, hover, F12 | S6 (dengan LSP), S9, S11 |
| Spike 3 | Terminal PowerShell (ConPTY, Ctrl `), pulih saat WebView2 crash dengan cadangan file belum disimpan, seret file dari Explorer Windows, versi WebView2 (palet › Kova: About Kova Editor) | S2, S3, S4, S7, S10 |

## Uji (isi kolom Hasil)

| No | Uji | Cara | Lulus bila | Hasil |
|---|---|---|---|---|
| S1 | Pintasan | Di editor: Ctrl F, Ctrl H, Ctrl D, Ctrl G, Ctrl P, Ctrl Shift P, Ctrl W, Ctrl S, Alt ↑/↓, Ctrl /, F5, Ctrl R, F12, Alt ← | Semua menjalankan perintah editor atau tidak terjadi apa-apa. Tidak ada UI browser (cari di halaman, cetak, muat ulang, devtools) | ⬜ |
| S2 | Crash WebView2 | Ketik di sebuah file tanpa menyimpan, tunggu 2 detik. Task Manager › Details › akhiri `msedgewebview2.exe` yang paling besar memorinya (renderer) | Jendela dimuat ulang sendiri kurang dari 3 detik, tab kembali dengan banner "Unsaved changes … are back". Bila semua `msedgewebview2.exe` diakhiri, jendela editor dibuka ulang (jendela utama Kova belum dipulihkan di spike) | ⬜ |
| S3 | IME | Microsoft IME Jepang, Pinyin, Korea di file teks | Tidak ada huruf ganda atau hilang (spike 3) | ⬜ |
| S4 | Seret | Seret file dari Windows Explorer ke editor; seret teks yang dipilih di dalam editor | File dari folder ini terbuka sebagai tab; file dari luar ditolak dengan pesan. Catat apakah seret teks di dalam editor masih jalan (penangan drop Tauri bisa mematikannya) | ⬜ |
| S5 | Buka/tutup | Buka dan tutup jendela editor 20 kali dari detail project | Tidak ada jendela putih, tidak macet; jendela utama tetap hidup | ⬜ |
| S6 | RAM | Lihat bagian RAM di bawah | Angka tercatat | ⬜ |
| S7 | Versi WebView2 | Di editor: Ctrl Shift P › Kova: About Kova Editor | Versi tercatat | ⬜ |
| S8 | Clipboard dan akhir baris | Salin/tempel teks besar (sekitar 10 MB); buka file CRLF, edit, simpan, cek dengan `git diff` | Isi utuh; file tetap CRLF | ⬜ |
| S9 | Vue | File `.vue`: error TypeScript di `<script setup>`, completion di template dan script, F12 ke komponen lain | Ketiganya jalan; tidak ada error dobel | ⬜ |
| S10 | Terminal | Ctrl ` → `php artisan`, `npm run dev` lalu Ctrl C, warna (`git log --oneline --graph --color`), pilih teks lalu Ctrl C (menyalin), Ctrl V, ubah ukuran jendela, Ctrl Shift ` (terminal kedua), tutup jendela editor saat `npm run dev` jalan | Output dan warna benar; Ctrl C menghentikan perintah; tidak ada proses `node` yang tertinggal setelah jendela ditutup | ⬜ |
| S11 | Phpactor | Project Laravel: completion setelah `->`, hover, F12 ke class lain, error di status bar | Completion pertama tidak lebih lambat dari VS Code; F12 membuka file lain sebagai tab | ⬜ |
| S12 | Folder besar | Ctrl P di project dengan `vendor` dan `node_modules` | Daftar file muncul kurang dari 2 detik; `vendor` dan `node_modules` ikut .gitignore | ⬜ |

Tambahan untuk spike 1:

- Simpan saat file sudah diubah program lain (mis. `git checkout` di terminal) → banner "changed on disk" dengan Overwrite with mine / Use disk version, bukan menimpa diam-diam.
- Hapus file dari terminal lalu kembali ke jendela editor → nama tab dicoret, Save to restore menulis ulang.
- File lebih dari 5 MB (mis. `storage/logs/laravel.log`) → terbuka tanpa highlight, jendela tetap lancar.
- Menutup jendela editor dengan file belum disimpan → dialog Save / Don't save / Cancel. Jendela utama tidak ikut tertutup.

## RAM (S6)

Angka "Kova" di status bar jendela utama sudah mencakup jendela editor, language server, dan terminal (semuanya proses anak Kova).


Ukur dengan angka RAM di status bar Kova (private working set, sama dengan Task Manager) dan Task Manager untuk VS Code (jumlah semua proses `Code.exe`).

| Skenario | VS Code | Kova + editor |
|---|---|---|
| Jendela kosong (tanpa folder) | | |
| Project Laravel, 5 file terbuka, tunggu 10 menit | | |
| Workspace terbesar, 5 file per stack, tunggu 10 menit | | |
| File 5.000 baris, mengetik 1 menit | | |

Spike 1 belum menjalankan language server, jadi bandingkan dulu dengan VS Code yang extension-nya dimatikan (`code --disable-extensions`). Perbandingan dengan extension (Intelephense, Vue) setelah spike 2.

## Keputusan setelah spike

Isi tabel RAM dan kolom Hasil, lalu kirimkan. Keputusan **lanjut ke MVP** bila: RAM Kova + editor + language server lebih hemat secara nyata dari VS Code dengan extension yang setara (target W8: sekitar 50%), S1, S2, S5, S9, S10, S11 lulus, dan tidak ada temuan yang memaksa ganti arah (mis. IME rusak di S3). Bila tidak, arah dievaluasi ulang sebelum MVP dibangun.
