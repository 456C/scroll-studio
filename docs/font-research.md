# Scroll Studio lettering research

Research checked 31 October 2026. These sources document creator choices, not a measured ranking of all comics on either platform. No official WEBTOON or Tapas font-usage leaderboard was found. Scroll Studio must not describe its bundled selection as the statistically proven three most popular fonts.

## What creators commonly mention

Anime Ace, CC Wild Words, and Digital Strip recur in [Tapas creators' font discussion](https://forums.tapas.io/t/which-fonts-do-you-use-for-your-comics/10212) and [a later discussion of lettering choices](https://forums.tapas.io/t/what-font-do-you-use-for-your-comic-and-why/69189). These are firsthand reports from individual artists, mostly from 2016–2022, rather than current platform-wide analytics. Creators also choose their own handwriting and less common fonts to suit a story's tone.

A [creator's WEBTOON CANVAS font comparison](https://www.webtoons.com/en/canvas/webtoon-test/font-anime-ace-sizes-test/viewer?episode_no=3&serviceZone=GLOBAL&title_no=602294) includes Anime Ace and Wild Words. This is evidence of creator use on WEBTOON, not an endorsement or popularity ranking from WEBTOON itself.

Those familiar fonts cannot simply be copied into an editor. [Blambot's license options](https://blambot.com/pages/license-options) distinguish creating artwork from embedding fonts in software and redistributing font files. [Comicraft's license FAQ](https://www.comicbookfonts.com/help_answer.php?ID=12) likewise requires app licensing to embed fonts for dynamic text. No such license was purchased for Scroll Studio.

## Three typography themes

The presets now target three distinct reading moods. The previous Comic Neue, Patrick Hand and Delius Unicase choices were too generic for the intended direction and are retained only for compatibility with existing work. The new choices were checked by rendering the same dialogue at the same size, rather than selecting from font names alone.

| Theme | Family | Design decision and evidence | Font bytes |
| --- | --- | --- | ---: |
| Mature | Lato Regular | Restrained, clean lettering for serious dialogue. The [Google Fonts family description](https://raw.githubusercontent.com/google/fonts/main/ofl/lato/DESCRIPTION.en_us.html) describes its balanced, serious structure and unobtrusive body-text design. | 656,568 |
| Casual | Balsamiq Sans Regular | Friendly lettering with rounded, steady strokes and readable lowercase. Its [designer explains](https://balsamiq.com/learn/balsamiq-sans/) the aim of a hand-drawn face that is cleaner and less distracting. | 407,528 |
| Webtoon Classic | Komika Hand Regular | Compact, inked capital-style comic lettering. [Komika Hand's distribution page](https://www.fontsquirrel.com/fonts/Komika-Hand) identifies it as a comic, all-caps family; [Tapas creators discuss using the Komika family](https://forums.tapas.io/t/what-font-do-you-use-for-your-comic-and-why/69189). | 81,452 |

These roles are Scroll Studio design choices, not a claim that each family is the most popular font in its genre. The first two are selected for mood and readability; Komika is a dedicated comic family. All font files remain unmodified. Changing the preset changes the lettering choice; it does not distort the glyphs to fit the bubble.

### Solo Leveling as a visual reference

The [official Yen Press / Ize Press page for Solo Leveling, Vol. 1](https://yenpress.com/titles/9781975319434-solo-leveling-vol-1-comic) credits letterer Abigail Blackman and provides a 32-page preview. Inspection of its dialogue pages shows compact capital lettering, centered lines, oval white balloons, and tails aimed at the speakers. Komika Hand is selected to evoke that general inked comic treatment.

The publisher page does **not** name its typeface. Scroll Studio does not claim Komika Hand is the exact Solo Leveling font. Other editions, languages, promotional art, system screens and unofficial translations can use different lettering. The title logo is also a separate design problem from readable dialogue.

## Provenance and redistribution

Lato and Balsamiq Sans were downloaded unmodified from the official Google Fonts repository. Their SIL Open Font License 1.1 permits bundling and embedding with software while retaining the copyright and license. Balsamiq's current font repository and Google Fonts distribute the OFL license; this is the license shipped with the font file.

Komika Hand was downloaded from Font Squirrel's Komika Hand archive as `Komika_Hand.ttf`; its bytes are unchanged in `assets/KomikaHand-Regular.ttf`. Font metadata credits **1999–2001 WolfBainX & Apostrophic Labs**. The original foundry site is no longer available; Font Squirrel preserves the foundry's Read Me as the [Apostrophic Labs license](https://www.fontsquirrel.com/license/Komika-Hand). This is a mirrored original license, not an OFL font. It permits as-is use in any context, restricts modifications for repackaging or re-release without permission, and prohibits selling the font design. The complete original notice is retained as `KomikaHand-LICENSE.txt`. The font is supplied unchanged with the app, not sold separately or relabeled as a proprietary Scroll Studio font.

Every font's license must remain in distributed application packages. The app's MIT license does not replace any font license. The Komika license's separate use restrictions also remain applicable; inspect its complete text before any later font modification or font resale.

| Family | Source font | Exact license |
| --- | --- | --- |
| Lato | [Google Fonts TTF](https://raw.githubusercontent.com/google/fonts/main/ofl/lato/Lato-Regular.ttf) | [OFL and copyright](https://raw.githubusercontent.com/google/fonts/main/ofl/lato/OFL.txt) |
| Balsamiq Sans | [Google Fonts TTF](https://raw.githubusercontent.com/google/fonts/main/ofl/balsamiqsans/BalsamiqSans-Regular.ttf) | [OFL and copyright](https://raw.githubusercontent.com/google/fonts/main/ofl/balsamiqsans/OFL.txt) |
| Komika Hand | [Font Squirrel original-family archive](https://www.fontsquirrel.com/fonts/download/Komika-Hand) | [Mirrored original foundry license](https://www.fontsquirrel.com/license/Komika-Hand) |

Downloaded SHA-256 fingerprints:

```text
Lato-Regular.ttf           d636e4683231f931eda222d588e944d082bfd3bdba02f928bee461c0f185b251
BalsamiqSans-Regular.ttf   fcc9c00a9ad6e78673fa84653745569097241d99b5d1387a23f395858ed46b7e
KomikaHand-Regular.ttf     f71235b688f92252702b8e0f602e3ae902107f68afe12dd369b42afe43ee858d
```

All three parse with the application's `ab_glyph` dependency; glyph outlines and positive advance widths were verified. Combined new font payload: 1,145,548 bytes, before executable compression. Missing curly quotes, dashes, or ellipses use their readable plain-text equivalents for display; the original dialogue remains unchanged. The same transformation is used for fitting, preview, and export. These families do not provide comprehensive world-script coverage.

## Earlier project fonts

Karla, Comic Neue, Patrick Hand and Delius Unicase are retained for old project appearance. Their original font files and OFL notices remain in `assets/`. They are not the new theme presets.

| Family | Source font | Exact license |
| --- | --- | --- |
| Comic Neue | [Google Fonts TTF](https://raw.githubusercontent.com/google/fonts/main/ofl/comicneue/ComicNeue-Regular.ttf) | [OFL and copyright](https://raw.githubusercontent.com/google/fonts/main/ofl/comicneue/OFL.txt) |
| Patrick Hand | [Google Fonts TTF](https://raw.githubusercontent.com/google/fonts/main/ofl/patrickhand/PatrickHand-Regular.ttf) | [OFL and copyright](https://raw.githubusercontent.com/google/fonts/main/ofl/patrickhand/OFL.txt) |
| Delius Unicase | [Google Fonts TTF](https://raw.githubusercontent.com/google/fonts/main/ofl/deliusunicase/DeliusUnicase-Regular.ttf) | [OFL and copyright](https://raw.githubusercontent.com/google/fonts/main/ofl/deliusunicase/OFL.txt) |
