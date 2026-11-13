use super::*;
use egui::{pos2, vec2, Rect, Sense, Stroke, StrokeKind};

#[derive(Clone, Copy)]
struct NextBeat {
    y: f32,
    height: f32,
    canvas_height: u32,
}

fn next_beat(chapter: &Chapter) -> Option<NextBeat> {
    if chapter.panels.len() >= 10_000 {
        return None;
    }
    let gap = chapter.width as f32 * 0.125;
    let y = if chapter.panels.is_empty() {
        gap.min(chapter.height as f32 - 1.0)
    } else {
        chapter
            .panels
            .iter()
            .map(|beat| beat.y + beat.height)
            .fold(0.0_f32, f32::max)
            + gap
    };
    let height = (chapter.width as f32 * 0.875).clamp(300.0, 2800.0);
    let end = (y + height + gap).ceil();
    if !end.is_finite() || end > 200_000.0 {
        return None;
    }
    Some(NextBeat {
        y,
        height,
        canvas_height: chapter.height.max(end as u32),
    })
}

fn card_height(height: f32, zoom: f32) -> f32 {
    (height * zoom - 14.0).clamp(260.0, 460.0)
}

// Cached geometry contains no script strings or image data. Viewing a group never
// sorts or changes the chapter itself.
#[derive(Clone, Copy, Debug)]
struct BeatAnchor {
    id: u64,
    source: usize,
    y: f32,
    height: f32,
}

#[derive(Clone, Copy, Debug)]
struct BeatGroup {
    first: usize,
    last: usize,
    top: f32,
    bottom: f32,
}

impl BeatGroup {
    fn rect(self, canvas: Rect, viewport: Rect) -> Rect {
        let height = (self.bottom - self.top)
            .min(if self.last > self.first { 320.0 } else { 460.0 })
            .min(viewport.height().max(1.0));
        let natural = canvas.top() + self.top;
        let bottom = canvas.top() + self.bottom;
        let top = natural.max(viewport.top()).min(bottom - height);
        Rect::from_min_size(pos2(canvas.right() + 24.0, top), vec2(264.0, height))
    }
}

#[derive(Default)]
pub(super) struct ScriptLane {
    key: Option<(u64, u64, u64, u32)>,
    anchors: Vec<BeatAnchor>,
    groups: Vec<BeatGroup>,
    chosen: Option<(u64, f32)>,
    pub(super) reveal: Option<u64>,
}

impl ScriptLane {
    fn prepare(&mut self, chapter: &Chapter, epoch: u64, revision: u64, zoom: f32) {
        let key = (epoch, chapter.id, revision, zoom.to_bits());
        if self.key == Some(key) {
            return;
        }
        if self
            .key
            .is_some_and(|old| (old.0, old.1) != (epoch, chapter.id))
        {
            self.chosen = None;
        }
        self.key = Some(key);
        self.anchors.clear();
        self.anchors.extend(
            chapter
                .panels
                .iter()
                .enumerate()
                .map(|(source, p)| BeatAnchor {
                    id: p.id,
                    source,
                    y: p.y,
                    height: p.height,
                }),
        );
        self.anchors
            .sort_by(|a, b| a.y.total_cmp(&b.y).then(a.id.cmp(&b.id)));
        self.groups.clear();
        for (index, beat) in self.anchors.iter().enumerate() {
            let top = beat.y * zoom;
            let bottom = top + card_height(beat.height, zoom).max(beat.height * zoom);
            if let Some(group) = self
                .groups
                .last_mut()
                .filter(|group| top < group.bottom + 12.0)
            {
                group.last = index;
                group.bottom = group.bottom.max(bottom);
            } else {
                self.groups.push(BeatGroup {
                    first: index,
                    last: index,
                    top,
                    bottom,
                });
            }
        }
    }

    fn follow(&mut self, scroll_y: f32, zoom: f32, focus: Option<u64>) {
        if self
            .chosen
            .is_some_and(|(_, y)| (scroll_y - y).abs() * zoom > 2.0)
        {
            self.chosen = None;
        }
        if let Some(id) = focus.or(self.reveal.take()) {
            self.chosen = Some((id, scroll_y));
        }
    }

    fn selected(&self, group: BeatGroup, focus_y: f32) -> usize {
        let anchors = &self.anchors[group.first..=group.last];
        if let Some((id, _)) = self.chosen {
            if let Some(index) = anchors.iter().position(|beat| beat.id == id) {
                return group.first + index;
            }
        }
        group.first
            + anchors
                .partition_point(|beat| beat.y <= focus_y)
                .saturating_sub(1)
    }
}

impl Studio {
    pub(super) fn add_beat(&mut self, y: f32) {
        if !y.is_finite() || self.story.chapters[self.chapter].panels.len() >= 10_000 {
            return;
        }
        let ch = &self.story.chapters[self.chapter];
        let chapter = ch.id;
        let y = y.clamp(0.0, ch.height as f32 - 1.0);
        let draft = commands::PanelDraft {
            title: format!("Beat {:02}", ch.panels.len() + 1),
            y,
            height: 1400.0_f32.min(ch.height as f32 - y),
            script: String::new(),
            notes: String::new(),
            prompt: String::new(),
        };
        let command = Command::Batch(vec![
            Command::AddPanel {
                chapter,
                after: None,
                panel: draft.clone(),
            },
            Command::SortPanels { chapter },
        ]);
        let mut fallback_id = None;
        let applied = self.commit(Some(command), |studio| {
            let id = studio.story.alloc_id();
            let ch = &mut studio.story.chapters[studio.chapter];
            ch.panels.push(Panel {
                id,
                title: draft.title,
                y: draft.y,
                height: draft.height,
                script: String::new(),
                notes: String::new(),
                prompt: String::new(),
            });
            ch.panels.sort_by(|a, b| a.y.total_cmp(&b.y));
            fallback_id = Some(id);
        });
        self.active_panel = applied
            .and_then(|a| a.created.first().copied())
            .or(fallback_id);
    }

    pub(super) fn script_timeline(&mut self, ui: &mut egui::Ui, canvas: Rect) {
        self.prepare_script_lane();
        if self
            .script_focus
            .is_some_and(|id| !self.script_lane.anchors.iter().any(|beat| beat.id == id))
        {
            self.script_focus = None;
        }
        self.script_lane
            .follow(self.scroll_y, self.zoom, self.script_focus);
        let mut viewport = ui.clip_rect();
        viewport.min.y = viewport.top().max(self.toolbar_rect.bottom() + 12.0);
        viewport.max.y -= 8.0;
        let focus_y = (viewport.top() + 100.0 - canvas.top()) / self.zoom;
        // Only the visible, selected editors own text copies. Group navigation
        // keeps its header fixed while long dialogue scrolls inside the card.
        let visible: Vec<_> = self
            .script_lane
            .groups
            .iter()
            .copied()
            .filter_map(|group| {
                let rect = group.rect(canvas, viewport);
                if !rect.intersects(viewport) {
                    return None;
                }
                let index = self.script_lane.selected(group, focus_y);
                Some((group, index, self.script_lane.anchors[index], rect))
            })
            .collect();
        for (group, index, anchor, rect) in visible {
            let Some(mut panel) = self.story.chapters[self.chapter]
                .panels
                .get(anchor.source)
                .filter(|panel| panel.id == anchor.id)
                .or_else(|| {
                    self.story.chapters[self.chapter]
                        .panels
                        .iter()
                        .find(|panel| panel.id == anchor.id)
                })
                .cloned()
            else {
                continue;
            };
            let height = rect.height();
            let active = focus_y >= group.top / self.zoom && focus_y <= group.bottom / self.zoom;
            let top = canvas.top() + panel.y * self.zoom;
            ui.painter().with_clip_rect(viewport).add(egui::Shape::line(
                vec![
                    pos2(canvas.right() + 4.0, top),
                    pos2(rect.left() - 10.0, top),
                    pos2(rect.left() - 10.0, rect.top() + 26.0),
                    pos2(rect.left(), rect.top() + 26.0),
                ],
                Stroke::new(1.0_f32, if active { ACCENT } else { MUTED }),
            ));
            #[cfg(test)]
            ui.data_mut(|data| {
                data.insert_temp(egui::Id::new(("beat_card_bounds", panel.id)), rect)
            });
            let mut navigate = None;
            let original = panel.clone();
            let mut remove = false;
            ui.scope_builder(
                egui::UiBuilder::new()
                    .id_salt(("script_beat", self.document_epoch, panel.id))
                    .max_rect(rect),
                |ui| {
                    ui.set_clip_rect(viewport.intersect(rect));
                    // Section labels are chrome, not selectable document text.
                    // Let the card backdrop own their secondary clicks.
                    ui.style_mut().interaction.selectable_labels = false;
                    // Register the backdrop first, behind the editable content.
                    // Upgrading Frame::response after its children steals their
                    // primary clicks as well as secondary clicks.
                    let beat_response =
                        ui.interact(rect, ui.id().with("beat_context"), Sense::click());
                    egui::Frame::new()
                        .fill(if active { CARD } else { PANEL })
                        .stroke(Stroke::new(
                            1.0_f32,
                            if active {
                                ACCENT.gamma_multiply(0.65)
                            } else {
                                CARD
                            },
                        ))
                        .corner_radius(9.0)
                        .inner_margin(14.0)
                        .show(ui, |ui| {
                            ui.set_width(234.0);
                            ui.set_min_height(height - 28.0);
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(format!("BEAT {:02}", index + 1))
                                        .size(10.0)
                                        .strong()
                                        .color(if active { ACCENT } else { MUTED }),
                                );
                                if active && group.first == group.last {
                                    ui.label(RichText::new("IN VIEW").size(9.0).color(ACCENT));
                                }
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        let menu = motion::menu_button(ui, "···", |ui| {
                                            if ui
                                                .add_enabled(
                                                    !panel.script.trim().is_empty(),
                                                    motion::Button::new("Make bubbles…"),
                                                )
                                                .clicked()
                                            {
                                                self.begin_dialogue_setup(panel.id);
                                                ui.close_menu();
                                            }
                                            self.assist_menu_item(
                                                ui,
                                                serde_json::json!({ "kind": "panel", "id": panel.id }),
                                            );
                                            ui.separator();
                                            ui.label("Position on canvas");
                                            ui.add(
                                                egui::DragValue::new(&mut panel.y)
                                                    .range(
                                                        0.0..=self.story.chapters[self.chapter]
                                                            .height
                                                            as f32,
                                                    )
                                                    .clamp_existing_to_range(false)
                                                    .suffix(" px"),
                                            );
                                            ui.label("Beat height");
                                            ui.add(
                                                egui::DragValue::new(&mut panel.height)
                                                    .range(1.0..=10000.0)
                                                    .clamp_existing_to_range(false)
                                                    .suffix(" px"),
                                            );
                                            if motion::button(ui, "Remove beat").clicked() {
                                                remove = true;
                                                ui.close_menu();
                                            }
                                        });
                                        #[cfg(test)]
                                        ui.data_mut(|data| {
                                            data.insert_temp(
                                                egui::Id::new(("beat_menu_bounds", panel.id)),
                                                menu.response.rect,
                                            )
                                        });
                                        #[cfg(not(test))]
                                        let _ = menu;
                                        if group.last > group.first {
                                            ui.spacing_mut().item_spacing.x = 3.0;
                                            let next = ui
                                                .add_enabled_ui(index < group.last, |ui| {
                                                    icons::small_button(
                                                        ui,
                                                        icons::Icon::ChevronRight,
                                                        false,
                                                        "Next beat",
                                                    )
                                                })
                                                .inner;
                                            if next.clicked() {
                                                navigate = Some(index + 1);
                                            }
                                            let picker = motion::menu_button(
                                                ui,
                                                RichText::new(format!(
                                                    "{}/{}",
                                                    index - group.first + 1,
                                                    group.last - group.first + 1
                                                ))
                                                .size(12.0),
                                                |ui| {
                                                    ui.set_width(220.0);
                                                    egui::ScrollArea::vertical()
                                                        .max_height(280.0)
                                                        .show_rows(
                                                            ui,
                                                            28.0,
                                                            group.last - group.first + 1,
                                                            |ui, rows| {
                                                                for offset in rows {
                                                                    let at = group.first + offset;
                                                                    let beat = self
                                                                        .script_lane
                                                                        .anchors[at];
                                                                    let title = self.story.chapters
                                                                        [self.chapter]
                                                                        .panels
                                                                        .get(beat.source)
                                                                        .filter(|p| p.id == beat.id)
                                                                        .map(|p| p.title.trim())
                                                                        .unwrap_or("");
                                                                    let title = if title.is_empty()
                                                                    {
                                                                        format!(
                                                                            "Beat {:02}",
                                                                            at + 1
                                                                        )
                                                                    } else {
                                                                        format!(
                                                                            "{:02}  {title}",
                                                                            at + 1
                                                                        )
                                                                    };
                                                                    if ui
                                                                        .add_sized(
                                                                            [220.0, 28.0],
                                                                            egui::Button::new(
                                                                                RichText::new(
                                                                                    title,
                                                                                )
                                                                                .size(13.0),
                                                                            )
                                                                            .selected(at == index)
                                                                            .truncate(),
                                                                        )
                                                                        .clicked()
                                                                    {
                                                                        navigate = Some(at);
                                                                        ui.close_menu();
                                                                    }
                                                                }
                                                            },
                                                        );
                                                },
                                            );
                                            #[cfg(test)]
                                            ui.data_mut(|data| {
                                                data.insert_temp(
                                                    egui::Id::new(("beat_picker_bounds", panel.id)),
                                                    picker.response.rect,
                                                )
                                            });
                                            picker.response.on_hover_text("Choose beat");
                                            let previous = ui
                                                .add_enabled_ui(index > group.first, |ui| {
                                                    icons::small_button(
                                                        ui,
                                                        icons::Icon::ChevronLeft,
                                                        false,
                                                        "Previous beat",
                                                    )
                                                })
                                                .inner;
                                            if previous.clicked() {
                                                navigate = Some(index - 1);
                                            }
                                        }
                                    },
                                );
                            });
                            egui::ScrollArea::vertical()
                                .id_salt(("beat", panel.id))
                                .max_height((height - 66.0).max(1.0))
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    ui.add(
                                        egui::TextEdit::multiline(&mut panel.title)
                                            .frame(false)
                                            .hint_text("Title")
                                            .font(egui::FontId::proportional(18.0))
                                            .desired_rows(1)
                                            .desired_width(f32::INFINITY),
                                    );
                                    ui.add_space(6.0);
                                    let script = ui.add(
                                        egui::TextEdit::multiline(&mut panel.script)
                                            .hint_text("Dialogue or action…")
                                            .desired_width(f32::INFINITY)
                                            .desired_rows(4)
                                            .frame(false),
                                    );
                                    if self.script_focus == Some(panel.id) {
                                        script.request_focus();
                                        self.script_focus = None;
                                    }
                                    #[cfg(test)]
                                    ui.data_mut(|data| {
                                        data.insert_temp(
                                            egui::Id::new(("beat_script_bounds", panel.id)),
                                            script.rect,
                                        )
                                    });
                                    #[cfg(not(test))]
                                    let _ = script;
                                    ui.add_space(6.0);
                                    ui.separator();
                                    eyebrow(ui, "NOTES");
                                    ui.add(
                                        egui::TextEdit::multiline(&mut panel.notes)
                                            .hint_text("Notes…")
                                            .desired_width(f32::INFINITY)
                                            .desired_rows(2)
                                            .frame(false),
                                    );
                                    ui.collapsing("AI prompt", |ui| {
                                        ui.add(
                                            egui::TextEdit::multiline(&mut panel.prompt)
                                                .hint_text("Scene prompt…")
                                                .desired_width(f32::INFINITY)
                                                .desired_rows(3),
                                        );
                                        if motion::copy_button(
                                            ui,
                                            ("copy_panel_prompt", self.document_epoch, panel.id),
                                            "Copy prompt",
                                            false,
                                        )
                                        .clicked()
                                        {
                                            ui.ctx().copy_text(
                                                scroll_studio::characters::with_story_style(
                                                    &self.story.visual_style,
                                                    &panel.prompt,
                                                ),
                                            );
                                            self.status = "Prompt copied".into();
                                        }
                                    });
                                });
                        });
                    self.register_context(&beat_response);
                    appearance::context_menu(&beat_response, |ui| {
                        self.beat_context_actions(ui, panel.id);
                    });
                },
            );
            if let Some(index) = navigate {
                let id = self.script_lane.anchors[index].id;
                self.script_lane.chosen = Some((id, self.scroll_y));
                self.active_panel = Some(id);
                ui.ctx().request_repaint();
            }
            if remove {
                let id = panel.id;
                self.commit(Some(Command::DeletePanel { panel: id }), |studio| {
                    studio.story.chapters[studio.chapter]
                        .panels
                        .retain(|p| p.id != id)
                });
            } else if panel != original {
                let command = commands::panel_edit(&original, &panel);
                self.commit(command, |studio| {
                    if let Some(p) = studio.story.chapters[studio.chapter]
                        .panels
                        .iter_mut()
                        .find(|p| p.id == panel.id)
                    {
                        *p = panel;
                    }
                });
            }
        }
        self.next_beat_control(ui, canvas);
    }

    // Includes the bounded append action, even when the last beat reaches the canvas edge.
    pub(super) fn script_lane_height(&mut self) -> f32 {
        self.prepare_script_lane();
        self.next_beat_top() + 44.0
    }

    fn prepare_script_lane(&mut self) {
        self.script_lane.prepare(
            &self.story.chapters[self.chapter],
            self.document_epoch,
            self.revision,
            self.zoom,
        );
    }

    fn next_beat_top(&self) -> f32 {
        let chapter = &self.story.chapters[self.chapter];
        let natural = next_beat(chapter).map_or(chapter.height as f32, |next| next.y) * self.zoom;
        self.script_lane
            .groups
            .last()
            .map_or(natural, |group| natural.max(group.bottom + 16.0))
            .min(200_000.0 * self.zoom + 476.0)
    }

    fn next_beat_control(&mut self, ui: &mut egui::Ui, canvas: Rect) {
        let top = canvas.top() + self.next_beat_top();
        let rect = Rect::from_min_size(pos2(canvas.right() + 84.0, top), vec2(144.0, 36.0));
        if !rect.intersects(ui.clip_rect()) {
            return;
        }
        let candidate = next_beat(&self.story.chapters[self.chapter]);
        let response = ui
            .scope_builder(
                egui::UiBuilder::new()
                    .id_salt((
                        "next_beat",
                        self.document_epoch,
                        self.story.chapters[self.chapter].id,
                    ))
                    .max_rect(rect)
                    .layout(egui::Layout::centered_and_justified(
                        egui::Direction::TopDown,
                    )),
                |ui| {
                    ui.add_enabled(
                        candidate.is_some(),
                        motion::Button::new(RichText::new("+ Beat").size(14.0).color(ACCENT))
                            .min_size(rect.size()),
                    )
                },
            )
            .inner;
        #[cfg(test)]
        ui.data_mut(|data| {
            data.insert_temp(
                egui::Id::new("next_beat_control"),
                (response.rect, response.enabled()),
            )
        });
        if let Some(next) = candidate {
            if response.hovered() || response.has_focus() {
                let target = Rect::from_min_size(
                    pos2(canvas.left(), canvas.top() + next.y * self.zoom),
                    vec2(canvas.width(), next.height * self.zoom),
                );
                let painter = ui.painter().with_clip_rect(ui.clip_rect());
                painter.rect_filled(target, 0.0, ACCENT.gamma_multiply(0.1));
                painter.rect_stroke(
                    target,
                    0.0,
                    Stroke::new(1.0_f32, ACCENT),
                    StrokeKind::Inside,
                );
                painter.line_segment(
                    [target.right_top(), response.rect.left_center()],
                    Stroke::new(1.0_f32, ACCENT),
                );
                #[cfg(test)]
                ui.data_mut(|data| data.insert_temp(egui::Id::new("next_beat_preview"), target));
            }
            if response.clicked() {
                self.append_beat(next);
                ui.ctx().request_repaint();
            }
            if next.canvas_height > self.story.chapters[self.chapter].height {
                response.on_hover_text(format!("Extends the canvas to {} px", next.canvas_height));
            }
        } else {
            response.on_disabled_hover_text("Chapter limit reached");
        }
    }

    fn append_beat(&mut self, next: NextBeat) {
        // Recompute before committing so a preview cannot create stale geometry.
        let Some(current) = next_beat(&self.story.chapters[self.chapter]) else {
            return;
        };
        if current.y != next.y
            || current.height != next.height
            || current.canvas_height != next.canvas_height
        {
            return;
        }
        let chapter = self.story.chapters[self.chapter].id;
        let mut parts = Vec::new();
        if self.story.chapters[self.chapter].height != next.canvas_height {
            parts.push(Command::SetChapterHeight {
                chapter,
                height: next.canvas_height,
            });
        }
        parts.push(Command::AddPanel {
            chapter,
            after: None,
            panel: commands::PanelDraft {
                title: String::new(),
                y: next.y,
                height: next.height,
                script: String::new(),
                notes: String::new(),
                prompt: String::new(),
            },
        });
        parts.push(Command::SortPanels { chapter });
        let mut fallback_id = None;
        let applied = self.commit(Some(Command::Batch(parts)), |studio| {
            let id = studio.story.alloc_id();
            let chapter = &mut studio.story.chapters[studio.chapter];
            chapter.height = next.canvas_height;
            chapter.panels.push(Panel {
                id,
                title: String::new(),
                y: next.y,
                height: next.height,
                script: String::new(),
                notes: String::new(),
                prompt: String::new(),
            });
            chapter.panels.sort_by(|a, b| a.y.total_cmp(&b.y));
            fallback_id = Some(id);
        });
        let id = applied
            .and_then(|a| a.created.first().copied())
            .or(fallback_id)
            .expect("a panel was added");
        self.active_panel = Some(id);
        self.script_focus = Some(id);
        self.jump_y = Some(next.y);
        self.tool = Tool::Move;
        self.status = "Beat added".into();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(ctx: &egui::Context) -> Studio {
        let mut studio = Studio::from_context(ctx);
        studio.story = Story::new("Script review");
        studio.story.chapters[0].panels.clear();
        studio.welcome = false;
        studio.error = None;
        studio.dirty = false;
        studio.show_script = true;
        studio.auto_fit = true;
        studio
    }

    fn frame(
        ctx: &egui::Context,
        studio: &mut Studio,
        size: egui::Vec2,
        density: f32,
        events: Vec<egui::Event>,
    ) {
        let mut input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, size)),
            events,
            ..Default::default()
        };
        input
            .viewports
            .entry(egui::ViewportId::ROOT)
            .or_default()
            .native_pixels_per_point = Some(density);
        let mut native = eframe::Frame::_new_kittest();
        let _ = ctx.run(input, |ctx| eframe::App::update(studio, ctx, &mut native));
    }

    fn button(ctx: &egui::Context) -> (Rect, bool) {
        ctx.data(|data| data.get_temp(egui::Id::new("next_beat_control")))
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
    fn empty_lane_has_a_bounded_preview_and_opens_directly_into_writing() {
        for (size, density) in [
            (vec2(1100.0, 720.0), 1.0),
            (vec2(1280.0, 760.0), 1.0),
            (vec2(1707.0, 960.0), 1.5),
            (vec2(2560.0, 1440.0), 1.0),
        ] {
            let ctx = egui::Context::default();
            let mut studio = fixture(&ctx);
            let original = studio.story.clone();
            for _ in 0..3 {
                frame(&ctx, &mut studio, size, density, vec![]);
            }
            let (control, enabled) = button(&ctx);
            assert!(enabled && Rect::from_min_size(egui::Pos2::ZERO, size).contains_rect(control));
            assert!(control.width() <= 150.0 && control.height() <= 40.0);
            frame(
                &ctx,
                &mut studio,
                size,
                density,
                vec![egui::Event::PointerMoved(control.center())],
            );
            let preview: Rect = ctx
                .data(|data| data.get_temp(egui::Id::new("next_beat_preview")))
                .unwrap();
            assert_eq!(preview.width(), studio.canvas_rect.width());
            assert_eq!(studio.story, original);
            assert!(studio.undo.is_empty() && !studio.dirty);
            frame(&ctx, &mut studio, size, density, click(control.center()));
            assert_eq!(studio.story.chapters[0].panels.len(), 1);
            assert_eq!(studio.undo.len(), 1);
            let added = studio.story.clone();
            for _ in 0..2 {
                frame(&ctx, &mut studio, size, density, vec![]);
            }
            frame(
                &ctx,
                &mut studio,
                size,
                density,
                vec![egui::Event::Text("Wait for me.".into())],
            );
            assert_eq!(studio.story.chapters[0].panels[0].script, "Wait for me.");
            studio.undo();
            assert_eq!(studio.story, added);
            studio.undo();
            assert_eq!(studio.story, original);
        }
    }

    #[test]
    fn appending_after_the_last_range_extends_once_and_preserves_existing_content() {
        let ctx = egui::Context::default();
        let mut studio = fixture(&ctx);
        studio.story.chapters[0].height = 1600;
        studio.add_beat(500.0);
        studio.story.chapters[0].panels[0].script = "Existing dialogue.".into();
        studio.undo.clear();
        let second = Chapter::new(studio.story.alloc_id(), "Other chapter", 800, 2000);
        studio.story.chapters.push(second);
        let original = studio.story.clone();
        let next = next_beat(&studio.story.chapters[0]).unwrap();
        assert_eq!(
            (next.y, next.height, next.canvas_height),
            (1800.0, 1400.0, 3400)
        );
        studio.append_beat(next);
        assert_eq!(studio.undo.len(), 1);
        assert_eq!(studio.story.chapters[0].height, 3400);
        assert_eq!(
            studio.story.chapters[0].panels[0],
            original.chapters[0].panels[0]
        );
        assert_eq!(studio.story.chapters[1], original.chapters[1]);
        let added = studio.story.clone();
        studio.append_beat(next);
        assert_eq!(
            studio.story, added,
            "A stale placement must not append twice"
        );
        studio.undo();
        assert_eq!(studio.story, original);
        studio.redo();
        assert_eq!(studio.story, added);
        studio.story.chapters[0].panels[1].y = 199_000.0;
        assert!(next_beat(&studio.story.chapters[0]).is_none());
        let before_invalid = studio.story.clone();
        studio.add_beat(f32::NAN);
        assert_eq!(studio.story, before_invalid);
    }

    #[test]
    fn long_script_lanes_only_build_visible_editors_and_can_reach_the_end() {
        let ctx = egui::Context::default();
        let mut studio = fixture(&ctx);
        studio.story.chapters[0].height = 200_000;
        for i in 0..1000 {
            let id = studio.story.alloc_id();
            studio.story.chapters[0].panels.push(Panel {
                id,
                title: format!("Scene {i}"),
                y: i as f32 * 200.0,
                height: 180.0,
                script: "Dialogue. ".repeat(100),
                notes: String::new(),
                prompt: String::new(),
            });
        }
        let last = studio.story.chapters[0].panels.last().unwrap().id;
        let size = vec2(1280.0, 760.0);
        for _ in 0..3 {
            frame(&ctx, &mut studio, size, 1.0, vec![]);
        }
        assert!(ctx
            .data(|data| data.get_temp::<Rect>(egui::Id::new(("beat_script_bounds", last))))
            .is_none());
        assert!(!studio.dirty && studio.undo.is_empty());
        studio.jump_y = Some(199_400.0);
        studio.script_lane.reveal = Some(last);
        for _ in 0..3 {
            frame(&ctx, &mut studio, size, 1.0, vec![]);
        }
        assert!(ctx
            .data(|data| data.get_temp::<Rect>(egui::Id::new(("beat_script_bounds", last))))
            .is_some());
        assert!(
            !button(&ctx).1,
            "No next beat may exceed the chapter height limit"
        );
        assert!(!studio.dirty && studio.undo.is_empty());
    }

    #[test]
    fn opening_options_preserves_short_and_large_saved_ranges() {
        for height in [1.0, 15_000.0] {
            let ctx = egui::Context::default();
            let mut studio = fixture(&ctx);
            studio.add_beat(100.0);
            studio.story.chapters[0].panels[0].height = height;
            studio.dirty = false;
            studio.undo.clear();
            let original = studio.story.clone();
            let id = studio.story.chapters[0].panels[0].id;
            let size = vec2(1100.0, 720.0);
            for _ in 0..3 {
                frame(&ctx, &mut studio, size, 1.0, vec![]);
            }
            let menu: Rect = ctx
                .data(|data| data.get_temp(egui::Id::new(("beat_menu_bounds", id))))
                .unwrap();
            frame(&ctx, &mut studio, size, 1.0, click(menu.center()));
            frame(&ctx, &mut studio, size, 1.0, vec![]);
            assert_eq!(studio.story, original);
            assert!(!studio.dirty && studio.undo.is_empty());
        }
    }
    fn dense_fixture(ctx: &egui::Context) -> Studio {
        let mut studio = fixture(ctx);
        for i in 0..9 {
            let id = studio.story.alloc_id();
            studio.story.chapters[0].panels.push(Panel {
                id,
                title: format!("Scene {}", i + 1),
                y: if i < 8 {
                    200.0 + i as f32 * 130.0
                } else {
                    5000.0
                },
                height: if i == 4 { 1.0 } else { 80.0 },
                script: format!("Dialogue {}", i + 1),
                notes: "Keep this note".into(),
                prompt: "Scene reference".into(),
            });
        }
        studio
    }

    #[test]
    fn dense_groups_keep_readable_nonoverlapping_cards_and_original_positions() {
        let ctx = egui::Context::default();
        let mut studio = dense_fixture(&ctx);
        studio.story.chapters[0].panels.swap(0, 7);
        let original = studio.story.clone();
        for zoom in [0.05, 0.164, 0.276, 1.0, 4.0] {
            let mut lane = ScriptLane::default();
            lane.prepare(&studio.story.chapters[0], 0, 0, zoom);
            assert_eq!(lane.anchors.len(), 9);
            assert!(lane.anchors.windows(2).all(|pair| pair[0].y <= pair[1].y));
            for scroll in [0.0, 300.0, 800.0, 1500.0] {
                let canvas = Rect::from_min_size(
                    pos2(40.0, 180.0 - scroll),
                    vec2(1600.0 * zoom, 9600.0 * zoom),
                );
                let viewport = Rect::from_min_max(pos2(0.0, 170.0), pos2(1280.0, 700.0));
                let rects: Vec<_> = lane
                    .groups
                    .iter()
                    .map(|group| group.rect(canvas, viewport))
                    .collect();
                assert!(rects
                    .iter()
                    .all(|r| r.height() >= 259.99 && (r.width() - 264.0).abs() < 0.01));
                assert!(rects
                    .windows(2)
                    .all(|pair| pair[0].bottom() + 11.0 <= pair[1].top()));
            }
        }
        assert_eq!(studio.story, original);
    }

    #[test]
    fn group_navigation_edits_only_the_chosen_beat_and_keeps_header_visible() {
        for (size, density) in [
            (vec2(1100.0, 720.0), 1.0),
            (vec2(1280.0, 760.0), 1.0),
            (vec2(1707.0, 960.0), 1.5),
        ] {
            let ctx = egui::Context::default();
            let mut studio = dense_fixture(&ctx);
            let original = studio.story.clone();
            for _ in 0..3 {
                frame(&ctx, &mut studio, size, density, vec![]);
            }
            let group = studio.script_lane.groups[0];
            let first = studio.script_lane.selected(group, 0.0);
            assert_eq!(first, 0);
            let next: Rect = ctx
                .data(|data| data.get_temp(egui::Id::new(("icon_button_bounds", "Next beat"))))
                .unwrap();
            frame(&ctx, &mut studio, size, density, click(next.center()));
            frame(&ctx, &mut studio, size, density, vec![]);
            let selected = studio.script_lane.chosen.unwrap().0;
            assert_eq!(selected, original.chapters[0].panels[1].id);
            assert_eq!(studio.story, original);
            assert!(!studio.dirty && studio.undo.is_empty());
            let bounds: Rect = ctx
                .data(|data| data.get_temp(egui::Id::new(("beat_card_bounds", selected))))
                .unwrap();
            let picker: Rect = ctx
                .data(|data| data.get_temp(egui::Id::new(("beat_picker_bounds", selected))))
                .unwrap();
            assert!(bounds.contains_rect(picker));
            assert!(Rect::from_min_size(egui::Pos2::ZERO, size).contains_rect(bounds));
            let editor: Rect = ctx
                .data(|data| data.get_temp(egui::Id::new(("beat_script_bounds", selected))))
                .unwrap();
            frame(
                &ctx,
                &mut studio,
                size,
                density,
                click(editor.left_top() + vec2(4.0, 8.0)),
            );
            frame(
                &ctx,
                &mut studio,
                size,
                density,
                vec![egui::Event::Text("Edited ".into())],
            );
            assert!(studio.story.chapters[0].panels[1]
                .script
                .contains("Edited "));
            for (i, panel) in studio.story.chapters[0].panels.iter().enumerate() {
                if i != 1 {
                    assert_eq!(panel, &original.chapters[0].panels[i]);
                }
                assert_eq!(
                    (panel.y, panel.height),
                    (
                        original.chapters[0].panels[i].y,
                        original.chapters[0].panels[i].height
                    )
                );
            }
            studio.undo();
            assert_eq!(studio.story, original);
            // A newly added beat may belong to an existing group; its editor
            // still wins over automatic following and receives the keystrokes.
            studio.script_focus = Some(original.chapters[0].panels[4].id);
            frame(&ctx, &mut studio, size, density, vec![]);
            frame(
                &ctx,
                &mut studio,
                size,
                density,
                vec![egui::Event::Text("Focused ".into())],
            );
            assert!(studio.story.chapters[0].panels[4]
                .script
                .contains("Focused "));
            studio.undo();
            assert_eq!(studio.story, original);
            studio.jump_y = Some(1000.0);
            for _ in 0..3 {
                frame(&ctx, &mut studio, size, density, vec![]);
            }
            assert!(
                studio.script_lane.chosen.is_none(),
                "Canvas scrolling resumes following"
            );
        }
    }

    #[test]
    fn overlapping_and_same_position_ranges_keep_all_beats_reachable() {
        let ctx = egui::Context::default();
        let mut studio = dense_fixture(&ctx);
        for beat in &mut studio.story.chapters[0].panels {
            beat.y = 200.0;
        }
        studio.story.chapters[0].panels[0].height = 15_000.0;
        studio.prepare_script_lane();
        let group = studio.script_lane.groups[0];
        assert_eq!(studio.script_lane.groups.len(), 1);
        for beat in &studio.story.chapters[0].panels {
            studio.script_lane.reveal = Some(beat.id);
            studio.script_lane.follow(0.0, 0.25, None);
            let selected = studio.script_lane.selected(group, 10_000.0);
            assert_eq!(studio.script_lane.anchors[selected].id, beat.id);
        }
        let first = studio.story.chapters[0].panels[0].id;
        studio.script_lane.reveal = Some(first);
        studio.script_lane.follow(0.0, 0.25, None);
        studio.story.chapters[0].panels.remove(0);
        studio.changed();
        studio.prepare_script_lane();
        let selected = studio
            .script_lane
            .selected(studio.script_lane.groups[0], 0.0);
        assert_ne!(studio.script_lane.anchors[selected].id, first);
        let other = Chapter::new(studio.story.alloc_id(), "Other", 800, 2000);
        studio.story.chapters.push(other);
        studio.chapter = 1;
        studio.prepare_script_lane();
        assert!(studio.script_lane.chosen.is_none());
        assert!(studio.script_lane.groups.is_empty());
    }
    #[test]
    fn large_group_navigation_is_bounded_and_independent_of_long_text_scroll() {
        let ctx = egui::Context::default();
        let mut studio = dense_fixture(&ctx);
        let template = studio.story.chapters[0].panels[0].clone();
        studio.story.chapters[0].panels.clear();
        for i in 0..10_000 {
            let mut beat = template.clone();
            beat.id = studio.story.alloc_id();
            beat.y = i as f32;
            studio.story.chapters[0].panels.push(beat);
        }
        let last = studio.story.chapters[0].panels.last_mut().unwrap();
        last.script = "A long passage. ".repeat(1000);
        let id = last.id;
        studio.script_lane.reveal = Some(id);
        let size = vec2(1100.0, 720.0);
        for _ in 0..3 {
            frame(&ctx, &mut studio, size, 1.0, vec![]);
        }
        let bounds: Rect = ctx
            .data(|data| data.get_temp(egui::Id::new(("beat_card_bounds", id))))
            .unwrap();
        let picker: Rect = ctx
            .data(|data| data.get_temp(egui::Id::new(("beat_picker_bounds", id))))
            .unwrap();
        let menu: Rect = ctx
            .data(|data| data.get_temp(egui::Id::new(("beat_menu_bounds", id))))
            .unwrap();
        assert!(bounds.contains_rect(picker) && bounds.contains_rect(menu));
        for label in ["Previous beat", "Next beat"] {
            let button: Rect = ctx
                .data(|data| data.get_temp(egui::Id::new(("icon_button_bounds", label))))
                .unwrap();
            assert!(bounds.contains_rect(button));
        }
        let editor: Rect = ctx
            .data(|data| data.get_temp(egui::Id::new(("beat_script_bounds", id))))
            .unwrap();
        frame(
            &ctx,
            &mut studio,
            size,
            1.0,
            vec![
                egui::Event::PointerMoved(bounds.center()),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: vec2(0.0, -220.0),
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        for _ in 0..4 {
            frame(&ctx, &mut studio, size, 1.0, vec![]);
        }
        let scrolled: Rect = ctx
            .data(|data| data.get_temp(egui::Id::new(("beat_script_bounds", id))))
            .unwrap();
        let fixed_picker: Rect = ctx
            .data(|data| data.get_temp(egui::Id::new(("beat_picker_bounds", id))))
            .unwrap();
        assert!(scrolled.top() < editor.top() - 20.0);
        assert_eq!(picker, fixed_picker);
        assert_eq!(studio.script_lane.chosen.unwrap().0, id);
        assert!(!studio.dirty && studio.undo.is_empty());
    }
    #[test]
    fn grouped_heading_context_targets_the_displayed_beat() {
        let ctx = egui::Context::default();
        let mut studio = dense_fixture(&ctx);
        let id = studio.story.chapters[0].panels[4].id;
        studio.script_lane.reveal = Some(id);
        let original = studio.story.clone();
        let size = vec2(1280.0, 760.0);
        for _ in 0..3 {
            frame(&ctx, &mut studio, size, 1.0, vec![]);
        }
        let bounds: Rect = ctx
            .data(|data| data.get_temp(egui::Id::new(("beat_card_bounds", id))))
            .unwrap();
        let pos = bounds.min + vec2(35.0, 24.0);
        for pressed in [true, false] {
            frame(
                &ctx,
                &mut studio,
                size,
                1.0,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Secondary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        frame(&ctx, &mut studio, size, 1.0, vec![]);
        let (remove, enabled): (Rect, bool) = ctx
            .data(|data| data.get_temp(egui::Id::new(("context_action", "Remove beat"))))
            .unwrap();
        assert!(enabled);
        frame(&ctx, &mut studio, size, 1.0, click(remove.center()));
        assert_eq!(studio.story.chapters[0].panels.len(), 8);
        assert!(!studio.story.chapters[0]
            .panels
            .iter()
            .any(|beat| beat.id == id));
        studio.undo();
        assert_eq!(studio.story, original);
    }
}
