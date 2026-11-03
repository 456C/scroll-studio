//! A full-resolution RGBA canvas. Stroke history stores only touched tiles.
use image::{Rgba, RgbaImage};
use std::collections::{HashMap, HashSet, VecDeque};

pub const TILE: u32 = 128;
pub type Tile = (u32, u32);
const HISTORY_BYTES: usize = 96 * 1024 * 1024;

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
    pub softness_percent: f32,
    pub spacing_percent: f32,
    pub color: [u8; 3],
}

struct Edit {
    tiles: HashMap<Tile, RgbaImage>,
    revision: u64,
}

impl Edit {
    fn bytes(&self) -> usize {
        self.tiles.values().map(|tile| tile.as_raw().len()).sum()
    }
}

pub struct Painting {
    pub image: RgbaImage,
    pub invalidated: HashSet<Tile>,
    edit: Option<Edit>,
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
            edit: None,
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

    pub fn dab(&mut self, center: [f32; 2], brush: Brush) {
        let radius = brush.size.clamp(1.0, 256.0) * 0.5;
        let feather = (radius * brush.softness_percent * 0.01).max(1.0);
        let [cx, cy] = center;
        let left = (cx - radius - 1.0).floor().max(0.0) as u32;
        let top = (cy - radius - 1.0).floor().max(0.0) as u32;
        let right = ((cx + radius + 1.0).ceil() as u32).min(self.image.width());
        let bottom = ((cy + radius + 1.0).ceil() as u32).min(self.image.height());
        for y in top..bottom {
            let mut last_changed_tile = None;
            for x in left..right {
                let dx = x as f32 + 0.5 - cx;
                let dy = y as f32 + 0.5 - cy;
                let distance = if brush.tip == Tip::Marker {
                    ((dx * 0.87 + dy * 0.5).powi(2) + ((dy * 0.87 - dx * 0.5) * 2.6).powi(2)).sqrt()
                } else {
                    dx.hypot(dy)
                };
                let edge = ((radius + 0.5 - distance) / feather).clamp(0.0, 1.0);
                let mask = match brush.tip {
                    Tip::Airbrush => edge * edge * (1.0 - distance / radius).max(0.0),
                    Tip::Pencil => {
                        let grain = x
                            .wrapping_mul(374761393)
                            .wrapping_add(y.wrapping_mul(668265263));
                        edge * (0.22
                            + 0.78
                                * ((grain ^ (grain >> 13)).wrapping_mul(1274126177) >> 24) as f32
                                / 255.0)
                    }
                    _ => edge,
                };
                let alpha = mask * brush.flow.clamp(0.0, 1.0);
                if alpha <= 0.0 {
                    continue;
                }
                let old = self.image[(x, y)].0;
                let mut next = old;
                let dst = old[3] as f32 / 255.0;
                if brush.tip == Tip::Eraser {
                    next[3] = (255.0 * dst * (1.0 - alpha)).round() as u8;
                    if next[3] == 0 {
                        next = [0; 4];
                    }
                } else {
                    let combined = alpha + dst * (1.0 - alpha);
                    for c in 0..3 {
                        next[c] = ((brush.color[c] as f32 * alpha
                            + old[c] as f32 * dst * (1.0 - alpha))
                            / combined)
                            .round() as u8;
                    }
                    next[3] = (combined * 255.0).round() as u8;
                }
                if next == old {
                    continue;
                }
                let key = (x / TILE, y / TILE);
                if last_changed_tile != Some(key) {
                    if !self
                        .edit
                        .as_ref()
                        .is_some_and(|e| e.tiles.contains_key(&key))
                    {
                        let snapshot = self.tile(key);
                        self.edit
                            .get_or_insert_with(|| Edit {
                                tiles: HashMap::new(),
                                revision: self.revision,
                            })
                            .tiles
                            .insert(key, snapshot);
                    }
                    self.invalidated.insert(key);
                    last_changed_tile = Some(key);
                }
                self.image[(x, y)] = Rgba(next);
            }
        }
    }

    pub fn finish(&mut self) {
        let Some(edit) = self.edit.take() else { return };
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
            self.invalidated.insert((tx, ty));
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

#[cfg(test)]
mod tests {
    use super::*;

    fn brush(tip: Tip) -> Brush {
        Brush {
            tip,
            size: 14.0,
            color: [235, 38, 95],
            flow: 0.65,
            softness_percent: 20.0,
            spacing_percent: 12.0,
        }
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
