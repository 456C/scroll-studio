# Scroll Studio download site

A buildless, responsive download page. Local fonts and illustration, accessible workflow tabs, no tracking, payment gate, external widgets or application backend.

## Build and review

Package the current Rust release first with `scripts/installer.ps1`, and the AI Lite addon with `scripts/package-ai-lite.ps1`. From this directory, run:

```sh
node build.mjs
node check.mjs
```

The build reads the app and addon versions and copies only that release's installer, portable ZIP, AI Lite ZIP and checksums into `dist/downloads`. The Addons page (`addons.html`) shows each addon's size, hardware and checksum from its package and `addon.json`. All downloads are independent of the private GitHub repository. Nothing in `dist` is a user story or working directory. The build refuses stale download files; move an old `website/dist` aside before building a different version.

Serve `dist` with a local static server for review. The primary download works without JavaScript. Workflow tabs support arrow keys, Home and End; FAQ and checksums use native disclosures. Motion respects the browser's reduced-motion preference.

## Public hosting

The public site is [scroll.project456.net](https://scroll.project456.net), hosted on the owner's Contabo server with Nginx. HTTPS uses Let's Encrypt with automatic Certbot renewal. See [deployment instructions](deploy/README.md) for packaging, activation, validation and rollback. The build itself does not deploy anything.

`wrangler.jsonc` is retained as an optional Cloudflare alternative. See [hosting research](../docs/hosting-research.md) for scaling options and the later upload, reader and payment architecture. The application repository stays private; public downloads are verified release files served directly from Contabo.

## Optional support

Activating an installer or portable download reveals a dismissible note from 456C with an optional [Ko-fi link](https://ko-fi.com/456th). The build shares the note between the home page and release notes. Downloads work directly with or without JavaScript.

## Updating

Package the new app, build into a fresh output directory, run checks, review desktop/mobile layout and publish the complete output together. Versioned downloads use immutable caching. Installer signing is not yet configured; the site states that the Windows installer is unsigned.

## Review record

The page was checked at desktop and mobile widths in the browser. All three workflow tabs, arrow-key switching and checksum disclosure worked; images loaded, the mobile document had no horizontal overflow and the console had no errors. A second pass changed mobile Timeline cards to natural height and kept the composition art behind its bubble. Local checks verify all four pages, local links and both release hashes.

The private Sites preview identity is retained in `.openai/hosting.json`. Its isolated deployment checkout is `target/site-preview` at the app repository root. It remains separate from the public Contabo site.
