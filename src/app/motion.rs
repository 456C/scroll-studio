//! Small, finite UI transitions. Artwork and hit targets never move with feedback.
use super::{egui, settings::Motion, Color32};

const MODE_KEY: &str = "scroll_studio_motion";

pub(super) fn set_mode(ctx: &egui::Context, mode: Motion) {
    ctx.data_mut(|data| data.insert_temp(egui::Id::new(MODE_KEY), mode));
}

pub(super) fn mode(ctx: &egui::Context) -> Motion {
    ctx.data(|data| data.get_temp(egui::Id::new(MODE_KEY)).unwrap_or_default())
}

#[derive(Clone, Copy)]
struct Transition {
    from: f32,
    target: f32,
    start: f64,
}

impl Transition {
    fn sample(self, now: f64, duration: f32, spring: bool) -> (f32, bool) {
        if duration <= 0.0 || self.from == self.target {
            return (self.target, false);
        }
        let time = ((now - self.start) as f32 / duration).clamp(0.0, 1.0);
        if time >= 1.0 {
            return (self.target, false);
        }
        let progress = if spring {
            // Damped response: a single soft overshoot, settled within 320 ms.
            1.0 - (-5.0 * time).exp() * ((11.0 * time).cos() + 5.0 / 11.0 * (11.0 * time).sin())
        } else {
            1.0 - (1.0 - time).powi(3)
        };
        (self.from + (self.target - self.from) * progress, true)
    }
}

fn transition(ctx: &egui::Context, id: egui::Id, target: f32, spring: bool) -> f32 {
    let motion = mode(ctx);
    let duration = match (motion, spring) {
        (Motion::Off, _) => 0.0,
        (Motion::Calm, _) => 0.15,
        (Motion::Lively, true) => 0.32,
        (Motion::Lively, false) => 0.19,
    };
    let spring = spring && motion == Motion::Lively;
    let now = ctx.input(|input| input.time);
    let (value, active) = ctx.data_mut(|data| {
        let mut animation = data.get_temp::<Transition>(id).unwrap_or(Transition {
            from: target,
            target,
            start: now,
        });
        if animation.target != target {
            animation = Transition {
                from: animation.sample(now, duration, spring).0,
                target,
                start: now,
            };
        }
        let sample = animation.sample(now, duration, spring);
        if !sample.1 {
            animation.from = target;
        }
        data.insert_temp(id, animation);
        sample
    });
    if active {
        ctx.request_repaint();
    }
    value
}

/// A bounded ease for selection underlines or other decorative scalar values.
pub(super) fn animate_value(ctx: &egui::Context, id: egui::Id, target: f32) -> f32 {
    transition(ctx, id, target, false)
}

/// Interpolate a selection highlight while its real interactive rectangles stay fixed.
pub(super) fn animate_rect(ctx: &egui::Context, id: egui::Id, target: egui::Rect) -> egui::Rect {
    egui::Rect::from_min_size(
        egui::pos2(
            animate_value(ctx, id.with("x"), target.min.x),
            animate_value(ctx, id.with("y"), target.min.y),
        ),
        egui::vec2(
            animate_value(ctx, id.with("width"), target.width()),
            animate_value(ctx, id.with("height"), target.height()),
        ),
    )
}

/// On navigation, reveal the new pane without translating its controls or delaying input.
/// The initial pane is fully visible; switching to reduced motion completes any reveal.
pub(super) fn reveal(ctx: &egui::Context, id: egui::Id, active_key: egui::Id) -> f32 {
    let motion = mode(ctx);
    let now = ctx.input(|input| input.time);
    let started = ctx.data_mut(|data| {
        let state = data.get_temp::<(egui::Id, f64)>(id);
        let state = match state {
            Some((key, start)) if key == active_key => (key, start),
            Some(_) => (active_key, now),
            None => (active_key, f64::NEG_INFINITY),
        };
        data.insert_temp(id, state);
        state.1
    });
    if motion == Motion::Off {
        return 1.0;
    }
    let duration = if motion == Motion::Lively { 0.18 } else { 0.12 };
    let (value, active) = Transition {
        from: 0.0,
        target: 1.0,
        start: started,
    }
    .sample(now, duration, false);
    if active {
        ctx.request_repaint();
    }
    value
}

pub(super) struct Feedback {
    pub hover: f32,
    pub selected: f32,
    pub press: f32,
    pub scale: f32,
}

impl Feedback {
    pub fn paint_rect(&self, rect: egui::Rect) -> egui::Rect {
        egui::Rect::from_center_size(rect.center(), rect.size() * self.scale)
    }
}

pub(super) fn feedback(ui: &egui::Ui, response: &egui::Response, selected: bool) -> Feedback {
    let enabled = ui.is_enabled();
    let id = response.id.with("motion");
    let hover = animate_value(
        ui.ctx(),
        id.with("hover"),
        if enabled && (response.hovered() || response.has_focus()) {
            1.0
        } else {
            0.0
        },
    );
    let selected = animate_value(
        ui.ctx(),
        id.with("selection"),
        if selected { 1.0 } else { 0.0 },
    );
    // A very fast click can begin and end in one input frame. Give it the same
    // release response as an ordinary press instead of losing all feedback.
    if enabled && response.clicked() && !response.is_pointer_button_down_on() {
        let now = ui.ctx().input(|input| input.time);
        ui.ctx().data_mut(|data| {
            data.insert_temp(
                id.with("press"),
                Transition {
                    from: 1.0,
                    target: 0.0,
                    start: now,
                },
            );
        });
    }
    let press = transition(
        ui.ctx(),
        id.with("press"),
        if enabled && response.is_pointer_button_down_on() {
            1.0
        } else {
            0.0
        },
        true,
    );
    let scale = if mode(ui.ctx()) == Motion::Off {
        1.0
    } else {
        (1.0 - press * 0.03).clamp(0.958, 1.012)
    };
    Feedback {
        hover,
        selected,
        press,
        scale,
    }
}

pub(super) fn mix(a: Color32, b: Color32, amount: f32) -> Color32 {
    let a = a.to_array();
    let b = b.to_array();
    let amount = amount.clamp(0.0, 1.0);
    let channel =
        |i| (f32::from(a[i]) + (f32::from(b[i]) - f32::from(a[i])) * amount).round() as u8;
    Color32::from_rgba_premultiplied(channel(0), channel(1), channel(2), channel(3))
}

pub(super) fn button(ui: &mut egui::Ui, text: impl Into<egui::WidgetText>) -> egui::Response {
    ui.add(Button::new(text))
}

pub(super) fn small_button(ui: &mut egui::Ui, text: impl Into<egui::WidgetText>) -> egui::Response {
    ui.add(Button::new(text).small())
}

/// Local acknowledgement for synchronous text-copy actions. Reserve both label
/// widths so feedback never moves the next control or its pointer target.
pub(super) fn copy_button(
    ui: &mut egui::Ui,
    id_salt: impl std::hash::Hash,
    label: &str,
    primary: bool,
) -> egui::Response {
    let id = ui.make_persistent_id(id_salt);
    let now = ui.input(|input| input.time);
    let until = ui
        .ctx()
        .data(|data| data.get_temp::<f64>(id))
        .unwrap_or(0.0);
    let copied = now < until;
    if copied {
        // One wake-up to clear the acknowledgement; no idle animation loop.
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_secs_f64(until - now));
    }
    let font = egui::TextStyle::Button.resolve(ui.style());
    let label_width = [label, "Copied"]
        .into_iter()
        .map(|text| {
            ui.painter()
                .layout_no_wrap(text.to_owned(), font.clone(), super::TEXT)
                .size()
                .x
        })
        .fold(0.0_f32, f32::max);
    let color = if primary {
        super::BG
    } else if copied {
        super::ACCENT
    } else {
        super::TEXT
    };
    let button =
        Button::new(egui::RichText::new(if copied { "Copied" } else { label }).color(color))
            .min_size(egui::vec2(
                label_width + ui.spacing().button_padding.x * 2.0,
                0.0,
            ));
    let response = ui.add(if primary {
        button.fill(super::ACCENT)
    } else {
        button
    });
    if response.clicked() {
        ui.ctx().data_mut(|data| data.insert_temp(id, now + 1.4));
        ui.ctx().request_repaint();
    }
    response
}

/// Keep egui's menu interaction, IDs, popup placement and keyboard handling.
/// Only the parent button's background paint is replaced with finite feedback.
pub(super) fn menu_button<R>(
    ui: &mut egui::Ui,
    title: impl Into<egui::WidgetText>,
    contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<Option<R>> {
    let background = ui.painter().add(egui::Shape::Noop);
    let original_style = ui.style().clone();
    {
        let widgets = &mut ui.style_mut().visuals.widgets;
        for visuals in [
            &mut widgets.inactive,
            &mut widgets.hovered,
            &mut widgets.active,
            &mut widgets.open,
            &mut widgets.noninteractive,
        ] {
            visuals.bg_fill = Color32::TRANSPARENT;
            visuals.weak_bg_fill = Color32::TRANSPARENT;
            visuals.bg_stroke = egui::Stroke::NONE;
        }
    }
    // Native menus create a fresh Area from Context style, so this local Ui
    // paint override does not alter popup fills, spacing or descendants.
    let mut open = false;
    let ctx = ui.ctx().clone();
    let result = super::appearance::with_menu_surface(&ctx, || {
        ui.menu_button(title, |menu_ui| {
            open = true;
            super::appearance::menu_contents(menu_ui, contents)
        })
    });
    ui.set_style(original_style.clone());
    if ui.is_rect_visible(result.response.rect) {
        let feedback = feedback(ui, &result.response, open);
        let visuals = &original_style.visuals;
        let widgets = &visuals.widgets;
        let rest = if visuals.button_frame {
            widgets.inactive.weak_bg_fill
        } else {
            Color32::TRANSPARENT
        };
        let fill = mix(
            mix(
                mix(rest, widgets.hovered.weak_bg_fill, feedback.hover),
                widgets.open.weak_bg_fill,
                feedback.selected,
            ),
            widgets.active.weak_bg_fill,
            feedback.press.max(0.0),
        );
        let stroke = if result.response.has_focus() {
            visuals.selection.stroke
        } else if visuals.button_frame {
            egui::Stroke::new(
                widgets.inactive.bg_stroke.width,
                mix(
                    widgets.inactive.bg_stroke.color,
                    widgets.hovered.bg_stroke.color,
                    feedback.hover * 0.55,
                ),
            )
        } else {
            egui::Stroke::NONE
        };
        // This single shape precedes the native text without adding a layout scope.
        ui.painter().set(
            background,
            egui::epaint::RectShape::new(
                feedback.paint_rect(result.response.rect),
                6.0,
                fill,
                stroke,
                egui::StrokeKind::Inside,
            ),
        );
    }
    result
}

/// Text button with the same layout and keyboard semantics as an egui button.
/// Only paint is compressed on press, preserving target size and surrounding layout.
#[must_use = "Add the button to a Ui"]
pub(super) struct Button {
    text: egui::WidgetText,
    fill: Option<Color32>,
    frame: Option<bool>,
    small: bool,
    min_size: egui::Vec2,
    selected: bool,
    wrap_mode: Option<egui::TextWrapMode>,
}

impl Button {
    pub fn new(text: impl Into<egui::WidgetText>) -> Self {
        Self {
            text: text.into(),
            fill: None,
            frame: None,
            small: false,
            min_size: egui::Vec2::ZERO,
            selected: false,
            wrap_mode: None,
        }
    }

    pub fn fill(mut self, fill: impl Into<Color32>) -> Self {
        self.fill = Some(fill.into());
        self.frame = Some(true);
        self
    }

    pub fn frame(mut self, frame: bool) -> Self {
        self.frame = Some(frame);
        self
    }
    pub fn min_size(mut self, size: egui::Vec2) -> Self {
        self.min_size = size;
        self
    }
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
    pub fn small(mut self) -> Self {
        self.small = true;
        self.text = self.text.text_style(egui::TextStyle::Body);
        self
    }
    pub fn truncate(mut self) -> Self {
        self.wrap_mode = Some(egui::TextWrapMode::Truncate);
        self
    }
}

impl egui::Widget for Button {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let frame = self.frame.unwrap_or(ui.visuals().button_frame);
        let mut padding = if frame {
            ui.spacing().button_padding
        } else {
            egui::Vec2::ZERO
        };
        if self.small {
            padding.y = 0.0;
        }
        let galley = self.text.into_galley(
            ui,
            self.wrap_mode,
            (ui.available_width() - padding.x * 2.0).max(0.0),
            egui::TextStyle::Button,
        );
        let mut size = galley.size() + padding * 2.0;
        if !self.small {
            size.y = size.y.max(ui.spacing().interact_size.y);
        }
        size = size.max(self.min_size);
        // Match egui::Button: a justified parent deliberately expands the
        // button. Empty-page actions must use a bounded child layout instead
        // of changing allocation here (add_sized and full-width menus rely on it).
        let (rect, response) = ui.allocate_at_least(size, egui::Sense::click());
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::WidgetType::Button,
                ui.is_enabled(),
                self.selected,
                galley.text(),
            )
        });
        if ui.is_rect_visible(rect) {
            let feedback = feedback(ui, &response, self.selected);
            let visuals = ui.visuals();
            let widgets = &visuals.widgets;
            let rest = self.fill.unwrap_or(if frame {
                widgets.inactive.weak_bg_fill
            } else {
                Color32::TRANSPARENT
            });
            let hover = self.fill.map_or(widgets.hovered.weak_bg_fill, |fill| {
                mix(fill, Color32::WHITE, 0.07)
            });
            let active = self.fill.map_or(widgets.active.weak_bg_fill, |fill| {
                mix(fill, Color32::BLACK, 0.07)
            });
            let fill = mix(
                mix(
                    mix(rest, hover, feedback.hover),
                    visuals.selection.bg_fill,
                    feedback.selected,
                ),
                active,
                feedback.press.max(0.0),
            );
            let paint_rect = feedback.paint_rect(rect);
            let stroke = if response.has_focus() {
                visuals.selection.stroke
            } else if frame {
                egui::Stroke::new(
                    widgets.inactive.bg_stroke.width,
                    mix(
                        widgets.inactive.bg_stroke.color,
                        widgets.hovered.bg_stroke.color,
                        feedback.hover * 0.55,
                    ),
                )
            } else {
                egui::Stroke::NONE
            };
            ui.painter()
                .rect(paint_rect, 6.0, fill, stroke, egui::StrokeKind::Inside);
            let text_color = mix(
                widgets.inactive.text_color(),
                widgets.hovered.text_color(),
                feedback.hover,
            );
            let text_rect = paint_rect.shrink2(padding);
            let text_pos = ui
                .layout()
                .align_size_within_rect(galley.size(), text_rect)
                .min;
            ui.painter().galley(text_pos, galley, text_color);
        }
        if response.hovered() {
            if let Some(cursor) = ui.visuals().interact_cursor {
                ui.ctx().set_cursor_icon(cursor);
            }
        }
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn button_matches_native_layout_including_explicit_justification_and_narrow_wrapping() {
        #[derive(Clone, Copy, Debug)]
        enum Placement {
            Horizontal,
            Vertical,
            Centered,
            Justified,
            Sized,
            Narrow,
        }

        let render = |custom: bool, placement: Placement| {
            let ctx = egui::Context::default();
            set_mode(&ctx, Motion::Off);
            let mut button_rect = egui::Rect::NOTHING;
            let place = |ui: &mut egui::Ui| {
                if custom {
                    ui.add(Button::new("Add a background")).rect
                } else {
                    ui.add(egui::Button::new("Add a background")).rect
                }
            };
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800.0, 520.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    button_rect = egui::CentralPanel::default()
                        .show(ctx, |ui| match placement {
                            Placement::Horizontal => ui.horizontal(place).inner,
                            Placement::Vertical => ui.vertical(place).inner,
                            Placement::Centered => ui.vertical_centered(place).inner,
                            Placement::Justified => ui.centered_and_justified(place).inner,
                            Placement::Sized => {
                                if custom {
                                    ui.add_sized([260.0, 42.0], Button::new("Add a background"))
                                        .rect
                                } else {
                                    ui.add_sized(
                                        [260.0, 42.0],
                                        egui::Button::new("Add a background"),
                                    )
                                    .rect
                                }
                            }
                            Placement::Narrow => {
                                ui.set_width(54.0);
                                ui.vertical(place).inner
                            }
                        })
                        .inner;
                },
            );
            button_rect
        };
        for placement in [
            Placement::Horizontal,
            Placement::Vertical,
            Placement::Centered,
            Placement::Justified,
            Placement::Sized,
            Placement::Narrow,
        ] {
            let custom = render(true, placement);
            let native = render(false, placement);
            assert_eq!(custom, native, "Layout changed for {placement:?}");
            assert!(custom.is_finite() && custom.is_positive());
            match placement {
                Placement::Justified => {
                    // This is intentional layout behavior, not a button bug.
                    assert!(custom.width() > 700.0 && custom.height() > 400.0);
                }
                Placement::Sized => assert_eq!(custom.size(), egui::vec2(260.0, 42.0)),
                Placement::Centered => assert!(custom.width() < 180.0 && custom.height() < 40.0),
                _ => {}
            }
        }
    }

    #[test]
    fn spring_settles_and_never_changes_the_interaction_rectangle() {
        let press = Transition {
            from: 1.0,
            target: 0.0,
            start: 10.0,
        };
        let original = egui::Rect::from_min_size(egui::pos2(100.0, 60.0), egui::vec2(84.0, 30.0));
        let mut rebound = false;
        for frame in 0..120 {
            let (value, active) = press.sample(10.0 + f64::from(frame) / 120.0, 0.32, true);
            let feedback = Feedback {
                hover: 0.0,
                selected: 0.0,
                press: value,
                scale: (1.0 - value * 0.03).clamp(0.958, 1.012),
            };
            let painted = feedback.paint_rect(original);
            assert!((painted.center().x - original.center().x).abs() < 0.001);
            assert!(painted.width() <= original.width() * 1.012 + 0.001);
            rebound |= painted.width() > original.width();
            if frame >= 40 {
                assert!(!active);
                assert_eq!(value, 0.0);
            }
        }
        assert!(rebound);
    }

    #[test]
    fn reduced_motion_is_immediate_and_a_reversed_ease_is_continuous() {
        let animation = Transition {
            from: 0.0,
            target: 1.0,
            start: 0.0,
        };
        assert_eq!(animation.sample(0.0, 0.0, true), (1.0, false));
        let current = animation.sample(0.08, 0.19, false).0;
        let reversed = Transition {
            from: current,
            target: 0.0,
            start: 0.08,
        };
        assert_eq!(reversed.sample(0.08, 0.19, false).0, current);
        assert!(reversed.sample(0.1, 0.19, false).0 < current);
        assert_eq!(reversed.sample(1.0, 0.19, false), (0.0, false));
    }

    #[test]
    fn animated_button_keeps_its_target_and_stops_requesting_idle_frames() {
        let ctx = egui::Context::default();
        set_mode(&ctx, Motion::Lively);
        let render = |time, events| {
            let mut response = None;
            let output = ctx.run(
                egui::RawInput {
                    time: Some(time),
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(400.0, 240.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        response =
                            Some(ui.add(Button::new("Preview").min_size(egui::vec2(100.0, 32.0))));
                    });
                },
            );
            (response.unwrap(), output)
        };
        let rect = render(0.0, Vec::new()).0.rect;
        let pointer = rect.center();
        render(0.02, vec![egui::Event::PointerMoved(pointer)]);
        let pressed = render(
            0.04,
            vec![egui::Event::PointerButton {
                pos: pointer,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            }],
        )
        .0;
        assert_eq!(pressed.rect, rect);
        let released = render(
            0.1,
            vec![egui::Event::PointerButton {
                pos: pointer,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        )
        .0;
        assert_eq!(released.rect, rect);
        assert!(released.clicked());
        render(1.0, Vec::new());
        let idle = render(1.1, Vec::new()).1;
        assert!(
            idle.viewport_output[&egui::ViewportId::ROOT].repaint_delay
                > std::time::Duration::from_secs(1)
        );
    }

    #[test]
    fn animated_menu_preserves_native_size_popup_style_and_escape() {
        let ctx = egui::Context::default();
        set_mode(&ctx, Motion::Lively);
        let expected_popup_hover = ctx.style().visuals.widgets.hovered.weak_bg_fill;
        let render = |time, events| {
            let mut response = None;
            let mut native_size = egui::Vec2::ZERO;
            let mut popup_shown = false;
            let _ = ctx.run(
                egui::RawInput {
                    time: Some(time),
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(400.0, 240.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            let original_fill = ui.visuals().widgets.inactive.weak_bg_fill;
                            response = Some(menu_button(ui, "File", |ui| {
                                popup_shown = true;
                                assert_eq!(
                                    ui.visuals().widgets.hovered.weak_bg_fill,
                                    expected_popup_hover
                                );
                                let _ = ui.button("Open");
                            }));
                            assert_eq!(ui.visuals().widgets.inactive.weak_bg_fill, original_fill);
                            native_size = ui.add(egui::Button::new("File")).rect.size();
                        });
                    });
                },
            );
            (response.unwrap(), native_size, popup_shown)
        };
        let (initial, native_size, popup_shown) = render(0.0, Vec::new());
        let rect = initial.response.rect;
        assert_eq!(rect.size(), native_size);
        assert!(initial.inner.is_none());
        assert!(!popup_shown);
        let pointer = rect.center();
        render(0.02, vec![egui::Event::PointerMoved(pointer)]);
        render(
            0.04,
            vec![egui::Event::PointerButton {
                pos: pointer,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        let opened = render(
            0.08,
            vec![egui::Event::PointerButton {
                pos: pointer,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert_eq!(opened.0.response.rect, rect);
        assert!(opened.2);
        let closed = render(
            0.12,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert!(!closed.2);
        assert_eq!(closed.0.response.rect, rect);
    }
}
