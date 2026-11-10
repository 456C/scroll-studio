<?php
declare(strict_types=1);
// Sign-in, registration, settings, interactions and moderation.
namespace Comics;

function safe_next(?string $next): string {
    $next = (string)$next;
    return str_starts_with($next, '/read') && !str_starts_with($next, '//') && !preg_match('/[\r\n]/', $next) ? $next : '/read/';
}

function page_login(?string $error = null): void {
    if (user()) redirect(safe_next($_GET['next'] ?? null));
    $next = safe_next($_GET['next'] ?? $_POST['next'] ?? null);
    $b = '<section class="auth"><a class="logo" href="/read/"><img src="/assets/app-icon.png" alt="" width="34" height="34"><span>Scroll <b>Comics</b></span></a><h1>Welcome back</h1>'
        . ($error ? '<p class="form-error" role="alert">' . h($error) . '</p>' : '')
        . '<form method="post" action="/read/login" class="form">' . csrf_field() . '<input type="hidden" name="next" value="' . h($next) . '">'
        . field('Username or email', '<input name="login" autocomplete="username" required maxlength="254" value="' . h($_POST['login'] ?? '') . '">')
        . field('Password', '<input name="password" type="password" autocomplete="current-password" required maxlength="200">')
        . '<button class="btn primary wide" type="submit">Sign in</button></form><p class="muted center">New here? <a href="/read/register?next=' . h(rawurlencode($next)) . '">Create an account</a></p></section>';
    layout('Sign in', $b, ['bare' => true, 'bodyClass' => 'auth-page', 'noindex' => true]);
}
function do_login(): void {
    check_post();
    try {
        throttle('login', 10, 900);
        $login = post('login', 254);
        $u = one('SELECT * FROM users WHERE username = ? OR email = ?', [$login, $login]);
        $ok = $u && password_verify((string)($_POST['password'] ?? ''), $u['pass']);
        ensure($ok, 'That username or password is not right.');
        ensure((int)$u['banned'] === 0, 'This account is suspended.', 403);
        if (password_needs_rehash($u['pass'], PASSWORD_DEFAULT)) run('UPDATE users SET pass = ? WHERE id = ?', [password_hash((string)$_POST['password'], PASSWORD_DEFAULT), $u['id']]);
        start_session((int)$u['id']);
        redirect(safe_next($_POST['next'] ?? null), 'Signed in as ' . $u['display_name'] . '.');
    } catch (Problem $p) { http_response_code($p->status); page_login($p->getMessage()); }
}

function page_register(?string $error = null): void {
    if (user()) redirect('/read/');
    $next = safe_next($_GET['next'] ?? $_POST['next'] ?? null);
    $v = fn($k) => h($_POST[$k] ?? '');
    $b = '<section class="auth"><a class="logo" href="/read/"><img src="/assets/app-icon.png" alt="" width="34" height="34"><span>Scroll <b>Comics</b></span></a><h1>Create your account</h1>'
        . ($error ? '<p class="form-error" role="alert">' . h($error) . '</p>' : '')
        . '<form method="post" action="/read/register" class="form">' . csrf_field() . '<input type="hidden" name="next" value="' . h($next) . '">'
        . field('Username', '<input name="username" autocomplete="username" required minlength="3" maxlength="24" pattern="[A-Za-z0-9_]+" value="' . $v('username') . '">', 'Letters, numbers and underscores. Shown in your profile link.')
        . field('Display name', '<input name="display_name" maxlength="40" value="' . $v('display_name') . '">')
        . field('Email', '<input name="email" type="email" autocomplete="email" required maxlength="254" value="' . $v('email') . '">')
        . field('Password', '<input name="password" type="password" autocomplete="new-password" required minlength="10" maxlength="200">', 'At least 10 characters.')
        . field('Date of birth', '<input name="birthdate" type="date" required max="' . date('Y-m-d') . '" value="' . $v('birthdate') . '">', 'Used only to decide whether mature series are available to you. Not shown publicly.')
        . '<label class="check"><input type="checkbox" name="rules" value="1" required> I agree to the <a href="/read/rules" target="_blank">community rules</a>.</label>'
        . '<button class="btn primary wide" type="submit">Create account</button></form><p class="muted center">Already have an account? <a href="/read/login?next=' . h(rawurlencode($next)) . '">Sign in</a></p></section>';
    layout('Join', $b, ['bare' => true, 'bodyClass' => 'auth-page', 'noindex' => true]);
}
function do_register(): void {
    check_post();
    try {
        ensure(($_POST['rules'] ?? '') === '1', 'Please agree to the community rules.');
        throttle('register', 5, 3600);
        $id = register(post('username', 24), post('email', 254), (string)($_POST['password'] ?? ''), post('birthdate', 10), post('display_name', 40));
        start_session($id);
        redirect(safe_next($_POST['next'] ?? null), 'Welcome to Scroll Comics!');
    } catch (Problem $p) { http_response_code($p->status); page_register($p->getMessage()); }
}

function page_settings(array $u, ?string $newToken = null): void {
    $tokens = all('SELECT * FROM api_tokens WHERE user_id = ? ORDER BY created_at DESC', [$u['id']]);
    $adult = is_adult($u);
    $b = '<div class="wrap narrow stack"><div class="page-head"><h1>Settings</h1></div>'
        . '<section class="panel"><h2>Profile</h2><form method="post" action="/read/settings/profile" enctype="multipart/form-data" class="form">' . csrf_field()
        . '<div class="avatar-row">' . avatar($u, 64) . field('Avatar', '<input type="file" name="avatar" accept="image/png,image/jpeg,image/webp">') . '</div>'
        . field('Display name', '<input name="display_name" required maxlength="40" value="' . h($u['display_name']) . '">')
        . field('Bio', '<textarea name="bio" rows="3" maxlength="500">' . h($u['bio']) . '</textarea>')
        . field('Support link', '<input name="support_url" type="url" maxlength="300" placeholder="https://ko-fi.com/you" value="' . h($u['support_url']) . '">', 'Ko-fi, Patreon or similar. Shown on your profile and series.')
        . '<button class="btn primary" type="submit">Save profile</button></form></section>'
        . '<section class="panel"><h2>Mature content</h2>';
    if ($adult) $b .= '<form method="post" action="/read/action/mature" class="inline-setting">' . csrf_field() . '<input type="hidden" name="back" value="/read/settings"><input type="hidden" name="on" value="' . ((int)$u['show_mature'] ? '0' : '1') . '">'
        . '<p>' . ((int)$u['show_mature'] ? 'Mature 18+ series are shown in browse, search and the Mature section.' : 'Mature 18+ series are hidden everywhere.') . '</p>'
        . '<button class="btn" type="submit">' . ((int)$u['show_mature'] ? 'Hide mature content' : 'Show mature content') . '</button></form>';
    else $b .= '<p class="muted">Mature series become available when you turn 18.</p>';
    $b .= '</section><section class="panel" id="app"><h2>Publish from Scroll Studio</h2><p class="muted">Create a token for the desktop app or your own scripts. It can publish and edit episodes in your series.</p>';
    if ($newToken) $b .= '<div class="token-reveal"><p><b>Copy this token now.</b> It will not be shown again.</p><code class="copy" data-copy>' . h($newToken) . '</code></div>';
    $b .= '<form method="post" action="/read/settings/token" class="inline-form">' . csrf_field() . '<input name="name" maxlength="40" placeholder="Token name, e.g. My laptop" required aria-label="Token name"><button class="btn" type="submit">' . icon('plus') . 'Create token</button></form>';
    if ($tokens) {
        $b .= '<ul class="token-list">';
        foreach ($tokens as $t) $b .= '<li><span><strong>' . h($t['name']) . '</strong><small>Created ' . date('M j, Y', (int)$t['created_at']) . ' · ' . ($t['last_used_at'] ? 'used ' . ago((int)$t['last_used_at']) : 'never used') . '</small></span>'
            . '<form method="post" action="/read/settings/token-revoke">' . csrf_field() . '<input type="hidden" name="id" value="' . (int)$t['id'] . '"><button class="link-btn danger" type="submit">Revoke</button></form></li>';
        $b .= '</ul>';
    }
    $b .= '</section><section class="panel"><h2>Password</h2><form method="post" action="/read/settings/password" class="form">' . csrf_field()
        . field('Current password', '<input name="current" type="password" autocomplete="current-password" required>')
        . field('New password', '<input name="password" type="password" autocomplete="new-password" minlength="10" required>')
        . '<button class="btn" type="submit">Change password</button></form></section>'
        . '<section class="panel danger-zone"><h2>Delete account</h2><p class="muted">Removes your account, series, episodes and comments permanently.</p><form method="post" action="/read/settings/delete" class="inline-form">' . csrf_field()
        . '<input name="confirm" placeholder="Type your username" aria-label="Type your username to confirm" required><button class="btn danger" type="submit">Delete account</button></form></section></div>';
    layout('Settings', $b, ['noindex' => true]);
}
function valid_url(string $url): string {
    if ($url === '') return '';
    ensure(filter_var($url, FILTER_VALIDATE_URL) !== false && str_starts_with(strtolower($url), 'https://'), 'Support links must start with https://.');
    return $url;
}
function do_settings(array $u, string $what): void {
    check_post();
    switch ($what) {
        case 'profile':
            $display = post('display_name', 40);
            ensure($display !== '', 'Enter a display name.');
            $avatar = upload_preview('avatar', 'avatars', 256, 256);
            if ($avatar) { remove_media($u['avatar']); run('UPDATE users SET avatar = ? WHERE id = ?', [$avatar, $u['id']]); }
            run('UPDATE users SET display_name = ?, bio = ?, support_url = ? WHERE id = ?', [$display, post('bio', 500), valid_url(post('support_url', 300)), $u['id']]);
            redirect('/read/settings', 'Profile saved.');
        case 'token':
            $name = post('name', 40);
            ensure($name !== '', 'Name the token.');
            ensure((int)value('SELECT COUNT(*) FROM api_tokens WHERE user_id = ?', [$u['id']]) < 10, 'Revoke an old token first (limit 10).');
            $raw = 'sct_' . bin2hex(random_bytes(24));
            run('INSERT INTO api_tokens(user_id, name, token, created_at) VALUES(?, ?, ?, ?)', [$u['id'], $name, hash('sha256', $raw), now()]);
            page_settings($u, $raw);
            return;
        case 'token-revoke':
            run('DELETE FROM api_tokens WHERE id = ? AND user_id = ?', [(int)($_POST['id'] ?? 0), $u['id']]);
            redirect('/read/settings#app', 'Token revoked.');
        case 'password':
            ensure(password_verify((string)($_POST['current'] ?? ''), $u['pass']), 'Your current password is not right.');
            $p = (string)($_POST['password'] ?? '');
            ensure(strlen($p) >= 10 && strlen($p) <= 200, 'Use a password with at least 10 characters.');
            run('UPDATE users SET pass = ? WHERE id = ?', [password_hash($p, PASSWORD_DEFAULT), $u['id']]);
            run('DELETE FROM sessions WHERE user_id = ?', [$u['id']]);
            start_session((int)$u['id']);
            redirect('/read/settings', 'Password changed. Other devices were signed out.');
        case 'delete':
            ensure(strcasecmp(post('confirm', 40), $u['username']) === 0, 'Type your username exactly to delete your account.');
            foreach (all('SELECT id FROM series WHERE user_id = ?', [$u['id']]) as $s) delete_series_media((int)$s['id']);
            remove_media($u['avatar']);
            run('DELETE FROM users WHERE id = ?', [$u['id']]);
            cookie('sc_session', '', 1);
            redirect('/read/', 'Your account was deleted.');
    }
    error_page(404, 'Unknown setting.');
}

// ---------- Interactions (form posts with JSON responses for scripts) ----------
function wants_json(): bool { return str_contains($_SERVER['HTTP_ACCEPT'] ?? '', 'application/json'); }
function done(array $json, string $message = ''): never {
    if (wants_json()) { header('Content-Type: application/json'); echo json_encode($json); exit; }
    redirect(back(), $message !== '' ? $message : null);
}
function do_action(?array $u, string $name): void {
    check_post();
    if (!$u) {
        if (wants_json()) { http_response_code(401); header('Content-Type: application/json'); echo json_encode(['login' => '/read/login?next=' . rawurlencode(back())]); exit; }
        redirect('/read/login?next=' . rawurlencode(back()));
    }
    $id = fn($k) => (int)($_POST[$k] ?? 0);
    switch ($name) {
        case 'subscribe':
            $s = one('SELECT * FROM series WHERE id = ?', [$id('series')]);
            ensure($s && series_access($u, $s) === 'ok', 'Series not found.', 404);
            $on = !value('SELECT 1 FROM subscriptions WHERE user_id = ? AND series_id = ?', [$u['id'], $s['id']]);
            $on ? run('INSERT INTO subscriptions VALUES(?, ?, ?)', [$u['id'], $s['id'], now()]) : run('DELETE FROM subscriptions WHERE user_id = ? AND series_id = ?', [$u['id'], $s['id']]);
            refresh_series((int)$s['id']);
            done(['on' => $on], $on ? 'Subscribed to ' . $s['title'] . '.' : 'Unsubscribed.');
        case 'like':
            $e = one('SELECT e.*, s.user_id, s.rating, s.hidden AS s_hidden, s.published FROM episodes e JOIN series s ON s.id = e.series_id WHERE e.id = ?', [$id('episode')]);
            ensure($e && episode_live($e) && series_access($u, ['user_id' => $e['user_id'], 'rating' => $e['rating'], 'hidden' => $e['s_hidden'], 'published' => $e['published']]) === 'ok', 'Episode not found.', 404);
            $on = !value('SELECT 1 FROM likes WHERE user_id = ? AND episode_id = ?', [$u['id'], $e['id']]);
            $on ? run('INSERT INTO likes VALUES(?, ?, ?)', [$u['id'], $e['id'], now()]) : run('DELETE FROM likes WHERE user_id = ? AND episode_id = ?', [$u['id'], $e['id']]);
            run('UPDATE episodes SET likes = (SELECT COUNT(*) FROM likes WHERE episode_id = ?) WHERE id = ?', [$e['id'], $e['id']]);
            done(['on' => $on, 'count' => compact_number((int)value('SELECT likes FROM episodes WHERE id = ?', [$e['id']]))]);
        case 'rate':
            $s = one('SELECT * FROM series WHERE id = ?', [$id('series')]);
            ensure($s && series_access($u, $s) === 'ok', 'Series not found.', 404);
            $score = $id('score');
            ensure($score >= 1 && $score <= 5, 'Choose 1 to 5 stars.');
            run('INSERT INTO ratings VALUES(?, ?, ?) ON CONFLICT(user_id, series_id) DO UPDATE SET score = excluded.score', [$u['id'], $s['id'], $score]);
            refresh_series((int)$s['id']);
            done(['score' => $score], 'Thanks for rating!');
        case 'comment':
            throttle('comment', 20, 600);
            $e = one('SELECT e.*, s.user_id, s.rating, s.hidden AS s_hidden, s.published FROM episodes e JOIN series s ON s.id = e.series_id WHERE e.id = ?', [$id('episode')]);
            ensure($e && episode_live($e) && series_access($u, ['user_id' => $e['user_id'], 'rating' => $e['rating'], 'hidden' => $e['s_hidden'], 'published' => $e['published']]) === 'ok', 'Episode not found.', 404);
            $body = post('body', 2000);
            ensure($body !== '', 'Write something first.');
            $parent = $id('parent') ?: null;
            if ($parent) ensure((bool)value('SELECT 1 FROM comments WHERE id = ? AND episode_id = ? AND parent_id IS NULL', [$parent, $e['id']]), 'That comment no longer exists.');
            $cid = insert('INSERT INTO comments(episode_id, user_id, parent_id, body, spoiler, created_at) VALUES(?, ?, ?, ?, ?, ?)', [$e['id'], $u['id'], $parent, $body, ($_POST['spoiler'] ?? '') === '1' ? 1 : 0, now()]);
            run('UPDATE episodes SET comments = (SELECT COUNT(*) FROM comments WHERE episode_id = ? AND hidden = 0) WHERE id = ?', [$e['id'], $e['id']]);
            redirect(strtok(back(), '#') . '#c' . $cid);
        case 'comment-like':
            $c = one('SELECT * FROM comments WHERE id = ? AND hidden = 0', [$id('comment')]);
            ensure($c !== null, 'Comment not found.', 404);
            $on = !value('SELECT 1 FROM comment_likes WHERE user_id = ? AND comment_id = ?', [$u['id'], $c['id']]);
            $on ? run('INSERT INTO comment_likes VALUES(?, ?)', [$u['id'], $c['id']]) : run('DELETE FROM comment_likes WHERE user_id = ? AND comment_id = ?', [$u['id'], $c['id']]);
            run('UPDATE comments SET likes = (SELECT COUNT(*) FROM comment_likes WHERE comment_id = ?) WHERE id = ?', [$c['id'], $c['id']]);
            done(['on' => $on, 'count' => compact_number((int)value('SELECT likes FROM comments WHERE id = ?', [$c['id']]))]);
        case 'comment-delete':
            $c = one('SELECT c.*, s.user_id AS owner FROM comments c JOIN episodes e ON e.id = c.episode_id JOIN series s ON s.id = e.series_id WHERE c.id = ?', [$id('comment')]);
            ensure($c && ((int)$c['user_id'] === (int)$u['id'] || (int)$c['owner'] === (int)$u['id'] || is_admin($u)), 'Comment not found.', 404);
            run('DELETE FROM comments WHERE id = ?', [$c['id']]);
            run('UPDATE episodes SET comments = (SELECT COUNT(*) FROM comments WHERE episode_id = ? AND hidden = 0) WHERE id = ?', [$c['episode_id'], $c['episode_id']]);
            redirect(strtok(back(), '#') . '#comments', 'Comment deleted.');
        case 'report':
            throttle('report', 10, 3600);
            $type = post('type', 10); $target = $id('id'); $reason = post('reason', 20);
            ensure(in_array($type, ['series', 'comment', 'user'], true) && isset(REPORT_REASONS[$reason]) && $target > 0, 'Invalid report.');
            run('INSERT INTO reports(user_id, target_type, target_id, reason, details, created_at) VALUES(?, ?, ?, ?, ?, ?)', [$u['id'], $type, $target, $reason, post('details', 1000), now()]);
            redirect(back(), 'Thanks. A moderator will review your report.');
        case 'mature':
            ensure(is_adult($u), 'Mature content is only available to readers 18 and older.', 403);
            run('UPDATE users SET show_mature = ? WHERE id = ?', [($_POST['on'] ?? '') === '1' ? 1 : 0, $u['id']]);
            redirect(back(), ($_POST['on'] ?? '') === '1' ? 'Mature content is on.' : 'Mature content is hidden.');
    }
    error_page(404, 'Unknown action.');
}

// ---------- Moderation ----------
function page_admin(array $u): void {
    ensure(is_admin($u), 'Not found.', 404);
    $open = all('SELECT r.*, u.username AS reporter FROM reports r LEFT JOIN users u ON u.id = r.user_id WHERE r.resolved_at IS NULL ORDER BY r.created_at DESC LIMIT 100');
    $b = '<div class="wrap stack"><div class="page-head"><h1>Moderation</h1><p class="muted">' . count($open) . ' open report' . (count($open) === 1 ? '' : 's') . '</p></div><section class="panel"><h2>Open reports</h2>';
    if (!$open) $b .= '<p class="muted">Nothing to review.</p>';
    else {
        $b .= '<ul class="report-list">';
        foreach ($open as $r) {
            $target = report_target($r);
            $b .= '<li><div><span class="badge' . ($r['reason'] === 'illegal' ? ' mature' : '') . '">' . h(REPORT_REASONS[$r['reason']] ?? $r['reason']) . '</span> <strong>' . h($r['target_type']) . '</strong> ' . $target['link']
                . '<p class="muted">by @' . h($r['reporter'] ?? 'deleted') . ' · ' . ago((int)$r['created_at']) . ($r['details'] ? ' · “' . h($r['details']) . '”' : '') . '</p>' . ($target['preview'] ? '<blockquote>' . h($target['preview']) . '</blockquote>' : '') . '</div>'
                . '<form method="post" action="/read/admin/resolve" class="report-actions">' . csrf_field() . '<input type="hidden" name="id" value="' . (int)$r['id'] . '">'
                . '<button class="btn small" name="do" value="dismiss">Dismiss</button><button class="btn small" name="do" value="hide">Hide content</button>'
                . ($r['target_type'] === 'series' ? '<button class="btn small" name="do" value="mature">Mark 18+</button>' : '') . '<button class="btn small danger" name="do" value="ban" data-confirm="Suspend this author?">Suspend author</button></form></li>';
        }
        $b .= '</ul>';
    }
    $series = all('SELECT s.*, u.username FROM series s JOIN users u ON u.id = s.user_id WHERE s.published = 1 ORDER BY s.created_at DESC LIMIT 30');
    $b .= '</section><section class="panel"><h2>Recent series</h2><ul class="report-list">';
    foreach ($series as $s) $b .= '<li><div><a href="/read/s/' . h($s['slug']) . '">' . h($s['title']) . '</a> ' . rating_badge($s['rating']) . ($s['hidden'] ? ' <span class="badge">Hidden</span>' : '') . '<p class="muted">@' . h($s['username']) . '</p></div>'
        . '<form method="post" action="/read/admin/feature" class="report-actions">' . csrf_field() . '<input type="hidden" name="id" value="' . (int)$s['id'] . '"><button class="btn small" type="submit">' . ($s['featured'] ? 'Unfeature' : 'Feature on home') . '</button></form></li>';
    layout('Moderation', $b . '</ul></section></div>', ['noindex' => true]);
}
function report_target(array $r): array {
    if ($r['target_type'] === 'series') { $s = one('SELECT slug, title FROM series WHERE id = ?', [$r['target_id']]); return ['link' => $s ? '<a href="/read/s/' . h($s['slug']) . '">' . h($s['title']) . '</a>' : '(deleted)', 'preview' => '']; }
    if ($r['target_type'] === 'comment') {
        $c = one('SELECT c.body, s.slug, e.number FROM comments c JOIN episodes e ON e.id = c.episode_id JOIN series s ON s.id = e.series_id WHERE c.id = ?', [$r['target_id']]);
        return ['link' => $c ? '<a href="/read/s/' . h($c['slug']) . '/' . (int)$c['number'] . '#c' . (int)$r['target_id'] . '">view</a>' : '(deleted)', 'preview' => $c ? mb_strimwidth($c['body'], 0, 300, '…') : ''];
    }
    $u = one('SELECT username FROM users WHERE id = ?', [$r['target_id']]);
    return ['link' => $u ? '<a href="/read/u/' . h($u['username']) . '">@' . h($u['username']) . '</a>' : '(deleted)', 'preview' => ''];
}
function do_admin(array $u, string $what): void {
    check_post();
    ensure(is_admin($u), 'Not found.', 404);
    if ($what === 'feature') { run('UPDATE series SET featured = 1 - featured WHERE id = ?', [(int)$_POST['id']]); redirect('/read/admin', 'Updated.'); }
    $r = one('SELECT * FROM reports WHERE id = ?', [(int)($_POST['id'] ?? 0)]);
    ensure($r !== null, 'Report not found.', 404);
    $do = (string)($_POST['do'] ?? '');
    $author = match ($r['target_type']) {
        'series' => value('SELECT user_id FROM series WHERE id = ?', [$r['target_id']]),
        'comment' => value('SELECT user_id FROM comments WHERE id = ?', [$r['target_id']]),
        default => $r['target_id'],
    };
    if ($do === 'hide' && $r['target_type'] === 'series') run('UPDATE series SET hidden = 1 WHERE id = ?', [$r['target_id']]);
    if ($do === 'hide' && $r['target_type'] === 'comment') {
        run('UPDATE comments SET hidden = 1 WHERE id = ?', [$r['target_id']]);
        run('UPDATE episodes SET comments = (SELECT COUNT(*) FROM comments c WHERE c.episode_id = episodes.id AND c.hidden = 0) WHERE id = (SELECT episode_id FROM comments WHERE id = ?)', [$r['target_id']]);
    }
    if ($do === 'mature' && $r['target_type'] === 'series') run("UPDATE series SET rating = 'mature' WHERE id = ?", [$r['target_id']]);
    if ($do === 'ban' && $author && (int)$author !== (int)$u['id']) { run('UPDATE users SET banned = 1 WHERE id = ?', [$author]); run('DELETE FROM sessions WHERE user_id = ?', [$author]); }
    run('UPDATE reports SET resolved_at = ?, resolved_by = ?, resolution = ? WHERE target_type = ? AND target_id = ? AND resolved_at IS NULL', [now(), $u['id'], $do, $r['target_type'], $r['target_id']]);
    redirect('/read/admin', 'Report resolved.');
}
