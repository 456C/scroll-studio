//! Bounded CPU compositing for previews and export. Export rasterizes one slice at
//! a time; a long chapter never requires a chapter-sized RGBA allocation.

use std::{
    collections::HashMap,
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    sync::Arc,
};

use ab_glyph::{point, Font, FontArc, ScaleFont};
use anyhow::{bail, ensure, Context, Result};
use image::{ImageFormat, ImageReader, Rgba, RgbaImage};

use crate::model::{AssetBytes, Chapter, Layer, LayerKind};

pub const FONT_BYTES: &[u8] = include_bytes!("../assets/Karla.ttf");
mod lettering;
pub use lettering::{
    fit_text_height, fit_text_size, font_bytes, text_bounds, text_hit_test, text_overflows,
};
const MAX_IMAGE_PIXELS: u64 = 32_000_000;
const MAX_IMAGE_SIDE: u32 = 16_384;
const MAX_ENCODED_BYTES: usize = 64 * 1024 * 1024;
const MAX_CACHE_BYTES: usize = 96 * 1024 * 1024;

/// Decode supported interchange images, checking dimensions before allocating.
pub fn decode_image(bytes: &[u8]) -> Result<RgbaImage> {
    ensure!(
        bytes.len() <= MAX_ENCODED_BYTES,
        "Image exceeds the 64 MB import limit"
    );
    let reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    ensure!(
        matches!(
            reader.format(),
            Some(ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::WebP)
        ),
        "Use a PNG, JPEG, or WebP image"
    );
    let (width, height) = reader
        .into_dimensions()
        .context("Cannot read image dimensions")?;
    ensure!(
        width > 0 && height > 0 && width <= MAX_IMAGE_SIDE && height <= MAX_IMAGE_SIDE,
        "Images must be between 1 and 16,384 pixels on each side"
    );
    ensure!(
        u64::from(width) * u64::from(height) <= MAX_IMAGE_PIXELS,
        "Image exceeds the 32 megapixel import limit; resize it before importing"
    );
    let mut reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_SIDE);
    limits.max_image_height = Some(MAX_IMAGE_SIDE);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    Ok(reader.decode().context("Cannot decode image")?.to_rgba8())
}

/// Even-odd polygon fill, including its boundary. Empty polygons mean no mask.
pub fn point_in_polygon(x: f32, y: f32, polygon: &[[f32; 2]]) -> bool {
    if polygon.is_empty() {
        return true;
    }
    if polygon.len() < 3 || !x.is_finite() || !y.is_finite() {
        return false;
    }
    let mut inside = false;
    let mut a = polygon[polygon.len() - 1];
    for &b in polygon {
        if !a[0].is_finite() || !a[1].is_finite() || !b[0].is_finite() || !b[1].is_finite() {
            return false;
        }
        let cross = (x - a[0]) * (b[1] - a[1]) - (y - a[1]) * (b[0] - a[0]);
        if cross.abs() <= 1e-7
            && x >= a[0].min(b[0])
            && x <= a[0].max(b[0])
            && y >= a[1].min(b[1])
            && y <= a[1].max(b[1])
        {
            return true;
        }
        if (a[1] > y) != (b[1] > y) && x < (b[0] - a[0]) * (y - a[1]) / (b[1] - a[1]) + a[0] {
            inside = !inside;
        }
        a = b;
    }
    inside
}

/// Intersect a normalized lasso with the image bounds, preserving crossings
/// instead of moving every outside vertex onto an edge. An empty result means
/// there is no usable selection; callers should keep the previous mask.
pub fn clip_polygon_to_unit_square(polygon: &[[f32; 2]]) -> Vec<[f32; 2]> {
    if polygon.len() < 3 || !polygon.iter().flatten().all(|n| n.is_finite()) {
        return Vec::new();
    }
    let mut points = polygon.to_vec();
    for (axis, edge, keep_greater) in [
        (0, 0.0, true),
        (0, 1.0, false),
        (1, 0.0, true),
        (1, 1.0, false),
    ] {
        if points.is_empty() {
            break;
        }
        let mut output = Vec::with_capacity(points.len() + 4);
        let inside = |p: [f32; 2]| {
            if keep_greater {
                p[axis] >= edge
            } else {
                p[axis] <= edge
            }
        };
        let mut a = points[points.len() - 1];
        let mut a_inside = inside(a);
        for &b in &points {
            let b_inside = inside(b);
            if a_inside != b_inside {
                let t = (edge - a[axis]) / (b[axis] - a[axis]);
                let mut crossing = [a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])];
                crossing[axis] = edge;
                output.push(crossing);
            }
            if b_inside {
                output.push(b);
            }
            a = b;
            a_inside = b_inside;
        }
        points = output;
    }
    points.dedup_by(|a, b| (a[0] - b[0]).abs() < 1e-6 && (a[1] - b[1]).abs() < 1e-6);
    if points.len() > 1 {
        let first = points[0];
        let last = points[points.len() - 1];
        if (first[0] - last[0]).abs() < 1e-6 && (first[1] - last[1]).abs() < 1e-6 {
            points.pop();
        }
    }
    if points.len() < 3 {
        Vec::new()
    } else {
        points
    }
}

/// Transparent preview of exactly the text/bubble used by the exporter.
/// Position is ignored. Opacity is already baked in: display with a white tint.
/// The longest side is capped at 2048 pixels to bound the preview allocation.
pub fn text_preview(layer: &Layer) -> Result<RgbaImage> {
    text_preview_at_scale(layer, 1.0)
}

/// Rasterize lettering at its displayed physical scale, preserving document
/// line breaks and pointer geometry. The allocation still has a 2048 px cap.
pub fn text_preview_at_scale(layer: &Layer, scale: f32) -> Result<RgbaImage> {
    ensure!(scale.is_finite() && scale > 0.0, "Invalid preview scale");
    ensure!(
        layer.width.is_finite()
            && layer.height.is_finite()
            && layer.width > 0.0
            && layer.height > 0.0,
        "Invalid text dimensions"
    );
    if !matches!(layer.kind, LayerKind::Text { .. }) {
        bail!("Text preview requires a text layer");
    }
    let [x, y, w, h] = text_bounds(layer);
    ensure!(
        w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0,
        "Invalid text bounds"
    );
    let scale = (2048.0 / w.max(h)).min(scale);
    let width = (w * scale).ceil().clamp(1.0, 2048.0) as u32;
    let height = (h * scale).ceil().clamp(1.0, 2048.0) as u32;
    let mut image = RgbaImage::new(width, height);
    let mut local = layer.clone();
    local.x -= x;
    local.y -= y;
    lettering::draw_text(&mut image, 0, scale, &local)?;
    Ok(image)
}

/// Return a clipped copy. The source stays intact for future mask edits.
pub fn masked_image(image: &RgbaImage, polygon: &[[f32; 2]]) -> RgbaImage {
    let mut result = image.clone();
    mask_image(&mut result, polygon, false);
    result
}

/// Clip an already premultiplied preview in place. Coverage must affect both
/// RGB and alpha; changing alpha alone would leave bright edges in Glow.
pub fn mask_premultiplied_image(image: &mut RgbaImage, polygon: &[[f32; 2]]) {
    mask_image(image, polygon, true);
}

fn mask_image(image: &mut RgbaImage, polygon: &[[f32; 2]], premultiplied: bool) {
    if polygon.is_empty() {
        return;
    }
    let w = image.width() as f32;
    let h = image.height() as f32;
    let mut row = vec![0u8; image.width() as usize];
    let mut intersections = Vec::new();
    for y in 0..image.height() {
        mask_row(
            &mut row,
            0.0,
            w,
            0.0,
            [(y as f32 + 0.25) / h, (y as f32 + 0.75) / h],
            polygon,
            &mut intersections,
        );
        for x in 0..image.width() {
            let pixel = image.get_pixel_mut(x, y);
            let first = if premultiplied { 0 } else { 3 };
            for value in &mut pixel.0[first..] {
                *value = ((u16::from(*value) * u16::from(row[x as usize]) + 2) / 4) as u8;
            }
        }
    }
}

/// Rasterize two scanlines with two horizontal samples each. The work grows
/// with polygon edges per row, rather than edges multiplied by every pixel.
fn mask_row(
    row: &mut [u8],
    output_x: f32,
    polygon_scale: f32,
    polygon_x: f32,
    ys: [f32; 2],
    polygon: &[[f32; 2]],
    intersections: &mut Vec<f32>,
) {
    row.fill(0);
    if polygon.len() < 3 || !polygon.iter().flatten().all(|n| n.is_finite()) {
        return;
    }
    for y in ys {
        intersections.clear();
        let mut a = polygon[polygon.len() - 1];
        for &b in polygon {
            if (a[1] > y) != (b[1] > y) {
                let x = a[0] + (y - a[1]) * (b[0] - a[0]) / (b[1] - a[1]);
                intersections.push(x * polygon_scale + polygon_x - output_x);
            }
            a = b;
        }
        intersections.sort_unstable_by(f32::total_cmp);
        for pair in intersections.as_chunks::<2>().0 {
            let start = pair[0].floor().max(0.0).min(row.len() as f32) as usize;
            let end = pair[1].ceil().max(0.0).min(row.len() as f32) as usize;
            for (x, value) in row.iter_mut().enumerate().take(end).skip(start) {
                for offset in [0.25, 0.75] {
                    let sample = x as f32 + offset;
                    *value += u8::from(sample >= pair[0] && sample < pair[1]);
                }
            }
        }
    }
}

#[derive(Default)]
pub(crate) struct DecodeCache {
    images: HashMap<u64, Arc<RgbaImage>>,
    clipped: HashMap<(u64, usize), (Arc<crate::clipping::ImageClip>, Arc<RgbaImage>)>,
    bytes: usize,
}

impl DecodeCache {
    fn image(&mut self, id: u64, assets: &AssetBytes) -> Result<Arc<RgbaImage>> {
        if let Some(image) = self.images.get(&id) {
            return Ok(image.clone());
        }
        let bytes = assets
            .get(&id)
            .with_context(|| format!("Missing image asset {id}"))?;
        let image =
            Arc::new(decode_image(bytes).with_context(|| format!("Cannot decode asset {id}"))?);
        let size = image.as_raw().len();
        if self.bytes.saturating_add(size) > MAX_CACHE_BYTES {
            self.images.clear();
            self.clipped.clear();
            self.bytes = 0;
        }
        if size <= MAX_CACHE_BYTES {
            self.bytes += size;
            self.images.insert(id, image.clone());
        }
        Ok(image)
    }

    fn clipped_image(
        &mut self,
        id: u64,
        assets: &AssetBytes,
        clip: &Arc<crate::clipping::ImageClip>,
    ) -> Result<Arc<RgbaImage>> {
        let key = (id, Arc::as_ptr(clip) as usize);
        if let Some((_, image)) = self.clipped.get(&key) {
            return Ok(image.clone());
        }
        let bytes = assets.get(&id).context("Missing clipped image")?;
        let mut image = decode_image(bytes)?;
        crate::clipping::apply(&mut image, clip)?;
        let image = Arc::new(image);
        let size = image.as_raw().len();
        if self.bytes.saturating_add(size) > MAX_CACHE_BYTES {
            self.images.clear();
            self.clipped.clear();
            self.bytes = 0;
        }
        if size <= MAX_CACHE_BYTES {
            self.bytes += size;
            self.clipped.insert(key, (clip.clone(), image.clone()));
        }
        Ok(image)
    }
}

pub(crate) fn output_height(chapter: &Chapter, width: u32) -> Result<u32> {
    ensure!(
        chapter.width > 0 && chapter.height > 0 && width > 0 && width <= 8192,
        "Invalid canvas/export dimensions"
    );
    let height = (u64::from(chapter.height) * u64::from(width)).div_ceil(u64::from(chapter.width));
    ensure!(height <= u64::from(u32::MAX), "Export is too tall");
    Ok(height as u32)
}

/// Render a strip. `y` and `height` are OUTPUT pixels, after scaling to
/// `output_width`; this keeps adjacent exports seamless at non-integer scales.
pub fn render_region(
    chapter: &Chapter,
    assets: &AssetBytes,
    y: u32,
    height: u32,
    output_width: u32,
) -> Result<RgbaImage> {
    render_with_cache(
        chapter,
        assets,
        y,
        height,
        output_width,
        &mut DecodeCache::default(),
    )
}

pub(crate) fn render_with_cache(
    chapter: &Chapter,
    assets: &AssetBytes,
    y: u32,
    height: u32,
    width: u32,
    cache: &mut DecodeCache,
) -> Result<RgbaImage> {
    let total_height = output_height(chapter, width)?;
    ensure!(
        height > 0 && y < total_height && height <= total_height - y,
        "Export region lies outside the chapter"
    );
    ensure!(
        u64::from(width) * u64::from(height) <= MAX_IMAGE_PIXELS,
        "Export slice exceeds 32 megapixels"
    );
    let mut output = RgbaImage::from_pixel(width, height, Rgba([255, 255, 255, 255]));
    let scale = width as f32 / chapter.width as f32;
    for layer in &chapter.layers {
        if !layer.visible || layer.opacity <= 0.0 {
            continue;
        }
        ensure!(
            layer.opacity.is_finite()
                && layer.x.is_finite()
                && layer.y.is_finite()
                && layer.width.is_finite()
                && layer.height.is_finite(),
            "Layer contains invalid coordinates"
        );
        match &layer.kind {
            LayerKind::Effect { effect } => {
                if intersects(layer, scale, y, height) {
                    for line in effect.lines(layer.width, layer.height) {
                        draw_stroke(
                            &mut output,
                            y,
                            scale,
                            layer,
                            &line.points,
                            line.color,
                            line.width,
                        );
                    }
                }
            }
            LayerKind::Image {
                asset_id,
                mask,
                clip,
                source_rect: _,
            } => {
                if !intersects(layer, scale, y, height) || layer.width <= 0.0 || layer.height <= 0.0
                {
                    continue;
                }
                let image = if let Some(clip) = clip {
                    cache.clipped_image(*asset_id, assets, clip)?
                } else {
                    cache.image(*asset_id, assets)?
                };
                draw_image(&mut output, y, scale, layer, &image, mask);
            }
            // Placeholders are for the editing view only; exports skip unknown kinds.
            LayerKind::Unknown { .. } => {}
            LayerKind::Shape { color } => {
                if intersects(layer, scale, y, height) {
                    draw_rect(&mut output, y, scale, layer, *color);
                }
            }
            LayerKind::Stroke {
                points,
                color,
                size,
            } => draw_stroke(&mut output, y, scale, layer, points, *color, *size),
            LayerKind::Text { .. } => {
                let [_, top, _, h] = text_bounds(layer);
                if (top + h) * scale >= y as f32 && top * scale <= (y + height) as f32 {
                    let rendered = crate::bubbles::joined_layer(layer, &chapter.layers);
                    lettering::draw_text(&mut output, y, scale, &rendered)?;
                }
            }
        }
    }
    Ok(output)
}

fn intersects(layer: &Layer, scale: f32, y: u32, height: u32) -> bool {
    (layer.y + layer.height) * scale >= y as f32 && layer.y * scale <= (y + height) as f32
}

fn blend(dst: &mut Rgba<u8>, color: [u8; 4], opacity: f32) {
    let alpha = (color[3] as f32 / 255.0 * opacity).clamp(0.0, 1.0);
    if alpha <= 0.0 {
        return;
    }
    let da = dst[3] as f32 / 255.0;
    let out_alpha = alpha + da * (1.0 - alpha);
    for c in 0..3 {
        dst[c] = ((color[c] as f32 * alpha + dst[c] as f32 * da * (1.0 - alpha)) / out_alpha)
            .round() as u8;
    }
    dst[3] = (out_alpha * 255.0).round() as u8;
}

fn bounds(output: &RgbaImage, y: u32, scale: f32, layer: &Layer) -> (u32, u32, u32, u32) {
    let x0 = (layer.x * scale)
        .floor()
        .max(0.0)
        .min(output.width() as f32) as u32;
    let x1 = ((layer.x + layer.width) * scale)
        .ceil()
        .max(0.0)
        .min(output.width() as f32) as u32;
    let y0 = (layer.y * scale - y as f32)
        .floor()
        .max(0.0)
        .min(output.height() as f32) as u32;
    let y1 = ((layer.y + layer.height) * scale - y as f32)
        .ceil()
        .max(0.0)
        .min(output.height() as f32) as u32;
    (x0, y0, x1, y1)
}

fn draw_rect(output: &mut RgbaImage, y: u32, scale: f32, layer: &Layer, color: [u8; 4]) {
    let (x0, y0, x1, y1) = bounds(output, y, scale, layer);
    for py in y0..y1 {
        for px in x0..x1 {
            blend(output.get_pixel_mut(px, py), color, layer.opacity);
        }
    }
}

fn sample_bilinear(image: &RgbaImage, u: f32, v: f32) -> [u8; 4] {
    let x = (u * image.width() as f32 - 0.5).clamp(0.0, (image.width() - 1) as f32);
    let y = (v * image.height() as f32 - 0.5).clamp(0.0, (image.height() - 1) as f32);
    let x0 = x.floor() as u32;
    let y0 = y.floor() as u32;
    let fx = x - x0 as f32;
    let fy = y - y0 as f32;
    let x1 = (x0 + 1).min(image.width() - 1);
    let y1 = (y0 + 1).min(image.height() - 1);
    let mut rgba = [0.0; 4];
    for (sx, sy, weight) in [
        (x0, y0, (1.0 - fx) * (1.0 - fy)),
        (x1, y0, fx * (1.0 - fy)),
        (x0, y1, (1.0 - fx) * fy),
        (x1, y1, fx * fy),
    ] {
        let pixel = image.get_pixel(sx, sy);
        let alpha = pixel[3] as f32 / 255.0;
        for c in 0..3 {
            rgba[c] += pixel[c] as f32 * alpha * weight;
        }
        rgba[3] += alpha * weight;
    }
    if rgba[3] <= f32::EPSILON {
        return [0; 4];
    }
    [
        (rgba[0] / rgba[3]).round() as u8,
        (rgba[1] / rgba[3]).round() as u8,
        (rgba[2] / rgba[3]).round() as u8,
        (rgba[3] * 255.0).round() as u8,
    ]
}

fn draw_image(
    output: &mut RgbaImage,
    y: u32,
    scale: f32,
    layer: &Layer,
    image: &RgbaImage,
    mask: &[[f32; 2]],
) {
    let (x0, y0, x1, y1) = bounds(output, y, scale, layer);
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let mut row = vec![4u8; (x1 - x0) as usize];
    let mut intersections = Vec::new();
    for py in y0..y1 {
        if !mask.is_empty() {
            let y_samples = [0.25, 0.75]
                .map(|dy| ((py as f32 + y as f32 + dy) / scale - layer.y) / layer.height);
            mask_row(
                &mut row,
                x0 as f32,
                layer.width * scale,
                layer.x * scale,
                y_samples,
                mask,
                &mut intersections,
            );
        }
        for px in x0..x1 {
            let u = ((px as f32 + 0.5) / scale - layer.x) / layer.width;
            let v = ((py as f32 + y as f32 + 0.5) / scale - layer.y) / layer.height;
            if !(0.0..=1.0).contains(&u) || !(0.0..=1.0).contains(&v) {
                continue;
            }
            let (u, v) = if let LayerKind::Image {
                source_rect: Some([x, y, w, h]),
                ..
            } = &layer.kind
            {
                (*x + u * *w, *y + v * *h)
            } else {
                (u, v)
            };
            let coverage = row[(px - x0) as usize] as f32 / 4.0;
            if coverage > 0.0 {
                blend(
                    output.get_pixel_mut(px, py),
                    sample_bilinear(image, u, v),
                    layer.opacity * coverage,
                );
            }
        }
    }
}

fn draw_stroke(
    output: &mut RgbaImage,
    y: u32,
    scale: f32,
    layer: &Layer,
    points: &[[f32; 2]],
    color: [u8; 4],
    size: f32,
) {
    if points.is_empty() || !size.is_finite() || size <= 0.0 {
        return;
    }
    let radius = size * scale / 2.0;
    let transform = |p: [f32; 2]| {
        [
            (p[0] + layer.x) * scale,
            (p[1] + layer.y) * scale - y as f32,
        ]
    };
    let mut min = [f32::INFINITY; 2];
    let mut max = [f32::NEG_INFINITY; 2];
    for &p in points {
        let p = transform(p);
        if !p.iter().all(|n| n.is_finite()) {
            continue;
        }
        for axis in 0..2 {
            min[axis] = min[axis].min(p[axis]);
            max[axis] = max[axis].max(p[axis]);
        }
    }
    let bx = (min[0] - radius - 1.0)
        .floor()
        .max(0.0)
        .min(output.width() as f32) as u32;
    let by = (min[1] - radius - 1.0)
        .floor()
        .max(0.0)
        .min(output.height() as f32) as u32;
    let ex = (max[0] + radius + 1.0)
        .ceil()
        .max(0.0)
        .min(output.width() as f32) as u32;
    let ey = (max[1] + radius + 1.0)
        .ceil()
        .max(0.0)
        .min(output.height() as f32) as u32;
    if bx >= ex || by >= ey {
        return;
    }
    let coverage_width = (ex - bx) as usize;
    // Max coverage avoids dark joins in translucent brush strokes.
    let mut coverage = vec![0u8; coverage_width * (ey - by) as usize];
    for index in 0..points.len() {
        let a = transform(points[index.saturating_sub(1)]);
        let b = transform(points[index]);
        if !a.iter().chain(b.iter()).all(|n| n.is_finite()) {
            continue;
        }
        let left = (a[0].min(b[0]) - radius - 1.0)
            .floor()
            .max(0.0)
            .min(output.width() as f32) as u32;
        let right = (a[0].max(b[0]) + radius + 1.0)
            .ceil()
            .max(0.0)
            .min(output.width() as f32) as u32;
        let top = (a[1].min(b[1]) - radius - 1.0)
            .floor()
            .max(0.0)
            .min(output.height() as f32) as u32;
        let bottom = (a[1].max(b[1]) + radius + 1.0)
            .ceil()
            .max(0.0)
            .min(output.height() as f32) as u32;
        let dx = b[0] - a[0];
        let dy = b[1] - a[1];
        let len2 = dx * dx + dy * dy;
        for py in top..bottom {
            for px in left..right {
                let cx = px as f32 + 0.5;
                let cy = py as f32 + 0.5;
                let t = if len2 > 0.0 {
                    ((cx - a[0]) * dx + (cy - a[1]) * dy) / len2
                } else {
                    0.0
                }
                .clamp(0.0, 1.0);
                let distance = ((cx - a[0] - t * dx).powi(2) + (cy - a[1] - t * dy).powi(2)).sqrt();
                let value = ((radius + 0.5 - distance).clamp(0.0, 1.0) * 255.0).round() as u8;
                let offset = (py - by) as usize * coverage_width + (px - bx) as usize;
                coverage[offset] = coverage[offset].max(value);
            }
        }
    }
    for (index, coverage) in coverage.into_iter().enumerate() {
        if coverage > 0 {
            let px = bx + (index % coverage_width) as u32;
            let py = by + (index / coverage_width) as u32;
            blend(
                output.get_pixel_mut(px, py),
                color,
                layer.opacity * coverage as f32 / 255.0,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_legacy_text(
    output: &mut RgbaImage,
    y: u32,
    scale: f32,
    layer: &Layer,
    text: &str,
    color: [u8; 4],
    size: f32,
    bubble: bool,
) -> Result<()> {
    if !size.is_finite() || size <= 0.0 || layer.width <= 0.0 || layer.height <= 0.0 {
        return Ok(());
    }
    let font = FontArc::try_from_slice(FONT_BYTES).context("Cannot load bundled font")?;
    let font = font.as_scaled(size * scale);
    let padding = if bubble { 12.0 } else { 0.0 };
    let left = (layer.x + padding) * scale;
    let right = (layer.x + layer.width - padding) * scale;
    let top = (layer.y + padding) * scale;
    let bottom = (layer.y + layer.height - padding) * scale;
    if bubble {
        let (x0, y0, x1, y1) = bounds(output, y, scale, layer);
        let radius = (20.0 * scale)
            .min(layer.width * scale / 2.0)
            .min(layer.height * scale / 2.0)
            .max(0.0);
        let l = layer.x * scale;
        let t = layer.y * scale;
        let r = (layer.x + layer.width) * scale;
        let b = (layer.y + layer.height) * scale;
        for py in y0..y1 {
            for px in x0..x1 {
                let cx = px as f32 + 0.5;
                let cy = py as f32 + y as f32 + 0.5;
                let nx = cx.clamp(l + radius, r - radius);
                let ny = cy.clamp(t + radius, b - radius);
                let edge = radius - ((cx - nx).powi(2) + (cy - ny).powi(2)).sqrt();
                let coverage = (edge + 0.5).clamp(0.0, 1.0);
                if coverage > 0.0 {
                    let border_distance = edge.min(cx - l).min(r - cx).min(cy - t).min(b - cy);
                    let fill = if border_distance < 2.0 * scale {
                        [35, 35, 40, 255]
                    } else {
                        [255, 255, 255, 255]
                    };
                    blend(output.get_pixel_mut(px, py), fill, layer.opacity * coverage);
                }
            }
        }
    }
    let line_height = size * scale * 1.2;
    let mut baseline = top + font.ascent();
    let mut cursor_x = left;
    let mut previous = None;
    for paragraph in text.split('\n') {
        for word in paragraph.split_inclusive(char::is_whitespace) {
            let word_width: f32 = word
                .chars()
                .map(|ch| font.h_advance(font.glyph_id(ch)))
                .sum();
            if cursor_x > left && cursor_x + word_width > right {
                baseline += line_height;
                cursor_x = left;
                previous = None;
            }
            for ch in word.chars() {
                if ch.is_control() {
                    continue;
                }
                let id = font.glyph_id(ch);
                let advance = font.h_advance(id);
                if cursor_x > left && cursor_x + advance > right {
                    baseline += line_height;
                    cursor_x = left;
                    previous = None;
                }
                if let Some(prev) = previous {
                    cursor_x += font.kern(prev, id);
                }
                if baseline - font.ascent() < bottom {
                    let glyph = id.with_scale_and_position(
                        size * scale,
                        point(cursor_x, baseline - y as f32),
                    );
                    if let Some(outline) = font.outline_glyph(glyph) {
                        let bounds = outline.px_bounds();
                        outline.draw(|gx, gy, coverage| {
                            let px = bounds.min.x as i32 + gx as i32;
                            let py = bounds.min.y as i32 + gy as i32;
                            if px >= 0
                                && py >= 0
                                && px < output.width() as i32
                                && py < output.height() as i32
                                && px as f32 >= left
                                && (px as f32) < right
                                && (py as f32 + y as f32) >= top
                                && (py as f32 + y as f32) < bottom
                            {
                                blend(
                                    output.get_pixel_mut(px as u32, py as u32),
                                    color,
                                    layer.opacity * coverage,
                                );
                            }
                        });
                    }
                }
                cursor_x += advance;
                previous = Some(id);
            }
        }
        baseline += line_height;
        cursor_x = left;
        previous = None;
    }
    Ok(())
}

/// Export sequential PNG slices without allocating the full chapter. Existing
/// output files are never replaced, and a failed export removes its own partials.
pub fn export_chapter(
    chapter: &Chapter,
    assets: &AssetBytes,
    directory: &Path,
    width: u32,
    slice_height: u32,
) -> Result<Vec<PathBuf>> {
    ensure!(
        slice_height > 0 && slice_height <= MAX_IMAGE_SIDE,
        "Invalid slice height"
    );
    let total = output_height(chapter, width)?;
    ensure!(
        u64::from(width) * u64::from(slice_height) <= MAX_IMAGE_PIXELS,
        "Export slice exceeds 32 megapixels"
    );
    let count = total.div_ceil(slice_height);
    ensure!(
        count <= 10_000,
        "Export would produce more than 10,000 files"
    );
    fs::create_dir_all(directory).context("Cannot create export directory")?;
    let paths: Vec<PathBuf> = (1..=count)
        .map(|i| directory.join(format!("chapter-{i:03}.png")))
        .collect();
    for path in &paths {
        ensure!(
            !path.exists(),
            "Export file already exists: {}. Choose an empty folder.",
            path.display()
        );
    }
    let mut created = Vec::new();
    let mut cache = DecodeCache::default();
    let result: Result<()> = (|| {
        for (index, path) in paths.iter().enumerate() {
            let y = index as u32 * slice_height;
            let image = render_with_cache(
                chapter,
                assets,
                y,
                slice_height.min(total - y),
                width,
                &mut cache,
            )?;
            let file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .with_context(|| format!("Cannot create {}", path.display()))?;
            created.push(path.clone());
            let mut writer = std::io::BufWriter::new(file);
            image.write_to(&mut writer, ImageFormat::Png)?;
            use std::io::Write;
            writer.flush()?;
        }
        Ok(())
    })();
    if let Err(error) = result {
        for path in &created {
            let _ = fs::remove_file(path);
        }
        bail!("Export failed: {error:#}");
    }
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Chapter, Layer};

    fn chapter() -> Chapter {
        Chapter {
            extensions: Default::default(),
            id: 1,
            name: "Test".into(),
            width: 16,
            height: 32,
            panels: vec![],
            layers: vec![],
        }
    }
    fn shape(id: u64, x: f32, y: f32, w: f32, h: f32, color: [u8; 4]) -> Layer {
        Layer {
            extensions: Default::default(),
            id,
            name: "Shape".into(),
            visible: true,
            locked: false,
            opacity: 1.0,
            x,
            y,
            width: w,
            height: h,
            kind: LayerKind::Shape { color },
        }
    }
    fn encoded(image: RgbaImage) -> Arc<Vec<u8>> {
        let mut bytes = Cursor::new(Vec::new());
        image.write_to(&mut bytes, ImageFormat::Png).unwrap();
        Arc::new(bytes.into_inner())
    }

    #[test]
    fn polygon_handles_concavity_boundary_and_empty() {
        let polygon = [
            [0., 0.],
            [1., 0.],
            [1., 0.4],
            [0.4, 0.4],
            [0.4, 1.],
            [0., 1.],
        ];
        assert!(point_in_polygon(0.2, 0.8, &polygon));
        assert!(!point_in_polygon(0.8, 0.8, &polygon));
        assert!(point_in_polygon(0., 0.5, &polygon));
        assert!(point_in_polygon(0.8, 0.8, &[]));
        assert!(!point_in_polygon(0., 0., &[[0., 0.]]));
    }

    #[test]
    fn clipped_crossings_and_disconnected_concave_areas_preserve_selection() {
        for polygon in [
            vec![[-1.0, 0.0], [0.0, 2.0], [2.0, 0.0]],
            vec![
                [-0.5, -0.5],
                [0.2, -0.5],
                [0.2, 1.5],
                [0.8, 1.5],
                [0.8, -0.5],
                [1.5, -0.5],
                [1.5, 2.0],
                [-0.5, 2.0],
            ],
        ] {
            let clipped = clip_polygon_to_unit_square(&polygon);
            assert!(clipped.iter().flatten().all(|n| (0.0..=1.0).contains(n)));
            for y in 0..20 {
                for x in 0..20 {
                    let x = (x as f32 + 0.5) / 20.0;
                    let y = (y as f32 + 0.5) / 20.0;
                    assert_eq!(
                        point_in_polygon(x, y, &polygon),
                        point_in_polygon(x, y, &clipped),
                        "clipped point {x},{y}"
                    );
                }
            }
        }
        assert!(clip_polygon_to_unit_square(&[[-2.0, 0.0], [-1.0, 0.0], [-1.0, 1.0]]).is_empty());
    }

    #[test]
    fn transparent_text_preview_matches_export_and_is_bounded() {
        let mut ch = chapter();
        ch.width = 256;
        ch.height = 256;
        let layer = Layer {
            kind: LayerKind::Text {
                text: "A letter.\nA beginning.".into(),
                color: [90, 20, 40, 180],
                size: 24.0,
                bubble: true,
                style: crate::model::TextStyle::default(),
            },
            opacity: 0.7,
            ..shape(2, 20.0, 40.0, 216.0, 160.0, [0; 4])
        };
        ch.layers.push(layer.clone());
        let preview = text_preview(&layer).unwrap();
        let rendered = render_region(&ch, &AssetBytes::new(), 0, 256, 256).unwrap();
        for (x, y, pixel) in preview.enumerate_pixels() {
            let mut over_white = Rgba([255; 4]);
            blend(&mut over_white, pixel.0, 1.0);
            let expected = rendered.get_pixel(x + 20, y + 40);
            for c in 0..4 {
                assert!(
                    over_white[c].abs_diff(expected[c]) <= 1,
                    "preview pixel {x},{y}"
                );
            }
        }
        let large = text_preview(&Layer {
            width: 16000.0,
            height: 5000.0,
            ..layer
        })
        .unwrap();
        assert!(large.width() <= 2048 && large.height() <= 2048);
    }

    #[test]
    fn display_scale_preview_matches_export_for_fonts_alpha_and_negative_pointers() {
        use crate::model::{BubbleShape, BubbleTail, LetteringTheme, TextStyle};
        for theme in LetteringTheme::ALL {
            for bubble_shape in [
                BubbleShape::Speech,
                BubbleShape::Thought,
                BubbleShape::Narration,
            ] {
                let style = TextStyle {
                    shape: bubble_shape,
                    fill: [240, 245, 250, 190],
                    tail: BubbleTail {
                        enabled: true,
                        tip: [-0.2, 1.3],
                        ..Default::default()
                    },
                    extra_tails: vec![BubbleTail {
                        enabled: true,
                        tip: [1.2, -0.3],
                        ..Default::default()
                    }],
                    ..TextStyle::for_theme(theme)
                };
                let mut layer = Layer {
                    kind: LayerKind::Text {
                        text: "Wait for me.\nI'll be there.".into(),
                        color: [25, 40, 55, 210],
                        size: 46.0,
                        bubble: true,
                        style,
                    },
                    opacity: 0.7,
                    ..shape(2, 0.0, 0.0, 480.0, 300.0, [0; 4])
                };
                let [x, y, _, _] = text_bounds(&layer);
                layer.x -= x;
                layer.y -= y;
                let original = layer.clone();
                let mut ch = chapter();
                ch.width = 1600;
                ch.height = 1600;
                ch.layers = vec![layer.clone()];
                for output_width in [360, 420, 540, 800, 1600] {
                    let scale = output_width as f32 / 1600.0;
                    let preview = text_preview_at_scale(&layer, scale).unwrap();
                    let export =
                        render_region(&ch, &AssetBytes::new(), 0, output_width, output_width)
                            .unwrap();
                    for (x, y, pixel) in preview.enumerate_pixels() {
                        let mut composited = Rgba([255; 4]);
                        blend(&mut composited, pixel.0, 1.0);
                        let expected = export.get_pixel(x, y);
                        assert!(
                            composited
                                .0
                                .iter()
                                .zip(expected.0)
                                .all(|(a, b)| a.abs_diff(b) <= 1),
                            "{theme:?} {bubble_shape:?} at {output_width}, pixel {x},{y}"
                        );
                    }
                }
                assert_eq!(layer, original);
            }
        }
    }

    #[test]
    fn scaled_text_preview_rejects_invalid_scales_and_caps_extreme_zoom() {
        let layer = Layer {
            kind: LayerKind::Text {
                text: "Bounded".into(),
                color: [0, 0, 0, 255],
                size: 76.0,
                bubble: true,
                style: crate::model::TextStyle::dialogue(),
            },
            ..shape(2, 0.0, 0.0, 913.37, 200.4, [0; 4])
        };
        for scale in [0.0, -0.2, f32::INFINITY, f32::NAN] {
            assert!(text_preview_at_scale(&layer, scale).is_err());
        }
        let phone = text_preview_at_scale(&layer, 0.225).unwrap();
        let full = text_preview(&layer).unwrap();
        assert!(phone.as_raw().len() < full.as_raw().len() / 10);
        for scale in [3.0, 10.0, 1000.0] {
            let large = text_preview_at_scale(&layer, scale).unwrap();
            assert!(large.width() <= 2048 && large.height() <= 2048);
        }
    }

    #[test]
    fn lasso_preserves_original_and_existing_alpha() {
        let original = RgbaImage::from_pixel(4, 4, Rgba([100, 50, 20, 128]));
        let cut = masked_image(&original, &[[0., 0.], [0.5, 0.], [0.5, 1.], [0., 1.]]);
        assert_eq!(original.get_pixel(3, 2)[3], 128);
        assert_eq!(cut.get_pixel(0, 2)[3], 128);
        assert_eq!(cut.get_pixel(3, 2)[3], 0);
    }

    #[test]
    fn scanline_mask_matches_even_odd_samples_for_concave_polygon() {
        let polygon = [
            [0.05, 0.02],
            [0.93, 0.08],
            [0.44, 0.48],
            [0.89, 0.95],
            [0.08, 0.78],
        ];
        let source = RgbaImage::from_pixel(19, 23, Rgba([7, 9, 11, 255]));
        let result = masked_image(&source, &polygon);
        for (x, y, pixel) in result.enumerate_pixels() {
            let mut samples = 0;
            for ox in [0.25, 0.75] {
                for oy in [0.25, 0.75] {
                    samples += u16::from(point_in_polygon(
                        (x as f32 + ox) / 19.0,
                        (y as f32 + oy) / 23.0,
                        &polygon,
                    ));
                }
            }
            assert_eq!(
                pixel[3],
                ((255 * samples + 2) / 4) as u8,
                "mask pixel {x},{y}"
            );
        }
    }

    #[test]
    fn stroke_translation_and_translucent_joins_are_consistent() {
        let mut ch = chapter();
        ch.layers.push(Layer {
            kind: LayerKind::Stroke {
                points: vec![[0.0, 0.0], [6.0, 0.0], [6.0, 6.0]],
                color: [0, 0, 0, 128],
                size: 4.0,
            },
            ..shape(2, 4.0, 8.0, 8.0, 8.0, [0; 4])
        });
        let image = render_region(&ch, &AssetBytes::new(), 0, 32, 16).unwrap();
        assert_eq!(image.get_pixel(5, 8).0, [127, 127, 127, 255]);
        assert_eq!(image.get_pixel(9, 8).0, [127, 127, 127, 255]);
        assert_eq!(image.get_pixel(1, 1).0, [255; 4]);
    }

    #[test]
    fn text_and_bubbles_are_identical_across_slice_boundaries() {
        let mut ch = chapter();
        ch.width = 256;
        ch.height = 512;
        ch.layers.push(Layer {
            kind: LayerKind::Text {
                text: "A quiet city.\nAn unexpected letter.\nA new beginning.".into(),
                color: [25, 30, 40, 255],
                size: 24.0,
                bubble: true,
                style: crate::model::TextStyle::default(),
            },
            ..shape(2, 20.0, 210.0, 216.0, 180.0, [0; 4])
        });
        let assets = AssetBytes::new();
        let full = render_region(&ch, &assets, 0, 512, 256).unwrap();
        let top = render_region(&ch, &assets, 0, 256, 256).unwrap();
        let bottom = render_region(&ch, &assets, 256, 256, 256).unwrap();
        assert!(full.pixels().any(|p| p[0] < 100));
        for y in 0..512 {
            for x in 0..256 {
                let sliced = if y < 256 {
                    top.get_pixel(x, y)
                } else {
                    bottom.get_pixel(x, y - 256)
                };
                assert_eq!(full.get_pixel(x, y), sliced, "text pixel {x},{y}");
            }
        }
    }

    #[test]
    fn blends_layer_order_opacity_hidden_and_masks() {
        let mut ch = chapter();
        ch.layers.push(shape(2, 0., 0., 16., 32., [255, 0, 0, 255]));
        let mut blue = shape(3, 0., 0., 16., 32., [0, 0, 255, 255]);
        blue.opacity = 0.5;
        ch.layers.push(blue);
        let mut hidden = shape(4, 0., 0., 16., 32., [0, 0, 0, 255]);
        hidden.visible = false;
        ch.layers.push(hidden);
        let image = render_region(&ch, &AssetBytes::new(), 0, 32, 16).unwrap();
        assert_eq!(image.get_pixel(8, 8).0, [128, 0, 128, 255]);
        let mut assets = AssetBytes::new();
        assets.insert(
            8,
            encoded(RgbaImage::from_pixel(4, 4, Rgba([0, 255, 0, 255]))),
        );
        ch.layers.push(Layer {
            kind: LayerKind::Image {
                source_rect: None,
                clip: None,
                asset_id: 8,
                mask: vec![[0., 0.], [0.5, 0.], [0.5, 1.], [0., 1.]],
            },
            ..shape(9, 0., 0., 16., 32., [0; 4])
        });
        let image = render_region(&ch, &assets, 0, 32, 16).unwrap();
        assert_eq!(image.get_pixel(2, 8).0, [0, 255, 0, 255]);
        assert_eq!(image.get_pixel(12, 8).0, [128, 0, 128, 255]);
    }

    #[test]
    fn arbitrary_scale_slices_match_unsliced_composition() {
        let mut ch = chapter();
        ch.layers
            .push(shape(2, 2., 4., 9., 22., [40, 90, 160, 255]));
        ch.layers.push(Layer {
            kind: LayerKind::Stroke {
                points: vec![[0., 0.], [16., 32.]],
                color: [0, 0, 0, 128],
                size: 2.,
            },
            ..shape(3, 0., 0., 16., 32., [0; 4])
        });
        let assets = AssetBytes::new();
        let full = render_region(&ch, &assets, 0, 26, 13).unwrap();
        let top = render_region(&ch, &assets, 0, 11, 13).unwrap();
        let bottom = render_region(&ch, &assets, 11, 15, 13).unwrap();
        for y in 0..26 {
            for x in 0..13 {
                let sliced = if y < 11 {
                    top.get_pixel(x, y)
                } else {
                    bottom.get_pixel(x, y - 11)
                };
                assert_eq!(full.get_pixel(x, y), sliced, "pixel {x},{y}");
            }
        }
    }

    #[test]
    fn malformed_images_and_bad_regions_are_rejected() {
        assert!(decode_image(b"not an image").is_err());
        assert!(render_region(&chapter(), &AssetBytes::new(), 33, 1, 16).is_err());
        assert!(render_region(&chapter(), &AssetBytes::new(), 0, 0, 16).is_err());
        assert!(render_region(&chapter(), &AssetBytes::new(), 0, 32, 0).is_err());
    }

    #[test]
    fn exported_slices_have_correct_dimensions_and_do_not_overwrite() {
        let path = std::env::temp_dir().join(format!(
            "scroll-export-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let paths = export_chapter(&chapter(), &AssetBytes::new(), &path, 8, 5).unwrap();
        assert_eq!(paths.len(), 4);
        for (i, p) in paths.iter().enumerate() {
            let image = image::open(p).unwrap();
            assert_eq!(
                (image.width(), image.height()),
                (8, if i == 3 { 1 } else { 5 })
            );
        }
        assert!(export_chapter(&chapter(), &AssetBytes::new(), &path, 8, 5).is_err());
        for p in paths {
            fs::remove_file(p).unwrap();
        }
        fs::remove_dir(path).unwrap();
    }
}

#[cfg(test)]
mod unknown_layer_tests {
    use super::*;
    use crate::model::{Chapter, Layer};

    fn layer(id: u64, kind: LayerKind) -> Layer {
        Layer {
            extensions: Default::default(),
            id,
            name: "L".into(),
            visible: true,
            locked: false,
            opacity: 1.0,
            x: 2.0,
            y: 2.0,
            width: 10.0,
            height: 10.0,
            kind,
        }
    }

    #[test]
    fn unknown_layers_are_skipped_by_rendering() {
        let mut chapter = Chapter {
            extensions: Default::default(),
            id: 1,
            name: "Test".into(),
            width: 16,
            height: 32,
            panels: vec![],
            layers: vec![layer(
                2,
                LayerKind::Shape {
                    color: [9, 8, 7, 255],
                },
            )],
        };
        let assets = AssetBytes::new();
        let without = render_region(&chapter, &assets, 0, 32, 16).unwrap();
        let unknown = LayerKind::Unknown {
            kind: "Mannequin".into(),
            data: serde_json::json!({ "Mannequin": { "pose": 1 } }),
        };
        chapter.layers.insert(0, layer(3, unknown.clone()));
        chapter.layers.push(layer(4, unknown));
        let with = render_region(&chapter, &assets, 0, 32, 16).unwrap();
        assert_eq!(with.as_raw(), without.as_raw());
    }
}
