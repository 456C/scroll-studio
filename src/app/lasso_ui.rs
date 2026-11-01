use super::*;
use scroll_studio::clipping::{
    Contour, EdgePattern, EdgeSettings, ImageClip, SelectionMask, SelectionMode,
};

pub(super) struct LassoState {
    pub edge: EdgeSettings,
    pub tolerance: u8,
    pub stabilizer: f32,
    pub mode: SelectionMode,
    bound: Option<u64>,
    editing: bool,
    job: Option<SelectionJob>,
    selection: Option<ActiveSelection>,
}
impl Default for LassoState {
    fn default() -> Self {
        Self {
            edge: EdgeSettings::default(),
            tolerance: 28,
            stabilizer: 0.0,
            mode: SelectionMode::Replace,
            bound: None,
            editing: false,
            job: None,
            selection: None,
        }
    }
}
impl LassoState {
    pub fn cancel(&mut self) {
        self.bound = None;
        self.editing = false;
        self.selection = None;
        if let Some(job) = &mut self.job {
            job.epoch = u64::MAX;
        }
    }
}
type SelectionResult = anyhow::Result<(Arc<ImageClip>, Vec<[[f32; 2]; 2]>)>;
struct SelectionJob {
    receiver: mpsc::Receiver<SelectionResult>,
    epoch: u64,
    revision: u64,
    chapter: u64,
    layer: u64,
    apply_crop: bool,
}
struct ActiveSelection {
    epoch: u64,
    revision: u64,
    chapter: u64,
    layer: u64,
    mask: Arc<SelectionMask>,
    outline: Vec<[[f32; 2]; 2]>,
}
pub(super) enum SelectionInput {
    Polygon(Vec<[f32; 2]>),
    Smart([u32; 2]),
}
pub(super) fn selection_mode(modifiers: egui::Modifiers) -> SelectionMode {
    if modifiers.alt {
        SelectionMode::Subtract
    } else if modifiers.shift {
        SelectionMode::Add
    } else {
        SelectionMode::Replace
    }
}

impl Studio {
    pub(super) fn separate_selection(&mut self, copy: bool) {
        let Some(selection) = self.active_selection() else {
            return;
        };
        let mask = selection.mask.clone();
        let id = selection.layer;
        let Some(index) = self.story.chapters[self.chapter]
            .layers
            .iter()
            .position(|l| l.id == id && !l.locked)
        else {
            return;
        };
        let original = self.story.chapters[self.chapter].layers[index].clone();
        let LayerKind::Image {
            asset_id,
            mask: legacy,
            clip,
            source_rect,
        } = &original.kind
        else {
            return;
        };
        let Some([sx, sy, sw, sh]) = mask.edge_bounds(self.lasso_state.edge) else {
            return;
        };
        let source = [mask.width as f32, mask.height as f32];
        let [cx, cy, cw, ch] = source_rect.unwrap_or([0.0, 0.0, 1.0, 1.0]);
        let crop = [
            sx as f32 / source[0],
            sy as f32 / source[1],
            sw as f32 / source[0],
            sh as f32 / source[1],
        ];
        let remaining = if copy {
            None
        } else {
            let result = (|| -> anyhow::Result<Arc<ImageClip>> {
                let mut rest = if let Some(clip) = clip {
                    (*clip.selection).clone()
                } else {
                    SelectionMask::polygon(mask.width, mask.height, legacy)?
                };
                rest.combine(&mask, SelectionMode::Subtract)?;
                Ok(Arc::new(ImageClip {
                    selection: Arc::new(rest),
                    edge: clip
                        .as_ref()
                        .map_or(EdgeSettings::default(), |clip| clip.edge),
                }))
            })();
            match result {
                Ok(rest) => Some(rest),
                Err(error) => {
                    self.error = Some(error.to_string());
                    return;
                }
            }
        };
        self.checkpoint();
        let new_id = self.story.alloc_id();
        let mut part = original.clone();
        part.id = new_id;
        part.name = format!("{} detail", original.name);
        part.x = original.x + (crop[0] - cx) / cw * original.width;
        part.y = original.y + (crop[1] - cy) / ch * original.height;
        part.width = crop[2] / cw * original.width;
        part.height = crop[3] / ch * original.height;
        part.kind = LayerKind::Image {
            asset_id: *asset_id,
            mask: vec![],
            clip: Some(Arc::new(ImageClip {
                selection: mask,
                edge: self.lasso_state.edge,
            })),
            source_rect: Some(crop),
        };
        if let Some(remaining) = remaining {
            if let LayerKind::Image { mask, clip, .. } =
                &mut self.story.chapters[self.chapter].layers[index].kind
            {
                mask.clear();
                *clip = Some(remaining);
            }
        }
        self.story.chapters[self.chapter]
            .layers
            .insert(index + 1, part);
        self.selected = Some(new_id);
        self.lasso_state.cancel();
        self.tool = Tool::Move;
        self.changed();
        self.status = "Selection separated; original image reused".into();
    }
    pub(super) fn selection_actions(&mut self, ui: &mut egui::Ui) {
        if self.active_selection().is_none() {
            return;
        }
        if motion::button(ui, "Crop selection").clicked() {
            self.apply_selection_crop();
            ui.close_menu();
        }
        if motion::button(ui, "Separate to layer").clicked() {
            self.separate_selection(false);
            ui.close_menu();
        }
        if motion::button(ui, "Copy to layer").clicked() {
            self.separate_selection(true);
            ui.close_menu();
        }
        if ui
            .add_enabled(
                self.job.is_none(),
                motion::Button::new("Content-aware fill"),
            )
            .clicked()
        {
            self.fill_active_selection(ui.ctx());
            ui.close_menu();
        }
    }
    fn active_selection(&self) -> Option<&ActiveSelection> {
        self.lasso_state.selection.as_ref().filter(|s| {
            s.epoch == self.document_epoch
                && s.revision == self.revision
                && Some(s.layer) == self.selected
                && self.story.chapters[self.chapter].id == s.chapter
        })
    }
    pub(super) fn fill_active_selection(&mut self, ctx: &egui::Context) {
        let Some(selection) = self.active_selection() else {
            return;
        };
        let Some(layer) = self.story.chapters[self.chapter]
            .layers
            .iter()
            .find(|l| l.id == selection.layer)
        else {
            return;
        };
        let [cx, cy, cw, ch] = if let LayerKind::Image { source_rect, .. } = layer.kind {
            source_rect.unwrap_or([0.0, 0.0, 1.0, 1.0])
        } else {
            [0.0, 0.0, 1.0, 1.0]
        };
        self.start_healing(
            ctx,
            super::healing_ui::HealInput::Selection(
                selection.mask.clone(),
                [
                    layer.x - cx / cw * layer.width,
                    layer.y - cy / ch * layer.height,
                    layer.width / cw,
                    layer.height / ch,
                ],
            ),
        );
    }
    pub(super) fn apply_selection_crop(&mut self) {
        let Some(selection) = self.active_selection() else {
            return;
        };
        let (id, mask) = (selection.layer, selection.mask.clone());
        let Some(index) = self.story.chapters[self.chapter]
            .layers
            .iter()
            .position(|l| l.id == id && !l.locked)
        else {
            return;
        };
        self.checkpoint();
        if let LayerKind::Image {
            mask: legacy, clip, ..
        } = &mut self.story.chapters[self.chapter].layers[index].kind
        {
            legacy.clear();
            *clip = Some(Arc::new(ImageClip {
                selection: mask,
                edge: self.lasso_state.edge,
            }));
        }
        self.changed();
        if let Some(selection) = &mut self.lasso_state.selection {
            selection.revision = self.revision;
        }
        self.status = "Crop applied".into();
    }
    pub(super) fn paint_selection_outline(
        &self,
        painter: &egui::Painter,
        canvas: egui::Rect,
        zoom: f32,
    ) {
        let Some(selection) = self.active_selection() else {
            return;
        };
        let Some(layer) = self.story.chapters[self.chapter]
            .layers
            .iter()
            .find(|l| l.id == selection.layer)
        else {
            return;
        };
        let origin = canvas.min + egui::vec2(layer.x, layer.y) * zoom;
        let size = egui::vec2(layer.width, layer.height) * zoom;
        let painter = painter.with_clip_rect(
            painter
                .clip_rect()
                .intersect(egui::Rect::from_min_size(origin, size)),
        );
        for line in &selection.outline {
            let crop = if let LayerKind::Image {
                source_rect: Some(crop),
                ..
            } = layer.kind
            {
                crop
            } else {
                [0.0, 0.0, 1.0, 1.0]
            };
            let points = line.map(|p| {
                origin + egui::vec2((p[0] - crop[0]) / crop[2], (p[1] - crop[1]) / crop[3]) * size
            });
            painter.line_segment(
                points,
                egui::Stroke::new(2.5_f32, Color32::from_black_alpha(190)),
            );
            painter.line_segment(
                points,
                egui::Stroke::new(1.0_f32, Color32::from_rgb(255, 203, 120)),
            );
        }
    }
    pub(super) fn lasso_controls(&mut self, ui: &mut egui::Ui) {
        let selected = self.story.chapters[self.chapter]
            .layers
            .iter()
            .find(|l| {
                Some(l.id) == self.selected
                    && !l.locked
                    && matches!(l.kind, LayerKind::Image { .. })
            })
            .cloned();
        if self.lasso_state.bound != selected.as_ref().map(|l| l.id) {
            self.lasso_state.bound = selected.as_ref().map(|l| l.id);
            if let Some(Layer {
                kind: LayerKind::Image {
                    clip: Some(clip), ..
                },
                ..
            }) = &selected
            {
                self.lasso_state.edge = clip.edge;
            }
        }
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 7.0;
            egui::ComboBox::from_id_salt("lasso_pattern")
                .width(76.0)
                .selected_text(self.lasso_state.edge.pattern.label())
                .show_ui(ui, |ui| {
                    for pattern in EdgePattern::ALL {
                        ui.selectable_value(
                            &mut self.lasso_state.edge.pattern,
                            pattern,
                            pattern.label(),
                        );
                    }
                });
            ui.label("Width");
            ui.add(
                egui::DragValue::new(&mut self.lasso_state.edge.width)
                    .range(0..=512)
                    .suffix(" px"),
            )
            .on_hover_text("Edge width in original image pixels");
            let more = motion::menu_button(ui, "Edge…", |ui| {
                ui.set_min_width(215.0);
                ui.horizontal(|ui| {
                    ui.label("Contour");
                    egui::ComboBox::from_id_salt("lasso_contour")
                        .selected_text(self.lasso_state.edge.contour.label())
                        .show_ui(ui, |ui| {
                            for contour in Contour::ALL {
                                ui.selectable_value(
                                    &mut self.lasso_state.edge.contour,
                                    contour,
                                    contour.label(),
                                );
                            }
                        });
                });
                if self.lasso_state.edge.pattern == EdgePattern::Dotted {
                    ui.add(
                        egui::Slider::new(&mut self.lasso_state.edge.dot_scale, 2..=128)
                            .text("Dot scale"),
                    );
                }
                ui.separator();
                let mut enabled = self.lasso_state.edge.background[3] > 0;
                if ui.checkbox(&mut enabled, "Edge background").changed() {
                    self.lasso_state.edge.background[3] = if enabled { 255 } else { 0 };
                }
                if enabled {
                    let mut rgb = [
                        self.lasso_state.edge.background[0],
                        self.lasso_state.edge.background[1],
                        self.lasso_state.edge.background[2],
                    ];
                    ui.horizontal(|ui| {
                        ui.color_edit_button_srgb(&mut rgb);
                        ui.label("Color");
                    });
                    self.lasso_state.edge.background[..3].copy_from_slice(&rgb);
                    let mut percent = self.lasso_state.edge.background[3] as f32 / 255.0 * 100.0;
                    if ui
                        .add(
                            egui::Slider::new(&mut percent, 0.0..=100.0)
                                .text("Opacity")
                                .suffix("%"),
                        )
                        .changed()
                    {
                        self.lasso_state.edge.background[3] = (percent * 2.55).round() as u8;
                    }
                }
                ui.separator();
                self.selection_actions(ui);
                if ui
                    .add_enabled(selected.is_some(), motion::Button::new("Reset clip"))
                    .clicked()
                {
                    if let Some(layer) = &selected {
                        self.reset_image_clip(layer.id);
                    }
                    ui.close_menu();
                }
            })
            .response;
            more.on_hover_text("Contour, dot scale and edge background");
            if ui
                .add_enabled(
                    self.active_selection().is_some(),
                    motion::Button::new("Crop"),
                )
                .on_hover_text("Apply this selection as a non-destructive crop")
                .clicked()
            {
                self.apply_selection_crop();
            }
            if self.tool == Tool::SmartLasso {
                ui.label("Tolerance");
                ui.add(egui::DragValue::new(&mut self.lasso_state.tolerance).range(0..=255));
            } else if self.tool == Tool::Lasso {
                ui.spacing_mut().slider_width = 65.0;
                ui.add(
                    egui::Slider::new(&mut self.lasso_state.stabilizer, 0.0..=1.0)
                        .show_value(false)
                        .text("Stabilizer"),
                );
            }
            if self.lasso_state.job.is_some() {
                ui.spinner();
            }
        });
        // Apply expensive source-resolution changes once when a gesture ends.
        if !ui.input(|i| i.pointer.any_down()) {
            if let Some(layer) = selected {
                if let LayerKind::Image {
                    clip: Some(clip), ..
                } = &layer.kind
                {
                    if clip.edge == self.lasso_state.edge {
                        return;
                    }
                    if !self.lasso_state.editing {
                        self.checkpoint();
                        self.lasso_state.editing = true;
                    }
                    if let Some(target) = self.story.chapters[self.chapter]
                        .layers
                        .iter_mut()
                        .find(|l| l.id == layer.id)
                    {
                        if let LayerKind::Image { clip: target, .. } = &mut target.kind {
                            *target = Some(Arc::new(ImageClip {
                                selection: clip.selection.clone(),
                                edge: self.lasso_state.edge,
                            }));
                        }
                    }
                    self.changed();
                    if let Some(selection) = &mut self.lasso_state.selection {
                        selection.revision = self.revision;
                    }
                }
            }
        }
        if !ui.input(|i| i.pointer.any_down()) {
            self.lasso_state.editing = false;
        }
    }

    pub(super) fn reset_image_clip(&mut self, id: u64) {
        let Some(index) = self.story.chapters[self.chapter]
            .layers
            .iter()
            .position(|l| l.id == id && !l.locked)
        else {
            return;
        };
        if !matches!(&self.story.chapters[self.chapter].layers[index].kind,LayerKind::Image{mask,clip,source_rect,..} if !mask.is_empty() || clip.is_some() || source_rect.is_some())
        {
            return;
        }
        self.checkpoint();
        let layer = &mut self.story.chapters[self.chapter].layers[index];
        if let LayerKind::Image {
            mask,
            clip,
            source_rect,
            ..
        } = &mut layer.kind
        {
            mask.clear();
            *clip = None;
            if let Some([cx, cy, cw, ch]) = source_rect.take() {
                layer.x -= cx / cw * layer.width;
                layer.y -= cy / ch * layer.height;
                layer.width /= cw;
                layer.height /= ch;
            }
        }
        self.lasso_state.cancel();
        self.changed();
    }

    pub(super) fn smart_select(&mut self, ctx: &egui::Context, p: [f32; 2]) {
        // Retain the selected image even when the click is outside its current
        // mask, allowing Shift to restore areas that were previously clipped.
        let contains = |l: &Layer| {
            !l.locked
                && l.visible
                && matches!(l.kind, LayerKind::Image { .. })
                && p[0] >= l.x
                && p[0] < l.x + l.width
                && p[1] >= l.y
                && p[1] < l.y + l.height
        };
        let layers = &self.story.chapters[self.chapter].layers;
        let target = layers
            .iter()
            .find(|l| Some(l.id) == self.selected && contains(l))
            .or_else(|| layers.iter().rev().find(|l| contains(l)))
            .cloned();
        let Some(layer) = target else {
            self.status = "Click an image to select a connected area".into();
            return;
        };
        let LayerKind::Image { asset_id, .. } = layer.kind else {
            return;
        };
        let Some(asset) = self.story.assets.iter().find(|a| a.id == asset_id) else {
            return;
        };
        let uv = source_uv(
            &layer,
            [
                (p[0] - layer.x) / layer.width,
                (p[1] - layer.y) / layer.height,
            ],
        );
        let seed = [
            (uv[0] * asset.width as f32) as u32,
            (uv[1] * asset.height as f32) as u32,
        ];
        self.selected = Some(layer.id);
        self.selected_asset = None;
        let mode = selection_mode(ctx.input(|i| i.modifiers));
        self.start_selection(ctx, SelectionInput::Smart(seed), mode);
    }

    pub(super) fn finish_lasso(&mut self, ctx: &egui::Context) {
        let Some(layer) = self.story.chapters[self.chapter].layers.iter().find(|l| {
            Some(l.id) == self.selected
                && l.visible
                && !l.locked
                && matches!(l.kind, LayerKind::Image { .. })
        }) else {
            return;
        };
        let polygon: Vec<_> = self
            .lasso
            .iter()
            .map(|p| {
                source_uv(
                    layer,
                    [
                        (p[0] - layer.x) / layer.width,
                        (p[1] - layer.y) / layer.height,
                    ],
                )
            })
            .collect();
        let polygon = compositor::clip_polygon_to_unit_square(&polygon);
        if polygon.len() < 3 {
            self.status = "The selection is outside the image".into();
            return;
        }
        self.start_selection(ctx, SelectionInput::Polygon(polygon), self.lasso_state.mode);
    }

    pub(super) fn start_selection(
        &mut self,
        ctx: &egui::Context,
        input: SelectionInput,
        mode: SelectionMode,
    ) {
        if self.lasso_state.job.is_some() {
            self.status = "Finishing selection…".into();
            return;
        }
        let Some(layer) = self.story.chapters[self.chapter]
            .layers
            .iter()
            .find(|l| Some(l.id) == self.selected && l.visible && !l.locked)
            .cloned()
        else {
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
        let Some(source) = self.assets.get(&asset_id).cloned() else {
            return;
        };
        let Some(asset) = self.story.assets.iter().find(|a| a.id == asset_id) else {
            return;
        };
        let (width, height) = (asset.width, asset.height);
        let previous = self.active_selection().map(|s| s.mask.clone());
        let edge = self.lasso_state.edge;
        let tolerance = self.lasso_state.tolerance;
        let (sender, receiver) = mpsc::sync_channel(1);
        let wake = ctx.clone();
        std::thread::spawn(move || {
            let result = (|| -> SelectionResult {
                let incoming = match input {
                    SelectionInput::Polygon(points) => {
                        SelectionMask::polygon(width, height, &points)?
                    }
                    SelectionInput::Smart(seed) => {
                        SelectionMask::smart(&compositor::decode_image(&source)?, seed, tolerance)?
                    }
                };
                let mut selection = if mode == SelectionMode::Replace {
                    incoming.clone()
                } else if let Some(previous) = previous {
                    (*previous).clone()
                } else if let Some(clip) = clip {
                    (*clip.selection).clone()
                } else if !mask.is_empty() || mode == SelectionMode::Subtract {
                    SelectionMask::polygon(width, height, &mask)?
                } else {
                    SelectionMask::empty(width, height)?
                };
                selection.combine(&incoming, mode)?;
                if let Some([x, y, w, h]) = source_rect {
                    let window = SelectionMask::polygon(
                        width,
                        height,
                        &[[x, y], [x + w, y], [x + w, y + h], [x, y + h]],
                    )?;
                    selection.intersect(&window)?;
                }
                let outline = selection_outline(&selection);
                Ok((
                    Arc::new(ImageClip {
                        selection: Arc::new(selection),
                        edge,
                    }),
                    outline,
                ))
            })();
            let _ = sender.send(result);
            wake.request_repaint();
        });
        self.lasso_state.job = Some(SelectionJob {
            receiver,
            epoch: self.document_epoch,
            revision: self.revision,
            chapter: self.story.chapters[self.chapter].id,
            layer: layer.id,
            apply_crop: self.tool == Tool::Crop,
        });
        self.status = "Selecting…".into();
    }

    pub(super) fn poll_selection(&mut self, _ctx: &egui::Context) {
        let Some(job) = &self.lasso_state.job else {
            return;
        };
        let result = match job.receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(_) => Err(anyhow::anyhow!("Selection worker stopped")),
        };
        let job = self.lasso_state.job.take().unwrap();
        if job.epoch != self.document_epoch || job.revision != self.revision {
            self.status = "Selection cancelled because the story changed".into();
            return;
        }
        match result {
            Ok((clip, outline)) => {
                if !self.story.chapters.iter().any(|c| {
                    c.id == job.chapter && c.layers.iter().any(|l| l.id == job.layer && !l.locked)
                }) {
                    return;
                }
                self.lasso_state.selection = Some(ActiveSelection {
                    epoch: self.document_epoch,
                    revision: self.revision,
                    chapter: job.chapter,
                    layer: job.layer,
                    mask: clip.selection.clone(),
                    outline,
                });
                if job.apply_crop {
                    self.apply_selection_crop();
                }
                self.status = "Selection ready".into();
            }
            Err(error) => self.error = Some(format!("Could not select image: {error:#}")),
        }
    }
}

pub(super) fn selection_outline(mask: &SelectionMask) -> Vec<[[f32; 2]; 2]> {
    let step = (mask.width.max(mask.height) as f32 / 512.0).max(1.0);
    let w = (mask.width as f32 / step).ceil() as u32;
    let h = (mask.height as f32 / step).ceil() as u32;
    let inside = |x: u32, y: u32| {
        x < w && y < h && mask.contains_uv((x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32)
    };
    let mut result = Vec::new();
    for y in 0..h {
        for x in 0..w {
            if !inside(x, y) {
                continue;
            }
            let x0 = x as f32 / w as f32;
            let x1 = (x + 1) as f32 / w as f32;
            let y0 = y as f32 / h as f32;
            let y1 = (y + 1) as f32 / h as f32;
            for (neighbor, line) in [
                (inside(x, y.wrapping_sub(1)), [[x0, y0], [x1, y0]]),
                (inside(x, y + 1), [[x0, y1], [x1, y1]]),
                (inside(x.wrapping_sub(1), y), [[x0, y0], [x0, y1]]),
                (inside(x + 1, y), [[x1, y0], [x1, y1]]),
            ] {
                if !neighbor {
                    result.push(line);
                    if result.len() >= 8192 {
                        return result;
                    }
                }
            }
        }
    }
    result
}

pub(super) fn source_uv(layer: &Layer, point: [f32; 2]) -> [f32; 2] {
    if let LayerKind::Image {
        source_rect: Some([x, y, w, h]),
        ..
    } = layer.kind
    {
        [x + point[0] * w, y + point[1] * h]
    } else {
        point
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (egui::Context, Studio, u64) {
        let ctx = egui::Context::default();
        let mut studio = Studio::from_context(&ctx);
        let id = studio.story.alloc_id();
        let image = image::RgbaImage::from_pixel(96, 64, image::Rgba([40, 70, 90, 255]));
        let mut bytes = std::io::Cursor::new(Vec::new());
        image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
        studio.assets.insert(id, Arc::new(bytes.into_inner()));
        studio.story.assets.push(Asset {
            id,
            name: "Original".into(),
            kind: AssetKind::Image,
            notes: String::new(),
            prompt: String::new(),
            width: 96,
            height: 64,
            character: CharacterProfile::default(),
        });
        let layer = studio.story.alloc_id();
        studio.story.chapters[0].layers.push(Layer {
            id: layer,
            name: "Image".into(),
            visible: true,
            locked: false,
            opacity: 1.0,
            x: 0.0,
            y: 0.0,
            width: 96.0,
            height: 64.0,
            kind: LayerKind::Image {
                source_rect: None,
                asset_id: id,
                mask: vec![],
                clip: None,
            },
        });
        studio.selected = Some(layer);
        (ctx, studio, id)
    }
    fn finish(ctx: &egui::Context, studio: &mut Studio) {
        let deadline = Instant::now() + std::time::Duration::from_secs(3);
        while studio.lasso_state.job.is_some() {
            assert!(Instant::now() < deadline);
            studio.poll_selection(ctx);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }
    #[test]
    fn both_tools_share_modifiers_and_selections_preserve_originals_and_undo() {
        assert_eq!(selection_mode(egui::Modifiers::SHIFT), SelectionMode::Add);
        assert_eq!(
            selection_mode(egui::Modifiers::ALT),
            SelectionMode::Subtract
        );
        assert_eq!(
            selection_mode(egui::Modifiers::NONE),
            SelectionMode::Replace
        );
        let (ctx, mut studio, id) = fixture();
        let original = studio.story.clone();
        let bytes = studio.assets[&id].clone();
        studio.lasso_state.edge.pattern = EdgePattern::Dotted;
        studio.tool = Tool::Crop;
        studio.start_selection(
            &ctx,
            SelectionInput::Polygon(vec![[0.0, 0.0], [0.5, 0.0], [0.5, 1.0], [0.0, 1.0]]),
            SelectionMode::Replace,
        );
        finish(&ctx, &mut studio);
        let clipped = studio.story.clone();
        assert_ne!(clipped, original);
        assert_eq!(studio.assets[&id], bytes);
        assert_eq!(studio.undo.len(), 1);
        studio.undo();
        assert_eq!(studio.story, original);
        studio.redo();
        assert_eq!(studio.story, clipped);
        studio.selected = studio.story.chapters[0].layers.first().map(|l| l.id);
        studio.start_selection(&ctx, SelectionInput::Smart([80, 20]), SelectionMode::Add);
        finish(&ctx, &mut studio);
        let LayerKind::Image {
            clip: Some(clip), ..
        } = &studio.story.chapters[0].layers[0].kind
        else {
            panic!()
        };
        assert!(clip.selection.contains(80, 20));
        assert!(clip.selection.contains(0, 0));
        let path =
            std::env::temp_dir().join(format!("scroll-clip-roundtrip-{}.scro", std::process::id()));
        storage::save_story(&path, &studio.story, &studio.assets).unwrap();
        let (loaded, assets) = storage::load_story(&path).unwrap();
        std::fs::remove_file(path).unwrap();
        assert_eq!(loaded, studio.story);
        assert_eq!(assets[&id], bytes);
        let full = compositor::render_region(&loaded.chapters[0], &assets, 0, 128, 800).unwrap();
        let a = compositor::render_region(&loaded.chapters[0], &assets, 0, 64, 800).unwrap();
        let b = compositor::render_region(&loaded.chapters[0], &assets, 64, 64, 800).unwrap();
        assert_eq!(
            full.as_raw(),
            &[a.as_raw().as_slice(), b.as_raw().as_slice()].concat()
        );
    }
    #[test]
    fn editing_or_loading_while_selection_runs_discards_stale_work() {
        let (ctx, mut studio, _) = fixture();
        let before = studio.story.clone();
        studio.start_selection(&ctx, SelectionInput::Smart([1, 1]), SelectionMode::Replace);
        studio.changed();
        finish(&ctx, &mut studio);
        assert_eq!(studio.story, before);
        assert!(studio.undo.is_empty());
        studio.start_selection(&ctx, SelectionInput::Smart([1, 1]), SelectionMode::Replace);
        studio.document_epoch += 1;
        finish(&ctx, &mut studio);
        assert_eq!(studio.story, before);
    }
    #[test]
    fn separated_parts_reuse_bytes_and_keep_crop_hit_testing_snapping_and_undo() {
        let (ctx, mut studio, asset) = fixture();
        let original = studio.story.clone();
        let bytes = studio.assets[&asset].clone();
        studio.tool = Tool::Select;
        studio.start_selection(
            &ctx,
            SelectionInput::Polygon(vec![[0.5, 0.25], [0.9, 0.25], [0.9, 0.75], [0.5, 0.75]]),
            SelectionMode::Replace,
        );
        finish(&ctx, &mut studio);
        assert_eq!(studio.story, original);
        studio.separate_selection(false);
        assert_eq!(studio.story.assets.len(), 1);
        assert_eq!(studio.assets[&asset], bytes);
        let part = studio.story.chapters[0].layers.last().unwrap();
        let uv = source_uv(part, [0.5, 0.5]);
        let LayerKind::Image {
            clip: Some(clip), ..
        } = &part.kind
        else {
            panic!()
        };
        assert!(clip.selection.contains_uv(uv[0], uv[1]));
        let bounds = scroll_studio::snapping::layer_bounds(part).unwrap();
        assert!((bounds.x - part.x).abs() < 0.01);
        assert!((bounds.width - part.width).abs() < 0.01);
        let separated = studio.story.clone();
        studio.undo();
        assert_eq!(studio.story, original);
        studio.redo();
        assert_eq!(studio.story, separated);
        studio.selected = studio.story.chapters[0].layers.last().map(|l| l.id);
        studio.reset_image_clip(studio.selected.unwrap());
        let restored = studio.story.chapters[0].layers.last().unwrap();
        assert_eq!(
            [restored.x, restored.y, restored.width, restored.height],
            [0.0, 0.0, 96.0, 64.0]
        );
    }
}
