use super::*;
use std::time::Duration;

#[derive(Clone)]
struct SurfaceStyle {
    renderer: glass::GlassRenderer,
    strength: f32,
    enabled: bool,
}

fn surface_style(ctx: &egui::Context) -> Option<SurfaceStyle> {
    ctx.data(|data| data.get_temp(egui::Id::new("studio_surface_style")))
}

/// Native popup frames are created before their content callback. Clear only
/// their fill/stroke for that call, then restore those fields (not all settings).
pub(super) fn with_menu_surface<R>(ctx: &egui::Context, show: impl FnOnce() -> R) -> R {
    if !surface_style(ctx).is_some_and(|style| style.enabled) {
        return show();
    }
    let current = ctx.style();
    let (fill, stroke) = (current.visuals.window_fill, current.visuals.window_stroke);
    ctx.style_mut(|style| {
        style.visuals.window_fill = Color32::TRANSPARENT;
        // Keep border geometry so Glass/Solid have identical hit targets.
        style.visuals.window_stroke = egui::Stroke::new(stroke.width, Color32::TRANSPARENT);
    });
    let result = show();
    ctx.style_mut(|style| {
        style.visuals.window_fill = fill;
        style.visuals.window_stroke = stroke;
    });
    result
}

pub(super) fn menu_contents<R>(ui: &mut egui::Ui, contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let Some(style) = surface_style(ui.ctx()).filter(|style| style.enabled) else {
        return contents(ui);
    };
    let paint = ui.painter().clone();
    let slot = paint.add(egui::Shape::Noop);
    let margin = ui.spacing().menu_margin;
    let result = contents(ui);
    let rect = ui.min_rect() + margin;
    paint.set(
        slot,
        style
            .renderer
            .shape(rect, 9.0, glass_tint(), style.strength),
    );
    result
}

pub(super) fn context_menu(
    response: &egui::Response,
    contents: impl FnOnce(&mut egui::Ui),
) -> Option<egui::InnerResponse<()>> {
    with_menu_surface(&response.ctx, || {
        response.context_menu(|ui| menu_contents(ui, contents))
    })
}

/// Bounded panel controls can share the same surface as menus and the canvas
/// shelf. Content is laid out normally; only the background is translucent.
pub(super) fn surface_frame<R>(
    ui: &mut egui::Ui,
    contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    let style = surface_style(ui.ctx());
    let painter = ui.painter().clone();
    let slot = painter.add(egui::Shape::Noop);
    let result = egui::Frame::new().inner_margin(10.0).show(ui, contents);
    let rect = result.response.rect;
    let shape = if let Some(style) = style.filter(|style| style.enabled) {
        style
            .renderer
            .shape(rect, 10.0, glass_tint(), style.strength)
    } else {
        egui::Shape::rect_filled(rect, 10.0, PANEL)
    };
    painter.set(slot, shape);
    result
}

impl Studio {
    pub(super) fn apply_preferences(&self, ctx: &egui::Context) {
        ctx.data_mut(|data| {
            data.insert_temp(
                egui::Id::new("studio_surface_style"),
                SurfaceStyle {
                    renderer: self.glass.clone(),
                    strength: self.preferences.glass_strength,
                    enabled: self.preferences.glass_enabled,
                },
            )
        });
        motion::set_mode(ctx, self.preferences.motion);
        let duration = match self.preferences.motion {
            settings::Motion::Off => 0.0,
            settings::Motion::Calm => 0.12,
            settings::Motion::Lively => 0.18,
        };
        ctx.style_mut(|style| style.animation_time = duration);
    }

    pub(super) fn settings_workspace(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG).inner_margin(24.0))
            .show(ctx, |ui| {
                ui.set_max_width(ui.available_width().min(1040.0));
                ui.horizontal(|ui| {
                    ui.heading("Settings");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if motion::button(ui, "Done").clicked() {
                            self.settings_open = false;
                        }
                    });
                });
                ui.add_space(16.0);
                let glass = self.glass.clone();
                let shown =
                    settings_ui::show(ui, &mut self.preferences, |painter, rect, strength| {
                        glass.paint(painter, rect, 18.0, glass_tint(), strength);
                    });
                if shown.addons {
                    self.addons_settings(ui);
                }
                if shown.changed {
                    self.snap_settings = self.preferences.snap_settings();
                    self.snap_guides.clear();
                    self.apply_preferences(ctx);
                    self.preferences_changed_at = Some(Instant::now());
                }
            });
    }

    pub(super) fn flush_preferences(&mut self, ctx: &egui::Context, force: bool) {
        if let Some(changed) = self.preferences_changed_at {
            let delay = Duration::from_millis(400);
            if force || (changed.elapsed() >= delay && !ctx.input(|i| i.pointer.any_down())) {
                self.save_preferences();
            } else {
                ctx.request_repaint_after(delay);
            }
        }
    }

    pub(super) fn save_preferences(&mut self) {
        if self.preferences_changed_at.take().is_some() {
            if let Some(path) = &self.preference_path {
                if let Err(error) = settings::save(path, &self.preferences) {
                    self.status = format!("Settings could not be saved: {error}");
                }
            }
        }
    }
}

pub(super) fn glass_tint() -> Color32 {
    Color32::from_rgba_unmultiplied(34, 39, 49, 178)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contextual_glass_precedes_content_and_preserves_solid_targets_and_escape() {
        fn callbacks(shape: &egui::Shape) -> usize {
            match shape {
                egui::Shape::Callback(_) => 1,
                egui::Shape::Vec(shapes) => shapes.iter().map(callbacks).sum(),
                _ => 0,
            }
        }
        let mut action_bounds = Vec::new();
        for glass in [false, true] {
            let ctx = egui::Context::default();
            let mut studio = Studio::from_context(&ctx);
            studio.preferences.glass_enabled = glass;
            studio.preferences.motion = settings::Motion::Off;
            studio.apply_preferences(&ctx);
            let native_fill = ctx.style().visuals.window_fill;
            let native_stroke = ctx.style().visuals.window_stroke;
            let mut tick = 0;
            let mut render = |events| {
                tick += 1;
                let mut target = egui::Rect::NOTHING;
                let mut action = None;
                let output = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(600.0, 420.0),
                        )),
                        time: Some(f64::from(tick) * 0.05),
                        events,
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            let (rect, response) = ui.allocate_exact_size(
                                egui::vec2(240.0, 120.0),
                                egui::Sense::click(),
                            );
                            target = rect;
                            ui.painter().rect_filled(rect, 0.0, ACCENT);
                            context_menu(&response, |ui| {
                                ui.set_min_width(180.0);
                                action = Some(motion::button(ui, "Duplicate").rect);
                            });
                        });
                    },
                );
                assert_eq!(ctx.style().visuals.window_fill, native_fill);
                assert_eq!(ctx.style().visuals.window_stroke, native_stroke);
                (output, target, action)
            };
            let target = render(vec![]).1.center();
            render(vec![egui::Event::PointerMoved(target)]);
            render(vec![egui::Event::PointerButton {
                pos: target,
                button: egui::PointerButton::Secondary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            }]);
            render(vec![egui::Event::PointerButton {
                pos: target,
                button: egui::PointerButton::Secondary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }]);
            let (output, _, action) = render(vec![]);
            action_bounds.push(action.expect("Context menu opens over its target"));
            let count: usize = output
                .shapes
                .iter()
                .map(|shape| callbacks(&shape.shape))
                .sum();
            assert_eq!(count > 0, glass, "Only Glass schedules a GPU surface");
            if glass {
                let backdrop = output
                    .shapes
                    .iter()
                    .position(|shape| callbacks(&shape.shape) > 0)
                    .unwrap();
                let text = output
                    .shapes
                    .iter()
                    .position(|shape| text_bounds(&shape.shape, "Duplicate").is_some())
                    .unwrap();
                assert!(
                    backdrop < text,
                    "Blur must not capture or obscure menu text"
                );
            }
            render(vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }]);
            // egui finalizes context-menu dismissal after its content pass.
            let (_, _, action) = render(vec![]);
            assert!(action.is_none(), "Escape closes the native context menu");
        }
        assert_eq!(
            action_bounds[0], action_bounds[1],
            "Surface settings must not shift targets"
        );
    }

    fn input(events: Vec<egui::Event>) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1100.0, 720.0),
            )),
            events,
            ..Default::default()
        }
    }

    fn frame(ctx: &egui::Context, studio: &mut Studio, input: egui::RawInput) -> egui::FullOutput {
        let mut frame = eframe::Frame::_new_kittest();
        ctx.run(input, |ctx| eframe::App::update(studio, ctx, &mut frame))
    }

    fn chord(key: egui::Key, modifiers: egui::Modifiers) -> Vec<egui::Event> {
        [true, false]
            .into_iter()
            .map(|pressed| egui::Event::Key {
                key,
                physical_key: Some(key),
                pressed,
                repeat: false,
                modifiers,
            })
            .collect()
    }

    fn text_bounds(shape: &egui::Shape, label: &str) -> Option<egui::Rect> {
        match shape {
            egui::Shape::Text(text) if text.galley.job.text == label => {
                Some(text.visual_bounding_rect())
            }
            egui::Shape::Vec(shapes) => shapes.iter().find_map(|shape| text_bounds(shape, label)),
            _ => None,
        }
    }

    fn control(output: &egui::FullOutput, label: &str) -> (egui::Rect, egui::Rect) {
        output
            .shapes
            .iter()
            .find_map(|shape| text_bounds(&shape.shape, label).map(|rect| (rect, shape.clip_rect)))
            .unwrap_or_else(|| panic!("Missing control {label}"))
    }

    fn click(
        ctx: &egui::Context,
        studio: &mut Studio,
        output: &egui::FullOutput,
        label: &str,
    ) -> egui::FullOutput {
        let pos = control(output, label).0.center();
        let events = [
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
        ];
        let _ = frame(ctx, studio, input(events.into()));
        frame(ctx, studio, input(Vec::new()))
    }

    fn studio_with_history(ctx: &egui::Context) -> Studio {
        let mut studio = Studio::from_context(ctx);
        studio.welcome = false;
        // The executable's test-name argument is not an application story path.
        studio.error = None;
        let id = studio.story.alloc_id();
        studio.story.chapters[0].layers.push(Layer {
            extensions: Default::default(),
            id,
            name: "Artwork".into(),
            visible: true,
            locked: false,
            opacity: 1.0,
            x: 100.0,
            y: 100.0,
            width: 200.0,
            height: 200.0,
            kind: LayerKind::Shape {
                color: [122, 184, 245, 255],
            },
        });
        studio.selected = Some(id);
        studio.undo.push_back(Story::new("Earlier revision"));
        studio.redo.push(Story::new("Later revision"));
        studio.dirty = false;
        studio
    }

    #[test]
    fn settings_shortcuts_block_canvas_commands_and_preserve_selection_on_exit() {
        let ctx = egui::Context::default();
        let mut studio = studio_with_history(&ctx);
        let original_story = studio.story.clone();
        let original_undo = studio.undo.clone();
        let original_redo = studio.redo.clone();
        let original_selection = studio.selected;
        let original_snap = studio.snap_settings;
        let command = egui::Modifiers::CTRL | egui::Modifiers::COMMAND;
        let _ = frame(&ctx, &mut studio, input(chord(egui::Key::Comma, command)));
        assert!(studio.settings_open);

        for key in [
            egui::Key::B,
            egui::Key::T,
            egui::Key::L,
            egui::Key::P,
            egui::Key::S,
            egui::Key::Delete,
        ] {
            let _ = frame(&ctx, &mut studio, input(chord(key, egui::Modifiers::NONE)));
        }
        for key in [egui::Key::Z, egui::Key::Y, egui::Key::V] {
            let _ = frame(&ctx, &mut studio, input(chord(key, command)));
        }
        let _ = frame(
            &ctx,
            &mut studio,
            input(vec![egui::Event::Paste("A paste in Settings".into())]),
        );
        assert!(studio.tool == Tool::Move);
        assert_eq!(studio.story, original_story);
        assert_eq!(studio.undo, original_undo);
        assert_eq!(studio.redo, original_redo);
        assert_eq!(studio.selected, original_selection);
        assert_eq!(studio.snap_settings, original_snap);
        assert!(!studio.dirty);
        assert!(studio.error.is_none());

        let _ = frame(
            &ctx,
            &mut studio,
            input(chord(egui::Key::Escape, egui::Modifiers::NONE)),
        );
        assert!(!studio.settings_open);
        assert_eq!(studio.selected, original_selection);
        let _ = frame(&ctx, &mut studio, input(chord(egui::Key::Comma, command)));
        assert!(studio.settings_open);
        let _ = frame(&ctx, &mut studio, input(chord(egui::Key::Comma, command)));
        assert!(!studio.settings_open);
    }

    #[test]
    fn settings_edits_sync_snapping_without_changing_story_or_history() {
        let ctx = egui::Context::default();
        let mut studio = studio_with_history(&ctx);
        let original_story = studio.story.clone();
        let original_undo = studio.undo.clone();
        let original_redo = studio.redo.clone();
        let original_revision = studio.revision;
        assert!(studio.preference_path.is_none());

        let output = frame(&ctx, &mut studio, input(Vec::new()));
        let output = click(&ctx, &mut studio, &output, "Settings");
        assert!(studio.settings_open);
        let output = click(&ctx, &mut studio, &output, "Solid");
        assert!(!studio.preferences.glass_enabled);
        let output = click(&ctx, &mut studio, &output, "Off");
        assert_eq!(studio.preferences.motion, settings::Motion::Off);
        assert_eq!(motion::mode(&ctx), settings::Motion::Off);
        assert_eq!(ctx.style().animation_time, 0.0);
        let output = click(&ctx, &mut studio, &output, "Canvas");
        let output = click(&ctx, &mut studio, &output, "Snapping");
        assert!(!studio.snap_settings.enabled);
        assert!(!studio.preferences.snap_enabled);
        let _ = click(&ctx, &mut studio, &output, "Done");
        assert!(!studio.settings_open);
        let _ = frame(
            &ctx,
            &mut studio,
            input(chord(egui::Key::S, egui::Modifiers::NONE)),
        );
        assert!(studio.snap_settings.enabled);
        assert!(studio.preferences.snap_enabled);
        assert!(studio.preferences_changed_at.is_some());
        studio.flush_preferences(&ctx, true);
        assert!(studio.preferences_changed_at.is_none());
        assert_eq!(studio.story, original_story);
        assert_eq!(studio.undo, original_undo);
        assert_eq!(studio.redo, original_redo);
        assert_eq!(studio.revision, original_revision);
        assert!(!studio.dirty);
    }

    #[test]
    fn settings_ignore_dropped_images_and_stories_without_hidden_imports() {
        let ctx = egui::Context::default();
        let mut studio = studio_with_history(&ctx);
        studio.settings_open = true;
        studio.tab = LibraryTab::Images;
        let original_story = studio.story.clone();
        let original_asset_count = studio.assets.len();
        let mut dropped = input(Vec::new());
        dropped.dropped_files = ["image.png", "story.scro"]
            .into_iter()
            .map(|name| egui::DroppedFile {
                path: Some(
                    std::env::temp_dir()
                        .join("scroll-studio-blocked-drop")
                        .join(name),
                ),
                ..Default::default()
            })
            .collect();
        let _ = frame(&ctx, &mut studio, dropped);
        assert_eq!(studio.story, original_story);
        assert_eq!(studio.assets.len(), original_asset_count);
        assert!(studio.error.is_none());
        assert!(studio.pending.is_none());
        assert!(studio.path.is_none());
        assert!(!studio.dirty);
    }

    #[test]
    fn settings_escape_works_from_welcome_and_paste_release_stays_suppressed_after_exit() {
        let ctx = egui::Context::default();
        let mut studio = studio_with_history(&ctx);
        studio.settings_open = true;
        studio.welcome = true;
        let _ = frame(
            &ctx,
            &mut studio,
            input(chord(egui::Key::Escape, egui::Modifiers::NONE)),
        );
        assert!(!studio.settings_open);
        studio.welcome = false;
        studio.settings_open = true;
        let paste_id = egui::Id::new("image_paste_press_handled");
        let command = egui::Modifiers::CTRL | egui::Modifiers::COMMAND;
        let mut events = chord(egui::Key::V, command);
        let release = events.pop().unwrap();
        let _ = frame(&ctx, &mut studio, input(events));
        assert!(ctx.data(|data| data.get_temp::<bool>(paste_id).unwrap_or(false)));
        // Navigation can leave Settings between native key press and release.
        studio.settings_open = false;
        let original = studio.story.clone();
        let _ = frame(&ctx, &mut studio, input(vec![release]));
        assert!(!ctx.data(|data| data.get_temp::<bool>(paste_id).unwrap_or(false)));
        assert_eq!(studio.story, original);
        assert!(studio.error.is_none());
        assert!(!studio.dirty);
    }

    #[test]
    fn complete_settings_workspace_fits_minimum_window_and_hides_asset_imports() {
        let ctx = egui::Context::default();
        let mut studio = studio_with_history(&ctx);
        studio.settings_open = true;
        studio.tab = LibraryTab::Images;
        studio.story.title = "A deliberately long story title ".repeat(10);
        let output = frame(&ctx, &mut studio, input(Vec::new()));
        let screen = input(Vec::new()).screen_rect.unwrap();
        let appearance_labels = [
            "File",
            "Scroll Studio",
            "Chapters",
            "Bubbles",
            "Characters",
            "Locations",
            "Images",
            "Settings",
            "Done",
            "Appearance",
            "Canvas",
            "Reset",
            "Surface",
            "Glass",
            "Solid",
            "Frost",
            "Motion",
            "Off",
            "Calm",
            "Lively",
            "Preview",
            "Try it",
        ];
        for label in appearance_labels {
            let (rect, clip) = control(&output, label);
            assert!(
                screen.contains_rect(rect),
                "{label} outside window: {rect:?}"
            );
            assert!(
                clip.expand(1.0).contains_rect(rect),
                "{label} clipped: {rect:?} / {clip:?}"
            );
        }
        for hidden in [
            "+ Import",
            "Search",
            "No images",
            "+ New",
            "Style",
            "Copy style",
            "Save",
            "Export",
        ] {
            assert!(!output
                .shapes
                .iter()
                .any(|shape| text_bounds(&shape.shape, hidden).is_some()));
        }
        let output = click(&ctx, &mut studio, &output, "Canvas");
        for label in [
            "Done",
            "Appearance",
            "Canvas",
            "Reset",
            "Snapping",
            "Canvas center & edges",
            "Objects",
            "Equal spacing",
            "Strength",
        ] {
            let (rect, clip) = control(&output, label);
            assert!(
                screen.contains_rect(rect),
                "{label} outside window: {rect:?}"
            );
            assert!(
                clip.expand(1.0).contains_rect(rect),
                "{label} clipped: {rect:?} / {clip:?}"
            );
        }
    }
}
