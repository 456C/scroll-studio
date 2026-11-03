# Painting responsiveness — 0.13.0

Research and local measurements, 2 November 2026.

## What the published guidance supports

- [Procreate Brush Studio settings](https://help.procreate.com/procreate/handbook/5.4/brushes/brush-studio-settings) describes stroke spacing, stabilization, and pressure control of brush size and paint behavior. These support exposing a few direct brush controls with optional dynamics. It does not disclose Procreate's proprietary rendering implementation.
- [Clip Studio brush customization](https://help.clip-studio.com/en-us/manual_en/240_brushes/Customizing_brush_tools.htm) separates common brush properties from deeper controls. Scroll Studio keeps size, opacity and smoothing visible; flow, softness, spacing and pressure response are in Brush settings.
- [Clip Studio preferences](https://help.clip-studio.com/en-us/manual_en/720_preferences/Preferences.htm) distinguishes cursor responsiveness, stabilization and stroke preview. [Its stroke-prediction FAQ](https://support.clip-studio.com/en-us/faq/articles/20250039) explains device-dependent predictive preview. Scroll Studio does not invent predicted permanent pixels: smoothing starts at zero, and actual input samples define the stroke.
- [Apple's coalesced-touch guidance](https://developer.apple.com/documentation/uikit/getting-high-fidelity-input-with-coalesced-touches) explains why drawing should preserve intermediate input samples. The equivalent change here processes all pointer events delivered by egui/winit, in order, including multiple complete strokes in a single frame. This cannot recover samples a driver never delivered.

## Implemented

The CPU brush kernel uses a small cache of 16-bit coverage masks, subpixel interpolation, symmetric tip generation, integer alpha composition, and dirty bounds within 128 px tiles. Pressure changes size and/or flow through an adjustable response curve. Round tips calculate one quadrant and mirror it, reducing the cost of changing sizes. Opacity caps a whole stroke; flow controls buildup within it.

The GPU keeps the existing texture handle and receives only each changed rectangle. Viewport rendering culls unseen tiles; images and PNG saves retain original dimensions. Undo records changed tiles with a 96 MiB/64-stroke history budget, allowing one larger stroke to remain undoable. No browser runtime, AI service, or new dependency was added.

## Reproducible CPU measurement

Run the release example: ./scripts/build.ps1 -Action run -Release -CargoArgs @('--example','brush_profile')

Intel Core Ultra 9 275HX, Windows x64, optimized build. Each case paints 120 batches of eight dabs on a 4096 × 4096 transparent image, following the same path. This measures the CPU brush rasterizer, including lazy tip generation and touched-tile snapshots. It excludes device sampling, event scheduling, texture upload, display refresh, and time to finish history. These are one-machine observations, not a universal latency guarantee.

| Tip, size | 0.12 median / p95 (ms) | 0.13 median / p95 (ms) |
| --- | ---: | ---: |
| Ink, 16 px | 0.174 / 0.403 | 0.188 / 0.460 |
| Ink, 128 px | 3.632 / 4.062 | 1.413 / 2.045 |
| Ink, 256 px | 12.354 / 13.609 | 4.607 / 6.278 |
| Airbrush, 16 px | 0.174 / 0.416 | 0.171 / 0.471 |
| Airbrush, 128 px | 4.108 / 4.551 | 1.395 / 1.788 |
| Airbrush, 256 px | 17.706 / 21.984 | 4.928 / 6.029 |

The final benchmark also varies tip diameter continuously between 30% and 100% to exercise pressure-driven cache misses. At a maximum of 256 px: Ink median 3.012 ms, p95 5.772 ms; Airbrush median 3.917 ms, p95 6.683 ms. The varying-size cases paint fewer pixels than the fixed-size cases and should not be compared as equivalent work.

## Validation and limits

Regression checks cover batched versus separately delivered events, corners and separated strokes, pressure width, antialiasing, stroke opacity, exact undo pixels, and partial texture updates without handle replacement. Mouse drawing and compact layouts were checked in the native interface.

Physical pressure-tablet hardware and end-to-end pen-to-display latency have not been tested. Windows pen pressure is consumed when delivered by the existing winit/egui backend; driver support still matters. Image Editing remains a focused single-image painter with five built-in tips, not a claim of complete Procreate or Clip Studio feature parity.
