use super::*;

/// Long sheet text is a reading surface until the artist explicitly chooses Edit.
/// Keep its scroll area bounded so category navigation never moves off screen.
pub(super) fn text_panel(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut String,
    hint: &str,
    height: f32,
    limit: usize,
) {
    let id = ui.make_persistent_id(id);
    let key = egui::Id::new("active_world_text_editor");
    let mut editing = ui.data(|d| d.get_temp::<egui::Id>(key)) == Some(id);
    let mut begin = false;
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let response = motion::button(ui, if editing { "Done" } else { "Edit" });
            #[cfg(test)]
            ui.data_mut(|d| d.insert_temp(egui::Id::new("text_panel_toggle"), response.rect));
            if response.clicked() {
                editing = !editing;
                begin = editing;
                if editing {
                    ui.data_mut(|d| d.insert_temp(key, id));
                } else {
                    ui.data_mut(|d| d.remove::<egui::Id>(key));
                    ui.memory_mut(|m| m.surrender_focus(id));
                }
            }
        });
    });
    egui::ScrollArea::vertical()
        .id_salt(id.with("scroll"))
        .max_height((height - 38.0).max(36.0))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if editing {
                let response = ui.add(
                    egui::TextEdit::multiline(value)
                        .id(id)
                        .hint_text(hint)
                        .char_limit(limit)
                        .desired_width(f32::INFINITY)
                        .min_size(egui::vec2(0.0, (height - 48.0).max(30.0)))
                        .desired_rows(1),
                );
                if begin {
                    response.request_focus();
                }
                if response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    response.surrender_focus();
                    ui.data_mut(|d| d.remove::<egui::Id>(key));
                }
            } else if value.is_empty() {
                ui.label(RichText::new(hint).color(MUTED));
            } else {
                ui.add(egui::Label::new(value.as_str()).wrap().selectable(true));
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reading_does_not_request_keyboard_and_edit_is_explicit() {
        let ctx = egui::Context::default();
        let mut value = "A quiet station".to_owned();
        let run = |events, value: &mut String| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(400.0, 350.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        text_panel(ui, "test", value, "Notes", 300.0, 20000)
                    });
                },
            )
        };
        let output = run(vec![], &mut value);
        assert!(output.platform_output.ime.is_none());
        let edit = ctx
            .data(|d| {
                d.get_temp::<egui::Rect>(egui::Id::new("text_panel_toggle"))
                    .unwrap()
            })
            .center();
        let click = |point| {
            vec![
                egui::Event::PointerMoved(point),
                egui::Event::PointerButton {
                    pos: point,
                    pressed: true,
                    button: egui::PointerButton::Primary,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::PointerButton {
                    pos: point,
                    pressed: false,
                    button: egui::PointerButton::Primary,
                    modifiers: egui::Modifiers::NONE,
                },
            ]
        };
        let output = run(click(egui::pos2(100.0, 100.0)), &mut value);
        assert!(output.platform_output.ime.is_none());
        run(click(edit), &mut value);
        run(vec![egui::Event::Text("!".into())], &mut value);
        assert!(value.contains('!'));
        let done = ctx
            .data(|d| {
                d.get_temp::<egui::Rect>(egui::Id::new("text_panel_toggle"))
                    .unwrap()
            })
            .center();
        let output = run(click(done), &mut value);
        assert!(output.platform_output.ime.is_none());
        assert!(!ctx.wants_keyboard_input());
    }
}
