use super::*;
use scroll_studio::dialogue::{self, DialogueBlock};

const PASSAGE_HEIGHT: f32 = 70.0;
const ROW_HEIGHT: f32 = PASSAGE_HEIGHT + 24.0;

/// Review state belongs to this document, never to the saved story.
pub(super) struct DialogueDraft {
    document_epoch: u64,
    chapter_id: u64,
    beat_id: u64,
    preset_id: u64,
    blocks: Vec<DialogueBlock>,
    error: Option<String>,
    reveal_block: Option<usize>,
}

impl Studio {
    pub(super) fn begin_dialogue_setup(&mut self, beat_id: u64) {
        let Some(chapter) = self.story.chapters.get(self.chapter) else {
            return;
        };
        let Some(beat) = chapter.panels.iter().find(|beat| beat.id == beat_id) else {
            return;
        };
        let Some(preset) = self
            .story
            .bubble_presets
            .iter()
            .find(|preset| Some(preset.id) == self.selected_bubble_preset)
            .or_else(|| self.story.bubble_presets.first())
        else {
            self.error = Some("Create a preset in Bubbles before adding dialogue.".into());
            return;
        };
        match dialogue::script_blocks(&beat.script) {
            Ok(blocks) => {
                self.dialogue_draft = Some(DialogueDraft {
                    document_epoch: self.document_epoch,
                    chapter_id: chapter.id,
                    beat_id,
                    preset_id: preset.id,
                    blocks,
                    error: None,
                    reveal_block: None,
                })
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    pub(super) fn dialogue_setup_ui(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.dialogue_draft.take() else {
            return;
        };
        if draft.document_epoch != self.document_epoch {
            return;
        }
        let mut create = false;
        let mut cancel = false;
        let mut edited = false;
        let modal = egui::Modal::new(egui::Id::new("dialogue_setup")).show(ctx, |ui| {
            ui.set_width((ctx.screen_rect().width() - 64.0).clamp(300.0, 560.0));
            ui.heading("Make bubbles");
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label("Preset");
                egui::ComboBox::from_id_salt("dialogue_preset")
                    .width(220.0)
                    .truncate()
                    .selected_text(
                        self.story
                            .bubble_presets
                            .iter()
                            .find(|preset| preset.id == draft.preset_id)
                            .map_or("Choose preset", |preset| preset.name.as_str()),
                    )
                    .show_ui(ui, |ui| {
                        ui.set_max_width(300.0);
                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                        for preset in &self.story.bubble_presets {
                            edited |= ui
                                .selectable_value(&mut draft.preset_id, preset.id, &preset.name)
                                .changed();
                        }
                    });
            });
            ui.add_space(10.0);
            let reveal_block = draft.reveal_block.take();
            let mut area = egui::ScrollArea::vertical().id_salt((
                "dialogue_blocks",
                draft.document_epoch,
                draft.beat_id,
            ));
            if let Some(index) = reveal_block {
                area = area.vertical_scroll_offset(
                    index as f32 * (ROW_HEIGHT + ui.spacing().item_spacing.y),
                );
            }
            area.max_height((ctx.screen_rect().height() - 280.0).clamp(120.0, 400.0))
                .auto_shrink([false, true])
                .show_rows(ui, ROW_HEIGHT, draft.blocks.len(), |ui, rows| {
                    for index in rows {
                        let block = &mut draft.blocks[index];
                        ui.push_id(index, |ui| {
                            egui::Frame::new()
                                .fill(CARD)
                                .corner_radius(6.0)
                                .inner_margin(10.0)
                                .show(ui, |ui| {
                                    ui.horizontal_top(|ui| {
                                        let include = ui.checkbox(
                                            &mut block.included,
                                            format!("{:02}", index + 1),
                                        );
                                        edited |= include.changed();
                                        #[cfg(test)]
                                        record_control(ctx, ("include", index), &include);
                                        let width = (ui.available_width() - 10.0).max(100.0);
                                        ui.allocate_ui_with_layout(
                                            egui::vec2(width, PASSAGE_HEIGHT),
                                            egui::Layout::top_down(egui::Align::Min),
                                            |ui| {
                                                egui::ScrollArea::vertical()
                                                    .id_salt("passage")
                                                    .max_height(PASSAGE_HEIGHT)
                                                    .auto_shrink([false, false])
                                                    .show(ui, |ui| {
                                                        let text = ui.add(
                                                            egui::TextEdit::multiline(
                                                                &mut block.text,
                                                            )
                                                            .hint_text("Dialogue")
                                                            .desired_width(f32::INFINITY)
                                                            .desired_rows(3)
                                                            .char_limit(dialogue::MAX_BLOCK_BYTES),
                                                        );
                                                        if reveal_block == Some(index) {
                                                            text.request_focus();
                                                        }
                                                        edited |= text.changed();
                                                        #[cfg(test)]
                                                        record_control(ctx, ("text", index), &text);
                                                    });
                                            },
                                        );
                                    });
                                });
                        });
                    }
                });
            if edited {
                draft.error = None;
            }
            if let Some(error) = &draft.error {
                ui.add_space(6.0);
                ui.colored_label(Color32::LIGHT_RED, error);
            }
            ui.add_space(12.0);
            let count = draft
                .blocks
                .iter()
                .filter(|block| block.included && !block.text.trim().is_empty())
                .count();
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        draft.blocks.len() < dialogue::MAX_BLOCKS,
                        motion::Button::new("+ Bubble"),
                    )
                    .clicked()
                {
                    draft.blocks.push(DialogueBlock {
                        included: true,
                        text: String::new(),
                    });
                    draft.reveal_block = Some(draft.blocks.len() - 1);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let valid_preset = self
                        .story
                        .bubble_presets
                        .iter()
                        .any(|preset| preset.id == draft.preset_id);
                    let submit = ui.add_enabled(
                        count > 0 && valid_preset,
                        motion::Button::new(
                            RichText::new(format!(
                                "Create {count} {}",
                                if count == 1 { "bubble" } else { "bubbles" }
                            ))
                            .color(BG),
                        )
                        .fill(ACCENT)
                        .min_size(egui::vec2(160.0, 32.0)),
                    );
                    create = submit.clicked();
                    let close = motion::button(ui, "Cancel");
                    cancel = close.clicked();
                    #[cfg(test)]
                    {
                        record_control(ctx, ("create", 0), &submit);
                        record_control(ctx, ("cancel", 0), &close);
                    }
                });
            });
        });
        #[cfg(test)]
        ctx.data_mut(|data| {
            data.insert_temp(egui::Id::new("dialogue_modal_rect"), modal.response.rect)
        });
        if cancel || modal.should_close() {
            return;
        }
        if create {
            match self.commit_dialogue(&draft) {
                Ok(()) => return,
                Err(error) => draft.error = Some(error.to_string()),
            }
        }
        self.dialogue_draft = Some(draft);
    }

    fn commit_dialogue(&mut self, draft: &DialogueDraft) -> anyhow::Result<()> {
        anyhow::ensure!(
            draft.document_epoch == self.document_epoch,
            "This story is no longer open"
        );
        let prepared = dialogue::prepare_layers(
            &self.story,
            draft.chapter_id,
            draft.beat_id,
            draft.preset_id,
            &draft.blocks,
        )?;
        let selected = prepared.layers.first().map(|layer| layer.id);
        let count = prepared.layers.len();
        self.checkpoint();
        self.story.next_id = prepared.next_id;
        self.story.chapters[prepared.chapter_index]
            .layers
            .extend(prepared.layers);
        self.chapter = prepared.chapter_index;
        self.selected = selected;
        self.selected_asset = None;
        self.selected_bubble_preset = Some(draft.preset_id);
        self.active_panel = Some(draft.beat_id);
        self.script_lane.reveal = Some(draft.beat_id);
        self.tool = Tool::Move;
        self.tab = LibraryTab::Chapters;
        self.jump_y = self.story.chapters[self.chapter]
            .panels
            .iter()
            .find(|beat| beat.id == draft.beat_id)
            .map(|beat| beat.y.max(0.0));
        self.changed();
        self.status = format!("{count} bubbles created");
        Ok(())
    }
}

#[cfg(test)]
fn record_control(ctx: &egui::Context, key: (&str, usize), response: &egui::Response) {
    ctx.data_mut(|data| {
        data.insert_temp(
            egui::Id::new(("dialogue_control", key)),
            (response.rect, response.enabled()),
        )
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(ctx: &egui::Context, script: &str) -> (Studio, u64) {
        let mut studio = Studio::from_context(ctx);
        studio.story = Story::new("Dialogue review");
        studio.story.chapters[0].panels[0].script = script.into();
        let second = Chapter::new(studio.story.alloc_id(), "Untouched chapter", 800, 6400);
        studio.story.chapters.push(second);
        studio.welcome = false;
        studio.error = None;
        studio.dirty = false;
        let beat = studio.story.chapters[0].panels[0].id;
        (studio, beat)
    }

    fn frame(
        ctx: &egui::Context,
        studio: &mut Studio,
        size: egui::Vec2,
        density: f32,
        events: Vec<egui::Event>,
    ) {
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            events,
            ..Default::default()
        };
        input
            .viewports
            .entry(egui::ViewportId::ROOT)
            .or_default()
            .native_pixels_per_point = Some(density);
        let _ = ctx.run(input, |ctx| studio.dialogue_setup_ui(ctx));
    }

    fn control(ctx: &egui::Context, label: &str, index: usize) -> (egui::Rect, bool) {
        ctx.data(|data| data.get_temp(egui::Id::new(("dialogue_control", (label, index)))))
            .unwrap()
    }

    fn click(pos: egui::Pos2) -> Vec<egui::Event> {
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            },
        ]
    }

    #[test]
    fn review_checkbox_text_edit_and_create_make_one_undoable_batch() {
        let ctx = egui::Context::default();
        let (mut studio, beat) = fixture(&ctx, "MIRA: Hi.\n\n[The door opens.]\n\nKAI: Hello.");
        let before = studio.story.clone();
        let size = egui::vec2(1100.0, 720.0);
        studio.begin_dialogue_setup(beat);
        frame(&ctx, &mut studio, size, 1.0, vec![]);
        frame(&ctx, &mut studio, size, 1.0, vec![]);
        let include = control(&ctx, "include", 1).0;
        frame(&ctx, &mut studio, size, 1.0, click(include.center()));
        assert!(!studio.dialogue_draft.as_ref().unwrap().blocks[1].included);
        let editor = control(&ctx, "text", 2).0;
        frame(&ctx, &mut studio, size, 1.0, click(editor.center()));
        frame(
            &ctx,
            &mut studio,
            size,
            1.0,
            vec![
                egui::Event::Key {
                    key: egui::Key::A,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::COMMAND,
                },
                egui::Event::Text("A revised reply.\nStill the same bubble.".into()),
            ],
        );
        assert_eq!(
            studio.dialogue_draft.as_ref().unwrap().blocks[2].text,
            "A revised reply.\nStill the same bubble."
        );
        assert_eq!(studio.story, before);
        assert!(!studio.dirty && studio.undo.is_empty());
        let create = control(&ctx, "create", 0).0;
        frame(&ctx, &mut studio, size, 1.0, click(create.center()));
        assert!(studio.dialogue_draft.is_none());
        assert_eq!(studio.undo.len(), 1);
        assert_eq!(studio.story.chapters[0].layers.len(), 2);
        let mut expected = before.clone();
        expected.chapters[0].layers = studio.story.chapters[0].layers.clone();
        expected.next_id += 2;
        assert_eq!(studio.story, expected);
        let texts: Vec<_> = studio.story.chapters[0]
            .layers
            .iter()
            .map(|layer| {
                let LayerKind::Text { text, .. } = &layer.kind else {
                    panic!("Expected dialogue")
                };
                text.as_str()
            })
            .collect();
        assert_eq!(
            texts,
            ["MIRA: Hi.", "A revised reply.\nStill the same bubble."]
        );
        assert!(studio.dirty);
        studio.undo();
        assert_eq!(studio.story, before);
    }

    #[test]
    fn short_review_shrinks_while_keeping_three_passages_and_footer_accessible() {
        let ctx = egui::Context::default();
        let (mut studio, beat) = fixture(&ctx, "Wait.\n\nI'm coming.\n\n[The train leaves.]");
        let size = egui::vec2(1100.0, 720.0);
        studio.begin_dialogue_setup(beat);
        for _ in 0..3 {
            frame(&ctx, &mut studio, size, 1.0, vec![]);
        }
        let bounds: egui::Rect = ctx
            .data(|data| data.get_temp(egui::Id::new("dialogue_modal_rect")))
            .unwrap();
        assert!(
            bounds.height() < 460.0,
            "Short review should not fill the screen: {bounds:?}"
        );
        for index in 0..3 {
            let text = control(&ctx, "text", index).0;
            assert!(bounds.contains_rect(text));
            assert!(text.height() >= 48.0, "Keep a usable multi-line editor");
        }
        assert!(bounds.contains_rect(control(&ctx, "create", 0).0));
    }

    #[test]
    fn long_review_keeps_footer_visible_and_cancel_preserves_story_at_desktop_sizes() {
        for (size, density) in [
            (egui::vec2(1100.0, 720.0), 1.0),
            (egui::vec2(1280.0, 760.0), 1.0),
            (egui::vec2(2560.0 / 1.5, 1440.0 / 1.5), 1.5),
        ] {
            let ctx = egui::Context::default();
            let script = (0..100)
                .map(|index| format!("MIRA: Passage {index}. {}", "A long line.\n".repeat(50)))
                .collect::<Vec<_>>()
                .join("\n\n");
            let (mut studio, beat) = fixture(&ctx, &script);
            studio.story.bubble_presets[0].name = "A very long preset name ".repeat(100);
            let before = studio.story.clone();
            studio.begin_dialogue_setup(beat);
            frame(&ctx, &mut studio, size, density, vec![]);
            frame(&ctx, &mut studio, size, density, vec![]);
            let bounds: egui::Rect = ctx
                .data(|data| data.get_temp(egui::Id::new("dialogue_modal_rect")))
                .unwrap();
            let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
            assert!(
                screen.contains_rect(bounds),
                "Review modal exceeds screen at {size:?}: {bounds:?}"
            );
            let create = control(&ctx, "create", 0).0;
            let cancel = control(&ctx, "cancel", 0).0;
            assert!(bounds.contains_rect(create) && bounds.contains_rect(cancel));
            assert!(create.width() >= 150.0 && create.height() >= 32.0);
            assert!(
                ctx.data(|data| data.get_temp::<(egui::Rect, bool)>(egui::Id::new((
                    "dialogue_control",
                    ("text", 50)
                ))))
                .is_none(),
                "Review list should only render visible rows"
            );
            frame(&ctx, &mut studio, size, density, click(cancel.center()));
            assert!(studio.dialogue_draft.is_none());
            assert_eq!(studio.story, before);
            assert!(!studio.dirty && studio.undo.is_empty());
        }
    }

    #[test]
    fn stale_preset_and_overflow_cannot_commit_or_create_history() {
        let ctx = egui::Context::default();
        let (mut studio, beat) = fixture(&ctx, "A passage.");
        studio.begin_dialogue_setup(beat);
        let mut draft = studio.dialogue_draft.take().unwrap();
        draft.preset_id = 9999;
        let before = studio.story.clone();
        assert!(studio.commit_dialogue(&draft).is_err());
        assert_eq!(studio.story, before);
        assert!(studio.undo.is_empty());
        draft.preset_id = studio.story.bubble_presets[0].id;
        studio.story.chapters[0].height = 64;
        let before = studio.story.clone();
        assert!(studio.commit_dialogue(&draft).is_err());
        assert_eq!(studio.story, before);
        assert!(studio.undo.is_empty());
        studio.document_epoch += 1;
        assert!(studio.commit_dialogue(&draft).is_err());
        assert_eq!(studio.story, before);
    }
}
