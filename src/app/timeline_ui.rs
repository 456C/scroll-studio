use super::*;
use egui::{pos2, vec2, Align2, FontId, Rect, Sense};
use scroll_studio::model::{ChronologyEntry, StoryEventKind};
use scroll_studio::timeline;

#[derive(Clone, Copy)]
struct ItemDrag(u64);

impl Studio {
    pub(super) fn move_timeline(&mut self, id: u64, before: Option<u64>) {
        // A move that changes nothing leaves history alone, as before.
        if let Err(e) = self.apply_command(Command::MoveChronology {
            chapter: id,
            before,
        }) {
            self.error = Some(e.to_string());
        }
    }

    fn add_timeline_event(&mut self, ctx: &egui::Context, after: Option<u64>) -> Option<u64> {
        let mut next = self.story.clone();
        match timeline::insert_event(&mut next, after) {
            Ok(id) => {
                self.checkpoint();
                self.story = next;
                self.changed();
                if let Some(entry) = self.story.timeline.iter().find(|entry| entry.key() == id) {
                    let draft_key = egui::Id::new(("timeline_note_draft", self.document_epoch));
                    ctx.data_mut(|data| data.insert_temp(draft_key, entry.clone()));
                }
                self.status = "Event added".into();
                Some(id)
            }
            Err(error) => {
                self.error = Some(error.to_string());
                None
            }
        }
    }

    fn delete_timeline_event(&mut self, ctx: &egui::Context, id: u64) {
        let mut next = self.story.clone();
        match timeline::delete_event(&mut next, id) {
            Ok(true) => {
                self.checkpoint();
                self.story = next;
                self.changed();
                let draft_key = egui::Id::new(("timeline_note_draft", self.document_epoch));
                ctx.data_mut(|data| {
                    if data
                        .get_temp::<ChronologyEntry>(draft_key)
                        .is_some_and(|entry| entry.key() == id)
                    {
                        data.remove::<ChronologyEntry>(draft_key);
                    }
                });
                self.status = "Event removed".into();
            }
            Ok(false) => {}
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    pub(super) fn timeline_workspace(&mut self, ctx: &egui::Context) {
        self.canvas_rect = Rect::NOTHING;
        let selected_key = egui::Id::new(("timeline_selected", self.document_epoch));
        let draft_key = egui::Id::new(("timeline_note_draft", self.document_epoch));
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG).inner_margin(22.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("Timeline");
                    ui.label(RichText::new("Chronology").color(MUTED))
                        .on_hover_text(
                            "Arrange chapters and in-world moments without changing reading order",
                        );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if !self.assets_only && primary(ui, "+ Chapter").clicked() {
                            self.dialog = Some(Dialog::Chapter);
                            self.dialog_name =
                                format!("Chapter {:02}", self.story.chapters.len() + 1);
                        }
                        let add_event = motion::button(ui, "+ Event");
                        #[cfg(test)]
                        ctx.data_mut(|data| {
                            data.insert_temp(egui::Id::new("timeline_add_event"), add_event.rect)
                        });
                        if add_event.clicked() {
                            let after = ctx
                                .data(|data| data.get_temp::<u64>(selected_key))
                                .or_else(|| {
                                    self.story
                                        .chapters
                                        .get(self.chapter)
                                        .map(|chapter| chapter.id)
                                });
                            if let Some(id) = self.add_timeline_event(ctx, after) {
                                ctx.data_mut(|data| data.insert_temp(selected_key, id));
                            }
                        }
                        motion::menu_button(ui, "•••", |ui| {
                            if motion::button(ui, "Use reading order").clicked() {
                                let old = timeline::entries(&self.story);
                                let next = timeline::reading_order(&self.story);
                                if old != next {
                                    self.commit(
                                        Some(Command::OrderChronologyByReading),
                                        |studio| studio.story.timeline = next,
                                    );
                                }
                                ui.close_menu();
                            }
                        });
                    });
                });
                ui.add_space(24.0);
                let entries = timeline::entries(&self.story);
                let mut selected = ctx
                    .data(|data| data.get_temp::<u64>(selected_key))
                    .filter(|id| entries.iter().any(|entry| entry.key() == *id))
                    .unwrap_or(self.story.chapters[self.chapter].id);
                let mut reorder = None;
                let mut remove_event = None;
                let mut card_rects = Vec::new();
                let area = egui::ScrollArea::horizontal()
                    .id_salt(("timeline_board", self.document_epoch))
                    .max_height(274.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 22.0;
                            for (index, entry) in entries.iter().enumerate() {
                                if !entry.is_event()
                                    && !self
                                        .story
                                        .chapters
                                        .iter()
                                        .any(|chapter| chapter.id == entry.chapter_id)
                                {
                                    continue;
                                }
                                let id = entry.key();
                                let title = timeline_label(&self.story, entry);
                                let cover = if entry.is_event() || self.assets_only {
                                    None
                                } else {
                                    self.story
                                        .chapters
                                        .iter()
                                        .find(|chapter| chapter.id == entry.chapter_id)
                                        .and_then(|chapter| {
                                            chapter.layers.iter().find_map(|layer| {
                                                if let LayerKind::Image { asset_id, .. } =
                                                    layer.kind
                                                {
                                                    layer.visible.then_some(asset_id)
                                                } else {
                                                    None
                                                }
                                            })
                                        })
                                };
                                let (rect, response) = ui.allocate_exact_size(
                                    vec2(238.0, 246.0),
                                    Sense::click_and_drag(),
                                );
                                #[cfg(test)]
                                ctx.data_mut(|data| {
                                    data.insert_temp(egui::Id::new(("timeline_card", id)), rect)
                                });
                                card_rects.push((id, rect));
                                response
                                    .clone()
                                    .on_hover_text("Drag to reorder · Right-click for more");
                                response.dnd_set_drag_payload(ItemDrag(id));
                                if response.clicked() {
                                    selected = id;
                                }
                                if response.double_clicked() {
                                    if entry.is_event() {
                                        ctx.data_mut(|data| {
                                            data.insert_temp(draft_key, entry.clone())
                                        });
                                    } else if !self.assets_only {
                                        self.open_timeline_chapter(entry.chapter_id);
                                    }
                                }
                                if ui.is_rect_visible(rect) {
                                    ui.painter().rect_filled(rect, 10.0, PANEL);
                                    let track_y = rect.bottom() - 7.0;
                                    ui.painter().line_segment(
                                        [
                                            pos2(rect.left(), track_y),
                                            pos2(rect.right() + 22.0, track_y),
                                        ],
                                        egui::Stroke::new(1.0_f32, Color32::from_gray(68)),
                                    );
                                    ui.painter().circle_filled(
                                        pos2(rect.left() + 14.0, track_y),
                                        3.0,
                                        if selected == id { ACCENT } else { MUTED },
                                    );
                                    ui.painter().text(
                                        rect.min + vec2(14.0, 20.0),
                                        Align2::LEFT_CENTER,
                                        format!("{:02}", index + 1),
                                        FontId::proportional(12.0),
                                        MUTED,
                                    );
                                    for x in [rect.right() - 22.0, rect.right() - 15.0] {
                                        for y in [
                                            rect.top() + 14.0,
                                            rect.top() + 21.0,
                                            rect.top() + 28.0,
                                        ] {
                                            ui.painter().circle_filled(pos2(x, y), 1.4, MUTED);
                                        }
                                    }
                                    if selected == id {
                                        ui.painter().rect_stroke(
                                            rect,
                                            10.0,
                                            egui::Stroke::new(1.5_f32, ACCENT),
                                            egui::StrokeKind::Inside,
                                        );
                                    }
                                    let art = Rect::from_min_size(
                                        rect.min + vec2(10.0, 36.0),
                                        vec2(218.0, 106.0),
                                    );
                                    if let Some(event) = &entry.event {
                                        ui.painter().text(
                                            art.center(),
                                            Align2::CENTER_CENTER,
                                            event.kind.label(),
                                            FontId::proportional(18.0),
                                            event_color(event.kind),
                                        );
                                    } else if let Some(asset_id) = cover {
                                        self.paint_character_reference(
                                            ui, ctx, asset_id, art, true,
                                        );
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
                                        &title,
                                        FontId::proportional(17.0),
                                        TEXT,
                                    );
                                    painter.text(
                                        rect.min + vec2(14.0, 192.0),
                                        Align2::LEFT_CENTER,
                                        if entry.when.is_empty() {
                                            "Undated"
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
                                let event = entry.is_event();
                                appearance::context_menu(&response, |ui| {
                                    if event && motion::button(ui, "Edit").clicked() {
                                        ctx.data_mut(|data| {
                                            data.insert_temp(draft_key, entry.clone())
                                        });
                                        ui.close_menu();
                                    }
                                    if !event
                                        && !self.assets_only
                                        && motion::button(ui, "Open chapter").clicked()
                                    {
                                        self.open_timeline_chapter(entry.chapter_id);
                                        ui.close_menu();
                                    }
                                    if event && motion::button(ui, "Delete").clicked() {
                                        remove_event = Some(id);
                                        ui.close_menu();
                                    }
                                    if ui
                                        .add_enabled(index > 0, motion::Button::new("Earlier"))
                                        .clicked()
                                    {
                                        reorder = Some((id, Some(entries[index - 1].key())));
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
                                            id,
                                            entries.get(index + 2).map(|entry| entry.key()),
                                        ));
                                        ui.close_menu();
                                    }
                                });
                            }
                        });
                    });
                if let Some(payload) = egui::DragAndDrop::payload::<ItemDrag>(ctx) {
                    if let Some(pointer) = ctx
                        .pointer_hover_pos()
                        .filter(|pointer| area.inner_rect.contains(*pointer))
                    {
                        let remaining: Vec<_> = card_rects
                            .iter()
                            .filter(|(card_id, _)| *card_id != payload.0)
                            .collect();
                        let next = remaining
                            .iter()
                            .find(|(_, rect)| pointer.x < rect.center().x);
                        let before = next.map(|(card_id, _)| *card_id);
                        let x = next.map_or_else(
                            || {
                                remaining
                                    .last()
                                    .map_or(area.inner_rect.left(), |(_, rect)| rect.right() + 11.0)
                            },
                            |(_, rect)| rect.left() - 11.0,
                        );
                        let painter = ui.painter().with_clip_rect(area.inner_rect);
                        painter.line_segment(
                            [
                                pos2(x, area.inner_rect.top() + 6.0),
                                pos2(x, area.inner_rect.bottom() - 8.0),
                            ],
                            egui::Stroke::new(3.0_f32, ACCENT),
                        );
                        painter.circle_filled(pos2(x, area.inner_rect.top() + 6.0), 4.0, ACCENT);
                        if let Some((_, rect)) =
                            card_rects.iter().find(|(card_id, _)| *card_id == payload.0)
                        {
                            painter.rect_filled(*rect, 10.0, Color32::from_black_alpha(155));
                        }
                        let ghost =
                            Rect::from_min_size(pointer + vec2(14.0, -38.0), vec2(190.0, 66.0));
                        let painter = ctx.layer_painter(egui::LayerId::new(
                            egui::Order::Tooltip,
                            egui::Id::new("timeline_drag_preview"),
                        ));
                        painter.rect_filled(
                            ghost.translate(vec2(0.0, 5.0)),
                            10.0,
                            Color32::from_black_alpha(70),
                        );
                        painter.rect_filled(ghost, 10.0, PANEL);
                        painter.rect_stroke(
                            ghost,
                            10.0,
                            egui::Stroke::new(1.5_f32, ACCENT),
                            egui::StrokeKind::Inside,
                        );
                        let label = entries
                            .iter()
                            .find(|entry| entry.key() == payload.0)
                            .map(|entry| timeline_label(&self.story, entry))
                            .unwrap_or_default();
                        painter.with_clip_rect(ghost.shrink(12.0)).text(
                            ghost.left_center() + vec2(12.0, 0.0),
                            Align2::LEFT_CENTER,
                            label,
                            FontId::proportional(16.0),
                            TEXT,
                        );
                        if ctx.input(|input| input.pointer.primary_released()) {
                            reorder = Some((payload.0, before));
                            egui::DragAndDrop::clear_payload(ctx);
                        } else {
                            let speed = if pointer.x < area.inner_rect.left() + 40.0 {
                                -1.0
                            } else if pointer.x > area.inner_rect.right() - 40.0 {
                                1.0
                            } else {
                                0.0
                            };
                            if speed != 0.0 {
                                let mut state = area.state;
                                let step = ui.input(|input| input.stable_dt.min(0.05)) * 650.0;
                                state.offset.x = (state.offset.x + speed * step).clamp(
                                    0.0,
                                    (area.content_size.x - area.inner_rect.width()).max(0.0),
                                );
                                state.store(ctx, area.id);
                                ctx.request_repaint();
                            }
                        }
                    }
                }
                if let Some((id, before)) = reorder {
                    self.move_timeline(id, before);
                    selected = id;
                }
                ui.add_space(20.0);
                if let Some(entry) = entries
                    .iter()
                    .find(|entry| entry.key() == selected)
                    .cloned()
                {
                    let title = timeline_label(&self.story, &entry);
                    ui.horizontal(|ui| {
                        ui.add(egui::Label::new(RichText::new(title).size(20.0)).truncate());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if motion::button(
                                ui,
                                if entry.is_event() {
                                    "Edit"
                                } else {
                                    "Edit notes"
                                },
                            )
                            .clicked()
                            {
                                ctx.data_mut(|data| data.insert_temp(draft_key, entry.clone()));
                            }
                            if entry.is_event() {
                                let remove = motion::button(ui, "Delete");
                                #[cfg(test)]
                                ctx.data_mut(|data| {
                                    data.insert_temp(
                                        egui::Id::new("timeline_delete_event"),
                                        remove.rect,
                                    )
                                });
                                if remove.clicked() {
                                    remove_event = Some(entry.key());
                                }
                            }
                            let index = entries
                                .iter()
                                .position(|item| item.key() == selected)
                                .unwrap_or(0);
                            if ui
                                .add_enabled(
                                    index + 1 < entries.len(),
                                    motion::Button::new("Later ›"),
                                )
                                .clicked()
                            {
                                self.move_timeline(
                                    selected,
                                    entries.get(index + 2).map(|item| item.key()),
                                );
                            }
                            if ui
                                .add_enabled(index > 0, motion::Button::new("‹ Earlier"))
                                .clicked()
                            {
                                self.move_timeline(selected, Some(entries[index - 1].key()));
                            }
                            if !entry.is_event()
                                && !self.assets_only
                                && motion::button(ui, "Open chapter").clicked()
                            {
                                self.open_timeline_chapter(selected);
                            }
                        });
                    });
                    ui.add_space(8.0);
                    if let Some(event) = &entry.event {
                        ui.label(RichText::new(event.kind.label()).color(event_color(event.kind)));
                        let links = event
                            .links
                            .iter()
                            .filter_map(|id| link_name(&self.story, *id))
                            .collect::<Vec<_>>()
                            .join("   ·   ");
                        if !links.is_empty() {
                            ui.label(RichText::new(links).color(MUTED));
                        }
                    }
                    ui.label(
                        RichText::new(if entry.when.is_empty() {
                            "Undated"
                        } else {
                            entry.when.as_str()
                        })
                        .color(ACCENT),
                    );
                    egui::ScrollArea::vertical()
                        .id_salt(("timeline_notes", selected))
                        .max_height(ui.available_height())
                        .show(ui, |ui| {
                            ui.add(egui::Label::new(&entry.summary).wrap().selectable(true));
                        });
                }
                if let Some(id) = remove_event {
                    let fallback = entries
                        .iter()
                        .map(|entry| entry.key())
                        .find(|key| *key != id);
                    self.delete_timeline_event(ctx, id);
                    if selected == id {
                        selected = fallback.unwrap_or(self.story.chapters[self.chapter].id);
                    }
                }
                ctx.data_mut(|data| data.insert_temp(selected_key, selected));
            });
        let draft_key = egui::Id::new(("timeline_note_draft", self.document_epoch));
        if let Some(mut draft) = ctx.data(|data| data.get_temp::<ChronologyEntry>(draft_key)) {
            let mut save = false;
            let mut cancel = false;
            let choices = event_link_choices(&self.story);
            let modal = egui::Modal::new(draft_key).show(ctx, |ui| {
                ui.set_width(440.0);
                if draft.is_event() {
                    ui.heading("Story event");
                } else {
                    ui.heading("Timeline notes");
                }
                ui.add_space(12.0);
                if let Some(event) = draft.event.as_mut() {
                    ui.add(
                        egui::TextEdit::singleline(&mut event.title)
                            .hint_text("Title")
                            .desired_width(f32::INFINITY)
                            .char_limit(timeline::MAX_TITLE_BYTES),
                    );
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        for kind in StoryEventKind::ALL {
                            ui.selectable_value(&mut event.kind, kind, kind.label());
                        }
                    });
                    ui.add_space(8.0);
                }
                let when_hint = if draft.is_event() {
                    "In-world date or era"
                } else {
                    "Date or era"
                };
                let summary_hint = if draft.is_event() {
                    "What happens in this moment…"
                } else {
                    "What happens in this chapter…"
                };
                ui.add(
                    egui::TextEdit::singleline(&mut draft.when)
                        .hint_text(when_hint)
                        .desired_width(f32::INFINITY)
                        .char_limit(1024),
                );
                ui.add_space(8.0);
                egui::ScrollArea::vertical()
                    .max_height(180.0)
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut draft.summary)
                                .hint_text(summary_hint)
                                .desired_width(f32::INFINITY)
                                .desired_rows(6)
                                .char_limit(20_000),
                        );
                    });
                if draft.is_event() && !choices.is_empty() {
                    ui.add_space(8.0);
                    egui::ScrollArea::vertical()
                        .max_height(140.0)
                        .show(ui, |ui| {
                            for (id, label) in &choices {
                                let links = &mut draft.event.as_mut().unwrap().links;
                                let mut on = links.contains(id);
                                if ui.checkbox(&mut on, label).changed() {
                                    if on {
                                        if !links.contains(id) {
                                            links.push(*id);
                                        }
                                    } else {
                                        links.retain(|link| link != id);
                                    }
                                }
                            }
                        });
                }
                ui.horizontal(|ui| {
                    cancel = motion::button(ui, "Cancel").clicked();
                    save = primary(ui, "Save").clicked();
                });
            });
            if save {
                let mut next = timeline::entries(&self.story);
                if let Some(entry) = next.iter_mut().find(|entry| entry.key() == draft.key()) {
                    if *entry != draft {
                        let command = Command::SetChronologyEntry {
                            chapter: draft.chapter_id,
                            when: draft.when.clone(),
                            summary: draft.summary.clone(),
                        };
                        *entry = draft;
                        self.commit(Some(command), |studio| studio.story.timeline = next);
                    }
                }
            } else if !cancel && !modal.should_close() {
                ctx.data_mut(|data| data.insert_temp(draft_key, draft));
                return;
            }
            ctx.data_mut(|data| data.remove::<ChronologyEntry>(draft_key));
        }
    }

    fn open_timeline_chapter(&mut self, id: u64) {
        if self.assets_only {
            return;
        }
        if let Some(index) = self
            .story
            .chapters
            .iter()
            .position(|chapter| chapter.id == id)
        {
            self.chapter = index;
            self.tab = LibraryTab::Chapters;
            self.selected = None;
            self.selected_asset = None;
            self.active_panel = None;
            self.jump_y = Some(0.0);
        }
    }
}

fn timeline_label(story: &Story, entry: &ChronologyEntry) -> String {
    if let Some(event) = &entry.event {
        if event.title.trim().is_empty() {
            "Untitled event".into()
        } else {
            event.title.clone()
        }
    } else {
        story
            .chapters
            .iter()
            .find(|chapter| chapter.id == entry.chapter_id)
            .map(|chapter| chapter.name.clone())
            .unwrap_or_else(|| "Chapter".into())
    }
}

fn link_name(story: &Story, id: u64) -> Option<String> {
    event_link_choices(story)
        .into_iter()
        .find(|(choice, _)| *choice == id)
        .map(|(_, label)| label)
}

fn event_link_choices(story: &Story) -> Vec<(u64, String)> {
    let mut choices = Vec::new();
    for asset in story
        .assets
        .iter()
        .filter(|asset| asset.kind == AssetKind::Character)
    {
        choices.push((asset.id, format!("Character · {}", asset.name)));
    }
    for location in &story.locations {
        choices.push((location.id, format!("Location · {}", location.name)));
    }
    for group in &story.groups {
        choices.push((group.id, format!("Group · {}", group.name)));
    }
    choices
}

fn event_color(kind: StoryEventKind) -> Color32 {
    match kind {
        StoryEventKind::Flashback => Color32::from_rgb(222, 176, 96),
        StoryEventKind::Lore => Color32::from_rgb(214, 186, 122),
        StoryEventKind::Other => ACCENT,
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
            ctx.data(|data| {
                data.get_temp::<Rect>(egui::Id::new(("timeline_card", id)))
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
    #[test]
    fn dropping_in_a_gap_and_cancelling_a_drag_do_not_focus_text() {
        let ctx = egui::Context::default();
        let mut studio = Studio::from_context(&ctx);
        studio.welcome = false;
        studio.assets_only = true;
        studio.tab = LibraryTab::Timeline;
        for name in ["Second", "Third"] {
            let id = studio.story.alloc_id();
            studio
                .story
                .chapters
                .push(Chapter::new(id, name, 800, 1200));
        }
        let ids: Vec<_> = studio
            .story
            .chapters
            .iter()
            .map(|chapter| chapter.id)
            .collect();
        for _ in 0..3 {
            frame(&ctx, &mut studio, vec![]);
        }
        let rect = |id| {
            ctx.data(|data| {
                data.get_temp::<Rect>(egui::Id::new(("timeline_card", id)))
                    .unwrap()
            })
        };
        let from = rect(ids[0]).center();
        let gap = pos2(rect(ids[1]).right() + 8.0, from.y);
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
        frame(&ctx, &mut studio, vec![egui::Event::PointerMoved(gap)]);
        frame(&ctx, &mut studio, vec![button(gap, false)]);
        assert_eq!(
            timeline::entries(&studio.story)
                .iter()
                .map(|entry| entry.chapter_id)
                .collect::<Vec<_>>(),
            vec![ids[1], ids[0], ids[2]]
        );
        assert!(!ctx.wants_keyboard_input());
        assert_eq!(studio.undo.len(), 1);
        let before = studio.story.clone();
        egui::DragAndDrop::set_payload(&ctx, ItemDrag(ids[0]));
        frame(
            &ctx,
            &mut studio,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        frame(&ctx, &mut studio, vec![button(gap, false)]);
        assert_eq!(studio.story, before);
        assert_eq!(studio.undo.len(), 1);
    }

    #[test]
    fn adding_an_event_places_it_after_the_selection_and_undoes_in_one_step() {
        let ctx = egui::Context::default();
        let mut studio = Studio::from_context(&ctx);
        studio.welcome = false;
        studio.tab = LibraryTab::Timeline;
        let first = studio.story.chapters[0].id;
        for _ in 0..3 {
            frame(&ctx, &mut studio, vec![]);
        }
        let rect = ctx.data(|data| {
            data.get_temp::<Rect>(egui::Id::new("timeline_add_event"))
                .unwrap()
        });
        let original = studio.story.clone();
        frame(
            &ctx,
            &mut studio,
            vec![
                egui::Event::PointerMoved(rect.center()),
                button(rect.center(), true),
            ],
        );
        frame(&ctx, &mut studio, vec![button(rect.center(), false)]);
        let items = timeline::entries(&studio.story);
        assert_eq!(items[0].chapter_id, first);
        assert!(items[1].is_event());
        assert_eq!(items[1].event.as_ref().unwrap().kind, StoryEventKind::Other);
        assert_eq!(studio.undo.len(), 1);
        studio.undo();
        assert_eq!(studio.story, original);
    }

    #[test]
    fn events_can_be_reordered_and_deleted_from_the_timeline() {
        let ctx = egui::Context::default();
        let mut studio = Studio::from_context(&ctx);
        studio.welcome = false;
        studio.tab = LibraryTab::Timeline;
        let first = studio.story.chapters[0].id;
        let second = studio.story.alloc_id();
        studio
            .story
            .chapters
            .push(Chapter::new(second, "Later", 800, 1200));
        let event = timeline::insert_event(&mut studio.story, Some(first)).unwrap();
        studio
            .story
            .timeline
            .iter_mut()
            .find(|entry| entry.key() == event)
            .unwrap()
            .event
            .as_mut()
            .unwrap()
            .title = "The fire".into();
        let original = studio.story.clone();
        for _ in 0..3 {
            frame(&ctx, &mut studio, vec![]);
        }
        let card = |id| {
            ctx.data(|data| {
                data.get_temp::<Rect>(egui::Id::new(("timeline_card", id)))
                    .unwrap()
            })
        };
        let from = card(event).center();
        let to = card(first).left_center() + vec2(8.0, 0.0);
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
        assert_eq!(timeline::entries(&studio.story)[0].key(), event);
        assert_eq!(studio.undo.len(), 1);
        frame(&ctx, &mut studio, vec![]);
        let remove = ctx.data(|data| {
            data.get_temp::<Rect>(egui::Id::new("timeline_delete_event"))
                .unwrap()
        });
        frame(
            &ctx,
            &mut studio,
            vec![
                egui::Event::PointerMoved(remove.center()),
                button(remove.center(), true),
            ],
        );
        frame(&ctx, &mut studio, vec![button(remove.center(), false)]);
        assert!(timeline::entries(&studio.story)
            .iter()
            .all(|entry| !entry.is_event()));
        studio.undo();
        assert_eq!(timeline::entries(&studio.story)[0].key(), event);
        studio.undo();
        assert_eq!(
            timeline::entries(&studio.story)
                .iter()
                .map(|entry| entry.key())
                .collect::<Vec<_>>(),
            timeline::entries(&original)
                .iter()
                .map(|entry| entry.key())
                .collect::<Vec<_>>()
        );
    }
}
