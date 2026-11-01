use super::*;
use scroll_studio::clipping::{EdgePattern, EdgeSettings, ImageClip, SelectionMask, SelectionMode};

#[derive(Clone, Copy, PartialEq)]
pub(super) enum SnippetKind {
    Background,
    Character,
}
pub(super) struct SnippetPicker {
    kind: SnippetKind,
    chapter: u64,
    target: [f32; 4],
    asset: Option<u64>,
    folder: Option<u64>,
    crop: [f32; 4],
    drag: Vec<[f32; 2]>,
    freehand: bool,
    tolerance: u8,
    edge: EdgeSettings,
    selection: Option<Arc<SelectionMask>>,
    outline: Vec<[[f32; 2]; 2]>,
    job: Option<mpsc::Receiver<anyhow::Result<Arc<SelectionMask>>>>,
    error: Option<String>,
    discard_job: bool,
}
impl Studio {
    pub(super) fn begin_snippet(&mut self, ctx: &egui::Context, kind: SnippetKind) {
        let chapter = &self.story.chapters[self.chapter];
        let target = chapter
            .panels
            .iter()
            .find(|p| Some(p.id) == self.active_panel)
            .map(|p| [0.0, p.y, chapter.width as f32, p.height])
            .unwrap_or_else(|| {
                let y = self
                    .scroll_y
                    .clamp(0.0, chapter.height.saturating_sub(1) as f32);
                let height = ((ctx.screen_rect().height() - 180.0) / self.zoom.max(0.1))
                    .max(100.0)
                    .min(chapter.height as f32 - y);
                [0.0, y, chapter.width as f32, height]
            });
        self.snippet = Some(SnippetPicker {
            kind,
            chapter: chapter.id,
            target,
            asset: None,
            folder: None,
            crop: [0.0, 0.0, 1.0, 1.0],
            drag: vec![],
            freehand: false,
            tolerance: 36,
            edge: EdgeSettings::default(),
            selection: None,
            outline: vec![],
            job: None,
            error: None,
            discard_job: false,
        });
    }
    pub(super) fn snippet_ui(&mut self, ctx: &egui::Context) {
        let Some(mut picker) = self.snippet.take() else {
            return;
        };
        if !ctx.wants_keyboard_input()
            && ctx.input_mut(|i| {
                i.consume_shortcut(&egui::KeyboardShortcut::new(
                    egui::Modifiers::COMMAND,
                    egui::Key::D,
                ))
            })
        {
            picker.selection = None;
            picker.outline.clear();
            picker.drag.clear();
            picker.discard_job = true;
            picker.error = None;
        }
        if let Some(receiver) = &picker.job {
            match receiver.try_recv() {
                Ok(result) => {
                    picker.job = None;
                    match result {
                        _ if picker.discard_job => {}
                        Ok(mask) => {
                            picker.outline = super::lasso_ui::selection_outline(&mask);
                            picker.selection = Some(mask);
                            picker.error = None;
                        }
                        Err(error) => picker.error = Some(error.to_string()),
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    picker.job = None;
                    picker.error = Some("Selection worker stopped".into());
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        let mut close = false;
        let mut place = false;
        let modal = egui::Modal::new(egui::Id::new("snippet_picker")).show(ctx, |ui| {
            ui.set_width((ctx.screen_rect().width() - 90.0).clamp(360.0, 900.0));
            let height = (ctx.screen_rect().height() - 190.0).clamp(240.0, 540.0);
            ui.horizontal(|ui| {
                if picker.asset.is_some() && motion::button(ui, "‹ References").clicked() {
                    picker.asset = None;
                    picker.selection = None;
                    picker.outline.clear();
                    picker.job = None;
                }
                ui.heading(if picker.kind == SnippetKind::Background {
                    "Background snippet"
                } else {
                    "Character snippet"
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if motion::button(ui, "Cancel").clicked() {
                        close = true;
                    }
                });
            });
            ui.add_space(8.0);
            if let Some(asset_id) = picker.asset {
                let Some(asset) = self.story.assets.iter().find(|a| a.id == asset_id).cloned()
                else {
                    close = true;
                    return;
                };
                if picker.kind == SnippetKind::Character {
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut picker.freehand, false, "Smart");
                        ui.selectable_value(&mut picker.freehand, true, "Lasso");
                        if !picker.freehand {
                            ui.label("Size");
                            ui.add(egui::DragValue::new(&mut self.lasso_state.smart_width).range(1.0..=256.0).suffix(" px"))
                                .on_hover_text("Selection brush diameter on screen; click or drag. Shift adds, Alt removes.");
                            ui.label("Tolerance");
                            ui.add(egui::DragValue::new(&mut picker.tolerance).range(0..=255));
                        }
                        egui::ComboBox::from_id_salt("snippet_edge")
                            .width(80.0)
                            .selected_text(picker.edge.pattern.label())
                            .show_ui(ui, |ui| {
                                for pattern in EdgePattern::ALL {
                                    ui.selectable_value(
                                        &mut picker.edge.pattern,
                                        pattern,
                                        pattern.label(),
                                    );
                                }
                            });
                        ui.label("Width");
                        ui.add(
                            egui::DragValue::new(&mut picker.edge.width)
                                .range(0..=512)
                                .suffix(" px"),
                        );
                        motion::menu_button(ui, "Edge…", |ui| {
                            ui.horizontal(|ui| {
                                for contour in scroll_studio::clipping::Contour::ALL {
                                    ui.selectable_value(
                                        &mut picker.edge.contour,
                                        contour,
                                        contour.label(),
                                    );
                                }
                            });
                            if picker.edge.pattern == EdgePattern::Dotted {
                                ui.add(
                                    egui::Slider::new(&mut picker.edge.dot_scale, 2..=128)
                                        .text("Dot scale"),
                                );
                            }
                            ui.horizontal(|ui| {
                                ui.label("Edge color");
                                ui.color_edit_button_srgba_unmultiplied(
                                    &mut picker.edge.background,
                                );
                            });
                            let mut opacity = picker.edge.background[3] as f32 / 255.0 * 100.0;
                            if ui
                                .add(
                                    egui::Slider::new(&mut opacity, 0.0..=100.0)
                                        .text("Opacity")
                                        .suffix("%"),
                                )
                                .changed()
                            {
                                picker.edge.background[3] = (opacity * 2.55).round() as u8;
                            }
                            ui.add(
                                egui::Slider::new(&mut self.lasso_state.stabilizer, 0.0..=4.0)
                                    .text("Stabilizer"),
                            );
                        });
                        if picker.job.is_some() {
                            ui.spinner();
                        }
                    });
                }
                let (area, response) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), height - 72.0),
                    egui::Sense::click_and_drag(),
                );
                let size = egui::vec2(asset.width as f32, asset.height as f32);
                let scale = ((area.width() - 12.0) / size.x).min((area.height() - 12.0) / size.y);
                let rect = egui::Rect::from_center_size(area.center(), size * scale);
                self.paint_character_reference(ui, ctx, asset_id, area, false);
                let local = |p: egui::Pos2| {
                    [
                        ((p.x - rect.left()) / rect.width()).clamp(0.0, 1.0),
                        ((p.y - rect.top()) / rect.height()).clamp(0.0, 1.0),
                    ]
                };
                let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
                if picker.kind == SnippetKind::Background {
                    if response.drag_started() {
                        if let Some(p) = ctx.input(|i| i.pointer.press_origin()) {
                            picker.drag = vec![local(p)];
                        }
                    }
                    if response.dragged() || response.drag_stopped() {
                        if let (Some(start), Some(end)) = (
                            picker.drag.first().copied(),
                            response.interact_pointer_pos(),
                        ) {
                            let end = local(end);
                            let ratio = picker.target[2] / picker.target[3] * asset.height as f32
                                / asset.width as f32;
                            let mut w = (end[0] - start[0])
                                .abs()
                                .max((end[1] - start[1]).abs() * ratio)
                                .max(0.01);
                            let mut h = w / ratio;
                            let shrink = (1.0 / w).min(1.0 / h).min(1.0);
                            w *= shrink;
                            h *= shrink;
                            let x = if end[0] < start[0] {
                                start[0] - w
                            } else {
                                start[0]
                            };
                            let y = if end[1] < start[1] {
                                start[1] - h
                            } else {
                                start[1]
                            };
                            picker.crop = [x.clamp(0.0, 1.0 - w), y.clamp(0.0, 1.0 - h), w, h];
                        }
                    }
                    let [x, y, w, h] = picker.crop;
                    let crop = egui::Rect::from_min_size(
                        rect.min + egui::vec2(x, y) * rect.size(),
                        egui::vec2(w, h) * rect.size(),
                    );
                    for outside in [
                        egui::Rect::from_min_max(rect.min, egui::pos2(rect.right(), crop.top())),
                        egui::Rect::from_min_max(egui::pos2(rect.left(), crop.bottom()), rect.max),
                        egui::Rect::from_min_max(
                            egui::pos2(rect.left(), crop.top()),
                            crop.left_bottom(),
                        ),
                        egui::Rect::from_min_max(
                            crop.right_top(),
                            egui::pos2(rect.right(), crop.bottom()),
                        ),
                    ] {
                        painter.rect_filled(outside, 0.0, Color32::from_black_alpha(150));
                    }
                    painter.rect_stroke(
                        crop,
                        0.0,
                        egui::Stroke::new(2.0_f32, ACCENT),
                        egui::StrokeKind::Inside,
                    );
                } else {
                    if picker.job.is_none() {
                        let mode = super::lasso_ui::selection_mode(ctx.input(|i| i.modifiers));
                        if !picker.freehand {
                            let radius = self.lasso_state.smart_width * 0.5;
                            if let Some(p) = ctx.pointer_hover_pos().filter(|p| rect.contains(*p) && response.hovered()) {
                                painter.circle_stroke(p, radius, egui::Stroke::new(2.5_f32, Color32::from_black_alpha(180)));
                                painter.circle_stroke(p, radius, egui::Stroke::new(1.0_f32, ACCENT));
                            }
                            if response.drag_started() {
                                if let Some(p) = ctx.input(|i| i.pointer.press_origin()).filter(|p| rect.contains(*p)) {
                                    picker.drag = vec![local(p)];
                                }
                            }
                            if response.dragged() && !picker.drag.is_empty() {
                                if let Some(p) = response.interact_pointer_pos() {
                                    let uv = local(p);
                                    if picker.drag.len() < 1999 && picker.drag.last().is_none_or(|last| {
                                        ((uv[0]-last[0])*rect.width()).hypot((uv[1]-last[1])*rect.height()) >= 2.0
                                    }) { picker.drag.push(uv); }
                                }
                            }
                            let points = if response.clicked() {
                                response.interact_pointer_pos().filter(|p| rect.contains(*p)).map(|p| vec![local(p)])
                            } else if response.drag_stopped() && !picker.drag.is_empty() {
                                if let Some(p) = response.interact_pointer_pos() { picker.drag.push(local(p)); }
                                Some(std::mem::take(&mut picker.drag))
                            } else { None };
                            if let Some(points) = points {
                                let points = points.into_iter().map(|uv| [uv[0]*asset.width as f32, uv[1]*asset.height as f32]).collect();
                                self.snippet_select(ctx, &mut picker, asset_id,
                                    SnippetSelection::SmartBrush(points, [radius/scale.max(0.0001); 2]), mode);
                            }
                        }
                        if picker.freehand {
                            if response.drag_started() {
                                if let Some(p) = ctx.input(|i| i.pointer.press_origin()) {
                                    picker.drag = vec![local(p)];
                                }
                            }
                            if response.dragged() {
                                if let Some(p) = response.interact_pointer_pos() {
                                    if picker.drag.len() < 2000 {
                                        let point = local(p);
                                        let point = picker.drag.last().map_or(point, |previous| {
                                            scroll_studio::clipping::stabilize(
                                                *previous,
                                                point,
                                                self.lasso_state.stabilizer,
                                                rect.width().max(1.0),
                                            )
                                        });
                                        picker.drag.push(point);
                                    }
                                }
                            }
                            if response.drag_stopped() && picker.drag.len() >= 3 {
                                let points = picker.drag.clone();
                                self.snippet_select(
                                    ctx,
                                    &mut picker,
                                    asset_id,
                                    SnippetSelection::Polygon(points),
                                    mode,
                                );
                                picker.drag.clear();
                            }
                        }
                    }
                    for line in &picker.outline {
                        painter.line_segment(
                            line.map(|p| rect.min + egui::vec2(p[0], p[1]) * rect.size()),
                            egui::Stroke::new(1.5_f32, ACCENT),
                        );
                    }
                    if picker.drag.len() > 1 {
                        painter.add(egui::Shape::line(
                            picker
                                .drag
                                .iter()
                                .map(|p| rect.min + egui::vec2(p[0], p[1]) * rect.size())
                                .collect(),
                            egui::Stroke::new(1.5_f32, ACCENT),
                        ));
                    }
                }
                ui.horizontal(|ui| {
                    if let Some(error) = &picker.error {
                        ui.colored_label(Color32::LIGHT_RED, error);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        place = ui
                            .add_enabled(
                                picker.job.is_none()
                                    && (picker.kind == SnippetKind::Background
                                        || picker.selection.as_ref().is_some_and(|s| s.any())),
                                motion::Button::new("Place snippet"),
                            )
                            .clicked();
                    });
                });
            } else {
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("snippet_folder")
                        .selected_text(
                            picker
                                .folder
                                .and_then(|id| self.story.locations.iter().find(|l| l.id == id))
                                .map_or(
                                    if picker.kind == SnippetKind::Character {
                                        "All characters & images"
                                    } else {
                                        "All backgrounds"
                                    },
                                    |l| l.name.as_str(),
                                ),
                        )
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut picker.folder,
                                None,
                                if picker.kind == SnippetKind::Character {
                                    "All characters & images"
                                } else {
                                    "All backgrounds"
                                },
                            );
                            if picker.kind == SnippetKind::Background {
                                for location in &self.story.locations {
                                    ui.selectable_value(
                                        &mut picker.folder,
                                        Some(location.id),
                                        &location.name,
                                    );
                                }
                            }
                        });
                });
                let assets: Vec<_> = self
                    .story
                    .assets
                    .iter()
                    .filter(|a| {
                        self.assets.contains_key(&a.id)
                            && a.width > 0
                            && match picker.kind {
                                SnippetKind::Background => {
                                    a.kind == AssetKind::Background
                                        && picker.folder.is_none_or(|folder| {
                                            self.story.locations.iter().any(|l| {
                                                l.id == folder && l.asset_ids.contains(&a.id)
                                            })
                                        })
                                }
                                SnippetKind::Character => {
                                    matches!(a.kind, AssetKind::Character | AssetKind::Image)
                                }
                            }
                    })
                    .cloned()
                    .collect();
                if assets.is_empty() {
                    ui.centered_and_justified(|ui| {
                        ui.label(if picker.kind == SnippetKind::Background {
                            "Add images to a location first"
                        } else {
                            "Add a character reference or generated image first"
                        });
                    });
                }
                let cols = (ui.available_width() / 180.0).floor().max(1.0) as usize;
                let width = (ui.available_width() - 10.0 * (cols - 1) as f32) / cols as f32;
                egui::ScrollArea::vertical().max_height(height).show_rows(
                    ui,
                    170.0,
                    assets.len().div_ceil(cols),
                    |ui, rows| {
                        for row in rows {
                            ui.horizontal(|ui| {
                                for asset in assets.iter().skip(row * cols).take(cols) {
                                    let (rect, response) = ui.allocate_exact_size(
                                        egui::vec2(width, 158.0),
                                        egui::Sense::click(),
                                    );
                                    self.paint_character_reference(ui, ctx, asset.id, rect, false);
                                    response.clone().on_hover_text(&asset.name);
                                    if response.clicked() {
                                        picker.asset = Some(asset.id);
                                        picker.selection = None;
                                        picker.outline.clear();
                                        let ratio = picker.target[2] / picker.target[3]
                                            * asset.height as f32
                                            / asset.width as f32;
                                        picker.crop = if ratio >= 1.0 {
                                            [0.0, (1.0 - 1.0 / ratio) * 0.5, 1.0, 1.0 / ratio]
                                        } else {
                                            [(1.0 - ratio) * 0.5, 0.0, ratio, 1.0]
                                        };
                                    }
                                }
                            });
                        }
                    },
                );
            }
        });
        if place {
            if let Err(error) = self.place_snippet(&picker) {
                picker.error = Some(error.to_string());
            } else {
                return;
            }
        }
        if !close && !modal.should_close() {
            self.snippet = Some(picker);
        }
    }
    fn snippet_select(
        &self,
        ctx: &egui::Context,
        picker: &mut SnippetPicker,
        asset: u64,
        input: SnippetSelection,
        mode: SelectionMode,
    ) {
        let Some(source) = self.assets.get(&asset).cloned() else {
            return;
        };
        let previous = picker.selection.clone();
        let tolerance = picker.tolerance;
        let wake = ctx.clone();
        let (sender, receiver) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<Arc<SelectionMask>> {
                let image = compositor::decode_image(&source)?;
                let incoming = match input {
                    SnippetSelection::SmartBrush(points, radii) => {
                        if previous.is_none() && mode == SelectionMode::Replace {
                            SelectionMask::subject_brush(&image, &points, radii, tolerance)?
                        } else {
                            SelectionMask::smart_brush(&image, &points, radii, tolerance)?
                        }
                    }
                    SnippetSelection::Polygon(points) => {
                        SelectionMask::polygon(image.width(), image.height(), &points)?
                    }
                };
                let mut mask = if mode == SelectionMode::Replace {
                    incoming.clone()
                } else if let Some(previous) = previous {
                    (*previous).clone()
                } else {
                    SelectionMask::empty(image.width(), image.height())?
                };
                mask.combine(&incoming, mode)?;
                Ok(Arc::new(mask))
            })();
            let _ = sender.send(result);
            wake.request_repaint();
        });
        picker.discard_job = false;
        picker.job = Some(receiver);
    }
    fn place_snippet(&mut self, picker: &SnippetPicker) -> anyhow::Result<()> {
        let asset_id = picker
            .asset
            .ok_or_else(|| anyhow::anyhow!("Choose an image"))?;
        let asset = self
            .story
            .assets
            .iter()
            .find(|a| a.id == asset_id)
            .ok_or_else(|| anyhow::anyhow!("Image no longer exists"))?
            .clone();
        let chapter = self
            .story
            .chapters
            .iter()
            .position(|c| c.id == picker.chapter)
            .ok_or_else(|| anyhow::anyhow!("Chapter no longer exists"))?;
        let (crop, clip) = if picker.kind == SnippetKind::Character {
            let selection = picker
                .selection
                .clone()
                .ok_or_else(|| anyhow::anyhow!("Select the character"))?;
            let [x, y, w, h] = selection
                .edge_bounds(picker.edge)
                .ok_or_else(|| anyhow::anyhow!("No selected pixels"))?;
            (
                [
                    x as f32 / asset.width as f32,
                    y as f32 / asset.height as f32,
                    w as f32 / asset.width as f32,
                    h as f32 / asset.height as f32,
                ],
                Some(Arc::new(ImageClip {
                    selection,
                    edge: picker.edge,
                })),
            )
        } else {
            (picker.crop, None)
        };
        let [mut x, mut y, mut width, mut height] = picker.target;
        if picker.kind == SnippetKind::Character {
            let ratio = asset.width as f32 * crop[2] / (asset.height as f32 * crop[3]);
            let h = height.min(width * 0.7 / ratio);
            let w = h * ratio;
            x += (width - w) * 0.5;
            y += (height - h) * 0.5;
            width = w;
            height = h;
        }
        self.checkpoint();
        let id = self.story.alloc_id();
        let layer = Layer {
            id,
            name: format!("{} snippet", asset.name),
            visible: true,
            locked: false,
            opacity: 1.0,
            x,
            y,
            width,
            height,
            kind: LayerKind::Image {
                asset_id,
                mask: vec![],
                clip,
                source_rect: Some(crop),
            },
        };
        let layers = &mut self.story.chapters[chapter].layers;
        let insertion = if picker.kind == SnippetKind::Character {
            let after_background=layers.iter().rposition(|l|matches!(&l.kind,LayerKind::Image{asset_id,..} if self.story.assets.iter().any(|a|a.id==*asset_id && a.kind==AssetKind::Background))).map_or(0,|i|i+1);
            layers
                .iter()
                .enumerate()
                .skip(after_background)
                .find(|(_, l)| matches!(l.kind, LayerKind::Text { .. }))
                .map_or(layers.len(), |(i, _)| i)
        } else {
            layers
                .iter()
                .position(|l| matches!(l.kind, LayerKind::Text { .. }))
                .unwrap_or(layers.len())
        };
        layers.insert(insertion, layer);
        self.chapter = chapter;
        self.tab = LibraryTab::Chapters;
        self.selected = Some(id);
        self.selected_asset = None;
        self.tool = Tool::Move;
        self.changed();
        self.status = "Snippet placed; original image reused".into();
        Ok(())
    }
}
enum SnippetSelection {
    SmartBrush(Vec<[f32; 2]>, [f32; 2]),
    Polygon(Vec<[f32; 2]>),
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snippets_are_metadata_only_and_round_trip_with_one_copy_of_source() {
        let ctx = egui::Context::default();
        let mut studio = Studio::from_context(&ctx);
        studio.story = Story::with_canvas("Snippets", 800, 1600);
        studio.assets.clear();
        let id = studio.story.alloc_id();
        let mut image = image::RgbaImage::new(64, 96);
        for y in 10..85 {
            for x in 15..50 {
                image.put_pixel(x, y, image::Rgba([30, 100, 140, 255]));
            }
        }
        let mut encoded = std::io::Cursor::new(Vec::new());
        image
            .write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();
        let bytes = Arc::new(encoded.into_inner());
        studio.assets.insert(id, bytes.clone());
        studio.story.assets.push(Asset {
            id,
            name: "Shared source".into(),
            kind: AssetKind::Background,
            width: 64,
            height: 96,
            notes: String::new(),
            prompt: String::new(),
            character: CharacterProfile::default(),
        });
        let original = studio.story.clone();
        studio.begin_snippet(&ctx, SnippetKind::Background);
        let mut picker = studio.snippet.take().unwrap();
        picker.asset = Some(id);
        picker.target = [20.0, 30.0, 400.0, 600.0];
        picker.crop = [0.25, 0.25, 0.5, 0.5];
        studio.place_snippet(&picker).unwrap();
        picker.kind = SnippetKind::Character;
        picker.selection = Some(Arc::new(
            SelectionMask::subject(&image, [30, 40], 20).unwrap(),
        ));
        picker.edge.pattern = EdgePattern::Fade;
        picker.edge.contour = scroll_studio::clipping::Contour::Outside;
        picker.edge.width = 8;
        studio.place_snippet(&picker).unwrap();
        assert_eq!(studio.story.assets.len(), 1);
        assert_eq!(studio.assets.len(), 1);
        assert!(Arc::ptr_eq(&studio.assets[&id], &bytes));
        let path =
            std::env::temp_dir().join(format!("scroll-snippet-test-{}.scro", std::process::id()));
        storage::save_story(&path, &studio.story, &studio.assets).unwrap();
        let (loaded, assets) = storage::load_story(&path).unwrap();
        assert_eq!(loaded, studio.story);
        assert_eq!(assets[&id], bytes);
        let archive = zip::ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        assert_eq!(archive.len(), 2);
        drop(archive);
        std::fs::remove_file(path).unwrap();
        studio.undo();
        studio.undo();
        assert_eq!(studio.story, original);
    }
    #[test]
    fn control_d_in_snippet_clears_selection_and_discards_pending_result() {
        let ctx = egui::Context::default();
        let mut studio = Studio::from_context(&ctx);
        studio.begin_snippet(&ctx, SnippetKind::Character);
        let mask = Arc::new(SelectionMask::polygon(16, 16, &[]).unwrap());
        let (sender, receiver) = mpsc::sync_channel(1);
        {
            let picker = studio.snippet.as_mut().unwrap();
            picker.selection = Some(mask.clone());
            picker.job = Some(receiver);
        }
        sender.send(Ok(mask)).unwrap();
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1100.0, 720.0),
                )),
                events: vec![egui::Event::Key {
                    key: egui::Key::D,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::COMMAND,
                }],
                ..Default::default()
            },
            |ctx| studio.snippet_ui(ctx),
        );
        let picker = studio.snippet.as_ref().unwrap();
        assert!(picker.selection.is_none());
        assert!(picker.outline.is_empty());
        assert!(picker.job.is_none());
    }
}
