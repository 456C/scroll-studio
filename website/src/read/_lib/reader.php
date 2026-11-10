<?php
declare(strict_types=1);
// Reader-facing pages: discovery, series, episodes, profiles, library.
namespace Comics;

function series_query(?array $viewer, string $where = '1', string $order = 's.last_episode_at DESC', int $limit = 24, array $args = [], int $offset = 0): array {
    return all('SELECT s.*, u.display_name AS author, u.username FROM series s JOIN users u ON u.id = s.user_id
        WHERE u.banned = 0 AND ' . visible_series_sql($viewer) . " AND ($where) ORDER BY $order LIMIT $limit OFFSET $offset", $args);
}
const TRENDING = '(SELECT COUNT(*) FROM view_log v JOIN episodes ve ON ve.id = v.episode_id WHERE ve.series_id = s.id) * 3 + s.subscribers DESC, s.views DESC';

function today_key(): string { return strtolower(date('D')); }

function day_tabs(string $current, string $base): string {
    $html = '<div class="chips" role="tablist" aria-label="Update day">';
    foreach (DAYS + ['completed' => 'Completed'] as $k => $v)
        $html .= '<a role="tab" class="chip' . ($k === today_key() ? ' today' : '') . '" href="' . $base . $k . '" aria-selected="' . ($k === $current ? 'true' : 'false') . '">' . $v . '</a>';
    return $html . '</div>';
}
function day_series(?array $viewer, string $day, int $limit = 60): array {
    if ($day === 'completed') return series_query($viewer, "s.status = 'completed'", 's.subscribers DESC', $limit);
    return series_query($viewer, "(',' || s.update_days || ',') LIKE ? AND s.status = 'ongoing'", 's.subscribers DESC', $limit, ['%,' . $day . ',%']);
}

function page_home(?array $viewer): void {
    $featured = series_query($viewer, 's.featured = 1 AND s.banner IS NOT NULL', 's.last_episode_at DESC', 5);
    if (!$featured) $featured = series_query($viewer, 's.banner IS NOT NULL', TRENDING, 3);
    $trending = series_query($viewer, '1', TRENDING, 10);
    $new = series_query($viewer, '1', 's.created_at DESC', 12);
    $updated = series_query($viewer, '1', 's.last_episode_at DESC', 12);
    $day = today_key();
    $today = day_series($viewer, $day, 12);
    $continue = $viewer ? all('SELECT s.slug, s.title, s.cover, s.rating, e.number, e.title AS ep_title FROM history h JOIN episodes e ON e.id = h.episode_id
        JOIN series s ON s.id = h.series_id WHERE h.user_id = ? AND h.read_at = (SELECT MAX(read_at) FROM history h2 WHERE h2.user_id = h.user_id AND h2.series_id = h.series_id)
        AND ' . visible_series_sql($viewer) . ' ORDER BY h.read_at DESC LIMIT 6', [$viewer['id']]) : [];
    $b = '';
    if ($featured) {
        $b .= '<section class="hero-carousel" aria-roledescription="carousel" aria-label="Featured"><div class="slides">';
        foreach ($featured as $i => $s) {
            $b .= '<a class="slide" href="/read/s/' . h($s['slug']) . '"' . ($i ? ' aria-hidden="true" tabindex="-1"' : '') . '><img src="' . h(media($s['banner'])) . '" alt="" width="1600" height="600">'
                . '<span class="slide-copy"><span class="card-genre">' . h(genre_label($s['genre'])) . rating_badge($s['rating']) . '</span><strong>' . h($s['title']) . '</strong>'
                . '<span class="slide-tag">' . h($s['tagline'] ?: mb_strimwidth($s['synopsis'], 0, 110, '…')) . '</span><span class="btn primary small">Read now</span></span></a>';
        }
        $b .= '</div>' . (count($featured) > 1 ? '<div class="dots">' . implode('', array_map(fn($i) => '<button type="button" aria-label="Show slide ' . ($i + 1) . '"' . ($i === 0 ? ' aria-current="true"' : '') . '></button>', array_keys($featured))) . '</div>' : '') . '</section>';
    } else {
        $b .= '<section class="welcome wrap"><div><h1>Stories that scroll.</h1><p>Read vertical comics from independent creators, or publish your own straight from Scroll Studio.</p>'
            . '<div class="actions"><a class="btn primary" href="/read/create">' . icon('upload') . 'Publish a series</a><a class="btn" href="/">Get Scroll Studio</a></div></div></section>';
    }
    $b .= '<div class="wrap stack">';
    if ($continue) {
        $b .= '<section><div class="sec-head"><h2>Continue reading</h2><a href="/read/library?tab=history">History</a></div><div class="shelf">';
        foreach ($continue as $c) $b .= '<a class="continue" href="/read/s/' . h($c['slug']) . '/' . (int)$c['number'] . '">' . cover_img($c, 'mini-cover') . '<span><strong>' . h($c['title']) . '</strong><span>Ep. ' . (int)$c['number'] . ' · ' . h($c['ep_title']) . '</span></span></a>';
        $b .= '</div></section>';
    }
    $b .= '<section><div class="sec-head"><h2>Updated ' . h(date('l')) . 's</h2><a href="/read/daily/' . $day . '">All daily</a></div>' . grid($today, ['empty' => 'No series update on ' . date('l') . ' yet.']) . '</section>';
    if ($trending) {
        $b .= '<section class="split"><div><div class="sec-head"><h2>Trending now</h2><a href="/read/ranking">Ranking</a></div><ol class="rows">';
        foreach (array_slice($trending, 0, 5) as $i => $s) $b .= '<li>' . series_row($s, $i + 1) . '</li>';
        $b .= '</ol></div><div><div class="sec-head"><h2>Popular</h2></div><ol class="rows">';
        foreach (array_slice($trending, 5, 5) as $i => $s) $b .= '<li>' . series_row($s, $i + 6) . '</li>';
        $b .= '</ol></div></section>';
    }
    $b .= '<section><div class="sec-head"><h2>Fresh episodes</h2></div>' . grid($updated, ['empty' => 'Published episodes will appear here.']) . '</section>';
    $b .= '<section><div class="sec-head"><h2>New series</h2></div>' . grid($new) . '</section>';
    $b .= '<section><div class="sec-head"><h2>Browse genres</h2></div><div class="genre-cloud">';
    foreach (GENRES as $k => $v) $b .= '<a class="genre-pill" href="/read/genres/' . $k . '">' . h($v) . '</a>';
    $b .= '</div></section>';
    $b .= '<section class="cta"><div><h2>Make the next one.</h2><p>Export from Scroll Studio and publish in a few clicks. Your originals stay full quality.</p></div><a class="btn primary" href="/read/create">' . icon('upload') . 'Start publishing</a></section>';
    $b .= '</div>';
    layout('Read webtoons', $b, ['active' => 'home']);
}

function page_daily(?array $viewer, string $day): void {
    if (!isset(DAYS[$day]) && $day !== 'completed') $day = today_key();
    $list = day_series($viewer, $day);
    $title = $day === 'completed' ? 'Completed series' : 'Updates every ' . date('l', strtotime('next ' . $day));
    layout('Daily', '<div class="wrap stack"><section><div class="page-head"><h1>Daily schedule</h1></div>' . day_tabs($day, '/read/daily/')
        . '<h2 class="sub">' . h($title) . '</h2>' . grid($list, ['empty' => 'No series on this day yet.']) . '</section></div>', ['active' => 'daily']);
}

function sort_links(string $base, string $current): string {
    $html = '<div class="seg" role="group" aria-label="Sort">';
    foreach (['popular' => 'Popular', 'updated' => 'Updated', 'new' => 'New', 'rating' => 'Top rated'] as $k => $v)
        $html .= '<a href="' . $base . (str_contains($base, '?') ? '&' : '?') . 'sort=' . $k . '"' . ($k === $current ? ' aria-current="true"' : '') . '>' . $v . '</a>';
    return $html . '</div>';
}
function order_for(string $sort): string {
    return match ($sort) {
        'updated' => 's.last_episode_at DESC', 'new' => 's.created_at DESC',
        'rating' => '(CASE WHEN s.rating_count = 0 THEN 0 ELSE (s.rating_sum + 3.5 * 5) * 1.0 / (s.rating_count + 5) END) DESC',
        default => 's.subscribers DESC, s.views DESC',
    };
}

function page_genres(?array $viewer, string $genre): void {
    $sort = (string)($_GET['sort'] ?? 'popular');
    $b = '<div class="wrap stack"><section><div class="page-head"><h1>' . ($genre && isset(GENRES[$genre]) ? h(GENRES[$genre]) : 'Genres') . '</h1>' . sort_links('/read/genres/' . h($genre), $sort) . '</div><div class="chips scroll-x">'
        . '<a class="chip" href="/read/genres" aria-selected="' . ($genre === '' ? 'true' : 'false') . '">All</a>';
    foreach (GENRES as $k => $v) $b .= '<a class="chip" href="/read/genres/' . $k . '" aria-selected="' . ($k === $genre ? 'true' : 'false') . '">' . h($v) . '</a>';
    $b .= '</div>';
    $list = isset(GENRES[$genre]) ? series_query($viewer, 's.genre = ? OR s.genre2 = ?', order_for($sort), 60, [$genre, $genre]) : series_query($viewer, '1', order_for($sort), 60);
    $b .= grid($list, ['empty' => 'No series in this genre yet.']) . '</section></div>';
    layout($genre ? (GENRES[$genre] ?? 'Genres') : 'Genres', $b, ['active' => 'genres']);
}

function page_ranking(?array $viewer): void {
    $tab = (string)($_GET['by'] ?? 'trending');
    $order = match ($tab) { 'subscribers' => 's.subscribers DESC', 'rating' => order_for('rating'), 'views' => 's.views DESC', default => TRENDING };
    $list = series_query($viewer, '1', $order, 50);
    $b = '<div class="wrap stack narrow-wide"><section><div class="page-head"><h1>Ranking</h1><div class="seg" role="group" aria-label="Rank by">';
    foreach (['trending' => 'Trending', 'subscribers' => 'Most subscribed', 'rating' => 'Top rated', 'views' => 'Most read'] as $k => $v)
        $b .= '<a href="/read/ranking?by=' . $k . '"' . ($k === $tab ? ' aria-current="true"' : '') . '>' . $v . '</a>';
    $b .= '</div></div>';
    if (!$list) $b .= grid([]);
    else { $b .= '<ol class="rows big">'; foreach ($list as $i => $s) $b .= '<li>' . series_row($s, $i + 1) . '</li>'; $b .= '</ol>'; }
    layout('Ranking', $b . '</section></div>', ['active' => 'ranking']);
}

function page_mature(?array $viewer): void {
    if (!sees_mature($viewer)) { mature_gate($viewer, '/read/mature'); return; }
    $sort = (string)($_GET['sort'] ?? 'popular');
    $list = series_query($viewer, "s.rating = 'mature'", order_for($sort), 60);
    layout('Mature', '<div class="wrap stack"><section><div class="page-head"><h1>Mature <span class="badge mature">18+</span></h1>' . sort_links('/read/mature', $sort) . '</div>'
        . grid($list, ['empty' => 'No mature series yet.']) . '</section></div>', ['active' => 'mature', 'noindex' => true]);
}

function mature_gate(?array $viewer, string $next): void {
    $access = $viewer === null ? 'login' : (!is_adult($viewer) ? 'underage' : 'confirm');
    $b = '<section class="narrow center-box"><div class="gate">' . icon('lock') . '<h1>Mature content</h1>';
    if ($access === 'login') $b .= '<p>This series is for readers 18 and older. Sign in to confirm your age.</p><div class="actions"><a class="btn primary" href="/read/login?next=' . h(rawurlencode($next)) . '">Sign in</a><a class="btn" href="/read/register?next=' . h(rawurlencode($next)) . '">Create account</a></div>';
    elseif ($access === 'underage') $b .= '<p>This series is only available to readers 18 and older.</p><a class="btn" href="/read/">Back to comics</a>';
    else $b .= '<p>It may contain explicit sexual content, graphic violence or other adult themes. You can turn this off any time in Settings.</p>'
        . '<form method="post" action="/read/action/mature">' . csrf_field() . '<input type="hidden" name="on" value="1"><input type="hidden" name="back" value="' . h($next) . '"><div class="actions">'
        . '<button class="btn primary" type="submit">I\'m 18+, show mature content</button><a class="btn" href="/read/">Not now</a></div></form>';
    layout('Mature content', $b . '</div></section>', ['noindex' => true]);
}

function page_search(?array $viewer): void {
    $q = trim(mb_substr((string)($_GET['q'] ?? ''), 0, 80));
    $b = '<div class="wrap stack"><section><div class="page-head"><h1>' . ($q === '' ? 'Search' : 'Results for “' . h($q) . '”') . '</h1></div>';
    if ($q !== '') {
        $like = '%' . str_replace(['\\', '%', '_'], ['\\\\', '\\%', '\\_'], $q) . '%';
        $list = series_query($viewer, "s.title LIKE :q ESCAPE '\\' OR s.tagline LIKE :q ESCAPE '\\' OR u.display_name LIKE :q ESCAPE '\\' OR u.username LIKE :q ESCAPE '\\'", 's.subscribers DESC', 60, ['q' => $like]);
        $creators = all("SELECT * FROM users WHERE banned = 0 AND (display_name LIKE :q ESCAPE '\\' OR username LIKE :q ESCAPE '\\') AND EXISTS(SELECT 1 FROM series s WHERE s.user_id = users.id AND " . visible_series_sql($viewer) . ') LIMIT 8', ['q' => $like]);
        if ($creators) {
            $b .= '<div class="creators">';
            foreach ($creators as $c) $b .= '<a class="creator-chip" href="/read/u/' . h($c['username']) . '">' . avatar($c, 28) . '<span>' . h($c['display_name']) . '</span></a>';
            $b .= '</div>';
        }
        $b .= grid($list, ['empty' => 'No series match that search.']);
    } else {
        $b .= '<div class="genre-cloud">'; foreach (GENRES as $k => $v) $b .= '<a class="genre-pill" href="/read/genres/' . $k . '">' . h($v) . '</a>'; $b .= '</div>';
    }
    layout($q === '' ? 'Search' : $q, $b . '</section></div>', ['query' => $q, 'noindex' => true]);
}

function load_series(string $slug): ?array {
    return one('SELECT s.*, u.display_name AS author, u.username, u.avatar, u.banned, u.support_url AS author_support FROM series s JOIN users u ON u.id = s.user_id WHERE s.slug = ?', [$slug]);
}

function page_series(?array $viewer, string $slug): void {
    $s = load_series($slug);
    if (!$s || ((int)$s['banned'] === 1 && !is_admin($viewer))) { error_page(404, 'That series does not exist or was removed.'); return; }
    $access = series_access($viewer, $s);
    if ($access === 'hidden') { error_page(404, 'That series is not available.'); return; }
    if ($access !== 'ok') { mature_gate($viewer, '/read/s/' . $slug); return; }
    $owner = owns($viewer, $s) || is_admin($viewer);
    $order = ($_GET['order'] ?? '') === 'first' ? 'ASC' : 'DESC';
    $episodes = all('SELECT e.*, (SELECT 1 FROM history h WHERE h.user_id = ? AND h.episode_id = e.id) AS seen FROM episodes e WHERE e.series_id = ? AND ' . ($owner ? '1' : live_episode_sql('e')) . " ORDER BY e.number $order",
        [$viewer['id'] ?? 0, $s['id']]);
    $live = array_values(array_filter($episodes, 'Comics\episode_live'));
    $first = $live ? min(array_map(fn($e) => (int)$e['number'], $live)) : null;
    $resume = $viewer ? one('SELECT e.number FROM history h JOIN episodes e ON e.id = h.episode_id WHERE h.user_id = ? AND h.series_id = ? AND ' . live_episode_sql('e') . ' ORDER BY h.read_at DESC LIMIT 1', [$viewer['id'], $s['id']]) : null;
    $subscribed = $viewer && value('SELECT 1 FROM subscriptions WHERE user_id = ? AND series_id = ?', [$viewer['id'], $s['id']]);
    $myRating = $viewer ? (int)value('SELECT score FROM ratings WHERE user_id = ? AND series_id = ?', [$viewer['id'], $s['id']]) : 0;
    $days = array_filter(explode(',', $s['update_days']));
    $schedule = $s['status'] === 'ongoing' && $days ? 'Every ' . implode(', ', array_map(fn($d) => DAYS[$d] ?? '', $days)) : STATUSES[$s['status']];
    $support = $s['support_url'] ?: $s['author_support'];

    $b = '<article class="series"><header class="series-hero">' . ($s['banner'] ? '<img class="series-banner" src="' . h(media($s['banner'])) . '" alt="" width="1600" height="600">' : '<div class="series-banner blank"></div>')
        . '<div class="wrap series-head">' . cover_img($s, 'series-cover') . '<div class="series-info">'
        . '<p class="card-genre">' . h(genre_label($s['genre'])) . ($s['genre2'] ? ' · ' . h(genre_label($s['genre2'])) : '') . ' ' . rating_badge($s['rating']) . '</p>'
        . '<h1>' . h($s['title']) . '</h1><a class="byline" href="/read/u/' . h($s['username']) . '">' . avatar($s, 24) . h($s['author']) . '</a>'
        . '<ul class="stat-line"><li>' . icon('eye') . '<b>' . compact_number((int)$s['views']) . '</b> views</li><li>' . icon('bell') . '<b>' . compact_number((int)$s['subscribers']) . '</b> subscribers</li>'
        . '<li>' . icon('star') . '<b>' . stars($s) . '</b> (' . compact_number((int)$s['rating_count']) . ')</li><li>' . icon('calendar') . h($schedule) . '</li></ul>'
        . '<div class="actions">';
    if ($first !== null) $b .= '<a class="btn primary" href="/read/s/' . h($slug) . '/' . ($resume ? (int)$resume['number'] : $first) . '">' . icon('book') . ($resume ? 'Continue ep. ' . (int)$resume['number'] : 'Read first episode') . '</a>';
    $b .= '<form method="post" action="/read/action/subscribe" class="js-toggle" data-on="Subscribed" data-off="Subscribe">' . csrf_field() . '<input type="hidden" name="series" value="' . (int)$s['id'] . '">'
        . '<button class="btn' . ($subscribed ? ' on' : '') . '" type="submit" aria-pressed="' . ($subscribed ? 'true' : 'false') . '">' . icon($subscribed ? 'check' : 'plus') . '<span>' . ($subscribed ? 'Subscribed' : 'Subscribe') . '</span></button></form>';
    if ($support) $b .= '<a class="btn" href="' . h($support) . '" rel="noopener nofollow ugc" target="_blank">' . icon('gift') . 'Support</a>';
    if ($owner) $b .= '<a class="btn ghost" href="/read/create/' . h($slug) . '">' . icon('pen') . 'Edit</a>';
    $b .= '</div></div></div></header><div class="wrap series-body"><div class="episodes"><div class="sec-head"><h2>' . count($live) . ' episode' . (count($live) === 1 ? '' : 's') . '</h2>'
        . '<div class="seg small"><a href="?order=latest"' . ($order === 'DESC' ? ' aria-current="true"' : '') . '>Latest</a><a href="?order=first"' . ($order === 'ASC' ? ' aria-current="true"' : '') . '>First</a></div></div><ul class="ep-list">';
    foreach ($episodes as $e) {
        $isLive = episode_live($e);
        $when = $isLive ? date('M j, Y', (int)($e['publish_at'] ?: $e['created_at'])) : ($e['status'] === 'scheduled' ? 'Scheduled ' . date('M j, H:i', (int)$e['publish_at']) : 'Draft');
        $b .= '<li><a class="ep' . ($e['seen'] ? ' seen' : '') . ($isLive ? '' : ' unpublished') . '" href="/read/s/' . h($slug) . '/' . (int)$e['number'] . '">'
            . ($e['thumb'] ? '<img src="' . h(media($e['thumb'])) . '" alt="" width="160" height="160" loading="lazy">' : '<span class="ep-thumb-blank">' . (int)$e['number'] . '</span>')
            . '<span class="ep-text"><span class="ep-title">' . h($e['title']) . '</span><span class="ep-meta">' . h($when) . '</span></span>'
            . '<span class="ep-stats"><span>' . icon('heart') . compact_number((int)$e['likes']) . '</span><span class="ep-num">#' . (int)$e['number'] . '</span></span></a></li>';
    }
    if (!$episodes) $b .= '<li class="empty small">No episodes yet.</li>';
    $b .= '</ul></div><aside class="series-side"><section class="panel"><h2>About</h2>' . ($s['tagline'] ? '<p class="tagline">' . h($s['tagline']) . '</p>' : '') . '<div class="prose">' . nl2br(h($s['synopsis'])) . '</div></section>'
        . '<section class="panel"><h2>Your rating</h2><form method="post" action="/read/action/rate" class="rate">' . csrf_field() . '<input type="hidden" name="series" value="' . (int)$s['id'] . '"><div class="star-input" role="radiogroup" aria-label="Rate this series">';
    for ($i = 5; $i >= 1; $i--) $b .= '<button type="submit" name="score" value="' . $i . '" role="radio" aria-checked="' . ($myRating === $i ? 'true' : 'false') . '" aria-label="' . $i . ' star' . ($i > 1 ? 's' : '') . '"' . ($i <= $myRating ? ' class="lit"' : '') . '>' . icon('star') . '</button>';
    $b .= '</div></form></section>';
    $more = all('SELECT s.*, u.display_name AS author FROM series s JOIN users u ON u.id = s.user_id WHERE s.user_id = ? AND s.id <> ? AND ' . visible_series_sql($viewer) . ' LIMIT 3', [$s['user_id'], $s['id']]);
    if ($more) { $b .= '<section class="panel"><h2>More from ' . h($s['author']) . '</h2><div class="mini-list">'; foreach ($more as $m) $b .= '<a href="/read/s/' . h($m['slug']) . '">' . cover_img($m, 'mini-cover') . '<span>' . h($m['title']) . '</span></a>'; $b .= '</div></section>'; }
    $b .= '<section class="panel quiet">' . report_form('series', (int)$s['id']) . '</section></aside></div></article>';
    layout($s['title'], $b, ['description' => $s['tagline'] ?: mb_strimwidth($s['synopsis'], 0, 160, '…'), 'image' => $s['cover'] ? media($s['cover']) : '', 'noindex' => $s['rating'] === 'mature']);
}

function report_form(string $type, int $id): string {
    $opts = '';
    foreach (REPORT_REASONS as $k => $v) $opts .= '<option value="' . $k . '">' . h($v) . '</option>';
    return '<details class="report"><summary>' . icon('flag') . 'Report</summary><form method="post" action="/read/action/report">' . csrf_field()
        . '<input type="hidden" name="type" value="' . $type . '"><input type="hidden" name="id" value="' . $id . '"><select name="reason" aria-label="Reason">' . $opts . '</select>'
        . '<textarea name="details" rows="2" maxlength="1000" placeholder="Details (optional)" aria-label="Details"></textarea><button class="btn small" type="submit">Send report</button></form></details>';
}

function page_episode(?array $viewer, string $slug, int $number): void {
    $s = load_series($slug);
    if (!$s || ((int)$s['banned'] === 1 && !is_admin($viewer))) { error_page(404, 'That series does not exist.'); return; }
    $access = series_access($viewer, $s);
    if ($access === 'hidden') { error_page(404, 'That series is not available.'); return; }
    if ($access !== 'ok') { mature_gate($viewer, "/read/s/$slug/$number"); return; }
    $owner = owns($viewer, $s) || is_admin($viewer);
    $e = one('SELECT * FROM episodes WHERE series_id = ? AND number = ?', [$s['id'], $number]);
    if (!$e || (!episode_live($e) && !$owner)) { error_page(404, 'That episode is not available.'); return; }
    $images = all('SELECT * FROM episode_images WHERE episode_id = ? ORDER BY position', [$e['id']]);
    $vis = $owner ? '1' : live_episode_sql('e');
    $prev = one("SELECT number, title FROM episodes e WHERE series_id = ? AND number < ? AND $vis ORDER BY number DESC LIMIT 1", [$s['id'], $number]);
    $next = one("SELECT number, title, thumb FROM episodes e WHERE series_id = ? AND number > ? AND $vis ORDER BY number ASC LIMIT 1", [$s['id'], $number]);
    $list = all("SELECT number, title, thumb FROM episodes e WHERE series_id = ? AND $vis ORDER BY number DESC", [$s['id']]);
    if (episode_live($e)) record_view($e, $viewer);
    $liked = $viewer && value('SELECT 1 FROM likes WHERE user_id = ? AND episode_id = ?', [$viewer['id'], $e['id']]);
    $subscribed = $viewer && value('SELECT 1 FROM subscriptions WHERE user_id = ? AND series_id = ?', [$viewer['id'], $s['id']]);
    $base = '/read/s/' . h($slug);
    $support = $s['support_url'] ?: $s['author_support'];

    $b = '<header class="reader-bar" data-autohide><a class="icon-btn" href="' . $base . '" aria-label="Back to series">' . icon('left') . '</a>'
        . '<div class="reader-title"><a href="' . $base . '">' . h($s['title']) . '</a><span>#' . (int)$e['number'] . ' ' . h($e['title']) . '</span></div>'
        . '<button class="icon-btn" type="button" data-drawer="episodes" aria-label="Episode list" aria-expanded="false">' . icon('list') . '</button></header>'
        . '<div class="progress" aria-hidden="true"><span></span></div>';
    if (!episode_live($e)) $b .= '<p class="preview-note">' . icon('lock') . 'Preview: only you can see this ' . h($e['status']) . ' episode.</p>';
    $b .= '<div class="strip">';
    foreach ($images as $i => $img) $b .= '<img src="' . h(media($img['file'])) . '" width="' . (int)$img['width'] . '" height="' . (int)$img['height'] . '" alt="' . ($i === 0 ? h($e['title']) : '') . '"' . ($i > 1 ? ' loading="lazy"' : '') . ' decoding="async">';
    if (!$images) $b .= '<div class="empty">' . icon('book') . '<p>This episode has no pages yet.</p></div>';
    $b .= '</div><section class="after"><div class="after-in">';
    $b .= '<div class="after-actions"><form method="post" action="/read/action/like" class="js-like">' . csrf_field() . '<input type="hidden" name="episode" value="' . (int)$e['id'] . '">'
        . '<button class="like' . ($liked ? ' on' : '') . '" type="submit" aria-pressed="' . ($liked ? 'true' : 'false') . '">' . icon('heart') . '<b>' . compact_number((int)$e['likes']) . '</b></button></form>'
        . '<form method="post" action="/read/action/subscribe" class="js-toggle" data-on="Subscribed" data-off="Subscribe">' . csrf_field() . '<input type="hidden" name="series" value="' . (int)$s['id'] . '">'
        . '<button class="btn' . ($subscribed ? ' on' : '') . '" type="submit" aria-pressed="' . ($subscribed ? 'true' : 'false') . '">' . icon($subscribed ? 'check' : 'plus') . '<span>' . ($subscribed ? 'Subscribed' : 'Subscribe') . '</span></button></form>'
        . ($support ? '<a class="btn" href="' . h($support) . '" rel="noopener nofollow ugc" target="_blank">' . icon('gift') . 'Support creator</a>' : '') . '</div>';
    if ($e['note'] !== '') $b .= '<div class="creator-note"><div class="note-head">' . avatar($s, 28) . '<strong>' . h($s['author']) . '</strong><span>Creator note</span></div><p>' . nl2br(h($e['note'])) . '</p></div>';
    $b .= '<nav class="ep-nav" aria-label="Episodes">' . ($prev ? '<a class="btn" href="' . $base . '/' . (int)$prev['number'] . '">' . icon('left') . 'Previous</a>' : '<span></span>')
        . ($next ? '<a class="next-card" href="' . $base . '/' . (int)$next['number'] . '">' . ($next['thumb'] ? '<img src="' . h(media($next['thumb'])) . '" alt="" width="64" height="64">' : '')
            . '<span><small>Next episode</small><strong>#' . (int)$next['number'] . ' ' . h($next['title']) . '</strong></span>' . icon('right') . '</a>'
            : '<span class="caught-up">' . icon('check') . 'You\'re caught up</span>') . '</nav>';
    $b .= comments_section($viewer, $e, $s) . '</div></section>';
    $b .= '<div class="drawer" id="drawer-episodes" hidden><div class="drawer-panel" role="dialog" aria-label="Episodes"><div class="drawer-head"><strong>' . h($s['title']) . '</strong><button class="icon-btn" type="button" data-close aria-label="Close">×</button></div><ul class="drawer-list">';
    foreach ($list as $l) $b .= '<li><a href="' . $base . '/' . (int)$l['number'] . '"' . ((int)$l['number'] === $number ? ' aria-current="page"' : '') . '>'
        . ($l['thumb'] ? '<img src="' . h(media($l['thumb'])) . '" alt="" width="48" height="48" loading="lazy">' : '<span class="ep-thumb-blank sm">' . (int)$l['number'] . '</span>') . '<span>#' . (int)$l['number'] . ' ' . h($l['title']) . '</span></a></li>';
    $b .= '</ul></div></div>';
    $b .= '<nav class="reader-dock" data-autohide aria-label="Reader">' . ($prev ? '<a class="icon-btn" href="' . $base . '/' . (int)$prev['number'] . '" aria-label="Previous episode">' . icon('left') . '</a>' : '<span class="icon-btn off">' . icon('left') . '</span>')
        . '<a class="icon-btn" href="#comments" aria-label="Comments">' . icon('chat') . '<small>' . compact_number((int)$e['comments']) . '</small></a>'
        . '<button class="icon-btn" type="button" data-top aria-label="Back to top">' . icon('up') . '</button>'
        . ($next ? '<a class="icon-btn" href="' . $base . '/' . (int)$next['number'] . '" aria-label="Next episode" rel="next">' . icon('right') . '</a>' : '<span class="icon-btn off">' . icon('right') . '</span>') . '</nav>';
    layout('#' . (int)$e['number'] . ' ' . $e['title'] . ' · ' . $s['title'], $b, ['bare' => true, 'bodyClass' => 'reading', 'noindex' => $s['rating'] === 'mature' || !episode_live($e), 'image' => $e['thumb'] ? media($e['thumb']) : '']);
}

function comments_section(?array $viewer, array $e, array $s): string {
    $sort = ($_GET['comments'] ?? '') === 'new' ? 'c.created_at DESC' : 'c.likes DESC, c.created_at DESC';
    $rows = all('SELECT c.*, u.username, u.display_name, u.avatar, (SELECT 1 FROM comment_likes l WHERE l.comment_id = c.id AND l.user_id = ?) AS liked FROM comments c JOIN users u ON u.id = c.user_id
        WHERE c.episode_id = ? AND c.hidden = 0 AND u.banned = 0 ORDER BY ' . $sort . ' LIMIT 300', [$viewer['id'] ?? 0, $e['id']]);
    $top = array_filter($rows, fn($c) => $c['parent_id'] === null);
    $replies = [];
    foreach (array_reverse($rows) as $c) if ($c['parent_id'] !== null) $replies[$c['parent_id']][] = $c;
    $b = '<section class="comments" id="comments"><div class="sec-head"><h2>Comments <span class="count">' . compact_number((int)$e['comments']) . '</span></h2><div class="seg small">'
        . '<a href="?comments=top#comments"' . ($sort[2] === 'l' ? ' aria-current="true"' : '') . '>Top</a><a href="?comments=new#comments"' . ($sort[2] === 'c' ? ' aria-current="true"' : '') . '>New</a></div></div>';
    if ($viewer) $b .= comment_form($e, null, $viewer);
    else $b .= '<p class="signin-prompt"><a href="/read/login?next=' . h(rawurlencode($_SERVER['REQUEST_URI'] ?? '')) . '">Sign in</a> to comment, like and subscribe.</p>';
    $b .= '<ul class="comment-list">';
    foreach ($top as $c) {
        $b .= comment_item($c, $viewer, $s, $e);
        if (!empty($replies[$c['id']])) { $b .= '<li class="replies"><ul>'; foreach ($replies[$c['id']] as $r) $b .= comment_item($r, $viewer, $s, $e, true); $b .= '</ul></li>'; }
    }
    if (!$top) $b .= '<li class="empty small">Be the first to comment.</li>';
    return $b . '</ul></section>';
}
function comment_form(array $e, ?int $parent, array $viewer): string {
    return '<form class="comment-form" method="post" action="/read/action/comment">' . csrf_field() . '<input type="hidden" name="episode" value="' . (int)$e['id'] . '">'
        . ($parent ? '<input type="hidden" name="parent" value="' . $parent . '">' : '') . avatar($viewer, 32)
        . '<div class="comment-box"><textarea name="body" rows="2" maxlength="2000" required placeholder="' . ($parent ? 'Write a reply' : 'Add a comment') . '" aria-label="Comment"></textarea>'
        . '<div class="comment-tools"><label class="check"><input type="checkbox" name="spoiler" value="1"> Spoiler</label><button class="btn primary small" type="submit">' . ($parent ? 'Reply' : 'Post') . '</button></div></div></form>';
}
function comment_item(array $c, ?array $viewer, array $s, array $e, bool $reply = false): string {
    $mine = $viewer && (int)$viewer['id'] === (int)$c['user_id'];
    $creator = (int)$c['user_id'] === (int)$s['user_id'];
    $body = nl2br(h($c['body']));
    if ($c['spoiler']) $body = '<details class="spoiler"><summary>Spoiler, tap to reveal</summary>' . $body . '</details>';
    $b = '<li class="comment" id="c' . (int)$c['id'] . '"><a href="/read/u/' . h($c['username']) . '">' . avatar($c, 32) . '</a><div class="comment-main"><div class="comment-head"><a href="/read/u/' . h($c['username']) . '"><strong>' . h($c['display_name']) . '</strong></a>'
        . ($creator ? '<span class="badge creator">Creator</span>' : '') . '<span class="muted">' . ago((int)$c['created_at']) . '</span></div><div class="comment-body">' . $body . '</div><div class="comment-actions">'
        . '<form method="post" action="/read/action/comment-like" class="js-like">' . csrf_field() . '<input type="hidden" name="comment" value="' . (int)$c['id'] . '"><button class="like small' . ($c['liked'] ? ' on' : '') . '" type="submit" aria-pressed="' . ($c['liked'] ? 'true' : 'false') . '" aria-label="Like comment">' . icon('heart') . '<b>' . compact_number((int)$c['likes']) . '</b></button></form>';
    if ($viewer && !$reply) $b .= '<details class="reply-toggle"><summary>Reply</summary>' . comment_form($e, (int)$c['id'], $viewer) . '</details>';
    if ($mine || owns($viewer, $s) || is_admin($viewer)) $b .= '<form method="post" action="/read/action/comment-delete">' . csrf_field() . '<input type="hidden" name="comment" value="' . (int)$c['id'] . '"><button class="link-btn" type="submit" data-confirm="Delete this comment?">Delete</button></form>';
    elseif ($viewer) $b .= report_form('comment', (int)$c['id']);
    return $b . '</div></div></li>';
}

function page_profile(?array $viewer, string $username): void {
    $u = one('SELECT * FROM users WHERE username = ?', [$username]);
    if (!$u || ((int)$u['banned'] === 1 && !is_admin($viewer))) { error_page(404, 'That creator does not exist.'); return; }
    $self = $viewer && (int)$viewer['id'] === (int)$u['id'];
    $series = all('SELECT s.*, ? AS author FROM series s WHERE s.user_id = ? AND ' . ($self ? 's.hidden = 0' : visible_series_sql($viewer)) . ' ORDER BY s.last_episode_at DESC', [$u['display_name'], $u['id']]);
    $totals = one('SELECT COALESCE(SUM(subscribers), 0) AS subs, COALESCE(SUM(views), 0) AS views FROM series WHERE user_id = ? AND published = 1 AND hidden = 0', [$u['id']]);
    $b = '<div class="wrap stack"><section class="profile">' . avatar($u, 96) . '<div><h1>' . h($u['display_name']) . '</h1><p class="muted">@' . h($u['username']) . ' · joined ' . date('F Y', (int)$u['created_at']) . '</p>'
        . ($u['bio'] ? '<p class="bio">' . nl2br(h($u['bio'])) . '</p>' : '') . '<ul class="stat-line"><li><b>' . count($series) . '</b> series</li><li><b>' . compact_number((int)$totals['subs']) . '</b> subscribers</li><li><b>' . compact_number((int)$totals['views']) . '</b> views</li></ul>'
        . '<div class="actions">' . ($u['support_url'] ? '<a class="btn" href="' . h($u['support_url']) . '" rel="noopener nofollow ugc" target="_blank">' . icon('gift') . 'Support</a>' : '') . ($self ? '<a class="btn ghost" href="/read/settings">' . icon('pen') . 'Edit profile</a>' : '') . '</div></div></section>'
        . '<section><div class="sec-head"><h2>Series</h2></div>' . grid($series, ['empty' => $self ? 'You have not published a series yet.' : 'No published series yet.']) . '</section></div>';
    layout($u['display_name'], $b, ['description' => mb_strimwidth($u['bio'] ?: 'Comics by ' . $u['display_name'], 0, 160, '…')]);
}

function page_library(array $viewer): void {
    $tab = ($_GET['tab'] ?? '') === 'history' ? 'history' : 'subscribed';
    $b = '<div class="wrap stack"><section><div class="page-head"><h1>Library</h1><div class="seg"><a href="/read/library"' . ($tab === 'subscribed' ? ' aria-current="true"' : '') . '>Subscribed</a><a href="/read/library?tab=history"' . ($tab === 'history' ? ' aria-current="true"' : '') . '>Recent</a></div></div>';
    if ($tab === 'subscribed') {
        $list = all('SELECT s.*, u.display_name AS author, (SELECT COUNT(*) FROM episodes e WHERE e.series_id = s.id AND ' . live_episode_sql('e') . ' AND NOT EXISTS(SELECT 1 FROM history h WHERE h.user_id = :me AND h.episode_id = e.id)) AS unread
            FROM subscriptions sub JOIN series s ON s.id = sub.series_id JOIN users u ON u.id = s.user_id WHERE sub.user_id = :me AND u.banned = 0 AND ' . visible_series_sql($viewer) . ' ORDER BY s.last_episode_at DESC', ['me' => $viewer['id']]);
        if (!$list) $b .= grid([], ['empty' => 'Subscribe to a series and new episodes will show up here.']);
        else $b .= '<div class="grid">' . implode('', array_map(fn($s) => series_card($s, ['meta' => (int)$s['unread'] > 0 ? (int)$s['unread'] . ' new' : 'Up to date']), $list)) . '</div>';
    } else {
        $list = all('SELECT s.slug, s.title, s.cover, s.rating, e.number, e.title AS ep_title, h.read_at FROM history h JOIN episodes e ON e.id = h.episode_id JOIN series s ON s.id = h.series_id
            WHERE h.user_id = ? AND ' . visible_series_sql($viewer) . ' ORDER BY h.read_at DESC LIMIT 60', [$viewer['id']]);
        if (!$list) $b .= grid([], ['empty' => 'Episodes you read will appear here.']);
        $b .= '<div class="history">';
        foreach ($list as $c) $b .= '<a class="continue" href="/read/s/' . h($c['slug']) . '/' . (int)$c['number'] . '">' . cover_img($c, 'mini-cover') . '<span><strong>' . h($c['title']) . '</strong><span>Ep. ' . (int)$c['number'] . ' · ' . h($c['ep_title']) . '</span><small>' . ago((int)$c['read_at']) . '</small></span></a>';
        $b .= '</div>';
    }
    layout('Library', $b . '</section></div>', ['active' => 'library', 'noindex' => true]);
}

function page_rules(): void {
    $b = '<section class="narrow prose-page"><h1>Community rules</h1>'
        . '<h2>Mark mature work</h2><p>Series with explicit sexual content, graphic gore or other adult themes must be rated <b>Mature 18+</b>. Mature series are hidden from everyone who has not signed in, confirmed they are 18 or older and turned on mature content.</p>'
        . '<h2>Never allowed</h2><ul><li>Sexual content involving minors or characters presented as minors, in any style.</li><li>Non-consensual intimate imagery of real people.</li><li>Harassment, threats or hate directed at people or groups.</li><li>Work you do not have the right to publish.</li><li>Spam, scams and misleading links.</li></ul>'
        . '<h2>Reporting</h2><p>Use <b>Report</b> on any series or comment. Moderators review reports and may hide content or suspend accounts. Copyright owners can report infringing work with details of the original.</p>'
        . '<h2>Support links</h2><p>Creators may link one support page (such as Ko-fi or Patreon). Scroll Comics does not process payments or take a share.</p></section>';
    layout('Community rules', $b);
}
