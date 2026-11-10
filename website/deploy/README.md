# Contabo deployment

The download pages use Nginx. The new `/studio/` Assets section uses a dedicated PHP 8.3 FPM pool and a small browser interface; the native desktop stays Rust. The private repository is never a document root. The existing Sites preview remains separate.

## Live deployment

- Public address: [scroll.project456.net](https://scroll.project456.net).
- Squarespace DNS: an A record named `scroll` points to the owner's Contabo IPv4 address. Root-domain, `www` and mail records stay independent.
- Active release: `v0.13.1-b44c08575e8f`, deployed November 3, 2026, including `/studio/` and Windows 0.13.1 downloads.
- Virtual host: `/etc/nginx/sites-available/scroll-studio`, linked from `sites-enabled`; it is not the default host.
- Certificate: `/etc/letsencrypt/live/scroll.project456.net/`, renewed through the existing `certbot.timer` with a stored Nginx validation/reload hook.
- ACME webroot: `/var/www/scroll-studio/acme`, available over HTTP even while ordinary requests redirect to HTTPS.

The public home page, redirect, real 404 response and both complete download hashes were verified. Existing Nginx site configurations remained unchanged.

## Package

From the app repository root, run `node website/deploy/package.mjs` after building and checking the website. It produces an allowlisted archive under `target/contabo-deploy` with the verified installer, portable ZIP, public page assets and a `SHA256SUMS` manifest. Credentials, private stories, Git metadata and the Sites identity are excluded.

## Release layout

Render the dedicated virtual host with `node website/deploy/render-nginx.mjs <hostname> http`. Once the certificate exists, use `https` as the final argument. Rendered files stay under the ignored `target/contabo-deploy` directory. The generator rejects schemes, paths and invalid hostnames; it does not install or enable a configuration.

- `/var/www/scroll-studio/releases/<release-id>`: immutable, verified website releases (including the PHP section).
- `/var/www/scroll-studio/current`: symlink to the active release.
- A dedicated Nginx virtual host for the owner-provided domain; do not replace another site's default host.
- HTTPS uses the server's existing Certbot installation after DNS points to the server.

Before enabling a release, verify the archive hash, inspect its member paths, extract into a new directory and verify every file using `sha256sum -c SHA256SUMS`. Set files to 0644 and directories to 0755. No web-process write permission is needed. Back up the prior site config and symlink target, validate with `nginx -t`, and reload Nginx gracefully. Do not restart unrelated services or change unrelated firewall rules.

After activation, check the home page, real 404 response, HTTPS redirect, response headers, both download hashes and the pre-existing sites. Rollback changes the `current` symlink to the previous release and restores only the dedicated Scroll Studio config, followed by validation/reload. Retain old releases until the new one is verified.

Root passwords are supplied only to SSH authentication in memory. They do not belong in scripts, repository files, deployment metadata, logs or shell command arguments.

For this hostname, render the final configuration with `node website/deploy/render-nginx.mjs scroll.project456.net https`. Do not enable HTTPS mode until its certificate exists. Initial issuance uses the existing Certbot account and `certonly --webroot -w /var/www/scroll-studio/acme -d scroll.project456.net --cert-name scroll.project456.net`, with `--deploy-hook 'nginx -t && systemctl reload nginx'` stored for later renewals. Check renewal using `certbot renew --cert-name scroll.project456.net --dry-run --no-random-sleep-on-renew --run-deploy-hooks`. These commands affect only this certificate; do not replace unrelated site configurations.

References: [Nginx static files](https://docs.nginx.com/nginx/admin-guide/web-server/serving-static-content/), [Nginx graceful reload](https://nginx.org/en/docs/control.html).

## Assets release — November 3, 2026

`v0.13.1-b44c08575e8f` is live at `https://scroll.project456.net/studio/`. The prior release remains available for rollback. The dedicated host configuration is backed up in `/var/www/scroll-studio/deploy-backups/v0.13.1-b44c08575e8f`. The cleanup timer is enabled. Public HTTPS checks passed for import, editing, original-quality images, thumbnail persistence, export, cookie protections, source-file denial, redirects, and both complete download hashes. Files exported by the live PHP endpoint reopened in the desktop loader. The unrelated Playworth and Stardew host configuration hashes are unchanged.

The archive SHA-256 is `9c2ec9e0c97ebc75af83d3150a9f5a60ec85a3a1163507c0ebd20c8304b44b51`. The installer SHA-256 is `039b7d7b07bdf177802d23dd1ff9178cfc11b8a7063804200cf5d8a1542643c9`. Never substitute another archive without rendering a fresh package manifest and reviewing its paths.

### PHP configuration

`php-pool.conf` uses an unprivileged `scrollstudio` account, an on-demand pool capped at three workers, and private workspace, upload and session directories under `/var/lib/scroll-studio-web`. Only `index.php` and `api.php` are executable routes; runtime helpers and the template are not downloadable. The rendered host restricts writes per IP and limits concurrent API requests. Existing hosts and certificate configuration remain independent.

Install `cleanup.php` outside the document root at `/usr/local/lib/scroll-studio/cleanup.php`; the supplied service and hourly timer remove workspaces after 24 hours of inactivity. Cleanup can lag the expiry threshold by one timer interval. The private workspace quota is 4 GiB. Individual archives are capped at 512 MiB, original images at 64 MiB and 32 megapixels, and manifest JSON at 16 MiB. This is a temporary editor, not permanent cloud storage or account synchronization.

### Compatibility and verification

The PHP editor accepts native format versions 1–11, preserves chapter artwork/scripts and original image bytes, and exports format 11. Card thumbnail choices use the optional `card_thumbnails` list introduced in desktop 0.13.1. Earlier desktop builds can open the file but do not retain those choices on subsequent saves. No original reference is replaced or recompressed when choosing a cover.

Run `php` with ZIP and GD enabled for `website/tests/roundtrip.php`. With a local preview running, `node website/tests/api.mjs <origin>` tests session isolation, CSRF, revision conflicts, recovery copies, image upload/previews, immutable chapters, export and thumbnail persistence. `scripts/build.ps1 run --example verify_web_roundtrip` opens those exports with the actual desktop loader. All 350 desktop tests passed. Browser checks covered the empty workspace, phone-size sheets and editing, local `.scro` import, and basic lore creation before a saved browser permission block prevented the remaining visual checks. Do not describe the final thumbnail menu as visually verified until browser access is restored.

After activation, rerun the API checks against the live HTTPS origin with synthetic fixtures only. Verify the PHP source routes return 404, cookies are Secure/HttpOnly/SameSite, both full download hashes match, and the unrelated host configuration hashes remain unchanged. Preserve the previous release for rollback.

## Scroll Comics (`/read/`)

A public reading and publishing platform: accounts, series and episode pages, a vertical reader, likes, ratings, comments, subscriptions, reading history, daily schedule, ranking, search, a gated Mature 18+ section, reports and moderation, a creator dashboard, and a token API for publishing from the desktop app. It is plain PHP 8.3 with SQLite. There is no framework, Composer or build step.

### Deploy or update

From the repository root on a machine that can SSH to the server:

```sh
ssh root@<server> "rm -rf /tmp/scroll-comics && mkdir -p /tmp/scroll-comics"
scp -r website/src/read website/deploy root@<server>:/tmp/scroll-comics/
ssh root@<server> "bash /tmp/scroll-comics/deploy/install-comics.sh"
```

`install-comics.sh` installs missing PHP extensions (`sqlite3`, `gd`, `zip`, `mbstring`, `intl`), creates the data directories and the `scroll-comics` FPM pool, and copies the active release into a new release with `read/` added. It installs `nginx-read.conf` as `/etc/nginx/snippets/scroll-comics.conf` and adds one `include` before `location /` in the HTTPS server block. It then runs `nginx -t`, switches `current`, reloads, and checks `/`, `/read/`, `/studio/`, static assets and source-file denial. Any failure restores the previous release, vhost and pool. Re-running it is safe; each run creates a fresh release from the active one.

A config rendered fresh with `render-nginx.mjs` already contains the same block from `nginx-read.conf`, so the installer skips the include.

- **Data:** `/var/lib/scroll-studio-web/comics/comics.sqlite` (WAL mode), `media/` (original episode bytes; covers and thumbnails are resampled), and `secret.key`. Back up the whole directory. Releases never contain user data.
- **Media access:** every image request passes through PHP, which checks drafts, scheduling, moderation and Mature 18+ access. Nginx then sends the file via `X-Accel-Redirect`.
- **First account:** the first account registered becomes the admin. Register yours before announcing the page. Admins see **Moderation** in the account menu.
- **Limits:** 120 images per episode, 20 MB per image, 200 MB per episode, images at most 2000 × 20000 px. Upload a ZIP or select a Scroll Studio export folder; files are ordered by natural filename. JPEG EXIF/XMP metadata is stripped without re-encoding.
- **Rate limits:** sign-in (10 per 15 min), registration (5 per hour), comments (20 per 10 min), reports and API uploads per IP, plus the Nginx `scroll_write` zone.

### Mature content

Registration requires a date of birth (13 and older only). Mature 18+ series are excluded from every listing, search result, profile and image response unless the reader is signed in, is 18 or older, and has turned on mature content (prompted at the gate or in Settings). Series pages and the Mature section are `noindex`. Age is self-declared. If you need stronger verification for your jurisdiction, add a provider before promoting mature content.

### API for the desktop app

Create a token in **Settings → Publish from Scroll Studio**. Requests use `Authorization: Bearer sct_…`.

- `GET /read/api/me`: your account and series, with each series' next episode number.
- `GET /read/api/series/<slug>/episodes`: list episodes.
- `POST /read/api/series/<slug>/episodes`: multipart `images[]` (files or a ZIP), optional `number`, `title`, `note`, `status` (`draft` default, `published`, `scheduled` with `publish_at`), and `replace=1` to replace an existing episode number. Returns `201` with the episode URL.

```sh
curl -H "Authorization: Bearer $TOKEN" -F status=draft -F "images[]=@episode.zip" https://scroll.project456.net/read/api/series/my-series/episodes
```

### Local preview

```sh
SCROLL_COMICS=/tmp/comics-preview php website/tests/seed-comics.php
SCROLL_COMICS=/tmp/comics-preview php -S 127.0.0.1:8090 -t website/src website/tests/comics-router.php
```

Open `http://127.0.0.1:8090/read/`. The seed creates synthetic series with procedurally drawn art. Sign in as `moonharbor` (admin) or `reader_ana`, password `preview-password`. Never seed the production directory.
