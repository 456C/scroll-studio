from __future__ import annotations

from pathlib import Path

ASSETS = Path(__file__).resolve().parent / "demo_assets"
SERIES_ID = "paper-harbor"
COMIC_ID = "the-fold"
PROSE_ID = "margin-note"

PANELS = (
    {
        "file": "panel-fold.jpg",
        "alt": "A brass lamp, a blank cream sheet, and a wooden ruler on a night desk.",
        "caption": "The sheet stayed blank on purpose.",
        "line": "Mira said, “If I draw the harbor, it will only be a picture of leaving.”",
    },
    {
        "file": "panel-hull.jpg",
        "alt": "Two hands folding the cream sheet into a paper hull beside a lantern.",
        "caption": "She turned the near edge in toward the light.",
        "line": "“A hull does not need a coastline.”",
    },
    {
        "file": "panel-sill.jpg",
        "alt": "The finished paper boat on a dark windowsill, rain on the glass.",
        "caption": "Rain found the window first.",
        "line": "The boat waited on the sill, dry, as if it were listening for a tide.",
    },
    {
        "file": "panel-puddle.jpg",
        "alt": "The paper boat in a dawn puddle between stone walls, one window still lit.",
        "caption": "At dawn the alley held one finger of water.",
        "line": "She set the boat down and did not follow it.",
    },
)

PROSE = (
    "Mira kept a pencil in the margin and almost never used it. The harbor chart on her wall had been copied from a kinder year, when the channel was wider and the lamps along the mole were still counted. She did not correct it. A corrected chart would have asked her to agree that the water had changed its mind.",
    "On the night of the fold she cleared the desk until only the lamp, the ruler, and one square of paper remained. The ruler was for courage. She did not measure anything. She folded the first edge because a straight crease is a decision that does not require a name.",
    "The second fold made a hull. It was a poor hull, too proud at the bow, but it held its shape when she set it down. Outside, rain stitched the glass. She put the boat on the sill and ate a cold pear and did not open the window. Some departures are only rehearsals. They still count, because the hands remember the crease.",
    "In the morning the alley below her building held a thin sleeve of water between the stones. She carried the boat down the stairs in both palms. A neighbor’s window was already gold. Mira lowered the hull until the puddle took its weight. The paper drank a little and did not sink. She watched until the reflection of the lit window broke around the bow.",
    "She climbed back up without it. On the chart she made one mark, not a depth and not a road: a short line in the margin, the length of the crease she could still feel in her thumb. If anyone asked, she would say it was a reminder to buy ink. She would not say that the harbor had room for a boat that existed only until the water finished its small, serious work.",
)


def resolve(method, path):
    if path != "/" and not path.startswith("/ss-publish/demo"):
        return None
    if method != "GET":
        return ("html", 405, _page("Method", "<p>This demo is read only.</p>", section="Demo"))
    if path == "/":
        return ("html", 200, _home())
    if path == "/ss-publish/demo":
        return ("html", 200, _browse())
    if path == "/ss-publish/demo/paper-harbor":
        return ("html", 200, _series())
    if path == "/ss-publish/demo/paper-harbor/the-fold":
        return ("html", 200, _comic())
    if path == "/ss-publish/demo/paper-harbor/margin-note":
        return ("html", 200, _prose())
    prefix = "/ss-publish/demo/assets/"
    if path.startswith(prefix):
        name = path[len(prefix) :]
        allowed = {item["file"] for item in PANELS}
        asset = (ASSETS / name).resolve()
        if name not in allowed or asset.parent != ASSETS.resolve() or not asset.is_file():
            return ("html", 404, _page("Missing", "<p>That picture is not in the demo.</p>", section="Demo"))
        return ("file", 200, ("image/jpeg", asset.read_bytes()))
    return ("html", 404, _page("Missing", "<p>That page is not in the demo.</p>", section="Demo"))


def _home():
    body = """
    <h1>Read the sample comic and prose</h1>
    <p>Scroll Studio’s local reading prototype has one sample story on this machine. Open it when you want to read.</p>
    <p><a class="enter" href="/ss-publish/demo">Enter the synthetic demo</a></p>
    """
    return _page("Scroll Studio", body, section="Start", footer=_sign_in())


def _browse():
    cover = PANELS[0]
    body = f"""
    <h1>Sample stories</h1>
    <article class="card">
      <a href="/ss-publish/demo/paper-harbor"><img src="/ss-publish/demo/assets/{cover["file"]}" width="1152" height="864" alt="{_esc(cover["alt"])}"></a>
      <div class="card-body">
        <p class="kicker">Series</p>
        <h2><a href="/ss-publish/demo/paper-harbor">Paper Harbor</a></h2>
        <p>A short comic and a short prose piece.</p>
        <p><a class="read" href="/ss-publish/demo/paper-harbor/the-fold">Read the comic</a> <span class="kicker">The Fold</span></p>
        <p><a class="read" href="/ss-publish/demo/paper-harbor/margin-note">Read the prose</a> <span class="kicker">What the Margin Kept</span></p>
      </div>
    </article>
    """
    return _page("Sample stories", body, section="Demo")


def _series():
    comic = PANELS[0]
    body = f"""
    <p><a href="/ss-publish/demo">Sample stories</a></p>
    <h1>Paper Harbor</h1>
    <p>Mira keeps a chart she will not correct. One night she folds a boat instead.</p>
    <div class="cards">
      <article class="card">
        <a href="/ss-publish/demo/paper-harbor/the-fold"><img src="/ss-publish/demo/assets/{comic["file"]}" width="1152" height="864" alt="{_esc(comic["alt"])}"></a>
        <div class="card-body">
          <p class="kicker">Comic</p>
          <h2>The Fold</h2>
          <p><a class="read" href="/ss-publish/demo/paper-harbor/the-fold">Read the comic</a></p>
        </div>
      </article>
      <article class="card">
        <div class="card-body">
          <p class="kicker">Prose</p>
          <h2>What the Margin Kept</h2>
          <p>The same night, told in sentences.</p>
          <p><a class="read" href="/ss-publish/demo/paper-harbor/margin-note">Read the prose</a></p>
        </div>
      </article>
    </div>
    """
    return _page("Paper Harbor", body, section="Demo")


def _comic():
    figures = []
    for index, panel in enumerate(PANELS, start=1):
        figures.append(
            "<figure>"
            f'<img class="panel" src="/ss-publish/demo/assets/{panel["file"]}" width="1152" height="864" alt="{_esc(panel["alt"])}">'
            f"<figcaption><strong>{index}.</strong> {_esc(panel['caption'])}</figcaption>"
            f'<p class="speech">{_esc(panel["line"])}</p>'
            "</figure>"
        )
    body = (
        '<p><a href="/ss-publish/demo/paper-harbor">Paper Harbor</a></p>'
        "<h1>The Fold</h1>"
        "<p class=\"kicker\">Comic. Four pictures.</p>"
        + "".join(figures)
        + '<nav class="ep" aria-label="Episode"><a href="/ss-publish/demo/paper-harbor">Back to the series</a>'
        '<a href="/ss-publish/demo/paper-harbor/margin-note">Next: What the Margin Kept</a></nav>'
    )
    return _page("The Fold", body, section="Demo")


def _prose():
    paragraphs = "".join(f"<p>{_esc(part)}</p>" for part in PROSE)
    body = (
        '<p><a href="/ss-publish/demo/paper-harbor">Paper Harbor</a></p>'
        "<h1>What the Margin Kept</h1>"
        '<p class="kicker">Prose.</p>'
        f'<article class="reading">{paragraphs}</article>'
        '<nav class="ep" aria-label="Episode"><a href="/ss-publish/demo/paper-harbor/the-fold">Previous: The Fold</a>'
        '<a href="/ss-publish/demo/paper-harbor">Back to the series</a></nav>'
    )
    return _page("What the Margin Kept", body, section="Demo")


def _sign_in():
    return '<a href="/ss-publish/browser/series">Creator test sign-in</a>'


def _page(title, body, section, footer=""):
    notice = "Local prototype. Synthetic demo. Not a real publication."
    return f"""<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{_esc(title)} — Scroll Studio</title>
<style>
:root {{ color-scheme: light; --paper:#f3eee4; --ink:#241c16; --rule:#d7c8b2; --lamp:#8a3d22; --card:#fffdf8; }}
* {{ box-sizing: border-box; }}
body {{ margin:0; background:var(--paper); color:var(--ink); font:1.125rem/1.55 Georgia, "Iowan Old Style", Palatino, serif; }}
.skip {{ position:absolute; left:.75rem; top:.75rem; transform:translateY(-150%); background:var(--ink); color:var(--paper); padding:.45rem .7rem; text-decoration:none; font-family:"Segoe UI", "Avenir Next", sans-serif; }}
.skip:focus {{ transform:none; }}
a:focus-visible, .enter:focus-visible, .read:focus-visible {{ outline:3px solid var(--lamp); outline-offset:3px; }}
.wrap {{ width:min(46rem, calc(100% - 1.5rem)); margin:0 auto; padding:1.25rem 0 3rem; }}
.site, footer.site {{ display:flex; justify-content:space-between; gap:.75rem; align-items:baseline; flex-wrap:wrap; font-family:"Segoe UI", "Avenir Next", sans-serif; font-size:.82rem; letter-spacing:.06em; text-transform:uppercase; }}
.site a, footer.site a {{ color:inherit; }}
.mark {{ font-weight:650; letter-spacing:.02em; text-transform:none; font-size:1rem; }}
.banner {{ border:1px solid var(--rule); padding:.55rem .75rem; margin:1rem 0 0; font-family:"Segoe UI", "Avenir Next", sans-serif; font-size:.95rem; }}
h1 {{ font-weight:560; font-size:clamp(1.7rem, 5vw, 2.5rem); line-height:1.12; letter-spacing:-.02em; }}
h2 {{ font-weight:560; font-size:1.35rem; margin:.2rem 0; }}
a {{ color:var(--lamp); }}
.enter, .read {{ display:inline-block; margin-top:.35rem; padding:.7rem 1rem; background:var(--ink); color:var(--paper); text-decoration:none; font-family:"Segoe UI", "Avenir Next", sans-serif; }}
.kicker {{ margin:0; color:#5c5348; font-family:"Segoe UI", "Avenir Next", sans-serif; font-size:.85rem; letter-spacing:.06em; text-transform:uppercase; }}
.cards {{ display:grid; grid-template-columns:1fr; gap:1rem; }}
@media (min-width:40rem) {{ .cards {{ grid-template-columns:1fr 1fr; }} }}
.card {{ background:var(--card); border:1px solid var(--rule); overflow:hidden; }}
.card img {{ width:100%; max-width:100%; height:220px; object-fit:cover; object-position:center; display:block; }}
img.panel {{ width:100%; max-width:100%; height:auto; display:block; }}
.card-body {{ padding:.9rem 1rem 1.1rem; }}
figure {{ margin:0 0 1.75rem; }}
figcaption, .speech {{ font-family:"Segoe UI", "Avenir Next", sans-serif; }}
.speech {{ margin:.35rem 0 0; }}
.reading p {{ margin:0 0 1rem; }}
nav.ep {{ display:flex; justify-content:space-between; gap:1rem; flex-wrap:wrap; margin-top:2rem; font-family:"Segoe UI", "Avenir Next", sans-serif; }}
footer.site {{ margin-top:2.5rem; padding-top:1rem; border-top:1px solid var(--rule); }}
</style>
</head>
<body>
<a class="skip" href="#content">Skip to content</a>
<header class="site wrap"><a class="mark" href="/">Scroll Studio</a><span>{_esc(section)}</span></header>
<p class="banner wrap">{notice}</p>
<main id="content" class="wrap" tabindex="-1">
{body}
</main>
<footer class="site wrap">{footer}</footer>
</body>
</html>
"""


def _esc(value):
    return (
        str(value)
        .replace("&", "&amp;")
        .replace("<", "&lt;")
        .replace(">", "&gt;")
        .replace('"', "&quot;")
    )
