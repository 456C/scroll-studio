//! Full-resolution RGBA painting with cached subpixel stamps and tiled stroke history.
use image::RgbaImage;
use std::collections::{HashMap, HashSet, VecDeque};

pub const TILE: u32 = 128;
pub type Tile = (u32, u32);
pub type Bounds = [u32; 4];
const HISTORY_BYTES: usize = 96 * 1024 * 1024;
const FULL: u32 = 65535;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tip {
    Ink,
    Pencil,
    Marker,
    Airbrush,
    Eraser,
}
impl Tip {
    pub const ALL: [Self; 5] = [
        Self::Ink,
        Self::Pencil,
        Self::Marker,
        Self::Airbrush,
        Self::Eraser,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Ink => "Ink",
            Self::Pencil => "Pencil",
            Self::Marker => "Marker",
            Self::Airbrush => "Airbrush",
            Self::Eraser => "Eraser",
        }
    }
}
#[derive(Clone, Copy)]
pub struct Brush {
    pub tip: Tip,
    pub size: f32,
    pub flow: f32,
    pub opacity: f32,
    pub softness_percent: f32,
    pub spacing_percent: f32,
    pub color: [u8; 3],
}
impl Default for Brush {
    fn default() -> Self {
        Self {
            tip: Tip::Ink,
            size: 9.0,
            flow: 1.0,
            opacity: 1.0,
            softness_percent: 0.0,
            spacing_percent: 12.0,
            color: [28, 30, 34],
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
struct StampKey {
    tip: Tip,
    diameter: u16,
    softness: u8,
}
struct Stamp {
    key: StampKey,
    side: u32,
    mask: Vec<u16>,
}
impl Stamp {
    fn new(key: StampKey) -> Self {
        let radius = f32::from(key.diameter) * 0.125;
        let side = (radius * 2.0).ceil() as u32 + 4;
        let center = side as f32 * 0.5;
        let feather = (radius * f32::from(key.softness) * 0.01).max(1.0);
        let mut mask = vec![0; (side * side) as usize];
        let round = key.tip != Tip::Marker;
        for y in 0..side.div_ceil(2) {
            for x in 0..if round { side.div_ceil(2) } else { side } {
                let dx = x as f32 + 0.5 - center;
                let dy = y as f32 + 0.5 - center;
                let distance = if key.tip == Tip::Marker {
                    ((dx * 0.87 + dy * 0.5).powi(2) + ((dy * 0.87 - dx * 0.5) * 2.6).powi(2)).sqrt()
                } else {
                    (dx * dx + dy * dy).sqrt()
                };
                let edge = ((radius + 0.5 - distance) / feather).clamp(0.0, 1.0);
                let coverage = if key.tip == Tip::Airbrush {
                    edge * edge * (1.0 - distance / radius).max(0.0)
                } else {
                    edge
                };
                let value = (coverage * FULL as f32).round() as u16;
                mask[(y * side + x) as usize] = value;
                mask[((side - 1 - y) * side + side - 1 - x) as usize] = value;
                if round {
                    mask[(y * side + side - 1 - x) as usize] = value;
                    mask[((side - 1 - y) * side + x) as usize] = value;
                }
            }
        }
        Self { key, side, mask }
    }
    fn sample(&self, x: u32, y: u32, fx: u32, fy: u32) -> u32 {
        if x == 0 || y == 0 || x >= self.side || y >= self.side {
            return 0;
        }
        let p = (y * self.side + x) as usize;
        let side = self.side as usize;
        let a = (u32::from(self.mask[p]) * (256 - fx) + u32::from(self.mask[p - 1]) * fx) >> 8;
        let b = (u32::from(self.mask[p - side]) * (256 - fx)
            + u32::from(self.mask[p - side - 1]) * fx)
            >> 8;
        (a * (256 - fy) + b * fy) >> 8
    }
}
struct StrokeTile {
    before: RgbaImage,
    coverage: Vec<u16>,
    changed: bool,
}
struct Edit {
    tiles: HashMap<Tile, RgbaImage>,
    revision: u64,
}
impl Edit {
    fn bytes(&self) -> usize {
        self.tiles.values().map(|t| t.as_raw().len()).sum()
    }
}

pub struct Painting {
    pub image: RgbaImage,
    pub invalidated: HashSet<Tile>,
    dirty: HashMap<Tile, Bounds>,
    active: HashMap<Tile, StrokeTile>,
    stamps: VecDeque<Stamp>,
    undo: VecDeque<Edit>,
    redo: Vec<Edit>,
    history_bytes: usize,
    pub revision: u64,
    serial: u64,
}
impl Painting {
    pub fn new(image: RgbaImage) -> Self {
        Self {
            image,
            invalidated: HashSet::new(),
            dirty: HashMap::new(),
            active: HashMap::new(),
            stamps: VecDeque::new(),
            undo: VecDeque::new(),
            redo: Vec::new(),
            history_bytes: 0,
            revision: 0,
            serial: 0,
        }
    }
    pub fn tile(&self, (x, y): Tile) -> RgbaImage {
        let (x, y) = (x * TILE, y * TILE);
        image::imageops::crop_imm(
            &self.image,
            x,
            y,
            TILE.min(self.image.width() - x),
            TILE.min(self.image.height() - y),
        )
        .to_image()
    }
    pub fn take_updates(&mut self) -> impl Iterator<Item = (Tile, Bounds)> + '_ {
        let dirty = &mut self.dirty;
        self.invalidated
            .drain()
            .filter_map(move |tile| dirty.remove(&tile).map(|bounds| (tile, bounds)))
    }
    fn mark_dirty(&mut self, tile: Tile, bounds: Bounds) {
        self.invalidated.insert(tile);
        self.dirty
            .entry(tile)
            .and_modify(|old| {
                old[0] = old[0].min(bounds[0]);
                old[1] = old[1].min(bounds[1]);
                old[2] = old[2].max(bounds[2]);
                old[3] = old[3].max(bounds[3]);
            })
            .or_insert(bounds);
    }

    pub fn dab(&mut self, center: [f32; 2], brush: Brush) {
        if !center.iter().all(|v| v.is_finite()) || !brush.size.is_finite() {
            return;
        }
        let key = StampKey {
            tip: brush.tip,
            diameter: (brush.size.clamp(0.5, 512.0) * 4.0).round() as u16,
            softness: brush.softness_percent.clamp(0.0, 100.0).round() as u8,
        };
        if let Some(index) = self.stamps.iter().position(|s| s.key == key) {
            if index != 0 {
                let stamp = self.stamps.remove(index).unwrap();
                self.stamps.push_front(stamp);
            }
        } else {
            self.stamps.push_front(Stamp::new(key));
            if self.stamps.len() > 12 {
                self.stamps.pop_back();
            }
        }
        let side = self.stamps[0].side;
        let ox = center[0] - side as f32 * 0.5;
        let oy = center[1] - side as f32 * 0.5;
        let left = ox.floor() as i32;
        let top = oy.floor() as i32;
        let fx = (ox.fract().rem_euclid(1.0) * 256.0).round().min(255.0) as u32;
        let fy = (oy.fract().rem_euclid(1.0) * 256.0).round().min(255.0) as u32;
        let x0 = left.max(0).min(self.image.width() as i32) as u32;
        let y0 = top.max(0).min(self.image.height() as i32) as u32;
        let x1 = (left.saturating_add(side as i32))
            .max(0)
            .min(self.image.width() as i32) as u32;
        let y1 = (top.saturating_add(side as i32))
            .max(0)
            .min(self.image.height() as i32) as u32;
        let flow = (brush.flow.clamp(0.0, 1.0) * FULL as f32).round() as u32;
        let opacity = (brush.opacity.clamp(0.0, 1.0) * FULL as f32).round() as u32;
        if x0 >= x1 || y0 >= y1 || flow == 0 || opacity == 0 {
            return;
        }
        let image_width = self.image.width();
        for ty in y0 / TILE..y1.div_ceil(TILE) {
            for tx in x0 / TILE..x1.div_ceil(TILE) {
                let tile = (tx, ty);
                if !self.active.contains_key(&tile) {
                    let before = self.tile(tile);
                    let coverage = vec![0; (before.width() * before.height()) as usize];
                    self.active.insert(
                        tile,
                        StrokeTile {
                            before,
                            coverage,
                            changed: false,
                        },
                    );
                }
                let stamp = &self.stamps[0];
                let edit = self.active.get_mut(&tile).unwrap();
                let tw = edit.before.width();
                let old = edit.before.as_raw();
                let pixels: &mut [u8] = self.image.as_mut();
                let xmin = x0.max(tx * TILE);
                let xmax = x1.min((tx + 1) * TILE);
                let ymin = y0.max(ty * TILE);
                let ymax = y1.min((ty + 1) * TILE);
                let mut changed = false;
                for y in ymin..ymax {
                    for x in xmin..xmax {
                        let mut coverage =
                            stamp.sample((x as i32 - left) as u32, (y as i32 - top) as u32, fx, fy);
                        if brush.tip == Tip::Pencil {
                            let grain = x
                                .wrapping_mul(374761393)
                                .wrapping_add(y.wrapping_mul(668265263));
                            let noise = ((grain ^ (grain >> 13)).wrapping_mul(1274126177) >> 24)
                                * 200
                                + 14400;
                            coverage = coverage * noise / FULL;
                        }
                        let alpha = coverage * flow / FULL;
                        if alpha == 0 {
                            continue;
                        }
                        let local = ((y - ty * TILE) * tw + x - tx * TILE) as usize;
                        let previous = u32::from(edit.coverage[local]);
                        let accumulated = previous + alpha * (FULL - previous) / FULL;
                        if previous == accumulated {
                            continue;
                        }
                        edit.coverage[local] = accumulated as u16;
                        let sa = accumulated * opacity / FULL;
                        let original = &old[local * 4..local * 4 + 4];
                        let offset = ((y * image_width + x) * 4) as usize;
                        let pixel = &mut pixels[offset..offset + 4];
                        let mut next = [0; 4];
                        if brush.tip == Tip::Eraser {
                            next.copy_from_slice(original);
                            next[3] =
                                ((u32::from(original[3]) * (FULL - sa) + FULL / 2) / FULL) as u8;
                            if next[3] == 0 && original[3] != 0 {
                                next = [0; 4];
                            }
                        } else {
                            let weight =
                                (u32::from(original[3]) * 257 * (FULL - sa) + FULL / 2) / FULL;
                            let out = sa + weight;
                            if out == 0 {
                                continue;
                            }
                            for c in 0..3 {
                                next[c] = ((u32::from(brush.color[c]) * sa
                                    + u32::from(original[c]) * weight
                                    + out / 2)
                                    / out) as u8;
                            }
                            next[3] = ((out + 128) / 257) as u8;
                        }
                        if pixel != next {
                            pixel.copy_from_slice(&next);
                            changed = true;
                        }
                    }
                }
                edit.changed |= changed;
                if changed {
                    self.mark_dirty(
                        tile,
                        [
                            xmin - tx * TILE,
                            ymin - ty * TILE,
                            xmax - tx * TILE,
                            ymax - ty * TILE,
                        ],
                    );
                }
            }
        }
    }
    pub fn finish(&mut self) {
        let tiles = self
            .active
            .drain()
            .filter(|(_, t)| t.changed)
            .map(|(id, t)| (id, t.before))
            .collect::<HashMap<_, _>>();
        if tiles.is_empty() {
            return;
        }
        let edit = Edit {
            tiles,
            revision: self.revision,
        };
        self.redo.clear();
        self.history_bytes += edit.bytes();
        self.undo.push_back(edit);
        while self.undo.len() > 1 && (self.history_bytes > HISTORY_BYTES || self.undo.len() > 64) {
            self.history_bytes -= self.undo.pop_front().unwrap().bytes();
        }
        self.serial += 1;
        self.revision = self.serial;
    }
    fn exchange(&mut self, edit: &mut Edit) {
        for (&(tx, ty), tile) in &mut edit.tiles {
            for (x, y, pixel) in tile.enumerate_pixels_mut() {
                std::mem::swap(
                    pixel,
                    self.image.get_pixel_mut(tx * TILE + x, ty * TILE + y),
                );
            }
            self.mark_dirty((tx, ty), [0, 0, tile.width(), tile.height()]);
        }
        std::mem::swap(&mut self.revision, &mut edit.revision);
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn undo(&mut self) {
        self.finish();
        if let Some(mut edit) = self.undo.pop_back() {
            self.history_bytes -= edit.bytes();
            self.exchange(&mut edit);
            self.redo.push(edit);
        }
    }
    pub fn redo(&mut self) {
        self.finish();
        if let Some(mut edit) = self.redo.pop() {
            self.exchange(&mut edit);
            self.history_bytes += edit.bytes();
            self.undo.push_back(edit);
        }
    }
}

#[derive(Clone, Copy)]
pub struct Dynamics {
    pub smoothing: f32,
    pub pressure_size: bool,
    pub pressure_flow: bool,
    pub minimum_size: f32,
    pub pressure_curve: f32,
}
impl Default for Dynamics {
    fn default() -> Self {
        Self {
            smoothing: 0.0,
            pressure_size: true,
            pressure_flow: false,
            minimum_size: 0.10,
            pressure_curve: 1.0,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Sample {
    pub point: [f32; 2],
    pub force: f32,
}
pub struct StrokePath {
    brush: Brush,
    dynamics: Dynamics,
    last: Option<Sample>,
    raw: Option<Sample>,
    until_dab: f32,
}
impl StrokePath {
    pub fn new(brush: Brush, dynamics: Dynamics) -> Self {
        Self {
            brush,
            dynamics,
            last: None,
            raw: None,
            until_dab: 0.0,
        }
    }
    fn brush_at(&self, force: f32) -> Brush {
        let force = force.clamp(0.0, 1.0).powf(self.dynamics.pressure_curve);
        let mut brush = self.brush;
        if self.dynamics.pressure_size {
            brush.size *= self.dynamics.minimum_size + (1.0 - self.dynamics.minimum_size) * force;
        }
        if self.dynamics.pressure_flow {
            brush.flow *= 0.05 + 0.95 * force;
        }
        brush
    }
    pub fn add(&mut self, painting: &mut Painting, mut sample: Sample) {
        if !sample.point.iter().all(|v| v.is_finite()) || !sample.force.is_finite() {
            return;
        }
        self.raw = Some(sample);
        let Some(last) = self.last else {
            let brush = self.brush_at(sample.force);
            painting.dab(sample.point, brush);
            self.until_dab = (brush.size * brush.spacing_percent * 0.01).max(0.25);
            self.last = Some(sample);
            return;
        };
        let dx = sample.point[0] - last.point[0];
        let dy = sample.point[1] - last.point[1];
        let distance = dx.hypot(dy);
        if self.dynamics.smoothing > 0.0 {
            // Distance-based filtering is independent of display refresh and event batching.
            let factor = 1.0 - (-distance / (self.dynamics.smoothing * 64.0).max(0.01)).exp();
            sample.point = [last.point[0] + dx * factor, last.point[1] + dy * factor];
        }
        let delta = [
            sample.point[0] - last.point[0],
            sample.point[1] - last.point[1],
        ];
        let length = delta[0].hypot(delta[1]);
        if length < 0.0001 {
            self.last = Some(sample);
            return;
        }
        let direction = [delta[0] / length, delta[1] / length];
        let mut entry = 0.0_f32;
        let mut exit = length;
        let size = self.brush.size;
        for (axis, extent) in [
            painting.image.width() as f32,
            painting.image.height() as f32,
        ]
        .into_iter()
        .enumerate()
        {
            let origin = last.point[axis];
            let velocity = direction[axis];
            if velocity == 0.0 {
                if origin < -size || origin > extent + size {
                    exit = -1.0;
                }
            } else {
                let a = (-size - origin) / velocity;
                let b = (extent + size - origin) / velocity;
                entry = entry.max(a.min(b));
                exit = exit.min(a.max(b));
            }
        }
        let spacing =
            (self.brush_at(last.force).size * self.brush.spacing_percent * 0.01).max(0.25);
        let mut travel =
            self.until_dab + ((entry - self.until_dab) / spacing).ceil().max(0.0) * spacing;
        while travel <= exit {
            let force = last.force + (sample.force - last.force) * (travel / length);
            let brush = self.brush_at(force);
            painting.dab(
                [
                    last.point[0] + direction[0] * travel,
                    last.point[1] + direction[1] * travel,
                ],
                brush,
            );
            travel += (brush.size * brush.spacing_percent * 0.01).max(0.25);
        }
        self.until_dab = if travel > length {
            travel - length
        } else {
            spacing - (length - travel).rem_euclid(spacing)
        };
        self.last = Some(sample);
    }
    pub fn finish(mut self, painting: &mut Painting) {
        if let Some(raw) = self.raw {
            self.dynamics.smoothing = 0.0;
            self.add(painting, raw);
        }
        painting.finish();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn brush(tip: Tip) -> Brush {
        Brush {
            tip,
            size: 14.0,
            color: [235, 38, 95],
            flow: 0.65,
            opacity: 1.0,
            softness_percent: 20.0,
            spacing_percent: 12.0,
        }
    }

    #[test]
    fn translucent_ink_has_consistent_stroke_opacity_and_builds_across_strokes() {
        let mut painting = Painting::new(RgbaImage::new(64, 64));
        let brush = Brush {
            size: 20.0,
            opacity: 0.5,
            ..Brush::default()
        };
        for _ in 0..30 {
            painting.dab([32.0, 32.0], brush);
        }
        painting.finish();
        assert_eq!(painting.image[(32, 32)][3], 128);
        let first = painting.image.clone();
        painting.dab([32.0, 32.0], brush);
        painting.finish();
        assert_eq!(painting.image[(32, 32)][3], 192);
        painting.undo();
        assert_eq!(painting.image, first);
    }

    #[test]
    fn pressure_changes_width_and_keeps_full_resolution_alpha() {
        let mut paint = Painting::new(RgbaImage::new(256, 100));
        let mut stroke = StrokePath::new(
            Brush {
                size: 40.0,
                ..Brush::default()
            },
            Dynamics::default(),
        );
        stroke.add(
            &mut paint,
            Sample {
                point: [30.0, 50.0],
                force: 0.1,
            },
        );
        stroke.add(
            &mut paint,
            Sample {
                point: [80.0, 50.0],
                force: 0.1,
            },
        );
        stroke.add(
            &mut paint,
            Sample {
                point: [180.0, 50.0],
                force: 1.0,
            },
        );
        stroke.add(
            &mut paint,
            Sample {
                point: [220.0, 50.0],
                force: 1.0,
            },
        );
        stroke.finish(&mut paint);
        let extent = |x| (0..100).filter(|y| paint.image[(x, *y)][3] > 20).count();
        assert!(extent(55) < extent(200) / 2);
        assert!(paint.image.pixels().any(|p| p[3] > 0 && p[3] < 255));
        let updates: Vec<_> = paint.take_updates().collect();
        assert!(updates.iter().all(|(_, b)| b[1] > 0 && b[3] < 100));
        paint.undo();
        assert!(paint.image.pixels().all(|p| p[3] == 0));
    }

    #[test]
    fn stroke_across_partial_tiles_undoes_exact_pixels_and_can_branch() {
        let source = RgbaImage::from_pixel(133, 140, Rgba([35, 140, 203, 127]));
        let mut painting = Painting::new(source.clone());
        painting.dab([127.5, 127.5], brush(Tip::Ink));
        painting.dab([132.5, 139.5], brush(Tip::Ink));
        painting.finish();
        assert_eq!(painting.undo.len(), 1);
        assert_eq!(painting.undo[0].tiles.len(), 4);
        let ink = painting.image.clone();
        let revision = painting.revision;
        assert_ne!(source, ink);
        painting.undo();
        assert_eq!(source, painting.image);
        assert_eq!(0, painting.revision);
        painting.redo();
        assert_eq!(ink, painting.image);
        assert_eq!(revision, painting.revision);
        painting.undo();
        painting.dab([40.0, 40.0], brush(Tip::Eraser));
        painting.finish();
        assert!(!painting.can_redo());
        assert_ne!(revision, painting.revision);
    }

    #[test]
    fn tips_have_distinct_footprints_and_transparent_erasures_do_not_dirty_a_blank_canvas() {
        let mut prints = HashSet::new();
        for tip in Tip::ALL {
            let mut paint = Painting::new(RgbaImage::new(24, 24));
            paint.dab([12.5, 12.5], brush(tip));
            paint.finish();
            assert!(prints.insert(paint.image.into_raw()));
            assert_eq!(paint.revision == 0, tip == Tip::Eraser);
        }
    }
}
