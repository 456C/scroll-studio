<?php
declare(strict_types=1);
// Comics platform core: storage, sessions, permissions. Never routed directly; only /read/index.php is executable.
namespace Comics;

use PDO;
use RuntimeException;

const SESSION_DAYS = 30;
const MAX_EPISODE_IMAGES = 120;
const MAX_IMAGE_BYTES = 20_000_000;
const MAX_IMAGE_WIDTH = 2000;
const MAX_IMAGE_HEIGHT = 20000;
const MAX_EPISODE_BYTES = 200_000_000;
const MIN_AGE = 13;
const ADULT_AGE = 18;
const GENRES = ['action' => 'Action', 'romance' => 'Romance', 'fantasy' => 'Fantasy', 'drama' => 'Drama', 'comedy' => 'Comedy',
    'slice-of-life' => 'Slice of life', 'thriller' => 'Thriller', 'horror' => 'Horror', 'sci-fi' => 'Sci-fi', 'supernatural' => 'Supernatural',
    'mystery' => 'Mystery', 'sports' => 'Sports', 'historical' => 'Historical', 'superhero' => 'Superhero', 'heartwarming' => 'Heartwarming',
    'bl' => 'BL', 'gl' => 'GL', 'informative' => 'Informative'];
const DAYS = ['mon' => 'Mon', 'tue' => 'Tue', 'wed' => 'Wed', 'thu' => 'Thu', 'fri' => 'Fri', 'sat' => 'Sat', 'sun' => 'Sun'];
const RATINGS = ['all' => 'All ages', 'teen' => 'Teen', 'mature' => 'Mature 18+'];
const STATUSES = ['ongoing' => 'Ongoing', 'hiatus' => 'Hiatus', 'completed' => 'Completed'];
const REPORT_REASONS = ['spam' => 'Spam or advertising', 'harassment' => 'Harassment or hate', 'unmarked' => 'Mature content not marked 18+',
    'illegal' => 'Illegal content or minors in sexual content', 'copyright' => 'Copyright infringement', 'other' => 'Something else'];

final class Problem extends RuntimeException {
    public function __construct(string $message, public int $status = 422) { parent::__construct($message); }
}
function ensure(bool $ok, string $message, int $status = 422): void { if (!$ok) throw new Problem($message, $status); }

function data_dir(): string {
    $dir = getenv('SCROLL_COMICS') ?: sys_get_temp_dir() . '/scroll-comics';
    if (!is_dir($dir) && !mkdir($dir, 0700, true) && !is_dir($dir)) throw new Problem('Storage unavailable.', 503);
    return $dir;
}
function media_dir(): string {
    $dir = data_dir() . '/media';
    if (!is_dir($dir)) mkdir($dir, 0700, true);
    return $dir;
}

function db(): PDO {
    static $pdo = null;
    if ($pdo) return $pdo;
    $pdo = new PDO('sqlite:' . data_dir() . '/comics.sqlite', null, null, [
        PDO::ATTR_ERRMODE => PDO::ERRMODE_EXCEPTION, PDO::ATTR_DEFAULT_FETCH_MODE => PDO::FETCH_ASSOC, PDO::ATTR_EMULATE_PREPARES => false]);
    $pdo->exec('PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000; PRAGMA synchronous=NORMAL;');
    migrate($pdo);
    return $pdo;
}

function migrate(PDO $pdo): void {
    $version = (int)$pdo->query('PRAGMA user_version')->fetchColumn();
    if ($version >= 1) return;
    $pdo->exec(<<<'SQL'
    CREATE TABLE users (
        id INTEGER PRIMARY KEY, username TEXT NOT NULL UNIQUE COLLATE NOCASE, email TEXT NOT NULL UNIQUE COLLATE NOCASE,
        pass TEXT NOT NULL, display_name TEXT NOT NULL, bio TEXT NOT NULL DEFAULT '', avatar TEXT, birthdate TEXT NOT NULL,
        show_mature INTEGER NOT NULL DEFAULT 0, role TEXT NOT NULL DEFAULT 'member', banned INTEGER NOT NULL DEFAULT 0,
        support_url TEXT NOT NULL DEFAULT '', created_at INTEGER NOT NULL);
    CREATE TABLE sessions (token TEXT PRIMARY KEY, user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        created_at INTEGER NOT NULL, expires_at INTEGER NOT NULL);
    CREATE TABLE api_tokens (id INTEGER PRIMARY KEY, user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        name TEXT NOT NULL, token TEXT NOT NULL UNIQUE, created_at INTEGER NOT NULL, last_used_at INTEGER);
    CREATE TABLE series (
        id INTEGER PRIMARY KEY, user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE, slug TEXT NOT NULL UNIQUE,
        title TEXT NOT NULL, tagline TEXT NOT NULL DEFAULT '', synopsis TEXT NOT NULL DEFAULT '', genre TEXT NOT NULL, genre2 TEXT,
        rating TEXT NOT NULL DEFAULT 'all', status TEXT NOT NULL DEFAULT 'ongoing', update_days TEXT NOT NULL DEFAULT '',
        cover TEXT, banner TEXT, support_url TEXT NOT NULL DEFAULT '', published INTEGER NOT NULL DEFAULT 0,
        hidden INTEGER NOT NULL DEFAULT 0, featured INTEGER NOT NULL DEFAULT 0, views INTEGER NOT NULL DEFAULT 0,
        subscribers INTEGER NOT NULL DEFAULT 0, rating_sum INTEGER NOT NULL DEFAULT 0, rating_count INTEGER NOT NULL DEFAULT 0,
        last_episode_at INTEGER, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL);
    CREATE INDEX series_user ON series(user_id);
    CREATE TABLE episodes (
        id INTEGER PRIMARY KEY, series_id INTEGER NOT NULL REFERENCES series(id) ON DELETE CASCADE, number INTEGER NOT NULL,
        title TEXT NOT NULL, thumb TEXT, note TEXT NOT NULL DEFAULT '', status TEXT NOT NULL DEFAULT 'draft', publish_at INTEGER,
        views INTEGER NOT NULL DEFAULT 0, likes INTEGER NOT NULL DEFAULT 0, comments INTEGER NOT NULL DEFAULT 0,
        hidden INTEGER NOT NULL DEFAULT 0, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, UNIQUE(series_id, number));
    CREATE INDEX episodes_publish ON episodes(status, publish_at);
    CREATE TABLE episode_images (id INTEGER PRIMARY KEY, episode_id INTEGER NOT NULL REFERENCES episodes(id) ON DELETE CASCADE,
        position INTEGER NOT NULL, file TEXT NOT NULL, width INTEGER NOT NULL, height INTEGER NOT NULL, bytes INTEGER NOT NULL);
    CREATE INDEX images_episode ON episode_images(episode_id, position);
    CREATE TABLE likes (user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE, episode_id INTEGER NOT NULL REFERENCES episodes(id) ON DELETE CASCADE,
        created_at INTEGER NOT NULL, PRIMARY KEY(user_id, episode_id));
    CREATE TABLE subscriptions (user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE, series_id INTEGER NOT NULL REFERENCES series(id) ON DELETE CASCADE,
        created_at INTEGER NOT NULL, PRIMARY KEY(user_id, series_id));
    CREATE TABLE ratings (user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE, series_id INTEGER NOT NULL REFERENCES series(id) ON DELETE CASCADE,
        score INTEGER NOT NULL CHECK(score BETWEEN 1 AND 5), PRIMARY KEY(user_id, series_id));
    CREATE TABLE comments (id INTEGER PRIMARY KEY, episode_id INTEGER NOT NULL REFERENCES episodes(id) ON DELETE CASCADE,
        user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE, parent_id INTEGER REFERENCES comments(id) ON DELETE CASCADE,
        body TEXT NOT NULL, spoiler INTEGER NOT NULL DEFAULT 0, likes INTEGER NOT NULL DEFAULT 0, hidden INTEGER NOT NULL DEFAULT 0,
        created_at INTEGER NOT NULL);
    CREATE INDEX comments_episode ON comments(episode_id, parent_id);
    CREATE TABLE comment_likes (user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE, comment_id INTEGER NOT NULL REFERENCES comments(id) ON DELETE CASCADE,
        PRIMARY KEY(user_id, comment_id));
    CREATE TABLE history (user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE, series_id INTEGER NOT NULL REFERENCES series(id) ON DELETE CASCADE,
        episode_id INTEGER NOT NULL REFERENCES episodes(id) ON DELETE CASCADE, read_at INTEGER NOT NULL, PRIMARY KEY(user_id, episode_id));
    CREATE INDEX history_recent ON history(user_id, read_at);
    CREATE TABLE view_log (episode_id INTEGER NOT NULL, viewer TEXT NOT NULL, day INTEGER NOT NULL, PRIMARY KEY(episode_id, viewer, day));
    CREATE TABLE reports (id INTEGER PRIMARY KEY, user_id INTEGER REFERENCES users(id) ON DELETE SET NULL, target_type TEXT NOT NULL,
        target_id INTEGER NOT NULL, reason TEXT NOT NULL, details TEXT NOT NULL DEFAULT '', created_at INTEGER NOT NULL,
        resolved_at INTEGER, resolved_by INTEGER, resolution TEXT);
    CREATE TABLE attempts (key TEXT NOT NULL, at INTEGER NOT NULL);
    CREATE INDEX attempts_key ON attempts(key, at);
    PRAGMA user_version = 1;
    SQL);
}

function now(): int { return time(); }
function one(string $sql, array $args = []): ?array { $s = db()->prepare($sql); $s->execute($args); return $s->fetch() ?: null; }
function all(string $sql, array $args = []): array { $s = db()->prepare($sql); $s->execute($args); return $s->fetchAll(); }
function value(string $sql, array $args = []): mixed { $s = db()->prepare($sql); $s->execute($args); return $s->fetchColumn(); }
function run(string $sql, array $args = []): int { $s = db()->prepare($sql); $s->execute($args); return $s->rowCount(); }
function insert(string $sql, array $args = []): int { run($sql, $args); return (int)db()->lastInsertId(); }
function tx(callable $fn): mixed {
    $pdo = db(); $pdo->beginTransaction();
    try { $r = $fn(); $pdo->commit(); return $r; } catch (\Throwable $e) { $pdo->rollBack(); throw $e; }
}

// ---------- Requests ----------
function secure(): bool { return ($_SERVER['HTTPS'] ?? '') !== '' && ($_SERVER['HTTPS'] ?? '') !== 'off'; }
function cookie(string $name, string $value, int $expires, bool $httpOnly = true): void {
    setcookie($name, $value, ['expires' => $expires, 'path' => '/read/', 'secure' => secure(), 'httponly' => $httpOnly, 'samesite' => 'Lax']);
}
function client_key(): string { return hash('sha256', ($_SERVER['REMOTE_ADDR'] ?? '') . '|' . secret()); }
function secret(): string {
    static $s = null;
    if ($s !== null) return $s;
    $file = data_dir() . '/secret.key';
    if (!is_file($file)) { file_put_contents($file, bin2hex(random_bytes(32)), LOCK_EX); chmod($file, 0600); }
    return $s = trim((string)file_get_contents($file));
}
function csrf(): string {
    static $token = null;
    if ($token) return $token;
    $token = $_COOKIE['sc_csrf'] ?? '';
    if (!preg_match('/^[a-f0-9]{64}$/', $token)) { $token = bin2hex(random_bytes(32)); cookie('sc_csrf', $token, now() + 86400 * 365); }
    return $token;
}
function check_post(): void {
    $origin = $_SERVER['HTTP_ORIGIN'] ?? '';
    if ($origin !== '') ensure(parse_url($origin, PHP_URL_HOST) === explode(':', $_SERVER['HTTP_HOST'] ?? '')[0], 'Cross-site requests are not allowed.', 403);
    $sent = (string)($_POST['csrf'] ?? $_SERVER['HTTP_X_CSRF_TOKEN'] ?? '');
    ensure(isset($_COOKIE['sc_csrf']) && hash_equals($_COOKIE['sc_csrf'], $sent), 'Your session changed. Refresh the page and try again.', 403);
}
function throttle(string $action, int $limit, int $window): void {
    $key = $action . ':' . client_key();
    run('DELETE FROM attempts WHERE at < ?', [now() - 86400]);
    $count = (int)value('SELECT COUNT(*) FROM attempts WHERE key = ? AND at > ?', [$key, now() - $window]);
    ensure($count < $limit, 'Too many attempts. Wait a few minutes and try again.', 429);
    run('INSERT INTO attempts(key, at) VALUES(?, ?)', [$key, now()]);
}
function post(string $key, int $max = 200): string {
    $v = $_POST[$key] ?? '';
    ensure(is_string($v), 'Invalid form field.');
    $v = trim(str_replace("\r\n", "\n", $v));
    ensure(mb_strlen($v) <= $max, 'A field is too long.');
    return $v;
}

// ---------- Accounts ----------
function user(): ?array {
    static $loaded = false, $user = null;
    if ($loaded) return $user;
    $loaded = true;
    $raw = $_COOKIE['sc_session'] ?? '';
    if (!preg_match('/^[a-f0-9]{64}$/', $raw)) return null;
    $user = one('SELECT u.* FROM sessions s JOIN users u ON u.id = s.user_id WHERE s.token = ? AND s.expires_at > ? AND u.banned = 0',
        [hash('sha256', $raw), now()]);
    return $user;
}
function require_user(): array {
    $u = user();
    if (!$u) { header('Location: /read/login?next=' . rawurlencode($_SERVER['REQUEST_URI'] ?? '/read/')); exit; }
    return $u;
}
function age(array $user): int {
    return (int)date_diff(date_create($user['birthdate']), date_create('today'))->y;
}
function is_adult(?array $user): bool { return $user !== null && age($user) >= ADULT_AGE; }
function sees_mature(?array $user): bool { return is_adult($user) && (int)$user['show_mature'] === 1; }
function is_admin(?array $user): bool { return $user !== null && $user['role'] === 'admin'; }
function start_session(int $userId): void {
    $raw = bin2hex(random_bytes(32));
    run('INSERT INTO sessions(token, user_id, created_at, expires_at) VALUES(?, ?, ?, ?)', [hash('sha256', $raw), $userId, now(), now() + SESSION_DAYS * 86400]);
    run('DELETE FROM sessions WHERE expires_at < ?', [now()]);
    cookie('sc_session', $raw, now() + SESSION_DAYS * 86400);
}
function end_session(): void {
    $raw = $_COOKIE['sc_session'] ?? '';
    if ($raw !== '') run('DELETE FROM sessions WHERE token = ?', [hash('sha256', $raw)]);
    cookie('sc_session', '', 1);
}
function register(string $username, string $email, string $password, string $birthdate, string $display): int {
    ensure(preg_match('/^[A-Za-z0-9_]{3,24}$/', $username) === 1, 'Usernames use 3–24 letters, numbers or underscores.');
    ensure(filter_var($email, FILTER_VALIDATE_EMAIL) !== false && strlen($email) <= 254, 'Enter a valid email address.');
    ensure(strlen($password) >= 10 && strlen($password) <= 200, 'Use a password with at least 10 characters.');
    $date = \DateTimeImmutable::createFromFormat('!Y-m-d', $birthdate);
    ensure($date !== false && $date->format('Y-m-d') === $birthdate && $date < new \DateTimeImmutable('today') && (int)$date->format('Y') > 1900, 'Enter your date of birth.');
    ensure(date_diff($date, date_create('today'))->y >= MIN_AGE, 'You must be at least ' . MIN_AGE . ' to create an account.');
    ensure(!value('SELECT 1 FROM users WHERE username = ?', [$username]), 'That username is taken.');
    ensure(!value('SELECT 1 FROM users WHERE email = ?', [$email]), 'An account already uses that email.');
    $display = $display !== '' ? $display : $username;
    $first = !value('SELECT 1 FROM users LIMIT 1');
    return insert('INSERT INTO users(username, email, pass, display_name, birthdate, role, created_at) VALUES(?, ?, ?, ?, ?, ?, ?)',
        [$username, $email, password_hash($password, PASSWORD_DEFAULT), $display, $birthdate, $first ? 'admin' : 'member', now()]);
}
function api_user(): array {
    $auth = $_SERVER['HTTP_AUTHORIZATION'] ?? $_SERVER['REDIRECT_HTTP_AUTHORIZATION'] ?? '';
    ensure(preg_match('/^Bearer (sct_[a-f0-9]{48})$/', $auth, $m) === 1, 'Missing or invalid API token.', 401);
    $row = one('SELECT u.*, t.id AS token_id FROM api_tokens t JOIN users u ON u.id = t.user_id WHERE t.token = ? AND u.banned = 0', [hash('sha256', $m[1])]);
    ensure($row !== null, 'Missing or invalid API token.', 401);
    run('UPDATE api_tokens SET last_used_at = ? WHERE id = ?', [now(), $row['token_id']]);
    return $row;
}

// ---------- Visibility ----------
/** SQL fragment limiting series to those the viewer may list. Alias must be `s`. */
function visible_series_sql(?array $viewer): string {
    $sql = 's.published = 1 AND s.hidden = 0 AND EXISTS(SELECT 1 FROM episodes e WHERE e.series_id = s.id AND ' . live_episode_sql('e') . ')';
    if (!sees_mature($viewer)) $sql .= " AND s.rating <> 'mature'";
    return $sql;
}
function live_episode_sql(string $alias = 'e'): string {
    return "$alias.hidden = 0 AND ($alias.status = 'published' OR ($alias.status = 'scheduled' AND $alias.publish_at <= " . now() . '))';
}
function owns(?array $viewer, array $series): bool { return $viewer !== null && (int)$viewer['id'] === (int)$series['user_id']; }
/** 'ok', 'login', 'underage', 'confirm' or 'hidden' */
function series_access(?array $viewer, array $series): string {
    if (owns($viewer, $series) || is_admin($viewer)) return 'ok';
    if ((int)$series['hidden'] === 1 || (int)$series['published'] === 0) return 'hidden';
    if ($series['rating'] !== 'mature') return 'ok';
    if ($viewer === null) return 'login';
    if (!is_adult($viewer)) return 'underage';
    return (int)$viewer['show_mature'] === 1 ? 'ok' : 'confirm';
}
function episode_live(array $e): bool {
    return (int)$e['hidden'] === 0 && ($e['status'] === 'published' || ($e['status'] === 'scheduled' && (int)$e['publish_at'] <= now()));
}
function slugify(string $title): string {
    $s = strtolower(trim(preg_replace('/[^A-Za-z0-9]+/', '-', iconv('UTF-8', 'ASCII//TRANSLIT//IGNORE', $title) ?: ''), '-'));
    $s = substr($s, 0, 60) ?: 'series';
    $base = $s; $i = 2;
    while (value('SELECT 1 FROM series WHERE slug = ?', [$s])) $s = $base . '-' . $i++;
    return $s;
}
function refresh_series(int $seriesId): void {
    run('UPDATE series SET
        subscribers = (SELECT COUNT(*) FROM subscriptions WHERE series_id = :id),
        rating_sum = (SELECT COALESCE(SUM(score), 0) FROM ratings WHERE series_id = :id),
        rating_count = (SELECT COUNT(*) FROM ratings WHERE series_id = :id),
        views = (SELECT COALESCE(SUM(views), 0) FROM episodes WHERE series_id = :id),
        last_episode_at = (SELECT MAX(COALESCE(publish_at, created_at)) FROM episodes e WHERE e.series_id = :id AND ' . live_episode_sql('e') . ')
        WHERE id = :id', ['id' => $seriesId]);
}
function record_view(array $episode, ?array $viewer): void {
    $who = $viewer ? 'u' . $viewer['id'] : client_key();
    $day = intdiv(now(), 86400);
    $added = run('INSERT OR IGNORE INTO view_log(episode_id, viewer, day) VALUES(?, ?, ?)', [$episode['id'], $who, $day]);
    if ($added) {
        run('UPDATE episodes SET views = views + 1 WHERE id = ?', [$episode['id']]);
        run('UPDATE series SET views = views + 1 WHERE id = ?', [$episode['series_id']]);
    }
    if ($viewer) run('INSERT INTO history(user_id, series_id, episode_id, read_at) VALUES(?, ?, ?, ?)
        ON CONFLICT(user_id, episode_id) DO UPDATE SET read_at = excluded.read_at', [$viewer['id'], $episode['series_id'], $episode['id'], now()]);
    if (random_int(1, 200) === 1) run('DELETE FROM view_log WHERE day < ?', [$day - 2]);
}
