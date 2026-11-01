# Interface research and review

Reviewed November 1, 2026. These are interaction references, not dependencies or copied component implementations. Scroll Studio implements its controls in Rust and egui.

| Reference | Useful pattern | Scroll Studio application |
| --- | --- | --- |
| [Bencho canvas toolbar](https://bencho.dev/blocks/toolbar) | A compact tool cluster with one clear selected state | Bound the floating shelf to 720 logical pixels, center it in the workspace, and show when Fit is active. |
| [Bencho copy field](https://bencho.dev/blocks/copy-field) | Acknowledgement at the control the user pressed | Text-copy controls briefly show “Copied” without shifting their width or neighboring controls. |
| [Wensity micro-interactions](https://ui.wensity.com/components/category/elite-micro-interactions) | Brief feedback on direct manipulation; stable image placeholders | Keep finite hover/press feedback and restrained selection surfaces. Artwork stays crisp and buttons retain fixed targets. |
| [Transitions.dev](https://transitions.dev/) | Short pane reveals, selection indicators, and success states | Use the existing reduced-motion-aware native transitions; schedule one wake-up to clear copy feedback. |
| [Kobra](https://kobra.systems/components/input-otp) | Separate form, navigation, overlay, and empty-state patterns | Treat empty collections as their own bounded layouts and remove controls that have no action in that state. |
| [Arc UI](https://uiarc.dev/) | Keyboard-aware everyday controls with quiet motion | Preserve native menu keyboard handling and clear control states as visual polish is added. |
| [Procreate Gallery](https://help.procreate.com/procreate/handbook/gallery/gallery-organize) | Thumbnail-first browsing with titles and a clear return path | Browse the cast as character cards, then open a focused sheet; preserve the gallery's scroll position on return. |

Also surveyed [beUI](https://beui.dev/) and [BoardUI](https://www.boardui.com/). Their dashboard/chart emphasis is less relevant to a focused comic canvas. No library installation, sign-in, or purchase is needed for this research.

## Review loop

1. Identify a concrete workflow or visual problem in the running editor.
2. Research a relevant interaction and adapt it to a native editing workflow.
3. Implement it without adding runtime dependencies or changing story data unnecessarily.
4. Check the actual native app at minimum and larger window sizes, empty and populated states.
5. Verify interaction boundaries, keyboard behavior, undo, and idle repaint behavior where affected.

## 0.9.1 passes

- Empty Characters, References, and Bubbles use compact vector illustrations and bounded actions. Blank workspace is inert. Regression coverage includes minimum size, large windows, and 150% display scaling.
- The canvas shelf stays compact at large window sizes. Fit tracks workspace resize, script visibility, and chapter width; manual zoom opts out until Fit is selected again. Narrow canvas/script groups center together.
- Prompt, story-style, and export-path copy controls acknowledge locally without growing or moving neighboring targets.
- Library thumbnails select across their complete cards with a visible selection state. Drag placement and double-click placement remain available; unavailable layer-order actions are disabled.

Visual inspiration is filtered through the editor's needs: no animated background, continuous decorative repaint, moving hit targets, or extra tutorial prose.

## Native review of 0.9.1

Checked the packaged Windows executable at 1100 × 720 and 1280 × 760 client sizes, then maximized on the current display. Empty Characters and References keep bounded actions; populated character navigation remains visible. Copy prompt shows local feedback with no control movement. Full-card padding selects the asset, dragging places one layer, and one undo removes that placement. The automated suite passed 177 tests, plus all-target Clippy with warnings denied.

## 0.9.2 passes

- A scrollable, virtualized character gallery leads into fixed sheets. Selection changes reset temporary scene prompts; each character retains its own reference/outfit view state.
- Location folders collect background images inside the story. Import, moving and renaming remain undoable. Format 6 protects this metadata from older writers while accepting earlier stories.
- The inspector presents compact visibility, locking and percentage opacity controls. Layer folders retain collapse/expand, with deeper actions in contextual menus.
- Working menus and inspector surfaces use the same Glass/Solid preference as the floating canvas shelf. Glass paints behind controls; it does not change their target geometry.
- Right-click menus target the clicked object, preserve text selections, and close when a virtualized source disappears. Layer rows use stable identities when selecting them changes the inspector layout.
- Script-to-bubble review validates the whole batch before changing the story. The reader retains its document position when changing width and preserves the editor view when returning. Unsaved-close prompts remain available in reading mode.

The full automated suite passes 233 tests (95 library and 138 app tests), including stable reference selection through removal and undo/redo. All-target Clippy with warnings denied passes. Native visual review of 0.9.2 is pending: Windows Computer Use reported a physical-Escape stop before the new build was opened. The native observations above apply to 0.9.1 only.

## 0.9.3 reference pass

The new reference window is bounded at minimum and larger supported window sizes. Automated real-frame checks cover image zoom/pan, window movement and resizing, contextual flip, library drops, file-drop import with one undo, source removal/ID reuse, and preserving chapter state beneath the viewer. Glass paints before the native controls; Glass and Solid retain identical window/image geometry. No image is duplicated merely to pin or inspect it.

All 240 tests (95 library and 145 app tests) pass, along with strict all-target Clippy. Native review found that the beat backdrop intercepted its child controls; a real-frame regression reproduced this before the fix and now verifies menu access and script editing.

The corrected packaged Windows build was reviewed at 1280 × 760 and 1100 × 720 client sizes. Character-card and sheet pin actions open the reference; it remains visible in the chapter. Native zoom and pan leave the chapter position unchanged, and glass visibly samples the backdrop in the frame and context menu. The title was reduced and the default position moved near the right edge after this review to keep the canvas clearer; the corrected placement was checked at both sizes.

The inherited dialogue workflow was also checked natively: the beat menu opens, a stage direction can be excluded, two bubbles are created, Read opens the phone view, Escape restores the editor, and one undo removes the full batch. This verifies those specific workflows, rather than every earlier 0.9.2 control. Temporary review stories were kept under `target/ui-qa-093`; user stories were not used.

## 0.9.4 lettering and preview pass

New presets scale with the starting canvas rather than using the same document-pixel size at every resolution. Existing stories and duplicate presets retain their saved appearance. Rendering tests measure glyph ink at 360 px reading width for all three themes and starting formats; additional checks cover setup/skip, undo, migration, and opening appearance tabs without clamping saved values.

Short dialogue reviews shrink to their contents. Three-line editors keep longer text internally scrollable, and long lists still virtualize their rows with the action footer outside the scrolling area. Native review at 1280 × 760 and 1100 × 720 verified the compact review, exclusion of a stage direction, and creation of two fitted bubbles.

The native phone view exposed speckled thin outlines during texture minification. Previews now use trilinear GPU mipmaps supported by the existing egui Glow backend. Every mip level is counted in the unchanged 64 MiB cache budget, including narrow textures whose overhead exceeds the usual square-texture estimate. The final native build showed continuous outlines in both the canvas and phone reader. This changes only preview filtering; exported rasterization and story appearance settings are unchanged.

The native Start blank flow created a 1600 px story whose three presets all saved at 76 px. The fresh story and populated dialogue fixture are retained under `target/ui-qa-094`. The system Save dialog initially used its default filename despite an automation value update; the newly created, verified blank QA file was relocated into that review folder. Existing user stories were not used.

All 247 tests (97 library and 150 app tests), strict all-target Clippy, and formatting checks pass. No runtime dependency or file-format changes were introduced.

## 0.9.5 preset proof pass

Bubbles now defaults to a 360-point reading strip using the current chapter's working width. The two primary choices are Phone and Fit. Font and sample changes cannot secretly reduce the Phone scale. Long samples scroll inside the stage; the sample editor and appearance controls stay outside that scrolling region. Fit reveals the full shape with a small inset around the bounds. A chapter picker appears only for multi-chapter stories; its choice affects preview resolution, not the active chapter or shared preset. Use identifies the actual placement chapter in its tooltip.

The preview centers the body and preserves the pointer's relationship to it. Horizontal overflow is clipped at the canvas edge and marked Outside canvas. Preview sample, mode and chapter selection remain runtime state, and a document reset clears them. The stage context menu offers Phone/Fit plus the existing preset actions and follows the same glass setting.

Automated real-frame checks cover minimum and laptop window sizes, 150% density, interactive mode changes, sample editing, long-text scrolling, bounded controls, the multi-chapter picker with long titles, stale chapter fallback, document reset, and unchanged story/undo state. Geometry checks cover natural fitting at all three starting resolutions, constant reading scale and visible oversized presets.

Native review at 1280 × 760 and 1100 × 720 verified Phone/Fit, the 1600-to-800 chapter comparison, long-sample wheel scrolling and direct scrollbar dragging, the glass context menu, and the overflow indication. This review caught a 12-point sample-editor jump between modes; the stage now uses a detached child after reserving its area, and a regression asserts stable editor bounds. The temporary sample edits did not mark the story dirty; the review window closed without a save prompt. The fixture is in `target/ui-qa-095`.

All 251 tests (97 library and 154 app tests) and strict all-target Clippy pass. No runtime dependencies or story-format changes were added. This desktop proof uses logical UI points; it is a convenient reading-size comparison, not a promise of identical physical dimensions on every phone.

## 0.9.6 chapter pacing pass

Spacing opens a bounded Before/After review from the chapter inspector and relevant context menus. Position and amount remain visible; crossing-object behavior is under Options. The cut is directly draggable in Before, and scrolling either side pans both before they are painted. Existing previews cull offscreen artwork and share the bounded cache. Only chapter metadata is staged, and unchanged requests reuse their prepared result.

Whole artwork and masks survive insertion/removal. Independent bubble targets and script ranges follow the gap, including negative pointers, targets above a moving bubble, hidden layers and collapsed beat ranges. Affected locked objects require opt-in and keep their lock flags. Newly clipped objects are counted, not deleted. Apply validates a fresh plan before one checkpoint; Cancel and stale document/revision previews cannot change the story.

Real-frame tests cover minimum, laptop, large and 150% density layouts; bounded action targets; empty and populated chapters; direct cut placement; synchronized scrolling; lock opt-in; cancellation; one-step undo/redo; untouched neighboring chapters; and stale drafts. Core tests cover masks, stroke extents, crossing choices, multiple independent pointers, script retention, clipping, limits and fractional-pointer stability.

Native review at 1280 × 760 and 1100 × 720 verified empty and populated previews, insertion, removal preview, crossing choices, clipping indication, lock inclusion, context-menu entry, and cancellation. Applying an insertion shifted the lower panel and its beat together. One undo restored them, and saving the isolated QA archive confirmed the original positions, height and lock flags. Escape initially dismissed the whole review from Options; the corrected build closes Options first, with a regression test and native recheck. Review fixtures are under `target/ui-qa-096`.

All 261 tests (103 library and 158 app tests), strict all-target Clippy, and formatting checks pass. The operation adds no runtime dependencies or archive-format changes.

## 0.9.7 script-lane and demo pass

An empty script lane now offers a bounded 144 × 36 + Beat action. The same action follows the last beat range. Hover or focus previews the proposed span on the canvas. A click creates the beat, brings it into view and focuses its script editor. The default height and gap scale with chapter width. If the next range needs more canvas, a tooltip shows the resulting height and the addition extends blank canvas within the same undo step. Existing beats, artwork and neighboring chapters are preserved. Stale placements, invalid coordinates and chapter limits cannot create unintended changes.

The timeline now copies only visible beat editors instead of cloning every script, note and prompt string each frame. Title editors start with one row, hints are shorter, and existing notes and prompt tools remain available. Range controls no longer clamp saved values merely because their options menu opens, including ranges collapsed by a spacing edit.

The built-in After the rain demonstration now uses 76 px lettering on its 1600 px canvas, fitted through the existing preset system. Caption and Letter are named shared definitions; dialogue uses Webtoon Classic. Its story style is populated and the final blank gutter is shorter. This changes only newly generated demos. Legacy loading tests explicitly construct the older text-layout defaults rather than inheriting modern demo settings.

Native review at 1280 × 760 and 1100 × 720 verified empty and populated lanes, bounded action targets, direct typing into a newly created beat, placement preview, the canvas-extension tooltip and appending a second beat. A toolbar undo removed that beat and restored the previous canvas height together. Saving the isolated review file confirmed the first beat's text remained intact. Review led to centered blue button text, rechecked in the final build at both sizes. The refreshed demo's captions, dialogue and ending gutter were checked in the native phone reader and a standalone 360 px render. Fixtures and the render are under `target/ui-qa-097`.

All 266 tests (104 library and 162 app tests), strict all-target Clippy, and formatting checks pass. Real-frame coverage includes 150% density and large windows, immediate text focus, no mutation on preview, undo/redo, stale placements, a thousand-beat lane, chapter limits, and opening extreme saved ranges without changes. Demo checks verify reading size, natural fitting, shared links and scene bounds. No runtime dependency or archive-format changes.

## 0.9.8 crowded script pass

Closely spaced and overlapping ranges now form a readable script group. Each group displays one editor, with fixed previous/next controls and a count menu containing the beat titles. Canvas scrolling follows the relevant beat; explicit navigation holds its choice until scrolling resumes. Short and collapsed ranges retain their content and a useful editing area. The connector points to the selected beat's actual canvas anchor, and grouping never reorders or moves saved content.

Cards remain alongside their group while it is visible, below the floating toolbar. Header controls stay outside the dialogue/notes/prompt scroll. Only visible selected editors copy strings; cached numeric geometry and a virtualized picker bound work for large chapters. Epoch, chapter, revision and zoom invalidate geometry. Missing selected IDs fall back safely, and pending new-beat focus takes priority over following.

Native review reproduced the original eight-card overlap and verified the grouped result at 1280 × 760 and 1100 × 720. Arrows, title selection, a collapsed beat, direct text editing, toolbar undo, canvas following and the final + Beat control were exercised. The saved QA archive confirmed all nine original ranges, scripts, notes, prompts, layers and canvas height after undo. A heading right-click initially fell through to the app menu; disabling text selection on card section labels corrected that route without intercepting editable fields, and the final native build was rechecked. The empty lane was also reviewed at both sizes: its bounded action creates a focused editor, and one toolbar undo restores the empty state. Isolated fixtures are under `target/ui-qa-098`.

All 271 tests (104 library and 167 app tests) and strict all-target Clippy pass. Coverage includes unsorted and same-position ranges, low-zoom nonoverlap, one-step edit undo, chapter switches, missing IDs, focused insertion, following, 150% density, a 10,000-beat group, fixed navigation while long text scrolls, and removing the displayed beat through its context menu. No runtime dependencies or story-format changes were introduced.

## 0.9.9 sample and packaging pass

The lettering playground is now three vertical 1600 × 3200 chapters with identical passages in Mature, Casual, and Classic, using 76 px dialogue and a shared Labels preset. The bubble playground is two 1600 × 4000 chapters with shared speech, thought, and rounded presets, including three independently aimed pointers and negative offsets. Short headings replace instructions painted on the comic. The character sample retains its full profiles and adds authored gender values and three locations. The layout sample now constructs its 800 px canvas directly, keeping its presets at 38 px. After the rain is included as a fifth editable sample.

`scripts/samples.ps1` generates the archives and ten 360 px chapter proofs. Packaging runs it from current source and requires every sample. It no longer copies optional old archives from examples/. Each generator validates storage, linked presets, minimum nominal reading size, text overflow, chapter bounds, lettering overlap and exact story/asset round trips. No user file or older distribution is migrated or overwritten.

Visual inspection covered the three font themes and both multi-pointer chapters. Native review opened the packaged lettering, bubble, and character stories; checked the Phone reader, chapter navigation and preset usage counts; and inspected character cards, the empty reference state, Female selection, and all three location folders. The populated sheet and folders fit 1100 × 720. A native preset size edit from 76 to 84 updated all four linked bubbles across both chapters while retaining every independent speaker target. One undo and a save restored the exact original story. QA copies are under `target/ui-qa-099`.

All 271 tests (104 library and 167 app tests), strict all-target Clippy, and formatting checks pass. The portable archive now includes five required editable stories. This pass adds no runtime dependency or file-format change.

## 0.9.10 display-resolution lettering pass

Lettering previews now rasterize through the export compositor at their displayed physical pixel scale. Texture rectangles retain raster padding and align to the physical display grid; saved layer geometry, line breaks, hit bounds and speaker targets remain unchanged. Editor, Phone/Wide reader, Spacing review and Bubbles proof share the path. Text-only gamma-space premultiplication corrects partially transparent edges for the existing Glow backend. Image preview behavior is unchanged in this pass.

Scale participates in cache identity. During zoom changes, the nearest cached scale can remain visible only when every appearance field matches. Changed dialogue, size or pointers cannot reuse stale content. The worker queues, 2048 px raster cap and 64 MB cache including mip levels remain bounded. No synchronous rasterization or idle animation loop was introduced.

A native three-column diagnostic compares direct export, the new preview and the old mipmapped path, accounting for the desktop's actual 150% scaling after native DPI initialization. Final packaged-app inspection at 1280 x 760 and 1100 x 720 covered editor zoom/Fit, Phone/Wide reading, Bubbles Phone/Fit, shared size changes, one-step undo and the compact empty Characters state. Saving the isolated QA story after undo restored its complete JSON and assets exactly. Fixtures are under `target/ui-qa-0910`.

For the twelve text layers in Lettering playground, a warmed release diagnostic at 360 logical pixels and 150% display scaling measured 20,642,852 versus 2,359,568 base raster bytes (88.6% less), and 165.185 versus 22.7824 ms preparation in one run. This is a sample measurement, not a frame-rate guarantee or the full GPU allocation; mip levels remain included in cache accounting. Large lettering can still reach the texture cap, and subpixel positions can differ slightly from a whole-chapter export.

All 277 tests (106 library and 171 app tests), strict all-target Clippy and formatting checks pass. Tests cover export agreement across fonts, shapes, negative pointers and output sizes; every alpha value over light/dark surfaces; 100-200% display scaling; zoom fallback; edited-content isolation; stale completion; and bounded work. No runtime dependency or story-format changes.

## 0.9.11 artwork transparency pass

Artwork uses gamma-premultiplied upload colors and opacity tints consistent with CPU export blending. This corrects the bright color cast on transparent images, shapes and strokes. Preview images premultiply before shrinking, so hidden RGB cannot contaminate cutout edges. Lasso coverage scales premultiplied RGB and alpha together. Text upload encoding also moves to the bounded worker. UI theme/glass colors retain their previous conversion.

Native inspection reproduced the original mismatch in 0.9.10, then verified its correction in the packaged 0.9.11 editor and Phone reader over white and dark paper. The fixture includes matching shapes/images at 50% opacity, an alpha ramp, a clipped transparent cutout and a stroke. Review covered 1280 x 760 and 1100 x 720 at 150% display scaling, including the compact empty Characters state. The generated fixture and CPU export proofs are under `target/alpha-review`; the final isolated copy is under `target/ui-qa-0911`.

All 281 tests (106 library and 175 app tests), strict all-target Clippy and formatting checks pass. Added tests compare prepared image uploads and vector colors with actual CPU exports across all alpha values, several opacity levels and light/dark surfaces; compare masked image edges; verify transparent RGB invariance during resizing; and inspect actual canvas paint output while proving opacity changes reuse the texture. Original assets and .scro contents remain unchanged. The five packaged samples regenerate successfully.

GPU filtering, mipmaps and vector coverage can still differ from CPU rasterization. The native fixture exposed an existing flat-stroke-end preview versus rounded export difference; this is documented and left unchanged. No new dependency, archive format or permanent UI control was added.

## Deferred review notes

- Brush endpoint geometry differs between the current vector preview and CPU export. No further pass was started: ongoing iteration was paused at the user's request after packaging 0.9.11.


## Windows installer — 0.9.11

The per-user Windows installer uses the existing app icon, a destination page, optional sample/file-integration/desktop components, progress and a launch finish page. Start menu registration and Windows Installed apps removal are included. Open with registration advertises `.scro` support without changing its default or UserChoice. Only explicitly named package files are embedded.

The NSIS 3.13 portable compiler is pinned to the SHA-256 published on its official SourceForge download page. Setup uses zlib compression and includes the compiler project's license notice; the compiler is a build tool and is not installed with Scroll Studio. The output is unsigned.

The actual production setup was installed silently into an isolated workspace directory. Checks verified the exact executable hash, five samples, Start menu shortcut, uninstall metadata and correctly quoted story-open command. Native inspection launched the installed executable with After the rain. A second installation preserved an edited sample, an additional saved story and notes; uninstall removed its executable, uninstaller, registration and shortcuts while preserving all five samples and the added user files. Destination and component pages were reviewed natively, then cancelled before installation. The user's separately running portable copy was left intact.


## 0.10.0 — two-pass feature review (November 1, 2026)

Pass 1 checked data, coordinates, undo, storage, export and bounded work. Pass 2 used rendered proofs, native interaction at 1280×760 and 1100×720 logical pixels on a scaled Windows display, plus targeted regressions for issues found during review. Additional corrections were rerun through the full suite.

| Feature | Pass 1 | Pass 2 and polish |
| --- | --- | --- |
| Sharp reference viewer | Display/DPI-sized requests; original-pixel regions; mirror mapping | Native chart and reference review; detail replaces the base texture without double alpha |
| Sharp canvas assets | Source crops and clipped textures share original bytes with export | Proof and native canvas comparison; crop hit coordinates corrected |
| Adaptive preview quality | Slow-frame hysteresis, bounded worker queues and cache | Zoomed detail obeys the reduced cap; rapid worker completions cannot exhaust idle restoration |
| Character reference board | Aspect-preserving masonry, visible tiles only | Native mixed-aspect board; portrait cards favor the upper image so faces remain visible |
| Locations | Shared image IDs, profile fields and composed angle prompts | Native cards/References/Details; stale canvas drop target cleared, bounded empty state with one create action |
| Rectangular selection | Transient selection separate from crop | Native drag exposed a duplicate closing corner; four-corner pointer regression now covers Select and Crop |
| Freehand Lasso | Original-pixel masks and undo | Compact controls, outline clipped to snippet bounds, freehand-only stabilizer |
| Smart Lasso | Connected color selection, transparent-pixel handling and bounded flood queue | Same modifier behavior, source crop coordinates and focused quick controls |
| Shift/Alt and stabilizer | Add/subtract/disconnected-mask tests | Shared behavior in the snippet picker; Shift/Left Alt tooltips and smoothed strokes |
| Separate Crop | One-step rectangular crop, selection-to-crop action | Restoring a snippet recovers source scale and position; no source pixel mutation |
| Dotted/Fade/Hard edges | Shared preview/export renderer; finite background band | Proofs inspected; outside bands retained in snippets; expensive slider changes commit on release |
| Contour/color/opacity | Inside/Center/Outside and edge alpha validated | Optional Edge menu exposes dot scale, contour, color and opacity without flooding the artwork |
| Separate/copy to layer | Source image reused; cut leaves a complementary mask | Crop mapping, snapping, undo/redo and archive round-trip regressions |
| Background Snippet | Crop metadata only, target aspect retained | Native picker and frame drag; archive test verifies one asset entry plus manifest |
| Character Snippet | Border/connected-subject estimate plus editable mask | Native smart click and placement; outward edge extent and independent layer preserved |
| Content-aware brush/fill | All visible composite layers sampled; unpainted pixels untouched | Native fill exposed distant donor artifacts; stable local guidance and opposite-boundary continuation added; source/undo/stale-result tests |
| Motion Blur | Directional premultiplied samples in a separate patch | Native directional stroke; independent useful brush size, visible brush footprint and current tool label |
| Action lines and smear | Editable procedural lines; thresholded source smear | Native right-click and smear generation; narrow inspector fixed, source opacity retained, generated working image bounded |
| Wind | Curved procedural geometry and validated controls | Export proof/native canvas, density/angle/curve controls and layer opacity inspected |
| Glint | Small editable procedural rays/glow with no raster asset | Proof/native canvas; independent position, size, color, angle and opacity |
| Angle popup | Finite projection and camera/framing prompts | Native orbit drag and Portrait framing; numeric ring labels, ±90° tilt, high-contrast Copy prompt |
| Linked bubbles | Independent text, shared preset, ephemeral render joins | Pixel test verifies no internal border; native repeated + clicks now advance past existing dialogue; save/preset regression and export proof preserve separate assets |
| Intensity and Impact bubbles | Shared native/export path and automatic text fitting | Spikes randomized to remove repeating saw-like blocks; pointed inward-curved border and safe text inset checked in proof |
| Transparency | Original alpha retained; all layer types expose opacity | Checkerboard references, premultiplied previews, independent generated patch opacity |
| Recovery and modal isolation | Stale worker results rejected; modal actions isolated | Native Windows folder virtualization reproduced and fixed by resolving the actual write directory before strict containment checks; native recovery round trip passed |

Validation: 314 tests passed (122 library + 192 application), all targets; Clippy with warnings denied; formatting check. `examples/studio_tools_playground.rs` produces a six-source story, original-byte round trip, identical full/sliced export pixels, native recovery check and effect/fill proofs. A representative release run completed composite fill plus proof in about 0.18 seconds; this is a fixture measurement, not a guarantee for all image sizes or hardware. Generated local texture fills can still need adjustment on complex scene structure.
