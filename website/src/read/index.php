<?php
declare(strict_types=1);
// Single entry point for /read/. Nginx routes every non-asset request here.
namespace Comics;

require __DIR__ . '/_lib/core.php';
require __DIR__ . '/_lib/media.php';
require __DIR__ . '/_lib/views.php';
require __DIR__ . '/_lib/reader.php';
require __DIR__ . '/_lib/account.php';
require __DIR__ . '/_lib/creator.php';

ini_set('display_errors', '0');
header('X-Content-Type-Options: nosniff');
header('Referrer-Policy: strict-origin-when-cross-origin');
header('X-Frame-Options: SAMEORIGIN');

$path = rawurldecode(parse_url($_SERVER['REQUEST_URI'] ?? '/read/', PHP_URL_PATH) ?: '/read/');
$path = trim(substr($path, strlen('/read')), '/');
$method = $_SERVER['REQUEST_METHOD'] ?? 'GET';
$post = $method === 'POST';

try {
    if (str_starts_with($path, 'media/')) serve_media(substr($path, 6), user());
    if (str_starts_with($path, 'api/')) { api(substr($path, 4)); exit; }

    header('Cache-Control: private, no-cache');
    csrf();
    // Scheduled episodes go live on read; keep series ordering fresh without a cron job.
    run("UPDATE series SET last_episode_at = (SELECT MAX(publish_at) FROM episodes e WHERE e.series_id = series.id AND " . live_episode_sql('e') . ")
        WHERE id IN (SELECT series_id FROM episodes WHERE status = 'scheduled' AND hidden = 0 AND publish_at <= ? AND publish_at > COALESCE(series.last_episode_at, 0))", [now()]);
    $u = user();
    $seg = $path === '' ? [] : explode('/', $path);

    switch (true) {
        case $path === '': page_home($u); break;
        case $seg[0] === 'daily' && count($seg) <= 2: page_daily($u, $seg[1] ?? today_key()); break;
        case $seg[0] === 'genres' && count($seg) <= 2: page_genres($u, $seg[1] ?? ''); break;
        case $path === 'ranking': page_ranking($u); break;
        case $path === 'mature': page_mature($u); break;
        case $path === 'search': page_search($u); break;
        case $path === 'rules': page_rules(); break;
        case $seg[0] === 's' && count($seg) === 2: page_series($u, $seg[1]); break;
        case $seg[0] === 's' && count($seg) === 3 && ctype_digit($seg[2]): page_episode($u, $seg[1], (int)$seg[2]); break;
        case $seg[0] === 'u' && count($seg) === 2: page_profile($u, $seg[1]); break;
        case $path === 'login': $post ? do_login() : page_login(); break;
        case $path === 'register': $post ? do_register() : page_register(); break;
        case $path === 'logout' && $post: check_post(); end_session(); redirect('/read/', 'Signed out.');
        case $path === 'library': page_library(require_user()); break;
        case $path === 'settings': page_settings(require_user()); break;
        case $seg[0] === 'settings' && count($seg) === 2 && $post: do_settings(require_user(), $seg[1]); break;
        case $seg[0] === 'action' && count($seg) === 2 && $post: do_action($u, $seg[1]); break;
        case $path === 'admin': page_admin(require_user()); break;
        case $seg[0] === 'admin' && count($seg) === 2 && $post: do_admin(require_user(), $seg[1]); break;
        case $path === 'create': page_dashboard(require_user()); break;
        case $path === 'create/new': $post ? do_series_new(require_user()) : page_series_new(require_user()); break;
        case $seg[0] === 'create' && count($seg) === 2: page_series_manage(require_user(), $seg[1]); break;
        case $seg[0] === 'create' && count($seg) === 3 && $seg[2] === 'save' && $post: do_series_save(require_user(), $seg[1]); break;
        case $seg[0] === 'create' && count($seg) === 3 && $seg[2] === 'delete' && $post: do_series_delete(require_user(), $seg[1]); break;
        case $seg[0] === 'create' && count($seg) === 3 && $seg[2] === 'episode':
            $post ? do_episode_save(require_user(), $seg[1], null) : page_episode_form(require_user(), $seg[1], null); break;
        case $seg[0] === 'create' && count($seg) === 4 && $seg[2] === 'episode' && ctype_digit($seg[3]):
            $post ? do_episode_save(require_user(), $seg[1], (int)$seg[3]) : page_episode_form(require_user(), $seg[1], (int)$seg[3]); break;
        case $seg[0] === 'create' && count($seg) === 5 && $seg[2] === 'episode' && ctype_digit($seg[3]) && $seg[4] === 'delete' && $post:
            do_episode_delete(require_user(), $seg[1], (int)$seg[3]); break;
        default: error_page(404, 'There is nothing at this address.');
    }
} catch (Problem $p) {
    if ($post && $p->status === 422 && !wants_json()) redirect(back(), $p->getMessage(), 'error');
    if (wants_json()) { http_response_code($p->status); header('Content-Type: application/json'); echo json_encode(['error' => $p->getMessage()]); exit; }
    error_page($p->status, $p->getMessage());
} catch (\Throwable $t) {
    error_log('[scroll-comics] ' . $t);
    if (!headers_sent()) error_page(500, 'Please try again in a moment.');
}
