//! The star button: only shown while an addon that offers it (manifest `assist`) is
//! running. The user types a request; the addon answers by proposing an edit, which goes
//! through the usual review.

use super::addons_ui::Purpose;
use super::*;
use egui::{pos2, vec2, Pos2};
use serde_json::{json, Value};

/// The open request box.
pub(super) struct Assist {
    pub(super) addon: String,
    label: String,
    /// What the request is about, as `addon.assist` sends it.
    target: Value,
    at: Pos2,
    text: String,
    /// The request the addon is working on.
    pub(super) request: Option<u64>,
    pub(super) error: Option<String>,
    /// Focus the text box once, when the box opens.
    focus: bool,
}

impl Studio {
    /// A star button for `target` (`{"kind": "story" | "character" | "panel" | "lore", "id"}`),
    /// or nothing when no addon offers one.
    pub(super) fn assist_star(&mut self, ui: &mut egui::Ui, target: Value) {
        let Some((addon, label)) = self.addons.assistant() else {
            return;
        };
        let open = self
            .addons
            .assist
            .as_ref()
            .is_some_and(|a| a.target == target);
        let button = icons::button(ui, icons::Icon::Star, open, &label);
        if button.clicked() {
            if open {
                self.close_assist();
            } else {
                self.open_assist(
                    addon,
                    label,
                    target,
                    button.rect.left_bottom() + vec2(0.0, 6.0),
                );
            }
        }
    }

    /// A menu entry that opens the request box for `target`, for places where a button
    /// would repeat on every item. Nothing when no addon offers it.
    pub(super) fn assist_menu_item(&mut self, ui: &mut egui::Ui, target: Value) {
        let Some((addon, label)) = self.addons.assistant() else {
            return;
        };
        let at = ui.max_rect().left_top();
        if motion::button(ui, format!("{label}…")).clicked() {
            self.open_assist(addon, label, target, at);
            ui.close_menu();
        }
    }

    fn open_assist(&mut self, addon: String, label: String, target: Value, at: Pos2) {
        self.close_assist();
        self.addons.assist = Some(Assist {
            addon,
            label,
            target,
            at,
            text: String::new(),
            request: None,
            error: None,
            focus: true,
        });
    }

    /// Close the request box, telling the addon to stop if it was working.
    pub(super) fn close_assist(&mut self) {
        if let Some(assist) = self.addons.assist.take() {
            if let Some(id) = assist.request {
                self.addons.cancel(&assist.addon, id);
            }
        }
    }

    pub(super) fn assist_ui(&mut self, ctx: &egui::Context) {
        let Some(assist) = &self.addons.assist else {
            return;
        };
        if self.addons.assistant().map(|(id, _)| id) != Some(assist.addon.clone()) {
            // The addon stopped or was disabled.
            self.addons.assist = None;
            return;
        }
        let working = assist.request.is_some();
        let mut send = false;
        // Escape closes the box only while typing in it, never from elsewhere in the app.
        let mut close = ctx.memory(|m| m.has_focus(egui::Id::new("addon_assist_text")))
            && ctx.input(|i| i.key_pressed(egui::Key::Escape))
            && !self.addon_proposal_open();
        let assist = self.addons.assist.as_mut().expect("checked");
        let width = 340.0_f32.min(ctx.screen_rect().width() - 32.0);
        let at = pos2(
            assist
                .at
                .x
                .clamp(16.0, (ctx.screen_rect().right() - width - 16.0).max(16.0)),
            assist
                .at
                .y
                .min((ctx.screen_rect().bottom() - 200.0).max(16.0)),
        );
        egui::Window::new(&assist.label)
            .id(egui::Id::new("addon_assist"))
            .fixed_pos(at)
            .fixed_size(vec2(width, 0.0))
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                // Taken before the text box sees it, which would otherwise keep the Enter.
                let had_focus = ui.memory(|m| m.has_focus(egui::Id::new("addon_assist_text")));
                let submit = had_focus
                    && !working
                    && ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Enter));
                let edit = ui.add_enabled(
                    !working,
                    egui::TextEdit::multiline(&mut assist.text)
                        .id(egui::Id::new("addon_assist_text"))
                        .hint_text("What should change?")
                        .char_limit(4000)
                        .desired_rows(3)
                        .desired_width(f32::INFINITY),
                );
                if std::mem::take(&mut assist.focus) {
                    edit.request_focus();
                }
                if let Some(error) = &assist.error {
                    ui.label(RichText::new(error).color(MUTED));
                }
                ui.horizontal(|ui| {
                    if working {
                        ui.spinner();
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if !working
                            && ui
                                .add_enabled(
                                    !assist.text.trim().is_empty(),
                                    motion::Button::new(RichText::new("Send").color(BG))
                                        .fill(ACCENT),
                                )
                                .on_hover_text("Ctrl+Enter")
                                .clicked()
                        {
                            send = true;
                        }
                        if motion::button(ui, "Cancel").clicked() {
                            close = true;
                        }
                    });
                });
                send |= submit && !assist.text.trim().is_empty();
            });
        if close {
            self.close_assist();
            return;
        }
        if send {
            let assist = self.addons.assist.as_ref().expect("checked");
            let (addon, params) = (
                assist.addon.clone(),
                json!({ "request": assist.text.trim(), "target": assist.target }),
            );
            let id = self
                .addons
                .ask(&addon, "addon.assist", params, Purpose::Assist);
            let assist = self.addons.assist.as_mut().expect("checked");
            assist.error = id.is_none().then(|| "The addon is busy; try again".into());
            assist.request = id;
        }
    }

    /// The addon finished a star-button request.
    pub(super) fn finish_assist(&mut self, id: u64, outcome: Result<Value, String>) {
        let Some(assist) = self
            .addons
            .assist
            .as_mut()
            .filter(|a| a.request == Some(id))
        else {
            return;
        };
        assist.request = None;
        match outcome {
            // Applied: done. Turned down or nothing to change: the request stays, so it
            // can be reworded and sent again.
            Ok(answer) if answer["applied"].as_bool() == Some(false) => {
                assist.error = answer["summary"]
                    .as_str()
                    .map(|summary| summary.chars().take(300).collect());
            }
            Ok(answer) => {
                if let Some(summary) = answer["summary"].as_str() {
                    self.status = summary.chars().take(200).collect();
                }
                self.addons.assist = None;
            }
            Err(error) => assist.error = Some(error),
        }
    }
}
