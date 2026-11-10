<?php
// Local preview only: php -S 127.0.0.1:8090 -t website/src website/tests/comics-router.php
// Mirrors the Nginx rules: static files are served directly, everything else under /read/ goes to read/index.php.
$path = parse_url($_SERVER['REQUEST_URI'], PHP_URL_PATH);
if (str_starts_with($path, '/read/_lib')) { http_response_code(404); exit; }
// build.mjs copies these from the repository's assets folder.
$copied = ['/assets/app-icon.png' => 'scroll-studio-icon.png', '/assets/Karla.ttf' => 'Karla.ttf'];
if (isset($copied[$path])) { header('Content-Type: ' . (str_ends_with($path, '.png') ? 'image/png' : 'font/ttf')); readfile(__DIR__ . '/../../assets/' . $copied[$path]); return true; }
if ($path === '/read') { header('Location: /read/'); exit; }
if (str_starts_with($path, '/read/') && !str_starts_with($path, '/read/assets/')) {
    $_SERVER['SCRIPT_NAME'] = '/read/index.php';
    require __DIR__ . '/../src/read/index.php';
    return true;
}
return false;
