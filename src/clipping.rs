//! Non-destructive selections at original image resolution. No generated pixels
//! are written into the asset; previews, clipboard and export share this renderer.
use anyhow::{ensure, Result};
use image::{Rgba, RgbaImage};
use serde::{Deserialize, Serialize};
use std::{collections::VecDeque, sync::Arc};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum EdgePattern {
    #[default]
    Hard,
    Dotted,
    Fade,
}
impl EdgePattern {
    pub const ALL: [Self; 3] = [Self::Hard, Self::Dotted, Self::Fade];
    pub fn label(self) -> &'static str {
        match self {
            Self::Hard => "Hard",
            Self::Dotted => "Dotted",
            Self::Fade => "Fade",
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Contour {
    Inside,
    #[default]
    Center,
    Outside,
}
impl Contour {
    pub const ALL: [Self; 3] = [Self::Inside, Self::Center, Self::Outside];
    pub fn label(self) -> &'static str {
        match self {
            Self::Inside => "Inside",
            Self::Center => "Center",
            Self::Outside => "Outside",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EdgeSettings {
    pub pattern: EdgePattern,
    /// Original image pixels. Placement and export never rescale the source.
    pub width: u16,
    pub contour: Contour,
    pub dot_scale: u16,
    /// Zero alpha disables the background. It exists only inside the edge band.
    pub background: [u8; 4],
    pub backing: crate::cutout::BackingSettings,
}
impl Default for EdgeSettings {
    fn default() -> Self {
        Self {
            pattern: EdgePattern::Hard,
            width: 32,
            contour: Contour::Center,
            dot_scale: 12,
            background: [255, 255, 255, 0],
            backing: crate::cutout::BackingSettings::default(),
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SelectionMode {
    #[default]
    Replace,
    Add,
    Subtract,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectionMask {
    pub width: u32,
    pub height: u32,
    /// One bit per ORIGINAL source pixel (at most 4 MB at the import limit).
    #[serde(with = "mask_archive")]
    bits: Vec<u64>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageClip {
    pub selection: Arc<SelectionMask>,
    pub edge: EdgeSettings,
}

mod mask_archive {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::io::{Read, Write};
    pub fn serialize<S: Serializer>(words: &[u64], serializer: S) -> Result<S::Ok, S::Error> {
        let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
        for word in words {
            encoder
                .write_all(&word.to_le_bytes())
                .map_err(serde::ser::Error::custom)?;
        }
        encoder
            .finish()
            .map_err(serde::ser::Error::custom)?
            .serialize(serializer)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u64>, D::Error> {
        let encoded = Vec::<u8>::deserialize(deserializer)?;
        let mut decoder = flate2::read::ZlibDecoder::new(encoded.as_slice()).take(4_000_001);
        let mut bytes = Vec::new();
        decoder
            .read_to_end(&mut bytes)
            .map_err(serde::de::Error::custom)?;
        if bytes.len() > 4_000_000 || bytes.len() % 8 != 0 {
            return Err(serde::de::Error::custom(
                "Selection exceeds source pixel limit",
            ));
        }
        Ok(bytes
            .as_chunks::<8>()
            .0
            .iter()
            .map(|b| u64::from_le_bytes(*b))
            .collect())
    }
}
impl EdgeSettings {
    pub fn outside_extent(self) -> u32 {
        let contour = if self.width == 0
            || (self.pattern == EdgePattern::Hard
                && self.contour == Contour::Center
                && self.background[3] == 0)
        {
            0
        } else {
            match self.contour {
                Contour::Inside => 0,
                Contour::Center => u32::from(self.width).div_ceil(2),
                Contour::Outside => u32::from(self.width),
            }
        };
        contour.max(if self.backing.enabled {
            u32::from(self.backing.reach)
        } else {
            0
        })
    }
}
impl SelectionMask {
    /// Include the entire generated edge when making a metadata-only snippet.
    pub fn edge_bounds(&self, edge: EdgeSettings) -> Option<[u32; 4]> {
        let [x, y, w, h] = self.bounds()?;
        let pad = edge.outside_extent();
        let left = x.saturating_sub(pad);
        let top = y.saturating_sub(pad);
        let right = (x + w + pad).min(self.width);
        let bottom = (y + h + pad).min(self.height);
        Some([left, top, right - left, bottom - top])
    }

    /// A local foreground estimate for reference sheets: flood dominant border
    /// colors, then keep the foreground component under the user's click.
    pub fn subject(image: &RgbaImage, seed: [u32; 2], tolerance: u8) -> Result<Self> {
        Self::subject_brush(
            image,
            &[[seed[0] as f32 + 0.5, seed[1] as f32 + 0.5]],
            [0.5; 2],
            tolerance,
        )
    }
    pub fn subject_brush(
        image: &RgbaImage,
        points: &[[f32; 2]],
        radii: [f32; 2],
        tolerance: u8,
    ) -> Result<Self> {
        let (w, h) = image.dimensions();
        let footprint = Self::brush(w, h, points, radii)?;
        let mut histogram = std::collections::HashMap::<[u8; 3], (usize, [u64; 3])>::new();
        let mut sample = |x, y| {
            let p = image.get_pixel(x, y);
            if p[3] < 32 {
                return;
            }
            let entry = histogram
                .entry([p[0] / 24, p[1] / 24, p[2] / 24])
                .or_default();
            entry.0 += 1;
            for c in 0..3 {
                entry.1[c] += u64::from(p[c]);
            }
        };
        for x in (0..w).step_by((w / 256).max(1) as usize) {
            sample(x, 0);
            sample(x, h - 1);
        }
        for y in (0..h).step_by((h / 256).max(1) as usize) {
            sample(0, y);
            sample(w - 1, y);
        }
        let mut colors: Vec<_> = histogram.into_values().collect();
        colors.sort_unstable_by_key(|a| std::cmp::Reverse(a.0));
        let total: usize = colors.iter().map(|c| c.0).sum();
        let palette: Vec<_> = colors
            .into_iter()
            .take(4)
            .filter(|c| c.0 * 10 >= total)
            .map(|(n, rgb)| rgb.map(|c| (c / n as u64) as u8))
            .collect();
        let is_background = |x, y| {
            let p = image.get_pixel(x, y);
            p[3] < 32
                || palette
                    .iter()
                    .any(|color| (0..3).all(|c| p[c].abs_diff(color[c]) <= tolerance))
        };
        let mut background = Self::empty(w, h)?;
        let mut queue = VecDeque::new();
        for x in 0..w {
            for y in [0, h - 1] {
                let i = (y * w + x) as usize;
                if !background.get_index(i) && is_background(x, y) {
                    background.set_index(i);
                    queue.push_back(i as u32);
                }
            }
        }
        for y in 0..h {
            for x in [0, w - 1] {
                let i = (y * w + x) as usize;
                if !background.get_index(i) && is_background(x, y) {
                    background.set_index(i);
                    queue.push_back(i as u32);
                }
            }
        }
        while let Some(i) = queue.pop_front() {
            let (x, y) = (i % w, i / w);
            for (nx, ny) in [
                (x.wrapping_sub(1), y),
                (x + 1, y),
                (x, y.wrapping_sub(1)),
                (x, y + 1),
            ] {
                if nx < w && ny < h {
                    let n = (ny * w + nx) as usize;
                    if !background.get_index(n) && is_background(nx, ny) {
                        background.set_index(n);
                        ensure!(
                            queue.len() < 4_000_000,
                            "Selection is too complex; use a smaller area"
                        );
                        queue.push_back(n as u32);
                    }
                }
            }
        }
        let mut selected = Self::empty(w, h)?;
        for i in 0..(w as usize * h as usize) {
            if footprint.get_index(i) && !background.get_index(i) {
                selected.set_index(i);
                ensure!(queue.len() < 4_000_000, "Use a smaller selection brush");
                queue.push_back(i as u32);
            }
        }
        ensure!(
            !queue.is_empty(),
            "Brush over the character, or use Lasso on a busy background"
        );
        while let Some(i) = queue.pop_front() {
            let (x, y) = (i % w, i / w);
            for (nx, ny) in [
                (x.wrapping_sub(1), y),
                (x + 1, y),
                (x, y.wrapping_sub(1)),
                (x, y + 1),
            ] {
                if nx < w && ny < h {
                    let n = (ny * w + nx) as usize;
                    if !background.get_index(n) && !selected.get_index(n) {
                        selected.set_index(n);
                        ensure!(
                            queue.len() < 4_000_000,
                            "Selection is too complex; use a smaller area"
                        );
                        queue.push_back(n as u32);
                    }
                }
            }
        }
        Ok(selected)
    }
    pub fn empty(width: u32, height: u32) -> Result<Self> {
        ensure!(
            width > 0
                && height > 0
                && width <= 16_384
                && height <= 16_384
                && u64::from(width) * u64::from(height) <= 32_000_000,
            "Invalid selection dimensions"
        );
        Ok(Self {
            width,
            height,
            bits: vec![0; (width as usize * height as usize).div_ceil(64)],
        })
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.width > 0
                && self.height > 0
                && self.width <= 16_384
                && self.height <= 16_384
                && u64::from(self.width) * u64::from(self.height) <= 32_000_000,
            "Invalid selection dimensions"
        );
        let count = self.width as usize * self.height as usize;
        ensure!(
            self.bits.len() == count.div_ceil(64),
            "Invalid selection data"
        );
        if !count.is_multiple_of(64) {
            ensure!(
                self.bits.last().copied().unwrap_or(0) >> (count % 64) == 0,
                "Invalid selection padding"
            );
        }
        Ok(())
    }
    pub(crate) fn get_index(&self, i: usize) -> bool {
        self.bits[i / 64] & (1 << (i % 64)) != 0
    }
    pub(crate) fn set_index(&mut self, i: usize) {
        self.bits[i / 64] |= 1 << (i % 64);
    }
    pub fn contains(&self, x: u32, y: u32) -> bool {
        x < self.width
            && y < self.height
            && self.get_index(y as usize * self.width as usize + x as usize)
    }
    pub fn contains_uv(&self, u: f32, v: f32) -> bool {
        u.is_finite()
            && v.is_finite()
            && (0.0..1.0).contains(&u)
            && (0.0..1.0).contains(&v)
            && self.contains(
                (u * self.width as f32) as u32,
                (v * self.height as f32) as u32,
            )
    }
    pub fn any(&self) -> bool {
        self.bits.iter().any(|word| *word != 0)
    }
    pub fn combine(&mut self, other: &Self, mode: SelectionMode) -> Result<()> {
        ensure!(
            (self.width, self.height) == (other.width, other.height),
            "Selection sizes differ"
        );
        for (a, b) in self.bits.iter_mut().zip(&other.bits) {
            *a = match mode {
                SelectionMode::Replace => *b,
                SelectionMode::Add => *a | *b,
                SelectionMode::Subtract => *a & !*b,
            };
        }
        Ok(())
    }
    pub fn intersect(&mut self, other: &Self) -> Result<()> {
        ensure!(
            (self.width, self.height) == (other.width, other.height),
            "Selection sizes differ"
        );
        for (a, b) in self.bits.iter_mut().zip(&other.bits) {
            *a &= *b;
        }
        Ok(())
    }
    pub fn polygon(width: u32, height: u32, points: &[[f32; 2]]) -> Result<Self> {
        let mut result = Self::empty(width, height)?;
        ensure!(
            points.is_empty()
                || (points.len() >= 3
                    && points.len() <= 100_000
                    && points.iter().flatten().all(|p| p.is_finite())),
            "Invalid selection polygon"
        );
        if points.is_empty() {
            result.bits.fill(u64::MAX);
            let count = width as usize * height as usize;
            if !count.is_multiple_of(64) {
                *result.bits.last_mut().unwrap() = (1 << (count % 64)) - 1;
            }
            return Ok(result);
        }
        let mut crossings = Vec::with_capacity(points.len());
        for y in 0..height {
            let v = (y as f32 + 0.5) / height as f32;
            crossings.clear();
            for (a, b) in points
                .iter()
                .zip(points.iter().cycle().skip(1))
                .take(points.len())
            {
                if (a[1] > v) != (b[1] > v) {
                    crossings.push(a[0] + (v - a[1]) * (b[0] - a[0]) / (b[1] - a[1]));
                }
            }
            crossings.sort_unstable_by(f32::total_cmp);
            for pair in crossings.as_chunks::<2>().0 {
                let start = (pair[0] * width as f32 - 0.5)
                    .ceil()
                    .clamp(0.0, width as f32) as u32;
                let end = (pair[1] * width as f32 - 0.5)
                    .ceil()
                    .clamp(0.0, width as f32) as u32;
                for x in start..end {
                    result.set_index(y as usize * width as usize + x as usize);
                }
            }
        }
        Ok(result)
    }
    /// Rasterize a circular screen brush mapped to original image coordinates.
    /// Elliptical source radii preserve a circular cursor on stretched images.
    pub fn brush(width: u32, height: u32, points: &[[f32; 2]], radii: [f32; 2]) -> Result<Self> {
        let mut mask = Self::empty(width, height)?;
        ensure!(
            !points.is_empty()
                && points.len() <= 2000
                && points.iter().flatten().all(|v| v.is_finite())
                && radii.iter().all(|r| r.is_finite() && *r > 0.0),
            "Invalid selection brush"
        );
        let rx = radii[0].clamp(0.5, 16384.0);
        let ry = radii[1].clamp(0.5, 16384.0);
        let mut work = 0_u64;
        for (a, b) in points
            .iter()
            .zip(points.iter().skip(1).chain(points.last()))
        {
            let left = (a[0].min(b[0]) - rx).floor().clamp(0.0, width as f32) as u32;
            let right = (a[0].max(b[0]) + rx).ceil().clamp(0.0, width as f32) as u32;
            let top = (a[1].min(b[1]) - ry).floor().clamp(0.0, height as f32) as u32;
            let bottom = (a[1].max(b[1]) + ry).ceil().clamp(0.0, height as f32) as u32;
            work += u64::from(right - left) * u64::from(bottom - top);
            ensure!(
                work <= 64_000_000,
                "Use a shorter selection stroke or a smaller brush"
            );
            let dx = (b[0] - a[0]) / rx;
            let dy = (b[1] - a[1]) / ry;
            let length = dx * dx + dy * dy;
            for y in top..bottom {
                for x in left..right {
                    let px = (x as f32 + 0.5 - a[0]) / rx;
                    let py = (y as f32 + 0.5 - a[1]) / ry;
                    let t = if length > 0.0 {
                        ((px * dx + py * dy) / length).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    if (px - t * dx).powi(2) + (py - t * dy).powi(2) <= 1.0 {
                        mask.set_index((y * width + x) as usize);
                    }
                }
            }
            // A one-pixel brush still selects its hit pixel at any subpixel position.
            for p in [a, b] {
                if p[0] >= 0.0 && p[1] >= 0.0 && p[0] < width as f32 && p[1] < height as f32 {
                    mask.set_index((p[1] as u32 * width + p[0] as u32) as usize);
                }
            }
        }
        ensure!(mask.any(), "Brush inside the image");
        Ok(mask)
    }

    /// Grow all touched color regions in one bounded multi-source flood, rather
    /// than decoding/flooding the whole image once for every pointer sample.
    pub fn smart_brush(
        image: &RgbaImage,
        points: &[[f32; 2]],
        radii: [f32; 2],
        tolerance: u8,
    ) -> Result<Self> {
        let (w, h) = image.dimensions();
        let mut selected = Self::brush(w, h, points, radii)?;
        // When the brush touches artwork, transparent padding is not another
        // color region to flood across the entire source image.
        let prefer_artwork = image
            .pixels()
            .enumerate()
            .any(|(i, p)| selected.get_index(i) && p[3] > 0);
        if prefer_artwork {
            for (i, p) in image.pixels().enumerate() {
                if p[3] == 0 {
                    selected.bits[i / 64] &= !(1_u64 << (i % 64));
                }
            }
        }
        let mut queue = VecDeque::new();
        for y in 0..h {
            for x in 0..w {
                if selected.contains(x, y)
                    && [
                        (x.wrapping_sub(1), y),
                        (x + 1, y),
                        (x, y.wrapping_sub(1)),
                        (x, y + 1),
                    ]
                    .iter()
                    .any(|&(nx, ny)| nx < w && ny < h && !selected.contains(nx, ny))
                {
                    ensure!(queue.len() < 4_000_000, "Use a smaller selection brush");
                    queue.push_back((y * w + x, image.get_pixel(x, y).0));
                }
            }
        }
        while let Some((i, target)) = queue.pop_front() {
            let (x, y) = (i % w, i / w);
            for (nx, ny) in [
                (x.wrapping_sub(1), y),
                (x + 1, y),
                (x, y.wrapping_sub(1)),
                (x, y + 1),
            ] {
                if nx >= w || ny >= h || selected.contains(nx, ny) {
                    continue;
                }
                let pixel = image.get_pixel(nx, ny).0;
                if prefer_artwork && pixel[3] == 0 {
                    continue;
                }
                let similar = pixel[3].abs_diff(target[3]) <= tolerance
                    && (0..3).all(|c| {
                        let a = u32::from(pixel[c]) * u32::from(pixel[3]) / 255;
                        let b = u32::from(target[c]) * u32::from(target[3]) / 255;
                        a.abs_diff(b) <= u32::from(tolerance)
                    });
                if similar {
                    selected.set_index((ny * w + nx) as usize);
                    ensure!(
                        queue.len() < 4_000_000,
                        "Selection is too complex; use a smaller tolerance"
                    );
                    queue.push_back((ny * w + nx, target));
                }
            }
        }
        Ok(selected)
    }
    /// Connected, seed-relative color selection. Transparent RGB is ignored.
    pub fn smart(image: &RgbaImage, seed: [u32; 2], tolerance: u8) -> Result<Self> {
        let (width, height) = image.dimensions();
        ensure!(
            seed[0] < width && seed[1] < height,
            "Click inside the selected image"
        );
        let mut selected = Self::empty(width, height)?;
        let mut visited = Self::empty(width, height)?;
        let target = image.get_pixel(seed[0], seed[1]).0;
        let similar = |pixel: [u8; 4]| {
            let mut difference = (i32::from(pixel[3]) - i32::from(target[3])).unsigned_abs();
            for c in 0..3 {
                let a = u32::from(pixel[c]) * u32::from(pixel[3]) / 255;
                let b = u32::from(target[c]) * u32::from(target[3]) / 255;
                difference = difference.max(a.abs_diff(b));
            }
            difference <= u32::from(tolerance)
        };
        let first = seed[1] * width + seed[0];
        let mut queue = VecDeque::from([first]);
        visited.set_index(first as usize);
        selected.set_index(first as usize);
        while let Some(i) = queue.pop_front() {
            let (x, y) = (i % width, i / width);
            for (nx, ny) in [
                (x.wrapping_sub(1), y),
                (x + 1, y),
                (x, y.wrapping_sub(1)),
                (x, y + 1),
            ] {
                if nx >= width || ny >= height {
                    continue;
                }
                let n = (ny * width + nx) as usize;
                if visited.get_index(n) {
                    continue;
                }
                visited.set_index(n);
                if similar(image.get_pixel(nx, ny).0) {
                    selected.set_index(n);
                    ensure!(
                        queue.len() < 4_000_000,
                        "Selection is too complex; try a smaller tolerance"
                    );
                    queue.push_back(n as u32);
                }
            }
        }
        Ok(selected)
    }
    /// Matching source color, optionally including disconnected enclosed gaps.
    pub fn color_region(
        image: &RgbaImage,
        seed: [u32; 2],
        tolerance: u8,
        connected: bool,
    ) -> Result<Self> {
        if connected {
            return Self::smart(image, seed, tolerance);
        }
        let (w, h) = image.dimensions();
        ensure!(seed[0] < w && seed[1] < h, "Click inside the image");
        let target = image.get_pixel(seed[0], seed[1]).0;
        let mut result = Self::empty(w, h)?;
        for (i, p) in image.pixels().enumerate() {
            if p[3].abs_diff(target[3]) <= tolerance
                && (0..3).all(|c| {
                    (u32::from(p[c]) * u32::from(p[3]) / 255)
                        .abs_diff(u32::from(target[c]) * u32::from(target[3]) / 255)
                        <= u32::from(tolerance)
                })
            {
                result.set_index(i);
            }
        }
        Ok(result)
    }
    pub fn bounds(&self) -> Option<[u32; 4]> {
        let (mut left, mut top, mut right, mut bottom) = (self.width, self.height, 0, 0);
        for y in 0..self.height {
            let row_start = y as usize * self.width as usize;
            let row_end = row_start + self.width as usize;
            for wi in row_start / 64..row_end.div_ceil(64) {
                let lo = row_start.saturating_sub(wi * 64);
                let hi = (row_end - wi * 64).min(64);
                let word = self.bits[wi] & (u64::MAX << lo) & (u64::MAX >> (64 - hi));
                if word == 0 {
                    continue;
                }
                let x0 = (wi * 64 + word.trailing_zeros() as usize - row_start) as u32;
                let x1 = (wi * 64 + 64 - word.leading_zeros() as usize - row_start) as u32;
                left = left.min(x0);
                right = right.max(x1);
                top = top.min(y);
                bottom = bottom.max(y + 1);
            }
        }
        if left < right && top < bottom {
            Some([left, top, right - left, bottom - top])
        } else {
            None
        }
    }
}
impl ImageClip {
    pub fn validate(&self, width: u32, height: u32) -> Result<()> {
        self.selection.validate()?;
        ensure!(
            (self.selection.width, self.selection.height) == (width, height),
            "Selection does not match original image dimensions"
        );
        ensure!(
            self.edge.width <= 512 && (2..=128).contains(&self.edge.dot_scale),
            "Invalid selection edge settings"
        );
        self.edge.backing.validate()?;
        Ok(())
    }
}

/// Fixed-point chamfer distance, eight units per source pixel. Only opposite
/// selection pixels seed the field; the image boundary is not an invented crop.
pub(crate) fn distances(mask: &SelectionMask, cap: u16) -> Vec<u16> {
    let w = mask.width as usize;
    let h = mask.height as usize;
    let mut d = vec![cap; w * h];
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let inside = mask.get_index(i);
            if (x > 0 && mask.get_index(i - 1) != inside)
                || (x + 1 < w && mask.get_index(i + 1) != inside)
                || (y > 0 && mask.get_index(i - w) != inside)
                || (y + 1 < h && mask.get_index(i + w) != inside)
            {
                d[i] = 4;
            }
        }
    }
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if x > 0 {
                d[i] = d[i].min(d[i - 1].saturating_add(8));
            }
            if y > 0 {
                d[i] = d[i].min(d[i - w].saturating_add(8));
                if x > 0 {
                    d[i] = d[i].min(d[i - w - 1].saturating_add(11));
                }
                if x + 1 < w {
                    d[i] = d[i].min(d[i - w + 1].saturating_add(11));
                }
            }
        }
    }
    for y in (0..h).rev() {
        for x in (0..w).rev() {
            let i = y * w + x;
            if x + 1 < w {
                d[i] = d[i].min(d[i + 1].saturating_add(8));
            }
            if y + 1 < h {
                d[i] = d[i].min(d[i + w].saturating_add(8));
                if x > 0 {
                    d[i] = d[i].min(d[i + w - 1].saturating_add(11));
                }
                if x + 1 < w {
                    d[i] = d[i].min(d[i + w + 1].saturating_add(11));
                }
            }
        }
    }
    d
}

/// Straight-alpha source pixels in, straight-alpha cutout pixels out.
pub fn apply(image: &mut RgbaImage, clip: &ImageClip) -> Result<()> {
    clip.validate(image.width(), image.height())?;
    let mask = &clip.selection;
    let edge = clip.edge;
    if edge.width == 0
        || (edge.pattern == EdgePattern::Hard
            && edge.contour == Contour::Center
            && edge.background[3] == 0)
    {
        for (i, pixel) in image.pixels_mut().enumerate() {
            if !mask.get_index(i) {
                *pixel = Rgba([0; 4]);
            }
        }
        return crate::cutout::apply_backing(image, edge.backing);
    }
    let field = distances(mask, (edge.width + 2) * 8);
    let width = f32::from(edge.width);
    let lower = match edge.contour {
        Contour::Inside => 0.0,
        Contour::Center => -width * 0.5,
        Contour::Outside => -width,
    };
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        let i = (y * mask.width + x) as usize;
        let distance = field[i] as f32 / 8.0 * if mask.get_index(i) { 1.0 } else { -1.0 };
        let t = ((distance - lower) / width).clamp(0.0, 1.0);
        let coverage = match edge.pattern {
            EdgePattern::Hard => {
                if t >= 0.5 {
                    1.0
                } else {
                    0.0
                }
            }
            EdgePattern::Fade => t * t * (3.0 - 2.0 * t),
            EdgePattern::Dotted => {
                if t == 0.0 || t == 1.0 {
                    t
                } else {
                    let spacing = f32::from(edge.dot_scale);
                    let row = (y as f32 / spacing).floor() as u32;
                    let dx = (x as f32
                        + 0.5
                        + if row.is_multiple_of(2) {
                            0.0
                        } else {
                            spacing * 0.5
                        })
                    .rem_euclid(spacing)
                        - spacing * 0.5;
                    let dy = (y as f32 + 0.5).rem_euclid(spacing) - spacing * 0.5;
                    (t.sqrt() * spacing * std::f32::consts::FRAC_1_SQRT_2 - dx.hypot(dy) + 0.5)
                        .clamp(0.0, 1.0)
                }
            }
        };
        // Background is confined to this finite contour band. Its two ends
        // become transparent, including the exterior; it never floods the asset.
        let band = 4.0 * t * (1.0 - t);
        let foreground = pixel[3] as f32 / 255.0 * coverage;
        let background =
            edge.background[3] as f32 / 255.0 * band * (1.0 - coverage) * (1.0 - foreground);
        let alpha = foreground + background;
        if alpha <= 0.0 {
            *pixel = Rgba([0; 4]);
            continue;
        }
        for c in 0..3 {
            pixel[c] = ((pixel[c] as f32 * foreground + edge.background[c] as f32 * background)
                / alpha)
                .round() as u8;
        }
        pixel[3] = (alpha * 255.0).round() as u8;
    }
    crate::cutout::apply_backing(image, edge.backing)
}

/// A bounded trailing filter in chapter coordinates; screen-space radius is
/// independent of zoom. Zero preserves raw input, and release keeps the endpoint.
pub fn stabilize(previous: [f32; 2], point: [f32; 2], strength: f32, zoom: f32) -> [f32; 2] {
    let radius = strength.clamp(0.0, 4.0) * 30.0 / zoom.max(0.02);
    let distance = (point[0] - previous[0]).hypot(point[1] - previous[1]);
    let amount = if radius == 0.0 {
        1.0
    } else {
        distance / (distance + radius)
    };
    [
        previous[0] + (point[0] - previous[0]) * amount,
        previous[1] + (point[1] - previous[1]) * amount,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    fn rect(left: f32, top: f32, right: f32, bottom: f32) -> Vec<[f32; 2]> {
        vec![[left, top], [right, top], [right, bottom], [left, bottom]]
    }
    #[test]
    fn combinations_keep_disconnected_islands_and_subtracted_holes_at_source_resolution() {
        let mut mask = SelectionMask::polygon(257, 193, &rect(0.1, 0.1, 0.4, 0.9)).unwrap();
        let other = SelectionMask::polygon(257, 193, &rect(0.6, 0.1, 0.9, 0.9)).unwrap();
        mask.combine(&other, SelectionMode::Add).unwrap();
        let hole = SelectionMask::polygon(257, 193, &rect(0.2, 0.3, 0.3, 0.7)).unwrap();
        mask.combine(&hole, SelectionMode::Subtract).unwrap();
        assert!(mask.contains_uv(0.15, 0.5));
        assert!(mask.contains_uv(0.8, 0.5));
        assert!(!mask.contains_uv(0.5, 0.5));
        assert!(!mask.contains_uv(0.25, 0.5));
        assert_eq!(mask.bounds(), Some([26, 19, 205, 155]));
        mask.validate().unwrap();
        let saved = serde_json::to_vec(&mask).unwrap();
        assert_eq!(
            serde_json::from_slice::<SelectionMask>(&saved).unwrap(),
            mask
        );
        mask.combine(&mask.clone(), SelectionMode::Subtract)
            .unwrap();
        assert!(!mask.any());
        assert_eq!(mask.bounds(), None);
    }
    #[test]
    fn smart_selection_is_connected_seed_relative_and_alpha_aware() {
        let mut image = RgbaImage::from_pixel(9, 5, Rgba([120, 40, 20, 255]));
        for y in 0..5 {
            image.put_pixel(4, y, Rgba([240, 240, 240, 255]));
        }
        image.put_pixel(0, 0, Rgba([126, 40, 20, 255]));
        let mask = SelectionMask::smart(&image, [1, 1], 8).unwrap();
        assert!(mask.contains(0, 0));
        assert!(mask.contains(3, 4));
        assert!(!mask.contains(5, 1));
        let strict = SelectionMask::smart(&image, [1, 1], 0).unwrap();
        assert!(!strict.contains(0, 0));
        image = RgbaImage::from_fn(9, 5, |x, y| Rgba([x as u8 * 20, y as u8 * 40, 220, 0]));
        let transparent = SelectionMask::smart(&image, [0, 0], 0).unwrap();
        assert!(transparent.contains(8, 4));
        assert!(SelectionMask::smart(&image, [9, 0], 0).is_err());
    }
    #[test]
    fn all_contours_and_patterns_keep_background_confined_to_the_crop_band() {
        let selection =
            Arc::new(SelectionMask::polygon(128, 96, &rect(0.25, 0.25, 0.75, 0.75)).unwrap());
        let source = RgbaImage::from_pixel(128, 96, Rgba([20, 60, 90, 180]));
        for pattern in EdgePattern::ALL {
            for contour in Contour::ALL {
                let clip = ImageClip {
                    selection: selection.clone(),
                    edge: EdgeSettings {
                        pattern,
                        contour,
                        width: 16,
                        dot_scale: 8,
                        background: [250, 10, 30, 160],
                        backing: Default::default(),
                    },
                };
                let mut cut = source.clone();
                apply(&mut cut, &clip).unwrap();
                assert_eq!(cut.get_pixel(0, 0).0, [0; 4]);
                assert_eq!(cut.get_pixel(127, 95).0, [0; 4]);
                assert_eq!(cut.get_pixel(64, 48), source.get_pixel(64, 48));
                assert_eq!(cut.get_pixel(10, 48).0, [0; 4]);
                if contour == Contour::Inside {
                    assert_eq!(cut.get_pixel(31, 48).0, [0; 4]);
                }
                if contour == Contour::Outside {
                    assert_eq!(cut.get_pixel(32, 48), source.get_pixel(32, 48));
                }
                let mut transparent = cut.clone();
                let mut no_bg = clip.clone();
                no_bg.edge.background[3] = 0;
                transparent.clone_from(&source);
                apply(&mut transparent, &no_bg).unwrap();
                assert!(cut
                    .pixels()
                    .zip(transparent.pixels())
                    .all(|(a, b)| a[3] >= b[3]));
            }
        }
        assert!(source.pixels().all(|p| p.0 == [20, 60, 90, 180]));
    }
    #[test]
    fn dot_scale_changes_the_pattern_and_fade_has_continuous_alpha() {
        let selection =
            Arc::new(SelectionMask::polygon(128, 64, &rect(0.0, 0.0, 0.5, 1.0)).unwrap());
        let source = RgbaImage::from_pixel(128, 64, Rgba([40, 70, 90, 255]));
        let mut clip = ImageClip {
            selection,
            edge: EdgeSettings {
                pattern: EdgePattern::Dotted,
                width: 48,
                ..Default::default()
            },
        };
        let mut small = source.clone();
        clip.edge.dot_scale = 4;
        apply(&mut small, &clip).unwrap();
        let mut large = source.clone();
        clip.edge.dot_scale = 24;
        apply(&mut large, &clip).unwrap();
        assert_ne!(small, large);
        clip.edge.pattern = EdgePattern::Fade;
        let mut fade = source.clone();
        apply(&mut fade, &clip).unwrap();
        let alphas: Vec<_> = (40..88).map(|x| fade.get_pixel(x, 32)[3]).collect();
        assert!(alphas.windows(2).all(|p| p[0] >= p[1]));
        assert!(
            alphas
                .iter()
                .copied()
                .collect::<std::collections::HashSet<_>>()
                .len()
                > 30
        );
    }
    #[test]
    fn invalid_masks_and_settings_fail_before_rendering() {
        assert!(SelectionMask::empty(u32::MAX, u32::MAX).is_err());
        let mut bad = SelectionMask::empty(3, 3).unwrap();
        bad.bits[0] = u64::MAX;
        assert!(bad.validate().is_err());
        let mut image = RgbaImage::new(3, 3);
        let clip = ImageClip {
            selection: Arc::new(SelectionMask::empty(4, 4).unwrap()),
            edge: EdgeSettings::default(),
        };
        assert!(apply(&mut image, &clip).is_err());
        assert_eq!(stabilize([0.0, 0.0], [20.0, 10.0], 0.0, 1.0), [20.0, 10.0]);
        assert!(stabilize([0.0, 0.0], [20.0, 10.0], 1.0, 1.0)[0] < 20.0);
    }
    #[test]
    fn smart_brush_width_samples_regions_and_strokes_without_crossing_unpainted_gaps() {
        let mut image = RgbaImage::from_pixel(80, 48, image::Rgba([0, 0, 0, 255]));
        for y in 0..48 {
            for x in 0..80 {
                if !(20..60).contains(&x) {
                    image.put_pixel(x, y, image::Rgba([220, 40, 30, 255]));
                } else if x < 40 {
                    image.put_pixel(x, y, image::Rgba([25, 150, 80, 255]));
                }
            }
        }
        let narrow = SelectionMask::smart_brush(&image, &[[17.5, 24.5]], [1.0; 2], 0).unwrap();
        assert!(narrow.contains(5, 5));
        assert!(!narrow.contains(25, 24));
        let wide = SelectionMask::smart_brush(&image, &[[17.5, 24.5]], [6.0; 2], 0).unwrap();
        assert!(wide.contains(25, 5));
        assert!(!wide.contains(50, 24));
        assert!(
            !wide.contains(70, 24),
            "Disconnected same-color region must stay outside"
        );
        let stroke =
            SelectionMask::brush(80, 48, &[[5.5, 20.5], [65.5, 20.5]], [3.0, 6.0]).unwrap();
        assert!(stroke.contains(40, 25));
        assert!(!stroke.contains(40, 28));
        assert!(!stroke.contains(70, 20));
        assert!(SelectionMask::brush(80, 48, &[[f32::NAN, 0.0]], [3.0; 2]).is_err());
    }
    #[test]
    fn subject_brush_catches_a_near_miss_without_selecting_background() {
        let mut image = RgbaImage::new(64, 64);
        for y in 10..54 {
            for x in 24..40 {
                image.put_pixel(x, y, image::Rgba([35, 145, 180, 255]));
            }
        }
        assert!(SelectionMask::subject(&image, [21, 32], 24).is_err());
        let mask = SelectionMask::subject_brush(&image, &[[21.5, 32.5]], [6.0; 2], 24).unwrap();
        assert!(mask.contains(32, 12));
        assert!(!mask.contains(21, 32));
        assert!(!mask.contains(0, 0));
    }
    #[test]
    fn stabilizer_extends_to_four_times_the_old_radius_without_changing_old_values() {
        assert_eq!(stabilize([0.0; 2], [120.0, 0.0], 1.0, 1.0), [96.0, 0.0]);
        assert_eq!(stabilize([0.0; 2], [120.0, 0.0], 4.0, 1.0), [60.0, 0.0]);
        assert_eq!(
            stabilize([0.0; 2], [120.0, 0.0], 4.0, 1.0),
            stabilize([0.0; 2], [120.0, 0.0], 1.0, 0.25)
        );
    }
    #[test]
    fn wide_smart_brush_ignores_transparent_padding_when_it_touches_artwork() {
        let mut image = RgbaImage::new(64, 64);
        for y in 10..54 {
            for x in 24..40 {
                image.put_pixel(x, y, image::Rgba([40, 150, 180, 255]));
            }
        }
        let mask = SelectionMask::smart_brush(&image, &[[22.5, 32.5]], [6.0; 2], 28).unwrap();
        assert!(mask.contains(32, 12));
        assert_eq!(mask.bounds(), Some([24, 10, 16, 44]));
        let empty_area = SelectionMask::smart_brush(&image, &[[5.5, 5.5]], [1.0; 2], 0).unwrap();
        assert!(empty_area.contains(60, 60));
        assert!(!empty_area.contains(32, 32));
    }
}
