//! Small, local texture fills. Samples the chapter composite and returns a
//! transparent patch; never changes the source images or unpainted pixels.
use crate::{
    compositor,
    model::{AssetBytes, Chapter},
};
use anyhow::{ensure, Result};
#[cfg(test)]
use image::Rgba;
use image::RgbaImage;
use std::collections::VecDeque;

pub struct HealPatch {
    pub image: RgbaImage,
    pub x: u32,
    pub y: u32,
}

pub fn heal_selection(
    chapter: &Chapter,
    assets: &AssetBytes,
    selection: &crate::clipping::SelectionMask,
    placement: [f32; 4],
) -> Result<HealPatch> {
    let [px, py, pw, ph] = placement;
    ensure!(
        placement.iter().all(|v| v.is_finite()) && pw > 0.0 && ph > 0.0,
        "Invalid selection placement"
    );
    let [sx, sy, sw, sh] = selection
        .bounds()
        .ok_or_else(|| anyhow::anyhow!("No selected pixels"))?;
    let x0 = (px + sx as f32 / selection.width as f32 * pw - 96.0)
        .floor()
        .clamp(0.0, chapter.width as f32) as u32;
    let y0 = (py + sy as f32 / selection.height as f32 * ph - 96.0)
        .floor()
        .clamp(0.0, chapter.height as f32) as u32;
    let x1 = (px + (sx + sw) as f32 / selection.width as f32 * pw + 96.0)
        .ceil()
        .clamp(0.0, chapter.width as f32) as u32;
    let y1 = (py + (sy + sh) as f32 / selection.height as f32 * ph + 96.0)
        .ceil()
        .clamp(0.0, chapter.height as f32) as u32;
    ensure!(x1 > x0 && y1 > y0, "Selection is outside the canvas");
    let (width, height) = (x1 - x0, y1 - y0);
    ensure!(
        u64::from(width) * u64::from(height) <= 4_000_000
            && u64::from(chapter.width) * u64::from(height) <= 32_000_000,
        "Fill a smaller selection at a time"
    );
    let strip = compositor::render_region(chapter, assets, y0, height, chapter.width)?;
    let source = image::imageops::crop_imm(&strip, x0, 0, width, height).to_image();
    drop(strip);
    let mask = (0..height)
        .flat_map(|y| {
            (0..width).map(move |x| {
                selection.contains_uv(
                    (x0 as f32 + x as f32 + 0.5 - px) / pw,
                    (y0 as f32 + y as f32 + 0.5 - py) / ph,
                )
            })
        })
        .collect::<Vec<_>>();
    Ok(HealPatch {
        image: texture_fill(&source, &mask)?,
        x: x0,
        y: y0,
    })
}

fn brush_region(
    chapter: &Chapter,
    assets: &AssetBytes,
    points: &[[f32; 2]],
    diameter: f32,
    padding: f32,
) -> Result<(RgbaImage, Vec<bool>, u32, u32)> {
    ensure!(
        !points.is_empty()
            && points.len() <= 4000
            && points.iter().flatten().all(|v| v.is_finite())
            && diameter.is_finite()
            && (1.0..=512.0).contains(&diameter),
        "Invalid content-aware brush stroke"
    );
    let radius = diameter * 0.5;
    let margin = (diameter * 2.0).max(padding).clamp(48.0, 256.0);
    let left = points.iter().map(|p| p[0]).fold(f32::INFINITY, f32::min) - radius;
    let top = points.iter().map(|p| p[1]).fold(f32::INFINITY, f32::min) - radius;
    let right = points
        .iter()
        .map(|p| p[0])
        .fold(f32::NEG_INFINITY, f32::max)
        + radius;
    let bottom = points
        .iter()
        .map(|p| p[1])
        .fold(f32::NEG_INFINITY, f32::max)
        + radius;
    ensure!(
        left < chapter.width as f32 && top < chapter.height as f32 && right > 0.0 && bottom > 0.0,
        "Paint inside the canvas"
    );
    let x0 = (left - margin).floor().max(0.0) as u32;
    let y0 = (top - margin).floor().max(0.0) as u32;
    let x1 = (right + margin).ceil().min(chapter.width as f32) as u32;
    let y1 = (bottom + margin).ceil().min(chapter.height as f32) as u32;
    let (width, height) = (x1 - x0, y1 - y0);
    ensure!(
        width > 0
            && height > 0
            && u64::from(width) * u64::from(height) <= 4_000_000
            && u64::from(chapter.width) * u64::from(height) <= 32_000_000,
        "Paint a smaller area per stroke"
    );
    let strip = compositor::render_region(chapter, assets, y0, height, chapter.width)?;
    let image = image::imageops::crop_imm(&strip, x0, 0, width, height).to_image();
    drop(strip);
    let mut mask = vec![false; width as usize * height as usize];
    let local: Vec<_> = points
        .iter()
        .map(|p| [p[0] - x0 as f32, p[1] - y0 as f32])
        .collect();
    for (a, b) in local.iter().zip(local.iter().skip(1).chain(local.last())) {
        let min_x = (a[0].min(b[0]) - radius - 1.0).floor().max(0.0) as u32;
        let min_y = (a[1].min(b[1]) - radius - 1.0).floor().max(0.0) as u32;
        let max_x = (a[0].max(b[0]) + radius + 1.0).ceil().min(width as f32) as u32;
        let max_y = (a[1].max(b[1]) + radius + 1.0).ceil().min(height as f32) as u32;
        let dx = b[0] - a[0];
        let dy = b[1] - a[1];
        let length = dx * dx + dy * dy;
        for y in min_y..max_y {
            for x in min_x..max_x {
                let px = x as f32 + 0.5;
                let py = y as f32 + 0.5;
                let t = if length > 0.0 {
                    ((px - a[0]) * dx + (py - a[1]) * dy) / length
                } else {
                    0.0
                }
                .clamp(0.0, 1.0);
                if (px - a[0] - t * dx).hypot(py - a[1] - t * dy) <= radius {
                    mask[(y * width + x) as usize] = true;
                }
            }
        }
    }
    ensure!(mask.iter().any(|m| *m), "Paint a larger area");
    Ok((image, mask, x0, y0))
}

pub fn heal_stroke(
    chapter: &Chapter,
    assets: &AssetBytes,
    points: &[[f32; 2]],
    diameter: f32,
) -> Result<HealPatch> {
    let (image, mask, x, y) = brush_region(chapter, assets, points, diameter, 48.0)?;
    Ok(HealPatch {
        image: texture_fill(&image, &mask)?,
        x,
        y,
    })
}
pub fn motion_stroke(
    chapter: &Chapter,
    assets: &AssetBytes,
    points: &[[f32; 2]],
    diameter: f32,
    length: f32,
    angle: f32,
) -> Result<HealPatch> {
    ensure!(
        length.is_finite() && (1.0..=128.0).contains(&length) && angle.is_finite(),
        "Invalid motion blur"
    );
    let (source, mask, x, y) = brush_region(chapter, assets, points, diameter, length + 4.0)?;
    let mut image = RgbaImage::new(source.width(), source.height());
    let (dy, dx) = angle.to_radians().sin_cos();
    let samples = (length.ceil() as usize).clamp(4, 32);
    for (i, pixel) in image.pixels_mut().enumerate() {
        if !mask[i] {
            continue;
        }
        let px = (i % source.width() as usize) as f32;
        let py = (i / source.width() as usize) as f32;
        let mut color = [0.0; 4];
        for sample in 0..samples {
            let offset = (sample as f32 / (samples - 1) as f32 - 0.5) * length;
            let sx = (px + dx * offset)
                .round()
                .clamp(0.0, (source.width() - 1) as f32) as u32;
            let sy = (py + dy * offset)
                .round()
                .clamp(0.0, (source.height() - 1) as f32) as u32;
            let p = source.get_pixel(sx, sy);
            let a = p[3] as f32 / 255.0;
            for c in 0..3 {
                color[c] += p[c] as f32 * a;
            }
            color[3] += a;
        }
        if color[3] > 0.0 {
            for c in 0..3 {
                pixel[c] = (color[c] / color[3]).round() as u8;
            }
            pixel[3] = (color[3] / samples as f32 * 255.0).round() as u8;
        }
    }
    Ok(HealPatch { image, x, y })
}

pub fn smear_image(
    source: &RgbaImage,
    length: u32,
    angle: f32,
    threshold: u8,
) -> Result<RgbaImage> {
    ensure!(
        (1..=256).contains(&length) && angle.is_finite(),
        "Invalid smear settings"
    );
    let w = source.width() + length * 2;
    let h = source.height() + length * 2;
    ensure!(
        w <= 16_384 && h <= 16_384 && u64::from(w) * u64::from(h) <= 4_000_000,
        "Use a smaller cutout for smear generation"
    );
    let mut output = RgbaImage::new(w, h);
    let (dy, dx) = angle.to_radians().sin_cos();
    let steps = length.clamp(4, 48);
    for (x, y, pixel) in output.enumerate_pixels_mut() {
        let px = x as f32 - length as f32;
        let py = y as f32 - length as f32;
        let lane = (-px * dy + py * dx).floor() as i32;
        let hash = (lane as u32).wrapping_mul(0x45d9f3b).rotate_left(11);
        let reach = length as f32 * (0.25 + (hash & 255) as f32 / 255.0 * 0.75);
        let mut best = [0; 4];
        for step in 1..=steps {
            let distance = step as f32 / steps as f32 * reach;
            let sx = (px + dx * distance).round() as i32;
            let sy = (py + dy * distance).round() as i32;
            if sx < 0 || sy < 0 || sx >= source.width() as i32 || sy >= source.height() as i32 {
                continue;
            }
            let p = source.get_pixel(sx as u32, sy as u32).0;
            if p[3] == 0 {
                continue;
            }
            let nx = (sx as f32 - dx * 2.0).round() as i32;
            let ny = (sy as f32 - dy * 2.0).round() as i32;
            let neighbor = if nx >= 0
                && ny >= 0
                && nx < source.width() as i32
                && ny < source.height() as i32
            {
                source.get_pixel(nx as u32, ny as u32).0
            } else {
                [0; 4]
            };
            let contrast = (0..4)
                .map(|c| p[c].abs_diff(neighbor[c]))
                .max()
                .unwrap_or(0);
            if contrast < threshold {
                continue;
            }
            let alpha = (p[3] as f32 * (1.0 - distance / (reach + 1.0)).sqrt()) as u8;
            if alpha > best[3] {
                best = [p[0], p[1], p[2], alpha];
            }
        }
        *pixel = image::Rgba(best);
    }
    Ok(output)
}

/// Bounded nearest-donor initialization followed by three alternating local
/// patch searches. Independent implementation; no model, service or foreign
/// runtime. Work is bounded by the affected region, not the chapter length.
pub fn texture_fill(source: &RgbaImage, mask: &[bool]) -> Result<RgbaImage> {
    let (w, h) = source.dimensions();
    let count = w as usize * h as usize;
    ensure!(
        w >= 5 && h >= 5 && count <= 4_000_000 && mask.len() == count,
        "Invalid fill region"
    );
    ensure!(mask.iter().any(|m| *m), "No painted area");
    let scale = (768.0 / w.max(h) as f32).min(1.0);
    if scale < 1.0 {
        let sw = (w as f32 * scale).round().max(5.0) as u32;
        let sh = (h as f32 * scale).round().max(5.0) as u32;
        let small = image::imageops::resize(source, sw, sh, image::imageops::FilterType::Triangle);
        let mut reduced = vec![false; (sw * sh) as usize];
        // Conservative reduction prevents a thin painted object becoming a donor.
        for y in 0..h {
            for x in 0..w {
                if mask[(y * w + x) as usize] {
                    let sx = (x as u64 * sw as u64 / w as u64) as u32;
                    let sy = (y as u64 * sh as u64 / h as u64) as u32;
                    reduced[(sy * sw + sx) as usize] = true;
                }
            }
        }
        let small_fill = fill_pixels(&small, &reduced)?;
        let up = image::imageops::resize(&small_fill, w, h, image::imageops::FilterType::Triangle);
        let mut patch = RgbaImage::new(w, h);
        for (i, (dst, src)) in patch.pixels_mut().zip(up.pixels()).enumerate() {
            if mask[i] {
                *dst = *src;
                dst[3] = 255;
            }
        }
        return Ok(patch);
    }
    let filled = fill_pixels(source, mask)?;
    let mut patch = RgbaImage::new(w, h);
    for (i, (dst, src)) in patch.pixels_mut().zip(filled.pixels()).enumerate() {
        if mask[i] {
            *dst = *src;
            dst[3] = 255;
        }
    }
    Ok(patch)
}

fn fill_pixels(source: &RgbaImage, mask: &[bool]) -> Result<RgbaImage> {
    let w = source.width() as usize;
    let h = source.height() as usize;
    let n = w * h;
    let mut valid = vec![false; n];
    let mut nearest = vec![u32::MAX; n];
    let mut queue = VecDeque::new();
    for y in 2..h - 2 {
        for x in 2..w - 2 {
            let i = y * w + x;
            let safe = (-2..=2).all(|dy| {
                (-2..=2)
                    .all(|dx| !mask[((y as isize + dy) as usize) * w + (x as isize + dx) as usize])
            });
            if safe {
                valid[i] = true;
                nearest[i] = i as u32;
                queue.push_back(i);
            }
        }
    }
    ensure!(
        !queue.is_empty(),
        "Leave some unpainted surroundings for the fill to sample"
    );
    while let Some(i) = queue.pop_front() {
        let (x, y) = (i % w, i / w);
        for (nx, ny) in [
            (x.wrapping_sub(1), y),
            (x + 1, y),
            (x, y.wrapping_sub(1)),
            (x, y + 1),
        ] {
            if nx >= w || ny >= h {
                continue;
            }
            let j = ny * w + nx;
            if nearest[j] == u32::MAX {
                nearest[j] = nearest[i];
                queue.push_back(j);
            }
        }
    }
    let mut result = source.clone();
    let targets: Vec<_> = mask
        .iter()
        .enumerate()
        .filter_map(|(i, m)| m.then_some(i))
        .collect();
    for &i in &targets {
        let j = nearest[i] as usize;
        result.put_pixel(
            (i % w) as u32,
            (i / w) as u32,
            *source.get_pixel((j % w) as u32, (j / w) as u32),
        );
    }
    // Keep a stable local color guide: refinement must not drift into a
    // distant face or unrelated object merely because its tiny patch matches.
    let guidance = boundary_guidance(source, &valid, mask, &result);
    // Patch comparisons start from continued structure rather than the
    // nearest-donor Voronoi pattern, which otherwise reinforces seams.
    result = guidance.clone();
    let mut random = 0x71c3_49a5_u32;
    for pass in 0..3 {
        for k in 0..targets.len() {
            let i = targets[if pass % 2 == 0 {
                k
            } else {
                targets.len() - 1 - k
            }];
            let x = i % w;
            let y = i / w;
            let mut best = nearest[i] as usize;
            let mut cost = patch_cost(&result, &guidance, source, mask, i, best);
            let mut candidates = [best; 4];
            if x > 0 {
                let p = nearest[i - 1] as usize;
                if p % w + 1 < w {
                    candidates[0] = p + 1;
                }
            }
            if y > 0 {
                let p = nearest[i - w] as usize;
                if p / w + 1 < h {
                    candidates[1] = p + w;
                }
            }
            if x + 1 < w {
                let p = nearest[i + 1] as usize;
                if !p.is_multiple_of(w) {
                    candidates[2] = p - 1;
                }
            }
            if y + 1 < h {
                let p = nearest[i + w] as usize;
                if p / w > 0 {
                    candidates[3] = p - w;
                }
            }
            for candidate in candidates {
                if valid[candidate] {
                    let score = patch_cost(&result, &guidance, source, mask, i, candidate);
                    if score < cost {
                        best = candidate;
                        cost = score;
                    }
                }
            }
            let mut radius = w.max(h) / 2;
            while radius >= 2 {
                random ^= random << 13;
                random ^= random >> 17;
                random ^= random << 5;
                let dx = (random as usize % (radius * 2 + 1)) as isize - radius as isize;
                random ^= random << 13;
                random ^= random >> 17;
                random ^= random << 5;
                let dy = (random as usize % (radius * 2 + 1)) as isize - radius as isize;
                let sx = (best % w) as isize + dx;
                let sy = (best / w) as isize + dy;
                if sx >= 0 && sy >= 0 && sx < w as isize && sy < h as isize {
                    let candidate = sy as usize * w + sx as usize;
                    if valid[candidate] {
                        let score = patch_cost(&result, &guidance, source, mask, i, candidate);
                        if score < cost {
                            best = candidate;
                            cost = score;
                        }
                    }
                }
                radius /= 2;
            }
            nearest[i] = best as u32;
            result.put_pixel(
                x as u32,
                y as u32,
                *source.get_pixel((best % w) as u32, (best / w) as u32),
            );
        }
    }
    Ok(result)
}
/// Prefer continuation between matching opposite boundaries. This preserves
/// simple horizons/window edges instead of extending nearest-color diamonds.
fn boundary_guidance(
    source: &RgbaImage,
    valid: &[bool],
    mask: &[bool],
    initial: &RgbaImage,
) -> RgbaImage {
    let (w, h) = (source.width() as usize, source.height() as usize);
    let mut candidates = [initial.clone(), initial.clone()];
    let mut previous = vec![usize::MAX; w * h];
    let mut covered = [vec![false; w * h], vec![false; w * h]];
    let mut scores = [0_u64; 2];
    let mut counts = [0_u64; 2];
    for (axis, vertical) in [true, false].into_iter().enumerate() {
        let (lanes, length) = if vertical { (w, h) } else { (h, w) };
        for lane in 0..lanes {
            let index = |p| if vertical { p * w + lane } else { lane * w + p };
            let mut last = usize::MAX;
            for p in 0..length {
                let i = index(p);
                if valid[i] {
                    last = p;
                }
                previous[i] = last;
            }
            let mut next = usize::MAX;
            for p in (0..length).rev() {
                let i = index(p);
                if valid[i] {
                    next = p;
                    continue;
                }
                let prev = previous[i];
                if !mask[i] || prev == usize::MAX || next == usize::MAX {
                    continue;
                }
                let a = index(prev);
                let b = index(next);
                let ca = source.get_pixel((a % w) as u32, (a / w) as u32);
                let cb = source.get_pixel((b % w) as u32, (b / w) as u32);
                let mut score = ((next - prev) * (next - prev) / 512) as u32;
                for c in 0..3 {
                    let d = i32::from(ca[c]) - i32::from(cb[c]);
                    score += (d * d) as u32;
                }
                scores[axis] += u64::from(score);
                counts[axis] += 1;
                covered[axis][i] = true;
                let t = (p - prev) as f32 / (next - prev) as f32;
                let color = std::array::from_fn(|c| {
                    (ca[c] as f32 * (1.0 - t) + cb[c] as f32 * t).round() as u8
                });
                candidates[axis].put_pixel((i % w) as u32, (i / w) as u32, image::Rgba(color));
            }
        }
    }
    // Choose a coherent continuation direction for the local brush region.
    // Choosing an axis independently per pixel lets tiny texture variations
    // alternate between incompatible edges and creates a speckled grid.
    let preferred = usize::from(
        counts[0] == 0 || (counts[1] > 0 && scores[1] * counts[0] < scores[0] * counts[1]),
    );
    let mut result = candidates[preferred].clone();
    for (i, pixel) in result.pixels_mut().enumerate() {
        if mask[i] && !covered[preferred][i] {
            *pixel = *candidates[1 - preferred].get_pixel((i % w) as u32, (i / w) as u32);
        }
    }
    result
}

fn patch_cost(
    current: &RgbaImage,
    guidance: &RgbaImage,
    source: &RgbaImage,
    mask: &[bool],
    target: usize,
    donor: usize,
) -> u32 {
    let w = source.width() as usize;
    let h = source.height() as usize;
    let (x, y) = (target % w, target / w);
    let (sx, sy) = (donor % w, donor / w);
    let dx = x.abs_diff(sx) as u32;
    let dy = y.abs_diff(sy) as u32;
    let mut sum = (dx * dx + dy * dy) * 2;
    let local = guidance.get_pixel(x as u32, y as u32);
    let candidate = source.get_pixel(sx as u32, sy as u32);
    for c in 0..3 {
        let diff = i32::from(local[c]) - i32::from(candidate[c]);
        sum += (diff * diff) as u32 * 16;
    }
    for dy in [-2_isize, 0, 2] {
        for dx in [-2_isize, 0, 2] {
            let tx = x as isize + dx;
            let ty = y as isize + dy;
            if tx < 0 || ty < 0 || tx >= w as isize || ty >= h as isize {
                continue;
            }
            let a = current.get_pixel(tx as u32, ty as u32);
            let b = source.get_pixel((sx as isize + dx) as u32, (sy as isize + dy) as u32);
            let weight = if mask[ty as usize * w + tx as usize] {
                1
            } else {
                4
            };
            for c in 0..3 {
                let diff = i32::from(a[c]) - i32::from(b[c]);
                sum += (diff * diff) as u32 * weight;
            }
        }
    }
    sum
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn removes_flat_color_intruder_without_changing_any_unpainted_pixels() {
        let mut image = RgbaImage::from_pixel(64, 64, Rgba([35, 100, 150, 255]));
        let mut mask = vec![false; 4096];
        for y in 22..42 {
            for x in 22..42 {
                image.put_pixel(x, y, Rgba([240, 20, 20, 255]));
                mask[(y * 64 + x) as usize] = true;
            }
        }
        let original = image.clone();
        let patch = texture_fill(&image, &mask).unwrap();
        for (i, pixel) in patch.pixels().enumerate() {
            assert_eq!(pixel.0, if mask[i] { [35, 100, 150, 255] } else { [0; 4] });
        }
        assert_eq!(image, original);
        assert!(texture_fill(&image, &vec![true; 4096]).is_err());
    }
    #[test]
    fn brush_samples_all_visible_layers_and_keeps_patch_transparent() {
        use crate::model::*;
        let mut chapter = Chapter::new(1, "Fill", 128, 128);
        chapter.layers.push(Layer {
            id: 2,
            name: "Backdrop".into(),
            x: 0.0,
            y: 0.0,
            width: 128.0,
            height: 128.0,
            visible: true,
            locked: false,
            opacity: 1.0,
            kind: LayerKind::Shape {
                color: [80, 130, 170, 255],
            },
        });
        chapter.layers.push(Layer {
            id: 3,
            name: "Intruder".into(),
            x: 60.0,
            y: 60.0,
            width: 8.0,
            height: 8.0,
            visible: true,
            locked: false,
            opacity: 1.0,
            kind: LayerKind::Shape {
                color: [240, 30, 40, 255],
            },
        });
        let original = chapter.clone();
        let patch = heal_stroke(&chapter, &AssetBytes::new(), &[[64.0, 64.0]], 24.0).unwrap();
        assert_eq!(
            patch.image.get_pixel(64 - patch.x, 64 - patch.y).0,
            [80, 130, 170, 255]
        );
        assert_eq!(patch.image.get_pixel(0, 0)[3], 0);
        assert_eq!(chapter, original);
    }
    #[test]
    fn smear_is_directional_thresholded_and_does_not_mutate_the_source() {
        let mut source = RgbaImage::new(40, 40);
        for y in 10..30 {
            for x in 10..30 {
                source.put_pixel(x, y, Rgba([20, 40, 60, 255]));
            }
        }
        let before = source.clone();
        let horizontal = smear_image(&source, 16, 0.0, 50).unwrap();
        let vertical = smear_image(&source, 16, 90.0, 50).unwrap();
        assert_ne!(horizontal, vertical);
        assert_eq!(source, before);
        assert_eq!(horizontal.dimensions(), (72, 72));
        assert_eq!(horizontal.get_pixel(0, 0)[3], 0);
        let high = smear_image(&source, 16, 0.0, 255).unwrap();
        let a: u64 = horizontal.pixels().map(|p| u64::from(p[3])).sum();
        let b: u64 = high.pixels().map(|p| u64::from(p[3])).sum();
        assert!(a >= b);
    }
    #[test]
    fn motion_brush_preserves_unpainted_area_and_direction_changes_result() {
        use crate::model::*;
        let mut chapter = Chapter::new(1, "Blur", 128, 128);
        chapter.layers.push(Layer {
            id: 2,
            name: "Stripe".into(),
            x: 60.0,
            y: 0.0,
            width: 8.0,
            height: 128.0,
            visible: true,
            locked: false,
            opacity: 1.0,
            kind: LayerKind::Shape {
                color: [0, 0, 0, 255],
            },
        });
        let original = chapter.clone();
        let horizontal = motion_stroke(
            &chapter,
            &AssetBytes::new(),
            &[[64.0, 64.0]],
            36.0,
            24.0,
            0.0,
        )
        .unwrap();
        let vertical = motion_stroke(
            &chapter,
            &AssetBytes::new(),
            &[[64.0, 64.0]],
            36.0,
            24.0,
            90.0,
        )
        .unwrap();
        assert_eq!(horizontal.image.get_pixel(0, 0)[3], 0);
        assert_ne!(horizontal.image, vertical.image);
        assert_eq!(chapter, original);
    }
    #[test]
    fn local_fill_does_not_pull_in_a_nearby_unrelated_subject() {
        let mut source = RgbaImage::from_pixel(128, 128, Rgba([30, 55, 75, 255]));
        for y in 25..70 {
            for x in 20..43 {
                source.put_pixel(x, y, Rgba([225, 190, 160, 255]));
            }
        }
        let mut mask = vec![false; 128 * 128];
        for y in 40..90 {
            for x in 65..105 {
                mask[(y * 128 + x) as usize] = true;
                source.put_pixel(x, y, Rgba([255, 255, 255, 255]));
            }
        }
        let patch = texture_fill(&source, &mask).unwrap();
        assert!(patch.pixels().filter(|p| p[3] > 0).all(|p| p[0] < 80));
    }
    #[test]
    fn fill_continues_a_straight_background_boundary() {
        let mut image = RgbaImage::new(128, 128);
        let mut mask = vec![false; 128 * 128];
        for (x, y, p) in image.enumerate_pixels_mut() {
            *p = Rgba(if x < 64 {
                [40, 130, 160, 255]
            } else {
                [25, 45, 60, 255]
            });
            if (40..88).contains(&x) && (40..88).contains(&y) {
                mask[(y * 128 + x) as usize] = true;
                *p = Rgba([240, 20, 20, 255]);
            }
        }
        let patch = texture_fill(&image, &mask).unwrap();
        assert_eq!(patch.get_pixel(56, 64).0, [40, 130, 160, 255]);
        assert_eq!(patch.get_pixel(72, 64).0, [25, 45, 60, 255]);
    }
    #[test]
    fn textured_parallel_edges_do_not_turn_into_a_speckled_grid() {
        let mut image = RgbaImage::new(256, 192);
        let mut mask = vec![false; 256 * 192];
        for (x, y, p) in image.enumerate_pixels_mut() {
            let grain = ((x * 17 + y * 31) % 13) as u8;
            *p = Rgba(if x % 80 < 40 {
                [70 + grain, 125 + grain, 145 + grain, 255]
            } else {
                [28 + grain, 48 + grain, 69 + grain, 255]
            });
            if ((x as f32 - 128.0) / 65.0).powi(2) + ((y as f32 - 96.0) / 47.0).powi(2) < 1.0 {
                mask[(y * 256 + x) as usize] = true;
                *p = Rgba([240, 20, 20, 255]);
            }
        }
        let patch = texture_fill(&image, &mask).unwrap();
        let mut wrong = 0;
        let mut checked = 0;
        for (x, y, p) in patch.enumerate_pixels() {
            // Allow the two-pixel patch radius at the actual boundary.
            if mask[(y * 256 + x) as usize] && (3..37).contains(&(x % 40)) {
                checked += 1;
                wrong += usize::from((p[1] > 90) != (x % 80 < 40));
            }
        }
        assert!(
            wrong * 100 < checked,
            "{wrong}/{checked} pixels crossed an edge"
        );
    }
}
