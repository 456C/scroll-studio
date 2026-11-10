<?php
declare(strict_types=1);
// Uploaded artwork keeps its original pixels and compression. Only previews (covers, thumbnails) are resampled.
namespace Comics;

const TYPES = [IMAGETYPE_JPEG => 'jpg', IMAGETYPE_PNG => 'png', IMAGETYPE_WEBP => 'webp'];

/** Normalize $_FILES[field] (single or multiple) into a list of [name, tmp, size]. */
function uploads(string $field): array {
    $f = $_FILES[$field] ?? null;
    if (!is_array($f)) return [];
    $list = [];
    if (is_array($f['name'])) {
        foreach ($f['name'] as $i => $name) {
            if ($f['error'][$i] === UPLOAD_ERR_NO_FILE) continue;
            ensure($f['error'][$i] === UPLOAD_ERR_OK && is_uploaded_file($f['tmp_name'][$i]), 'An upload failed or exceeds the file limit.');
            $list[] = ['name' => (string)$name, 'tmp' => $f['tmp_name'][$i], 'size' => (int)$f['size'][$i]];
        }
    } elseif ($f['error'] !== UPLOAD_ERR_NO_FILE) {
        ensure($f['error'] === UPLOAD_ERR_OK && is_uploaded_file($f['tmp_name']), 'The upload failed or exceeds the file limit.');
        $list[] = ['name' => (string)$f['name'], 'tmp' => $f['tmp_name'], 'size' => (int)$f['size']];
    }
    return $list;
}

/** Validate an image file and return [ext, width, height]. */
function inspect(string $path, int $maxBytes = MAX_IMAGE_BYTES): array {
    $size = filesize($path);
    ensure($size > 0 && $size <= $maxBytes, 'Each image must be smaller than ' . round($maxBytes / 1e6) . ' MB.');
    $info = @getimagesize($path);
    ensure(is_array($info) && isset(TYPES[$info[2]]), 'Images must be PNG, JPEG or WebP.');
    [$w, $h] = $info;
    ensure($w > 0 && $h > 0 && $w <= MAX_IMAGE_WIDTH && $h <= MAX_IMAGE_HEIGHT, "Images must be at most " . MAX_IMAGE_WIDTH . " px wide and " . MAX_IMAGE_HEIGHT . " px tall.");
    return [TYPES[$info[2]], $w, $h];
}

/** Remove EXIF/XMP (APP1) and comment segments from JPEG bytes without re-encoding. */
function strip_jpeg_metadata(string $b): string {
    if (substr($b, 0, 2) !== "\xFF\xD8") return $b;
    $out = "\xFF\xD8"; $i = 2; $n = strlen($b);
    while ($i + 4 <= $n && $b[$i] === "\xFF") {
        $marker = ord($b[$i + 1]);
        if ($marker === 0xDA) return $out . substr($b, $i); // start of scan: image data follows
        $len = unpack('n', substr($b, $i + 2, 2))[1];
        if ($len < 2 || $i + 2 + $len > $n) return $b;
        if ($marker !== 0xE1 && $marker !== 0xFE) $out .= substr($b, $i, 2 + $len);
        $i += 2 + $len;
    }
    return $b;
}

/** Store an original upload under media/<dir>/ and return its relative path. */
function store_original(string $tmp, string $dir, string $ext): string {
    $rel = $dir . '/' . bin2hex(random_bytes(12)) . '.' . $ext;
    $abs = media_dir() . '/' . $rel;
    if (!is_dir(dirname($abs))) mkdir(dirname($abs), 0700, true);
    $bytes = file_get_contents($tmp);
    if ($ext === 'jpg') $bytes = strip_jpeg_metadata($bytes);
    ensure(file_put_contents($abs, $bytes, LOCK_EX) === strlen($bytes), 'Could not save the image.', 503);
    return $rel;
}

function load_gd(string $path) {
    $data = file_get_contents($path);
    $img = @imagecreatefromstring($data);
    ensure($img !== false, 'Could not read that image.');
    return $img;
}

/** Cover-crop an image to w×h and save a JPEG (or WebP when supported) preview. */
function preview(string $sourcePath, string $dir, int $w, int $h, string $anchor = 'center'): string {
    [, $sw, $sh] = inspect($sourcePath, MAX_IMAGE_BYTES);
    $src = load_gd($sourcePath);
    $scale = max($w / $sw, $h / $sh);
    $cw = (int)round($w / $scale); $ch = (int)round($h / $scale);
    $cx = (int)(($sw - $cw) / 2);
    $cy = $anchor === 'top' ? 0 : (int)(($sh - $ch) / 2);
    $dst = imagecreatetruecolor($w, $h);
    imagefill($dst, 0, 0, imagecolorallocate($dst, 32, 33, 36));
    imagecopyresampled($dst, $src, 0, 0, $cx, $cy, $w, $h, $cw, $ch);
    $rel = $dir . '/' . bin2hex(random_bytes(12)) . '.jpg';
    $abs = media_dir() . '/' . $rel;
    if (!is_dir(dirname($abs))) mkdir(dirname($abs), 0700, true);
    imageinterlace($dst, true);
    ensure(imagejpeg($dst, $abs, 86), 'Could not save the preview.', 503);
    imagedestroy($src); imagedestroy($dst);
    return $rel;
}

/** Accept one uploaded image for a cover/banner/thumb/avatar and return the stored preview path. */
function upload_preview(string $field, string $dir, int $w, int $h, string $anchor = 'center'): ?string {
    $files = uploads($field);
    if (!$files) return null;
    inspect($files[0]['tmp']);
    return preview($files[0]['tmp'], $dir, $w, $h, $anchor);
}

function remove_media(?string $rel): void {
    if (!$rel || !preg_match('#^[a-z0-9/_-]+\.(jpg|png|webp)$#', $rel)) return;
    $abs = media_dir() . '/' . $rel;
    if (is_file($abs)) unlink($abs);
}

/**
 * Collect episode images from individual uploads and/or ZIP archives (such as a Scroll Studio export folder),
 * ordered naturally by filename. Returns list of [tmp path, name].
 */
function episode_sources(string $field, array &$cleanup): array {
    $sources = [];
    foreach (uploads($field) as $file) {
        $head = file_get_contents($file['tmp'], false, null, 0, 4);
        if ($head === "PK\x03\x04") {
            $zip = new \ZipArchive();
            ensure($zip->open($file['tmp'], \ZipArchive::RDONLY) === true, 'Could not open that ZIP archive.');
            for ($i = 0; $i < $zip->numFiles; $i++) {
                $stat = $zip->statIndex($i);
                $name = $stat['name'];
                if (str_ends_with($name, '/') || str_contains($name, '__MACOSX') || !preg_match('/\.(jpe?g|png|webp)$/i', $name)) continue;
                ensure($stat['size'] <= MAX_IMAGE_BYTES, basename($name) . ' is larger than ' . round(MAX_IMAGE_BYTES / 1e6) . ' MB.');
                ensure(count($sources) < MAX_EPISODE_IMAGES, 'An episode can have at most ' . MAX_EPISODE_IMAGES . ' images.');
                $tmp = tempnam(sys_get_temp_dir(), 'ep');
                $cleanup[] = $tmp;
                file_put_contents($tmp, $zip->getFromIndex($i));
                $sources[] = [$tmp, basename($name)];
            }
            $zip->close();
        } else {
            $sources[] = [$file['tmp'], $file['name']];
        }
    }
    usort($sources, fn($a, $b) => strnatcasecmp($a[1], $b[1]));
    ensure(count($sources) <= MAX_EPISODE_IMAGES, 'An episode can have at most ' . MAX_EPISODE_IMAGES . ' images.');
    return $sources;
}

/** Replace an episode's strip with the given sources. Returns stored image count. */
function save_episode_images(int $episodeId, array $sources): int {
    $total = 0; $rows = []; $width = null;
    foreach ($sources as [$tmp, $name]) {
        [$ext, $w, $h] = inspect($tmp);
        $total += filesize($tmp);
        ensure($total <= MAX_EPISODE_BYTES, 'An episode can be at most ' . round(MAX_EPISODE_BYTES / 1e6) . ' MB.');
        $rows[] = [$tmp, $ext, $w, $h];
    }
    $stored = [];
    foreach ($rows as $i => [$tmp, $ext, $w, $h]) {
        $stored[] = [$i, store_original($tmp, 'episodes/' . $episodeId, $ext), $w, $h, filesize($tmp)];
    }
    foreach (all('SELECT file FROM episode_images WHERE episode_id = ?', [$episodeId]) as $old) remove_media($old['file']);
    run('DELETE FROM episode_images WHERE episode_id = ?', [$episodeId]);
    foreach ($stored as [$pos, $file, $w, $h, $bytes])
        run('INSERT INTO episode_images(episode_id, position, file, width, height, bytes) VALUES(?, ?, ?, ?, ?, ?)', [$episodeId, $pos, $file, $w, $h, $bytes]);
    return count($stored);
}

/** Serve /read/media/<path> after checking the viewer may see the owning series. */
function serve_media(string $rel, ?array $viewer): never {
    ensure(preg_match('#^(covers|banners|thumbs|avatars|episodes/\d+)/[a-f0-9]{24}\.(jpg|png|webp)$#', $rel, $m) === 1, 'Not found.', 404);
    $abs = media_dir() . '/' . $rel;
    ensure(is_file($abs), 'Not found.', 404);
    $series = null;
    if (str_starts_with($rel, 'episodes/')) {
        $row = one('SELECT s.*, e.status AS e_status, e.publish_at AS e_publish_at, e.hidden AS e_hidden FROM episode_images i
            JOIN episodes e ON e.id = i.episode_id JOIN series s ON s.id = e.series_id WHERE i.file = ?', [$rel]);
        ensure($row !== null, 'Not found.', 404);
        $episodeLive = episode_live(['status' => $row['e_status'], 'publish_at' => $row['e_publish_at'], 'hidden' => $row['e_hidden']]);
        ensure($episodeLive || owns($viewer, $row) || is_admin($viewer), 'Not found.', 404);
        $series = $row;
    } elseif (in_array($m[1], ['covers', 'banners'], true)) {
        $series = one('SELECT * FROM series WHERE cover = ? OR banner = ?', [$rel, $rel]);
    } elseif ($m[1] === 'thumbs') {
        $series = one('SELECT s.* FROM episodes e JOIN series s ON s.id = e.series_id WHERE e.thumb = ?', [$rel]);
    }
    if ($series) ensure(series_access($viewer, $series) === 'ok', 'Not found.', 404);
    $types = ['jpg' => 'image/jpeg', 'png' => 'image/png', 'webp' => 'image/webp'];
    header('Content-Type: ' . $types[$m[2]]);
    header('X-Content-Type-Options: nosniff');
    header('Content-Security-Policy: default-src \'none\'');
    header('Cache-Control: ' . ($series && $series['rating'] === 'mature' ? 'private' : 'public') . ', max-age=604800, immutable');
    if (getenv('SCROLL_COMICS_ACCEL')) { header('X-Accel-Redirect: /_comics_media/' . $rel); exit; }
    header('Content-Length: ' . filesize($abs));
    readfile($abs);
    exit;
}
