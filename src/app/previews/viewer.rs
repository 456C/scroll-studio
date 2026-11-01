//! Display-sized references, with a bounded source crop when zoomed in.
use super::*;

#[derive(Clone, Copy)]
pub(crate) struct ImageViewport {
    pub source_size: [u32; 2],
    pub image: egui::Rect,
    pub clip: egui::Rect,
    pub mirrored: bool,
}

pub(crate) struct ViewerImage {
    pub(crate) texture: TextureHandle,
    pub(crate) rect: egui::Rect,
    pub(crate) mirrored: bool,
}

impl ViewerImage {
    pub(crate) fn paint(&self, painter: &egui::Painter) {
        artwork_color::transparency_grid(painter, self.rect);
        self.paint_tinted(painter, egui::Color32::WHITE);
    }
    pub(crate) fn paint_tinted(&self, painter: &egui::Painter, tint: egui::Color32) {
        let (left, right) = if self.mirrored {
            (1.0, 0.0)
        } else {
            (0.0, 1.0)
        };
        painter.image(
            self.texture.id(),
            self.rect,
            egui::Rect::from_min_max(egui::pos2(left, 0.0), egui::pos2(right, 1.0)),
            tint,
        );
    }
}

fn resolution(side: f32) -> u32 {
    [384, 768, 1024, 1536, 2048]
        .into_iter()
        .find(|limit| side <= *limit as f32)
        .unwrap_or(2048)
}

#[derive(Debug)]
struct Detail {
    region: [u32; 4],
    rect: egui::Rect,
    side: u32,
}

impl ImageViewport {
    fn detail(self, density: f32) -> Option<Detail> {
        if self.image.size().max_elem() * density <= 2048.0 {
            return None;
        }
        let visible = self.image.intersect(self.clip);
        let size = self.image.size();
        let mut min = (visible.min - self.image.min) / size;
        let mut max = (visible.max - self.image.min) / size;
        if self.mirrored {
            (min.x, max.x) = (1.0 - max.x, 1.0 - min.x);
        }
        // A small, stable margin keeps tiny pans from re-decoding every frame.
        let lower = |value: f32, pixels: u32| {
            (((value * 64.0).floor() - 1.0).max(0.0) / 64.0 * pixels as f32).floor() as u32
        };
        let upper = |value: f32, pixels: u32| {
            (((value * 64.0).ceil() + 1.0).min(64.0) / 64.0 * pixels as f32).ceil() as u32
        };
        let [width, height] = self.source_size;
        let x0 = lower(min.x, width).min(width);
        let x1 = upper(max.x, width).min(width);
        let y0 = lower(min.y, height).min(height);
        let y1 = upper(max.y, height).min(height);
        if x0 >= x1 || y0 >= y1 {
            return None;
        }
        let mut left = x0 as f32 / width as f32;
        let mut right = x1 as f32 / width as f32;
        if self.mirrored {
            (left, right) = (1.0 - right, 1.0 - left);
        }
        let rect = egui::Rect::from_min_max(
            self.image.min + egui::vec2(left, y0 as f32 / height as f32) * size,
            self.image.min + egui::vec2(right, y1 as f32 / height as f32) * size,
        );
        Some(Detail {
            region: [x0, y0, x1 - x0, y1 - y0],
            rect,
            side: resolution(rect.size().max_elem() * density),
        })
    }
}

impl PreviewCache {
    pub(crate) fn source_crop_viewer(
        &mut self,
        ctx: &egui::Context,
        asset_id: u64,
        source: &Arc<Vec<u8>>,
        clip: Option<&Arc<scroll_studio::clipping::ImageClip>>,
        source_rect: [f32; 4],
        view: ImageViewport,
    ) -> Option<ViewerImage> {
        let [x, y, w, h] = source_rect;
        let [sw, sh] = view.source_size;
        let x0 = (x * sw as f32).floor() as u32;
        let y0 = (y * sh as f32).floor() as u32;
        let x1 = ((x + w) * sw as f32).ceil().min(sw as f32) as u32;
        let y1 = ((y + h) * sh as f32).ceil().min(sh as f32) as u32;
        if x0 >= x1
            || y0 >= y1
            || !view.image.is_positive()
            || !view.image.intersect(view.clip).is_positive()
        {
            return None;
        }
        let crop = [x0, y0, x1 - x0, y1 - y0];
        let view = ImageViewport {
            source_size: [crop[2], crop[3]],
            ..view
        };
        let density = ctx.pixels_per_point() * self.quality.scale();
        let side = self
            .quality
            .limit(resolution(view.image.size().max_elem() * density));
        let generation = self.generation.load(Ordering::Acquire);
        let identity = Arc::as_ptr(source) as usize;
        let mut requests = Vec::with_capacity(2);
        if let Some(detail) = view.detail(density) {
            requests.push((
                [
                    detail.region[0] + x0,
                    detail.region[1] + y0,
                    detail.region[2],
                    detail.region[3],
                ],
                self.quality.limit(detail.side),
                detail.rect,
            ));
        }
        requests.push((crop, side, view.image));
        for (region, side, rect) in requests {
            let source_key = if let Some(clip) = clip {
                SourceKey::ClippedImage {
                    asset_id,
                    identity,
                    clip: ClipKey(clip.clone()),
                    region: Some(region),
                    side,
                }
            } else {
                SourceKey::ImageRegion {
                    asset_id,
                    identity,
                    region,
                    side,
                }
            };
            let key = Key {
                generation,
                source: source_key,
            };
            if let Some(entry) = self.cache.get(&key) {
                if let Some(texture) = &entry.texture {
                    return Some(ViewerImage {
                        texture: texture.clone(),
                        rect,
                        mirrored: false,
                    });
                }
            } else if !self.pending.contains(&key) && self.pending.len() < MAX_PENDING {
                let work = if let Some(clip) = clip {
                    Work::ClippedImage {
                        source: source.clone(),
                        clip: clip.clone(),
                        region: Some(region),
                        side,
                    }
                } else {
                    Work::ImageRegion {
                        source: source.clone(),
                        region,
                        side,
                    }
                };
                self.submit(ctx, key, work);
            }
        }
        let fallback = self.cache.values.iter().find_map(|(key, entry)| {
            if key.generation != generation || entry.value.texture.is_none() {
                return None;
            }
            let same = match &key.source {
                SourceKey::ImageRegion {
                    asset_id: id,
                    identity: allocation,
                    region,
                    ..
                } => {
                    clip.is_none() && *id == asset_id && *allocation == identity && *region == crop
                }
                SourceKey::ClippedImage {
                    asset_id: id,
                    identity: allocation,
                    region: Some(region),
                    clip: old,
                    ..
                } => {
                    clip.is_some_and(|c| Arc::ptr_eq(c, &old.0))
                        && *id == asset_id
                        && *allocation == identity
                        && *region == crop
                }
                _ => false,
            };
            same.then_some(key.clone())
        })?;
        Some(ViewerImage {
            texture: self.cache.get(&fallback)?.texture.clone()?,
            rect: view.image,
            mirrored: false,
        })
    }
    pub(crate) fn source_size(&mut self, id: u64, source: &Arc<Vec<u8>>) -> Option<[u32; 2]> {
        let key = (id, Arc::as_ptr(source) as usize);
        if let Some((_, size)) = self.dimensions.get(&key) {
            return Some(*size);
        }
        let reader = image::ImageReader::new(std::io::Cursor::new(source.as_slice()))
            .with_guessed_format()
            .ok()?;
        let (w, h) = reader.into_dimensions().ok()?;
        if w == 0 || h == 0 || w > 16_384 || h > 16_384 || u64::from(w) * u64::from(h) > 32_000_000
        {
            return None;
        }
        if self.dimensions.len() >= 128 {
            self.dimensions.clear();
        }
        self.dimensions.insert(key, (source.clone(), [w, h]));
        Some([w, h])
    }

    pub(crate) fn clipped_viewer(
        &mut self,
        ctx: &egui::Context,
        asset_id: u64,
        source: &Arc<Vec<u8>>,
        clip: &Arc<scroll_studio::clipping::ImageClip>,
        view: ImageViewport,
    ) -> Option<ViewerImage> {
        if !view.image.is_finite()
            || !view.image.is_positive()
            || !view.clip.is_finite()
            || !view.image.intersect(view.clip).is_positive()
            || view.source_size.contains(&0)
        {
            return None;
        }
        let density = ctx.pixels_per_point() * self.quality.scale();
        let generation = self.generation.load(Ordering::Acquire);
        let identity = Arc::as_ptr(source) as usize;
        let detail = view.detail(density);
        let side = self
            .quality
            .limit(resolution(view.image.size().max_elem() * density));
        let mut requests = Vec::with_capacity(2);
        if let Some(detail) = detail {
            requests.push((
                Some(detail.region),
                self.quality.limit(detail.side),
                detail.rect,
            ));
        }
        requests.push((None, side, view.image));
        for (region, side, rect) in requests {
            let key = Key {
                generation,
                source: SourceKey::ClippedImage {
                    asset_id,
                    identity,
                    clip: ClipKey(clip.clone()),
                    region,
                    side,
                },
            };
            if let Some(entry) = self.cache.get(&key) {
                if let Some(texture) = &entry.texture {
                    return Some(ViewerImage {
                        texture: texture.clone(),
                        rect,
                        mirrored: view.mirrored,
                    });
                }
            } else if !self.pending.contains(&key) && self.pending.len() < MAX_PENDING {
                self.submit(
                    ctx,
                    key,
                    Work::ClippedImage {
                        source: source.clone(),
                        clip: clip.clone(),
                        region,
                        side,
                    },
                );
            }
        }
        // Only an exact selection/appearance can stand in while a new size loads.
        let fallback = self
            .cache
            .values
            .iter()
            .filter_map(|(key, entry)| match &key.source {
                SourceKey::ClippedImage {
                    asset_id: id,
                    identity: allocation,
                    clip: old,
                    region: None,
                    side: old_side,
                } if key.generation == generation
                    && *id == asset_id
                    && *allocation == identity
                    && Arc::ptr_eq(&old.0, clip)
                    && entry.value.texture.is_some() =>
                {
                    Some((
                        key.clone(),
                        (f64::from(*old_side) / f64::from(side)).ln().abs(),
                    ))
                }
                _ => None,
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(key, _)| key)?;
        Some(ViewerImage {
            texture: self.cache.get(&fallback)?.texture.clone()?,
            rect: view.image,
            mirrored: view.mirrored,
        })
    }
    pub(crate) fn viewer_image(
        &mut self,
        ctx: &egui::Context,
        asset_id: u64,
        source: &Arc<Vec<u8>>,
        view: ImageViewport,
    ) -> Option<ViewerImage> {
        let density = ctx.pixels_per_point() * self.quality.scale();
        if !view.image.is_finite()
            || !view.clip.is_finite()
            || !view.image.is_positive()
            || !view.image.intersect(view.clip).is_positive()
            || view.source_size.contains(&0)
            || !density.is_finite()
            || density <= 0.0
        {
            return None;
        }
        #[cfg(test)]
        ctx.data_mut(|data| {
            data.insert_temp(
                egui::Id::new(("viewer_resolution", asset_id)),
                resolution(view.image.size().max_elem() * density),
            )
        });
        let generation = self.generation.load(Ordering::Acquire);
        let identity = Arc::as_ptr(source) as usize;
        if let Some(detail) = view.detail(density) {
            let key = Key {
                generation,
                source: SourceKey::ImageRegion {
                    asset_id,
                    identity,
                    region: detail.region,
                    side: self.quality.limit(detail.side),
                },
            };
            if let Some(entry) = self.cache.get(&key) {
                if let Some(texture) = &entry.texture {
                    // The detail covers the whole visible viewport. Paint once;
                    // drawing over a thumbnail would double-blend transparency.
                    return Some(ViewerImage {
                        texture: texture.clone(),
                        rect: detail.rect,
                        mirrored: view.mirrored,
                    });
                }
            } else if !self.pending.contains(&key) && self.pending.len() < MAX_PENDING {
                self.submit(
                    ctx,
                    key,
                    Work::ImageRegion {
                        source: source.clone(),
                        region: detail.region,
                        side: self.quality.limit(detail.side),
                    },
                );
            }
        }
        let side = self
            .quality
            .limit(resolution(view.image.size().max_elem() * density));
        let texture = self.image(ctx, asset_id, source, &[], side).or_else(|| {
            // Existing full-image previews remain visible while detail loads.
            // Asset bytes and mask must match; cropped regions are never reused here.
            let fallback = self
                .cache
                .values
                .iter()
                .filter_map(|(key, entry)| {
                    if key.generation != generation || entry.value.texture.is_none() {
                        return None;
                    }
                    match &key.source {
                        SourceKey::Image {
                            asset_id: id,
                            identity: allocation,
                            side: old,
                            mask,
                        } if *id == asset_id && *allocation == identity && mask.is_empty() => {
                            Some((key.clone(), (f64::from(*old) / f64::from(side)).ln().abs()))
                        }
                        _ => None,
                    }
                })
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(key, _)| key);
            self.cache.get(&fallback?)?.texture.clone()
        })?;
        Some(ViewerImage {
            texture,
            rect: view.image,
            mirrored: view.mirrored,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn details_cover_the_viewport_and_map_mirrored_source_pixels_at_all_densities() {
        for source_size in [[4096, 3072], [2571, 1793], [500, 9000]] {
            for density in [1.0, 1.5, 2.0] {
                for mirrored in [false, true] {
                    for offset in [
                        egui::vec2(-3500.0, -1700.0),
                        egui::vec2(-123.4, -97.2),
                        egui::Vec2::ZERO,
                    ] {
                        let view = ImageViewport {
                            source_size,
                            image: egui::Rect::from_min_size(
                                egui::pos2(30.0, 70.0) + offset,
                                egui::vec2(4800.0, 3600.0),
                            ),
                            clip: egui::Rect::from_min_size(
                                egui::pos2(30.0, 70.0),
                                egui::vec2(640.0, 500.0),
                            ),
                            mirrored,
                        };
                        let detail = view.detail(density).unwrap();
                        assert!(detail
                            .rect
                            .expand(0.001)
                            .contains_rect(view.image.intersect(view.clip)));
                        let [x, y, w, h] = detail.region;
                        assert!(
                            w > 0 && h > 0 && x + w <= source_size[0] && y + h <= source_size[1]
                        );
                        assert!(detail.side <= 2048);
                        let normalized =
                            (detail.rect.center() - view.image.min) / view.image.size();
                        let u = if mirrored {
                            1.0 - normalized.x
                        } else {
                            normalized.x
                        };
                        assert!(
                            (u * source_size[0] as f32 - (x as f32 + w as f32 * 0.5)).abs() < 0.002
                        );
                        assert!(
                            (normalized.y * source_size[1] as f32 - (y as f32 + h as f32 * 0.5))
                                .abs()
                                < 0.002
                        );
                    }
                }
            }
        }
        let mut view = ImageViewport {
            source_size: [4096, 4096],
            image: egui::Rect::from_min_size(
                egui::pos2(-1025.0, -1025.0),
                egui::vec2(4096.0, 4096.0),
            ),
            clip: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640.0, 640.0)),
            mirrored: false,
        };
        let first = view.detail(1.0).unwrap().region;
        view.image = view.image.translate(egui::vec2(-0.1, -0.1));
        assert_eq!(
            view.detail(1.0).unwrap().region,
            first,
            "Subpixel pan must reuse detail"
        );
    }
    #[test]
    fn adaptive_limits_apply_to_zoomed_detail_requests_too() {
        let ctx = egui::Context::default();
        let mut previews = PreviewCache::new();
        for n in 0..6 {
            previews.quality.observe(0.06, true, n as f64 * 0.03);
        }
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::RgbaImage::new(64, 64)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        let source = Arc::new(bytes.into_inner());
        let view = ImageViewport {
            source_size: [64, 64],
            image: egui::Rect::from_min_size(
                egui::pos2(-3000.0, -3000.0),
                egui::vec2(8000.0, 8000.0),
            ),
            clip: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(2500.0, 1800.0)),
            mirrored: false,
        };
        previews.viewer_image(&ctx, 1, &source, view);
        assert!(!previews.pending.is_empty());
        for key in &previews.pending {
            match &key.source {
                SourceKey::Image { side, .. } | SourceKey::ImageRegion { side, .. } => {
                    assert!(*side <= 768)
                }
                _ => {}
            }
        }
    }
}
