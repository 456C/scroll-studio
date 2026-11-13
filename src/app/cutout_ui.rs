use super::*;
use scroll_studio::{
    clipping::{ImageClip, SelectionMask},
    cutout::BackingSettings,
};

#[derive(Clone)]
struct Draft {
    revision: u64,
    settings: BackingSettings,
}
pub(super) struct SceneColorResult {
    epoch: u64,
    revision: u64,
    chapter: u64,
    layer: u64,
    color: [u8; 3],
    record_undo: bool,
}

pub(super) fn settings(layer: &Layer) -> BackingSettings {
    if let LayerKind::Image {
        clip: Some(clip), ..
    } = &layer.kind
    {
        clip.edge.backing
    } else {
        BackingSettings::default()
    }
}

fn set_backing(
    layer: &mut Layer,
    dimensions: [u32; 2],
    settings: BackingSettings,
) -> anyhow::Result<()> {
    let LayerKind::Image {
        mask,
        clip,
        source_rect,
        ..
    } = &mut layer.kind
    else {
        return Ok(());
    };
    let mut next = if let Some(clip) = clip {
        (**clip).clone()
    } else {
        let mut selection = SelectionMask::polygon(dimensions[0], dimensions[1], mask)?;
        if let Some([x, y, w, h]) = *source_rect {
            selection.intersect(&SelectionMask::polygon(
                dimensions[0],
                dimensions[1],
                &[[x, y], [x + w, y], [x + w, y + h], [x, y + h]],
            )?)?;
        }
        ImageClip {
            selection: Arc::new(selection),
            edge: Default::default(),
        }
    };
    next.edge.backing = settings;
    // Expand metadata crops into available source padding. Source pixel scale
    // and the character's position remain exactly the same on the canvas.
    if settings.enabled {
        if let (Some(old), Some([x, y, w, h])) =
            (*source_rect, next.selection.edge_bounds(next.edge))
        {
            let bounds = [
                x as f32 / dimensions[0] as f32,
                y as f32 / dimensions[1] as f32,
                w as f32 / dimensions[0] as f32,
                h as f32 / dimensions[1] as f32,
            ];
            let left = old[0].min(bounds[0]);
            let top = old[1].min(bounds[1]);
            let right = (old[0] + old[2]).max(bounds[0] + bounds[2]);
            let bottom = (old[1] + old[3]).max(bounds[1] + bounds[3]);
            layer.x += (left - old[0]) / old[2] * layer.width;
            layer.y += (top - old[1]) / old[3] * layer.height;
            layer.width *= (right - left) / old[2];
            layer.height *= (bottom - top) / old[3];
            *source_rect = Some([left, top, right - left, bottom - top]);
        }
    }
    mask.clear();
    *clip = Some(Arc::new(next));
    Ok(())
}

impl Studio {
    pub(super) fn enable_backing(&mut self, ctx: &egui::Context, id: u64) {
        let Some(index) = self.story.chapters[self.chapter]
            .layers
            .iter()
            .position(|l| l.id == id && !l.locked)
        else {
            return;
        };
        let mut layer = self.story.chapters[self.chapter].layers[index].clone();
        let LayerKind::Image { asset_id, .. } = layer.kind else {
            return;
        };
        let Some(asset) = self.story.assets.iter().find(|a| a.id == asset_id) else {
            return;
        };
        let mut backing = settings(&layer);
        backing.enabled = true;
        if let Err(error) = set_backing(&mut layer, [asset.width, asset.height], backing) {
            self.error = Some(error.to_string());
            return;
        }
        self.selected = Some(id);
        self.selected_asset = None;
        if layer != self.story.chapters[self.chapter].layers[index] {
            self.checkpoint();
            self.story.chapters[self.chapter].layers[index] = layer;
            self.changed();
            self.start_backing_match(ctx, id, false);
        }
        ctx.data_mut(|d| {
            d.insert_temp(
                egui::Id::new(("open_backing", self.document_epoch, id)),
                true,
            )
        });
    }
    pub(super) fn backing_editor(&mut self, ui: &mut egui::Ui, layer: &mut Layer) {
        let saved = settings(layer);
        let key = egui::Id::new(("backing_draft", self.document_epoch, layer.id));
        let mut draft = ui
            .ctx()
            .data(|d| d.get_temp::<Draft>(key))
            .filter(|d| d.revision == self.revision)
            .unwrap_or(Draft {
                revision: self.revision,
                settings: saved,
            });
        let opened = ui.ctx().data_mut(|d| {
            d.remove_temp::<bool>(egui::Id::new((
                "open_backing",
                self.document_epoch,
                layer.id,
            )))
        });
        egui::CollapsingHeader::new("Blend edges")
            .id_salt(("backing", layer.id))
            .open(opened)
            .show(ui, |ui| {
                ui.checkbox(&mut draft.settings.enabled, "Soft backing");
                if draft.settings.enabled {
                    ui.horizontal(|ui| {
                        let mut rgb = [
                            draft.settings.color[0],
                            draft.settings.color[1],
                            draft.settings.color[2],
                        ];
                        ui.color_edit_button_srgb(&mut rgb);
                        draft.settings.color[..3].copy_from_slice(&rgb);
                        if ui
                            .add_enabled(self.job.is_none(), motion::Button::new("Match scene"))
                            .clicked()
                        {
                            self.start_backing_match(ui.ctx(), layer.id, true);
                        }
                    });
                    ui.spacing_mut().slider_width =
                        (ui.available_width() - 146.0).clamp(42.0, 80.0);
                    let controls = egui::Grid::new(("backing_controls", layer.id))
                        .spacing(egui::vec2(8.0, 6.0))
                        .show(ui, |ui| {
                            ui.label("Opacity");
                            let mut opacity = (draft.settings.color[3] as f32 / 2.55).round();
                            if ui
                                .add(
                                    egui::Slider::new(&mut opacity, 0.0..=100.0)
                                        .suffix("%")
                                        .max_decimals(0),
                                )
                                .changed()
                            {
                                draft.settings.color[3] = (opacity * 2.55).round() as u8;
                            }
                            ui.end_row();
                            ui.label("Reach");
                            ui.add(
                                egui::Slider::new(&mut draft.settings.reach, 0..=512).suffix(" px"),
                            );
                            ui.end_row();
                            ui.label("Softness");
                            ui.add(
                                egui::Slider::new(&mut draft.settings.softness, 0..=100)
                                    .suffix("%"),
                            );
                            ui.end_row();
                            ui.label("Small gaps");
                            ui.add(
                                egui::Slider::new(&mut draft.settings.gaps, 0..=128).suffix(" px"),
                            );
                            ui.end_row();
                        });
                    #[cfg(test)]
                    ui.data_mut(|d| {
                        d.insert_temp(
                            egui::Id::new("backing_control_bounds"),
                            (controls.response.rect, ui.clip_rect()),
                        )
                    });
                    let _ = controls;
                }
            });
        if draft.settings != saved && !ui.input(|i| i.pointer.any_down()) {
            if let LayerKind::Image { asset_id, .. } = layer.kind {
                if let Some(asset) = self.story.assets.iter().find(|a| a.id == asset_id) {
                    if let Err(error) =
                        set_backing(layer, [asset.width, asset.height], draft.settings)
                    {
                        self.error = Some(error.to_string());
                    }
                }
            }
        }
        ui.ctx().data_mut(|d| d.insert_temp(key, draft));
    }
    pub(super) fn start_backing_match(&mut self, ctx: &egui::Context, id: u64, record_undo: bool) {
        if self.job.is_some() {
            return;
        }
        let Some(index) = self.story.chapters[self.chapter]
            .layers
            .iter()
            .position(|l| l.id == id)
        else {
            return;
        };
        let layer = self.story.chapters[self.chapter].layers[index].clone();
        let mut chapter = self.story.chapters[self.chapter].clone();
        chapter.layers.truncate(index);
        let assets = self.assets.clone();
        let epoch = self.document_epoch;
        let revision = self.revision;
        let wake = ctx.clone();
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<JobResult> {
                let scale = 256.0 / chapter.width as f32;
                let output_height =
                    (u64::from(chapter.height) * 256).div_ceil(u64::from(chapter.width)) as u32;
                let y = ((layer.y + layer.height * 0.5 - 64.0) * scale)
                    .clamp(0.0, output_height.saturating_sub(1) as f32)
                    as u32;
                let height = ((128.0 * scale).ceil() as u32)
                    .max(1)
                    .min(output_height - y);
                let image = compositor::render_region(&chapter, &assets, y, height, 256)?;
                let center = ((layer.x + layer.width * 0.5) / chapter.width as f32
                    * image.width() as f32)
                    .clamp(0.0, (image.width() - 1) as f32) as u32;
                let radius = (layer.width / chapter.width as f32 * image.width() as f32 * 0.25)
                    .clamp(1.0, 64.0) as u32;
                let mut sum = [0_u64; 3];
                let mut count = 0_u64;
                for py in 0..image.height() {
                    for px in
                        center.saturating_sub(radius)..=(center + radius).min(image.width() - 1)
                    {
                        let pixel = image.get_pixel(px, py);
                        for c in 0..3 {
                            sum[c] += u64::from(pixel[c]);
                        }
                        count += 1;
                    }
                }
                anyhow::ensure!(count > 0, "No scene color to sample");
                Ok(JobResult::BackingColor(SceneColorResult {
                    epoch,
                    revision,
                    chapter: chapter.id,
                    layer: id,
                    color: sum.map(|v| (v / count) as u8),
                    record_undo,
                }))
            })();
            let _ = sender.send(result);
            wake.request_repaint();
        });
        self.job = Some(receiver);
        self.busy_label = "Matching scene…".into();
    }
    pub(super) fn apply_backing_color(&mut self, result: SceneColorResult) {
        if result.epoch != self.document_epoch || result.revision != self.revision {
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
        let Some(index) = self.story.chapters[chapter]
            .layers
            .iter()
            .position(|l| l.id == result.layer && !l.locked)
        else {
            return;
        };
        let mut layer = self.story.chapters[chapter].layers[index].clone();
        let LayerKind::Image {
            clip: Some(clip), ..
        } = &mut layer.kind
        else {
            return;
        };
        if !clip.edge.backing.enabled {
            return;
        }
        let mut next = (**clip).clone();
        next.edge.backing.color[..3].copy_from_slice(&result.color);
        *clip = Arc::new(next);
        if result.record_undo {
            self.checkpoint();
        }
        self.story.chapters[chapter].layers[index] = layer;
        self.changed();
        self.status = "Backing matched to scene".into();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn backing_expands_crop_without_moving_or_rescaling_source_pixels() {
        let mut layer = Layer {
            extensions: Default::default(),
            id: 1,
            name: "Cutout".into(),
            x: 10.0,
            y: 20.0,
            width: 100.0,
            height: 100.0,
            visible: true,
            locked: false,
            opacity: 1.0,
            kind: LayerKind::Image {
                asset_id: 2,
                mask: vec![],
                clip: None,
                source_rect: Some([0.25, 0.25, 0.5, 0.5]),
            },
        };
        set_backing(
            &mut layer,
            [64, 64],
            BackingSettings {
                enabled: true,
                reach: 8,
                ..Default::default()
            },
        )
        .unwrap();
        let LayerKind::Image {
            source_rect: Some([x, y, w, h]),
            clip: Some(clip),
            ..
        } = &layer.kind
        else {
            panic!()
        };
        assert_eq!(layer.x + (0.25 - x) / w * layer.width, 10.0);
        assert_eq!(layer.y + (0.25 - y) / h * layer.height, 20.0);
        assert_eq!(layer.width / w, 200.0);
        assert_eq!(layer.height / h, 200.0);
        assert!(!clip.selection.contains(0, 0));
        assert!(clip.selection.contains(32, 32));
        assert!(layer.width > 100.0);
    }
    #[test]
    fn scene_match_samples_only_below_the_asset_and_joins_enable_into_one_undo() {
        let ctx = egui::Context::default();
        let mut studio = Studio::from_context(&ctx);
        studio.story = Story::with_canvas("Backing", 800, 2000);
        studio.assets.clear();
        let asset = studio.story.alloc_id();
        let id = studio.story.alloc_id();
        let bg = studio.story.alloc_id();
        studio.story.assets.push(Asset {
            extensions: Default::default(),
            provenance: None,
            id: asset,
            name: "Cutout".into(),
            kind: AssetKind::Image,
            width: 32,
            height: 32,
            notes: String::new(),
            prompt: String::new(),
            character: Default::default(),
        });
        let image = image::RgbaImage::from_pixel(32, 32, image::Rgba([230, 40, 20, 255]));
        let mut bytes = std::io::Cursor::new(Vec::new());
        image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
        studio.assets.insert(asset, Arc::new(bytes.into_inner()));
        studio.story.chapters[0].layers = vec![
            Layer {
                extensions: Default::default(),
                id: bg,
                name: "Ground".into(),
                x: 0.0,
                y: 0.0,
                width: 800.0,
                height: 2000.0,
                visible: true,
                locked: false,
                opacity: 1.0,
                kind: LayerKind::Shape {
                    color: [30, 60, 90, 255],
                },
            },
            Layer {
                extensions: Default::default(),
                id,
                name: "Subject".into(),
                x: 40.0,
                y: 1500.0,
                width: 100.0,
                height: 100.0,
                visible: true,
                locked: false,
                opacity: 1.0,
                kind: LayerKind::Image {
                    asset_id: asset,
                    mask: vec![],
                    clip: None,
                    source_rect: None,
                },
            },
        ];
        let original = studio.story.clone();
        let source = studio.assets[&asset].clone();
        studio.enable_backing(&ctx, id);
        let deadline = Instant::now() + std::time::Duration::from_secs(3);
        while studio.job.is_some() {
            assert!(Instant::now() < deadline);
            studio.poll_job(&ctx);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(
            settings(&studio.story.chapters[0].layers[1]).color[..3],
            [30, 60, 90]
        );
        assert_eq!(studio.undo.len(), 1);
        let saved = serde_json::to_vec(&studio.story).unwrap();
        let loaded: Story = serde_json::from_slice(&saved).unwrap();
        assert_eq!(loaded, studio.story);
        assert!(Arc::ptr_eq(&source, &studio.assets[&asset]));
        for width in [1100.0, 1280.0] {
            for frame in 0..3 {
                let _ = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, 720.0),
                        )),
                        time: Some(f64::from(width) + frame as f64),
                        ..Default::default()
                    },
                    |ctx| studio.inspector(ctx),
                );
            }
            let (controls, clip): (egui::Rect, egui::Rect) =
                ctx.data(|d| d.get_temp(egui::Id::new("backing_control_bounds")).unwrap());
            assert!(
                controls.left() >= clip.left() && controls.right() <= clip.right(),
                "Backing controls overflow the inspector: {controls:?} in {clip:?}"
            );
        }
        studio.undo();
        assert_eq!(studio.story, original);
    }
}
