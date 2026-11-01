//! Read a chapter at a phone-like width without allocating an episode bitmap.
use super::*;

pub(super) struct ReaderView {
    chapter_id: u64,
    wide: bool,
    initial_y: Option<f32>,
    scroll_y: f32,
}

impl ReaderView {
    fn set_wide(&mut self, wide: bool) {
        if self.wide != wide {
            self.wide = wide;
            self.initial_y = Some(self.scroll_y);
        }
    }
}

impl Studio {
    pub(super) fn begin_reader(&mut self) {
        let Some(chapter) = self.story.chapters.get(self.chapter) else {
            return;
        };
        self.reader = Some(ReaderView {
            chapter_id: chapter.id,
            wide: false,
            initial_y: Some(self.scroll_y.max(0.0)),
            scroll_y: self.scroll_y.max(0.0),
        });
        self.drag_layer = None;
        self.drag_tail = None;
        self.snap_drag = None;
        self.snap_guides.clear();
        self.resize_anchor = None;
    }

    pub(super) fn reader_ui(&mut self, ctx: &egui::Context) {
        let Some(mut reader) = self.reader.take() else {
            return;
        };
        let Some(chapter) = self
            .story
            .chapters
            .iter()
            .find(|chapter| chapter.id == reader.chapter_id)
        else {
            return;
        };
        let mut close = self.dialog.is_none()
            && self.error.is_none()
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        egui::TopBottomPanel::top("reader_header")
            .frame(egui::Frame::new().fill(PANEL).inner_margin(12.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let back = motion::button(ui, "‹ Back to studio");
                    #[cfg(test)]
                    ctx.data_mut(|data| {
                        data.insert_temp(egui::Id::new("reader_back_bounds"), back.rect)
                    });
                    close |= back.clicked();
                    ui.add_space(14.0);
                    let title_width = (ui.available_width() - 230.0).max(40.0);
                    ui.add_sized(
                        egui::vec2(title_width, 30.0),
                        egui::Label::new(RichText::new(&chapter.name).strong()).truncate(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if motion::button(ui, "Top").clicked() {
                            reader.initial_y = Some(0.0);
                        }
                        ui.add_space(10.0);
                        for (wide, label) in [(true, "Wide"), (false, "Phone")] {
                            let response =
                                ui.add(motion::Button::new(label).selected(reader.wide == wide));
                            #[cfg(test)]
                            ctx.data_mut(|data| {
                                data.insert_temp(
                                    egui::Id::new(("reader_width_bounds", wide)),
                                    response.rect,
                                )
                            });
                            if response.clicked() {
                                reader.set_wide(wide);
                            }
                        }
                    });
                });
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG).inner_margin(18.0))
            .show(ctx, |ui| {
                let background = ui.interact(
                    ui.max_rect(),
                    ui.id().with("reader_context"),
                    egui::Sense::click(),
                );
                let width = (if reader.wide { 420.0_f32 } else { 360.0_f32 })
                    .min((ui.available_width() - 24.0).max(80.0));
                let scale = width / chapter.width.max(1) as f32;
                let available = ui.available_rect_before_wrap();
                let viewport = egui::Rect::from_min_size(
                    egui::pos2(available.center().x - (width + 16.0) * 0.5, available.top()),
                    egui::vec2(width + 16.0, available.height()),
                );
                ui.scope_builder(egui::UiBuilder::new().max_rect(viewport), |ui| {
                    let mut area = egui::ScrollArea::vertical()
                        .id_salt(("reader", self.document_epoch, chapter.id))
                        .auto_shrink([false, false])
                        .scroll_bar_visibility(
                            egui::scroll_area::ScrollBarVisibility::AlwaysVisible,
                        );
                    if let Some(y) = reader.initial_y.take() {
                        area = area.vertical_scroll_offset(y * scale);
                    }
                    let output = area.show(ui, |ui| {
                        let (canvas, response) = ui.allocate_exact_size(
                            egui::vec2(width, chapter.height as f32 * scale),
                            egui::Sense::hover(),
                        );
                        let painter = ui
                            .painter()
                            .with_clip_rect(ui.clip_rect().intersect(canvas));
                        painter.rect_filled(canvas, 0.0, Color32::WHITE);
                        #[cfg(test)]
                        ctx.data_mut(|data| {
                            data.insert_temp(
                                egui::Id::new("reader_canvas_bounds"),
                                (canvas, painter.clip_rect()),
                            )
                        });
                        for layer in chapter.layers.iter().filter(|layer| layer.visible) {
                            let rendered =
                                scroll_studio::bubbles::joined_layer(layer, &chapter.layers);
                            Self::paint_layer(
                                &mut self.previews,
                                &self.assets,
                                ctx,
                                &painter,
                                &rendered,
                                canvas,
                                scale,
                            );
                        }
                        appearance::context_menu(&response, |ui| {
                            reader_actions(ui, &mut reader, &mut close)
                        });
                    });
                    reader.scroll_y = output.state.offset.y / scale;
                });
                appearance::context_menu(&background, |ui| {
                    reader_actions(ui, &mut reader, &mut close)
                });
            });
        if !close {
            self.reader = Some(reader);
        } else {
            ctx.request_repaint();
        }
        // The app deliberately skips editor input in reading mode. Closing the
        // window must still present Save/Discard/Cancel over this workspace.
        self.dialogs(ctx);
        self.flush_preferences(ctx, false);
    }
}

fn reader_actions(ui: &mut egui::Ui, reader: &mut ReaderView, close: &mut bool) {
    for (wide, label) in [(false, "Phone"), (true, "Wide")] {
        if ui.selectable_label(reader.wide == wide, label).clicked() {
            reader.set_wide(wide);
            ui.close_menu();
        }
    }
    if motion::button(ui, "Back to top").clicked() {
        reader.initial_y = Some(0.0);
        ui.close_menu();
    }
    ui.separator();
    if motion::button(ui, "Back to studio").clicked() {
        *close = true;
        ui.close_menu();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reader_keeps_the_unsaved_close_prompt_visible() {
        fn text(shape: &egui::Shape, labels: &mut Vec<String>) {
            match shape {
                egui::Shape::Text(shape) => labels.push(shape.galley.text().to_owned()),
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| text(shape, labels)),
                _ => (),
            }
        }
        let ctx = egui::Context::default();
        let mut studio = Studio::from_context(&ctx);
        studio.welcome = false;
        studio.dirty = true;
        studio.begin_reader();
        studio.request(Pending::Quit);
        let mut labels = Vec::new();
        for frame in 0..3 {
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1100.0, 720.0),
                    )),
                    events: if frame == 2 {
                        vec![egui::Event::Key {
                            key: egui::Key::Escape,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers: egui::Modifiers::NONE,
                        }]
                    } else {
                        Vec::new()
                    },
                    ..Default::default()
                },
                |ctx| studio.reader_ui(ctx),
            );
            for shape in &output.shapes {
                text(&shape.shape, &mut labels);
            }
        }
        assert!(labels.iter().any(|label| label == "Save your story?"));
        assert!(labels.iter().any(|label| label == "Save & continue"));
        assert!(labels.iter().any(|label| label == "Cancel"));
        assert!(studio.reader.is_some());
        assert!(studio.dialog.is_some());
        assert!(!studio.allow_close);
    }

    #[test]
    fn changing_reader_width_preserves_document_position_with_a_long_title() {
        let ctx = egui::Context::default();
        let mut studio = Studio::from_context(&ctx);
        studio.story.chapters[studio.chapter].name = "A long chapter title ".repeat(100);
        studio.scroll_y = 900.0;
        studio.begin_reader();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1100.0, 720.0));
        let mut frame = |events| {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(screen),
                    events,
                    ..Default::default()
                },
                |ctx| studio.reader_ui(ctx),
            );
        };
        frame(Vec::new());
        frame(Vec::new());
        let wide = ctx
            .data(|data| data.get_temp::<egui::Rect>(egui::Id::new(("reader_width_bounds", true))))
            .unwrap();
        let phone = ctx
            .data(|data| data.get_temp::<egui::Rect>(egui::Id::new(("reader_width_bounds", false))))
            .unwrap();
        assert!(screen.contains_rect(wide) && screen.contains_rect(phone));
        for pressed in [true, false] {
            frame(vec![
                egui::Event::PointerMoved(wide.center()),
                egui::Event::PointerButton {
                    pos: wide.center(),
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ]);
        }
        frame(Vec::new());
        let reader = studio.reader.as_ref().unwrap();
        assert!(reader.wide);
        assert!(
            (reader.scroll_y - 900.0).abs() < 1.0,
            "width changes should retain the current comic position"
        );
        assert_eq!(studio.scroll_y, 900.0);
    }

    #[test]
    fn reader_is_bounded_read_only_and_returns_to_exact_editor_state() {
        for size in [egui::vec2(1100.0, 720.0), egui::vec2(1707.0, 960.0)] {
            let ctx = egui::Context::default();
            let mut studio = Studio::from_context(&ctx);
            studio.welcome = false;
            studio.zoom = 0.61;
            studio.auto_fit = false;
            studio.scroll_y = 300.0;
            studio.selected = studio.story.chapters[studio.chapter]
                .layers
                .first()
                .map(|layer| layer.id);
            let before = studio.story.clone();
            let selected = studio.selected;
            let history = studio.undo.len();
            let dirty = studio.dirty;
            studio.begin_reader();
            for _ in 0..2 {
                let _ = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                        ..Default::default()
                    },
                    |ctx| studio.reader_ui(ctx),
                );
            }
            let (canvas, clip) = ctx
                .data(|data| {
                    data.get_temp::<(egui::Rect, egui::Rect)>(egui::Id::new("reader_canvas_bounds"))
                })
                .unwrap();
            assert_eq!(canvas.width(), 360.0);
            assert!(egui::Rect::from_min_size(egui::Pos2::ZERO, size).contains_rect(clip));
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    events: vec![egui::Event::Key {
                        key: egui::Key::Escape,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    }],
                    ..Default::default()
                },
                |ctx| studio.reader_ui(ctx),
            );
            assert!(studio.reader.is_none());
            assert_eq!(studio.story, before);
            assert_eq!(studio.selected, selected);
            assert_eq!(studio.undo.len(), history);
            assert_eq!(studio.dirty, dirty);
            assert_eq!(studio.zoom, 0.61);
            assert!(!studio.auto_fit);
            assert_eq!(studio.scroll_y, 300.0);
        }
    }
}
