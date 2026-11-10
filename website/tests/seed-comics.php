<?php
declare(strict_types=1);
// Local preview data only. Generates synthetic accounts, series and episode art with GD.
// SCROLL_COMICS=/tmp/comics-preview php website/tests/seed-comics.php
namespace Comics;

require __DIR__ . '/../src/read/_lib/core.php';
require __DIR__ . '/../src/read/_lib/media.php';
require __DIR__ . '/../src/read/_lib/views.php';
require __DIR__ . '/../src/read/_lib/creator.php';

$font = __DIR__ . '/../../assets/Karla.ttf';
ensure(is_file($font), 'Missing assets/Karla.ttf');
ensure(!value('SELECT 1 FROM users LIMIT 1'), 'The preview database already has data. Use an empty SCROLL_COMICS directory.');
mt_srand(456);

function rgb($img, array $c, int $a = 0) { return imagecolorallocatealpha($img, $c[0], $c[1], $c[2], $a); }
function mix(array $a, array $b, float $t): array { return [(int)($a[0] + ($b[0] - $a[0]) * $t), (int)($a[1] + ($b[1] - $a[1]) * $t), (int)($a[2] + ($b[2] - $a[2]) * $t)]; }
function gradient($img, int $x, int $y, int $w, int $h, array $top, array $bottom): void {
    for ($i = 0; $i < $h; $i++) imageline($img, $x, $y + $i, $x + $w - 1, $y + $i, rgb($img, mix($top, $bottom, $i / max(1, $h - 1))));
}
function scene($img, int $w, int $h, array $p, int $seed, string $font, ?string $line = null, string $title = ''): void {
    mt_srand($seed);
    gradient($img, 0, 0, $w, $h, $p[0], $p[1]);
    for ($i = 0; $i < 60; $i++) imagefilledellipse($img, mt_rand(0, $w), mt_rand(0, (int)($h * .55)), 3, 3, rgb($img, [255, 255, 255], mt_rand(30, 100)));
    $mx = mt_rand((int)($w * .2), (int)($w * .8)); $my = mt_rand((int)($h * .12), (int)($h * .32)); $mr = (int)(min($w, $h) * mt_rand(10, 22) / 100);
    for ($g = 4; $g >= 1; $g--) imagefilledellipse($img, $mx, $my, $mr * 2 + $g * 26, $mr * 2 + $g * 26, rgb($img, $p[2], 100));
    imagefilledellipse($img, $mx, $my, $mr * 2, $mr * 2, rgb($img, $p[2]));
    for ($layer = 0; $layer < 3; $layer++) {
        $base = (int)($h * (.58 + $layer * .12)); $col = mix($p[1], $p[3], .45 + $layer * .25); $pts = [0, $h];
        for ($x = 0; $x <= $w; $x += (int)($w / 8)) { $pts[] = $x; $pts[] = $base - mt_rand(0, (int)($h * .16)); }
        $pts[] = $w; $pts[] = $h;
        imagefilledpolygon($img, $pts, rgb($img, $col));
    }
    if ($seed % 3 !== 0) {
        $fx = mt_rand((int)($w * .25), (int)($w * .7)); $fy = (int)($h * .8); $s = $w / 800;
        $dark = rgb($img, $p[3]);
        imagefilledellipse($img, $fx, (int)($fy - 210 * $s), (int)(62 * $s), (int)(70 * $s), $dark);
        imagefilledpolygon($img, [$fx - (int)(46 * $s), $fy, $fx + (int)(46 * $s), $fy, $fx + (int)(30 * $s), (int)($fy - 175 * $s), $fx - (int)(30 * $s), (int)($fy - 175 * $s)], $dark);
        imagefilledpolygon($img, [$fx - (int)(30 * $s), (int)($fy - 170 * $s), $fx - (int)(110 * $s), (int)($fy - 40 * $s), $fx - (int)(90 * $s), (int)($fy - 30 * $s)], $dark);
    }
    if ($line) {
        $size = (int)(28 * $w / 800); $box = imagettfbbox($size, 0, $font, $line); $tw = $box[2] - $box[0];
        $bw = $tw + 70; $bh = $size + 60; $bx = mt_rand(30, max(31, $w - $bw - 30)); $by = mt_rand((int)($h * .38), (int)($h * .5));
        imagefilledellipse($img, $bx + (int)($bw / 2), $by + (int)($bh / 2), $bw + 8, $bh + 8, rgb($img, [20, 20, 24]));
        imagefilledellipse($img, $bx + (int)($bw / 2), $by + (int)($bh / 2), $bw, $bh, rgb($img, [250, 250, 248]));
        imagefilledpolygon($img, [$bx + (int)($bw * .4), $by + $bh - 6, $bx + (int)($bw * .55), $by + $bh - 6, $bx + (int)($bw * .38), $by + $bh + 34], rgb($img, [250, 250, 248]));
        imagettftext($img, $size, 0, $bx + 35, $by + (int)($bh / 2) + (int)($size / 2.6), rgb($img, [24, 24, 28]), $font, $line);
    }
    if ($title !== '') {
        $size = (int)(min($w, $h) / 9); $lines = explode("\n", wordwrap($title, 14, "\n"));
        foreach ($lines as $i => $l) {
            $y = (int)($h * .62) + $i * (int)($size * 1.15);
            imagettftext($img, $size, 0, (int)($w * .08) + 3, $y + 3, rgb($img, [0, 0, 0], 60), $font, $l);
            imagettftext($img, $size, 0, (int)($w * .08), $y, rgb($img, [255, 255, 255]), $font, $l);
        }
    }
}
function save_tmp($img, string $type = 'jpg'): string {
    $tmp = tempnam(sys_get_temp_dir(), 'seed');
    $type === 'png' ? imagepng($img, $tmp) : imagejpeg($img, $tmp, 88);
    imagedestroy($img);
    return $tmp;
}

$palettes = [
    [[18, 28, 58], [92, 66, 120], [250, 236, 200], [14, 14, 30]], [[244, 160, 120], [120, 70, 110], [255, 246, 220], [40, 22, 44]],
    [[20, 60, 70], [120, 180, 170], [240, 255, 250], [10, 30, 34]], [[60, 18, 24], [200, 80, 60], [255, 220, 170], [24, 8, 10]],
    [[30, 40, 30], [130, 160, 90], [250, 250, 210], [16, 24, 14]], [[230, 200, 230], [130, 120, 200], [255, 255, 255], [50, 40, 80]],
    [[10, 10, 14], [70, 30, 90], [230, 200, 255], [6, 4, 10]], [[200, 220, 245], [120, 160, 210], [255, 250, 230], [30, 50, 80]],
    [[40, 20, 10], [190, 120, 60], [255, 230, 180], [26, 14, 8]], [[24, 24, 30], [170, 50, 80], [255, 210, 220], [14, 10, 14]],
];
$creators = [
    ['moonharbor', 'Moon Harbor Studio', 'Quiet fantasy and long sea voyages.'], ['inkfox', 'Ink Fox', 'Action, swords and terrible puns.'],
    ['lumen_ella', 'Ella Lumen', 'Romance for the chronically online.'], ['kestrel', 'Kestrel', 'Horror that lives in the walls.'],
];
$lines = ['Did you hear that?', 'We leave at dawn.', 'This is not over.', 'Look up.', 'Who are you?', 'Run!', 'I remember now.', 'Stay close.', 'It\'s beautiful...', 'Again?!'];
$series = [
    ['The Quiet Citadel', 'A city that sleeps for a hundred years wakes up for one night.', 'fantasy', 'mystery', 'all', 'mon,thu', 0, 1],
    ['Saltwater Letters', 'Two lighthouse keepers. One bottle. Too many feelings.', 'romance', 'slice-of-life', 'all', 'wed', 2, 0],
    ['Blade of the Ninth Moon', 'The last moon-knight has nine nights to save a dying sky.', 'action', 'fantasy', 'teen', 'sat', 1, 1],
    ['Static Hours', 'Every radio in town plays the same voice at 3:33 a.m.', 'horror', 'thriller', 'teen', 'fri', 3, 1],
    ['Greenhouse Hearts', 'A botanist and a florist fight over the last rare orchid.', 'romance', 'comedy', 'all', 'tue,sat', 2, 0],
    ['Ashfall', 'After the volcano, the survivors build a new kind of city.', 'sci-fi', 'drama', 'teen', 'sun', 1, 0],
    ['Velvet After Dark', 'An exclusive club where every secret has a price.', 'drama', 'romance', 'mature', 'thu', 2, 0],
    ['Pocket Dragons', 'Tiny dragons, tiny problems, enormous chaos.', 'comedy', 'heartwarming', 'all', 'mon', 0, 0],
    ['The Hollow Choir', 'A choir that sings the dead back for one verse.', 'supernatural', 'horror', 'mature', 'wed', 3, 0],
    ['Kick Off!', 'A last-place team. A first-day coach. One season.', 'sports', 'comedy', 'all', 'fri', 1, 0],
];

$userIds = [];
foreach ($creators as $i => [$name, $display, $bio]) {
    $userIds[] = $id = register($name, "$name@example.test", 'preview-password', '1994-05-0' . ($i + 1), $display);
    run('UPDATE users SET bio = ?, show_mature = 1, support_url = ? WHERE id = ?', [$bio, $i === 0 ? 'https://ko-fi.com/' : '', $id]);
}
$readers = [];
foreach (['reader_ana', 'reader_bo', 'reader_cy', 'reader_dee', 'reader_eli', 'reader_fen'] as $i => $name)
    $readers[] = register($name, "$name@example.test", 'preview-password', '2001-02-1' . $i, ucfirst(substr($name, 7)));
$comments = ['This panel is gorgeous.', 'The moon in this episode!!', 'Okay I was NOT ready for that ending.', 'Waiting for next week like…', 'The colors are so good', 'Called it.', 'Who else re-read this three times?', 'Protect this character at all costs.'];

foreach ($series as $si => [$title, $tag, $g1, $g2, $rating, $days, $creator, $featured]) {
    $p = $palettes[$si % count($palettes)];
    $c = imagecreatetruecolor(600, 800); scene($c, 600, 800, $p, $si * 100, $font, null, $title); $coverTmp = save_tmp($c);
    $b = imagecreatetruecolor(1600, 600); scene($b, 1600, 600, $p, $si * 100 + 1, $font); $bannerTmp = save_tmp($b);
    $created = now() - (86400 * (60 - $si * 4));
    $sid = insert('INSERT INTO series(user_id, slug, title, tagline, synopsis, genre, genre2, rating, status, update_days, published, featured, cover, banner, created_at, updated_at) VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1, ?, ?, ?, ?, ?)', [
        $userIds[$creator], slugify($title), $title, $tag, "$tag\n\nA synthetic preview series generated for local testing. Every panel, cover and banner was drawn procedurally.",
        $g1, $g2, $rating, $si === 7 ? 'completed' : 'ongoing', $days, $featured, preview($coverTmp, 'covers', 600, 800), preview($bannerTmp, 'banners', 1600, 600), $created, $created]);
    unlink($coverTmp); unlink($bannerTmp);
    $count = 3 + $si % 4;
    for ($n = 1; $n <= $count; $n++) {
        $publish = $created + $n * 86400 * 3;
        $status = 'published';
        if ($n === $count && $si % 3 === 0) { $status = 'scheduled'; $publish = now() + 86400 * 2; }
        if ($publish > now() && $status === 'published') $publish = now() - mt_rand(3600, 86400);
        $eid = insert('INSERT INTO episodes(series_id, number, title, note, status, publish_at, created_at, updated_at) VALUES(?, ?, ?, ?, ?, ?, ?, ?)',
            [$sid, $n, ['The Lantern', 'Night Market', 'Old Promises', 'The Bridge', 'Low Tide', 'What the Moon Saw'][$n - 1] ?? "Chapter $n", $n === 1 ? 'Thank you for reading the first episode! New episodes every week.' : '', $status, $publish, $publish, $publish]);
        $sources = [];
        for ($k = 0; $k < 5; $k++) {
            $img = imagecreatetruecolor(800, 1280);
            scene($img, 800, 1280, $palettes[($si + ($k === 3 ? 1 : 0)) % count($palettes)], $si * 1000 + $n * 10 + $k, $font, $k % 2 === 0 ? $lines[($si + $n + $k) % count($lines)] : null);
            $sources[] = [save_tmp($img), sprintf('ep%02d_%03d.jpg', $n, $k + 1)];
        }
        save_episode_images($eid, $sources);
        foreach ($sources as [$tmp]) unlink($tmp);
        $first = value('SELECT file FROM episode_images WHERE episode_id = ? ORDER BY position LIMIT 1', [$eid]);
        run('UPDATE episodes SET thumb = ? WHERE id = ?', [preview(media_dir() . '/' . $first, 'thumbs', 320, 320, 'top'), $eid]);
        if ($status === 'published') {
            run('UPDATE episodes SET views = ? WHERE id = ?', [mt_rand(200, 40000) * (10 - $si), $eid]);
            foreach ($readers as $ri => $rid) {
                if (mt_rand(0, 2)) run('INSERT INTO likes VALUES(?, ?, ?)', [$rid, $eid, $publish]);
                if (mt_rand(0, 3) === 0) insert('INSERT INTO comments(episode_id, user_id, body, likes, created_at) VALUES(?, ?, ?, ?, ?)', [$eid, $rid, $comments[mt_rand(0, count($comments) - 1)], mt_rand(0, 40), $publish + mt_rand(60, 86400)]);
            }
            run('UPDATE episodes SET likes = (SELECT COUNT(*) FROM likes WHERE episode_id = :id) * 37, comments = (SELECT COUNT(*) FROM comments WHERE episode_id = :id) WHERE id = :id', ['id' => $eid]);
        }
    }
    foreach ($readers as $rid) {
        if (mt_rand(0, 1)) run('INSERT INTO subscriptions VALUES(?, ?, ?)', [$rid, $sid, now()]);
        run('INSERT INTO ratings VALUES(?, ?, ?)', [$rid, $sid, mt_rand(3, 5)]);
    }
    refresh_series($sid);
    run('UPDATE series SET subscribers = subscribers * ? WHERE id = ?', [mt_rand(150, 2400), $sid]);
}
echo "Seeded " . count($series) . " series. Sign in as moonharbor / preview-password (admin) or reader_ana / preview-password.\n";
