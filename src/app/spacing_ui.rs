use super::*;
use scroll_studio::spacing::{self, Crossing, PreparedSpacing, SpaceMode, SpaceRequest};

pub(super) struct SpacingDraft {
    epoch: u64,
    revision: u64,
    chapter_id: u64,
    request: SpaceRequest,
    include_locked: bool,
    options_open: bool,
    view_y: f32,
    prepared_for: Option<SpaceRequest>,
    prepared: Option<PreparedSpacing>,
    error: Option<String>,
}

impl SpacingDraft {
    fn refresh(&mut self, chapter: &Chapter) {
        if self.prepared_for == Some(self.request) {
            return;
        }
        self.prepared_for = Some(self.request);
        match spacing::prepare(chapter, self.request) {
            Ok(plan) => {
                self.prepared = Some(plan);
                self.error = None;
            }
            Err(error) => {
                self.prepared = None;
                self.error = Some(error.to_string());
            }
        }
    }
}

impl Studio {
    pub(super) fn begin_spacing(&mut self, chapter_id: u64, at: f32) {
        let Some(chapter) = self
            .story
            .chapters
            .iter()
            .find(|chapter| chapter.id == chapter_id)
        else {
            return;
        };
        let at = at.clamp(0.0, chapter.height as f32).round() as u32;
        self.spacing_draft = Some(SpacingDraft {
            epoch: self.document_epoch,
            revision: self.revision,
            chapter_id,
            request: SpaceRequest {
                at,
                amount: (chapter.width / 4).max(1),
                mode: SpaceMode::Insert,
                crossing: Crossing::Keep,
            },
            include_locked: false,
            options_open: false,
            view_y: (at as f32 - chapter.width as f32 * 0.4).max(0.0),
            prepared_for: None,
            prepared: None,
            error: None,
        });
        self.drag_layer = None;
        self.drag_tail = None;
        self.snap_drag = None;
        self.snap_guides.clear();
        self.resize_anchor = None;
    }

    pub(super) fn spacing_ui(&mut self, ctx: &egui::Context) {
        // Save/Discard/Cancel remains reachable if the OS closes this window.
        if self.dialog.is_some() {
            return;
        }
        let Some(mut draft) = self.spacing_draft.take() else {
            return;
        };
        if draft.epoch != self.document_epoch || draft.revision != self.revision {
            return;
        }
        let Some(index) = self
            .story
            .chapters
            .iter()
            .position(|chapter| chapter.id == draft.chapter_id)
        else {
            return;
        };
        let chapter = &self.story.chapters[index];
        let mut apply = false;
        let mut cancel = false;
        let options_were_open = draft.options_open;
        let modal = egui::Modal::new(egui::Id::new("spacing_review")).show(ctx, |ui| {
            ui.set_width((ctx.screen_rect().width() - 96.0).clamp(640.0, 860.0));
            ui.horizontal(|ui| {
                ui.heading("Spacing");
                ui.add_space(12.0);
                for (mode, title) in [(SpaceMode::Insert, "Insert"), (SpaceMode::Remove, "Remove")]
                {
                    let response =
                        ui.add(motion::Button::new(title).selected(draft.request.mode == mode));
                    #[cfg(test)]
                    record(ui, title, &response);
                    if response.clicked() {
                        draft.request.mode = mode;
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let options = motion::menu_button(ui, "Options", |ui| {
                        ui.label("Crossing objects");
                        ui.radio_value(
                            &mut draft.request.crossing,
                            Crossing::Keep,
                            "Keep position",
                        );
                        ui.radio_value(
                            &mut draft.request.crossing,
                            Crossing::Move,
                            "Move with lower artwork",
                        );
                    });
                    draft.options_open = egui::menu::BarState::load(ui.ctx(), ui.id()).is_some();
                    #[cfg(test)]
                    record(ui, "options", &options.response);
                    let _ = options;
                });
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label("Space");
                let amount = ui.add(
                    egui::DragValue::new(&mut draft.request.amount)
                        .range(1..=200_000)
                        .speed(10.0)
                        .suffix(" px"),
                );
                #[cfg(test)]
                record(ui, "amount", &amount);
                let _ = amount;
                ui.add_space(18.0);
                ui.label("At");
                let at = ui.add(
                    egui::DragValue::new(&mut draft.request.at)
                        .range(0..=chapter.height)
                        .speed(10.0)
                        .suffix(" px"),
                );
                #[cfg(test)]
                record(ui, "position", &at);
                if at.changed() {
                    draft.view_y = (draft.request.at as f32 - chapter.width as f32 * 0.4).max(0.0);
                }
                ui.add_space(16.0);
                ui.add(egui::Label::new(RichText::new(&chapter.name).color(MUTED)).truncate());
            });
            ui.add_space(10.0);
            let preview_height = (ctx.screen_rect().height() - 280.0).clamp(220.0, 380.0);
            draft.refresh(chapter);
            let (before, after) = ui.columns(2, |columns| {
                columns[0].label(RichText::new("Before").color(MUTED));
                let before = allocate_preview(&mut columns[0], preview_height, false);
                columns[1].label(RichText::new("After").color(MUTED));
                let after = allocate_preview(&mut columns[1], preview_height, true);
                (before, after)
            });
            let scale = before.rect.width() / chapter.width.max(1) as f32;
            if before.hovered() || after.hovered() {
                let scroll = ctx.input(|input| input.smooth_scroll_delta.y);
                if scroll != 0.0 {
                    draft.view_y =
                        (draft.view_y - scroll / scale).clamp(0.0, chapter.height as f32);
                    ctx.input_mut(|input| {
                        input.smooth_scroll_delta = egui::Vec2::ZERO;
                        input.raw_scroll_delta = egui::Vec2::ZERO;
                    });
                }
            }
            if before.hovered() {
                ctx.set_cursor_icon(egui::CursorIcon::ResizeVertical);
            }
            if before.clicked() || before.dragged() {
                if let Some(point) = before.interact_pointer_pos() {
                    draft.request.at = (draft.view_y + (point.y - before.rect.top()) / scale)
                        .clamp(0.0, chapter.height as f32)
                        .round() as u32;
                    draft.refresh(chapter);
                }
            }
            // Handle input before painting either side so their viewports stay synchronized.
            for (response, is_after) in [(&before, false), (&after, true)] {
                paint_preview(
                    ui,
                    ctx,
                    &mut self.previews,
                    &self.assets,
                    if is_after {
                        draft
                            .prepared
                            .as_ref()
                            .map_or(chapter, |plan| &plan.chapter)
                    } else {
                        chapter
                    },
                    draft.request,
                    draft.view_y,
                    response.rect,
                    is_after,
                );
            }
            before
                .on_hover_text("Click or drag to place the cut. Scroll to look along the chapter.");
            after.on_hover_text("Scroll to look along the chapter");
            ui.add_space(8.0);
            // One fixed status row prevents the preview moving as the cut crosses artwork.
            ui.allocate_ui(egui::vec2(ui.available_width(), 24.0), |ui| {
                if let Some(error) = &draft.error {
                    ui.add(
                        egui::Label::new(RichText::new(error).color(Color32::LIGHT_RED)).truncate(),
                    )
                    .on_hover_text(error);
                } else if let Some(plan) = &draft.prepared {
                    ui.horizontal(|ui| {
                        ui.label(format!(
                            "{} objects · {} beats",
                            plan.moved_objects, plan.changed_beats
                        ));
                        if plan.crossing_objects > 0 {
                            ui.label(
                                RichText::new(format!("· {} crossing", plan.crossing_objects))
                                    .color(MUTED),
                            );
                        }
                        if plan.newly_clipped > 0 {
                            ui.label(
                                RichText::new(format!("· {} clipped", plan.newly_clipped))
                                    .color(Color32::from_rgb(243, 183, 108)),
                            )
                            .on_hover_text("Objects remain editable outside the shortened canvas");
                        }
                    });
                }
            });
            ui.horizontal(|ui| {
                if let Some(plan) = &draft.prepared {
                    if plan.locked_objects > 0 {
                        let include = ui.checkbox(
                            &mut draft.include_locked,
                            format!("Include {} locked", plan.locked_objects),
                        );
                        #[cfg(test)]
                        record(ui, "include_locked", &include);
                        include.on_hover_text(
                            "Move these objects for this change; keep them locked afterward",
                        );
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let enabled = draft
                        .prepared
                        .as_ref()
                        .is_some_and(|plan| plan.locked_objects == 0 || draft.include_locked);
                    let commit = ui.add_enabled(
                        enabled,
                        motion::Button::new(RichText::new("Apply").color(BG))
                            .fill(ACCENT)
                            .min_size(egui::vec2(96.0, 32.0)),
                    );
                    apply = commit.clicked();
                    let close = motion::button(ui, "Cancel");
                    cancel = close.clicked();
                    #[cfg(test)]
                    {
                        record(ui, "apply", &commit);
                        record(ui, "cancel", &close);
                    }
                });
            });
        });
        #[cfg(test)]
        ctx.data_mut(|data| data.insert_temp(egui::Id::new("spacing_modal"), modal.response.rect));
        // egui's native menu state is separate from Modal's popup tracking.
        // Escape/outside click backs out of Options before dismissing the review.
        if options_were_open {
            ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        }
        if cancel || (!options_were_open && modal.should_close()) {
            return;
        }
        if apply {
            match self.commit_spacing(&draft) {
                Ok(()) => return,
                Err(error) => draft.error = Some(error.to_string()),
            }
        }
        self.spacing_draft = Some(draft);
    }

    fn commit_spacing(&mut self, draft: &SpacingDraft) -> anyhow::Result<()> {
        anyhow::ensure!(
            draft.epoch == self.document_epoch && draft.revision == self.revision,
            "This spacing preview is no longer current"
        );
        let index = self
            .story
            .chapters
            .iter()
            .position(|chapter| chapter.id == draft.chapter_id)
            .ok_or_else(|| anyhow::anyhow!("This chapter is no longer in the story"))?;
        let prepared = spacing::prepare(&self.story.chapters[index], draft.request)?;
        anyhow::ensure!(
            prepared.locked_objects == 0 || draft.include_locked,
            "Include the locked objects to apply this change"
        );
        self.checkpoint();
        self.story.chapters[index] = prepared.chapter;
        self.chapter = index;
        self.active_panel = None;
        self.tab = LibraryTab::Chapters;
        self.selected = None;
        self.selected_asset = None;
        self.jump_y = Some(draft.request.at.saturating_sub(160) as f32);
        self.changed();
        self.status = match draft.request.mode {
            SpaceMode::Insert => "Space inserted",
            SpaceMode::Remove => "Space removed",
        }
        .into();
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_preview(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    previews: &mut previews::PreviewCache,
    assets: &AssetBytes,
    chapter: &Chapter,
    request: SpaceRequest,
    view_y: f32,
    rect: egui::Rect,
    after: bool,
) {
    let scale = rect.width() / chapter.width.max(1) as f32;
    let canvas = egui::Rect::from_min_size(
        rect.min - egui::vec2(0.0, view_y * scale),
        egui::vec2(rect.width(), chapter.height as f32 * scale),
    );
    let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    painter.rect_filled(rect, 4.0, PANEL);
    painter.rect_filled(canvas, 0.0, Color32::WHITE);
    for layer in chapter.layers.iter().filter(|layer| layer.visible) {
        let rendered = scroll_studio::bubbles::joined_layer(layer, &chapter.layers);
        Studio::paint_layer(
            previews,
            assets,
            ctx,
            &painter.with_clip_rect(painter.clip_rect().intersect(canvas)),
            &rendered,
            canvas,
            scale,
        );
    }
    let y = canvas.top() + request.at as f32 * scale;
    let insertion = request.mode == SpaceMode::Insert;
    let color = if insertion {
        ACCENT
    } else {
        Color32::from_rgb(241, 146, 118)
    };
    if after == insertion {
        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(rect.left(), y),
                egui::pos2(rect.right(), y + request.amount as f32 * scale),
            ),
            0.0,
            color.gamma_multiply(0.14),
        );
        painter.hline(
            rect.x_range(),
            y + request.amount as f32 * scale,
            egui::Stroke::new(1.0_f32, color),
        );
    }
    painter.hline(rect.x_range(), y, egui::Stroke::new(2.0_f32, color));
    if rect.y_range().contains(y) {
        painter.circle_filled(egui::pos2(rect.left() + 7.0, y), 4.0, color);
        painter.circle_filled(egui::pos2(rect.right() - 7.0, y), 4.0, color);
    }
}

fn allocate_preview(ui: &mut egui::Ui, height: f32, after: bool) -> egui::Response {
    let (_rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), height),
        if after {
            egui::Sense::hover()
        } else {
            egui::Sense::click_and_drag()
        },
    );
    #[cfg(test)]
    ui.data_mut(|data| data.insert_temp(egui::Id::new(("spacing_preview", after)), response.rect));
    response
}

#[cfg(test)]
fn record(ui: &egui::Ui, key: &str, response: &egui::Response) {
    ui.data_mut(|data| {
        data.insert_temp(
            egui::Id::new(("spacing_control", key)),
            (response.rect, response.enabled()),
        )
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(ctx: &egui::Context, locked: bool) -> Studio {
        let mut studio = Studio::from_context(ctx);
        studio.story = Story::new("Spacing review");
        studio.story.chapters[0].name = "A long chapter name ".repeat(100);
        let id = studio.story.alloc_id();
        studio.story.chapters[0].layers.push(Layer {
            id,
            name: "Lower panel".into(),
            x: 100.0,
            y: 1000.0,
            width: 1200.0,
            height: 600.0,
            visible: true,
            locked,
            opacity: 1.0,
            kind: LayerKind::Shape {
                color: [50, 70, 95, 255],
            },
        });
        studio.story.chapters[0].panels[0].y = 1000.0;
        studio.story.chapters[0].panels[0].script = "Wait for me.".into();
        let second = Chapter::new(studio.story.alloc_id(), "Untouched", 800, 6400);
        studio.story.chapters.push(second);
        studio.welcome = false;
        studio.error = None;
        studio.dirty = false;
        studio.begin_spacing(studio.story.chapters[0].id, 500.0);
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
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            events,
            ..Default::default()
        };
        input
            .viewports
            .entry(egui::ViewportId::ROOT)
            .or_default()
            .native_pixels_per_point = Some(density);
        let mut frame = eframe::Frame::_new_kittest();
        let _ = ctx.run(input, |ctx| eframe::App::update(studio, ctx, &mut frame));
    }

    fn control(ctx: &egui::Context, key: &str) -> (egui::Rect, bool) {
        ctx.data(|data| data.get_temp(egui::Id::new(("spacing_control", key))))
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
    fn locked_art_requires_explicit_opt_in_and_spacing_is_one_undoable_change() {
        let ctx = egui::Context::default();
        let mut studio = fixture(&ctx, true);
        let before = studio.story.clone();
        let size = egui::vec2(1100.0, 720.0);
        for _ in 0..3 {
            frame(&ctx, &mut studio, size, 1.0, vec![]);
        }
        assert!(!control(&ctx, "apply").1);
        let draft = studio.spacing_draft.take().unwrap();
        assert!(studio.commit_spacing(&draft).is_err());
        studio.spacing_draft = Some(draft);
        assert_eq!(studio.story, before);
        assert!(studio.undo.is_empty());
        let include = control(&ctx, "include_locked").0;
        frame(&ctx, &mut studio, size, 1.0, click(include.center()));
        frame(&ctx, &mut studio, size, 1.0, vec![]);
        assert!(control(&ctx, "apply").1);
        let apply = control(&ctx, "apply").0;
        frame(&ctx, &mut studio, size, 1.0, click(apply.center()));
        assert!(studio.spacing_draft.is_none());
        assert_eq!(studio.undo.len(), 1);
        assert_eq!(
            studio.story.chapters[0].height,
            before.chapters[0].height + 400
        );
        assert_eq!(studio.story.chapters[0].layers[0].y, 1400.0);
        assert!(studio.story.chapters[0].layers[0].locked);
        assert_eq!(studio.story.chapters[0].panels[0].y, 1400.0);
        assert_eq!(studio.story.chapters[0].panels[0].script, "Wait for me.");
        assert_eq!(studio.story.chapters[1], before.chapters[1]);
        let after = studio.story.clone();
        studio.undo();
        assert_eq!(studio.story, before);
        studio.redo();
        assert_eq!(studio.story, after);
    }

    #[test]
    fn empty_and_populated_previews_fit_supported_windows_and_cancel_without_changes() {
        for (size, density, populated) in [
            (egui::vec2(1100.0, 720.0), 1.0, false),
            (egui::vec2(1280.0, 760.0), 1.0, true),
            (egui::vec2(1707.0, 960.0), 1.5, true),
            (egui::vec2(2560.0, 1440.0), 1.0, false),
        ] {
            let ctx = egui::Context::default();
            let mut studio = fixture(&ctx, false);
            if !populated {
                studio.story.chapters[0].layers.clear();
            }
            let before = studio.story.clone();
            for _ in 0..3 {
                frame(&ctx, &mut studio, size, density, vec![]);
            }
            let modal: egui::Rect = ctx
                .data(|data| data.get_temp(egui::Id::new("spacing_modal")))
                .unwrap();
            let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
            assert!(
                screen.contains_rect(modal),
                "Modal exceeds {size:?}: {modal:?}"
            );
            for key in ["Insert", "Remove", "amount", "position", "apply", "cancel"] {
                assert!(
                    modal.contains_rect(control(&ctx, key).0),
                    "{key} outside modal"
                );
            }
            assert!(control(&ctx, "apply").0.width() <= 120.0);
            let remove = control(&ctx, "Remove").0;
            frame(&ctx, &mut studio, size, density, click(remove.center()));
            assert_eq!(
                studio.spacing_draft.as_ref().unwrap().request.mode,
                SpaceMode::Remove
            );
            let cancel = control(&ctx, "cancel").0;
            frame(&ctx, &mut studio, size, density, click(cancel.center()));
            assert!(studio.spacing_draft.is_none());
            assert_eq!(studio.story, before);
            assert!(!studio.dirty && studio.undo.is_empty());
        }
    }

    #[test]
    fn direct_preview_interaction_stays_local_and_stale_drafts_cannot_commit() {
        let ctx = egui::Context::default();
        let mut studio = fixture(&ctx, false);
        let before = studio.story.clone();
        let size = egui::vec2(1100.0, 720.0);
        for _ in 0..3 {
            frame(&ctx, &mut studio, size, 1.0, vec![]);
        }
        let preview: egui::Rect = ctx
            .data(|data| data.get_temp(egui::Id::new(("spacing_preview", false))))
            .unwrap();
        frame(&ctx, &mut studio, size, 1.0, click(preview.center()));
        let draft = studio.spacing_draft.as_ref().unwrap();
        assert!(draft.request.at > 600 && draft.request.at < 800);
        let at = draft.request.at;
        let view_y = draft.view_y;
        let scroll_y = studio.scroll_y;
        let after: egui::Rect = ctx
            .data(|data| data.get_temp(egui::Id::new(("spacing_preview", true))))
            .unwrap();
        frame(
            &ctx,
            &mut studio,
            size,
            1.0,
            vec![
                egui::Event::PointerMoved(after.center()),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, -100.0),
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        assert!(studio.spacing_draft.as_ref().unwrap().view_y > view_y);
        assert_eq!(studio.spacing_draft.as_ref().unwrap().request.at, at);
        assert_eq!(studio.scroll_y, scroll_y);
        assert_eq!(studio.story, before);
        let draft = studio.spacing_draft.take().unwrap();
        studio.revision += 1;
        assert!(studio.commit_spacing(&draft).is_err());
        studio.revision -= 1;
        studio.document_epoch += 1;
        assert!(studio.commit_spacing(&draft).is_err());
        assert_eq!(studio.story, before);
        assert!(studio.undo.is_empty());
    }

    #[test]
    fn escape_closes_options_before_canceling_the_review() {
        let ctx = egui::Context::default();
        let mut studio = fixture(&ctx, false);
        let before = studio.story.clone();
        let size = egui::vec2(1100.0, 720.0);
        for _ in 0..3 {
            frame(&ctx, &mut studio, size, 1.0, vec![]);
        }
        let options = control(&ctx, "options").0;
        let mut events = click(options.center());
        let release = events.pop().unwrap();
        frame(&ctx, &mut studio, size, 1.0, events);
        frame(&ctx, &mut studio, size, 1.0, vec![release]);
        frame(&ctx, &mut studio, size, 1.0, vec![]);
        assert!(studio.spacing_draft.as_ref().unwrap().options_open);
        let escape = || {
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }]
        };
        frame(&ctx, &mut studio, size, 1.0, escape());
        assert!(!studio.spacing_draft.as_ref().unwrap().options_open);
        frame(&ctx, &mut studio, size, 1.0, vec![]);
        frame(&ctx, &mut studio, size, 1.0, escape());
        assert!(studio.spacing_draft.is_none());
        assert_eq!(studio.story, before);
        assert!(!studio.dirty && studio.undo.is_empty());
    }
}
