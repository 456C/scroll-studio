use super::*;
use egui::{pos2, vec2, Align2, FontId, Rect, Sense};
use scroll_studio::timeline;

#[derive(Clone, Copy)]
struct ChapterDrag(u64);
impl Studio {
    fn move_timeline(&mut self, id: u64, before: Option<u64>) {
        let mut next = self.story.clone();
        match timeline::move_before(&mut next, id, before) {
            Ok(true) => {
                self.checkpoint();
                self.story = next;
                self.changed();
            }
            Ok(false) => {}
            Err(e) => self.error = Some(e.to_string()),
        }
    }
    pub(super) fn timeline_workspace(&mut self, ctx: &egui::Context) {
        self.canvas_rect = Rect::NOTHING;
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG).inner_margin(22.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("Timeline");
                    ui.label(RichText::new("Chronology").color(MUTED))
                        .on_hover_text(
                            "Arrange the story's events without changing chapter reading order",
                        );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if !self.assets_only && primary(ui, "+ Chapter").clicked() {
                            self.dialog = Some(Dialog::Chapter);
                            self.dialog_name =
                                format!("Chapter {:02}", self.story.chapters.len() + 1);
                        }
                        motion::menu_button(ui, "•••", |ui| {
                            if motion::button(ui, "Use reading order").clicked() {
                                let old = timeline::entries(&self.story);
                                let next: Vec<_> = self
                                    .story
                                    .chapters
                                    .iter()
                                    .filter_map(|c| {
                                        old.iter().find(|e| e.chapter_id == c.id).cloned()
                                    })
                                    .collect();
                                if old != next {
                                    self.checkpoint();
                                    self.story.timeline = next;
                                    self.changed();
                                }
                                ui.close_menu();
                            }
                        });
                    });
                });
                ui.add_space(24.0);
                let entries = timeline::entries(&self.story);
                let selected_key = egui::Id::new(("timeline_selected", self.document_epoch));
                let mut selected = ctx
                    .data(|d| d.get_temp::<u64>(selected_key))
                    .filter(|id| entries.iter().any(|e| e.chapter_id == *id))
                    .unwrap_or(self.story.chapters[self.chapter].id);
                let mut reorder = None;
                let area = egui::ScrollArea::horizontal()
                    .id_salt(("timeline_board", self.document_epoch))
                    .max_height(274.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 22.0;
                            for (index, entry) in entries.iter().enumerate() {
                                let Some(chapter) = self
                                    .story
                                    .chapters
                                    .iter()
                                    .find(|c| c.id == entry.chapter_id)
                                else {
                                    continue;
                                };
                                let name = chapter.name.clone();
                                let cover = chapter
                                    .layers
                                    .iter()
                                    .filter(|_| !self.assets_only)
                                    .find_map(|l| {
                                        if let LayerKind::Image { asset_id, .. } = l.kind {
                                            l.visible.then_some(asset_id)
                                        } else {
                                            None
                                        }
                                    });
                                let (rect, response) = ui.allocate_exact_size(
                                    vec2(238.0, 246.0),
                                    Sense::click_and_drag(),
                                );
                                #[cfg(test)]
                                ctx.data_mut(|d| {
                                    d.insert_temp(
                                        egui::Id::new(("timeline_card", entry.chapter_id)),
                                        rect,
                                    )
                                });
                                response.dnd_set_drag_payload(ChapterDrag(entry.chapter_id));
                                if response.clicked() {
                                    selected = entry.chapter_id;
                                }
                                if !self.assets_only && response.double_clicked() {
                                    self.open_timeline_chapter(entry.chapter_id);
                                }
                                if ui.is_rect_visible(rect) {
                                    ui.painter().rect_filled(rect, 10.0, PANEL);
                                    if selected == entry.chapter_id {
                                        ui.painter().rect_stroke(
                                            rect,
                                            10.0,
                                            egui::Stroke::new(1.5_f32, ACCENT),
                                            egui::StrokeKind::Inside,
                                        );
                                    }
                                    let art = Rect::from_min_size(
                                        rect.min + vec2(10.0, 10.0),
                                        vec2(218.0, 132.0),
                                    );
                                    if let Some(id) = cover {
                                        self.paint_character_reference(ui, ctx, id, art, true);
                                    } else {
                                        icons::draw(
                                            ui.painter(),
                                            Rect::from_center_size(art.center(), vec2(44.0, 44.0)),
                                            icons::Icon::Chapter,
                                            MUTED,
                                        );
                                    }
                                    let painter = ui.painter().with_clip_rect(rect.shrink(12.0));
                                    painter.text(
                                        rect.min + vec2(14.0, 163.0),
                                        Align2::LEFT_CENTER,
                                        name,
                                        FontId::proportional(17.0),
                                        TEXT,
                                    );
                                    painter.text(
                                        rect.min + vec2(14.0, 192.0),
                                        Align2::LEFT_CENTER,
                                        if entry.when.is_empty() {
                                            "When…"
                                        } else {
                                            &entry.when
                                        },
                                        FontId::proportional(13.0),
                                        ACCENT,
                                    );
                                    painter.text(
                                        rect.min + vec2(14.0, 222.0),
                                        Align2::LEFT_CENTER,
                                        entry.summary.lines().next().unwrap_or(""),
                                        FontId::proportional(12.0),
                                        MUTED,
                                    );
                                }
                                let before = if ctx
                                    .pointer_hover_pos()
                                    .is_some_and(|p| p.x > rect.center().x)
                                {
                                    entries.get(index + 1).map(|e| e.chapter_id)
                                } else {
                                    Some(entry.chapter_id)
                                };
                                if response.dnd_hover_payload::<ChapterDrag>().is_some() {
                                    let x = if before == Some(entry.chapter_id) {
                                        rect.left() - 10.0
                                    } else {
                                        rect.right() + 10.0
                                    };
                                    ui.painter().line_segment(
                                        [pos2(x, rect.top()), pos2(x, rect.bottom())],
                                        egui::Stroke::new(3.0_f32, ACCENT),
                                    );
                                }
                                if let Some(payload) = response.dnd_release_payload::<ChapterDrag>()
                                {
                                    reorder = Some((payload.0, before));
                                }
                                appearance::context_menu(&response, |ui| {
                                    if !self.assets_only
                                        && motion::button(ui, "Open chapter").clicked()
                                    {
                                        self.open_timeline_chapter(entry.chapter_id);
                                        ui.close_menu();
                                    }
                                    if ui
                                        .add_enabled(index > 0, motion::Button::new("Earlier"))
                                        .clicked()
                                    {
                                        reorder = Some((
                                            entry.chapter_id,
                                            Some(entries[index - 1].chapter_id),
                                        ));
                                        ui.close_menu();
                                    }
                                    if ui
                                        .add_enabled(
                                            index + 1 < entries.len(),
                                            motion::Button::new("Later"),
                                        )
                                        .clicked()
                                    {
                                        reorder = Some((
                                            entry.chapter_id,
                                            entries.get(index + 2).map(|e| e.chapter_id),
                                        ));
                                        ui.close_menu();
                                    }
                                });
                            }
                        });
                    });
                if egui::DragAndDrop::has_payload_of_type::<ChapterDrag>(ctx) {
                    if let Some(pointer) = ctx
                        .pointer_hover_pos()
                        .filter(|p| area.inner_rect.contains(*p))
                    {
                        let speed = if pointer.x < area.inner_rect.left() + 35.0 {
                            -12.0
                        } else if pointer.x > area.inner_rect.right() - 35.0 {
                            12.0
                        } else {
                            0.0
                        };
                        if speed != 0.0 {
                            let mut state = area.state;
                            state.offset.x = (state.offset.x + speed).max(0.0);
                            state.store(ctx, area.id);
                            ctx.request_repaint();
                        }
                    }
                }
                if let Some((id, before)) = reorder {
                    self.move_timeline(id, before);
                    selected = id;
                }
                ctx.data_mut(|d| d.insert_temp(selected_key, selected));
                ui.add_space(20.0);
                if let Some(entry) = entries.iter().find(|e| e.chapter_id == selected) {
                    let mut updated = entry.clone();
                    let name = self
                        .story
                        .chapters
                        .iter()
                        .find(|c| c.id == selected)
                        .map_or("Chapter", |c| c.name.as_str())
                        .to_owned();
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(name).size(20.0));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if !self.assets_only && motion::button(ui, "Open chapter").clicked() {
                                self.open_timeline_chapter(selected);
                            }
                        });
                    });
                    ui.add_space(8.0);
                    ui.add(
                        egui::TextEdit::singleline(&mut updated.when)
                            .hint_text("When · e.g. Before the fall")
                            .char_limit(1024)
                            .desired_width(280.0),
                    );
                    ui.add_space(8.0);
                    ui.add_sized(
                        [
                            ui.available_width(),
                            ui.available_height().clamp(60.0, 180.0),
                        ],
                        egui::TextEdit::multiline(&mut updated.summary)
                            .hint_text("What happens in this chapter…")
                            .char_limit(20_000),
                    );
                    if updated != *entry {
                        let mut next = timeline::entries(&self.story);
                        if let Some(target) = next.iter_mut().find(|e| e.chapter_id == selected) {
                            *target = updated;
                        }
                        self.checkpoint();
                        self.story.timeline = next;
                        self.changed();
                    }
                }
            });
    }
    fn open_timeline_chapter(&mut self, id: u64) {
        if self.assets_only {
            return;
        }
        if let Some(index) = self.story.chapters.iter().position(|c| c.id == id) {
            self.chapter = index;
            self.tab = LibraryTab::Chapters;
            self.selected = None;
            self.selected_asset = None;
            self.active_panel = None;
            self.jump_y = Some(0.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame(ctx: &egui::Context, studio: &mut Studio, events: Vec<egui::Event>) {
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(1100.0, 720.0))),
                events,
                ..Default::default()
            },
            |ctx| {
                studio.library(ctx);
                studio.timeline_workspace(ctx);
            },
        );
    }
    fn button(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            pressed,
            button: egui::PointerButton::Primary,
            modifiers: egui::Modifiers::NONE,
        }
    }
    #[test]
    fn assets_mode_lore_and_timeline_never_open_chapters_or_change_story_art() {
        let ctx = egui::Context::default();
        let mut studio = Studio::from_context(&ctx);
        studio.story = Story::demo();
        studio.assets_only = true;
        studio.reset_view();
        let chapters = studio.story.chapters.clone();
        assert!(studio.tab == LibraryTab::Characters);
        studio.new_lore();
        let id = studio.selected_lore.unwrap();
        studio.undo();
        assert!(studio.story.lore.is_empty());
        studio.redo();
        assert_eq!(studio.story.lore[0].id, id);
        studio.tab = LibraryTab::Timeline;
        studio.open_timeline_chapter(chapters[0].id);
        assert!(studio.tab == LibraryTab::Timeline);
        assert_eq!(studio.story.chapters, chapters);
    }

    #[test]
    fn dragging_chapter_cards_reorders_chronology_with_one_undo_and_preserves_reading_order() {
        let ctx = egui::Context::default();
        let mut studio = Studio::from_context(&ctx);
        studio.welcome = false;
        studio.tab = LibraryTab::Timeline;
        for name in ["Second", "Third"] {
            let id = studio.story.alloc_id();
            studio
                .story
                .chapters
                .push(Chapter::new(id, name, 800, 1200));
        }
        let original = studio.story.clone();
        let first = studio.story.chapters[0].id;
        let second = studio.story.chapters[1].id;
        for _ in 0..3 {
            frame(&ctx, &mut studio, vec![]);
        }
        let card = |id| {
            ctx.data(|d| {
                d.get_temp::<Rect>(egui::Id::new(("timeline_card", id)))
                    .unwrap()
            })
        };
        let from = card(first).center();
        let to = card(second).right_center() - vec2(22.0, 0.0);
        frame(
            &ctx,
            &mut studio,
            vec![egui::Event::PointerMoved(from), button(from, true)],
        );
        frame(
            &ctx,
            &mut studio,
            vec![egui::Event::PointerMoved(from + vec2(30.0, 0.0))],
        );
        frame(&ctx, &mut studio, vec![egui::Event::PointerMoved(to)]);
        frame(&ctx, &mut studio, vec![button(to, false)]);
        assert_eq!(studio.story.timeline[0].chapter_id, second);
        assert_eq!(studio.story.timeline[1].chapter_id, first);
        assert_eq!(studio.story.chapters, original.chapters);
        assert_eq!(studio.undo.len(), 1);
        studio.undo();
        assert_eq!(studio.story, original);
        studio.redo();
        assert_eq!(studio.story.timeline[0].chapter_id, second);
    }
}
