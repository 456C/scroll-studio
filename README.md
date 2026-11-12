# Scroll Studio

A native Rust desktop editor for composing vertical comics. A story contains its chapters, script beats, character sheets, backgrounds, image assets, and editable layers in one `.scro` file.

This is an early desktop editor, currently version 0.13.0. It uses egui with the OpenGL/Glow backend, without a browser runtime or an account requirement. Image generation happens in a separate application; Scroll Studio provides the local composition and continuity workflow.

## Changes in 0.13.0

- **Lore** is an investigation board. Pin characters, locations and groups, drag cards freely, and join them with colored, labeled connections. Click a port and a destination, or drag the port onto a card. Right-click empty space to add a note there.
- Notes and card-specific details stay editable in the sidebar. Double-click a character, location or group to open its sheet. Pan empty space, Ctrl+wheel to zoom around the cursor, and **Fit** to see the whole board. Additions, moves and connections support undo/redo.
- **Image Editing** now consumes every delivered pointer sample, preserving fast corners instead of skipping to the last point in a frame. Supported pen input controls size and flow, with minimum size and response under **Brush settings**. Smoothing defaults to zero and remains optional.
- Cached, antialiased brush tips, bounded changed-region texture updates and reusable texture handles reduce painting work while keeping the full-resolution pixels. Stroke opacity is separate from flow, so translucent strokes build predictably across separate strokes.
- A reproducible CPU benchmark measured large 256 px ink/airbrush batches about **2.7× / 3.6× faster** on the development machine. This is rasterization timing, not measured pen-to-display latency. Research, methodology and limitations are in [painting-performance.md](docs/painting-performance.md).
- Stories now use **format 11**. Older lore notes and links become a board automatically. Original assets and chapter content stay intact. Files saved by this version require **Scroll Studio 0.13.0 or newer**.

## Changes in 0.12.0

- The start screen separates **Your stories**, **Edit Assets**, and **Image Editing**. Use **File → Start screen** to switch workspaces.
- **Edit Assets** opens or creates a complete `.scro` for characters, locations, groups, lore and chronology. Chapter canvases and scripts are available through **Your stories**.
- **Lore** stores named notes with links to characters, locations, groups, and other notes. Click a link to open its entry.
- **Image Editing** opens PNG, JPEG and WebP images or starts a transparent canvas. Paint with Ink, Pencil, Marker, Airbrush and Eraser. Size and flow are visible, with edge softness and spacing under **Brush settings**. Save full-resolution RGBA pixels as PNG.
- Stories now use `.scro` format **10**. Older stories open normally. Files saved by this version require Scroll Studio 0.12.0 or newer. If another editor has saved your open file, **Save as** preserves your separate copy.

## Changes in 0.11.0

- A brief **456C** author splash fades into the app. Click, Enter or Escape skips it; reduced motion keeps it static.
- **Groups** holds story-wide affiliations, color, descriptions and reference boards. Link characters and locations from their sheets or the group's Members tab. Group details follow name changes and join copied prompts.
- **Timeline** arranges chapters chronologically with draggable cards, date/era labels and summaries. Reading order stays separate. Double-click a card to open its chapter; changes support undo/redo.
- Group references reuse original assets. New imports retain their original bytes; no duplicate image storage is needed to share a reference.
- Saving uses `.scro` format **9**. Older stories load normally; newly saved files require Scroll Studio 0.11.0 or newer.
- A minimalist download website is in `website/`, with verified installer/ZIP links, local fonts, mobile layouts and no tracking or payment gate. Hosting and optional-support recommendations are in `docs/hosting-research.md` and `docs/monetization-research.md`.

## Changes in 0.10.1

- Smart Lasso has a **Size** control (1–256 screen pixels) and a visible circular brush. Click or drag to select connected regions; Shift adds and Left Alt subtracts. Character Snippet uses the same width and catches nearby foreground inside the brush.
- Active selections survive undo/redo and ordinary edits while their original image exists. **Ctrl+D** clears the selection without undoing a crop; Escape cancels the current gesture.
- Stabilizer range reaches **4×** its previous maximum. Existing values keep the same strength.
- **Magic Eraser (E)** removes a clicked color through a reversible mask. Tolerance and Connected are the only quick controls; turn Connected off to include enclosed gaps. An active pixel selection limits the removal.
- **Blend edges** adds a soft backing around the actual cutout and across small hair gaps. Right-click an image to enable it; the inspector exposes color, scene matching, opacity, reach, softness and gap size. Original image bytes and opaque pixels remain unchanged.
- **Ctrl + mouse wheel** zooms the canvas around the point under the cursor, including when zooming out below Fit. Ordinary wheel scrolling stays unchanged.
- Scene-color matching accounts for export scale, fixing “Export region lies outside the chapter.”
- Saving uses `.scro` format **8** for backing settings. Older stories load normally; files saved by this version require Scroll Studio 0.10.1 or newer.

## Changes in 0.10.0

- **Selection, Crop, Lasso and Smart Lasso** share additive/subtractive selections. Shift adds and Left Alt subtracts. Crop keeps original pixels intact; separate or copy a selection into its own layer for independent motion.
- **Crop edges** offer Hard, Dotted and Fade with width, Inside/Center/Outside contour, dot scale, edge color and opacity. The generated color stays inside the edge band. A stabilizer smooths freehand input.
- **Snips** opens Background or Character Snippet. Choose a reference, crop or smart-select, and place it. A snippet stores a source ID, crop and selection, sharing the original image bytes. Background frames match the target panel or visible canvas; characters are placed above backgrounds.
- **Locations** now has visual cards, a reference board, descriptions, atmosphere, lighting, continuity, notes and reusable angle prompts. Character references also use a masonry board. **Angle** opens a lightweight mannequin with orbit, tilt, roll and framing controls, plus one-click character/style/camera prompt copying.
- **Content-Aware Brush** samples the visible chapter composite to hide small unwanted details. A selection can also be filled. It uses local texture sampling, with an undoable patch layer and no external service.
- **Effects** includes Motion Blur, Glint and curved Wind. Right-click an image for **Add motion**; its inspector offers directional action lines and threshold-controlled **Generate smear**. Separated parts can each have their own effect.
- **Linked bubbles** use a small + beside the selected bubble. Each bubble remains an independent text layer, with shared story presets and no interior outline at compatible overlaps. New **Intensity** and **Impact** borders follow the supplied examples.
- High-DPI references and zoomed detail use the original source. Preview quality temporarily steps down after sustained slow frames and recovers while idle. Original images, saved source bytes and export resolution remain unchanged. Transparent references have checkerboards; every canvas layer retains opacity controls.
- Recovery resolves the actual Windows write directory before validating paths, supporting redirected LocalAppData without weakening session containment.
- `.scro` format 7 adds compressed selections, source crops, location profiles, effect parameters and bubble clusters. Existing versions load with defaults; older app versions cannot open format 7. Undo/redo and recovery retain these features.

**Scope:** Smart subject selection estimates foreground from connected regions and border colors; busy backgrounds may need the editable lasso. Local content-aware fill works best on small objects and repeated textures; it does not understand or reconstruct a scene like an image-generation model. Generated fills use bounded texture synthesis, and generated smears use a working image up to 1400 px on its longest side. These limits apply to new effect pixels, never to source artwork. Large brush operations ask for a smaller area rather than blocking the editor.

## Changes in 0.9.12

- Large character references request previews at their actual display size, including high-DPI scaling, instead of stretching a 384 px thumbnail.
- Zoomed pinned references load the visible part of the original image. Panning and mirroring preserve correct detail placement; existing previews remain visible while sharper detail loads.
- Detail textures stay within the 2048 px cap and existing 64 MB cache. Source images and saved stories are unchanged.

## Changes in 0.9.11

- Transparent images, shapes, and brush colors use the export compositor's blending in artwork previews. Lowering opacity no longer adds a bright color cast.
- Resized cutouts ignore hidden RGB in fully transparent pixels. Lasso mask edges scale premultiplied color and alpha together.
- Color preparation runs in the bounded preview worker. Opacity changes reuse the existing image texture; original assets, export output, and saved stories stay unchanged.
- The same image cache serves chapters, reading previews, library thumbnails, character references, and the pinned reference window. UI glass keeps its own appearance settings.
- No runtime dependencies or archive-format changes.

## Changes in 0.9.10

- Text and bubble previews rasterize at their displayed physical size, improving lettering in the editor, Phone reader, and Bubbles proof, including high-DPI displays.
- Preview pixels align to the display grid without changing saved geometry, wrapping, pointer targets, or export output. Transparent lettering edges follow the export compositor's blend.
- Zooming keeps a matching cached preview visible while the new size prepares. Changed words or pointers cannot borrow an outdated appearance.
- Background work stays bounded, with the existing 2048 px texture cap and 64 MB shared cache. Images keep their current preview pipeline.
- No runtime dependencies or archive-format changes.

## Changes in 0.9.9

- Lettering samples now use vertical chapters and reading-size text. Compare Mature, Casual, and Classic using the same dialogue; every text layer belongs to a named shared preset.
- The two-chapter bubble example demonstrates shared speech, thoughts, rounded bubbles, and several independent pointers, including targets above and to the left.
- The character example includes the current gender selector values, three named locations, and the full outfits, palettes, continuity details, and story style. The layout example uses presets scaled for its 800 px canvas.
- Packaging generates and validates all five samples from current source, including **After the rain**. Reading-size PNG proofs stay in the build directory; the portable package contains editable stories.
- Existing user files and older distributions stay untouched. No runtime dependencies or archive-format changes.

## Changes in 0.9.8

- Nearby script beats share a readable card instead of overlapping at low zoom. Step through them with the arrows or choose a title from the count menu.
- The card follows the visible part of the comic. Navigation stays available while long dialogue, notes, and prompts scroll inside it.
- Grouping changes only the view. Beat positions, script content, artwork, and story history stay intact.
- Geometry is cached separately from text; only visible selected editors copy strings, and the beat picker virtualizes large groups.
- No runtime dependencies or archive-format changes.

## Changes in 0.9.7

- **+ Beat** starts an empty script lane and continues after the final beat. Hover or keyboard focus previews its canvas range; adding opens directly into writing.
- The next beat leaves a gap after existing ranges. If it needs more canvas, the preview tooltip shows the new height and the addition extends the blank canvas in the same undo step.
- Script editors copy only visible beats, keeping large offscreen scripts out of the frame's editing work. Shorter hints and a single initial title row give dialogue more room.
- The built-in **After the rain** demo uses fitted lettering at reading size, named shared Caption/Letter presets, a story style prompt, and a shorter final gutter. Existing saved stories and older sample archives retain their appearance.
- No runtime dependencies or archive-format changes.

## Changes in 0.9.6

- **Spacing** inserts or removes vertical space with a synchronized Before/After preview. Open it from the chapter inspector or right-click a chapter, script beat, or canvas position.
- Click or drag in Before to place the cut; scroll either preview to inspect the chapter. Exact position and amount remain directly accessible. **Options** controls whether artwork crossing the cut stays in place or moves with lower artwork.
- Whole objects remain editable. Script ranges and individual bubble pointers follow the spacing change; source images, masks, and object sizes stay intact. Shortening the canvas can clip artwork at the edge, which the preview flags.
- Affected locked objects require an explicit Include choice and remain locked afterward. Cancel changes nothing; Apply is one undo step. No runtime dependencies or file-format changes.

## Changes in 0.9.5

- **Bubbles** opens with a phone-width proof of the selected preset. Text-size changes stay visible at a fixed reading scale; **Fit** reveals the entire bubble for close inspection.
- Long samples scroll inside the proof. Oversized bubbles retain their scale and show **Outside canvas** when their bounds exceed the selected chapter's width.
- Stories with multiple chapters have a preview chapter picker. It uses that chapter's resolution without switching the editor's active chapter or changing any shared preset.
- Preview mode, sample dialogue, and chapter choice are temporary workspace state. They do not change `.scro` data, dirty status, or undo history, and reset when another story opens.

## Changes in 0.9.4

- New story presets scale lettering, bubble padding, outlines, and pointer width to the selected working resolution. WEBTOON, WEBTOON 2×, and Tapas begin with consistent reading sizes. Existing stories retain their saved appearance.
- New presets use the current chapter's width; duplicating a preset keeps its exact appearance. Choosing a lettering theme uses padding proportional to the text size. Presets remain shared across the story and do not automatically rescale when switching chapters.
- **Make bubbles** has a more compact review for short passages, with three-line editors, internal scrolling for long text, and a fixed footer for long lists.
- Opening appearance controls no longer clamps saved values to slider ranges. No runtime dependencies or file-format changes.
- Canvas, reader, and library previews use GPU mipmaps to keep thin outlines and image details smooth when reduced. All mip levels count toward the existing 64 MiB preview cache limit; zooming adds no CPU resampling work.

## Changes in 0.9.3

- **Pin reference** keeps an existing image in a movable, resizable viewer while you compose the chapter. Character references have previous/next controls; the sheet also has a direct **Pin** button.
- Scroll over the reference to zoom and drag to pan. **Fit** or a double-click restores the whole image. Flip, copy, open the character sheet, and close are available in its menu. The frame follows Glass/Solid settings.
- Drag a library image into the viewer to switch references. Dropping an image file imports and pins it without placing a canvas layer. Imports remain undoable; viewing, zooming and flipping do not edit the story.
- The viewer shares the existing bounded preview cache, tracks source identity, and clears on story changes. It adds no runtime dependencies or file-format changes.
- Script beat menus and text editing keep their own input targets when right-click actions are available. The reader's blank-area menu also stays behind its scroll controls.

## Changes in 0.9.2

- **Characters** opens as a scrollable card gallery with reference portraits, names, roles, and palette swatches. Search the cast and open any card for its complete sheet. **All characters** returns to the same browse position; the sheet itself keeps its fixed tabs.
- Large casts render visible rows and request only visible thumbnails through the existing bounded preview cache.
- A bubble-preset slider drag is one undo step. Linked bubbles still update live across every chapter, including their natural sizing and pointer positions. Numeric typing, colors, and separate choices retain their own edits.
- Background and image previews stay visible while editing notes or prompts.
- Backgrounds are grouped into named **Locations**, with scoped import, rename, moving images, and Unsorted for existing artwork.
- Identity uses a compact **Female / Male** control; palette swatches no longer require a visible name field. Legacy data remains preserved.
- Reference sheets give single images more room and reveal notes under **Details**. **Save PNG** is in the reference menu.
- The inspector uses visibility/lock icons, percent opacity, compact folder controls and contextual actions. Menus and inspector controls follow Glass/Solid settings.
- **Make bubbles** reviews script paragraphs before creating a fitted batch in one undo step. **Read** opens a phone-width chapter preview. See [workflow research](docs/workflow-research.md).
- A simpler S icon replaces the detailed scroll mark; the large toolbar icon is removed.
- New saves use `.scro` version 6 so older builds cannot silently discard location folders or character identity. Versions 1–5 still open and upgrade in memory. No runtime dependencies were added.

## Changes in 0.9.1

- Empty Characters, References, and Bubbles have compact, bounded actions instead of oversized full-workspace buttons. Redundant empty sidebar controls are removed.
- The floating canvas toolbar stays compact and centered. **Fit** follows window size, Script visibility, and chapter width; manual zoom stays fixed until Fit is selected again.
- Text-copy actions briefly confirm **Copied** at the button without shifting nearby controls.
- Image and background cards select across their full surface and clearly show selection. Layer ordering disables unavailable actions.
- No runtime dependencies or story-format changes. See the [interface research and review loop](docs/ui-review.md).

## Changes in 0.9

- **Settings** is a dedicated section, also available from **File** or `Ctrl+,`. **Appearance** offers Glass/Solid surfaces, frost strength, and Off/Calm/Lively motion with a live preview. **Canvas** saves snapping preferences. These preferences are local to the app and do not change story data or undo history.
- The canvas tool shelf floats above the scrolling artwork. Glass mode uses real GPU backdrop blur; Solid mode keeps an opaque surface. Artwork, exports, and `.scro` files remain unchanged by interface effects.
- Buttons ease into hover and selection, compress subtly on press, and settle on release. Navigation indicators glide and character panes fade briefly. Hit targets stay fixed, and Off disables custom motion.
- Blur captures only a small padded region at quarter resolution with two separable GPU passes. Reused blur textures are capped at 2 MiB; there is no CPU readback or continuous effects repaint loop. Unsupported drivers keep a readable tinted surface.
- The new blue comic-scroll icon is used in the window, header, and Windows executable. The original concept is preserved under `assets`; see [icon generation and exact prompt](docs/icon-generation.md).
- No new runtime dependencies or story-format migration. Windows builds now embed icon resources using the existing portable toolchain's `windres`; the build script configures it automatically.

## Changes in 0.8

- A compact graphite interface combines Adobe-inspired panel hierarchy with Procreate-inspired simplicity: blue selection accents, vector tool icons, flat library rows, and focused settings tabs. Character sheets retain their fixed workspace and always-visible category navigation.
- Drag layers into alignment with the canvas center and edges, other visible objects, or equal spacing. Live magenta guides show the active alignment and matching gaps.
- **Snap** toggles the magnets; **S** is its shortcut and holding **Alt** temporarily allows free movement. The adjacent chevron reveals **Canvas**, **Objects**, **Equal spacing**, and **Strength** controls.
- Placing an asset without a drop location starts it centered horizontally on the canvas. Explicit canvas drops keep the chosen location.
- The editable **Layout playground.scro** sample provides panels for trying alignment and spacing.
- Existing stories remain compatible, `.scro` stays at format version 5, and no runtime dependencies were added.

## Changes in 0.7

- **Characters** is a fixed workspace with seven visible tabs: References, Appearance, Outfits, Palette, Identity, Notes, and Prompt. Details open in place instead of expanding into a scrolling page. References, cast, and outfits use paging; long text scrolls only inside its editor.
- Removed the duplicate character inspector, persistent tutorials, redundant headings, and the separate style strip. **Style** and **Copy style** stay in the main toolbar.
- **Bubbles** uses one adaptive preview and **Style / Text / Bubble** settings tabs. Chapter pointers have numbered selectors and direction buttons, with exact coordinates under **…**.
- Background and image details use **Image / Notes / Prompt** tabs. Exact layer transforms are under **Position & size**. Layer folders keep their **Collapse / Expand** controls.
- Shorter setup, export, and welcome screens retain their actions. The default window fits a smaller desktop, and layout regression tests cover small windows and full character collections.
- Opening a story fits its canvas and script to the available workspace automatically.
- Story data and `.scro` format remain compatible with version 0.6. No new runtime dependencies.

## Changes in 0.6

- **Characters** is a dedicated workspace: name, concept, reference board, and copy actions come first. Optional sections reveal appearance, outfits, named color swatches, consistency rules, identity, backstory, relationships, and custom prompt instructions.
- **Prompt pack** combines the current story style with the selected character and outfit. Choose portrait, turnaround, expressions, or a scene. Copy the prompt, then copy or export a labeled PNG reference sheet for your image generator. Images remain embedded in the `.scro` file.
- **Story style** and **Copy style** are always available above the workspace. `Ctrl+Shift+C` copies the base prompt. Character, background, and scene prompt actions include its latest text automatically.
- New stories open a three-step interactive setup: story/canvas, visual style, and optional first character. **Skip · start blank** is available at every step. Cancel leaves the previous document intact.
- Layers are grouped by type into collapsible folders with **Expand all** and **Collapse all**. Grouping preserves the actual drawing order.
- New saves use `.scro` version 5. Earlier stories retain their notes, prompts, artwork, and bubble settings. **Character studio playground.scro** supplies two editable characters with outfits and palettes.

The design rule for this project is a simple first view with deeper controls available on demand. Common tasks should work without a tutorial; advanced details should be easy to discover when needed.

## Changes in 0.5

- **Bubbles** is a story section for creating, naming, duplicating, and previewing reusable presets. Every story starts with Webtoon Classic, Mature, and Casual. Appearance is edited here: font, shape, text size, colors, outline, spacing, opacity, and pointer width/curve.
- Chapters only choose a preset, edit dialogue, and position bubbles and pointers. Editing a preset updates all its uses across every chapter, including hidden or locked bubbles. Undo restores the preset and affected chapters together. Presets are saved inside the `.scro` story.
- A bubble can have up to eight independent pointers. **+ Add pointer**, then drag its numbered handle toward another speaker. Left/up directions and negative exact target coordinates are supported. Thought bubbles get multiple dot trails; narration suppresses pointers while retaining their placements.
- Natural fitting preserves all speaker targets. Appearance edits refit linked bubbles; manual chapter resizing is still available. Existing stories acquire presets without changing their original appearance or geometry.
- New saves use `.scro` version 4. The package includes **Bubble presets playground.scro**, with shared presets in two chapters, plus the earlier lettering sample.

## Changes in 0.4

- Three visible lettering themes with live font samples: **Mature / Lato**, **Casual / Balsamiq Sans**, and **Webtoon Classic / Komika Hand**. New dialogue defaults to Webtoon Classic with capital lettering. Original typed text is retained.
- Natural automatic fitting changes both width and height. Adding dialogue grows a bubble; deleting dialogue shrinks it. Wrapping uses the actual font metrics, respects explicit newlines, and searches for a balanced shape within the chosen maximum width.
- Fitting keeps the bubble's center and the pointer's target steady. Dragging a corner switches to manual sizing; **Fit bubble to text** enables automatic fitting again. **More options** contains its toggle, maximum width, individual fonts, and uppercase control.
- Version 1 and 2 stories retain their original fonts and geometry. New saves use `.scro` version 3 to preserve the new fonts and editing behavior. The package includes an editable **Lettering playground.scro** with the three themes at different dialogue lengths.

## Changes in 0.3

- Added Comic Neue, Patrick Hand, and Delius Unicase. These remain available for existing stories; version 0.4 introduces the new theme defaults above.
- Speech, rounded, thought, whisper, shout, narration, and electronic bubbles, plus text without a bubble.
- A pointer you can drag toward a character, word wrapping, and a **Fit bubble to text** action. More options exposes colors, borders, spacing, alignment, pointer width, and curve.
- Shared preview/export rendering and portable bubble settings inside `.scro`; version 0.3 introduced format version 2.

## Changes in 0.2

- Automatic recovery copies every minute while the story has new unsaved changes, including untitled stories. **File → Recover previous work** restores a copy after an interrupted session.
- Background image and text preview preparation, smaller library thumbnails, bounded work queues, and a 64 MiB cache that keeps recently used previews.
- PNG or JPEG export with a quality control, automatic splitting to satisfy each image's size limit, episode warnings, and an export report.
- Corrected **Fit** zoom and removed full layer-list cloning during canvas repaint.

The Windows package is versioned (`dist/Scroll Studio 0.13.0`) so updating does not overwrite an older executable that may still be running. The archive is `dist/Scroll-Studio-0.13.0-windows-x64.zip`.

## Install on Windows

Run `dist/Scroll-Studio-0.13.0-Setup-x64.exe` for Windows 10/11 x64. Setup installs for your account under `%LOCALAPPDATA%\Programs\Scroll Studio` without administrator access. It adds a Start menu shortcut; sample stories, Windows Open with integration for `.scro`, and a desktop shortcut are selectable. Your existing default file association is not changed.

Remove the app from Windows Installed apps or run `Uninstall.exe` in its folder. Uninstall preserves sample stories, other saved files, preferences and recovery copies. Upgrades keep any sample you have edited. The installer is currently unsigned.

To rebuild setup, run `scripts/installer.ps1` (or `-SkipBuild` after verifying the current release executable). It prepares the portable package, downloads the project-local NSIS 3.13 compiler if needed, checks its published SHA-256, and writes setup plus a `.sha256` checksum beside the ZIP. No system compiler installation or PATH changes are needed. Installation itself works offline.

## Run on Windows

From PowerShell in this repository, install the project-local build tools if they are not already present:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/setup-toolchain.ps1
```

This downloads Rust and LLVM-MinGW into `.tools`. It does not modify the system PATH. Internet access is required for initial setup and dependency downloads. The development toolchain and build cache are much larger than the resulting application.

Build and launch the optimized application:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/build.ps1 -Action run -Release
```

Build without launching, then run the executable directly:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/build.ps1 -Action build -Release
.\target\release\scroll-studio.exe
```

The executable uses the installed Windows graphics driver. Runtime operation is local and does not require the Rust toolchain or a network connection.

Open the editable sample or an existing story:

```powershell
.\target\release\scroll-studio.exe --demo
.\target\release\scroll-studio.exe "C:\Comics\My story.scro"
```

Generate the editable examples and their 360 px reading proofs:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/samples.ps1
```

Outputs go to `target/sample-package`. Packaging runs this step automatically and checks file validation, shared preset links, reading size, text overflow, bounds, lettering overlap, and an exact save/load round trip. To create the Windows ZIP, run `scripts/package.ps1`; generated samples are required, not optional files from an older checkout.

With Rust 1.89 or newer and native build dependencies already available, standard `cargo run --release`, `cargo test`, and `cargo clippy` also work. The included portable setup scripts target Windows x64; other operating systems have not been verified here.

## Workflow

1. Choose **New story** and follow the skippable setup for a name, canvas, visual style, and optional first character. **Start blank** keeps the name and canvas but clears other setup fields. Your first chapter starts with no artwork or script scaffolding; add beats where needed.
2. Use **+ New → Chapter, Character, or Background**. Browse character cards for sheets, references, and prompts. In Backgrounds, **+ Location** creates a named folder; selecting it scopes subsequent imports to that location.
3. Write each beat's action, dialogue, notes, and optional generation prompt beside the canvas. Beats share the canvas scroll position. **Make bubbles…** in a beat's menu lets you review blank-line-separated passages, omit stage directions, and create fitted dialogue in one undo step.
4. Copy a prompt into your image generator. Bring the result back by dropping a PNG, JPEG, or WebP file into the library or directly onto the canvas, using **Import images**, or pasting an image from the clipboard.
5. Drag an asset from the library onto the canvas. Double-clicking its name or using **Place on canvas** also places it. Reusing an asset creates another layer without duplicating its original image in the story archive.
6. Move artwork by dragging; **Snap** aligns it to the canvas, nearby objects, and equal gaps with live magenta guides. Press **S** to toggle snapping or hold **Alt** to move freely. Drag a selected layer's corner to resize it; hold Shift to keep its proportions. Use the inspector for exact dimensions, position, opacity, visibility, and locking. Raise or lower layers to arrange their stacking order.
7. Select an image, choose **Lasso** or **Smart Lasso**, then **Crop** to keep the selection. Shift adds and Left Alt subtracts. Right-click for **Separate to layer**, **Copy to layer**, or **Content-aware fill**. **Reset clip** restores the full source at its existing scale; **Copy cutout** copies the current crop with transparency. The dedicated **Crop** tool applies a rectangular crop directly.
8. Create shared presets in **Bubbles**, then use the **Bubble** tool (`T`) in chapters for dialogue and independently aimed pointers. **Brush** provides simple drawing and **Beat** adds a script beat where you click. Script and notes remain editing information and are not included in image exports.
9. Use **Read** to check the episode at a phone-like width; returning preserves your editing position. Save the complete story as `.scro`. **Export chapter** lets you choose WEBTOON, Tapas, or original width, then PNG or JPEG. It writes numbered slices and an export report into a new timestamped subfolder of your selected destination.

Saving your chosen `.scro` file is manual. The app prompts about unsaved changes before opening another story, starting a new one, or closing. Recovery copies are automatic and separate from that saved file. Replacing a reference image adds a new asset version so existing canvas instances remain intact.

Right-click artwork, a layer, character, reference, preset, location, chapter, or script beat for its actions. Text fields offer text editing commands. Menus follow the app's Glass/Solid preference.

## Recovering interrupted work

When a story has new unsaved changes, Scroll Studio saves a recovery snapshot on a background worker at roughly one-minute intervals. Unchanged stories are not repeatedly written, and a pointer drag defers the snapshot until it ends. The latest edits since the previous completed snapshot may still be lost if the app or computer stops.

On the next launch, available copies from inactive sessions are offered on the welcome screen. Use **File → Recover previous work** to find them later. **Restore copy** opens the complete story as an unsaved document; use Save to choose its destination. Recovery never writes over the original story. Hover **Auto recovery** in the status bar to see its state; failures are shown as **Recovery needs attention**.

On Windows the files live under `%LOCALAPPDATA%/Scroll Studio/recovery`. Each document session has its own locked directory. The app writes an immutable `.scro` snapshot and atomically commits a small metadata pointer, so a failed write preserves the previous copy. Live sessions are excluded from recovery discovery. A successful manual save clears that document's recovery copy; deliberate document discard also retires its session. Restored source snapshots are removed only after a successful save, and stale cleanup cannot remove a newer snapshot. Recovery is not a version-history backup system.

## Canvas and export presets

| Preset | Working canvas | Export |
| --- | --- | --- |
| WEBTOON working size | 1600 × 9600 px | 800 px wide, slices up to 1280 px tall |
| WEBTOON compact | 800 × 6400 px | 800 px wide, slices up to 1280 px tall |
| Tapas | 940 × 8000 px | 940 px wide, slices up to 1600 px tall |
| Full width export | Current chapter width | Slices up to 2000 px tall |

Chapter height can be extended in the inspector. Export scales proportionally and keeps the final slice's actual remaining height. The working heights are convenient starting points chosen for this app.

WEBTOON documents a maximum image size of **800 × 1280 px** and automatic slicing of longer uploads. [Official WEBTOON notice](https://www.webtoons.com/en/notice/detail?noticeNo=1766).

Tapas documents **940 px width** and no height limit for PNG/JPEG, with a 10 MB file limit. The app's 1600 px slice height is a practical export choice, not a Tapas requirement. [Official Tapas publishing guide](https://help.tapas.io/hc/en-us/articles/1260802028970-Series-Basics-How-to-publish-a-comic-episode-on-Tapas).

Export enforces per-image limits using conservative decimal bytes: less than 2,000,000 bytes for WEBTOON and less than 10,000,000 for Tapas. Oversized slices are split vertically without missing or repeating rows. JPEG keeps the quality you choose; the exporter does not silently lower it. Original-width export applies no platform byte limit.

For WEBTOON, the report warns if the chapter exceeds 100 images or 20,000,000 total image bytes. These warnings do not discard your output: choose JPEG, reduce its quality, or divide the chapter into shorter episodes. These limits come from the [official publishing checklist](https://webtoons-static.pstatic.net/creator101/en/pdf/Before-You-Publish-Checklist-2024.pdf?dt=2024011001). Check the destination's current requirements before publishing.

The completion window shows the result, and `export-report.txt` records file order, dimensions, byte counts, quality, warnings, and specification links. Rendering finishes in a staging directory before files are published; existing outputs are never overwritten, and failures clean up only this export's files. Finished chapters are flattened over white. The exporter does not publish episodes or generate series thumbnails.

## Shortcuts

| Shortcut | Action |
| --- | --- |
| Ctrl+, | Open/close Settings |
| Ctrl+N | New story |
| Ctrl+O | Open `.scro` story |
| Ctrl+S | Save |
| Ctrl+Shift+S | Save as |
| Ctrl+Shift+C | Copy story visual style |
| Ctrl+Z | Undo |
| Ctrl+Shift+Z or Ctrl+Y | Redo |
| Ctrl+V | Paste an image when not editing text |
| V | Move/select |
| M | Rectangular selection |
| C | Rectangular crop |
| W | Smart Lasso |
| E | Magic Eraser |
| Ctrl + mouse wheel | Zoom canvas under the cursor |
| J | Content-Aware Brush |
| O | Motion Blur brush |
| K | Glint |
| U | Wind |
| G / H | Background / Character Snippet |
| Shift / Left Alt while selecting | Add / subtract |
| L | Lasso |
| B | Brush |
| T | Add a dialogue bubble |
| P | Add script beat |
| S | Toggle snapping |
| Alt while moving | Temporarily bypass snapping |
| Delete | Delete the selected unlocked layer |
| Ctrl+D | Deselect pixels; retain the layer and applied crop |
| Escape | Cancel the current gesture |

Tool shortcuts are inactive while a text field is focused. Use the toolbar's **−**, **+**, and **Fit** controls for zoom; the canvas scrolls both vertically and horizontally.

## App settings and effects

Open **Settings** in the left navigation or use `Ctrl+,`. **Glass** enables a frosted floating tool shelf, while **Solid** uses opaque surfaces. **Frost** changes blur strength. **Motion** offers **Off**, **Calm**, and **Lively**; Off removes elastic feedback and custom transitions. **Reset** restores only the current settings category. **Done** or Escape returns to the workspace.

Preferences save after interaction settles and on exit, independently of the story. On Windows they live at `%LOCALAPPDATA%/Scroll Studio/settings.json`. Switching stories preserves these preferences. The canvas toolbar's Snap options and the Settings Canvas category stay synchronized.

Glass uses OpenGL 3.3 or OpenGL ES 3.0 when supported, with a tinted fallback if a driver cannot provide the effect. The Windows application has been verified with the Glow renderer; other operating systems have not been tested. No background animation runs when the interface is idle. Menus and controls retain keyboard focus and fixed pointer targets.

The visual pass follows [Apple's materials guidance](https://developer.apple.com/design/human-interface-guidelines/materials) for separating controls from content and its [motion guidance](https://developer.apple.com/design/human-interface-guidelines/motion) for brief, purposeful, optional animation. These are native Rust/egui effects, not an Apple platform component.

## File format

`.scro` format **12** is a ZIP container. Versions 1–11 still open and upgrade in memory. The first save that replaces an older file writes a backup beside it, named `<name>.v<version>.bak.scro` (for example `My story.v11.bak.scro`). An existing backup is left untouched. The status line names the backup that was written. Format 12 adds in-world timeline events — a title, an in-world date, a summary, a kind (`Flashback`, `Lore`, or `Other`), and optional links to characters, locations, or groups — ordered among chapters. Chapter reading order stays separate.

`.scro` version 6 introduced the ZIP container (version 1–5 files are upgraded in memory when opened):

```text
manifest.json          Story title, notes, chapters, script beats, layers,
                       asset metadata, stable IDs, and format version
assets/<id>.bin        Original encoded PNG, JPEG, or WebP bytes
```

Image bytes are stored once and referenced by ID. A character/background sheet may exist without an image. Scripts, notes, prompts, masks, drawing points, and dialogue are serialized in the manifest; a chapter is not a separate file.

The archive contains no external image-path dependencies. Saving writes a temporary file in the destination directory and replaces the previous archive only after a successful write. Loading validates the version, dimensions, IDs, references, entry names, and archive limits. Unknown future format versions are rejected. `.scro` is an application format, not PSD compatibility.

## Rendering and performance boundaries

- The interface repaints in response to interaction rather than continuously animating an idle canvas. Canvas artwork outside the visible region is skipped during painting.
- Preview decoding, resizing, masking, and text rasterization run on one worker. The UI uploads at most two completed textures per frame. The request queue holds four jobs and the completed-image queue holds two, preventing an unlimited backlog.
- Library thumbnails use at most 384 px on their longest side. Canvas previews use stable 384/768/1024/1536/2048 px buckets based on their screen size. GPU textures use a 64 MiB least-recently-used cache. Offscreen artwork is skipped. Original images remain available for full-resolution export. Text previews share export rasterization.
- Switching documents or undoing edits invalidates old preview results. Asset identities prevent a reused numeric ID from showing a stale image, and failed previews are cached instead of retried on every frame. Workers wake the UI when results are ready; previews require no continuous polling.
- Export runs on a worker thread and rasterizes one slice at a time. It never allocates an entire tall chapter bitmap. The decoded export-image cache is capped at 96 MiB.
- Cutout backing uses a distance grid capped at 2048 px on its longest side, computed in the preview worker. The foreground is never resized by this operation. Backing stays within the original image bounds; metadata crops can expand into available source padding.
- Lasso rasterization uses polygon scanlines. Brush export allocates coverage for the visible stroke bounds and avoids repeatedly darkening translucent joins.
- Images are limited to 64 MiB encoded, 32 megapixels, and 16,384 px per side. Story archives are limited to 512 MiB; manifests are limited to 16 MiB. These are validation limits, not guaranteed smooth-performance targets.
- Undo keeps up to 48 story snapshots. Encoded image data is shared rather than copied into every snapshot. Dense drawing histories and large asset libraries still increase memory use.
- Initial import validation, clipboard conversion, and opening archives can still briefly occupy the UI thread. This version does not implement disk-backed tile streaming or memory-budgeted undo. Recovery snapshots rewrite the complete archive on a worker; they are not incremental backups.

The brush is a simple fixed-size stroke tool without tablet pressure, erasing, or advanced brush engines. There is no PSD import/export, rotation, blend-mode system, color-management workflow, editable panel-border tool, or direct AI API integration. Lettering uses bundled Latin fonts; complex-script shaping and broad international font fallback are not implemented. Each bubble supports up to eight independently placed pointers. Freehand balloon outlines and mixed fonts within a single text layer are not implemented.

## Development checks

```powershell
& .\scripts\build.ps1 -Action test -CargoArgs @('--locked', '--all-targets')
& .\scripts\build.ps1 -Action clippy -CargoArgs @('--locked', '--all-targets', '--', '-D', 'warnings')
& .\scripts\build.ps1 -Action fmt -CargoArgs @('--', '--check')
```

Tests cover `.scro` round trips and invalid archives, recovery failures and concurrent sessions, queued cleanup, preview cache budgets and stale results, pointer gestures, polygon clipping, transparency, matching text previews, slice continuity, actual oversized PNG splitting, fixed JPEG quality, and export rollback.

## Research behind this update

The existing build's highest-impact gaps were manual-only protection, expensive preview preparation during repaint, and dimension-only exports. [Clip Studio's recovery and background-save documentation](https://help.clip-studio.com/en-us/manual_en/720_preferences/Preferences.htm) supports keeping recovery separate from deliberate saves and taking immutable background snapshots. Scroll Studio's one-minute interval is its own design choice.

Preview work follows egui's worker-safe `Context::request_repaint` contract (also verified in the pinned 0.31.1 source) and Rust's [bounded-channel behavior](https://doc.rust-lang.org/std/sync/mpsc/fn.sync_channel.html). Publishing checks use the official WEBTOON and Tapas documents linked above. These changes add no new third-party dependencies.

Lato, Balsamiq Sans, and the four earlier fonts use the SIL Open Font License. Komika Hand is supplied unchanged under its original Apostrophic Laboratories freeware notice, preserved in `assets/KomikaHand-LICENSE.txt`. All seven font licenses are included in the portable package. See [lettering research and font provenance](docs/font-research.md).

## Characters and story style

Open **Characters** for a searchable gallery of character cards. Open a card for its fixed sheet with **References**, **Appearance**, **Outfits**, **Palette**, **Identity**, **Notes**, and **Prompt** tabs. **All characters** returns to the gallery at the same scroll position. Select a field within Appearance, Identity, or Notes to edit it without moving the navigation. Notes includes consistency, avoid, continuity, and extra prompt instructions. Character sheets support up to 32 outfits and 16 palette colors. Choose **Use in prompt** on an outfit or switch it in the Prompt tab.

Add reference images with **+ Reference** (or **+ Add image** on an empty sheet), by dropping files onto the Characters workspace, or by pasting an image with `Ctrl+V` while no text field is focused. New images attach to the selected character without replacing earlier references. Each character supports an original image plus up to 24 additional references; use the thumbnail strip and paging buttons to switch images. Removing an added reference under **…** detaches it from the sheet; the original image remains in Images and on existing chapter layers.

Use **Copy sheet** for a labeled contact sheet on the clipboard or **Save PNG** for a portable image. Sheet preparation runs in the background, processes one full source at a time, and limits the output to 2048 pixels per side. The original images are kept at their imported resolution. **Place** adds an individual reference to the current chapter.

The header prompt selector offers **Portrait**, **Turnaround**, **Expressions**, or **Scene**. **Copy prompt** includes the current story visual style, supplied appearance details, active outfit, palette, consistency rules, reference notes, and custom instructions. The Prompt tab previews the result and edits a scene. Story-only backstory and relationships stay in the sheet rather than becoming accidental drawing instructions. Copy the prompt into your image generator and attach the reference sheet separately.

**Style** stays in the main toolbar. Define linework, palette, lighting, proportions, texture, and mood once; **Copy style** or `Ctrl+Shift+C` copies the base prompt on its own. Character, background, and script-beat prompt buttons build their text from the latest saved-in-memory style. Changing this text affects future copied instructions, not artwork already generated.

## Layer folders

The chapter inspector groups layers as **Bubbles & text**, **Characters**, **Backgrounds**, **Images**, **Brush strokes**, and **Shapes**. Character reference images are recognized as character artwork when placed. Each folder shows its count and can collapse or expand; **Collapse** and **Expand** control the whole list. Within folders, topmost artwork appears first. **Raise** and **Lower** still change the chapter's real stacking order across all types. Folder grouping is a list organization feature and does not change how the comic renders.

## Lettering and bubbles

Open **Bubbles** in the story library. Choose a starter preset or **+ Preset**. **Style** chooses the theme, shape, and size; **Text** controls the font, alignment, spacing, and fit; **Bubble** controls fill, outline, opacity, and pointer shape. The preview lets you try dialogue without changing chapter text. Every appearance edit immediately updates all bubbles linked to that preset throughout the story.

Choose **Use in chapter**, click the canvas, and type in the inspector. The Bubble toolbar selects the preset for new bubbles; the inspector can assign a different preset to an existing one. **Edit preset in Bubbles** opens its shared appearance. Appearance controls are not shown in chapters.

Use **+ Add pointer** for another speaker, then drag the numbered blue handle. Each bubble supports up to eight pointers. **Left**, **Up**, **Down**, and **Right** give quick directions; **Exact target** accepts signed chapter X/Y coordinates, including negative values above/left of the page. Pointers outside the page can still be edited, but image export clips to the chapter canvas. **Remove** removes one pointer without moving the others. Overlapping attachments form a single clean outline. Thought bubbles use dot trails; narration has no visible pointers.

Bubbles naturally grow and shrink in both dimensions around their dialogue while preserving each pointer target. **Fit bubble to text** enables automatic fitting; dragging a corner sets a manual size. **Placement** contains exact body dimensions and the automatic fitting toggle. Older stories remain manually sized until refitted or their preset appearance is changed. Existing text, pointer counts, and speaker targets are individual to each placed bubble.

The presets are based on [Blambot's professional lettering conventions](https://blambot.com/pages/comic-book-grammar-tradition), a [webtoon creator's explanation of bubble types](https://tips.clip-studio.com/en-us/articles/10886), and [Clip Studio's official balloon tool guide](https://tips.clip-studio.com/en-us/articles/863). A speech pointer suggests the speaker's mouth; thought bubbles use smaller circles toward the head. Whisper outlines are dashed, shouts use bursts, and narration uses a box without a pointer. These are editable storytelling conventions, not publishing requirements.

The new theme choices come from their visible lettering characteristics. The [official Solo Leveling publisher preview](https://yenpress.com/titles/9781975319434-solo-leveling-vol-1-comic) informed the compact capitals, centered lines, and oval balloons of Webtoon Classic. The publisher does not identify its exact typeface; Komika Hand is a stylistic choice, not a claim to be Solo Leveling's font. Mature uses restrained Lato; Casual uses the friendlier Balsamiq Sans. Missing smart punctuation in Komika uses readable plain punctuation for display while retaining the source text. The research document records sources, licenses, and the distinction between creator recommendations and platform-wide popularity.


### Browser Assets section

The companion Assets editor lives under the website’s `/studio/` section. It edits characters, locations/background references, groups, chronology, story style and the linked lore board. Import a `.scro`, edit its world, then export it for desktop use; chapter canvases and scripts stay intact. Original images are retained byte for byte, while galleries use smaller cached previews. Browser workspaces are temporary and expire after inactivity, so export to keep your work.

In desktop 0.13.1 and the companion editor, right-click a reference and choose **Use as thumbnail** to set its character, location or group card cover. Mobile exposes the same action in Reference options. The cover is shared with lore cards and saved in `.scro` without duplicating an image or changing reference order.
