use super::*;

/// Chapters edit content and placement; all appearance belongs to a story preset.
pub(super) fn bubble_editor(
    ui: &mut egui::Ui,
    layer: &mut Layer,
    presets: &[BubblePreset],
) -> Option<u64> {
    let before = layer.clone();
    let current = match &layer.kind {
        LayerKind::Text { style, .. } => style.preset_id,
        _ => return None,
    };
    let mut choice = current;
    ui.label("Preset");
    egui::ComboBox::from_id_salt("chapter_bubble_preset")
        .selected_text(
            presets
                .iter()
                .find(|p| Some(p.id) == current)
                .map_or("Choose a preset", |p| p.name.as_str()),
        )
        .width(ui.available_width())
        .show_ui(ui, |ui| {
            for preset in presets {
                ui.selectable_value(&mut choice, Some(preset.id), &preset.name);
            }
        });
    if choice != current {
        if let Some(preset) = presets.iter().find(|p| Some(p.id) == choice) {
            if let Err(error) = scroll_studio::bubbles::apply_preset(layer, preset) {
                ui.colored_label(
                    Color32::LIGHT_RED,
                    format!("Could not apply preset: {error}"),
                );
            }
        }
    }
    let edit = motion::small_button(ui, "Edit preset")
        .on_hover_text("Edit this style across the story")
        .clicked()
        .then_some(choice)
        .flatten();
    ui.add_space(8.0);
    let LayerKind::Text {
        text,
        style,
        bubble,
        ..
    } = &mut layer.kind
    else {
        return edit;
    };
    egui::ScrollArea::vertical()
        .id_salt(("dialogue_editor", layer.id))
        .max_height(88.0)
        .min_scrolled_height(88.0)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add(
                egui::TextEdit::multiline(text)
                    .hint_text("Dialogue")
                    .desired_rows(4)
                    .desired_width(f32::INFINITY),
            );
        });
    ui.add_space(8.0);
    if *bubble && style.shape != BubbleShape::Narration {
        let selection_id = ui.id().with(("pointer_selection", layer.id));
        let mut selected = ui.data_mut(|data| data.get_temp::<usize>(selection_id).unwrap_or(0));
        let enabled: Vec<_> = style
            .tails()
            .enumerate()
            .filter_map(|(index, tail)| tail.enabled.then_some(index))
            .collect();
        if !enabled.contains(&selected) {
            selected = enabled.first().copied().unwrap_or(0);
        }
        ui.horizontal(|ui| {
            ui.label("Pointers");
            if ui
                .add_enabled(
                    enabled.len() < MAX_BUBBLE_POINTERS,
                    motion::Button::new("+").small(),
                )
                .on_hover_text("Add pointer")
                .clicked()
            {
                add_pointer(style);
                selected = style
                    .tails()
                    .enumerate()
                    .find_map(|(index, tail)| {
                        (tail.enabled && !enabled.contains(&index)).then_some(index)
                    })
                    .unwrap_or(selected);
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            ui.spacing_mut().button_padding = egui::vec2(6.0, 4.0);
            for (index, _) in style.tails().enumerate().filter(|(_, tail)| tail.enabled) {
                ui.selectable_value(&mut selected, index, (index + 1).to_string());
            }
        });
        let mut remove = false;
        if let Some(tail) = style.tail_at_mut(selected).filter(|tail| tail.enabled) {
            ui.horizontal(|ui| {
                ui.spacing_mut().button_padding = egui::vec2(6.0, 3.0);
                ui.spacing_mut().item_spacing.x = 4.0;
                for (direction, hint, tip) in [
                    (egui::vec2(-1.0, 0.0), "Point left", [-0.35, 0.65]),
                    (egui::vec2(0.0, -1.0), "Point up", [0.35, -0.45]),
                    (egui::vec2(0.0, 1.0), "Point down", [0.65, 1.45]),
                    (egui::vec2(1.0, 0.0), "Point right", [1.35, 0.65]),
                ] {
                    if direction_button(ui, direction, hint).clicked() {
                        tail.tip = tip;
                    }
                }
                motion::menu_button(ui, "…", |ui| {
                    ui.label("Target");
                    for (axis, (origin, dimension)) in
                        [(layer.x, layer.width), (layer.y, layer.height)]
                            .into_iter()
                            .enumerate()
                    {
                        let mut value = origin + tail.tip[axis] * dimension;
                        ui.horizontal(|ui| {
                            ui.label(if axis == 0 { "X" } else { "Y" });
                            if ui
                                .add(egui::DragValue::new(&mut value).speed(2.0).range(
                                    (origin - MAX_POINTER_OFFSET)..=(origin + MAX_POINTER_OFFSET),
                                ))
                                .changed()
                            {
                                tail.tip[axis] = (value - origin) / dimension;
                            }
                        });
                    }
                    ui.separator();
                    if motion::button(ui, "Remove pointer").clicked() {
                        remove = true;
                        ui.close_menu();
                    }
                });
            });
        }
        if remove {
            remove_pointer(style, selected);
        }
        ui.data_mut(|data| data.insert_temp(selection_id, selected));
    }
    let mut fit = false;
    ui.horizontal(|ui| {
        fit = motion::button(ui, "Fit text").clicked();
        motion::menu_button(ui, "Position", |ui| {
            ui.checkbox(&mut style.auto_fit, "Auto-fit");
            egui::Grid::new("bubble_placement")
                .num_columns(2)
                .show(ui, |ui| {
                    ui.label("X");
                    ui.add(egui::DragValue::new(&mut layer.x).speed(2.0));
                    ui.end_row();
                    ui.label("Y");
                    ui.add(egui::DragValue::new(&mut layer.y).speed(2.0));
                    ui.end_row();
                    ui.label("Width");
                    if ui
                        .add(
                            egui::DragValue::new(&mut layer.width)
                                .range(40.0..=16384.0)
                                .speed(2.0),
                        )
                        .changed()
                    {
                        style.auto_fit = false;
                    }
                    ui.end_row();
                    ui.label("Height");
                    if ui
                        .add(
                            egui::DragValue::new(&mut layer.height)
                                .range(20.0..=200000.0)
                                .speed(2.0),
                        )
                        .changed()
                    {
                        style.auto_fit = false;
                    }
                    ui.end_row();
                });
        });
    });
    update_text_fit(&before, layer, fit);
    if compositor::text_overflows(layer) {
        ui.label(
            RichText::new("Text clipped · Fit text")
                .size(11.0)
                .color(Color32::from_rgb(243, 183, 108)),
        );
    }
    edit
}

fn direction_button(ui: &mut egui::Ui, direction: egui::Vec2, label: &str) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(28.0, 25.0), egui::Sense::click());
    let visuals = ui.style().interact(&response);
    ui.painter().rect(
        rect,
        4.0,
        visuals.weak_bg_fill,
        visuals.bg_stroke,
        egui::StrokeKind::Inside,
    );
    let center = rect.center();
    let tip = center + direction * 6.0;
    let cross = egui::vec2(-direction.y, direction.x) * 3.5;
    let stroke = egui::Stroke::new(1.5_f32, visuals.fg_stroke.color);
    ui.painter()
        .line_segment([center - direction * 6.0, tip], stroke);
    ui.painter()
        .line_segment([tip - direction * 4.0 + cross, tip], stroke);
    ui.painter()
        .line_segment([tip - direction * 4.0 - cross, tip], stroke);
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    response.on_hover_text(label)
}

fn add_pointer(style: &mut TextStyle) {
    let tips = [
        [0.7, 1.5],
        [-0.35, 0.65],
        [1.35, 0.65],
        [0.35, -0.45],
        [0.2, 1.6],
        [0.8, -0.6],
        [-0.5, 0.2],
        [1.5, 0.2],
    ];
    let enabled = style.tails().filter(|t| t.enabled).count();
    if enabled >= MAX_BUBBLE_POINTERS {
        return;
    }
    let tip = tips
        .iter()
        .copied()
        .find(|candidate| !style.tails().any(|t| t.enabled && t.tip == *candidate))
        .unwrap_or(tips[enabled]);
    let mut tail = style.tail.clone();
    tail.enabled = true;
    tail.tip = tip;
    if let Some(disabled) = style.tails_mut().find(|t| !t.enabled) {
        *disabled = tail;
        return;
    }
    if style.extra_tails.len() + 1 < MAX_BUBBLE_POINTERS {
        style.extra_tails.push(tail);
    }
}

fn remove_pointer(style: &mut TextStyle, index: usize) {
    if index == 0 {
        if style.extra_tails.is_empty() {
            style.tail.enabled = false;
        } else {
            style.tail = style.extra_tails.remove(0);
        }
    } else if index <= style.extra_tails.len() {
        style.extra_tails.remove(index - 1);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AppearanceTab {
    Style,
    Text,
    Bubble,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum PresetSlider {
    Size,
    Spacing,
    MaxWidth,
    Outline,
    Padding,
    PointerWidth,
    Curve,
    Opacity,
}

impl PresetSlider {
    pub(super) fn is_only_change(self, original: &BubblePreset, edited: &BubblePreset) -> bool {
        let mut edited = edited.clone();
        self.restore_value(original, &mut edited);
        edited == *original
    }

    pub(super) fn restore_value(self, original: &BubblePreset, edited: &mut BubblePreset) {
        let (target, value) = match self {
            Self::Size => (&mut edited.size, original.size),
            Self::Spacing => (&mut edited.style.line_spacing, original.style.line_spacing),
            Self::MaxWidth => (&mut edited.style.max_width, original.style.max_width),
            Self::Outline => (
                &mut edited.style.outline_width,
                original.style.outline_width,
            ),
            Self::Padding => (&mut edited.style.padding, original.style.padding),
            Self::PointerWidth => (&mut edited.style.tail.width, original.style.tail.width),
            Self::Curve => (&mut edited.style.tail.bend, original.style.tail.bend),
            Self::Opacity => (&mut edited.opacity, original.opacity),
        };
        *target = value;
    }
}

#[derive(Clone, Copy)]
pub(super) struct PresetSliderDrag {
    pub property: PresetSlider,
    pub started: bool,
    pub stopped: bool,
}

/// One focused group at a time keeps the whole editor within the inspector.
pub(super) fn preset_appearance_editor(
    ui: &mut egui::Ui,
    preset: &mut BubblePreset,
) -> Option<PresetSliderDrag> {
    let mut drag = None;
    let tab_id = ui.id().with(("appearance_tab", preset.id));
    let mut tab = ui.data_mut(|data| data.get_temp(tab_id).unwrap_or(AppearanceTab::Style));
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.spacing_mut().button_padding = egui::vec2(10.0, 6.0);
        ui.selectable_value(&mut tab, AppearanceTab::Style, "Style");
        ui.selectable_value(&mut tab, AppearanceTab::Text, "Text");
        ui.selectable_value(&mut tab, AppearanceTab::Bubble, "Bubble");
    });
    ui.data_mut(|data| data.insert_temp(tab_id, tab));
    ui.add_space(6.0);
    let style = &mut preset.style;
    match tab {
        AppearanceTab::Style => {
            for theme in LetteringTheme::ALL {
                let theme_style = TextStyle::for_theme(theme);
                if ui
                    .add_sized(
                        [ui.available_width(), 40.0],
                        motion::Button::new(
                            RichText::new(theme.label())
                                .family(egui::FontFamily::Name(theme_style.font.label().into()))
                                .size(16.0),
                        )
                        .selected(style.theme() == Some(theme)),
                    )
                    .clicked()
                {
                    style.font = theme_style.font;
                    style.uppercase = theme_style.uppercase;
                    style.padding =
                        (theme_style.padding * preset.size / BubblePreset::BASE_SIZE).min(256.0);
                    style.line_spacing = theme_style.line_spacing;
                    style.align = theme_style.align;
                    style.modern_layout = true;
                }
            }
            ui.add_space(6.0);
            ui.label("Shape");
            egui::ComboBox::from_id_salt("preset_shape")
                .selected_text(if preset.bubble {
                    style.shape.label()
                } else {
                    "Text only"
                })
                .width(ui.available_width())
                .show_ui(ui, |ui| {
                    for shape in BubbleShape::ALL {
                        if ui
                            .selectable_label(preset.bubble && style.shape == shape, shape.label())
                            .on_hover_text(shape_hint(shape))
                            .clicked()
                        {
                            preset.bubble = true;
                            style.shape = shape;
                            style.modern_layout = true;
                        }
                    }
                    if ui.selectable_label(!preset.bubble, "Text only").clicked() {
                        preset.bubble = false;
                    }
                });
            setting_slider(
                ui,
                PresetSlider::Size,
                "Size",
                &mut preset.size,
                12.0..=120.0,
                &mut drag,
            );
        }
        AppearanceTab::Text => {
            ui.label("Font");
            egui::ComboBox::from_id_salt("preset_font")
                .selected_text(style.font.label())
                .width(ui.available_width())
                .show_ui(ui, |ui| {
                    for font in FontChoice::ALL {
                        ui.selectable_value(&mut style.font, font, font.label());
                    }
                });
            ui.horizontal(|ui| {
                ui.checkbox(&mut style.uppercase, "Uppercase");
                ui.color_edit_button_srgba_unmultiplied(&mut preset.color)
                    .on_hover_text("Text color");
            });
            ui.horizontal(|ui| {
                ui.spacing_mut().button_padding = egui::vec2(8.0, 5.0);
                for align in TextAlign::ALL {
                    ui.selectable_value(&mut style.align, align, align.label());
                }
            });
            setting_slider(
                ui,
                PresetSlider::Size,
                "Size",
                &mut preset.size,
                12.0..=120.0,
                &mut drag,
            );
            setting_slider(
                ui,
                PresetSlider::Spacing,
                "Spacing",
                &mut style.line_spacing,
                0.8..=2.0,
                &mut drag,
            );
            setting_slider(
                ui,
                PresetSlider::MaxWidth,
                "Max width",
                &mut style.max_width,
                120.0..=2000.0,
                &mut drag,
            );
        }
        AppearanceTab::Bubble => {
            ui.add_enabled_ui(preset.bubble, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Fill");
                    ui.color_edit_button_srgba_unmultiplied(&mut style.fill);
                    ui.label("Outline");
                    ui.color_edit_button_srgba_unmultiplied(&mut style.outline);
                });
                setting_slider(
                    ui,
                    PresetSlider::Outline,
                    "Outline",
                    &mut style.outline_width,
                    0.0..=16.0,
                    &mut drag,
                );
                setting_slider(
                    ui,
                    PresetSlider::Padding,
                    "Padding",
                    &mut style.padding,
                    4.0..=100.0,
                    &mut drag,
                );
                if style.shape != BubbleShape::Narration {
                    setting_slider(
                        ui,
                        PresetSlider::PointerWidth,
                        "Pointer width",
                        &mut style.tail.width,
                        8.0..=160.0,
                        &mut drag,
                    );
                    if style.shape != BubbleShape::Thought {
                        setting_slider(
                            ui,
                            PresetSlider::Curve,
                            "Curve",
                            &mut style.tail.bend,
                            -1.0..=1.0,
                            &mut drag,
                        );
                    }
                }
            });
            setting_slider(
                ui,
                PresetSlider::Opacity,
                "Opacity",
                &mut preset.opacity,
                0.0..=1.0,
                &mut drag,
            );
        }
    }
    drag
}

fn update_text_fit(before: &Layer, layer: &mut Layer, fit: bool) {
    if let LayerKind::Text { style, .. } = &mut layer.kind {
        if fit {
            style.auto_fit = true;
        }
        if style.auto_fit {
            style.modern_layout = true;
        }
    }
    let layout_changed = match (&before.kind, &layer.kind) {
        (
            LayerKind::Text {
                text: a,
                size: a_size,
                bubble: a_bubble,
                style: a_style,
                ..
            },
            LayerKind::Text {
                text: b,
                size: b_size,
                bubble: b_bubble,
                style: b_style,
                ..
            },
        ) => {
            a != b
                || a_size != b_size
                || a_bubble != b_bubble
                || a_style.font != b_style.font
                || a_style.shape != b_style.shape
                || a_style.padding != b_style.padding
                || a_style.line_spacing != b_style.line_spacing
                || a_style.uppercase != b_style.uppercase
                || a_style.max_width != b_style.max_width
                || a_style.auto_fit != b_style.auto_fit
                || a_style.modern_layout != b_style.modern_layout
        }
        _ => false,
    };
    if let LayerKind::Text { style, .. } = &layer.kind {
        if style.auto_fit && (fit || layout_changed) {
            if let Ok([width, height]) = compositor::fit_text_size(layer, style.max_width) {
                let center = [layer.x + layer.width * 0.5, layer.y + layer.height * 0.5];
                layer.width = width.clamp(40.0, 16384.0);
                layer.height = height.clamp(20.0, 200000.0);
                layer.x = center[0] - layer.width * 0.5;
                layer.y = center[1] - layer.height * 0.5;
            }
        }
    }
    // Resizing to fit dialogue must not redirect an already aimed pointer.
    // Explicit direction/Reset pointer changes still take precedence.
    if before.width != layer.width || before.height != layer.height {
        let targets: Vec<_> = pointer_targets(before).into_iter().filter(|(index, _)| {
            matches!((&before.kind, &layer.kind),
                (LayerKind::Text { style: old, .. }, LayerKind::Text { style: new, .. })
                if old.tail_at(*index).zip(new.tail_at(*index)).is_some_and(|(a,b)| a.tip == b.tip && b.enabled))
        }).collect();
        preserve_pointer_targets(&targets, layer);
    }
}

fn setting_slider(
    ui: &mut egui::Ui,
    property: PresetSlider,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    drag: &mut Option<PresetSliderDrag>,
) {
    let response = ui
        .push_id(property, |ui| {
            ui.label(label);
            ui.add(egui::Slider::new(value, range).clamping(egui::SliderClamping::Edits))
        })
        .inner;
    #[cfg(test)]
    ui.data_mut(|data| {
        data.insert_temp(
            egui::Id::new(("preset_slider_bounds", property)),
            (response.rect, ui.spacing().slider_width, response.id),
        );
    });
    // The number field and keyboard arrows remain independent edits. Color
    // popups deliberately never enter this slider-only transaction path.
    if response.dragged_by(egui::PointerButton::Primary)
        || response.drag_stopped_by(egui::PointerButton::Primary)
    {
        *drag = Some(PresetSliderDrag {
            property,
            started: response.drag_started(),
            stopped: response.drag_stopped(),
        });
    }
}

pub(super) fn pointer_targets(layer: &Layer) -> Vec<(usize, [f32; 2])> {
    let LayerKind::Text { style, bubble, .. } = &layer.kind else {
        return Vec::new();
    };
    if !*bubble || style.shape == BubbleShape::Narration {
        return Vec::new();
    }
    style
        .tails()
        .enumerate()
        .filter(|(_, tail)| tail.enabled)
        .map(|(index, tail)| {
            (
                index,
                [
                    layer.x + tail.tip[0] * layer.width,
                    layer.y + tail.tip[1] * layer.height,
                ],
            )
        })
        .collect()
}

pub(super) fn preserve_pointer_targets(targets: &[(usize, [f32; 2])], layer: &mut Layer) {
    if let LayerKind::Text { style, bubble, .. } = &mut layer.kind {
        if !*bubble || style.shape == BubbleShape::Narration {
            return;
        }
        for (index, target) in targets {
            if let Some(tail) = style.tail_at_mut(*index).filter(|tail| tail.enabled) {
                tail.tip = [
                    (target[0] - layer.x).clamp(-MAX_POINTER_OFFSET, MAX_POINTER_OFFSET)
                        / layer.width,
                    (target[1] - layer.y).clamp(-MAX_POINTER_OFFSET, MAX_POINTER_OFFSET)
                        / layer.height,
                ];
            }
        }
    }
}

#[cfg(test)]
pub(super) fn pointer_target(layer: &Layer) -> Option<[f32; 2]> {
    pointer_targets(layer)
        .into_iter()
        .find(|(index, _)| *index == 0)
        .map(|(_, target)| target)
}

fn shape_hint(shape: BubbleShape) -> &'static str {
    match shape {
        BubbleShape::Speech => "Everyday dialogue · oval with a pointer",
        BubbleShape::Rounded => "Everyday dialogue · soft rounded box",
        BubbleShape::Thought => "Inner thoughts · cloud with a trail of dots",
        BubbleShape::Shout => "Raised voices · a sharp burst",
        BubbleShape::Whisper => "Quiet speech · a dashed outline",
        BubbleShape::Narration => "Narration or inner monologue · a box without a pointer",
        BubbleShape::Electronic => "Phone, radio or speaker · an electric outline",
        BubbleShape::Intensity => "Dense, irregular spikes around an oval",
        BubbleShape::Impact => "Curved sides with sharp reaction points",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opening_appearance_tabs_never_clamps_saved_values_to_slider_ranges() {
        let ctx = egui::Context::default();
        let _studio = Studio::from_context(&ctx);
        let mut preset = BubblePreset::for_canvas(1, LetteringTheme::Casual, 8192);
        let before = preset.clone();
        for tab in [
            AppearanceTab::Style,
            AppearanceTab::Text,
            AppearanceTab::Bubble,
        ] {
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.data_mut(|data| {
                        data.insert_temp(ui.id().with(("appearance_tab", preset.id)), tab)
                    });
                    preset_appearance_editor(ui, &mut preset);
                });
            });
            assert_eq!(
                preset, before,
                "Viewing an appearance tab must not edit the story"
            );
        }
    }

    #[test]
    fn appearance_categories_fit_a_compact_inspector() {
        let ctx = egui::Context::default();
        let studio = Studio::from_context(&ctx);
        for tab in [
            AppearanceTab::Style,
            AppearanceTab::Text,
            AppearanceTab::Bubble,
        ] {
            let mut preset = studio.story.bubble_presets[0].clone();
            for _ in 0..2 {
                let _ = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1024.0, 700.0),
                        )),
                        ..Default::default()
                    },
                    |ctx| {
                        egui::SidePanel::right("test_inspector")
                            .exact_width(280.0)
                            .frame(egui::Frame::new().inner_margin(16.0))
                            .show(ctx, |ui| {
                                let start = ui.cursor().top();
                                ui.data_mut(|data| {
                                    data.insert_temp(
                                        ui.id().with(("appearance_tab", preset.id)),
                                        tab,
                                    )
                                });
                                preset_appearance_editor(ui, &mut preset);
                                assert!(
                                    ui.cursor().top() - start < 430.0,
                                    "The active category must fit without page scrolling"
                                );
                                assert!(
                                    ui.min_rect().width() <= 249.0,
                                    "Controls must fit the inspector width"
                                );
                            });
                    },
                );
            }
        }
    }

    #[test]
    fn long_dialogue_and_eight_pointers_keep_editor_bounded() {
        let ctx = egui::Context::default();
        let studio = Studio::from_context(&ctx);
        let mut layer = bubble();
        edit_text(&mut layer, &"Long dialogue\n".repeat(300));
        if let LayerKind::Text { style, .. } = &mut layer.kind {
            for _ in 0..MAX_BUBBLE_POINTERS {
                add_pointer(style);
            }
        }
        let original = layer.clone();
        for _ in 0..2 {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1024.0, 700.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::SidePanel::right("test_dialogue_inspector")
                        .exact_width(280.0)
                        .frame(egui::Frame::new().inner_margin(16.0))
                        .show(ctx, |ui| {
                            let start = ui.cursor().top();
                            bubble_editor(ui, &mut layer, &studio.story.bubble_presets);
                            assert!(
                                ui.cursor().top() - start < 440.0,
                                "Dialogue length and pointer count cannot grow the page"
                            );
                            assert!(ui.min_rect().width() <= 249.0);
                        });
                },
            );
        }
        assert_eq!(
            layer, original,
            "Navigating the editor must not rewrite the bubble"
        );
    }

    #[test]
    fn adding_and_removing_pointers_preserves_other_targets_and_obeys_limit() {
        let mut style = TextStyle::dialogue();
        let first = style.tail.tip;
        for _ in 0..MAX_BUBBLE_POINTERS + 2 {
            add_pointer(&mut style);
        }
        assert_eq!(
            style.tails().filter(|tail| tail.enabled).count(),
            MAX_BUBBLE_POINTERS
        );
        assert_eq!(style.tail.tip, first);
        let second = style.extra_tails[0].clone();
        remove_pointer(&mut style, 0);
        assert_eq!(style.tail, second);
        while !style.extra_tails.is_empty() {
            remove_pointer(&mut style, 1);
        }
        remove_pointer(&mut style, 0);
        assert!(!style.tail.enabled);
        add_pointer(&mut style);
        assert!(style.tail.enabled && style.extra_tails.is_empty());
    }

    #[test]
    fn typing_refits_all_pointers_without_changing_the_shared_preset_link() {
        let mut layer = bubble();
        if let LayerKind::Text { style, .. } = &mut layer.kind {
            style.preset_id = Some(77);
            add_pointer(style);
        }
        let targets = pointer_targets(&layer);
        let before = layer.clone();
        edit_text(&mut layer, "Both of us are speaking. This dialogue needs more room, but the pointers should stay aimed at us.");
        update_text_fit(&before, &mut layer, true);
        for ((a_index, a), (b_index, b)) in targets.into_iter().zip(pointer_targets(&layer)) {
            assert_eq!(a_index, b_index);
            assert_point_near(a, b);
        }
        let LayerKind::Text { style, .. } = &layer.kind else {
            panic!()
        };
        assert_eq!(style.preset_id, Some(77));
    }

    fn bubble() -> Layer {
        Layer {
            extensions: Default::default(),
            id: 1,
            name: "Dialogue".into(),
            visible: true,
            locked: false,
            opacity: 1.0,
            x: 100.0,
            y: 200.0,
            width: 420.0,
            height: 160.0,
            kind: LayerKind::Text {
                text: "Hi.".into(),
                color: [0, 0, 0, 255],
                size: 38.0,
                bubble: true,
                style: TextStyle::dialogue(),
            },
        }
    }

    fn edit_text(layer: &mut Layer, value: &str) {
        if let LayerKind::Text { text, .. } = &mut layer.kind {
            *text = value.into();
        }
    }

    fn assert_point_near(a: [f32; 2], b: [f32; 2]) {
        assert!(
            (a[0] - b[0]).abs() < 0.001 && (a[1] - b[1]).abs() < 0.001,
            "{a:?} != {b:?}"
        );
    }

    #[test]
    fn typing_and_deleting_fit_both_dimensions_without_moving_center_or_speaker() {
        let mut layer = bubble();
        update_text_fit(&layer.clone(), &mut layer, true);
        let short_size = [layer.width, layer.height];
        let center = [layer.x + layer.width * 0.5, layer.y + layer.height * 0.5];
        let target = pointer_target(&layer).unwrap();
        let before = layer.clone();
        edit_text(&mut layer, "I thought this station was empty. If we miss the last train, we will have to walk all the way across the city tonight.");
        update_text_fit(&before, &mut layer, false);
        let long_size = [layer.width, layer.height];
        assert!(long_size[0] > short_size[0] && long_size[1] > short_size[1]);
        assert_point_near(
            [layer.x + layer.width * 0.5, layer.y + layer.height * 0.5],
            center,
        );
        assert_point_near(pointer_target(&layer).unwrap(), target);
        assert!(!compositor::text_overflows(&layer));
        let before = layer.clone();
        edit_text(&mut layer, "Hi.");
        update_text_fit(&before, &mut layer, false);
        assert!(layer.width < long_size[0] && layer.height < long_size[1]);
        assert_point_near([layer.width, layer.height], short_size);
        assert_point_near(
            [layer.x + layer.width * 0.5, layer.y + layer.height * 0.5],
            center,
        );
        assert_point_near(pointer_target(&layer).unwrap(), target);
    }

    #[test]
    fn manual_size_stays_fixed_until_fit_is_requested() {
        let mut layer = bubble();
        if let LayerKind::Text { style, .. } = &mut layer.kind {
            style.auto_fit = false;
        }
        let before = layer.clone();
        edit_text(&mut layer, "A considerably longer line of dialogue needs enough space to breathe inside the bubble.");
        update_text_fit(&before, &mut layer, false);
        assert_eq!((layer.width, layer.height), (420.0, 160.0));
        update_text_fit(&layer.clone(), &mut layer, true);
        let LayerKind::Text { style, .. } = &layer.kind else {
            unreachable!()
        };
        assert!(style.auto_fit);
        assert!(!compositor::text_overflows(&layer));
    }

    #[test]
    fn explicit_pointer_direction_takes_precedence_during_fit() {
        let mut layer = bubble();
        let before = layer.clone();
        if let LayerKind::Text { style, .. } = &mut layer.kind {
            style.tail.tip = [-0.35, 0.65];
        }
        update_text_fit(&before, &mut layer, true);
        let LayerKind::Text { style, .. } = &layer.kind else {
            unreachable!()
        };
        assert_eq!(style.tail.tip, [-0.35, 0.65]);
    }

    #[test]
    fn shrinking_long_dialogue_keeps_a_distant_speaker_target() {
        let mut layer = bubble();
        edit_text(
            &mut layer,
            &"There is a long way to go before we reach the city. ".repeat(40),
        );
        update_text_fit(&layer.clone(), &mut layer, true);
        if let LayerKind::Text { style, .. } = &mut layer.kind {
            style.tail.tip = [1.4, 3.0];
        }
        let before = layer.clone();
        let target = pointer_target(&layer).unwrap();
        edit_text(&mut layer, "Hi.");
        update_text_fit(&before, &mut layer, false);
        let LayerKind::Text { style, .. } = &layer.kind else {
            unreachable!()
        };
        assert!(
            style.tail.tip[1] > 5.0,
            "A shorter body needs a larger relative pointer offset"
        );
        assert_point_near(pointer_target(&layer).unwrap(), target);
    }

    #[test]
    fn fitting_legacy_text_opts_into_modern_layout_and_manual_resize_keeps_it() {
        let mut layer = bubble();
        if let LayerKind::Text { style, .. } = &mut layer.kind {
            *style = TextStyle::default();
            assert!(!style.modern_layout);
        }
        update_text_fit(&layer.clone(), &mut layer, true);
        let LayerKind::Text { style, .. } = &layer.kind else {
            unreachable!()
        };
        assert!(style.modern_layout && style.auto_fit);
        assert_eq!(style.font, FontChoice::Karla);
        let before = layer.clone();
        if let LayerKind::Text { style, .. } = &mut layer.kind {
            style.auto_fit = false;
        }
        layer.width = 450.0;
        layer.height = 200.0;
        update_text_fit(&before, &mut layer, false);
        let LayerKind::Text { style, .. } = &layer.kind else {
            unreachable!()
        };
        assert!(style.modern_layout);
        assert!(!style.auto_fit);
        assert_eq!((layer.width, layer.height), (450.0, 200.0));
    }
}
