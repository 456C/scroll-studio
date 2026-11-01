use super::*;

#[derive(Clone, Copy)]
pub(super) struct PresetSliderEdit {
    document_epoch: u64,
    preset_id: u64,
    property: lettering_ui::PresetSlider,
    revision: u64,
}
use scroll_studio::bubbles;

impl Studio {
    fn current_bubble_preset(&self) -> Option<&BubblePreset> {
        self.story
            .bubble_presets
            .iter()
            .find(|p| Some(p.id) == self.selected_bubble_preset)
            .or_else(|| self.story.bubble_presets.first())
    }

    fn preset_usage(&self, id: u64) -> usize {
        self.story.chapters.iter().flat_map(|c| &c.layers).filter(|layer| {
            matches!(&layer.kind, LayerKind::Text { style, .. } if style.preset_id == Some(id))
        }).count()
    }

    pub(super) fn add_bubble_preset(&mut self, duplicate: bool) {
        if self.story.bubble_presets.len() >= 4096 {
            self.status = "This story has reached its preset limit.".into();
            return;
        }
        let mut preset = if duplicate {
            self.current_bubble_preset().cloned()
        } else {
            None
        }
        .unwrap_or_else(|| {
            let width = self
                .story
                .chapters
                .get(self.chapter)
                .map_or(1600, |c| c.width);
            let mut preset = BubblePreset::for_canvas(0, LetteringTheme::WebtoonClassic, width);
            preset.name = "New bubble".into();
            preset
        });
        if duplicate {
            preset.name.push_str(" copy");
        }
        self.checkpoint();
        preset.id = self.story.alloc_id();
        preset.style.preset_id = None;
        self.selected_bubble_preset = Some(preset.id);
        self.story.bubble_presets.push(preset);
        self.tab = LibraryTab::Bubbles;
        self.selected_asset = None;
        self.changed();
    }

    pub(super) fn bubble_preset_list(&mut self, ui: &mut egui::Ui) {
        if self.story.bubble_presets.is_empty() {
            return;
        }
        if primary(ui, "+ Preset").clicked() {
            self.add_bubble_preset(false);
        }
        let selected = self.current_bubble_preset().map(|p| p.id);
        let entries: Vec<_> = self
            .story
            .bubble_presets
            .iter()
            .map(|p| (p.id, p.name.clone(), self.preset_usage(p.id)))
            .collect();
        egui::ScrollArea::vertical()
            .id_salt("bubble_preset_list")
            .show(ui, |ui| {
                for (id, name, usage) in entries {
                    ui.push_id(id, |ui| {
                        let response = panels::list_row(
                            ui,
                            &format!("{name}  ·  {usage}"),
                            selected == Some(id),
                            34.0,
                            None,
                        );
                        if response.clicked() || response.secondary_clicked() {
                            self.selected_bubble_preset = Some(id);
                        }
                        self.register_context(&response);
                        appearance::context_menu(&response, |ui| {
                            self.preset_context_actions(ui, id)
                        });
                    });
                }
            });
    }

    pub(super) fn bubble_preset_inspector(&mut self, ctx: &egui::Context) {
        if self.current_bubble_preset().is_none() {
            return;
        }
        egui::SidePanel::right("preset_inspector")
            .exact_width(280.0)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(PANEL)
                    .stroke(egui::Stroke::new(1.0_f32, BG))
                    .inner_margin(14.0),
            )
            .show(ctx, |ui| {
                let Some(original) = self.current_bubble_preset().cloned() else {
                    return;
                };
                let mut preset = original.clone();
                let usage = self.preset_usage(preset.id);
                let mut duplicate = false;
                let mut delete = false;
                if self
                    .bubble_preset_name
                    .as_ref()
                    .is_none_or(|(id, _)| *id != original.id)
                {
                    self.bubble_preset_name = Some((original.id, original.name.clone()));
                }
                let (_, name) = self.bubble_preset_name.as_mut().unwrap();
                ui.horizontal(|ui| {
                    let response = ui.add(
                        egui::TextEdit::singleline(name)
                            .hint_text("Preset name")
                            .char_limit(1024)
                            .desired_width((ui.available_width() - 42.0).max(80.0)),
                    );
                    if !name.trim().is_empty() {
                        preset.name = name.clone();
                    } else if response.lost_focus() {
                        *name = original.name.clone();
                    }
                    motion::menu_button(ui, "…", |ui| {
                        if motion::button(ui, "Duplicate").clicked() {
                            duplicate = true;
                            ui.close_menu();
                        }
                        if ui
                            .add_enabled(
                                usage == 0 && self.story.bubble_presets.len() > 1,
                                motion::Button::new("Delete"),
                            )
                            .on_disabled_hover_text(if usage > 0 {
                                "This preset is still in use."
                            } else {
                                "Keep at least one preset in the story."
                            })
                            .clicked()
                        {
                            delete = true;
                            ui.close_menu();
                        }
                    });
                });
                ui.label(
                    RichText::new(format!("{usage} in story"))
                        .size(12.0)
                        .color(MUTED),
                )
                .on_hover_text("Changes apply to every bubble using this preset");
                ui.add_space(8.0);
                let drag = ui
                    .push_id((self.document_epoch, preset.id), |ui| {
                        lettering_ui::preset_appearance_editor(ui, &mut preset)
                    })
                    .inner;
                self.edit_bubble_preset(&original, preset, drag);
                if duplicate {
                    self.add_bubble_preset(true);
                }
                if delete {
                    self.checkpoint();
                    self.story.bubble_presets.retain(|p| p.id != original.id);
                    self.selected_bubble_preset = None;
                    self.changed();
                }
            });
    }

    fn edit_bubble_preset(
        &mut self,
        original: &BubblePreset,
        mut preset: BubblePreset,
        mut drag: Option<lettering_ui::PresetSliderDrag>,
    ) {
        if self.block_preset_slider_until_release {
            if let Some(interrupted) = drag.take() {
                interrupted.property.restore_value(original, &mut preset);
            }
        }
        let slider = drag.filter(|drag| drag.property.is_only_change(original, &preset));
        let continuing = slider
            .zip(self.preset_slider_edit)
            .is_some_and(|(drag, edit)| {
                !drag.started
                    && edit.document_epoch == self.document_epoch
                    && edit.preset_id == original.id
                    && edit.property == drag.property
                    && edit.revision == self.revision
            });
        if !continuing {
            self.preset_slider_edit = None;
        }
        if preset != *original {
            if !continuing {
                self.checkpoint();
            }
            match bubbles::update_preset(&mut self.story, preset) {
                Ok(()) => {
                    self.changed();
                    self.preset_slider_edit = slider.map(|drag| PresetSliderEdit {
                        document_epoch: self.document_epoch,
                        preset_id: original.id,
                        property: drag.property,
                        revision: self.revision,
                    });
                }
                Err(e) => {
                    self.preset_slider_edit = None;
                    self.error = Some(format!("Could not update bubble preset: {e:#}"));
                }
            }
        }
        // A move and its release can arrive together. Commit that final value
        // into the existing history entry before ending the gesture.
        if slider.is_none_or(|drag| drag.stopped) {
            self.preset_slider_edit = None;
        }
    }

    pub(super) fn interrupt_preset_slider_edit(&mut self) {
        if self.preset_slider_edit.take().is_some() {
            self.block_preset_slider_until_release = true;
        }
    }

    pub(super) fn finish_preset_slider_frame(&mut self, ctx: &egui::Context) {
        if self.preset_slider_edit.is_none() && !self.block_preset_slider_until_release {
            return;
        }
        let (pointer_down, focused) =
            ctx.input(|input| (input.pointer.primary_down(), input.focused));
        let preset_id = self.current_bubble_preset().map(|preset| preset.id);
        if !pointer_down
            || !focused
            || self.tab != LibraryTab::Bubbles
            || self.settings_open
            || self.welcome
            || self.dialog.is_some()
            || self.style_prompt_open
            || self.setup.is_some()
            || self.preset_slider_edit.is_some_and(|edit| {
                edit.document_epoch != self.document_epoch
                    || Some(edit.preset_id) != preset_id
                    || edit.revision != self.revision
            })
        {
            self.interrupt_preset_slider_edit();
        }
        if !pointer_down {
            self.block_preset_slider_until_release = false;
        }
    }

    pub(super) fn bubble_preset_workspace(&mut self, ctx: &egui::Context) {
        self.canvas_rect = egui::Rect::NOTHING;
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG).inner_margin(16.0))
            .show(ctx, |ui| {
                let Some(preset) = self.current_bubble_preset().cloned() else {
                    let available = ui.available_rect_before_wrap();
                    let composition = egui::Rect::from_center_size(
                        available.center() - egui::vec2(0.0, 20.0),
                        egui::vec2(240.0, 148.0).min(available.size()),
                    );
                    ui.scope_builder(
                        egui::UiBuilder::new()
                            .max_rect(composition)
                            .layout(egui::Layout::top_down(egui::Align::Center)),
                        |ui| {
                            let (mark, _) = ui
                                .allocate_exact_size(egui::vec2(84.0, 76.0), egui::Sense::hover());
                            icons::draw(
                                ui.painter(),
                                mark.shrink(10.0),
                                icons::Icon::Bubble,
                                MUTED.gamma_multiply(0.6),
                            );
                            ui.add_space(16.0);
                            let response = ui.add_sized(
                                [148.0, 36.0],
                                motion::Button::new(RichText::new("+ Preset").color(BG))
                                    .fill(ACCENT),
                            );
                            #[cfg(test)]
                            ctx.data_mut(|data| {
                                data.insert_temp(
                                    egui::Id::new("empty_bubble_action_bounds"),
                                    (available, response.rect),
                                );
                            });
                            if response.clicked() {
                                self.add_bubble_preset(false);
                            }
                        },
                    );
                    return;
                };
                ui.horizontal(|ui| {
                    ui.heading("Bubbles");
                    ui.add_space(12.0);
                    self.bubble_preview.mode_controls(ui);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if primary(ui, "Use")
                            .on_hover_text(self.story.chapters.get(self.chapter).map_or_else(
                                || "Place in chapter".into(),
                                |chapter| format!("Place in {}", chapter.name),
                            ))
                            .clicked()
                        {
                            self.selected_bubble_preset = Some(preset.id);
                            self.tab = LibraryTab::Chapters;
                            self.selected = None;
                            self.tool = Tool::Text;
                            self.status = preset.name.clone();
                        }
                        if ui
                            .add_enabled(!self.redo.is_empty(), motion::Button::new("Redo"))
                            .on_hover_text("Redo")
                            .clicked()
                        {
                            self.redo();
                        }
                        if ui
                            .add_enabled(!self.undo.is_empty(), motion::Button::new("Undo"))
                            .on_hover_text("Undo")
                            .clicked()
                        {
                            self.undo();
                        }
                    });
                });
                ui.add_space(8.0);
                self.bubble_preview
                    .chapter_control(ui, &self.story.chapters, self.chapter);
                let preview_height = (ui.available_height() - 104.0).max(80.0);
                self.paint_preset_preview(ui, ctx, &preset, preview_height);
                ui.add_space(4.0);
                egui::ScrollArea::vertical()
                    .id_salt("preset_sample_text")
                    .max_height(80.0)
                    .min_scrolled_height(80.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let response = ui.add(
                            egui::TextEdit::multiline(&mut self.bubble_preview.text)
                                .hint_text("Sample dialogue")
                                .char_limit(16_384)
                                .desired_rows(3)
                                .desired_width(f32::INFINITY),
                        );
                        #[cfg(test)]
                        ctx.data_mut(|data| {
                            data.insert_temp(
                                egui::Id::new("bubble_preview_text"),
                                response.rect.intersect(ui.clip_rect()),
                            )
                        });
                        let _ = response;
                    });
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lettering_ui::{PresetSlider, PresetSliderDrag};

    #[test]
    fn adding_a_preset_uses_current_canvas_but_duplication_keeps_saved_appearance() {
        let ctx = egui::Context::default();
        let mut studio = Studio::from_context(&ctx);
        studio.story = Story::with_canvas("Mixed resolutions", 800, 9600);
        let second = Chapter::new(studio.story.alloc_id(), "2x artwork", 1600, 9600);
        studio.story.chapters.push(second);
        studio.chapter = 1;
        let before = studio.story.clone();
        studio.add_bubble_preset(false);
        assert_eq!(studio.story.bubble_presets.last().unwrap().size, 76.0);
        studio.undo();
        assert_eq!(studio.story, before);
        studio.selected_bubble_preset = Some(studio.story.bubble_presets[0].id);
        let source = studio.story.bubble_presets[0].clone();
        studio.add_bubble_preset(true);
        let copy = studio.story.bubble_presets.last().unwrap();
        assert_eq!(copy.size, source.size);
        assert_eq!(copy.style, source.style);
        assert_ne!(copy.id, source.id);
        studio.undo();
        assert_eq!(studio.story, before);
    }

    fn slider_fixture() -> (egui::Context, Studio) {
        let ctx = egui::Context::default();
        let mut studio = Studio::from_context(&ctx);
        studio.story = Story::demo();
        let mut chapter = studio.story.chapters[0].clone();
        chapter.id = studio.story.alloc_id();
        for layer in &mut chapter.layers {
            layer.id = studio.story.alloc_id();
        }
        for panel in &mut chapter.panels {
            panel.id = studio.story.alloc_id();
        }
        studio.story.chapters.push(chapter);
        studio.selected_bubble_preset =
            studio.story.chapters[0]
                .layers
                .iter()
                .find_map(|layer| match &layer.kind {
                    LayerKind::Text { style, .. } => style.preset_id,
                    _ => None,
                });
        studio.tab = LibraryTab::Bubbles;
        studio.welcome = false;
        studio.dirty = false;
        studio.preferences.motion = settings::Motion::Off;
        studio.apply_preferences(&ctx);
        slider_frame(&ctx, &mut studio, Vec::new());
        slider_frame(&ctx, &mut studio, Vec::new());
        (ctx, studio)
    }

    fn slider_frame(ctx: &egui::Context, studio: &mut Studio, events: Vec<egui::Event>) {
        let _ = empty_frame(ctx, studio, egui::vec2(1100.0, 720.0), events);
    }

    fn slider_bounds(ctx: &egui::Context) -> (egui::Rect, f32, egui::Id) {
        ctx.data(|data| {
            data.get_temp(egui::Id::new(("preset_slider_bounds", PresetSlider::Size)))
                .unwrap()
        })
    }

    fn slider_point(ctx: &egui::Context, fraction: f32) -> egui::Pos2 {
        let (rect, width, _) = slider_bounds(ctx);
        egui::pos2(rect.left() + width * fraction, rect.center().y)
    }

    fn slider_button(point: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    fn drag_slider(ctx: &egui::Context, studio: &mut Studio, start: f32, end: f32) {
        let start = slider_point(ctx, start);
        slider_frame(
            ctx,
            studio,
            vec![egui::Event::PointerMoved(start), slider_button(start, true)],
        );
        let end = slider_point(ctx, end);
        slider_frame(
            ctx,
            studio,
            vec![egui::Event::PointerMoved(end), slider_button(end, false)],
        );
    }

    #[test]
    fn preset_slider_drag_is_one_undo_across_linked_chapters_including_release() {
        let (ctx, mut studio) = slider_fixture();
        let original = studio.story.clone();
        let start = slider_point(&ctx, 0.3);
        slider_frame(
            &ctx,
            &mut studio,
            vec![egui::Event::PointerMoved(start), slider_button(start, true)],
        );
        for fraction in [0.4, 0.5, 0.6, 0.7] {
            let point = slider_point(&ctx, fraction);
            slider_frame(&ctx, &mut studio, vec![egui::Event::PointerMoved(point)]);
            assert_eq!(studio.undo.len(), 1);
        }
        let before_release = studio.current_bubble_preset().unwrap().size;
        let end = slider_point(&ctx, 0.9);
        slider_frame(
            &ctx,
            &mut studio,
            vec![egui::Event::PointerMoved(end), slider_button(end, false)],
        );
        let edited = studio.story.clone();
        assert!(studio.current_bubble_preset().unwrap().size > before_release);
        assert_eq!(studio.undo.len(), 1);
        assert!(studio.preset_slider_edit.is_none());
        assert_ne!(edited.chapters[0], original.chapters[0]);
        assert_ne!(edited.chapters[1], original.chapters[1]);
        studio.undo();
        assert_eq!(studio.story, original);
        studio.redo();
        assert_eq!(studio.story, edited);
    }

    #[test]
    fn separate_slider_gestures_and_presets_have_separate_history() {
        let (ctx, mut studio) = slider_fixture();
        drag_slider(&ctx, &mut studio, 0.3, 0.5);
        let first = studio.story.clone();
        drag_slider(&ctx, &mut studio, 0.6, 0.8);
        let second = studio.story.clone();
        assert_eq!(studio.undo.len(), 2);
        let previous_id = studio.current_bubble_preset().unwrap().id;
        studio.selected_bubble_preset = studio
            .story
            .bubble_presets
            .iter()
            .find(|preset| preset.id != previous_id)
            .map(|preset| preset.id);
        slider_frame(&ctx, &mut studio, Vec::new());
        drag_slider(&ctx, &mut studio, 0.4, 0.7);
        assert_eq!(studio.undo.len(), 3);
        studio.undo();
        assert_eq!(studio.story, second);
        studio.undo();
        assert_eq!(studio.story, first);
    }

    #[test]
    fn unchanged_slider_gesture_keeps_history_and_redo() {
        let (ctx, mut studio) = slider_fixture();
        let mut preset = studio.current_bubble_preset().unwrap().clone();
        preset.size = 120.0;
        bubbles::update_preset(&mut studio.story, preset).unwrap();
        let original = studio.story.clone();
        studio.redo.push(Story::new("Unrelated redo branch"));
        let redo = studio.redo.clone();
        drag_slider(&ctx, &mut studio, 1.0, 1.0);
        assert_eq!(studio.story, original);
        assert!(studio.undo.is_empty());
        assert_eq!(studio.redo, redo);
        assert!(!studio.dirty);
    }

    #[test]
    fn undo_during_a_slider_drag_blocks_the_remaining_gesture() {
        let (ctx, mut studio) = slider_fixture();
        let original = studio.story.clone();
        let start = slider_point(&ctx, 0.5);
        slider_frame(
            &ctx,
            &mut studio,
            vec![egui::Event::PointerMoved(start), slider_button(start, true)],
        );
        assert_ne!(studio.story, original);
        studio.undo();
        assert!(studio.block_preset_slider_until_release);
        let end = slider_point(&ctx, 0.9);
        slider_frame(&ctx, &mut studio, vec![egui::Event::PointerMoved(end)]);
        assert_eq!(studio.story, original);
        slider_frame(&ctx, &mut studio, vec![slider_button(end, false)]);
        assert_eq!(studio.story, original);
        assert!(!studio.block_preset_slider_until_release);
        assert_eq!(studio.redo.len(), 1);
        drag_slider(&ctx, &mut studio, 0.4, 0.6);
        assert_ne!(studio.story, original);
        assert_eq!(studio.undo.len(), 1);
        assert!(studio.redo.is_empty());
    }

    #[test]
    fn slider_number_entry_remains_independent_and_commits_on_focus_loss() {
        let (ctx, mut studio) = slider_fixture();
        drag_slider(&ctx, &mut studio, 0.3, 0.5);
        let after_drag = studio.story.clone();
        let (bounds, _, _) = slider_bounds(&ctx);
        let value = egui::pos2(bounds.right() - 12.0, bounds.center().y);
        slider_frame(&ctx, &mut studio, pointer_click(value));
        slider_frame(&ctx, &mut studio, vec![egui::Event::Text("82".into())]);
        slider_frame(&ctx, &mut studio, pointer_click(egui::pos2(500.0, 650.0)));
        assert_eq!(studio.current_bubble_preset().unwrap().size, 82.0);
        assert!(studio.preset_slider_edit.is_none());
        assert_eq!(studio.undo.len(), 2);
        studio.undo();
        assert_eq!(studio.story, after_drag);
    }

    #[test]
    fn leaving_the_preset_editor_ends_and_suppresses_its_active_drag() {
        let (ctx, mut studio) = slider_fixture();
        let start = slider_point(&ctx, 0.5);
        slider_frame(
            &ctx,
            &mut studio,
            vec![egui::Event::PointerMoved(start), slider_button(start, true)],
        );
        let edited = studio.story.clone();
        studio.settings_open = true;
        slider_frame(&ctx, &mut studio, Vec::new());
        assert!(studio.preset_slider_edit.is_none());
        assert!(studio.block_preset_slider_until_release);
        studio.settings_open = false;
        let end = slider_point(&ctx, 0.8);
        slider_frame(&ctx, &mut studio, vec![egui::Event::PointerMoved(end)]);
        slider_frame(&ctx, &mut studio, vec![slider_button(end, false)]);
        assert_eq!(studio.story, edited);
        assert_eq!(studio.undo.len(), 1);
        drag_slider(&ctx, &mut studio, 0.6, 0.9);
        assert_eq!(studio.undo.len(), 2);
        studio.undo();
        assert_eq!(studio.story, edited);
    }

    #[test]
    fn unrelated_preset_changes_never_join_an_active_slider_transaction() {
        let (ctx, mut studio) = slider_fixture();
        let start = slider_point(&ctx, 0.5);
        slider_frame(
            &ctx,
            &mut studio,
            vec![egui::Event::PointerMoved(start), slider_button(start, true)],
        );
        let after_drag = studio.story.clone();
        let original = studio.current_bubble_preset().unwrap().clone();
        let mut mixed = original.clone();
        mixed.size += 1.0;
        mixed.name = "Renamed while editing".into();
        studio.edit_bubble_preset(
            &original,
            mixed,
            Some(PresetSliderDrag {
                property: PresetSlider::Size,
                started: false,
                stopped: false,
            }),
        );
        assert_eq!(studio.undo.len(), 2);
        assert!(studio.preset_slider_edit.is_none());
        let after_name = studio.story.clone();
        let original = studio.current_bubble_preset().unwrap().clone();
        let mut color = original.clone();
        color.style.fill = [100, 120, 140, 255];
        studio.edit_bubble_preset(&original, color, None);
        assert_eq!(studio.undo.len(), 3);
        studio.undo();
        assert_eq!(studio.story, after_name);
        studio.undo();
        assert_eq!(studio.story, after_drag);
    }

    fn empty_frame(
        ctx: &egui::Context,
        studio: &mut Studio,
        screen: egui::Vec2,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        let mut frame = eframe::Frame::_new_kittest();
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, screen)),
                events,
                ..Default::default()
            },
            |ctx| eframe::App::update(studio, ctx, &mut frame),
        )
    }

    fn pointer_click(point: egui::Pos2) -> Vec<egui::Event> {
        vec![
            egui::Event::PointerMoved(point),
            egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            },
        ]
    }

    fn inspect_empty_shapes(shape: &egui::Shape, actions: &mut usize, fills: &mut Vec<egui::Rect>) {
        match shape {
            egui::Shape::Text(text) if text.galley.job.text == "+ Preset" => *actions += 1,
            egui::Shape::Rect(rect) if rect.fill == ACCENT => fills.push(rect.rect),
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    inspect_empty_shapes(shape, actions, fills);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn empty_bubbles_has_one_compact_action_and_no_unused_inspector() {
        for (screen, pixels_per_point) in [
            (egui::vec2(1100.0, 720.0), 1.0),
            (egui::vec2(2560.0, 1440.0), 1.0),
            (egui::vec2(2560.0 / 1.5, 1440.0 / 1.5), 1.5),
        ] {
            let ctx = egui::Context::default();
            ctx.set_pixels_per_point(pixels_per_point);
            let mut studio = Studio::from_context(&ctx);
            studio.story = Story::new("Empty presets");
            studio.story.bubble_presets.clear();
            studio.welcome = false;
            studio.error = None;
            studio.dirty = false;
            studio.tab = LibraryTab::Bubbles;
            studio.preferences.motion = settings::Motion::Off;
            studio.apply_preferences(&ctx);
            let original = studio.story.clone();
            let _ = empty_frame(&ctx, &mut studio, screen, Vec::new());
            let output = empty_frame(&ctx, &mut studio, screen, Vec::new());
            assert_eq!(output.pixels_per_point, pixels_per_point);
            let (available, action) = ctx.data(|data| {
                data.get_temp::<(egui::Rect, egui::Rect)>(egui::Id::new(
                    "empty_bubble_action_bounds",
                ))
                .unwrap()
            });
            assert!(available.contains_rect(action));
            assert!(action.width() <= 149.0 && action.height() <= 37.0);
            assert!((action.center().x - available.center().x).abs() < 1.0);
            assert!((available.right() - (screen.x - 16.0)).abs() < 1.0);

            let mut actions = 0;
            let mut fills = Vec::new();
            for shape in output.shapes {
                inspect_empty_shapes(&shape.shape, &mut actions, &mut fills);
            }
            assert_eq!(
                actions, 1,
                "Only the central creation action should be shown"
            );
            assert!(fills.iter().any(|rect| rect.contains(action.center())));
            assert!(fills.iter().all(|rect| rect.area() <= 200.0 * 48.0));
            assert_eq!(studio.story, original);
            assert!(!studio.dirty);

            let blank = available.right_bottom() - egui::vec2(30.0, 30.0);
            let _ = empty_frame(&ctx, &mut studio, screen, pointer_click(blank));
            assert_eq!(
                studio.story, original,
                "Blank space must not create a preset"
            );
            assert!(studio.undo.is_empty());
            let _ = empty_frame(&ctx, &mut studio, screen, pointer_click(action.center()));
            assert_eq!(studio.story.bubble_presets.len(), 1);
            assert_eq!(studio.undo.len(), 1);
            let created = studio.story.clone();
            studio.undo();
            assert_eq!(studio.story, original);
            studio.redo();
            assert_eq!(studio.story, created);
        }
    }

    #[test]
    fn undo_and_redo_restore_preset_and_all_chapters_together() {
        let ctx = egui::Context::default();
        let mut studio = Studio::from_context(&ctx);
        studio.story = Story::demo();
        let mut chapter = studio.story.chapters[0].clone();
        chapter.id = studio.story.alloc_id();
        for layer in &mut chapter.layers {
            layer.id = studio.story.alloc_id();
        }
        for panel in &mut chapter.panels {
            panel.id = studio.story.alloc_id();
        }
        studio.story.chapters.push(chapter);
        let preset_id = studio.story.chapters[0]
            .layers
            .iter()
            .find_map(|layer| match &layer.kind {
                LayerKind::Text { style, .. } => style.preset_id,
                _ => None,
            })
            .unwrap();
        let mut preset = studio
            .story
            .bubble_presets
            .iter()
            .find(|p| p.id == preset_id)
            .unwrap()
            .clone();
        let original = studio.story.clone();
        preset.style.font = FontChoice::Lato;
        preset.size = 64.0;
        studio.checkpoint();
        bubbles::update_preset(&mut studio.story, preset).unwrap();
        let edited = studio.story.clone();
        assert_ne!(edited.chapters[0], original.chapters[0]);
        assert_ne!(edited.chapters[1], original.chapters[1]);
        studio.undo();
        assert_eq!(studio.story, original);
        studio.redo();
        assert_eq!(studio.story, edited);
    }
}
