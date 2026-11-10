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
    <p class="banner">Local prototype. Synthetic demo. Not a publication.</p>
    <h1>A desk, a sheet of paper, and the harbor it refuses to draw.</h1>
    <p>The reading room on this machine holds one original story. It is not an account, not a catalogue of visitors, and not a claim that anything was published.</p>
    <p><a class="enter" href="/ss-publish/demo">Enter the synthetic demo</a></p>
    <p class="quiet">The fixture sign-in is separate and test-only. It does not open this story and it does not publish it. <a href="/ss-publish/browser/series">Fixture sign-in</a></p>
    """
    return _page("Local prototype", body, section="Start")


def _browse():
    body = """
    <p class="banner">Local prototype. Synthetic demo. Not a publication.</p>
    <h1>Browse</h1>
    <article class="card">
      <p class="kicker">One series</p>
      <h2><a href="/ss-publish/demo/paper-harbor">Paper Harbor</a></h2>
      <p>Two episodes. The first is a short comic. The second is a short prose piece. Neither is drawn from the fixture accounts.</p>
    </article>
    """
    return _page("Browse", body, section="Demo")


def _series():
    body = """
    <p class="banner">Local prototype. Synthetic demo. Not a publication.</p>
    <p><a href="/ss-publish/demo">Browse</a></p>
    <h1>Paper Harbor</h1>
    <p>Mira keeps a chart she will not correct. One night she folds a boat instead.</p>
    <ol class="episodes">
      <li><a href="/ss-publish/demo/paper-harbor/the-fold">1. The Fold</a> <span>Comic</span></li>
      <li><a href="/ss-publish/demo/paper-harbor/margin-note">2. What the Margin Kept</a> <span>Prose</span></li>
    </ol>
    """
    return _page("Paper Harbor", body, section="Demo")


def _comic():
    figures = []
    for index, panel in enumerate(PANELS, start=1):
        figures.append(
            "<figure>"
            f'<img class="panel" src="/ss-publish/demo/assets/{panel["file"]}" alt="{_esc(panel["alt"])}">'
            f"<figcaption><strong>{index}.</strong> {_esc(panel['caption'])}</figcaption>"
            f'<p class="speech">{_esc(panel["line"])}</p>'
            "</figure>"
        )
    body = (
        '<p class="banner">Local prototype. Synthetic demo. Not a publication.</p>'
        '<p><a href="/ss-publish/demo/paper-harbor">Paper Harbor</a></p>'
        "<h1>The Fold</h1>"
        "<p class=\"kicker\">Comic. Four pictures.</p>"
        + "".join(figures)
        + '<nav class="ep"><a href="/ss-publish/demo/paper-harbor">Back to the series</a>'
        '<a href="/ss-publish/demo/paper-harbor/margin-note">Next: What the Margin Kept</a></nav>'
    )
    return _page("The Fold", body, section="Demo")


def _prose():
    paragraphs = "".join(f"<p>{_esc(part)}</p>" for part in PROSE)
    body = (
        '<p class="banner">Local prototype. Synthetic demo. Not a publication.</p>'
        '<p><a href="/ss-publish/demo/paper-harbor">Paper Harbor</a></p>'
        "<h1>What the Margin Kept</h1>"
        '<p class="kicker">Prose.</p>'
        f'<article class="reading">{paragraphs}</article>'
        '<nav class="ep"><a href="/ss-publish/demo/paper-harbor/the-fold">Previous: The Fold</a>'
        '<a href="/ss-publish/demo/paper-harbor">Back to the series</a></nav>'
    )
    return _page("What the Margin Kept", body, section="Demo")


def _page(title, body, section):
    return f"""<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{_esc(title)}</title>
<style>
:root {{ color-scheme: light; --paper:#f4efe4; --ink:#221c16; --rule:#d5c7b2; --lamp:#8a3f22; --quiet:#5e564c; }}
* {{ box-sizing: border-box; }}
body {{ margin:0; background:var(--paper); color:var(--ink); font:1.125rem/1.55 Georgia, "Iowan Old Style", Palatino, "Palatino Linotype", serif; }}
.wrap {{ width:min(40rem, calc(100% - 2rem)); margin:0 auto; padding:1.25rem 0 3rem; }}
.site {{ display:flex; justify-content:space-between; gap:1rem; align-items:baseline; font-family:"Segoe UI", "Avenir Next", sans-serif; font-size:.8rem; letter-spacing:.08em; text-transform:uppercase; }}
.site a {{ color:inherit; text-decoration:none; }}
.banner {{ border:1px solid var(--rule); padding:.55rem .75rem; font-family:"Segoe UI", "Avenir Next", sans-serif; font-size:.95rem; }}
h1 {{ font-weight:500; font-size:clamp(1.7rem, 4vw, 2.4rem); line-height:1.15; letter-spacing:-.02em; }}
h2 {{ font-weight:500; }}
a {{ color:var(--lamp); }}
.enter {{ display:inline-block; margin-top:.25rem; padding:.7rem 1rem; background:var(--ink); color:var(--paper); text-decoration:none; font-family:"Segoe UI", "Avenir Next", sans-serif; }}
.quiet, .kicker {{ color:var(--quiet); font-family:"Segoe UI", "Avenir Next", sans-serif; }}
.card, .episodes {{ border-top:1px solid var(--rule); padding-top:1rem; }}
.episodes {{ padding-left:1.2rem; }}
.episodes li {{ margin:.7rem 0; }}
.episodes span {{ color:var(--quiet); font-family:"Segoe UI", "Avenir Next", sans-serif; font-size:.9rem; }}
figure {{ margin:0 0 1.75rem; }}
img.panel {{ width:100%; height:auto; display:block; }}
figcaption, .speech {{ font-family:"Segoe UI", "Avenir Next", sans-serif; }}
.speech {{ margin:.35rem 0 0; font-size:1.05rem; }}
.reading p {{ margin:0 0 1rem; }}
nav.ep {{ display:flex; justify-content:space-between; gap:1rem; flex-wrap:wrap; margin-top:2rem; font-family:"Segoe UI", "Avenir Next", sans-serif; }}
</style>
</head>
<body>
<main class="wrap">
<header class="site"><a href="/">Local prototype</a><span>{_esc(section)}</span></header>
{body}
</main>
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
