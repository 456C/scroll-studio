# Contabo deployment

The public website is a static Nginx site. The desktop application and private source repository are not server processes. The existing Sites preview remains separate.

## Live deployment

- Public address: [scroll.project456.net](https://scroll.project456.net).
- Squarespace DNS: an A record named `scroll` points to the owner's Contabo IPv4 address. Root-domain, `www` and mail records stay independent.
- Active release: `v0.11.0-d48c377fbb9a`, deployed November 2, 2026.
- Virtual host: `/etc/nginx/sites-available/scroll-studio`, linked from `sites-enabled`; it is not the default host.
- Certificate: `/etc/letsencrypt/live/scroll.project456.net/`, renewed through the existing `certbot.timer` with a stored Nginx validation/reload hook.
- ACME webroot: `/var/www/scroll-studio/acme`, available over HTTP even while ordinary requests redirect to HTTPS.

The public home page, redirect, real 404 response and both complete download hashes were verified. Existing Nginx site configurations remained unchanged.

## Package

From the app repository root, run `node website/deploy/package.mjs` after building and checking the website. It produces an allowlisted archive under `target/contabo-deploy` with the verified installer, portable ZIP, public page assets and a `SHA256SUMS` manifest. Credentials, private stories, Git metadata and the Sites identity are excluded.

## Release layout

Render the dedicated virtual host with `node website/deploy/render-nginx.mjs <hostname> http`. Once the certificate exists, use `https` as the final argument. Rendered files stay under the ignored `target/contabo-deploy` directory. The generator rejects schemes, paths and invalid hostnames; it does not install or enable a configuration.

- `/var/www/scroll-studio/releases/<release-id>`: immutable, verified static releases.
- `/var/www/scroll-studio/current`: symlink to the active release.
- A dedicated Nginx virtual host for the owner-provided domain; do not replace another site's default host.
- HTTPS uses the server's existing Certbot installation after DNS points to the server.

Before enabling a release, verify the archive hash, inspect its member paths, extract into a new directory and verify every file using `sha256sum -c SHA256SUMS`. Set files to 0644 and directories to 0755. No web-process write permission is needed. Back up the prior site config and symlink target, validate with `nginx -t`, and reload Nginx gracefully. Do not restart unrelated services or change unrelated firewall rules.

After activation, check the home page, real 404 response, HTTPS redirect, response headers, both download hashes and the pre-existing sites. Rollback changes the `current` symlink to the previous release and restores only the dedicated Scroll Studio config, followed by validation/reload. Retain old releases until the new one is verified.

Root passwords are supplied only to SSH authentication in memory. They do not belong in scripts, repository files, deployment metadata, logs or shell command arguments.

For this hostname, render the final configuration with `node website/deploy/render-nginx.mjs scroll.project456.net https`. Do not enable HTTPS mode until its certificate exists. Initial issuance uses the existing Certbot account and `certonly --webroot -w /var/www/scroll-studio/acme -d scroll.project456.net --cert-name scroll.project456.net`, with `--deploy-hook 'nginx -t && systemctl reload nginx'` stored for later renewals. Check renewal using `certbot renew --cert-name scroll.project456.net --dry-run --no-random-sleep-on-renew --run-deploy-hooks`. These commands affect only this certificate; do not replace unrelated site configurations.

References: [Nginx static files](https://docs.nginx.com/nginx/admin-guide/web-server/serving-static-content/), [Nginx graceful reload](https://nginx.org/en/docs/control.html).
