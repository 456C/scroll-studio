# Webtoon production workflow research

Reviewed November 1, 2026. These sources document working creators and official production tools. They do not establish representative market share, so this review does not claim that a particular program is universally “the most used.”

## Repeated workflow needs

| Evidence | Existing support | Useful next step |
| --- | --- | --- |
| Creator dogdog5500 [places dialogue before drawing](https://tips.clip-studio.com/en-us/articles/11509) to plan composition. CELSYS documents [paragraph-based story editing](https://tips.clip-studio.com/en-us/articles/2875). | Script beats, natural bubble fitting, story-wide bubble presets. | Review script paragraphs and turn selected dialogue into fitted bubbles in one operation. |
| CELSYS provides [smartphone-ratio webtoon previews](https://tips.clip-studio.com/en-us/articles/4001). Tapas recommends [checking mobile and desktop lettering](https://help.tapas.io/hc/en-us/articles/1260802028970-Series-Basics-How-to-publish-a-comic-episode-on-Tapas). | Platform export profiles, splitting, size checks, Fit and zoom. | A separate phone-width reader with editing overlays hidden. |
| WEBTOON Original creator 69Michi describes [reference views and reusable materials](https://tips.clip-studio.com/en-us/articles/4143); Nari describes [reference use for outfits and colors](https://tips.clip-studio.com/en-us/articles/10045). | Character sheets, reference images, palettes, prompt copying, drag/drop assets. | Keep references visible while composing; reuse clipped pieces across chapters. |
| CELSYS documents [inserting or removing vertical space](https://help.clip-studio.com/en-us/manual_en/210_file/Change_canvas_size.htm) in a long canvas. | Spacing review coordinates whole artwork, independent pointers and script ranges. | Evaluate pacing using the phone reader after applying a gap. |

## Implemented in this pass

**Make bubbles** opens a review of the selected script beat. Blank paragraphs form separate editable blocks; inclusion checkboxes let the creator omit directions or other prose. No speaker-name guessing or AI service is required. Existing presets supply appearance and natural fitting. The batch is staged and checked before a single undoable commit; it does not rewrite the script or invent bidirectional synchronization.

**Read** presents the chapter at a compact phone-like width or a wider reading width. It uses the existing layer renderer and visible-region preview cache rather than allocating one enormous episode image. Editing overlays are hidden. Returning restores the editor's zoom, selection and scroll position.

The gallery and location-folder work also supports reuse: character cards lead to fixed sheets, location groups organize background images, and their images remain inside the story archive. Context menus put appropriate actions at the item being worked on.

**Pin reference** adds a movable, resizable reference viewer. The creator's Sub View workflow above and [Procreate's Reference Companion](https://help.procreate.com/procreate/handbook/actions/actions-canvas) inform this choice: compare character art without switching away from the chapter. Existing story images share the same preview cache; viewing, zooming, panning and flipping do not change artwork or history. Character references can be cycled in place. Library drops replace the view without adding a layer; file drops import once and remain undoable. Pins are temporary workspace state and clear when the story changes.

## Lettering at reading size — 0.9.4

[WEBTOON CANVAS's own guide](https://tips.clip-studio.com/en-us/articles/4891) emphasizes checking text on a phone and explains that suitable font sizes vary with canvas dimensions, resolution, and typeface. [Tapas's episode publishing guide](https://help.tapas.io/hc/en-us/articles/1260802028970-Series-Basics-How-to-publish-a-comic-episode-on-Tapas) likewise asks creators to check lettering in both desktop and mobile previews. Neither source establishes a universal font size or a ranked list of the most popular fonts.

Native review exposed an internal mismatch: new stories used a 1600 px working canvas but 38 px presets intended for an 800 px canvas. At a 360 px reader width, the nominal text size fell from 17.1 to 8.55 px. New-story and newly added presets now scale font size and dimensional bubble settings together: 38 px at 800, 44.65 px at 940, and 76 px at 1600. These are Scroll Studio's starting choices, not platform requirements. The existing phone reader remains the place to judge the result with actual dialogue and artwork.

The implementation leaves loaded presets, existing layers, and legacy migration untouched. A duplicate preserves its source appearance; story-wide presets never change merely because a different chapter is selected. Rendering tests measure real glyph ink across the three starting formats, check natural fitting, and allow only small differences caused by pixel rounding. The dialogue review was also tightened without removing editable text, exclusion choices, or long-passage scrolling.

## Lettering proof beside its controls — 0.9.5

CELSYS's [webtoon feature guide](https://tips.clip-studio.com/en-us/articles/4001) describes smartphone-ratio canvas previews and a live preview on a companion device. The useful principle for Scroll Studio is to judge lettering at a stable reading scale while its controls remain accessible. The Bubbles workspace now has Phone and Fit views, with a preview chapter selector when a story contains several chapters. The desktop strip is 360 logical UI points wide; it does not emulate a specific physical phone. Long samples scroll rather than reducing the font, and horizontal overflow is visible. No new image is exported, no mobile connection is required, and viewing a proof does not edit the story.

## Follow-up candidates

- Save source-image-plus-mask cutouts as reusable materials. This requires storage/reference validation and is a larger change than the viewer and dialogue batch.

These are candidates for later passes, not claims that the current build already implements them.

## Pacing without moving every object — 0.9.6

CELSYS's [canvas-height documentation](https://help.clip-studio.com/en-us/manual_en/210_file/Change_canvas_size.htm) and [webtoon feature guide](https://tips.clip-studio.com/en-us/articles/4001) expose both numerical start/height settings and direct placement of the affected strip. They also describe different handling for raster artwork and whole objects such as image materials, text and balloons. Creator [69Michi's workflow](https://tips.clip-studio.com/en-us/articles/4143) connects panel spacing with reading pauses. These support adding a focused pacing operation rather than requiring repeated individual transforms.

Scroll Studio's Spacing review shows synchronized Before/After views. The cut can be placed directly or numerically, and scrolling either view pans both. Blue marks added space; orange marks removed space. Options exposes the crossing-object decision: keep position or move with lower artwork. This is a whole-object operation: it does not split an image into new raster layers or erase source pixels. Shortening the chapter can leave objects beyond its edge; the review counts newly clipped objects while keeping them editable.

Affected locked objects block Apply until explicitly included; their lock flags survive. Every enabled bubble pointer maps its absolute speaker target independently from its bubble body. Script ranges follow the change; a fully removed range retains its script and notes as a minimal range. The preview uses existing cached layer rendering, clones only chapter metadata, and stages changes until one undoable commit. Cancel and stale previews cannot mutate the story.

## Starting and continuing a script — 0.9.7

Creator dogdog5500's [episode workflow](https://tips.clip-studio.com/en-us/articles/11509) starts with an outline and places dialogue before character art to guide composition. Scroll Studio already supports script beats and a reviewed dialogue-to-bubble batch; the missing step was a direct action in an empty script lane.

The lane now has a bounded + Beat action, initially and after its final range. Hover/focus previews the proposed canvas span, and clicking scrolls to the new beat with its script editor focused. The next range leaves a gap after existing ranges. Its starting height and gap scale with chapter width as application defaults, not platform requirements. When more canvas is needed, a tooltip identifies the new height and the single undoable insertion extends blank canvas without moving artwork. Existing beat ranges and other chapters remain unchanged.

Only visible beat editors copy script, note and prompt strings into the editing frame. Offscreen ranges are inspected as lightweight metadata. Shorter placeholders and one initial title row expose more dialogue without removing notes or prompt controls. The refreshed built-in demo uses the existing reading-size lettering defaults and named shared presets; no existing saved file is rescaled.


## Crowded script lanes — 0.9.8

Apple's [disclosure-control guidance](https://developer.apple.com/design/human-interface-guidelines/disclosure-controls) recommends keeping frequent actions visible and revealing secondary detail when relevant. This supports keeping beat navigation above the editor and placing a title list behind its count. Grouping by overlapping canvas annotations is Scroll Studio's design choice, not a prescribed webtoon workflow.

Native inspection showed that eight short beats could overlap into a stack of headers. Nearby ranges now share one readable card, with previous/next controls and a virtualized title picker. Scrolling the canvas follows the relevant beat; an explicit selection lasts until canvas scrolling resumes. A connector still points to the selected beat's actual position. The card remains alongside its group while the group is visible, and its navigation stays outside the inner text scroll. Original ranges, note text, prompts, and artwork remain unchanged.

Layout uses cached IDs and numeric geometry, sorted independently of the saved chapter. Only selected visible editors clone text. Even a collapsed range keeps an editable card, and selecting or scrolling creates no undo checkpoint. Group metadata invalidates on revision, zoom, chapter, or document changes.


## Samples that demonstrate reading size — 0.9.9

[WEBTOON CANVAS's production guide](https://tips.clip-studio.com/en-us/articles/4891) emphasizes vertical reading and checking lettering on a small screen; it also cautions that font size depends on canvas dimensions and typeface. The old lettering playground was a 2400 px comparison grid with 42 px type, only 6.3 nominal pixels at a 360 px reading width. The old bubble playground used 38 px text on a 1600 px canvas, half the current starting size.

The refreshed lettering example gives Mature, Casual, and Classic their own 1600 px vertical chapter, using the same three passages at 76 px. The bubble example uses the same scale in two chapters that share speech, thought, and rounded presets. Short labels replace instructions painted onto the comic. Its targets demonstrate several independent pointers and negative offsets without changing the story-wide appearance model. The figures are application defaults, not new platform requirements or a ranking of font popularity.

The character example retains its detailed continuity, outfits, palettes, and style prompt, with authored Female/Male values and three story locations. The 800 px layout example starts from the matching canvas constructor so its new lettering presets stay proportional. The built-in After the rain story is now an additional packaged editable sample.

Release packaging regenerates all five stories into the build directory. Generation checks shared preset links, a minimum nominal reading size, text overflow, lettering overlap, chapter bounds, and exact story/asset round trips, then renders each chapter at 360 px for visual inspection. These checks concern the authored samples; they do not rescale or reject existing user stories. Only the editable archives are bundled, and older local examples and distributions are left intact.


## Lettering preview at display resolution — 0.9.10

The native comparison identified a difference between export rasterization and shrinking an existing lettering texture. Plain filtering lost fine outline segments; mipmaps reduced that problem but softened small type. The comparison also confirmed 150% native display scaling, so logical reading width alone was insufficient for a fair comparison. `examples/preview_comparison.rs` now compares direct export, display-size text, and the previous mipmapped path at the same physical scale and updates when native DPI initializes.

Text previews now use the exporter's glyph layout and bubble paths at the requested physical pixel scale. Raster padding is kept in the painted rectangle, and its origin snaps to the display pixel grid; the saved layer, line breaks, hit bounds, speaker targets, and export output stay unchanged. The same path serves the editor, reader, spacing previews and Bubbles proof.

The [egui_glow 0.31.1 shader](https://github.com/emilk/egui/blob/0.31.1/crates/egui_glow/src/shader/fragment.glsl) operates on gamma-space colors. Inspection of the installed backend and compositor showed that text texture upload also needed gamma-space premultiplication to agree with the exporter's alpha blend. This correction is restricted to text/bubble previews; the existing image upload path is unchanged. Tests compare every alpha value over light and dark surfaces and compare scaled text previews with exports for the three themes, several shapes and negative pointers.

Resolution is part of the bounded cache identity. While a new zoom is preparing, the closest available texture may be reused only when all appearance fields match. Edited words or pointers cannot borrow obsolete content. Work and result queues remain bounded; no synchronous rasterization or idle animation was added. Preview dimensions remain capped at 2048 px, and mip levels still count toward the existing 64 MB cache. Very large lettering that reaches this cap can still require upscaling. Subpixel positioning can produce small raster differences from a whole-chapter export.


## Transparent artwork and cutouts — 0.9.11

A native test story reproduced a visible preview/export difference: cyan artwork at half opacity appeared brighter and greener in both the editor and Phone reader. This affected image opacity, partially transparent PNGs, solid shapes and brush colors. The test places identical artwork over white and dark paper and includes an alpha ramp and a lasso-clipped transparent cutout; `examples/alpha_playground.rs` regenerates the editable fixture and three CPU export proofs.

The installed [egui Glow shader](https://github.com/emilk/egui/blob/0.31.1/crates/egui_glow/src/shader/fragment.glsl) multiplies and blends gamma-space values, while egui's ordinary [Color32 conversions](https://github.com/emilk/egui/blob/0.31.1/crates/ecolor/src/color32.rs) premultiply through linear color. Scroll Studio's exporter blends sRGBA directly. Artwork-specific conversion and opacity tinting now follow the exporter; UI theme and glass colors retain their existing behavior.

The [image 0.25.10 resize documentation](https://docs.rs/image/0.25.10/image/imageops/fn.resize.html) requires premultiplied input for varying alpha. Preview images now premultiply in place before Triangle resizing, preventing invisible RGB from entering cutout edges. Mask coverage then scales all premultiplied channels. Text also prepares its upload encoding on the worker. No extra full-size floating-point image, opacity-specific texture, dependency or idle render loop is introduced. Original encoded image bytes, stored masks and exports remain unchanged.

This corrects blending and hidden-color contamination, not every rasterization difference. GPU filtering and mipmaps, the 2048 px preview cap, and vector edge coverage can still differ from CPU export samples. Native review also identified flat stroke endpoints in the existing vector preview versus rounded exported endpoints; that geometry is a separate follow-up.


## Local selection and effects research — 0.10.0

[Microsoft Research: Region Filling and Object Removal by Exemplar-Based Image Inpainting](https://www.microsoft.com/en-us/research/publication/region-filling-and-object-removal-by-exemplar-based-image-inpainting/) explains filling missing areas from surrounding texture patches. [Adobe Research: PatchMatch](https://research.adobe.com/publication/patchmatch-a-randomized-correspondence-algorithm-for-structural-image-editing/) describes correspondence searches for structural editing. These informed an independently written, bounded local donor search; no research implementation, cloud service or trained model is bundled.

For Scroll Studio, the brush samples the visible composite and stores only a new transparent patch. Masked pixels are excluded from donors. Small local fills are the intended use; there is no claim of Photoshop feature parity. Smart character selection similarly uses local color/connectivity and provides manual additions and removals rather than promising universal subject recognition.

Source-linked snippets and split selections reference original image bytes. Raster patches represent newly generated pixels and necessarily add some archive data; snippets, glints, wind, action lines and ordinary crops do not duplicate source image data. Motion smears and fills have bounded working resolutions for predictable memory use. All layers have independent opacity.
