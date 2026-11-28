<?php
// Kova Dumps (spike AQ). Dimuat lewat auto_prepend_file oleh ini milik Kova,
// hanya untuk proses yang dijalankan Kova. Tidak mencetak apa pun dan tidak
// memuat kelas apa pun: hanya mengarahkan VarDumper ke penerima Kova.
//
// Bila penerima tidak mendengar, ServerDumper Symfony jatuh ke dumper biasa,
// jadi dump() tetap tampil di halaman atau terminal seperti tanpa Kova.
(static function () {
    if (isset($_SERVER['VAR_DUMPER_FORMAT'])) {
        return; // pilihan project sendiri menang
    }
    $target = getenv('KOVA_DUMPS');
    $via = 'env';
    if (!is_string($target) || $target === '') {
        // ServeCommand Laravel bisa menyaring env worker `php -S`, jadi cari
        // juga di daftar milik Kova berdasarkan folder kerja proses ini.
        $target = null;
        $via = 'registry';
        $registry = getenv('KOVA_DUMPS_REGISTRY');
        if (!is_string($registry) || $registry === '') {
            $registry = __DIR__ . DIRECTORY_SEPARATOR . 'projects.json';
        }
        $map = is_file($registry) ? json_decode((string) @file_get_contents($registry), true) : null;
        $dir = getcwd();
        if (is_array($map) && is_string($dir)) {
            for ($i = 0; $i < 4 && $target === null; $i++) {
                $key = strtolower(str_replace('\\', '/', rtrim($dir, '/\\')));
                if (isset($map[$key]) && is_string($map[$key])) {
                    $target = $map[$key];
                }
                $up = dirname($dir);
                if ($up === $dir) {
                    break;
                }
                $dir = $up;
            }
        }
    }
    if (!is_string($target) || strpos($target, 'tcp://127.0.0.1:') !== 0) {
        return;
    }
    $_SERVER['VAR_DUMPER_FORMAT'] = $target;
    $_SERVER['KOVA_DUMPS_VIA'] = $via;
})();
