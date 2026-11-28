<?php
// Kova Dumps (spike AQ): penerima. Dijalankan Kova dengan PHP milik project:
//   php receiver.php <vendor/autoload.php> <127.0.0.1:port>
// Membaca dump dari ServerDumper Symfony lewat DumpServer (PHP sendiri yang
// membuka format serialisasinya), lalu menulis satu baris JSON per dump ke
// stdout untuk Kova.

$out = static function (array $msg): void {
    echo json_encode($msg, JSON_UNESCAPED_SLASHES | JSON_INVALID_UTF8_SUBSTITUTE | JSON_PARTIAL_OUTPUT_ON_ERROR), "\n";
    fflush(STDOUT);
};

if ($argc < 3 || !is_file($argv[1])) {
    $out(['type' => 'error', 'message' => 'vendor/autoload.php not found']);
    exit(2);
}
require $argv[1];

if (!class_exists(\Symfony\Component\VarDumper\Server\DumpServer::class)) {
    $out(['type' => 'error', 'message' => 'symfony/var-dumper with DumpServer is not installed in this project']);
    exit(2);
}

use Symfony\Component\VarDumper\Cloner\Data;
use Symfony\Component\VarDumper\Dumper\CliDumper;
use Symfony\Component\VarDumper\Server\DumpServer;

$server = new DumpServer($argv[2]);
try {
    $server->start();
} catch (\Throwable $e) {
    $out(['type' => 'error', 'message' => $e->getMessage()]);
    exit(3);
}
$out(['type' => 'ready', 'php' => PHP_VERSION, 'varDumper' => class_exists(\Composer\InstalledVersions::class)
    ? (\Composer\InstalledVersions::getPrettyVersion('symfony/var-dumper') ?? '') : '']);

$dumper = new CliDumper(null, 'UTF-8', CliDumper::DUMP_LIGHT_ARRAY);
$dumper->setColors(false);
$limit = 256 * 1024;

$server->listen(static function (Data $data, array $context, int $clientId) use ($dumper, $out, $limit): void {
    $text = (string) $dumper->dump($data, true);
    $cut = strlen($text) > $limit;
    if ($cut) {
        $text = substr($text, 0, $limit);
    }
    $source = $context['source'] ?? [];
    $request = $context['request'] ?? null;
    $cli = $context['cli'] ?? null;
    $out([
        'type' => 'dump',
        'text' => rtrim($text, "\n"),
        'cut' => $cut,
        'time' => isset($context['timestamp']) ? (float) $context['timestamp'] : microtime(true),
        'file' => $source['file'] ?? null,
        'line' => isset($source['line']) ? (int) $source['line'] : null,
        'name' => $source['name'] ?? null,
        'request' => is_array($request) ? [
            'method' => $request['method'] ?? null,
            'uri' => $request['uri'] ?? null,
            'id' => $request['identifier'] ?? null,
        ] : null,
        'command' => is_array($cli) ? ($cli['command_line'] ?? null) : null,
        'client' => $clientId,
    ]);
});
