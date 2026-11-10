#!/usr/bin/env bash
# Install or update Scroll Comics (/read/) on the Contabo host. Run as root from an uploaded bundle:
#   /tmp/scroll-comics/read/                  (copy of website/src/read)
#   /tmp/scroll-comics/deploy/install-comics.sh, comics-pool.conf, nginx-read.conf
# Creates a new release from the active one plus /read/, adds one include to the existing vhost,
# validates, switches the `current` symlink, checks the live site, and rolls back on any failure.
# It does not touch other sites, certificates, firewall rules or the Assets (/studio/) pool.
set -Eeuo pipefail

HOST=${HOST:-scroll.project456.net}
SITE=${SITE:-/var/www/scroll-studio}
DATA=${DATA:-/var/lib/scroll-studio-web}
VHOST=${VHOST:-/etc/nginx/sites-available/scroll-studio}
SNIPPET=/etc/nginx/snippets/scroll-comics.conf
BUNDLE=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
STAMP=$(date -u +%Y%m%dT%H%M%SZ)
BACKUP=$SITE/deploy-backups/comics-$STAMP

say() { printf '\n== %s\n' "$*"; }
die() { printf '\nERROR: %s\n' "$*" >&2; exit 1; }

[ "$(id -u)" -eq 0 ] || die "Run as root."
[ -f "$BUNDLE/read/index.php" ] && [ -f "$BUNDLE/deploy/comics-pool.conf" ] && [ -f "$BUNDLE/deploy/nginx-read.conf" ] || die "Bundle incomplete in $BUNDLE."
[ -L "$SITE/current" ] || die "$SITE/current is not a symlink; this host does not match the documented layout."
[ -f "$VHOST" ] || die "Missing $VHOST. Set VHOST=/path/to/the/scroll-studio/site/config."
id scrollstudio >/dev/null 2>&1 || die "Missing scrollstudio user (the Assets pool user)."

PREVIOUS=$(readlink -f "$SITE/current")
NEW=$SITE/releases/$(basename "$PREVIOUS" | sed 's/-comics-[0-9TZ]*$//')-comics-$STAMP
mkdir -p "$BACKUP"
echo "$PREVIOUS" > "$BACKUP/previous-release"
cp -a "$VHOST" "$BACKUP/vhost"
SWITCHED=0
PHPV=""
POOL_TARGET=""
DONE=0

rollback() {
  printf '\n!! Deployment failed; rolling back.\n' >&2
  if [ "$SWITCHED" = 1 ]; then ln -sfn "$PREVIOUS" "$SITE/current.rollback" && mv -Tf "$SITE/current.rollback" "$SITE/current"; fi
  cp -a "$BACKUP/vhost" "$VHOST"
  if [ -n "$POOL_TARGET" ]; then
    if [ -f "$BACKUP/pool.conf" ]; then cp -a "$BACKUP/pool.conf" "$POOL_TARGET"; else rm -f "$POOL_TARGET"; fi
    "php-fpm$PHPV" -t 2>/dev/null && systemctl reload "php$PHPV-fpm" || true
  fi
  if nginx -t 2>/dev/null; then systemctl reload nginx; fi
  printf 'Restored %s and the previous vhost. Backup: %s\n' "$PREVIOUS" "$BACKUP" >&2
}
trap 'rc=$?; if [ $rc -ne 0 ] && [ "$DONE" != 1 ]; then rollback; fi' EXIT

say "PHP"
POOL_FILE=$(grep -l '^\[scroll-studio\]' /etc/php/*/fpm/pool.d/*.conf 2>/dev/null | head -1) || true
[ -n "$POOL_FILE" ] || die "Could not find the existing [scroll-studio] PHP-FPM pool."
PHPV=$(echo "$POOL_FILE" | sed -E 's#/etc/php/([0-9.]+)/.*#\1#')
echo "PHP-FPM $PHPV ($POOL_FILE)"
missing=()
for pair in pdo_sqlite:sqlite3 gd:gd zip:zip mbstring:mbstring intl:intl; do
  mod=${pair%%:*}; pkg=${pair##*:}
  "php$PHPV" -m | grep -qix "$mod" || missing+=("php$PHPV-$pkg")
done
if [ ${#missing[@]} -gt 0 ]; then
  echo "Installing ${missing[*]}"
  DEBIAN_FRONTEND=noninteractive apt-get install -y "${missing[@]}"
fi

say "Data directories"
install -d -o scrollstudio -g scrollstudio -m 0700 "$DATA/comics" "$DATA/comics-uploads"
ls -ld "$DATA/comics" "$DATA/comics-uploads"

say "PHP-FPM pool"
POOL_DIR=$(dirname "$POOL_FILE")
[ -f "$POOL_DIR/scroll-comics.conf" ] && cp -a "$POOL_DIR/scroll-comics.conf" "$BACKUP/pool.conf"
POOL_TARGET=$POOL_DIR/scroll-comics.conf
sed "s#/var/lib/scroll-studio-web#$DATA#g; s#/var/www/scroll-studio/#$SITE/#g" "$BUNDLE/deploy/comics-pool.conf" > "$POOL_DIR/scroll-comics.conf"
"php-fpm$PHPV" -t
systemctl reload "php$PHPV-fpm"
for _ in 1 2 3 4 5 6 7 8 9 10; do [ -S /run/php/scroll-comics.sock ] && break; sleep 0.5; done
[ -S /run/php/scroll-comics.sock ] || die "PHP-FPM did not create /run/php/scroll-comics.sock."

say "Release $NEW"
cp -a "$PREVIOUS" "$NEW"
rm -rf "$NEW/read"
cp -r "$BUNDLE/read" "$NEW/read"
chown -R root:root "$NEW/read"
find "$NEW/read" -type d -exec chmod 0755 {} +
find "$NEW/read" -type f -exec chmod 0644 {} +
if ! grep -q 'href="/read/"' "$NEW/index.html"; then
  sed -i 's#<a href="\#studio">Explore</a>#<a href="/read/">Read</a><a href="\#studio">Explore</a>#' "$NEW/index.html"
  grep -q 'href="/read/"' "$NEW/index.html" || echo "Note: could not add the Read link to the home page navigation."
fi
for f in index.php _lib/core.php _lib/media.php _lib/views.php _lib/reader.php _lib/account.php _lib/creator.php assets/read.css assets/read.js; do
  [ -f "$NEW/read/$f" ] || die "Release is missing read/$f"
done
"php$PHPV" -l "$NEW/read/index.php" >/dev/null

say "Nginx"
install -m 0644 "$BUNDLE/deploy/nginx-read.conf" "$SNIPPET"
if ! grep -qE "$SNIPPET|scroll-comics.sock" "$VHOST"; then
  # Insert before the first `location / {` inside the HTTPS server block.
  awk -v inc="    include $SNIPPET;" '
    /listen[ \t]+443/ { tls = 1 }
    tls && !done && /^[ \t]*location \/ \{/ { print inc; done = 1 }
    { print }
    END { if (!done) exit 3 }' "$BACKUP/vhost" > "$VHOST.new" || { rm -f "$VHOST.new"; die "Could not find 'location / {' in the HTTPS server block of $VHOST."; }
  mv "$VHOST.new" "$VHOST"
fi
nginx -t

say "Switch"
ln -sfn "$NEW" "$SITE/current.next"
mv -Tf "$SITE/current.next" "$SITE/current"
SWITCHED=1
systemctl reload nginx
sleep 1

say "Checks"
check() { # path expected-status
  local got
  got=$(curl -s -o /dev/null -w '%{http_code}' --resolve "$HOST:443:127.0.0.1" "https://$HOST$1")
  printf '%-28s %s (want %s)\n' "$1" "$got" "$2"
  [ "$got" = "$2" ]
}
check / 200
check /read/ 200
check /read/register 200
check /read/assets/read.css 200
check /read/_lib/core.php 404
check /read/media/covers/000000000000000000000000.jpg 404
check /studio/ 200
runuser -u scrollstudio -- test -w "$DATA/comics" || die "scrollstudio cannot write $DATA/comics"
[ -f "$DATA/comics/comics.sqlite" ] || die "The database was not created."

DONE=1
say "Done"
echo "Live: https://$HOST/read/"
echo "Previous release kept for rollback: $PREVIOUS"
echo "Backup of vhost and pool: $BACKUP"
echo "Rollback: ln -sfn $PREVIOUS $SITE/current && cp -a $BACKUP/vhost $VHOST && nginx -t && systemctl reload nginx"
echo "Register your own account now: the first account becomes the admin."
