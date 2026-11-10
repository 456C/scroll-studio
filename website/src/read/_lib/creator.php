<?php
declare(strict_types=1);
// Creator dashboard and the token API used by Scroll Studio to publish episodes.
namespace Comics;

function my_series(array $u, string $slug): array {
    $s = one('SELECT * FROM series WHERE slug = ?', [$slug]);
    ensure($s !== null && (owns($u, $s) || is_admin($u)), 'Series not found.', 404);
    return $s;
}
function delete_series_media(int $seriesId): void {
    $s = one('SELECT cover, banner FROM series WHERE id = ?', [$seriesId]);
    if ($s) { remove_media($s['cover']); remove_media($s['banner']); }
    foreach (all('SELECT id, thumb FROM episodes WHERE series_id = ?', [$seriesId]) as $e) delete_episode_media((int)$e['id'], $e['thumb']);
}
function delete_episode_media(int $episodeId, ?string $thumb): void {
    foreach (all('SELECT file FROM episode_images WHERE episode_id = ?', [$episodeId]) as $i) remove_media($i['file']);
    remove_media($thumb);
    $dir = media_dir() . '/episodes/' . $episodeId;
    if (is_dir($dir)) @rmdir($dir);
}

function page_dashboard(array $u): void {
    $series = all('SELECT s.*, (SELECT COUNT(*) FROM episodes e WHERE e.series_id = s.id) AS episode_count,
        (SELECT COALESCE(SUM(likes), 0) FROM episodes e WHERE e.series_id = s.id) AS likes,
        (SELECT COALESCE(SUM(comments), 0) FROM episodes e WHERE e.series_id = s.id) AS comment_count FROM series s WHERE s.user_id = ? ORDER BY s.updated_at DESC', [$u['id']]);
    $t = ['views' => 0, 'subscribers' => 0, 'likes' => 0, 'comment_count' => 0];
    foreach ($series as $s) foreach ($t as $k => $_) $t[$k] += (int)$s[$k];
    $b = '<div class="wrap stack"><div class="page-head"><h1>Creator dashboard</h1><a class="btn primary" href="/read/create/new">' . icon('plus') . 'New series</a></div>';
    if (!$series) {
        $b .= '<section class="onboard"><div class="onboard-steps"><div><span>1</span><strong>Make your episode</strong><p>In Scroll Studio, use <b>Export → WEBTOON</b> or <b>Tapas</b>.</p></div>'
            . '<div><span>2</span><strong>Create a series</strong><p>Add a title, cover and genre.</p></div><div><span>3</span><strong>Upload</strong><p>Drop the exported images or ZIP. They stay full quality.</p></div></div>'
            . '<a class="btn primary" href="/read/create/new">' . icon('plus') . 'Create your first series</a></section>';
    } else {
        $b .= '<ul class="kpis"><li><span>Views</span><b>' . compact_number($t['views']) . '</b></li><li><span>Subscribers</span><b>' . compact_number($t['subscribers']) . '</b></li>'
            . '<li><span>Likes</span><b>' . compact_number($t['likes']) . '</b></li><li><span>Comments</span><b>' . compact_number($t['comment_count']) . '</b></li></ul><div class="dash-list">';
        foreach ($series as $s) {
            $state = $s['hidden'] ? '<span class="badge mature">Hidden by moderator</span>' : ($s['published'] ? '<span class="badge live">Public</span>' : '<span class="badge">Private</span>');
            $b .= '<a class="dash-item" href="/read/create/' . h($s['slug']) . '">' . cover_img($s, 'mini-cover') . '<span class="dash-text"><strong>' . h($s['title']) . '</strong> ' . $state . rating_badge($s['rating'])
                . '<span class="muted">' . (int)$s['episode_count'] . ' episodes · ' . compact_number((int)$s['views']) . ' views · ' . compact_number((int)$s['subscribers']) . ' subscribers · ★ ' . stars($s) . '</span></span>' . icon('right') . '</a>';
        }
        $b .= '</div>';
    }
    layout('Creator dashboard', $b . '</div>', ['active' => 'create', 'noindex' => true]);
}

function series_form(array $s, ?string $error = null): string {
    $isNew = empty($s['id']);
    $days = array_filter(explode(',', $s['update_days'] ?? ''));
    $dayBoxes = '';
    foreach (DAYS as $k => $v) $dayBoxes .= '<label class="toggle"><input type="checkbox" name="days[]" value="' . $k . '"' . (in_array($k, $days, true) ? ' checked' : '') . '><span>' . $v . '</span></label>';
    $ratingRadios = '';
    foreach (RATINGS as $k => $v) $ratingRadios .= '<label class="toggle"><input type="radio" name="rating" value="' . $k . '"' . (($s['rating'] ?? 'all') === $k ? ' checked' : '') . ' required><span>' . $v . '</span></label>';
    return ($error ? '<p class="form-error" role="alert">' . h($error) . '</p>' : '')
        . '<form method="post" enctype="multipart/form-data" class="form series-form" action="' . ($isNew ? '/read/create/new' : '/read/create/' . h($s['slug']) . '/save') . '">' . csrf_field()
        . '<div class="form-grid"><div class="form-art"><div class="cover-drop">' . (empty($s['cover']) ? '<span class="cover placeholder">' . icon('upload') . '</span>' : cover_img($s)) . '</div>'
        . field('Cover', '<input type="file" name="cover" accept="image/png,image/jpeg,image/webp" data-preview>', '3:4, at least 600 × 800.')
        . field('Banner', '<input type="file" name="banner" accept="image/png,image/jpeg,image/webp">', 'Wide image for your series header and the home carousel.') . '</div>'
        . '<div class="form-main">' . field('Title', '<input name="title" required maxlength="80" value="' . h($s['title'] ?? '') . '">')
        . field('Tagline', '<input name="tagline" maxlength="140" value="' . h($s['tagline'] ?? '') . '" placeholder="One line that hooks readers">')
        . field('Synopsis', '<textarea name="synopsis" rows="5" maxlength="3000">' . h($s['synopsis'] ?? '') . '</textarea>')
        . '<div class="two">' . field('Genre', select('genre', GENRES, $s['genre'] ?? null)) . field('Second genre', select('genre2', GENRES, $s['genre2'] ?? null, true)) . '</div>'
        . '<fieldset class="field"><legend class="field-label">Audience</legend><div class="toggles">' . $ratingRadios . '</div><span class="hint">Mature 18+ is required for explicit sexual content or graphic gore. Mature series are only shown to signed-in adults who opt in.</span></fieldset>'
        . '<div class="two">' . field('Status', select('status', STATUSES, $s['status'] ?? 'ongoing')) . field('Support link', '<input name="support_url" type="url" maxlength="300" placeholder="https://ko-fi.com/you" value="' . h($s['support_url'] ?? '') . '">') . '</div>'
        . '<fieldset class="field"><legend class="field-label">Update days</legend><div class="toggles">' . $dayBoxes . '</div></fieldset>'
        . '<label class="check"><input type="checkbox" name="published" value="1"' . (!empty($s['published']) || $isNew ? ' checked' : '') . '> Public: list this series once it has a published episode</label>'
        . '<div class="actions"><button class="btn primary" type="submit">' . ($isNew ? 'Create series' : 'Save changes') . '</button></div></div></div></form>';
}

function series_from_post(array $u, ?array $s): array {
    $title = post('title', 80);
    ensure($title !== '', 'Give your series a title.');
    $genre = post('genre', 20); $genre2 = post('genre2', 20) ?: null;
    ensure(isset(GENRES[$genre]) && ($genre2 === null || isset(GENRES[$genre2])), 'Choose a genre.');
    $rating = post('rating', 10); ensure(isset(RATINGS[$rating]), 'Choose an audience.');
    $status = post('status', 10); ensure(isset(STATUSES[$status]), 'Choose a status.');
    $days = array_values(array_intersect(array_keys(DAYS), array_map('strval', (array)($_POST['days'] ?? []))));
    return ['title' => $title, 'tagline' => post('tagline', 140), 'synopsis' => post('synopsis', 3000), 'genre' => $genre, 'genre2' => $genre2 === $genre ? null : $genre2,
        'rating' => $rating, 'status' => $status, 'update_days' => implode(',', $days), 'support_url' => valid_url(post('support_url', 300)), 'published' => ($_POST['published'] ?? '') === '1' ? 1 : 0];
}

function page_series_new(array $u, ?string $error = null): void {
    $draft = $_SERVER['REQUEST_METHOD'] === 'POST' ? $_POST + ['update_days' => implode(',', (array)($_POST['days'] ?? []))] : [];
    layout('New series', '<div class="wrap narrow-wide stack"><div class="page-head"><a class="back" href="/read/create">' . icon('left') . 'Dashboard</a><h1>New series</h1></div>' . series_form($draft, $error) . '</div>', ['active' => 'create', 'noindex' => true]);
}
function do_series_new(array $u): void {
    check_post();
    try {
        throttle('series', 10, 3600);
        $d = series_from_post($u, null);
        $cover = upload_preview('cover', 'covers', 600, 800);
        $banner = upload_preview('banner', 'banners', 1600, 600);
        $slug = slugify($d['title']);
        insert('INSERT INTO series(user_id, slug, title, tagline, synopsis, genre, genre2, rating, status, update_days, support_url, published, cover, banner, created_at, updated_at)
            VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)', [$u['id'], $slug, $d['title'], $d['tagline'], $d['synopsis'], $d['genre'], $d['genre2'], $d['rating'], $d['status'], $d['update_days'], $d['support_url'], $d['published'], $cover, $banner, now(), now()]);
        redirect("/read/create/$slug", 'Series created. Add your first episode.');
    } catch (Problem $p) { http_response_code($p->status); page_series_new($u, $p->getMessage()); }
}

function page_series_manage(array $u, string $slug, ?string $error = null): void {
    $s = my_series($u, $slug);
    $eps = all('SELECT e.*, (SELECT COUNT(*) FROM episode_images i WHERE i.episode_id = e.id) AS pages FROM episodes e WHERE e.series_id = ? ORDER BY e.number DESC', [$s['id']]);
    $b = '<div class="wrap stack"><div class="page-head"><a class="back" href="/read/create">' . icon('left') . 'Dashboard</a><h1>' . h($s['title']) . '</h1>'
        . '<div class="actions"><a class="btn" href="/read/s/' . h($slug) . '">' . icon('eye') . 'View</a><a class="btn primary" href="/read/create/' . h($slug) . '/episode">' . icon('upload') . 'New episode</a></div></div>'
        . '<div class="manage"><section class="panel"><h2>Episodes</h2>';
    if (!$eps) $b .= '<div class="empty small">' . icon('upload') . '<p>No episodes yet.</p><a class="btn primary" href="/read/create/' . h($slug) . '/episode">Upload episode 1</a></div>';
    else {
        $b .= '<table class="ep-table"><thead><tr><th>#</th><th>Episode</th><th>Status</th><th class="num">Views</th><th class="num">Likes</th><th class="num">Comments</th><th></th></tr></thead><tbody>';
        foreach ($eps as $e) {
            $st = episode_live($e) ? '<span class="badge live">Live</span>' : ($e['status'] === 'scheduled' ? '<span class="badge">' . date('M j, H:i', (int)$e['publish_at']) . '</span>' : ($e['hidden'] ? '<span class="badge mature">Hidden</span>' : '<span class="badge">Draft</span>'));
            $b .= '<tr><td>' . (int)$e['number'] . '</td><td><a href="/read/create/' . h($slug) . '/episode/' . (int)$e['id'] . '">' . h($e['title']) . '</a><small class="muted">' . (int)$e['pages'] . ' images</small></td><td>' . $st . '</td>'
                . '<td class="num">' . compact_number((int)$e['views']) . '</td><td class="num">' . compact_number((int)$e['likes']) . '</td><td class="num">' . compact_number((int)$e['comments']) . '</td>'
                . '<td class="num"><a href="/read/s/' . h($slug) . '/' . (int)$e['number'] . '">Open</a></td></tr>';
        }
        $b .= '</tbody></table>';
    }
    $b .= '</section><section class="panel"><h2>Series details</h2>' . series_form($s, $error) . '</section>'
        . '<section class="panel danger-zone"><h2>Delete series</h2><form method="post" action="/read/create/' . h($slug) . '/delete" class="inline-form">' . csrf_field()
        . '<input name="confirm" required placeholder="Type the series title" aria-label="Type the series title to confirm"><button class="btn danger" type="submit">Delete permanently</button></form></section></div></div>';
    layout($s['title'] . ' · Dashboard', $b, ['active' => 'create', 'noindex' => true]);
}
function do_series_save(array $u, string $slug): void {
    check_post();
    $s = my_series($u, $slug);
    try {
        $d = series_from_post($u, $s);
        if ($cover = upload_preview('cover', 'covers', 600, 800)) { remove_media($s['cover']); $d['cover'] = $cover; }
        if ($banner = upload_preview('banner', 'banners', 1600, 600)) { remove_media($s['banner']); $d['banner'] = $banner; }
        $d['updated_at'] = now();
        $sets = implode(', ', array_map(fn($k) => "$k = :$k", array_keys($d)));
        run("UPDATE series SET $sets WHERE id = :id", $d + ['id' => $s['id']]);
        redirect("/read/create/$slug", 'Series saved.');
    } catch (Problem $p) { http_response_code($p->status); page_series_manage($u, $slug, $p->getMessage()); }
}
function do_series_delete(array $u, string $slug): void {
    check_post();
    $s = my_series($u, $slug);
    ensure(post('confirm', 80) === $s['title'], 'Type the series title exactly to delete it.');
    delete_series_media((int)$s['id']);
    run('DELETE FROM series WHERE id = ?', [$s['id']]);
    redirect('/read/create', 'Series deleted.');
}

function page_episode_form(array $u, string $slug, ?int $episodeId, ?string $error = null): void {
    $s = my_series($u, $slug);
    $e = $episodeId ? one('SELECT * FROM episodes WHERE id = ? AND series_id = ?', [$episodeId, $s['id']]) : null;
    ensure(!$episodeId || $e, 'Episode not found.', 404);
    $next = (int)value('SELECT COALESCE(MAX(number), 0) + 1 FROM episodes WHERE series_id = ?', [$s['id']]);
    $images = $e ? all('SELECT * FROM episode_images WHERE episode_id = ? ORDER BY position', [$e['id']]) : [];
    $status = $e['status'] ?? 'published';
    $when = $e && $e['publish_at'] ? date('Y-m-d\TH:i', (int)$e['publish_at']) : '';
    $action = $e ? '/read/create/' . h($slug) . '/episode/' . (int)$e['id'] : '/read/create/' . h($slug) . '/episode';
    $b = '<div class="wrap narrow-wide stack"><div class="page-head"><a class="back" href="/read/create/' . h($slug) . '">' . icon('left') . h($s['title']) . '</a><h1>' . ($e ? 'Edit episode ' . (int)$e['number'] : 'New episode') . '</h1></div>'
        . ($error ? '<p class="form-error" role="alert">' . h($error) . '</p>' : '')
        . '<form method="post" enctype="multipart/form-data" class="form" action="' . $action . '" data-upload>' . csrf_field()
        . '<div class="dropzone" data-dropzone>' . icon('upload') . '<strong>' . ($images ? 'Replace episode images' : 'Drop episode images or a ZIP') . '</strong><span>Scroll Studio WEBTOON or Tapas exports work as-is. Files are ordered by name. PNG, JPEG or WebP up to ' . MAX_IMAGE_WIDTH . ' px wide.</span>'
        . '<input type="file" name="images[]" accept="image/png,image/jpeg,image/webp,.zip,application/zip" multiple' . ($e ? '' : ' required') . ' aria-label="Episode images"><ol class="file-list" data-files></ol></div>';
    if ($images) {
        $b .= '<details class="current-strip"><summary>Current strip · ' . count($images) . ' images</summary><div class="strip-preview">';
        foreach (array_slice($images, 0, 40) as $i) $b .= '<img src="' . h(media($i['file'])) . '" alt="" loading="lazy" width="' . (int)$i['width'] . '" height="' . (int)$i['height'] . '">';
        $b .= '</div></details>';
    }
    $b .= '<div class="two">' . field('Episode number', '<input name="number" type="number" min="0" max="9999" required value="' . (int)($e['number'] ?? $next) . '">') . field('Title', '<input name="title" maxlength="100" value="' . h($e['title'] ?? '') . '" placeholder="Episode ' . $next . '">') . '</div>'
        . field('Thumbnail', '<input type="file" name="thumb" accept="image/png,image/jpeg,image/webp">', 'Square. If left empty, the top of the first image is used.')
        . field('Creator note', '<textarea name="note" rows="3" maxlength="2000" placeholder="Say hi to your readers (optional)">' . h($e['note'] ?? '') . '</textarea>')
        . '<fieldset class="field"><legend class="field-label">Publish</legend><div class="toggles">'
        . '<label class="toggle"><input type="radio" name="status" value="published"' . ($status === 'published' ? ' checked' : '') . '><span>Now</span></label>'
        . '<label class="toggle"><input type="radio" name="status" value="scheduled"' . ($status === 'scheduled' ? ' checked' : '') . '><span>Schedule</span></label>'
        . '<label class="toggle"><input type="radio" name="status" value="draft"' . ($status === 'draft' ? ' checked' : '') . '><span>Draft</span></label></div>'
        . '<input name="publish_at" type="datetime-local" value="' . h($when) . '" aria-label="Scheduled time" data-schedule><span class="hint">Times use the server clock (' . date('T') . ').</span></fieldset>'
        . '<div class="progress-bar" data-progress hidden><span></span></div><div class="actions"><button class="btn primary" type="submit">' . ($e ? 'Save episode' : 'Upload episode') . '</button>'
        . ($e ? '<a class="btn" href="/read/s/' . h($slug) . '/' . (int)$e['number'] . '">Preview</a>' : '') . '</div></form>';
    if ($e) $b .= '<section class="panel danger-zone"><h2>Delete episode</h2><form method="post" action="/read/create/' . h($slug) . '/episode/' . (int)$e['id'] . '/delete">' . csrf_field() . '<button class="btn danger" type="submit" data-confirm="Delete this episode, its likes and comments?">Delete episode</button></form></section>';
    layout(($e ? 'Edit' : 'New') . ' episode · ' . $s['title'], $b . '</div>', ['active' => 'create', 'noindex' => true]);
}

/** Shared by the dashboard form and the API. Returns the saved episode row. */
function save_episode(array $s, ?array $e, array $in, string $field = 'images'): array {
    $title = trim((string)($in['title'] ?? ''));
    $number = (int)($in['number'] ?? 0);
    ensure($number >= 0 && $number <= 9999, 'Episode numbers run from 0 to 9999.');
    $title = mb_substr($title !== '' ? $title : 'Episode ' . $number, 0, 100);
    $status = (string)($in['status'] ?? 'published');
    ensure(in_array($status, ['published', 'scheduled', 'draft'], true), 'Choose when to publish.');
    $publishAt = null;
    if ($status === 'scheduled') {
        $publishAt = strtotime((string)($in['publish_at'] ?? ''));
        ensure($publishAt !== false && $publishAt > now(), 'Choose a future time to schedule the episode.');
    } elseif ($status === 'published') {
        $publishAt = $e && $e['status'] === 'published' && $e['publish_at'] ? (int)$e['publish_at'] : now();
    }
    $note = mb_substr(trim(str_replace("\r\n", "\n", (string)($in['note'] ?? ''))), 0, 2000);
    $clash = value('SELECT id FROM episodes WHERE series_id = ? AND number = ?', [$s['id'], $number]);
    ensure(!$clash || ($e && (int)$clash === (int)$e['id']), "Episode $number already exists in this series.");
    $cleanup = [];
    try {
        $sources = episode_sources($field, $cleanup);
        ensure($e || $sources, 'Add the episode images.');
        return tx(function () use ($s, $e, $title, $number, $status, $publishAt, $note, $sources) {
            if ($e) {
                run('UPDATE episodes SET number = ?, title = ?, note = ?, status = ?, publish_at = ?, updated_at = ? WHERE id = ?', [$number, $title, $note, $status, $publishAt, now(), $e['id']]);
                $id = (int)$e['id'];
            } else {
                $id = insert('INSERT INTO episodes(series_id, number, title, note, status, publish_at, created_at, updated_at) VALUES(?, ?, ?, ?, ?, ?, ?, ?)', [$s['id'], $number, $title, $note, $status, $publishAt, now(), now()]);
            }
            if ($sources) save_episode_images($id, $sources);
            $thumb = upload_preview('thumb', 'thumbs', 320, 320);
            $old = $e['thumb'] ?? null;
            if (!$thumb && (!$old || $sources)) {
                $first = value('SELECT file FROM episode_images WHERE episode_id = ? ORDER BY position LIMIT 1', [$id]);
                if ($first) $thumb = preview(media_dir() . '/' . $first, 'thumbs', 320, 320, 'top');
            }
            if ($thumb) { remove_media($old); run('UPDATE episodes SET thumb = ? WHERE id = ?', [$thumb, $id]); }
            run('UPDATE series SET updated_at = ? WHERE id = ?', [now(), $s['id']]);
            refresh_series((int)$s['id']);
            return one('SELECT * FROM episodes WHERE id = ?', [$id]);
        });
    } finally { foreach ($cleanup as $f) if (is_file($f)) unlink($f); }
}

function do_episode_save(array $u, string $slug, ?int $episodeId): void {
    check_post();
    $s = my_series($u, $slug);
    $e = $episodeId ? one('SELECT * FROM episodes WHERE id = ? AND series_id = ?', [$episodeId, $s['id']]) : null;
    ensure(!$episodeId || $e, 'Episode not found.', 404);
    try {
        $saved = save_episode($s, $e, $_POST);
        $msg = episode_live($saved) ? 'Episode ' . $saved['number'] . ' is live.' : ($saved['status'] === 'scheduled' ? 'Episode scheduled.' : 'Draft saved.');
        if (wants_json()) { header('Content-Type: application/json'); echo json_encode(['redirect' => "/read/create/$slug"]); flash($msg); exit; }
        redirect("/read/create/$slug", $msg);
    } catch (Problem $p) {
        if (wants_json()) { http_response_code($p->status); header('Content-Type: application/json'); echo json_encode(['error' => $p->getMessage()]); exit; }
        http_response_code($p->status); page_episode_form($u, $slug, $episodeId, $p->getMessage());
    }
}
function do_episode_delete(array $u, string $slug, int $episodeId): void {
    check_post();
    $s = my_series($u, $slug);
    $e = one('SELECT * FROM episodes WHERE id = ? AND series_id = ?', [$episodeId, $s['id']]);
    ensure($e !== null, 'Episode not found.', 404);
    delete_episode_media((int)$e['id'], $e['thumb']);
    run('DELETE FROM episodes WHERE id = ?', [$e['id']]);
    refresh_series((int)$s['id']);
    redirect("/read/create/$slug", 'Episode deleted.');
}

// ---------- Token API (JSON) ----------
function api(string $path): void {
    header('Content-Type: application/json; charset=utf-8');
    header('Cache-Control: no-store');
    try {
        $u = api_user();
        $method = $_SERVER['REQUEST_METHOD'];
        $base = (secure() ? 'https://' : 'http://') . ($_SERVER['HTTP_HOST'] ?? 'localhost');
        $episodeJson = fn(array $s, array $e) => ['id' => (int)$e['id'], 'number' => (int)$e['number'], 'title' => $e['title'], 'status' => $e['status'], 'live' => episode_live($e),
            'publish_at' => $e['publish_at'] ? (int)$e['publish_at'] : null, 'url' => $base . '/read/s/' . $s['slug'] . '/' . $e['number']];
        if ($path === 'me' && $method === 'GET') {
            $series = all('SELECT * FROM series WHERE user_id = ? ORDER BY updated_at DESC', [$u['id']]);
            echo json_encode(['user' => ['username' => $u['username'], 'display_name' => $u['display_name']], 'series' => array_map(fn($s) => [
                'slug' => $s['slug'], 'title' => $s['title'], 'rating' => $s['rating'], 'url' => $base . '/read/s/' . $s['slug'],
                'next_number' => (int)value('SELECT COALESCE(MAX(number), 0) + 1 FROM episodes WHERE series_id = ?', [$s['id']])], $series)], JSON_UNESCAPED_SLASHES);
            return;
        }
        if (preg_match('#^series/([a-z0-9-]+)/episodes$#', $path, $m)) {
            $s = my_series($u, $m[1]);
            if ($method === 'GET') {
                echo json_encode(['episodes' => array_map(fn($e) => $episodeJson($s, $e), all('SELECT * FROM episodes WHERE series_id = ? ORDER BY number', [$s['id']]))], JSON_UNESCAPED_SLASHES);
                return;
            }
            ensure($method === 'POST', 'Method not allowed.', 405);
            throttle('api-upload', 60, 3600);
            $in = $_POST;
            $in['number'] ??= value('SELECT COALESCE(MAX(number), 0) + 1 FROM episodes WHERE series_id = ?', [$s['id']]);
            $in['status'] ??= 'draft';
            $existing = !empty($in['replace']) ? one('SELECT * FROM episodes WHERE series_id = ? AND number = ?', [$s['id'], (int)$in['number']]) : null;
            $e = save_episode($s, $existing, $in);
            http_response_code($existing ? 200 : 201);
            echo json_encode(['episode' => $episodeJson($s, $e), 'edit_url' => $base . '/read/create/' . $s['slug'] . '/episode/' . $e['id']], JSON_UNESCAPED_SLASHES);
            return;
        }
        throw new Problem('Not found.', 404);
    } catch (Problem $p) { http_response_code($p->status); echo json_encode(['error' => $p->getMessage()]); }
}
