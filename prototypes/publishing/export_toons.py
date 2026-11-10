from __future__ import annotations

import argparse
import shutil
from pathlib import Path

from prototypes.publishing.demo_site import ASSETS, PANELS, PROSE

THEME = Path(__file__).resolve().parent / "live_theme"
SITE_LINKS = (
    ("/", "Home"),
    ("/#studio", "Explore"),
    ("/studio/", "Assets"),
    ("/#download", "Download"),
)
APPROVED_SITE = {href for href, _label in SITE_LINKS} | {"/privacy.html"}
NOTICE = "Prototype. Synthetic sample. Not a real publication."


def export_toons(out: Path, folder: str = "toons") -> Path:
    if folder.startswith("/") or ".." in folder or "/" in folder:
        raise ValueError("folder")
    root = out / folder
    if root.exists():
        shutil.rmtree(root)
    assets = root / "assets"
    series = root / "paper-harbor"
    assets.mkdir(parents=True)
    series.mkdir()
    for name in ("Karla.ttf", "app-icon.png"):
        shutil.copyfile(THEME / name, assets / name)
    for panel in PANELS:
        shutil.copyfile(ASSETS / panel["file"], assets / panel["file"])
    (assets / "toons.css").write_text(_css(), encoding="utf-8")
    (root / "index.html").write_text(_home(), encoding="utf-8")
    (series / "index.html").write_text(_series(), encoding="utf-8")
    (series / "the-fold.html").write_text(_comic(), encoding="utf-8")
    (series / "margin-note.html").write_text(_prose(), encoding="utf-8")
    (out / "TOONS-NAV-PROPOSAL.txt").write_text(_proposal(), encoding="utf-8")
    return root


def _proposal() -> str:
    return """Observed live header on 2026-11-10 from https://scroll.project456.net/
Do not replace that page. Add one link inside the existing Main nav.

Current:
<nav aria-label="Main"><a href="#studio">Explore</a><a href="/studio/">Assets</a><a href="#download">Download <span aria-hidden="true">↗</span></a></nav>

Proposed:
<nav aria-label="Main"><a href="#studio">Explore</a><a href="/toons/">Toons</a><a href="/studio/">Assets</a><a href="#download">Download <span aria-hidden="true">↗</span></a></nav>

/toons/ returned HTTP 404 on that host at the time of the observation. Confirm the path is still unused before copying files.
"""


def _css() -> str:
    return """@font-face{font-family:Karla;src:url('Karla.ttf') format('truetype');font-display:optional}
*{box-sizing:border-box}
html{color-scheme:dark}
body{margin:0;background:#151719;color:#f0f1ef;font-family:Karla,system-ui,sans-serif}
a{color:inherit;text-decoration:none}
.skip{position:absolute;left:12px;top:-70px;background:#d0eadf;color:#172721;padding:12px;z-index:10}
.skip:focus{top:12px}
a:focus-visible{outline:2px solid #a5ead7;outline-offset:5px}
.wrap{width:min(1160px,calc(100% - 64px));margin-inline:auto}
.nav{min-height:100px;display:flex;align-items:center;justify-content:space-between;gap:16px;border-bottom:1px solid #ffffff12;flex-wrap:wrap}
.brand{display:flex;align-items:center;gap:12px;font-size:18px;font-weight:700}
.nav nav{display:flex;gap:28px;color:#b3b8ba;font-size:15px;flex-wrap:wrap}
.nav nav a:hover{color:#c0f3e3}
.eyebrow{font-size:11px;font-weight:600;letter-spacing:1.8px;color:#a9b2b4;margin:0 0 18px}
.status-dot{display:inline-block;width:6px;height:6px;border-radius:50%;background:#9bd7c7;margin-right:8px}
h1{font-size:clamp(40px,5vw,64px);line-height:1.05;letter-spacing:-2px;font-weight:500;margin:0 0 18px}
h1 em{font-style:normal;color:#a6dacb}
.intro{font-size:19px;line-height:1.65;color:#b5bbbd;max-width:38rem}
.button{display:inline-flex;align-items:center;min-height:54px;padding:17px 23px;border-radius:9px;font-weight:700;background:#c7eddf;color:#172721}
.notice{margin:28px 0;color:#a9b2b4}
.cards{display:grid;grid-template-columns:1fr;gap:18px}
.card{background:#202326;border:1px solid #ffffff12;border-radius:12px;overflow:hidden}
.card h2{margin:8px 0}
.card p,.card .kicker{color:#b5bbbd}
.card-body{padding:18px 20px 22px}
img{max-width:100%;height:auto;display:block}
.card img{width:100%;height:240px;object-fit:cover;object-position:center}
figure img{width:min(100%,1152px);aspect-ratio:1152/864;background:#202326}
figure{margin:0 0 28px}
figcaption,.speech{color:#d5dbdc}
.reading{max-width:40rem}
.reading p{font-size:18px;line-height:1.7;color:#e4e7e6}
.ep{display:flex;justify-content:space-between;gap:16px;flex-wrap:wrap;margin-top:28px}
footer.wrap{display:flex;justify-content:space-between;gap:16px;flex-wrap:wrap;padding:28px 0 48px;color:#b3b8ba;border-top:1px solid #ffffff12;margin-top:20px}
@media(max-width:660px){.wrap{width:calc(100% - 36px)}.nav{min-height:76px}.nav nav{gap:14px;font-size:13px}h1{letter-spacing:-1px}}
@media(prefers-reduced-motion:reduce){*{transition:none!important}}
"""


def _shell(title: str, body: str, depth: int) -> str:
    asset = "../" * depth + "assets/"
    home = "../" * depth + "index.html"
    nav = []
    for href, label in SITE_LINKS:
        nav.append(f'<a href="{href}">{label}</a>')
    return f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="theme-color" content="#151719">
<title>{_esc(title)} — Scroll Studio</title>
<link rel="preload" href="{asset}Karla.ttf" as="font" type="font/ttf" crossorigin>
<link rel="stylesheet" href="{asset}toons.css">
</head>
<body>
<a class="skip" href="#main">Skip to content</a>
<header class="nav wrap"><a class="brand" href="{home}"><img src="{asset}app-icon.png" width="28" height="28" alt=""><span>Scroll Studio</span></a><nav aria-label="Main">{''.join(nav)}</nav></header>
<main id="main" tabindex="-1"><div class="wrap">
<p class="notice">{NOTICE}</p>
{body}
</div></main>
<footer class="wrap"><span>Toons · synthetic sample</span><a href="/privacy.html">Privacy</a></footer>
</body>
</html>
"""


def _home() -> str:
    body = """
<p class="eyebrow"><span class="status-dot"></span> TOONS</p>
<h1>Read the sample comic and prose.</h1>
<p class="intro">A synthetic story in two parts. It is a sample on this website, not a release and not your file.</p>
<p><a class="button" href="paper-harbor/index.html">Enter the synthetic demo</a></p>
"""
    return _shell("Toons", body, 0)


def _series() -> str:
    cover = PANELS[0]
    body = f"""
<p class="eyebrow">SERIES</p>
<h1>Paper Harbor</h1>
<p class="intro">Mira keeps a chart she will not correct. One night she folds a boat instead.</p>
<div class="cards">
<article class="card"><a href="the-fold.html"><img src="../assets/{cover["file"]}" width="1152" height="864" alt="{_esc(cover["alt"])}"></a><div class="card-body"><p class="kicker">Comic</p><h2>The Fold</h2><p><a class="button" href="the-fold.html">Read the comic</a></p></div></article>
<article class="card"><div class="card-body"><p class="kicker">Prose</p><h2>What the Margin Kept</h2><p>The same night, told in sentences.</p><p><a class="button" href="margin-note.html">Read the prose</a></p></div></article>
</div>
<p><a href="../index.html">Back</a></p>
"""
    return _shell("Paper Harbor", body, 1)


def _comic() -> str:
    figures = []
    for index, panel in enumerate(PANELS, start=1):
        figures.append(
            "<figure>"
            f'<img src="../assets/{panel["file"]}" width="1152" height="864" alt="{_esc(panel["alt"])}">'
            f"<figcaption><strong>{index}.</strong> {_esc(panel['caption'])}</figcaption>"
            f'<p class="speech">{_esc(panel["line"])}</p></figure>'
        )
    body = (
        '<p class="eyebrow">COMIC</p><h1>The Fold</h1>'
        + "".join(figures)
        + '<nav class="ep" aria-label="Episode"><a href="index.html">Back to the series</a>'
        '<a href="margin-note.html">Next: What the Margin Kept</a></nav>'
    )
    return _shell("The Fold", body, 1)


def _prose() -> str:
    paragraphs = "".join(f"<p>{_esc(part)}</p>" for part in PROSE)
    body = (
        '<p class="eyebrow">PROSE</p><h1>What the Margin Kept</h1>'
        f'<article class="reading">{paragraphs}</article>'
        '<nav class="ep" aria-label="Episode"><a href="the-fold.html">Previous: The Fold</a>'
        '<a href="index.html">Back to the series</a></nav>'
    )
    return _shell("What the Margin Kept", body, 1)


def _esc(value: str) -> str:
    return (
        value.replace("&", "&amp;")
        .replace("<", "&lt;")
        .replace(">", "&gt;")
        .replace('"', "&quot;")
    )


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", required=True)
    args = parser.parse_args()
    root = export_toons(Path(args.out))
    print(root)


if __name__ == "__main__":
    main()
