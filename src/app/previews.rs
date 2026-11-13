//! Background preview preparation. Only texture upload happens on the UI thread.
//! Work/results are bounded, and stale document generations never enter the cache.

use super::artwork_color;
mod quality;
pub(super) mod viewer;
use eframe::egui::{self, TextureHandle};
use scroll_studio::{
    compositor,
    model::{BubbleShape, BubbleTail, FontChoice, Layer, LayerKind, TextAlign, TextStyle},
};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    hash::Hash,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Arc,
    },
};
pub(super) use viewer::ImageViewport;

#[derive(Clone, Debug)]
struct ClipKey(Arc<scroll_studio::clipping::ImageClip>);
impl PartialEq for ClipKey {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for ClipKey {}
impl std::hash::Hash for ClipKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        (Arc::as_ptr(&self.0) as usize).hash(state);
    }
}

const GPU_BUDGET: usize = 64 * 1024 * 1024;
const MAX_ENTRIES: usize = 128;
const REQUEST_CAPACITY: usize = 4;
const RESULT_CAPACITY: usize = 2;
const MAX_PENDING: usize = REQUEST_CAPACITY + RESULT_CAPACITY + 1;

/// Include every mip level in the existing GPU budget. Narrow textures can
/// approach twice the base allocation, so a blanket 4/3 estimate is unsafe.
fn mipmapped_bytes(mut width: u32, mut height: u32) -> usize {
    let mut bytes = 0;
    loop {
        bytes += width as usize * height as usize * 4;
        if width <= 1 && height <= 1 {
            return bytes;
        }
        width = (width / 2).max(1);
        height = (height / 2).max(1);
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Key {
    generation: u64,
    source: SourceKey,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum SourceKey {
    ClippedImage {
        asset_id: u64,
        identity: usize,
        clip: ClipKey,
        region: Option<[u32; 4]>,
        side: u32,
    },
    Image {
        asset_id: u64,
        identity: usize,
        side: u32,
        mask: Vec<[u32; 2]>,
    },
    ImageRegion {
        asset_id: u64,
        identity: usize,
        region: [u32; 4],
        side: u32,
    },
    Text {
        appearance: TextKey,
        scale: u32,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct TextKey {
    text: String,
    color: [u8; 4],
    size: u32,
    bubble: bool,
    style: StyleKey,
    width: u32,
    height: u32,
    opacity: u32,
}

/// Physical pixels are kept intact at the requested scale. The ceil padding
/// belongs to the preview, not to the saved layer's geometry or hit target.
pub(super) struct TextTexture {
    texture: TextureHandle,
    scale: f32,
    bounds_width: f32,
}

impl TextTexture {
    fn paint_rect(&self, rect: egui::Rect, density: f32) -> egui::Rect {
        let zoom = rect.width() / self.bounds_width;
        let factor = zoom / self.scale;
        let factor = if (factor * density - 1.0).abs() < 0.0001 {
            1.0 / density
        } else {
            factor
        };
        let min = egui::pos2(
            (rect.min.x * density).round() / density,
            (rect.min.y * density).round() / density,
        );
        egui::Rect::from_min_size(min, self.texture.size_vec2() * factor)
    }

    pub(super) fn paint(&self, painter: &egui::Painter, rect: egui::Rect) {
        let rect = self.paint_rect(rect, painter.ctx().pixels_per_point());
        painter.image(
            self.texture.id(),
            rect,
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
    }
}

// All appearance fields participate in cache identity. Geometry uses float bits
// so editing a tail or changing a font cannot reuse an old preview.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct StyleKey {
    font: FontChoice,
    uppercase: bool,
    modern_layout: bool,
    shape: BubbleShape,
    fill: [u8; 4],
    outline: [u8; 4],
    outline_width: u32,
    padding: u32,
    line_spacing: u32,
    align: TextAlign,
    tail_enabled: bool,
    tail_tip: [u32; 2],
    tail_width: u32,
    tail_bend: u32,
    extra_tails: Vec<TailKey>,
    joins: Vec<([u32; 4], BubbleShape, bool)>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct TailKey {
    enabled: bool,
    tip: [u32; 2],
    width: u32,
    bend: u32,
}

impl From<&BubbleTail> for TailKey {
    fn from(tail: &BubbleTail) -> Self {
        Self {
            enabled: tail.enabled,
            tip: tail.tip.map(f32::to_bits),
            width: tail.width.to_bits(),
            bend: tail.bend.to_bits(),
        }
    }
}

impl From<&TextStyle> for StyleKey {
    fn from(style: &TextStyle) -> Self {
        Self {
            font: style.font,
            uppercase: style.uppercase,
            modern_layout: style.modern_layout,
            shape: style.shape,
            fill: style.fill,
            outline: style.outline,
            outline_width: style.outline_width.to_bits(),
            padding: style.padding.to_bits(),
            line_spacing: style.line_spacing.to_bits(),
            align: style.align,
            tail_enabled: style.tail.enabled,
            tail_tip: style.tail.tip.map(f32::to_bits),
            tail_width: style.tail.width.to_bits(),
            tail_bend: style.tail.bend.to_bits(),
            extra_tails: style.extra_tails.iter().map(TailKey::from).collect(),
            joins: style
                .joins
                .iter()
                .map(|j| {
                    (
                        [
                            j.x.to_bits(),
                            j.y.to_bits(),
                            j.width.to_bits(),
                            j.height.to_bits(),
                        ],
                        j.shape,
                        j.painted_before,
                    )
                })
                .collect(),
        }
    }
}

impl Key {
    fn asset_id(&self) -> Option<u64> {
        match self.source {
            SourceKey::Image { asset_id, .. }
            | SourceKey::ImageRegion { asset_id, .. }
            | SourceKey::ClippedImage { asset_id, .. } => Some(asset_id),
            _ => None,
        }
    }
}

enum Work {
    ClippedImage {
        source: Arc<Vec<u8>>,
        clip: Arc<scroll_studio::clipping::ImageClip>,
        region: Option<[u32; 4]>,
        side: u32,
    },
    Image {
        source: Arc<Vec<u8>>,
        mask: Vec<[f32; 2]>,
        side: u32,
    },
    ImageRegion {
        source: Arc<Vec<u8>>,
        region: [u32; 4],
        side: u32,
    },
    Text {
        layer: Box<Layer>,
        scale: f32,
    },
}

struct Request {
    key: Key,
    work: Work,
    ctx: egui::Context,
}
struct Completion {
    key: Key,
    source: Option<Arc<Vec<u8>>>,
    result: Result<image::RgbaImage, String>,
}

struct Entry {
    texture: Option<TextureHandle>,
    // Keep the allocation alive while its address forms part of an image key.
    // This makes an abandoned asset ID safe even if the allocator reuses memory.
    _source: Option<Arc<Vec<u8>>>,
}

type DimensionCache = HashMap<(u64, usize), (Arc<Vec<u8>>, [u32; 2])>;
pub(super) struct PreviewCache {
    quality: quality::PreviewQuality,
    dimensions: DimensionCache,
    cache: Lru<Key, Entry>,
    pending: HashSet<Key>,
    generation: Arc<AtomicU64>,
    requests: mpsc::SyncSender<Request>,
    results: mpsc::Receiver<Completion>,
    errors: VecDeque<String>,
}

impl PreviewCache {
    pub(super) fn new() -> Self {
        let (requests, work) = mpsc::sync_channel::<Request>(REQUEST_CAPACITY);
        let (done, results) = mpsc::sync_channel(RESULT_CAPACITY);
        let generation = Arc::new(AtomicU64::new(0));
        let worker_generation = Arc::clone(&generation);
        std::thread::spawn(move || {
            while let Ok(request) = work.recv() {
                if request.key.generation != worker_generation.load(Ordering::Acquire) {
                    request.ctx.request_repaint();
                    continue;
                }
                let (source, result) = prepare(request.work);
                if request.key.generation != worker_generation.load(Ordering::Acquire) {
                    request.ctx.request_repaint();
                    continue;
                }
                if done
                    .send(Completion {
                        key: request.key,
                        source,
                        result,
                    })
                    .is_err()
                {
                    break;
                }
                request.ctx.request_repaint();
            }
        });
        Self {
            dimensions: HashMap::new(),
            quality: quality::PreviewQuality::default(),
            cache: Lru::new(GPU_BUDGET, MAX_ENTRIES),
            pending: HashSet::new(),
            generation,
            requests,
            results,
            errors: VecDeque::new(),
        }
    }

    pub(super) fn clear(&mut self) {
        self.dimensions.clear();
        self.generation.fetch_add(1, Ordering::AcqRel);
        self.pending.clear();
        self.cache.clear();
        self.errors.clear();
        while self.results.try_recv().is_ok() {}
    }

    pub(super) fn retain_assets(&mut self, retained: &HashSet<u64>) {
        self.dimensions.retain(|(id, _), _| retained.contains(id));
        self.cache
            .retain(|key| key.asset_id().is_none_or(|id| retained.contains(&id)));
        self.pending
            .retain(|key| key.asset_id().is_none_or(|id| retained.contains(&id)));
    }

    pub(super) fn begin_frame(&mut self, ctx: &egui::Context, cpu_seconds: Option<f32>) {
        let (active, now) = ctx.input(|i| {
            (
                i.pointer.any_down() || i.raw_scroll_delta != egui::Vec2::ZERO,
                i.time,
            )
        });
        if self
            .quality
            .observe(cpu_seconds.unwrap_or(0.0), active, now)
        {
            ctx.request_repaint_after(std::time::Duration::from_millis(800));
        }
    }
    pub(super) fn poll(&mut self, ctx: &egui::Context) {
        // Called once at frame start. Limit texture uploads per frame as well as
        // completed CPU images; each worker completion wakes the UI when needed.
        for _ in 0..RESULT_CAPACITY {
            let Ok(completion) = self.results.try_recv() else {
                break;
            };
            self.accept(ctx, completion);
        }
    }

    fn accept(&mut self, ctx: &egui::Context, completion: Completion) {
        if completion.key.generation != self.generation.load(Ordering::Acquire)
            || !self.pending.remove(&completion.key)
        {
            return;
        }
        let (texture, bytes) = match completion.result {
            Ok(image) => {
                let bytes = mipmapped_bytes(image.width(), image.height());
                let color = artwork_color::upload(&image);
                // The Glow backend generates these once at upload. Filtering
                // across levels preserves fine outlines at phone/overview zoom
                // without CPU resampling or new work on every zoom change.
                let texture = ctx.load_texture(
                    "scroll-preview",
                    color,
                    egui::TextureOptions::LINEAR
                        .with_mipmap_mode(Some(egui::TextureFilter::Linear)),
                );
                (Some(texture), bytes)
            }
            Err(message) => {
                if self.errors.len() < 8 {
                    self.errors
                        .push_back(format!("Could not prepare an image preview: {message}"));
                }
                (None, 0)
            }
        };
        self.cache.insert(
            completion.key,
            Entry {
                texture,
                _source: completion.source,
            },
            bytes,
        );
    }

    pub(super) fn take_error(&mut self) -> Option<String> {
        self.errors.pop_front()
    }

    pub(super) fn image(
        &mut self,
        ctx: &egui::Context,
        asset_id: u64,
        source: &Arc<Vec<u8>>,
        mask: &[[f32; 2]],
        side: u32,
    ) -> Option<TextureHandle> {
        let side = self.quality.limit(side.clamp(1, 2048));
        let key = Key {
            generation: self.generation.load(Ordering::Acquire),
            source: SourceKey::Image {
                asset_id,
                identity: Arc::as_ptr(source) as usize,
                side,
                mask: mask
                    .iter()
                    .map(|p| [p[0].to_bits(), p[1].to_bits()])
                    .collect(),
            },
        };
        if let Some(entry) = self.cache.get(&key) {
            return entry.texture.clone();
        }
        if self.pending.contains(&key) || self.pending.len() >= MAX_PENDING {
            return None;
        }
        self.submit(
            ctx,
            key,
            Work::Image {
                source: Arc::clone(source),
                mask: mask.to_vec(),
                side,
            },
        );
        None
    }

    pub(super) fn text(
        &mut self,
        ctx: &egui::Context,
        layer: &Layer,
        pixels_per_unit: f32,
    ) -> Option<TextTexture> {
        let LayerKind::Text {
            text,
            color,
            size,
            bubble,
            style,
        } = &layer.kind
        else {
            return None;
        };
        let [_, _, width, height] = compositor::text_bounds(layer);
        if !pixels_per_unit.is_finite()
            || pixels_per_unit <= 0.0
            || !width.is_finite()
            || !height.is_finite()
            || width <= 0.0
            || height <= 0.0
        {
            return None;
        }
        let scale = pixels_per_unit.min(2048.0 / width.max(height));
        let appearance = TextKey {
            text: text.clone(),
            color: *color,
            size: size.to_bits(),
            bubble: *bubble,
            style: StyleKey::from(style),
            width: layer.width.to_bits(),
            height: layer.height.to_bits(),
            opacity: layer.opacity.to_bits(),
        };
        let key = Key {
            generation: self.generation.load(Ordering::Acquire),
            source: SourceKey::Text {
                appearance,
                scale: scale.to_bits(),
            },
        };
        if let Some(entry) = self.cache.get(&key) {
            return entry.texture.clone().map(|texture| TextTexture {
                texture,
                scale,
                bounds_width: width,
            });
        }
        // Keep text visible during a resize/zoom. Fallbacks must match every
        // appearance field, so edited words or pointers never show stale content.
        let fallback = self
            .cache
            .values
            .iter()
            .filter_map(|(candidate, entry)| {
                if candidate.generation != key.generation || entry.value.texture.is_none() {
                    return None;
                }
                if let (
                    SourceKey::Text {
                        appearance: a,
                        scale: old,
                    },
                    SourceKey::Text { appearance: b, .. },
                ) = (&candidate.source, &key.source)
                {
                    if a == b {
                        return Some((
                            candidate.clone(),
                            (f32::from_bits(*old) / scale).ln().abs(),
                        ));
                    }
                }
                None
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(key, _)| key);
        if !self.pending.contains(&key) && self.pending.len() < MAX_PENDING {
            let mut preview_layer = layer.clone();
            preview_layer.x = 0.0;
            preview_layer.y = 0.0;
            self.submit(
                ctx,
                key,
                Work::Text {
                    layer: Box::new(preview_layer),
                    scale,
                },
            );
        }
        fallback.and_then(|key| {
            let SourceKey::Text { scale, .. } = key.source else {
                return None;
            };
            self.cache
                .get(&key)?
                .texture
                .clone()
                .map(|texture| TextTexture {
                    texture,
                    scale: f32::from_bits(scale),
                    bounds_width: width,
                })
        })
    }

    fn submit(&mut self, ctx: &egui::Context, key: Key, work: Work) {
        let request = Request {
            key: key.clone(),
            work,
            ctx: ctx.clone(),
        };
        match self.requests.try_send(request) {
            Ok(()) => {
                self.pending.insert(key);
            }
            Err(mpsc::TrySendError::Full(_)) => (), // Existing work will wake the UI to retry.
            Err(mpsc::TrySendError::Disconnected(request)) => {
                let source = match request.work {
                    Work::Image { source, .. }
                    | Work::ImageRegion { source, .. }
                    | Work::ClippedImage { source, .. } => Some(source),
                    _ => None,
                };
                self.cache.insert(
                    key,
                    Entry {
                        texture: None,
                        _source: source,
                    },
                    0,
                );
                if self.errors.len() < 8 {
                    self.errors
                        .push_back("The image preview worker stopped unexpectedly.".into());
                }
            }
        }
    }
}

// Every successful result contains gamma-premultiplied pixels ready for upload.
fn prepare(work: Work) -> (Option<Arc<Vec<u8>>>, Result<image::RgbaImage, String>) {
    match work {
        Work::ClippedImage {
            source,
            clip,
            region,
            side,
        } => {
            let result = (|| -> anyhow::Result<image::RgbaImage> {
                let mut image = compositor::decode_image(&source)?;
                scroll_studio::clipping::apply(&mut image, &clip)?;
                if let Some([x, y, w, h]) = region {
                    anyhow::ensure!(
                        w > 0
                            && h > 0
                            && x.checked_add(w).is_some_and(|end| end <= image.width())
                            && y.checked_add(h).is_some_and(|end| end <= image.height()),
                        "Invalid clipped detail region"
                    );
                    image = image::imageops::crop_imm(&image, x, y, w, h).to_image();
                }
                artwork_color::premultiply(&mut image);
                Ok(resize_preview(image, side))
            })()
            .map_err(|error| format!("{error:#}"));
            (Some(source), result)
        }
        Work::ImageRegion {
            source,
            region: [x, y, width, height],
            side,
        } => {
            let result = (|| -> anyhow::Result<image::RgbaImage> {
                let image = compositor::decode_image(&source)?;
                anyhow::ensure!(
                    width > 0
                        && height > 0
                        && x.checked_add(width).is_some_and(|end| end <= image.width())
                        && y.checked_add(height)
                            .is_some_and(|end| end <= image.height()),
                    "Image detail lies outside its source"
                );
                let mut detail = image::imageops::crop_imm(&image, x, y, width, height).to_image();
                drop(image);
                artwork_color::premultiply(&mut detail);
                Ok(resize_preview(detail, side))
            })()
            .map_err(|error| format!("{error:#}"));
            (Some(source), result)
        }
        Work::Image { source, mask, side } => {
            let result = (|| -> anyhow::Result<image::RgbaImage> {
                let mut image = compositor::decode_image(&source)?;
                artwork_color::premultiply(&mut image);
                let mut preview = resize_preview(image, side);
                if !mask.is_empty() {
                    compositor::mask_premultiplied_image(&mut preview, &mask);
                }
                Ok(preview)
            })()
            .map_err(|error| format!("{error:#}"));
            (Some(source), result)
        }
        Work::Text { layer, scale } => (
            None,
            compositor::text_preview_at_scale(&layer, scale)
                .map(|mut image| {
                    artwork_color::premultiply(&mut image);
                    image
                })
                .map_err(|error| format!("{error:#}")),
        ),
    }
}

fn resize_preview(image: image::RgbaImage, side: u32) -> image::RgbaImage {
    let side = side.clamp(1, 2048);
    if image.width().max(image.height()) <= side {
        return image;
    }
    let scale = side as f32 / image.width().max(image.height()) as f32;
    image::imageops::resize(
        &image,
        (image.width() as f32 * scale).max(1.0) as u32,
        (image.height() as f32 * scale).max(1.0) as u32,
        image::imageops::FilterType::Triangle,
    )
}

struct Stored<V> {
    value: V,
    bytes: usize,
    touched: u64,
}
struct Lru<K, V> {
    values: HashMap<K, Stored<V>>,
    bytes: usize,
    budget: usize,
    max_entries: usize,
    tick: u64,
}

impl<K: Clone + Eq + Hash, V> Lru<K, V> {
    fn new(budget: usize, max_entries: usize) -> Self {
        Self {
            values: HashMap::new(),
            bytes: 0,
            budget,
            max_entries,
            tick: 0,
        }
    }
    fn get(&mut self, key: &K) -> Option<&V> {
        let stored = self.values.get_mut(key)?;
        self.tick += 1;
        stored.touched = self.tick;
        Some(&stored.value)
    }
    fn insert(&mut self, key: K, value: V, bytes: usize) {
        if let Some(previous) = self.values.remove(&key) {
            self.bytes -= previous.bytes;
        }
        if bytes > self.budget || self.max_entries == 0 {
            return;
        }
        while self.bytes.saturating_add(bytes) > self.budget
            || self.values.len() >= self.max_entries
        {
            let Some(oldest) = self
                .values
                .iter()
                .min_by_key(|(_, entry)| entry.touched)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            if let Some(entry) = self.values.remove(&oldest) {
                self.bytes -= entry.bytes;
            }
        }
        self.tick += 1;
        self.bytes += bytes;
        self.values.insert(
            key,
            Stored {
                value,
                bytes,
                touched: self.tick,
            },
        );
    }
    fn retain(&mut self, mut keep: impl FnMut(&K) -> bool) {
        self.values.retain(|key, value| {
            if keep(key) {
                true
            } else {
                self.bytes -= value.bytes;
                false
            }
        });
    }
    fn clear(&mut self) {
        self.values.clear();
        self.bytes = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filtered_previews_count_all_mip_levels_against_the_cache_budget() {
        assert_eq!(mipmapped_bytes(1, 1), 4);
        assert_eq!(mipmapped_bytes(8, 1), (8 + 4 + 2 + 1) * 4);
        assert_eq!(mipmapped_bytes(5, 3), (15 + 2 + 1) * 4);
        let mut previews = PreviewCache::new();
        previews.cache = Lru::new(150, 10);
        let ctx = egui::Context::default();
        // Two 5x3 textures including their mip chains fit; a third evicts one.
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            for asset_id in 1..=3 {
                let key = image_key(0, asset_id);
                previews.pending.insert(key.clone());
                previews.accept(
                    ctx,
                    Completion {
                        key,
                        source: None,
                        result: Ok(image::RgbaImage::new(5, 3)),
                    },
                );
            }
        });
        assert_eq!(previews.cache.bytes, 144);
        assert_eq!(previews.cache.values.len(), 2);
        assert!(!previews.cache.values.contains_key(&image_key(0, 1)));
        let uploads: Vec<_> = output
            .textures_delta
            .set
            .iter()
            .filter(|(_, delta)| delta.image.size() == [5, 3])
            .collect();
        assert_eq!(uploads.len(), 3);
        assert!(uploads
            .iter()
            .all(|(_, delta)| delta.options.mipmap_mode == Some(egui::TextureFilter::Linear)));
    }

    #[test]
    fn every_lettering_setting_changes_preview_identity() {
        let original = TextStyle::dialogue();
        let key = StyleKey::from(&original);
        let changes: &[fn(&mut TextStyle)] = &[
            |s| s.font = FontChoice::PatrickHand,
            |s| s.uppercase = !s.uppercase,
            |s| s.modern_layout = !s.modern_layout,
            |s| s.shape = BubbleShape::Thought,
            |s| s.fill[3] = 100,
            |s| s.outline[0] = 128,
            |s| s.outline_width += 1.0,
            |s| s.padding += 1.0,
            |s| s.line_spacing += 0.1,
            |s| s.align = TextAlign::Right,
            |s| s.tail.enabled = !s.tail.enabled,
            |s| s.tail.tip[0] += 0.1,
            |s| s.tail.tip[1] += 0.1,
            |s| s.tail.width += 1.0,
            |s| s.tail.bend += 0.1,
            |s| s.extra_tails.push(s.tail.clone()),
        ];
        for change in changes {
            let mut edited = original.clone();
            change(&mut edited);
            assert_ne!(key, StyleKey::from(&edited));
        }
    }

    #[test]
    fn every_extra_pointer_setting_changes_preview_identity_but_preset_link_does_not() {
        let mut original = TextStyle::dialogue();
        original.extra_tails.push(original.tail.clone());
        let key = StyleKey::from(&original);
        let changes: &[fn(&mut BubbleTail)] = &[
            |tail| tail.enabled = false,
            |tail| tail.tip[0] = -1.5,
            |tail| tail.tip[1] = -0.5,
            |tail| tail.width += 10.0,
            |tail| tail.bend = -0.5,
        ];
        for change in changes {
            let mut edited = original.clone();
            change(&mut edited.extra_tails[0]);
            assert_ne!(key, StyleKey::from(&edited));
        }
        original.preset_id = Some(123);
        assert_eq!(key, StyleKey::from(&original));
    }

    #[test]
    fn lru_evicts_least_recently_used_and_keeps_exact_byte_accounting() {
        let mut cache = Lru::new(10, 3);
        cache.insert(1, "first", 4);
        cache.insert(2, "second", 4);
        assert_eq!(cache.get(&1), Some(&"first"));
        cache.insert(3, "third", 4);
        assert!(cache.get(&2).is_none());
        assert_eq!(cache.bytes, 8);
        cache.insert(1, "replacement", 2);
        assert_eq!(cache.bytes, 6);
        cache.retain(|key| *key == 1);
        assert_eq!(cache.bytes, 2);
        cache.clear();
        assert_eq!(cache.bytes, 0);
    }

    #[test]
    fn zero_byte_failure_entries_and_oversized_previews_are_bounded() {
        let mut cache = Lru::new(10, 2);
        for key in 0..100 {
            cache.insert(key, (), 0);
        }
        assert_eq!(cache.values.len(), 2);
        cache.insert(500, (), 11);
        assert!(cache.get(&500).is_none());
        assert_eq!(cache.bytes, 0);
    }

    fn image_key(generation: u64, asset_id: u64) -> Key {
        Key {
            generation,
            source: SourceKey::Image {
                asset_id,
                identity: 1,
                side: 384,
                mask: vec![],
            },
        }
    }

    #[test]
    fn stale_document_and_abandoned_asset_results_never_enter_cache() {
        let mut previews = PreviewCache::new();
        let ctx = egui::Context::default();
        let old = image_key(0, 1);
        previews.pending.insert(old.clone());
        previews.clear();
        previews.accept(
            &ctx,
            Completion {
                key: old,
                source: None,
                result: Err("old failure".into()),
            },
        );
        assert!(previews.cache.values.is_empty());
        assert!(previews.take_error().is_none());
        let orphan = image_key(1, 2);
        previews.pending.insert(orphan.clone());
        previews.retain_assets(&HashSet::new());
        previews.accept(
            &ctx,
            Completion {
                key: orphan,
                source: None,
                result: Err("orphan failure".into()),
            },
        );
        assert!(previews.cache.values.is_empty());
    }

    #[test]
    fn repeated_requests_are_deduplicated_and_failures_are_cached() {
        let mut previews = PreviewCache::new();
        let ctx = egui::Context::default();
        let invalid = Arc::new(vec![1, 2, 3]);
        for _ in 0..100 {
            previews.image(&ctx, 1, &invalid, &[], 384);
        }
        assert!(previews.pending.len() <= 1);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while previews.cache.values.is_empty() && std::time::Instant::now() < deadline {
            previews.poll(&ctx);
            std::thread::yield_now();
        }
        assert_eq!(previews.cache.values.len(), 1);
        assert!(previews.pending.is_empty());
        for _ in 0..100 {
            previews.image(&ctx, 1, &invalid, &[], 384);
        }
        assert!(previews.pending.is_empty());
        assert_eq!(previews.errors.len(), 1);
    }

    #[test]
    fn a_busy_worker_cannot_build_an_unbounded_request_backlog() {
        // Keep the receiver alive without consuming it to model a busy decoder.
        let (requests, _work) = mpsc::sync_channel(REQUEST_CAPACITY);
        let (_done, results) = mpsc::sync_channel(RESULT_CAPACITY);
        let mut previews = PreviewCache {
            dimensions: HashMap::new(),
            quality: quality::PreviewQuality::default(),
            cache: Lru::new(GPU_BUDGET, MAX_ENTRIES),
            pending: HashSet::new(),
            generation: Arc::new(AtomicU64::new(0)),
            requests,
            results,
            errors: VecDeque::new(),
        };
        let ctx = egui::Context::default();
        let source = Arc::new(vec![1, 2, 3]);
        for id in 1..100 {
            previews.image(&ctx, id, &source, &[], 384);
        }
        assert_eq!(previews.pending.len(), REQUEST_CAPACITY);
        for _ in 0..100 {
            previews.image(&ctx, 1, &source, &[], 384);
        }
        assert_eq!(previews.pending.len(), REQUEST_CAPACITY);
        previews.clear();
        assert!(previews.pending.is_empty());
    }

    #[test]
    fn thumbnail_preparation_respects_requested_size_and_original_bytes() {
        let mut encoded = std::io::Cursor::new(Vec::new());
        image::RgbaImage::from_pixel(1000, 500, image::Rgba([40, 80, 120, 255]))
            .write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();
        let source = Arc::new(encoded.into_inner());
        let original = source.clone();
        let (_, prepared) = prepare(Work::Image {
            source,
            mask: vec![],
            side: 384,
        });
        let image = prepared.unwrap();
        assert_eq!((image.width(), image.height()), (384, 192));
        assert_eq!(compositor::decode_image(&original).unwrap().width(), 1000);
    }
    fn text_layer() -> Layer {
        scroll_studio::model::Story::demo().chapters[0]
            .layers
            .iter()
            .find(|layer| matches!(layer.kind, LayerKind::Text { .. }))
            .unwrap()
            .clone()
    }

    fn controlled_cache() -> (PreviewCache, mpsc::Receiver<Request>) {
        let (requests, work) = mpsc::sync_channel(REQUEST_CAPACITY);
        let (_done, results) = mpsc::sync_channel(RESULT_CAPACITY);
        (
            PreviewCache {
                dimensions: HashMap::new(),
                quality: quality::PreviewQuality::default(),
                cache: Lru::new(GPU_BUDGET, MAX_ENTRIES),
                pending: HashSet::new(),
                generation: Arc::new(AtomicU64::new(0)),
                requests,
                results,
                errors: VecDeque::new(),
            },
            work,
        )
    }

    fn complete(previews: &mut PreviewCache, work: &mpsc::Receiver<Request>, ctx: &egui::Context) {
        let request = work.try_recv().unwrap();
        let (source, result) = prepare(request.work);
        assert!(result.is_ok());
        previews.accept(
            ctx,
            Completion {
                key: request.key,
                source,
                result,
            },
        );
    }

    #[test]
    fn text_zoom_reuses_a_matching_preview_until_the_new_scale_is_ready() {
        let ctx = egui::Context::default();
        let (mut previews, work) = controlled_cache();
        let mut layer = text_layer();
        assert!(previews.text(&ctx, &layer, 0.225).is_none());
        complete(&mut previews, &work, &ctx);
        let first = previews.text(&ctx, &layer, 0.225).unwrap();
        assert_eq!(first.scale, 0.225);
        let bytes = previews.cache.bytes;
        let full = compositor::text_preview(&layer).unwrap();
        assert!(bytes < mipmapped_bytes(full.width(), full.height()) / 10);
        for _ in 0..100 {
            assert_eq!(
                previews.text(&ctx, &layer, 0.225).unwrap().texture.id(),
                first.texture.id()
            );
        }
        assert!(previews.pending.is_empty());
        let fallback = previews.text(&ctx, &layer, 0.45).unwrap();
        assert_eq!(fallback.texture.id(), first.texture.id());
        assert_eq!(fallback.scale, 0.225);
        complete(&mut previews, &work, &ctx);
        let larger = previews.text(&ctx, &layer, 0.45).unwrap();
        assert_ne!(larger.texture.id(), first.texture.id());
        assert_eq!(larger.scale, 0.45);
        layer.x += 127.5;
        layer.y -= 32.25;
        assert_eq!(
            previews.text(&ctx, &layer, 0.45).unwrap().texture.id(),
            larger.texture.id()
        );
        if let LayerKind::Text { text, .. } = &mut layer.kind {
            text.push_str(" Different words.");
        }
        assert!(
            previews.text(&ctx, &layer, 0.45).is_none(),
            "Changed words must not borrow a stale preview"
        );
        previews.clear();
        complete(&mut previews, &work, &ctx);
        assert!(
            previews.cache.values.is_empty(),
            "Old document work must be discarded"
        );
    }

    #[test]
    fn display_size_text_keeps_physical_pixels_at_multiple_densities() {
        let ctx = egui::Context::default();
        let (mut previews, work) = controlled_cache();
        let layer = text_layer();
        for density in [1.0, 1.5, 2.0] {
            let requested = 0.225 * density;
            let _ = previews.text(&ctx, &layer, requested);
            complete(&mut previews, &work, &ctx);
            let texture = previews.text(&ctx, &layer, requested).unwrap();
            let [_, _, w, h] = compositor::text_bounds(&layer);
            let rect =
                egui::Rect::from_min_size(egui::pos2(80.37, 110.62), egui::vec2(w, h) * 0.225);
            let painted = texture.paint_rect(rect, density);
            assert!((painted.size() * density - texture.texture.size_vec2()).length() < 0.001);
            assert!((painted.left() * density - (rect.left() * density).round()).abs() < 0.001);
            assert!((painted.top() * density - (rect.top() * density).round()).abs() < 0.001);
            assert!(
                painted.width() >= rect.width() - 0.001
                    && painted.width() <= rect.width() + 1.0 / density
            );
        }
    }

    #[test]
    fn rapid_text_zoom_and_appearance_changes_keep_work_bounded() {
        let ctx = egui::Context::default();
        let (mut previews, work) = controlled_cache();
        let layer = text_layer();
        let _ = previews.text(&ctx, &layer, 0.225);
        complete(&mut previews, &work, &ctx);
        for i in 1..=100 {
            assert!(previews
                .text(&ctx, &layer, 0.25 + i as f32 * 0.01)
                .is_some());
        }
        assert_eq!(previews.pending.len(), REQUEST_CAPACITY);
        let mut changed = layer.clone();
        if let LayerKind::Text { style, .. } = &mut changed.kind {
            style.tail.tip[0] -= 1.0;
        }
        assert!(previews.text(&ctx, &changed, 0.5).is_none());
        for invalid in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            assert!(previews.text(&ctx, &layer, invalid).is_none());
        }
        assert_eq!(previews.pending.len(), REQUEST_CAPACITY);
    }
    #[test]
    fn preview_upload_matches_export_alpha_over_light_and_dark_surfaces() {
        let mut image = image::RgbaImage::new(256, 1);
        for alpha in 0..=255u8 {
            image.put_pixel(u32::from(alpha), 0, image::Rgba([70, 180, 240, alpha]));
        }
        let mut premultiplied = image.clone();
        artwork_color::premultiply(&mut premultiplied);
        let upload = artwork_color::upload(&premultiplied);
        for (source, encoded) in image.pixels().zip(upload.pixels) {
            let alpha = source[3] as f32 / 255.0;
            for background in [[255.0; 3], [25.0, 35.0, 45.0]] {
                for channel in 0..3 {
                    let exported =
                        source[channel] as f32 * alpha + background[channel] * (1.0 - alpha);
                    let displayed = encoded[channel] as f32 + background[channel] * (1.0 - alpha);
                    assert!((exported - displayed).abs() <= 0.51);
                }
            }
            assert_eq!(encoded.a(), source[3]);
        }
        assert_eq!(upload.size, [256, 1]);
    }

    fn encoded(image: &image::RgbaImage) -> Arc<Vec<u8>> {
        let mut bytes = std::io::Cursor::new(Vec::new());
        image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
        Arc::new(bytes.into_inner())
    }

    fn gpu_over(color: egui::Color32, tint: egui::Color32, background: [u8; 3]) -> [f32; 3] {
        let alpha = color.a() as f32 / 255.0 * tint.a() as f32 / 255.0;
        std::array::from_fn(|c| {
            color[c] as f32 * tint[c] as f32 / 255.0 + background[c] as f32 * (1.0 - alpha)
        })
    }

    #[test]
    fn prepared_images_and_vector_colors_agree_with_export_across_alpha_and_opacity() {
        use scroll_studio::model::{AssetBytes, Chapter};
        for rgb in [[70, 180, 240], [255, 255, 255], [240, 90, 20], [0, 0, 0]] {
            let image = image::RgbaImage::from_fn(256, 1, |x, _| {
                image::Rgba([rgb[0], rgb[1], rgb[2], x as u8])
            });
            let source = encoded(&image);
            let mut assets = AssetBytes::new();
            assets.insert(1, source.clone());
            let (_, prepared) = prepare(Work::Image {
                source,
                mask: vec![],
                side: 384,
            });
            let upload = artwork_color::upload(&prepared.unwrap());
            for background in [[255; 3], [25, 35, 45]] {
                for opacity in [0.0, 0.003, 0.1, 0.25, 0.5, 0.75, 1.0] {
                    let mut chapter = Chapter::new(2, "Alpha", 256, 2);
                    let base = Layer {
                        extensions: Default::default(),
                        id: 3,
                        name: "Test".into(),
                        visible: true,
                        locked: false,
                        opacity: 1.0,
                        x: 0.0,
                        y: 0.0,
                        width: 256.0,
                        height: 2.0,
                        kind: LayerKind::Shape {
                            color: [background[0], background[1], background[2], 255],
                        },
                    };
                    chapter.layers.push(base.clone());
                    chapter.layers.push(Layer {
                        id: 4,
                        height: 1.0,
                        opacity,
                        kind: LayerKind::Image {
                            source_rect: None,
                            clip: None,
                            asset_id: 1,
                            mask: vec![],
                        },
                        ..base.clone()
                    });
                    for alpha in 0..=255u8 {
                        chapter.layers.push(Layer {
                            id: u64::from(alpha) + 5,
                            x: alpha as f32,
                            y: 1.0,
                            width: 1.0,
                            height: 1.0,
                            opacity,
                            kind: LayerKind::Shape {
                                color: [rgb[0], rgb[1], rgb[2], alpha],
                            },
                            ..base.clone()
                        });
                    }
                    let proof = compositor::render_region(&chapter, &assets, 0, 2, 256).unwrap();
                    for alpha in 0..=255u8 {
                        let displayed = gpu_over(
                            upload.pixels[alpha as usize],
                            artwork_color::opacity_tint(opacity),
                            background,
                        );
                        let vector = gpu_over(
                            artwork_color::color([rgb[0], rgb[1], rgb[2], alpha], opacity),
                            egui::Color32::WHITE,
                            background,
                        );
                        for (y, pixel) in [displayed, vector].iter().enumerate() {
                            for (c, channel) in pixel.iter().enumerate() {
                                assert!((*channel - proof.get_pixel(alpha as u32,y as u32)[c] as f32).abs() <= 2.0,
                                    "RGB {rgb:?}, alpha {alpha}, opacity {opacity}, background {background:?}, row {y}, channel {c}");
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn thumbnail_resizing_ignores_hidden_rgb_and_keeps_assets_unchanged() {
        let fixture = |hidden| {
            image::RgbaImage::from_fn(1024, 64, |x, y| {
                if x < 256 + y * 8 {
                    image::Rgba([70, 180, 240, 255])
                } else {
                    image::Rgba(hidden)
                }
            })
        };
        let a = encoded(&fixture([255, 0, 0, 0]));
        let b = encoded(&fixture([0, 255, 0, 0]));
        let original = a.as_ref().clone();
        let (_, first) = prepare(Work::Image {
            source: a.clone(),
            mask: vec![],
            side: 384,
        });
        let (_, second) = prepare(Work::Image {
            source: b,
            mask: vec![],
            side: 384,
        });
        let first = first.unwrap();
        assert_eq!(
            first,
            second.unwrap(),
            "Invisible RGB must not tint the shrinking cutout"
        );
        assert_eq!((first.width(), first.height()), (384, 24));
        assert!(first.pixels().any(|p| p[3] > 0 && p[3] < 255));
        assert!(first.pixels().all(|p| p.0[..3].iter().all(|c| *c <= p[3])));
        assert_eq!(*a, original);
    }

    #[test]
    fn masked_preview_matches_export_without_bright_edges_or_source_changes() {
        use scroll_studio::model::{AssetBytes, Chapter};
        let image =
            image::RgbaImage::from_fn(64, 64, |x, _| image::Rgba([70, 180, 240, (x * 4) as u8]));
        let source = encoded(&image);
        let original = source.as_ref().clone();
        let mask = vec![[0.05, 0.15], [0.95, 0.25], [0.8, 0.9], [0.2, 0.75]];
        let mut assets = AssetBytes::new();
        assets.insert(1, source.clone());
        let (_, prepared) = prepare(Work::Image {
            source: source.clone(),
            mask: mask.clone(),
            side: 384,
        });
        let preview = prepared.unwrap();
        let upload = artwork_color::upload(&preview);
        for bg in [[255; 3], [25, 35, 45]] {
            let mut chapter = Chapter::new(2, "Mask", 64, 64);
            let base = Layer {
                extensions: Default::default(),
                id: 3,
                name: "Paper".into(),
                visible: true,
                locked: false,
                opacity: 1.0,
                x: 0.0,
                y: 0.0,
                width: 64.0,
                height: 64.0,
                kind: LayerKind::Shape {
                    color: [bg[0], bg[1], bg[2], 255],
                },
            };
            chapter.layers.push(base.clone());
            chapter.layers.push(Layer {
                id: 4,
                opacity: 0.6,
                kind: LayerKind::Image {
                    source_rect: None,
                    clip: None,
                    asset_id: 1,
                    mask: mask.clone(),
                },
                ..base
            });
            let proof = compositor::render_region(&chapter, &assets, 0, 64, 64).unwrap();
            for (encoded, expected) in upload.pixels.iter().zip(proof.pixels()) {
                let displayed = gpu_over(*encoded, artwork_color::opacity_tint(0.6), bg);
                for c in 0..3 {
                    assert!((displayed[c] - expected[c] as f32).abs() <= 2.0);
                }
            }
        }
        assert!(preview.pixels().any(|p| p[3] == 0 && p.0 == [0; 4]));
        assert!(preview.pixels().any(|p| p[3] > 0));
        assert_eq!(*source, original);
    }

    #[test]
    fn canvas_opacity_reuses_the_image_and_applies_export_compatible_tints() {
        use scroll_studio::model::AssetBytes;
        let ctx = egui::Context::default();
        let (mut previews, work) = controlled_cache();
        let source = encoded(&image::RgbaImage::from_pixel(
            64,
            64,
            image::Rgba([70, 180, 240, 128]),
        ));
        let mut assets = AssetBytes::new();
        assets.insert(1, source.clone());
        previews.image(&ctx, 1, &source, &[], 384);
        complete(&mut previews, &work, &ctx);
        let texture = previews.image(&ctx, 1, &source, &[], 384).unwrap();
        let mut layer = Layer {
            extensions: Default::default(),
            id: 2,
            name: "Opacity".into(),
            visible: true,
            locked: false,
            opacity: 0.5,
            x: 0.0,
            y: 0.0,
            width: 64.0,
            height: 64.0,
            kind: LayerKind::Image {
                source_rect: None,
                clip: None,
                asset_id: 1,
                mask: vec![],
            },
        };
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(64.0, 64.0));
        for opacity in [0.1, 0.5, 1.0] {
            layer.opacity = opacity;
            let output = ctx.run(egui::RawInput::default(), |ctx| {
                let painter = ctx.layer_painter(egui::LayerId::background());
                super::super::Studio::paint_layer(
                    &mut previews,
                    &assets,
                    ctx,
                    &painter,
                    &layer,
                    rect,
                    1.0,
                );
            });
            let mesh = output
                .shapes
                .iter()
                .find_map(|s| match &s.shape {
                    egui::Shape::Mesh(mesh) if mesh.texture_id == texture.id() => Some(mesh),
                    _ => None,
                })
                .expect("Image must use the already prepared texture");
            assert!(mesh
                .vertices
                .iter()
                .all(|v| v.color == artwork_color::opacity_tint(opacity)));
            assert!(
                previews.pending.is_empty(),
                "Opacity must not queue another decode"
            );
        }
        for kind in [
            LayerKind::Shape {
                color: [70, 180, 240, 128],
            },
            LayerKind::Stroke {
                points: vec![[0.0, 20.0], [60.0, 20.0]],
                color: [70, 180, 240, 128],
                size: 8.0,
            },
        ] {
            layer.kind = kind;
            layer.opacity = 0.5;
            let output = ctx.run(egui::RawInput::default(), |ctx| {
                let painter = ctx.layer_painter(egui::LayerId::background());
                super::super::Studio::paint_layer(
                    &mut previews,
                    &assets,
                    ctx,
                    &painter,
                    &layer,
                    rect,
                    1.0,
                );
            });
            let expected = artwork_color::color([70, 180, 240, 128], 0.5);
            assert!(output.shapes.iter().any(|s| match &s.shape {
                egui::Shape::Rect(rect) => rect.fill == expected,
                egui::Shape::Path(path) =>
                    path.stroke.color == egui::epaint::ColorMode::Solid(expected),
                _ => false,
            }));
        }
    }

    #[test]
    fn viewer_upgrades_thumbnails_and_isolates_replaced_sources_and_masks() {
        let ctx = egui::Context::default();
        let (mut previews, work) = controlled_cache();
        let source = encoded(&image::RgbaImage::from_pixel(
            1600,
            1000,
            image::Rgba([30, 90, 180, 255]),
        ));
        previews.image(&ctx, 1, &source, &[], 384);
        complete(&mut previews, &work, &ctx);
        let thumbnail = previews.image(&ctx, 1, &source, &[], 384).unwrap();
        let view = ImageViewport {
            source_size: [1600, 1000],
            image: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(750.0, 468.75)),
            clip: egui::Rect::EVERYTHING,
            mirrored: false,
        };
        // Use a finite viewport as the UI does.
        let view = ImageViewport {
            clip: view.image,
            ..view
        };
        ctx.set_pixels_per_point(2.0);
        let _ = ctx.run(egui::RawInput::default(), |_| {});
        let fallback = previews.viewer_image(&ctx, 1, &source, view).unwrap();
        assert_eq!(fallback.texture.id(), thumbnail.id());
        for _ in 0..100 {
            assert!(previews.viewer_image(&ctx, 1, &source, view).is_some());
        }
        assert_eq!(previews.pending.len(), 1);
        complete(&mut previews, &work, &ctx);
        let full = previews.viewer_image(&ctx, 1, &source, view).unwrap();
        assert_eq!(full.texture.size(), [1536, 960]);
        assert_ne!(full.texture.id(), thumbnail.id());
        let replacement = Arc::new(source.as_ref().clone());
        assert!(previews.viewer_image(&ctx, 1, &replacement, view).is_none());
        complete(&mut previews, &work, &ctx);
        previews.image(&ctx, 2, &source, &[[0.0, 0.0], [1.0, 0.0], [1.0, 0.5]], 384);
        complete(&mut previews, &work, &ctx);
        assert!(
            previews.viewer_image(&ctx, 2, &source, view).is_none(),
            "A lasso preview cannot serve an unmasked reference"
        );
    }

    #[test]
    fn zoomed_viewer_uses_source_detail_once_and_bounds_pan_work() {
        let ctx = egui::Context::default();
        let (mut previews, work) = controlled_cache();
        let original = image::RgbaImage::from_fn(4096, 512, |x, y| {
            image::Rgba([if (x + y) % 2 == 0 { 0 } else { 255 }, 80, 140, 128])
        });
        let source = encoded(&original);
        let view = ImageViewport {
            source_size: [4096, 512],
            image: egui::Rect::from_min_size(egui::pos2(-1400.0, 0.0), egui::vec2(4096.0, 512.0)),
            clip: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640.0, 512.0)),
            mirrored: false,
        };
        assert!(previews.viewer_image(&ctx, 1, &source, view).is_none());
        assert_eq!(previews.pending.len(), 2);
        let request = work.try_recv().unwrap();
        let SourceKey::ImageRegion {
            region: [x, y, w, h],
            ..
        } = request.key.source
        else {
            panic!("Detail must have priority")
        };
        let (retained, result) = prepare(request.work);
        let pixels = result.unwrap();
        assert_eq!(pixels.dimensions(), (w, h));
        let mut expected = image::imageops::crop_imm(&original, x, y, w, h).to_image();
        artwork_color::premultiply(&mut expected);
        assert_eq!(
            pixels, expected,
            "Native detail must preserve every source texel"
        );
        previews.accept(
            &ctx,
            Completion {
                key: request.key,
                source: retained,
                result: Ok(pixels),
            },
        );
        complete(&mut previews, &work, &ctx);
        let sharp = previews.viewer_image(&ctx, 1, &source, view).unwrap();
        assert!(sharp.rect.contains_rect(view.clip));
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            sharp.paint(&ctx.layer_painter(egui::LayerId::background()))
        });
        assert_eq!(
            output
                .shapes
                .iter()
                .filter(|s| matches!(s.shape, egui::Shape::Mesh(_)))
                .count(),
            1,
            "Transparent detail must not paint over the base"
        );
        for step in 0..100 {
            let panned = ImageViewport {
                image: view.image.translate(egui::vec2(-(step as f32) * 15.0, 0.0)),
                ..view
            };
            assert!(previews.viewer_image(&ctx, 1, &source, panned).is_some());
        }
        assert!(previews.pending.len() <= REQUEST_CAPACITY);
        assert!(previews.cache.bytes <= GPU_BUDGET);
        previews.clear();
        complete(&mut previews, &work, &ctx);
        assert!(
            previews.cache.values.is_empty(),
            "Old detail must not enter a new document"
        );
        assert_eq!(compositor::decode_image(&source).unwrap(), original);
    }

    #[test]
    fn invalid_detail_requests_fail_without_panicking_or_allocating_huge_outputs() {
        let source = encoded(&image::RgbaImage::from_pixel(
            64,
            32,
            image::Rgba([60, 80, 100, 255]),
        ));
        for region in [
            [0, 0, 0, 32],
            [0, 0, 65, 32],
            [63, 0, u32::MAX, 1],
            [0, 32, 1, 1],
        ] {
            assert!(prepare(Work::ImageRegion {
                source: source.clone(),
                region,
                side: 2048
            })
            .1
            .is_err());
        }
        let ctx = egui::Context::default();
        let (mut previews, _work) = controlled_cache();
        let view = ImageViewport {
            source_size: [64, 32],
            image: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640.0, 320.0)),
            clip: egui::Rect::NOTHING,
            mirrored: false,
        };
        assert!(previews.viewer_image(&ctx, 1, &source, view).is_none());
        assert!(previews.pending.is_empty());
    }
}
