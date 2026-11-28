# Spike AQ · Dumps (Experimental) · checklist uji

Tujuan: membuktikan `dump()` dan `dd()` dari project PHP bisa ditangkap Kova **tanpa mengubah file project**, sebelum tab Dumps dibangun penuh (AR). Hasil tiap baris: ✅ / ❌ / ⏭ dengan catatan singkat.

## Cara kerja (singkat)

1. Settings › About › Diagnostics › **Dumps (Experimental)** menulis file penanda `<folder data Kova>\dumps\enabled`.
2. Saat project PHP dengan `vendor/symfony/var-dumper` di-start, Kova:
   - menjalankan penerima `php <data>\dumps\receiver.php vendor\autoload.php 127.0.0.1:<port>` dengan PHP project (versi dari mise);
   - memberi dev server dan proses pendamping env `PHP_INI_SCAN_DIR` (ditambah folder `<data>\dumps\ini`, berisi `auto_prepend_file`), `KOVA_DUMPS=tcp://127.0.0.1:<port>`, dan `KOVA_DUMPS_REGISTRY`;
   - mencatat folder project → port di `<data>\dumps\projects.json`.
3. `prepend.php` mengisi `$_SERVER['VAR_DUMPER_FORMAT']` sehingga Laravel tidak memasang dumper HTML/CLI-nya, dan ServerDumper Symfony mengirim dump ke penerima. Bila penerima mati, dump kembali tampil seperti biasa.

Folder data Windows: `%APPDATA%\com.devhardiyanto.servel` (tombol **Open** di Diagnostics).

## Persiapan

- Project Laravel (Windows, bukan WSL) yang sudah `composer install`.
- Route uji di `routes/web.php` (hapus setelah uji):

```php
Route::get('/kova-dump', function () {
    dump(['id' => 42, 'roles' => ['admin', 'billing']]);
    return 'ok';
});
Route::get('/kova-dd', fn () => dd(request()->query()));
```

## Checklist

| No | Langkah | Yang diharapkan | Hasil |
|---|---|---|---|
| D1 | Nyalakan Dumps, start project (`php artisan serve`) | Tab **Dumps** muncul; status "Listening · PHP x · var-dumper y"; log project berisi `[dumps] receiver on 127.0.0.1:<port>` | ⬜ |
| D2 | Buka `/kova-dump` | Halaman hanya "ok" (dump tidak tampil di halaman); dump muncul di tab Dumps dengan `GET http://127.0.0.1:<port>/kova-dump` | ⬜ |
| D3 | Klik `routes/web.php:NN` di kartu dump | VS Code terbuka di baris itu | ⬜ |
| D4 | Buka `/kova-dd?x=1` | Dump `["x" => "1"]` muncul; halaman berhenti (500 kosong adalah perilaku `dd` lewat server) | ⬜ |
| D5 | Ganti command ke `composer run dev`, restart | D2 tetap lulus (artisan serve di dalam concurrently) | ⬜ |
| D6 | Proses pendamping `php artisan queue:listen` + job yang memanggil `dump()` | Dump muncul dengan nama perintah, bukan URL | ⬜ |
| D7 | Bila D2 gagal: tambahkan baris dari `<data>\dumps\ini\kova-dumps.ini` ke `php.ini` PHP project, restart | Dump muncul lewat jalur registry (artinya env disaring `artisan serve`) | ⬜ |
| D8 | Matikan Dumps di Settings, buka `/kova-dump` lagi | Dump tampil di halaman seperti biasa (tanpa Kova) | ⬜ |
| D9 | Dump besar (`dump(range(1, 50000))`) | Tampil, dipotong di 256 KB dengan catatan "Cut at 256 KB."; Kova tetap responsif | ⬜ |
| D10 | Project tanpa `vendor/` (belum `composer install`) | Tab Dumps tidak muncul; dev server tetap start | ⬜ |
| D11 | Quit Kova | Proses `php ... receiver.php` ikut berhenti (Task Manager) | ⬜ |

## Diagnosis bila gagal

- Log project (tab Logs) memuat baris `[dumps] …` dari stderr penerima.
- D2 gagal tapi D7 lulus: `php artisan serve` menyaring env worker (`variables_order` berisi `E`). Keputusan AR: tulis `auto_prepend_file` ke `php.ini` milik PHP yang dipasang Kova, dengan prepend yang diam bila folder tidak terdaftar.
- Dump tetap tampil di halaman: `VAR_DUMPER_FORMAT` sudah diset project sendiri (`.env`), atau Laravel versi lama yang tidak membaca `$_SERVER['VAR_DUMPER_FORMAT']`. Catat versi Laravel.

## Yang tidak dicakup spike

- Project di WSL dan mode container (Kova tidak menyuntikkan apa pun di sana).
- Pohon nilai yang bisa dibuka-tutup, Inspector query/job (AR, AS).
