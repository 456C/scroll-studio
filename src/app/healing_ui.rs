use super::*;
use scroll_studio::{clipping::SelectionMask, healing};

pub(super) struct HealingResult {
    bytes: Vec<u8>,
    width: u32,
    height: u32,
    x: f32,
    y: f32,
    placed_width: f32,
    placed_height: f32,
    name: String,
    opacity: f32,
    before: Option<u64>,
    epoch: u64,
    revision: u64,
    chapter: u64,
}
pub(super) enum HealInput {
    MotionStroke(Vec<[f32; 2]>, f32, f32, f32),
    Stroke(Vec<[f32; 2]>, f32),
    Selection(Arc<SelectionMask>, [f32; 4]),
}
impl Studio {
    pub(super) fn start_healing(&mut self, ctx: &egui::Context, input: HealInput) {
        if self.job.is_some() {
            self.status = "Wait for the current operation to finish".into();
            return;
        }
        let chapter = self.story.chapters[self.chapter].clone();
        let assets = assets_for_story(&self.story, &self.assets);
        let label = if matches!(input, HealInput::MotionStroke(..)) {
            "Motion blur"
        } else {
            "Content-aware patch"
        }
        .to_owned();
        let epoch = self.document_epoch;
        let revision = self.revision;
        let (sender, receiver) = mpsc::channel();
        let wake = ctx.clone();
        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<JobResult> {
                let patch = match input {
                    HealInput::MotionStroke(ref points, size, length, angle) => {
                        healing::motion_stroke(&chapter, &assets, points, size, length, angle)?
                    }
                    HealInput::Stroke(points, size) => {
                        healing::heal_stroke(&chapter, &assets, &points, size)?
                    }
                    HealInput::Selection(mask, rect) => {
                        healing::heal_selection(&chapter, &assets, &mask, rect)?
                    }
                };
                let (width, height) = patch.image.dimensions();
                let mut bytes = std::io::Cursor::new(Vec::new());
                patch.image.write_to(&mut bytes, image::ImageFormat::Png)?;
                Ok(JobResult::Healed(HealingResult {
                    bytes: bytes.into_inner(),
                    width,
                    height,
                    x: patch.x as f32,
                    y: patch.y as f32,
                    placed_width: width as f32,
                    placed_height: height as f32,
                    name: label,
                    opacity: 1.0,
                    before: None,
                    epoch,
                    revision,
                    chapter: chapter.id,
                }))
            })();
            let _ = sender.send(result);
            wake.request_repaint();
        });
        self.job = Some(receiver);
        self.busy_label = "Blending surroundings…".into();
    }
    pub(super) fn apply_healing(&mut self, result: HealingResult) {
        if result.epoch != self.document_epoch || result.revision != self.revision {
            self.status = "Fill discarded because the artwork changed".into();
            return;
        }
        let Some(chapter) = self
            .story
            .chapters
            .iter()
            .position(|c| c.id == result.chapter)
        else {
            return;
        };
        if self.story.assets.len() >= 4096 {
            self.error = Some("This story has reached its image limit".into());
            return;
        }
        self.checkpoint();
        let asset_id = self.story.alloc_id();
        let layer_id = self.story.alloc_id();
        self.story.assets.push(Asset {
            id: asset_id,
            name: result.name.clone(),
            kind: AssetKind::Image,
            notes: String::new(),
            prompt: String::new(),
            width: result.width,
            height: result.height,
            character: CharacterProfile::default(),
        });
        self.assets.insert(asset_id, Arc::new(result.bytes));
        let insertion = result
            .before
            .and_then(|id| {
                self.story.chapters[chapter]
                    .layers
                    .iter()
                    .position(|l| l.id == id)
            })
            .unwrap_or(self.story.chapters[chapter].layers.len());
        self.story.chapters[chapter].layers.insert(
            insertion,
            Layer {
                id: layer_id,
                name: result.name.clone(),
                visible: true,
                locked: false,
                opacity: result.opacity,
                x: result.x,
                y: result.y,
                width: result.placed_width,
                height: result.placed_height,
                kind: LayerKind::Image {
                    source_rect: None,
                    asset_id,
                    mask: vec![],
                    clip: None,
                },
            },
        );
        self.selected = Some(layer_id);
        self.selected_asset = None;
        self.lasso_state.cancel();
        self.changed();
        self.status = format!("{} added", result.name);
    }

    pub(super) fn generate_smear(
        &mut self,
        ctx: &egui::Context,
        motion: &scroll_studio::effects::Motion,
    ) {
        if self.job.is_some() {
            return;
        }
        let Some(layer) = self.story.chapters[self.chapter]
            .layers
            .iter()
            .find(|l| l.id == motion.source_layer)
            .cloned()
        else {
            self.status = "The source image is no longer in this chapter".into();
            return;
        };
        let LayerKind::Image {
            asset_id,
            mask,
            clip,
            source_rect,
        } = layer.kind
        else {
            return;
        };
        let Some(bytes) = self.assets.get(&asset_id).cloned() else {
            return;
        };
        let epoch = self.document_epoch;
        let revision = self.revision;
        let chapter = self.story.chapters[self.chapter].id;
        let motion = motion.clone();
        let wake = ctx.clone();
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<JobResult> {
                let source = compositor::decode_image(&bytes)?;
                let mut image = compositor::masked_image(&source, &mask);
                drop(source);
                if let Some(clip) = clip {
                    scroll_studio::clipping::apply(&mut image, &clip)?;
                }
                if let Some([x, y, w, h]) = source_rect {
                    let sw = image.width() as f32;
                    let sh = image.height() as f32;
                    let x0 = (x * sw).floor() as u32;
                    let y0 = (y * sh).floor() as u32;
                    let x1 = ((x + w) * sw).ceil().min(sw) as u32;
                    let y1 = ((y + h) * sh).ceil().min(sh) as u32;
                    image = image::imageops::crop_imm(&image, x0, y0, x1 - x0, y1 - y0).to_image();
                }
                // Only the generated streak layer uses a bounded working image.
                // The linked artwork retains its original bytes and resolution.
                if image.width().max(image.height()) > 1400 {
                    let ratio = 1400.0 / image.width().max(image.height()) as f32;
                    let size = [
                        (image.width() as f32 * ratio).round().max(1.0) as u32,
                        (image.height() as f32 * ratio).round().max(1.0) as u32,
                    ];
                    super::artwork_color::premultiply(&mut image);
                    image = image::imageops::resize(
                        &image,
                        size[0],
                        size[1],
                        image::imageops::FilterType::Triangle,
                    );
                    for pixel in image.pixels_mut() {
                        if pixel[3] > 0 {
                            let alpha = u32::from(pixel[3]);
                            for c in 0..3 {
                                pixel[c] = (u32::from(pixel[c]) * 255 / alpha).min(255) as u8;
                            }
                        }
                    }
                }
                let scale_x = layer.width / image.width() as f32;
                let scale_y = layer.height / image.height() as f32;
                let reach = (image.width().max(image.height()) as f32 * 0.15 * motion.length)
                    .round()
                    .clamp(1.0, 256.0) as u32;
                let image =
                    healing::smear_image(&image, reach, motion.angle, motion.smear_threshold)?;
                let (width, height) = image.dimensions();
                let mut encoded = std::io::Cursor::new(Vec::new());
                image.write_to(&mut encoded, image::ImageFormat::Png)?;
                Ok(JobResult::Healed(HealingResult {
                    bytes: encoded.into_inner(),
                    width,
                    height,
                    x: layer.x - reach as f32 * scale_x,
                    y: layer.y - reach as f32 * scale_y,
                    placed_width: width as f32 * scale_x,
                    placed_height: height as f32 * scale_y,
                    name: "Motion smear".into(),
                    opacity: layer.opacity,
                    before: Some(layer.id),
                    epoch,
                    revision,
                    chapter,
                }))
            })();
            let _ = sender.send(result);
            wake.request_repaint();
        });
        self.job = Some(receiver);
        self.busy_label = "Generating smear…".into();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generated_patches_are_undoable_and_stale_results_cannot_modify_a_story() {
        let ctx = egui::Context::default();
        let mut studio = Studio::from_context(&ctx);
        let original = studio.story.clone();
        let result = |epoch, revision, chapter| {
            let mut bytes = std::io::Cursor::new(Vec::new());
            image::RgbaImage::from_pixel(8, 8, image::Rgba([50, 90, 120, 128]))
                .write_to(&mut bytes, image::ImageFormat::Png)
                .unwrap();
            HealingResult {
                bytes: bytes.into_inner(),
                width: 8,
                height: 8,
                x: 10.0,
                y: 20.0,
                placed_width: 8.0,
                placed_height: 8.0,
                name: "Patch".into(),
                opacity: 0.6,
                before: None,
                epoch,
                revision,
                chapter,
            }
        };
        studio.apply_healing(result(
            studio.document_epoch + 1,
            studio.revision,
            studio.story.chapters[0].id,
        ));
        assert_eq!(studio.story, original);
        studio.apply_healing(result(
            studio.document_epoch,
            studio.revision + 1,
            studio.story.chapters[0].id,
        ));
        assert_eq!(studio.story, original);
        studio.apply_healing(result(
            studio.document_epoch,
            studio.revision,
            studio.story.chapters[0].id,
        ));
        assert_eq!(studio.story.chapters[0].layers.last().unwrap().opacity, 0.6);
        assert_eq!(studio.undo.len(), 1);
        storage::validate_story(
            &studio.story,
            &assets_for_story(&studio.story, &studio.assets),
        )
        .unwrap();
        studio.undo();
        assert_eq!(studio.story, original);
    }
}
