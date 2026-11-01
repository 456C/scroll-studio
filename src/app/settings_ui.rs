use super::{motion, panels, settings::*, ACCENT, BG, CARD, MUTED, PANEL, TEXT};
use eframe::egui::{self, vec2, Align, Color32, Layout, RichText};

#[derive(Clone, Copy, Default, PartialEq)]
enum Category {
    #[default]
    Appearance,
    Canvas,
}

/// The caller supplies the optional GPU glass painter so this view can also run
/// in the CPU-only layout tests without a window or graphics context.
pub(super) fn show(
    ui: &mut egui::Ui,
    preferences: &mut Preferences,
    mut paint_glass: impl FnMut(&egui::Painter, egui::Rect, f32),
) -> bool {
    let before = preferences.clone();
    let category_id = ui.id().with("settings_category");
    let mut category = ui
        .ctx()
        .data_mut(|data| data.get_temp::<Category>(category_id).unwrap_or_default());
    ui.horizontal(|ui| {
        for (tab, label) in [
            (Category::Appearance, "Appearance"),
            (Category::Canvas, "Canvas"),
        ] {
            if panels::section_tab(ui, label, category == tab).clicked() {
                category = tab;
            }
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if motion::button(ui, "Reset")
                .on_hover_text("Reset this section")
                .clicked()
            {
                let defaults = Preferences::default();
                match category {
                    Category::Appearance => {
                        preferences.glass_enabled = defaults.glass_enabled;
                        preferences.glass_strength = defaults.glass_strength;
                        preferences.motion = defaults.motion;
                    }
                    Category::Canvas => preferences.set_snap_settings(&defaults.snap_settings()),
                }
            }
        });
    });
    ui.ctx()
        .data_mut(|data| data.insert_temp(category_id, category));
    ui.add_space(18.0);
    match category {
        Category::Appearance => {
            ui.columns(2, |columns| {
                appearance(&mut columns[0], preferences);
                motion::set_mode(columns[0].ctx(), preferences.motion);
                appearance_preview(&mut columns[1], preferences, &mut paint_glass);
            });
        }
        Category::Canvas => {
            ui.columns(2, |columns| {
                canvas(&mut columns[0], preferences);
                canvas_preview(&mut columns[1], preferences);
            });
        }
    }
    preferences.sanitize();
    *preferences != before
}

fn appearance(ui: &mut egui::Ui, preferences: &mut Preferences) {
    card(ui, |ui| {
        ui.label(RichText::new("Surface").color(TEXT));
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            for (enabled, label) in [(true, "Glass"), (false, "Solid")] {
                if ui
                    .add(
                        motion::Button::new(label)
                            .selected(preferences.glass_enabled == enabled)
                            .min_size(vec2(88.0, 34.0)),
                    )
                    .clicked()
                {
                    preferences.glass_enabled = enabled;
                }
            }
        });
        ui.add_space(15.0);
        ui.add_enabled_ui(preferences.glass_enabled, |ui| {
            ui.label("Frost");
            ui.add(
                egui::Slider::new(&mut preferences.glass_strength, 0.0..=1.0)
                    .show_value(false)
                    .step_by(0.01),
            )
            .on_hover_text("Frost strength");
        });
        ui.add_space(24.0);
        ui.label("Motion");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            for (mode, label) in [
                (Motion::Off, "Off"),
                (Motion::Calm, "Calm"),
                (Motion::Lively, "Lively"),
            ] {
                if ui
                    .add(
                        motion::Button::new(label)
                            .selected(preferences.motion == mode)
                            .min_size(vec2(70.0, 34.0)),
                    )
                    .on_hover_text(match mode {
                        Motion::Off => "Reduce motion: instant feedback",
                        Motion::Calm => "Soft transitions",
                        Motion::Lively => "Soft transitions with a gentle spring",
                    })
                    .clicked()
                {
                    preferences.motion = mode;
                }
            }
        });
    });
}

fn appearance_preview(
    ui: &mut egui::Ui,
    preferences: &Preferences,
    paint_glass: &mut impl FnMut(&egui::Painter, egui::Rect, f32),
) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 304.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 16.0, BG);
    let center = rect.center();
    let shapes = [
        (
            vec2(-54.0, -48.0),
            vec2(106.0, 190.0),
            Color32::from_rgb(62, 106, 159),
        ),
        (
            vec2(66.0, 36.0),
            vec2(100.0, 152.0),
            Color32::from_rgb(111, 80, 142),
        ),
        (
            vec2(12.0, 76.0),
            vec2(144.0, 46.0),
            Color32::from_rgb(59, 123, 138),
        ),
    ];
    for (offset, size, color) in shapes {
        painter.rect_filled(
            egui::Rect::from_center_size(center + offset, size),
            24.0,
            color,
        );
    }
    let surface = egui::Rect::from_center_size(
        center,
        vec2((rect.width() - 40.0).clamp(160.0, 280.0), 156.0),
    );
    if preferences.glass_enabled {
        paint_glass(&painter, surface, preferences.glass_strength);
    } else {
        painter.rect_filled(surface, 16.0, PANEL);
        painter.rect_stroke(
            surface,
            16.0,
            egui::Stroke::new(1.0_f32, Color32::from_white_alpha(28)),
            egui::StrokeKind::Inside,
        );
    }
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(surface.shrink(20.0))
            .layout(Layout::top_down(Align::Center)),
        |ui| {
            ui.add_space(3.0);
            ui.label(RichText::new("Preview").color(TEXT).size(17.0));
            ui.add_space(17.0);
            ui.add(
                motion::Button::new(RichText::new("Try it").color(BG))
                    .fill(ACCENT)
                    .min_size(vec2(120.0, 36.0)),
            );
        },
    );
}

fn canvas(ui: &mut egui::Ui, preferences: &mut Preferences) {
    card(ui, |ui| {
        ui.checkbox(&mut preferences.snap_enabled, "Snapping");
        ui.add_space(15.0);
        ui.add_enabled_ui(preferences.snap_enabled, |ui| {
            ui.checkbox(&mut preferences.snap_canvas, "Canvas center & edges");
            ui.checkbox(&mut preferences.snap_objects, "Objects");
            ui.checkbox(&mut preferences.snap_spacing, "Equal spacing");
            ui.add_space(20.0);
            ui.label("Strength");
            ui.add(
                egui::Slider::new(&mut preferences.snap_threshold_px, 3.0..=16.0)
                    .integer()
                    .suffix(" px"),
            )
            .on_hover_text("On-screen snapping distance at any zoom");
        });
    });
}

fn canvas_preview(ui: &mut egui::Ui, preferences: &Preferences) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 304.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 16.0, BG);
    let sheet = egui::Rect::from_center_size(rect.center(), vec2(180.0, 252.0));
    painter.rect_filled(sheet, 5.0, Color32::from_rgb(221, 225, 232));
    let center = sheet.center();
    for (offset, color) in [
        (vec2(0.0, -73.0), Color32::from_rgb(154, 163, 178)),
        (vec2(0.0, 0.0), ACCENT),
        (vec2(0.0, 73.0), Color32::from_rgb(154, 163, 178)),
    ] {
        painter.rect_filled(
            egui::Rect::from_center_size(center + offset, vec2(118.0, 52.0)),
            4.0,
            color,
        );
    }
    if preferences.snap_enabled {
        let guide = egui::Stroke::new(1.0_f32, Color32::from_rgb(220, 132, 244));
        if preferences.snap_canvas {
            painter.line_segment([sheet.center_top(), sheet.center_bottom()], guide);
        }
        if preferences.snap_objects {
            for x in [center.x - 59.0, center.x + 59.0] {
                painter.line_segment(
                    [
                        egui::pos2(x, sheet.top() + 20.0),
                        egui::pos2(x, sheet.bottom() - 20.0),
                    ],
                    guide,
                );
            }
        }
        if preferences.snap_spacing {
            for y in [center.y - 47.0, center.y + 26.0] {
                let x = center.x + 72.0;
                painter.line_segment([egui::pos2(x, y), egui::pos2(x, y + 21.0)], guide);
                for end in [y, y + 21.0] {
                    painter
                        .line_segment([egui::pos2(x - 4.0, end), egui::pos2(x + 4.0, end)], guide);
                }
            }
        }
    }
}

fn card(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(CARD.gamma_multiply(0.5))
        .corner_radius(12.0)
        .inner_margin(18.0)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.set_min_height(268.0);
            ui.visuals_mut().widgets.inactive.fg_stroke.color = TEXT;
            ui.visuals_mut().widgets.noninteractive.fg_stroke.color = MUTED;
            add_contents(ui);
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_bounds(shape: &egui::Shape, label: &str) -> Option<egui::Rect> {
        match shape {
            egui::Shape::Text(text) if text.galley.job.text == label => {
                Some(text.visual_bounding_rect())
            }
            egui::Shape::Vec(shapes) => shapes.iter().find_map(|shape| text_bounds(shape, label)),
            _ => None,
        }
    }

    #[test]
    fn settings_controls_fit_a_small_workspace_without_scrolling() {
        for category in [Category::Appearance, Category::Canvas] {
            let ctx = egui::Context::default();
            let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(802.0, 600.0));
            let mut preferences = Preferences::default();
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(screen),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let id = ui.id().with("settings_category");
                        ctx.data_mut(|data| data.insert_temp(id, category));
                        show(ui, &mut preferences, |painter, rect, _| {
                            painter.rect_filled(rect, 16.0, PANEL);
                        });
                    });
                },
            );
            let labels = match category {
                Category::Appearance => vec![
                    "Reset",
                    "Appearance",
                    "Canvas",
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
                ],
                Category::Canvas => vec![
                    "Reset",
                    "Appearance",
                    "Canvas",
                    "Snapping",
                    "Canvas center & edges",
                    "Objects",
                    "Equal spacing",
                    "Strength",
                ],
            };
            for label in labels {
                let (rect, clip) = output
                    .shapes
                    .iter()
                    .find_map(|shape| {
                        text_bounds(&shape.shape, label).map(|rect| (rect, shape.clip_rect))
                    })
                    .unwrap_or_else(|| panic!("Missing settings control {label}"));
                assert!(
                    screen.contains_rect(rect),
                    "{label} outside workspace: {rect:?}"
                );
                assert!(
                    clip.expand(1.0).contains_rect(rect),
                    "{label} clipped: {rect:?} / {clip:?}"
                );
            }
            assert_eq!(preferences, Preferences::default());
        }
    }
}
