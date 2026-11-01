//! A reference stays outside the artwork, using the existing bounded preview cache.
use super::*;
use std::sync::Weak;

pub(super) struct PinnedReference {
    asset: u64,
    character: Option<u64>,
    source: Weak<Vec<u8>>,
    zoom: f32,
    pan: egui::Vec2,
    mirrored: bool,
}

impl PinnedReference {
    fn fit(&mut self) {
        self.zoom = 1.0;
        self.pan = egui::Vec2::ZERO;
    }

    fn image_rect(&mut self, viewport: egui::Rect, size: egui::Vec2) -> egui::Rect {
        let fit = (viewport.width() / size.x).min(viewport.height() / size.y);
        let drawn = size * fit * self.zoom;
        let limit = ((drawn - viewport.size()) * 0.5).max(egui::Vec2::ZERO);
        self.pan = self.pan.clamp(-limit, limit);
        egui::Rect::from_center_size(viewport.center() + self.pan, drawn)
    }

    fn zoom_at(&mut self, viewport: egui::Rect, size: egui::Vec2, point: egui::Pos2, factor: f32) {
        let previous = self.image_rect(viewport, size);
        let zoom = (self.zoom * factor).clamp(1.0, 8.0);
        self.pan = point - viewport.center() - (point - previous.center()) * (zoom / self.zoom);
        self.zoom = zoom;
        self.image_rect(viewport, size);
    }
}

impl Studio {
    pub(super) fn can_pin_reference(&self, asset: u64) -> bool {
        self.assets.contains_key(&asset)
            && self
                .story
                .assets
                .iter()
                .any(|item| item.id == asset && item.width > 0 && item.height > 0)
    }

    pub(super) fn pin_reference(
        &mut self,
        ctx: &egui::Context,
        asset: u64,
        character: Option<u64>,
    ) {
        if !self.can_pin_reference(asset) {
            return;
        }
        let character = character.filter(|id| {
            self.story.assets.iter().any(|item| {
                item.id == *id
                    && item.kind == AssetKind::Character
                    && scroll_studio::characters::reference_ids(item).contains(&asset)
            })
        });
        self.pinned_reference = Some(PinnedReference {
            asset,
            character,
            source: Arc::downgrade(&self.assets[&asset]),
            zoom: 1.0,
            pan: egui::Vec2::ZERO,
            mirrored: false,
        });
        // A pin from a menu appears on the next frame; no story or history change.
        ctx.request_repaint();
    }

    fn reference_ids(&self, reference: &PinnedReference) -> Vec<u64> {
        if let Some(character) = reference.character {
            self.story
                .assets
                .iter()
                .find(|asset| asset.id == character && asset.kind == AssetKind::Character)
                .map(|asset| {
                    scroll_studio::characters::reference_ids(asset)
                        .into_iter()
                        .filter(|id| self.can_pin_reference(*id))
                        .collect()
                })
                .unwrap_or_default()
        } else if self.can_pin_reference(reference.asset) {
            vec![reference.asset]
        } else {
            Vec::new()
        }
    }

    fn reference_source_is_current(&self, reference: &PinnedReference) -> bool {
        self.assets
            .get(&reference.asset)
            .is_some_and(|bytes| reference.source.ptr_eq(&Arc::downgrade(bytes)))
    }

    pub(super) fn reference_contains(&self, point: egui::Pos2) -> bool {
        self.pinned_reference.is_some() && self.reference_rect.contains(point)
    }

    pub(super) fn import_pinned_reference(&mut self, ctx: &egui::Context, path: &std::path::Path) {
        let before = self.story.assets.len();
        self.import_file(path, false, None);
        if let Some(asset) = self.story.assets.get(before) {
            let id = asset.id;
            let owner = self
                .story
                .assets
                .iter()
                .find(|asset| {
                    asset.kind == AssetKind::Character
                        && scroll_studio::characters::reference_ids(asset).contains(&id)
                })
                .map(|asset| asset.id);
            self.pin_reference(ctx, id, owner);
        }
    }

    /// Called before the canvas so scroll and image gestures cannot pass through it.
    pub(super) fn reference_window(&mut self, ctx: &egui::Context) {
        self.reference_rect = egui::Rect::NOTHING;
        let Some(mut reference) = self.pinned_reference.take() else {
            return;
        };
        let ids = self.reference_ids(&reference);
        if ids.is_empty() || !self.reference_source_is_current(&reference) {
            return;
        }
        if !ids.contains(&reference.asset) {
            reference.asset = ids[0];
            reference.source = Arc::downgrade(&self.assets[&reference.asset]);
            reference.fit();
        }
        if self.welcome || self.settings_open || self.reader.is_some() {
            self.pinned_reference = Some(reference);
            return;
        }
        let id = egui::Id::new(("pinned_reference", self.document_epoch));
        let layer = egui::LayerId::new(egui::Order::Middle, id);
        let painter = ctx.layer_painter(layer);
        let background = painter.add(egui::Shape::Noop);
        let mut frame = egui::Frame::window(&ctx.style());
        if self.preferences.glass_enabled {
            frame.fill = Color32::TRANSPARENT;
            frame.stroke.color = Color32::TRANSPARENT;
            // Blur owns the surface. Keep the native margins/resize geometry.
            frame.shadow = egui::Shadow::NONE;
        }
        let mut open = true;
        let mut close = false;
        let mut replacement = None;
        let screen = ctx.screen_rect().shrink(8.0);
        let max_size = (screen.size() - egui::vec2(24.0, 90.0)).min(egui::vec2(640.0, 680.0));
        let default_position =
            egui::pos2(screen.right() - 320.0, (screen.bottom() - 400.0).max(176.0));
        let result = egui::Window::new(RichText::new("Reference").size(14.0).strong())
            .id(id)
            .open(&mut open)
            .frame(frame)
            .fade_in(false)
            .fade_out(false)
            .default_pos(default_position)
            .default_size(egui::vec2(288.0, 330.0))
            .min_size(egui::vec2(220.0, 200.0))
            .max_size(max_size)
            .constrain_to(screen)
            .show(ctx, |ui| {
                let index = ids
                    .iter()
                    .position(|id| *id == reference.asset)
                    .unwrap_or(0);
                ui.horizontal(|ui| {
                    if ids.len() > 1 {
                        if ui
                            .add_enabled(index > 0, motion::Button::new("‹"))
                            .on_hover_text("Previous reference")
                            .clicked()
                        {
                            reference.asset = ids[index - 1];
                            reference.source = Arc::downgrade(&self.assets[&reference.asset]);
                            reference.fit();
                        }
                        ui.label(
                            RichText::new(format!("{}/{}", index + 1, ids.len()))
                                .size(12.0)
                                .color(MUTED),
                        );
                        if ui
                            .add_enabled(index + 1 < ids.len(), motion::Button::new("›"))
                            .on_hover_text("Next reference")
                            .clicked()
                        {
                            reference.asset = ids[index + 1];
                            reference.source = Arc::downgrade(&self.assets[&reference.asset]);
                            reference.fit();
                        }
                    }
                    let fit = motion::button(ui, "Fit");
                    #[cfg(test)]
                    ctx.data_mut(|data| data.insert_temp(id.with("fit"), fit.rect));
                    if fit.clicked() {
                        reference.fit();
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        icons::menu_button(ui, "Reference actions", |ui| {
                            self.reference_actions(ui, &mut reference, &mut close)
                        });
                    });
                });
                let asset = self
                    .story
                    .assets
                    .iter()
                    .find(|asset| asset.id == reference.asset)
                    .unwrap();
                ui.add(
                    egui::Label::new(RichText::new(&asset.name).size(12.0).color(MUTED)).truncate(),
                );
                let size = egui::vec2(asset.width as f32, asset.height as f32);
                let (viewport, response) = ui.allocate_exact_size(
                    egui::vec2(
                        ui.available_width().max(1.0),
                        ui.available_height().clamp(120.0, max_size.y - 60.0),
                    ),
                    egui::Sense::click_and_drag(),
                );
                #[cfg(test)]
                ctx.data_mut(|data| data.insert_temp(id.with("image"), viewport));
                if let Some(asset) = response.dnd_release_payload::<u64>() {
                    replacement = Some(*asset);
                }
                if response.clicked() || response.drag_started() {
                    response.request_focus();
                }
                if response.double_clicked() {
                    reference.fit();
                }
                if response.dragged_by(egui::PointerButton::Primary) {
                    reference.pan += response.drag_delta();
                }
                if response.hovered() {
                    ctx.set_cursor_icon(if response.dragged() {
                        egui::CursorIcon::Grabbing
                    } else {
                        egui::CursorIcon::Grab
                    });
                    let wheel = ctx.input_mut(|input| {
                        let wheel = input.smooth_scroll_delta.y;
                        input.smooth_scroll_delta = egui::Vec2::ZERO;
                        input.raw_scroll_delta = egui::Vec2::ZERO;
                        wheel
                    });
                    if wheel != 0.0 {
                        if let Some(point) = response.hover_pos() {
                            reference.zoom_at(viewport, size, point, (wheel * 0.006).exp());
                        }
                    }
                }
                if response.has_focus()
                    && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::F))
                {
                    reference.fit();
                }
                let image_rect = reference.image_rect(viewport, size);
                let painter = ui
                    .painter()
                    .with_clip_rect(ui.clip_rect().intersect(viewport));
                painter.rect_filled(viewport, 3.0, BG);
                if let Some(image) = self.previews.viewer_image(
                    ctx,
                    reference.asset,
                    &self.assets[&reference.asset],
                    super::previews::ImageViewport {
                        source_size: [asset.width, asset.height],
                        image: image_rect,
                        clip: painter.clip_rect(),
                        mirrored: reference.mirrored,
                    },
                ) {
                    image.paint(&painter);
                } else {
                    icons::draw(
                        &painter,
                        egui::Rect::from_center_size(viewport.center(), egui::vec2(32.0, 32.0)),
                        icons::Icon::Image,
                        MUTED,
                    );
                }
                self.register_context(&response);
                appearance::context_menu(&response, |ui| {
                    self.reference_actions(ui, &mut reference, &mut close)
                });
            });
        if let Some(result) = result {
            self.reference_rect = result.response.rect;
            if self.preferences.glass_enabled {
                painter.set(
                    background,
                    self.glass.shape(
                        self.reference_rect,
                        12.0,
                        appearance::glass_tint(),
                        self.preferences.glass_strength,
                    ),
                );
            }
            // Borders/header absorb the wheel too. The chapter must stay put.
            if ctx.pointer_hover_pos().is_some_and(|point| {
                self.reference_rect.contains(point) && ctx.layer_id_at(point) == Some(layer)
            }) {
                ctx.input_mut(|input| {
                    input.smooth_scroll_delta = egui::Vec2::ZERO;
                    input.raw_scroll_delta = egui::Vec2::ZERO;
                });
            }
            self.register_context(&result.response);
            appearance::context_menu(&result.response, |ui| {
                self.reference_actions(ui, &mut reference, &mut close)
            });
        }
        if open && !close {
            self.pinned_reference = Some(reference);
            if let Some(asset) = replacement {
                self.pin_reference(ctx, asset, None);
            }
        } else {
            self.reference_rect = egui::Rect::NOTHING;
            ctx.request_repaint();
        }
    }

    fn reference_actions(
        &mut self,
        ui: &mut egui::Ui,
        reference: &mut PinnedReference,
        close: &mut bool,
    ) {
        if motion::button(ui, "Fit image").clicked() {
            reference.fit();
            ui.close_menu();
        }
        if ui
            .selectable_label(reference.mirrored, "Flip horizontal")
            .clicked()
        {
            reference.mirrored = !reference.mirrored;
            ui.close_menu();
        }
        if motion::button(ui, "Copy image").clicked() {
            self.copy_asset_image(reference.asset);
            ui.close_menu();
        }
        if let Some(character) = reference.character {
            if motion::button(ui, "Open sheet").clicked() {
                self.tab = LibraryTab::Characters;
                self.select_character(character);
                ui.close_menu();
            }
        }
        ui.separator();
        if motion::button(ui, "Close reference").clicked() {
            *close = true;
            ui.close_menu();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::ImageEncoder;

    fn png() -> Vec<u8> {
        let mut bytes = Vec::new();
        let image = image::RgbaImage::from_pixel(64, 64, image::Rgba([90, 160, 210, 255]));
        image::codecs::png::PngEncoder::new(&mut bytes)
            .write_image(image.as_raw(), 64, 64, image::ExtendedColorType::Rgba8)
            .unwrap();
        bytes
    }

    fn fixture() -> (egui::Context, Studio, Vec<u64>) {
        let ctx = egui::Context::default();
        let mut studio = Studio::from_context(&ctx);
        studio.welcome = false;
        studio.preferences.motion = settings::Motion::Off;
        studio.preferences.glass_enabled = false;
        studio.apply_preferences(&ctx);
        studio.tab = LibraryTab::Characters;
        studio.import_bytes(png(), "Character portrait".into(), false, None);
        studio.import_bytes(png(), "Profile".into(), false, None);
        studio.import_bytes(png(), "Expression".into(), false, None);
        let ids = studio
            .story
            .assets
            .iter()
            .map(|asset| asset.id)
            .collect::<Vec<_>>();
        studio.tab = LibraryTab::Chapters;
        studio.show_script = false;
        studio.selected_asset = None;
        studio.undo.clear();
        studio.redo.clear();
        studio.dirty = false;
        studio.pin_reference(&ctx, ids[1], Some(ids[0]));
        (ctx, studio, ids)
    }

    fn frame(
        ctx: &egui::Context,
        studio: &mut Studio,
        size: egui::Vec2,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        let mut frame = eframe::Frame::_new_kittest();
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                events,
                ..Default::default()
            },
            |ctx| eframe::App::update(studio, ctx, &mut frame),
        )
    }

    fn pointer(point: egui::Pos2, button: egui::PointerButton, pressed: bool) -> Vec<egui::Event> {
        vec![
            egui::Event::PointerMoved(point),
            egui::Event::PointerButton {
                pos: point,
                button,
                pressed,
                modifiers: egui::Modifiers::NONE,
            },
        ]
    }

    fn image_rect(ctx: &egui::Context, studio: &Studio) -> egui::Rect {
        ctx.data(|data| {
            data.get_temp(egui::Id::new(("pinned_reference", studio.document_epoch)).with("image"))
        })
        .unwrap()
    }

    #[test]
    fn reference_gestures_do_not_edit_or_scroll_the_chapter_and_window_is_bounded() {
        for size in [egui::vec2(1100.0, 720.0), egui::vec2(1707.0, 960.0)] {
            let (ctx, mut studio, _) = fixture();
            studio.tool = Tool::Text;
            let before = studio.story.clone();
            for _ in 0..3 {
                frame(&ctx, &mut studio, size, Vec::new());
            }
            let viewport = image_rect(&ctx, &studio);
            assert!(studio.reference_rect.contains_rect(viewport));
            assert!(egui::Rect::from_min_size(egui::Pos2::ZERO, size)
                .contains_rect(studio.reference_rect));
            let scroll = studio.scroll_y;
            let point = viewport.center();
            frame(
                &ctx,
                &mut studio,
                size,
                vec![egui::Event::PointerMoved(point)],
            );
            frame(
                &ctx,
                &mut studio,
                size,
                vec![egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, 150.0),
                    modifiers: egui::Modifiers::NONE,
                }],
            );
            assert!(studio.pinned_reference.as_ref().unwrap().zoom > 1.0);
            frame(
                &ctx,
                &mut studio,
                size,
                pointer(point, egui::PointerButton::Primary, true),
            );
            frame(
                &ctx,
                &mut studio,
                size,
                vec![egui::Event::PointerMoved(point + egui::vec2(45.0, 20.0))],
            );
            frame(
                &ctx,
                &mut studio,
                size,
                pointer(
                    point + egui::vec2(45.0, 20.0),
                    egui::PointerButton::Primary,
                    false,
                ),
            );
            assert!(studio.pinned_reference.as_ref().unwrap().pan.length() > 0.0);
            frame(
                &ctx,
                &mut studio,
                size,
                vec![egui::Event::Key {
                    key: egui::Key::Delete,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
            assert_eq!(studio.story, before);
            assert_eq!(studio.scroll_y, scroll);
            assert!(studio.undo.is_empty());
            assert!(!studio.dirty);
            let fit = ctx
                .data(|data| {
                    data.get_temp::<egui::Rect>(
                        egui::Id::new(("pinned_reference", studio.document_epoch)).with("fit"),
                    )
                })
                .unwrap();
            frame(
                &ctx,
                &mut studio,
                size,
                pointer(fit.center(), egui::PointerButton::Primary, true),
            );
            frame(
                &ctx,
                &mut studio,
                size,
                pointer(fit.center(), egui::PointerButton::Primary, false),
            );
            assert_eq!(studio.pinned_reference.as_ref().unwrap().zoom, 1.0);
            assert_eq!(
                studio.pinned_reference.as_ref().unwrap().pan,
                egui::Vec2::ZERO
            );
        }
    }

    #[test]
    fn removed_reference_rebases_safely_and_reused_asset_id_cannot_show_foreign_art() {
        let (ctx, mut studio, ids) = fixture();
        let size = egui::vec2(1100.0, 720.0);
        studio.story.assets[0]
            .character
            .references
            .retain(|reference| reference.asset_id != ids[1]);
        frame(&ctx, &mut studio, size, Vec::new());
        assert_eq!(studio.pinned_reference.as_ref().unwrap().asset, ids[0]);
        studio.pin_reference(&ctx, ids[2], None);
        studio.assets.insert(ids[2], Arc::new(png()));
        frame(&ctx, &mut studio, size, Vec::new());
        assert!(studio.pinned_reference.is_none());
        studio.pin_reference(&ctx, ids[0], Some(ids[0]));
        studio.reset_view();
        assert!(studio.pinned_reference.is_none());
        assert!(!studio.reference_rect.is_positive());
    }

    fn text_rect(shape: &egui::Shape, label: &str) -> Option<egui::Rect> {
        match shape {
            egui::Shape::Text(text) if text.galley.text() == label => {
                Some(text.visual_bounding_rect())
            }
            egui::Shape::Vec(shapes) => shapes.iter().find_map(|shape| text_rect(shape, label)),
            _ => None,
        }
    }

    #[test]
    fn reference_can_move_resize_and_accept_a_library_drop_without_placing_art() {
        let (ctx, mut studio, ids) = fixture();
        let size = egui::vec2(1100.0, 720.0);
        for _ in 0..3 {
            frame(&ctx, &mut studio, size, Vec::new());
        }
        let before_story = studio.story.clone();
        let before_move = studio.reference_rect;
        let header = before_move.center_top() + egui::vec2(0.0, 12.0);
        frame(
            &ctx,
            &mut studio,
            size,
            pointer(header, egui::PointerButton::Primary, true),
        );
        frame(
            &ctx,
            &mut studio,
            size,
            vec![egui::Event::PointerMoved(header - egui::vec2(180.0, 60.0))],
        );
        frame(
            &ctx,
            &mut studio,
            size,
            pointer(
                header - egui::vec2(180.0, 60.0),
                egui::PointerButton::Primary,
                false,
            ),
        );
        frame(&ctx, &mut studio, size, Vec::new());
        assert!(studio.reference_rect.left() < before_move.left() - 100.0);
        let original = studio.reference_rect;
        let corner = original.max - egui::vec2(2.0, 2.0);
        frame(
            &ctx,
            &mut studio,
            size,
            pointer(corner, egui::PointerButton::Primary, true),
        );
        frame(
            &ctx,
            &mut studio,
            size,
            vec![egui::Event::PointerMoved(corner + egui::vec2(100.0, 40.0))],
        );
        frame(
            &ctx,
            &mut studio,
            size,
            pointer(
                corner + egui::vec2(100.0, 40.0),
                egui::PointerButton::Primary,
                false,
            ),
        );
        frame(&ctx, &mut studio, size, Vec::new());
        assert!(studio.reference_rect.width() > original.width() + 30.0);
        let drop_at = image_rect(&ctx, &studio).center();
        frame(
            &ctx,
            &mut studio,
            size,
            pointer(drop_at, egui::PointerButton::Primary, true),
        );
        egui::DragAndDrop::set_payload(&ctx, ids[2]);
        frame(
            &ctx,
            &mut studio,
            size,
            pointer(drop_at, egui::PointerButton::Primary, false),
        );
        assert_eq!(studio.pinned_reference.as_ref().unwrap().asset, ids[2]);
        assert_eq!(studio.story, before_story);
        assert!(studio.undo.is_empty());
    }

    #[test]
    fn glass_paints_before_reference_controls_and_solid_keeps_the_same_geometry() {
        fn kinds(shape: &egui::Shape, result: &mut Vec<&'static str>) {
            match shape {
                egui::Shape::Callback(_) => result.push("blur"),
                egui::Shape::Text(_) => result.push("text"),
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| kinds(shape, result)),
                _ => (),
            }
        }
        let mut bounds = Vec::new();
        for glass in [false, true] {
            let (ctx, mut studio, _) = fixture();
            studio.preferences.glass_enabled = glass;
            studio.apply_preferences(&ctx);
            let mut output = None;
            for _ in 0..3 {
                output = Some(ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1100.0, 720.0),
                        )),
                        ..Default::default()
                    },
                    |ctx| studio.reference_window(ctx),
                ));
            }
            bounds.push((studio.reference_rect, image_rect(&ctx, &studio)));
            let mut ordered = Vec::new();
            for shape in &output.unwrap().shapes {
                kinds(&shape.shape, &mut ordered);
            }
            if glass {
                assert_eq!(ordered.iter().filter(|kind| **kind == "blur").count(), 1);
                assert!(
                    ordered.iter().position(|kind| *kind == "blur")
                        < ordered.iter().position(|kind| *kind == "text")
                );
            } else {
                assert!(!ordered.contains(&"blur"));
            }
        }
        assert_eq!(bounds[0], bounds[1]);
    }

    #[test]
    fn reference_menu_is_contextual_and_flip_is_view_only() {
        let (ctx, mut studio, _) = fixture();
        let size = egui::vec2(1100.0, 720.0);
        let before = studio.story.clone();
        for _ in 0..3 {
            frame(&ctx, &mut studio, size, Vec::new());
        }
        let point = image_rect(&ctx, &studio).center();
        frame(
            &ctx,
            &mut studio,
            size,
            pointer(point, egui::PointerButton::Secondary, true),
        );
        frame(
            &ctx,
            &mut studio,
            size,
            pointer(point, egui::PointerButton::Secondary, false),
        );
        let output = frame(&ctx, &mut studio, size, Vec::new());
        let flip = output
            .shapes
            .iter()
            .find_map(|shape| text_rect(&shape.shape, "Flip horizontal"))
            .expect("Reference actions must open over the image");
        assert!(!output
            .shapes
            .iter()
            .any(|shape| text_rect(&shape.shape, "Delete").is_some()));
        frame(
            &ctx,
            &mut studio,
            size,
            pointer(flip.center(), egui::PointerButton::Primary, true),
        );
        frame(
            &ctx,
            &mut studio,
            size,
            pointer(flip.center(), egui::PointerButton::Primary, false),
        );
        assert!(studio.pinned_reference.as_ref().unwrap().mirrored);
        assert_eq!(studio.story, before);
        assert!(studio.undo.is_empty());
    }

    #[test]
    fn dropping_a_file_on_reference_imports_once_without_placing_a_layer() {
        let (ctx, mut studio, _) = fixture();
        let size = egui::vec2(1100.0, 720.0);
        for _ in 0..3 {
            frame(&ctx, &mut studio, size, Vec::new());
        }
        let point = image_rect(&ctx, &studio).center();
        let path =
            std::env::temp_dir().join(format!("scroll-reference-drop-{}.png", std::process::id()));
        std::fs::write(&path, png()).unwrap();
        let before = studio.story.clone();
        let mut app_frame = eframe::Frame::_new_kittest();
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                events: vec![egui::Event::PointerMoved(point)],
                dropped_files: vec![egui::DroppedFile {
                    path: Some(path.clone()),
                    ..Default::default()
                }],
                ..Default::default()
            },
            |ctx| eframe::App::update(&mut studio, ctx, &mut app_frame),
        );
        std::fs::remove_file(path).unwrap();
        assert_eq!(studio.story.chapters, before.chapters);
        assert_eq!(studio.story.assets.len(), before.assets.len() + 1);
        assert_eq!(studio.undo.len(), 1);
        assert_eq!(
            studio.pinned_reference.as_ref().unwrap().asset,
            studio.story.assets.last().unwrap().id
        );
        studio.undo();
        frame(&ctx, &mut studio, size, Vec::new());
        assert_eq!(studio.story, before);
        assert!(studio.pinned_reference.is_none());
    }
}
