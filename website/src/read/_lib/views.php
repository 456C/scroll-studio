<?php
declare(strict_types=1);
namespace Comics;

function h(?string $s): string { return htmlspecialchars((string)$s, ENT_QUOTES | ENT_SUBSTITUTE, 'UTF-8'); }
function media(?string $rel): string { return $rel ? '/read/media/' . $rel : ''; }
function compact_number(int $n): string {
    if ($n >= 1_000_000) return rtrim(rtrim(number_format($n / 1_000_000, 1), '0'), '.') . 'M';
    if ($n >= 1_000) return rtrim(rtrim(number_format($n / 1_000, 1), '0'), '.') . 'K';
    return (string)$n;
}
function ago(int $t): string {
    $d = now() - $t;
    if ($d < 60) return 'just now';
    if ($d < 3600) return intdiv($d, 60) . 'm ago';
    if ($d < 86400) return intdiv($d, 3600) . 'h ago';
    if ($d < 86400 * 7) return intdiv($d, 86400) . 'd ago';
    return date('M j, Y', $t);
}
function stars(array $s): string { return $s['rating_count'] > 0 ? number_format($s['rating_sum'] / $s['rating_count'], 1) : '–'; }
function csrf_field(): string { return '<input type="hidden" name="csrf" value="' . h(csrf()) . '">'; }
function flash(?string $message = null, string $kind = 'ok'): ?array {
    if ($message !== null) { cookie('sc_flash', $kind . '|' . $message, now() + 60, true); return null; }
    $raw = $_COOKIE['sc_flash'] ?? null;
    if (!$raw) return null;
    cookie('sc_flash', '', 1);
    [$k, $m] = array_pad(explode('|', $raw, 2), 2, '');
    return ['kind' => $k === 'error' ? 'error' : 'ok', 'message' => mb_substr($m, 0, 300)];
}
function redirect(string $to, ?string $message = null, string $kind = 'ok'): never {
    if (!str_starts_with($to, '/read') || str_starts_with($to, '//')) $to = '/read/';
    if ($message !== null) flash($message, $kind);
    header('Location: ' . $to, true, 303);
    exit;
}
function back(): string {
    $ref = $_POST['back'] ?? $_SERVER['HTTP_REFERER'] ?? '/read/';
    $path = parse_url((string)$ref, PHP_URL_PATH) ?: '/read/';
    $query = parse_url((string)$ref, PHP_URL_QUERY);
    return str_starts_with($path, '/read') ? $path . ($query ? '?' . $query : '') : '/read/';
}

function icon(string $name): string {
    static $paths = [
        'search' => '<circle cx="11" cy="11" r="6.5"/><path d="m16 16 4.5 4.5"/>',
        'heart' => '<path d="M12 20s-7.5-4.6-7.5-10.2A4.3 4.3 0 0 1 12 7.4a4.3 4.3 0 0 1 7.5 2.4C19.5 15.4 12 20 12 20Z"/>',
        'star' => '<path d="m12 3.8 2.5 5.1 5.6.8-4.1 4 1 5.6-5-2.7-5 2.7 1-5.6-4.1-4 5.6-.8Z"/>',
        'eye' => '<path d="M2.5 12S6 5.5 12 5.5 21.5 12 21.5 12 18 18.5 12 18.5 2.5 12 2.5 12Z"/><circle cx="12" cy="12" r="2.8"/>',
        'bell' => '<path d="M6 16.5V11a6 6 0 0 1 12 0v5.5l1.5 1.5h-15Z"/><path d="M10 20.5a2 2 0 0 0 4 0"/>',
        'plus' => '<path d="M12 5v14M5 12h14"/>',
        'check' => '<path d="m5 12.5 4.5 4.5L19 7.5"/>',
        'chat' => '<path d="M4.5 5.5h15v10h-9l-4.5 3.5v-3.5h-1.5Z"/>',
        'left' => '<path d="m14.5 5-7 7 7 7"/>',
        'right' => '<path d="m9.5 5 7 7-7 7"/>',
        'list' => '<path d="M8 6.5h12M8 12h12M8 17.5h12"/><circle cx="4.5" cy="6.5" r=".6"/><circle cx="4.5" cy="12" r=".6"/><circle cx="4.5" cy="17.5" r=".6"/>',
        'flag' => '<path d="M5.5 21V4.5M5.5 4.5h11l-2 4 2 4h-11"/>',
        'book' => '<path d="M4.5 5.5A2 2 0 0 1 6.5 3.5h13v15h-13a2 2 0 0 0-2 2Z"/><path d="M4.5 20.5a2 2 0 0 1 2-2h13v2Z"/>',
        'pen' => '<path d="m15 5 4 4L8.5 19.5H4.5v-4Z"/><path d="m13 7 4 4"/>',
        'user' => '<circle cx="12" cy="8.5" r="3.8"/><path d="M4.5 20.5a7.5 7.5 0 0 1 15 0"/>',
        'up' => '<path d="m5 15 7-7 7 7"/>',
        'gift' => '<path d="M4 10h16v10H4zM3 7h18v3H3zM12 7v13"/><path d="M12 7c-2-3.5-5.5-3-5.5-1S9 7 12 7c3 0 5.5 0 5.5-1s-3.5-2.5-5.5 1Z"/>',
        'calendar' => '<rect x="4" y="5.5" width="16" height="14.5" rx="2"/><path d="M4 10h16M8.5 3.5v4M15.5 3.5v4"/>',
        'lock' => '<rect x="5" y="10.5" width="14" height="10" rx="2"/><path d="M8 10.5V8a4 4 0 0 1 8 0v2.5"/>',
        'upload' => '<path d="M12 15.5V4M7 8.5l5-5 5 5M4.5 15v4.5h15V15"/>',
        'menu' => '<path d="M4 7h16M4 12h16M4 17h16"/>',
        'shield' => '<path d="M12 3.5 19 6v5.5c0 4.5-3 7.5-7 9-4-1.5-7-4.5-7-9V6Z"/>',
    ];
    return '<svg class="i" viewBox="0 0 24 24" aria-hidden="true">' . ($paths[$name] ?? '') . '</svg>';
}

function rating_badge(string $rating): string {
    return $rating === 'mature' ? '<span class="badge mature">18+</span>' : ($rating === 'teen' ? '<span class="badge teen">Teen</span>' : '');
}
function genre_label(?string $g): string { return GENRES[$g ?? ''] ?? ''; }

function cover_img(array $s, string $class = 'cover'): string {
    if ($s['cover']) return '<img class="' . $class . '" src="' . h(media($s['cover'])) . '" alt="" loading="lazy" width="480" height="640">';
    $hue = crc32($s['slug']) % 360;
    return '<span class="' . $class . ' placeholder hue-' . ($hue - $hue % 30) . '" aria-hidden="true">' . h(mb_strtoupper(mb_substr($s['title'], 0, 1))) . '</span>';
}

function series_card(array $s, array $opts = []): string {
    $meta = $opts['meta'] ?? (genre_label($s['genre']));
    $rank = isset($opts['rank']) ? '<span class="rank">' . (int)$opts['rank'] . '</span>' : '';
    $fresh = !empty($s['last_episode_at']) && now() - (int)$s['last_episode_at'] < 86400 * 2 ? '<span class="badge up">UP</span>' : '';
    return '<a class="card" href="/read/s/' . h($s['slug']) . '">' . $rank
        . '<span class="card-art">' . cover_img($s) . '<span class="card-badges">' . $fresh . rating_badge($s['rating']) . '</span></span>'
        . '<span class="card-genre">' . h($meta) . '</span>'
        . '<span class="card-title">' . h($s['title']) . '</span>'
        . '<span class="card-author">' . h($s['author'] ?? '') . '</span>'
        . '<span class="card-stats">' . icon('heart') . compact_number((int)$s['subscribers']) . '<span class="dot">·</span>' . icon('star') . stars($s) . '</span>'
        . '</a>';
}
function series_row(array $s, int $rank): string {
    return '<a class="row-card" href="/read/s/' . h($s['slug']) . '"><span class="row-rank">' . $rank . '</span>' . cover_img($s, 'row-cover')
        . '<span class="row-text"><span class="card-genre">' . h(genre_label($s['genre'])) . '</span><span class="card-title">' . h($s['title']) . rating_badge($s['rating'])
        . '</span><span class="card-author">' . h($s['author'] ?? '') . '</span></span><span class="row-stat">' . icon('eye') . compact_number((int)$s['views']) . '</span></a>';
}
function grid(array $series, array $opts = []): string {
    if (!$series) return '<div class="empty">' . icon('book') . '<p>' . h($opts['empty'] ?? 'Nothing here yet.') . '</p></div>';
    return '<div class="grid">' . implode('', array_map(fn($s) => series_card($s, $opts), $series)) . '</div>';
}

function nav_link(string $href, string $label, string $active, string $key): string {
    return '<a href="' . $href . '"' . ($active === $key ? ' aria-current="page"' : '') . '>' . $label . '</a>';
}

function layout(string $title, string $body, array $o = []): void {
    $u = user();
    $active = $o['active'] ?? '';
    $f = flash();
    $desc = $o['description'] ?? 'Read free webtoons and vertical comics made with Scroll Studio.';
    $mature = sees_mature($u) ? nav_link('/read/mature', 'Mature', $active, 'mature') : '';
    $account = $u
        ? '<a class="nav-icon" href="/read/library" aria-label="Library"' . ($active === 'library' ? ' aria-current="page"' : '') . '>' . icon('book') . '</a>'
          . '<details class="menu"><summary class="avatar-btn" aria-label="Account">' . avatar($u, 32) . '</summary><div class="menu-pop">'
          . '<div class="menu-head"><strong>' . h($u['display_name']) . '</strong><span>@' . h($u['username']) . '</span></div>'
          . '<a href="/read/u/' . h($u['username']) . '">' . icon('user') . 'Profile</a><a href="/read/create">' . icon('pen') . 'Creator dashboard</a>'
          . '<a href="/read/library">' . icon('book') . 'Library</a><a href="/read/settings">' . icon('shield') . 'Settings</a>'
          . (is_admin($u) ? '<a href="/read/admin">' . icon('flag') . 'Moderation</a>' : '')
          . '<form method="post" action="/read/logout">' . csrf_field() . '<button type="submit">Sign out</button></form></div></details>'
        : '<a class="btn ghost small" href="/read/login">Sign in</a><a class="btn primary small hide-sm" href="/read/register">Join</a>';
    $bodyClass = $o['bodyClass'] ?? '';
    header('Content-Type: text/html; charset=utf-8');
    echo '<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1,viewport-fit=cover">'
        . '<meta name="theme-color" content="#18191c"><title>' . h($title) . ' · Scroll Comics</title><meta name="description" content="' . h($desc) . '">'
        . '<meta property="og:title" content="' . h($title) . '"><meta property="og:description" content="' . h($desc) . '">'
        . (!empty($o['image']) ? '<meta property="og:image" content="' . h($o['image']) . '">' : '')
        . (!empty($o['noindex']) ? '<meta name="robots" content="noindex">' : '')
        . '<link rel="icon" href="/assets/app-icon.png"><link rel="stylesheet" href="/read/assets/read.css?v=1"><script src="/read/assets/read.js?v=1" defer></script></head>'
        . '<body class="' . h($bodyClass) . '">';
    if (empty($o['bare'])) {
        echo '<a class="skip" href="#main">Skip to content</a><header class="top"><div class="top-in">'
            . '<a class="logo" href="/read/"><img src="/assets/app-icon.png" alt="" width="26" height="26"><span>Scroll <b>Comics</b></span></a>'
            . '<nav class="main-nav" aria-label="Sections">' . nav_link('/read/', 'Home', $active, 'home') . nav_link('/read/daily', 'Daily', $active, 'daily')
            . nav_link('/read/genres', 'Genres', $active, 'genres') . nav_link('/read/ranking', 'Ranking', $active, 'ranking') . $mature
            . nav_link('/read/create', 'Publish', $active, 'create') . '</nav>'
            . '<form class="search" action="/read/search" role="search"><label class="sr" for="q">Search</label>' . icon('search')
            . '<input id="q" name="q" type="search" placeholder="Search titles, creators" value="' . h($o['query'] ?? '') . '" maxlength="80"></form>'
            . '<div class="account">' . $account . '</div></div></header>';
    }
    if ($f) echo '<div class="flash ' . $f['kind'] . '" role="status">' . h($f['message']) . '</div>';
    echo '<main id="main">' . $body . '</main>';
    if (empty($o['bare'])) {
        $tab = fn($href, $icon, $label, $key) => '<a href="' . $href . '"' . ($active === $key ? ' aria-current="page"' : '') . '>' . icon($icon) . '<span>' . $label . '</span></a>';
        echo '<nav class="tabbar" aria-label="Sections">' . $tab('/read/', 'book', 'Home', 'home') . $tab('/read/daily', 'calendar', 'Daily', 'daily')
            . $tab('/read/ranking', 'star', 'Ranking', 'ranking') . $tab($u ? '/read/library' : '/read/login', 'heart', 'Library', 'library') . $tab('/read/create', 'pen', 'Publish', 'create') . '</nav>';
    }
    if (empty($o['bare'])) {
        echo '<footer class="foot"><div class="foot-in"><a class="logo" href="/read/"><img src="/assets/app-icon.png" alt="" width="22" height="22"><span>Scroll <b>Comics</b></span></a>'
            . '<nav aria-label="Footer"><a href="/">Get Scroll Studio</a><a href="/studio/">Assets editor</a><a href="/read/create">Publish your comic</a><a href="/read/rules">Community rules</a><a href="/privacy.html">Privacy</a></nav>'
            . '<p>Made with Scroll Studio by 456C.</p></div></footer>';
    }
    echo '</body></html>';
}

function avatar(array $u, int $size = 36): string {
    if (!empty($u['avatar'])) return '<img class="avatar" src="' . h(media($u['avatar'])) . '" alt="" width="' . $size . '" height="' . $size . '">';
    $hue = crc32((string)$u['username']) % 360;
    return '<span class="avatar letter hue-' . ($hue - $hue % 30) . ' s' . $size . '" aria-hidden="true">' . h(mb_strtoupper(mb_substr((string)($u['display_name'] ?? $u['author'] ?? $u['username']), 0, 1))) . '</span>';
}

function field(string $label, string $control, string $hint = ''): string {
    return '<label class="field"><span class="field-label">' . $label . '</span>' . $control . ($hint ? '<span class="hint">' . $hint . '</span>' : '') . '</label>';
}
function select(string $name, array $options, ?string $current, bool $blank = false): string {
    $html = '<select name="' . $name . '">' . ($blank ? '<option value="">None</option>' : '');
    foreach ($options as $k => $v) $html .= '<option value="' . h((string)$k) . '"' . ((string)$k === (string)$current ? ' selected' : '') . '>' . h($v) . '</option>';
    return $html . '</select>';
}
function error_page(int $status, string $message): void {
    http_response_code($status);
    layout($status === 404 ? 'Not found' : 'Something went wrong', '<section class="narrow center-box"><div class="empty">' . icon('book')
        . '<h1>' . ($status === 404 ? 'Page not found' : 'Something went wrong') . '</h1><p>' . h($message) . '</p><a class="btn primary" href="/read/">Back to comics</a></div></section>', ['noindex' => true]);
}
