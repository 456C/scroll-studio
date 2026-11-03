use super::*;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum Icon {
    Move,
    Lasso,
    Select,
    Crop,
    SmartLasso,
    Eraser,
    Brush,
    Heal,
    Bubble,
    Beat,
    Undo,
    Redo,
    Magnet,
    ChevronDown,
    ChevronLeft,
    ChevronRight,
    Chapter,
    Characters,
    Groups,
    Timeline,
    Lore,
    Background,
    Image,
    Settings,
    Eye,
    EyeOff,
    Lock,
    Unlock,
    ChevronsUp,
    ChevronsDown,
    More,
}

pub(super) fn button(ui: &mut egui::Ui, icon: Icon, selected: bool, label: &str) -> egui::Response {
    sized_button(ui, icon, selected, label, 30.0)
}

pub(super) fn small_button(
    ui: &mut egui::Ui,
    icon: Icon,
    selected: bool,
    label: &str,
) -> egui::Response {
    sized_button(ui, icon, selected, label, 24.0)
}

fn sized_button(
    ui: &mut egui::Ui,
    icon: Icon,
    selected: bool,
    label: &str,
    size: f32,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), selected, label)
    });
    let feedback = motion::feedback(ui, &response, selected);
    let painted = feedback.paint_rect(rect);
    let base = motion::mix(Color32::TRANSPARENT, CARD, feedback.hover);
    let fill = motion::mix(base, ui.visuals().selection.bg_fill, feedback.selected);
    ui.painter().rect_filled(painted, 7.0, fill);
    if response.has_focus() {
        ui.painter().rect_stroke(
            painted,
            7.0,
            egui::Stroke::new(1.0_f32, ACCENT),
            egui::StrokeKind::Inside,
        );
    }
    let color = if !ui.is_enabled() {
        MUTED.gamma_multiply(0.45)
    } else {
        motion::mix(TEXT, ACCENT, feedback.selected)
    };
    draw(ui.painter(), painted.shrink(5.0), icon, color);
    #[cfg(test)]
    ui.ctx()
        .data_mut(|data| data.insert_temp(egui::Id::new(("icon_button_bounds", label)), rect));
    response.on_hover_text(label)
}

pub(super) fn menu_button<R>(
    ui: &mut egui::Ui,
    label: &str,
    contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<Option<R>> {
    let background = ui.painter().add(egui::Shape::Noop);
    let ctx = ui.ctx().clone();
    let mut open = false;
    let result = appearance::with_menu_surface(&ctx, || {
        egui::menu::menu_custom_button(
            ui,
            egui::Button::new("")
                .frame(false)
                .min_size(egui::vec2(30.0, 30.0)),
            |ui| {
                open = true;
                appearance::menu_contents(ui, contents)
            },
        )
    });
    let feedback = motion::feedback(ui, &result.response, open);
    ui.painter().set(
        background,
        egui::Shape::rect_filled(
            feedback.paint_rect(result.response.rect),
            7.0,
            motion::mix(
                Color32::TRANSPARENT,
                CARD,
                feedback.hover.max(feedback.selected),
            ),
        ),
    );
    if result.response.has_focus() {
        ui.painter().rect_stroke(
            result.response.rect,
            7.0,
            egui::Stroke::new(1.0_f32, ACCENT),
            egui::StrokeKind::Inside,
        );
    }
    draw(
        ui.painter(),
        result.response.rect.shrink(5.0),
        Icon::More,
        if ui.is_enabled() {
            TEXT
        } else {
            MUTED.gamma_multiply(0.45)
        },
    );
    result.response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    #[cfg(test)]
    ui.data_mut(|data| {
        data.insert_temp(
            egui::Id::new(("icon_button_bounds", label)),
            result.response.rect,
        )
    });
    egui::InnerResponse {
        response: result.response.on_hover_text(label),
        inner: result.inner,
    }
}

/// Small native vector glyphs stay sharp at any display scale and need no font.
pub(super) fn draw(painter: &egui::Painter, rect: egui::Rect, icon: Icon, color: Color32) {
    let scale = rect.width().min(rect.height()) / 24.0;
    let origin = rect.center() - egui::vec2(12.0, 12.0) * scale;
    let p = |x, y| origin + egui::vec2(x, y) * scale;
    let stroke = egui::Stroke::new(1.7_f32, color);
    let circle = |center: [f32; 2], radius: f32| {
        painter.circle_stroke(p(center[0], center[1]), radius * scale, stroke)
    };
    let line = |points: &[[f32; 2]]| {
        painter.add(egui::Shape::line(
            points.iter().map(|v| p(v[0], v[1])).collect(),
            stroke,
        ));
    };
    let frame = |x1, y1, x2, y2| {
        painter.rect_stroke(
            egui::Rect::from_min_max(p(x1, y1), p(x2, y2)),
            2.0,
            stroke,
            egui::StrokeKind::Inside,
        );
    };
    match icon {
        Icon::Eye | Icon::EyeOff => {
            line(&[
                [2.0, 12.0],
                [6.0, 7.0],
                [12.0, 5.0],
                [18.0, 7.0],
                [22.0, 12.0],
                [18.0, 17.0],
                [12.0, 19.0],
                [6.0, 17.0],
                [2.0, 12.0],
            ]);
            painter.circle_stroke(p(12.0, 12.0), 3.0 * scale, stroke);
            if icon == Icon::EyeOff {
                line(&[[3.0, 3.0], [21.0, 21.0]]);
            }
        }
        Icon::Lock | Icon::Unlock => {
            frame(5.0, 10.0, 19.0, 22.0);
            if icon == Icon::Lock {
                line(&[
                    [8.0, 10.0],
                    [8.0, 6.0],
                    [10.0, 3.0],
                    [14.0, 3.0],
                    [16.0, 6.0],
                    [16.0, 10.0],
                ]);
            } else {
                line(&[
                    [8.0, 10.0],
                    [8.0, 6.0],
                    [10.0, 3.0],
                    [14.0, 3.0],
                    [16.0, 6.0],
                ]);
            }
            painter.circle_filled(p(12.0, 15.0), 1.2 * scale, color);
            line(&[[12.0, 15.0], [12.0, 18.0]]);
        }
        Icon::ChevronsUp | Icon::ChevronsDown => {
            let flip = |y| {
                if icon == Icon::ChevronsUp {
                    24.0 - y
                } else {
                    y
                }
            };
            line(&[[6.0, flip(6.0)], [12.0, flip(12.0)], [18.0, flip(6.0)]]);
            line(&[[6.0, flip(12.0)], [12.0, flip(18.0)], [18.0, flip(12.0)]]);
        }
        Icon::More => {
            for x in [5.0, 12.0, 19.0] {
                painter.circle_filled(p(x, 12.0), 1.5 * scale, color);
            }
        }
        Icon::Move => {
            line(&[
                [4.0, 2.0],
                [4.0, 19.0],
                [8.5, 15.0],
                [12.0, 22.0],
                [15.0, 20.5],
                [11.5, 13.5],
                [18.0, 13.0],
                [4.0, 2.0],
            ]);
        }
        Icon::Lasso => {
            let points: Vec<_> = (0..=24)
                .map(|i| {
                    let angle = i as f32 / 24.0 * std::f32::consts::TAU;
                    p(12.0 + angle.cos() * 9.0, 9.0 + angle.sin() * 6.0)
                })
                .collect();
            painter.add(egui::Shape::line(points, stroke));
            line(&[
                [7.0, 14.0],
                [5.0, 19.0],
                [9.0, 21.0],
                [12.0, 18.0],
                [9.0, 16.0],
            ]);
        }
        Icon::Select => {
            for points in [
                &[[3.0, 8.0], [3.0, 3.0], [8.0, 3.0]][..],
                &[[16.0, 3.0], [21.0, 3.0], [21.0, 8.0]],
                &[[21.0, 16.0], [21.0, 21.0], [16.0, 21.0]],
                &[[8.0, 21.0], [3.0, 21.0], [3.0, 16.0]],
            ] {
                line(points);
            }
        }
        Icon::Crop => {
            line(&[[6.0, 2.0], [6.0, 18.0], [22.0, 18.0]]);
            line(&[[2.0, 6.0], [18.0, 6.0], [18.0, 22.0]]);
        }
        Icon::SmartLasso => {
            line(&[
                [5.0, 20.0],
                [18.0, 7.0],
                [16.0, 5.0],
                [3.0, 18.0],
                [5.0, 20.0],
            ]);
            line(&[[5.0, 2.0], [5.0, 7.0]]);
            line(&[[2.5, 4.5], [7.5, 4.5]]);
            line(&[[20.0, 12.0], [20.0, 18.0]]);
            line(&[[17.0, 15.0], [23.0, 15.0]]);
        }
        Icon::Eraser => {
            line(&[
                [2.0, 15.0],
                [13.0, 4.0],
                [22.0, 13.0],
                [14.0, 21.0],
                [8.0, 21.0],
                [2.0, 15.0],
            ]);
            line(&[[7.0, 10.0], [17.0, 19.0]]);
            line(&[[15.0, 23.0], [23.0, 23.0]]);
        }
        Icon::Heal => {
            line(&[
                [4.0, 16.0],
                [15.0, 5.0],
                [19.0, 9.0],
                [8.0, 20.0],
                [4.0, 20.0],
                [4.0, 16.0],
            ]);
            line(&[[18.0, 14.0], [18.0, 22.0]]);
            line(&[[14.0, 18.0], [22.0, 18.0]]);
        }
        Icon::Brush => {
            line(&[
                [8.0, 14.0],
                [18.0, 3.0],
                [21.0, 6.0],
                [11.0, 17.0],
                [8.0, 14.0],
            ]);
            line(&[
                [8.0, 15.0],
                [4.0, 16.0],
                [3.0, 22.0],
                [9.0, 21.0],
                [11.0, 18.0],
            ]);
        }
        Icon::Bubble => {
            line(&[
                [6.0, 4.0],
                [18.0, 4.0],
                [21.0, 7.0],
                [21.0, 14.0],
                [18.0, 17.0],
                [12.0, 17.0],
                [6.0, 22.0],
                [7.0, 17.0],
                [4.0, 16.0],
                [2.0, 13.0],
                [2.0, 8.0],
                [6.0, 4.0],
            ]);
        }
        Icon::Beat => {
            frame(3.0, 3.0, 21.0, 21.0);
            line(&[[8.0, 8.0], [17.0, 8.0]]);
            line(&[[8.0, 12.0], [17.0, 12.0]]);
            line(&[[8.0, 16.0], [14.0, 16.0]]);
        }
        Icon::Undo | Icon::Redo => {
            let points = [[8.0, 4.0], [3.0, 9.0], [8.0, 14.0]];
            let path = [
                [3.0, 9.0],
                [14.0, 9.0],
                [18.0, 11.0],
                [20.0, 15.0],
                [18.0, 20.0],
            ];
            let flip = |values: &[[f32; 2]]| {
                values
                    .iter()
                    .map(|v| {
                        [
                            if icon == Icon::Redo {
                                24.0 - v[0]
                            } else {
                                v[0]
                            },
                            v[1],
                        ]
                    })
                    .collect::<Vec<_>>()
            };
            line(&flip(&points));
            line(&flip(&path));
        }
        Icon::Magnet => {
            line(&[
                [4.0, 3.0],
                [4.0, 14.0],
                [6.0, 19.0],
                [12.0, 22.0],
                [18.0, 19.0],
                [20.0, 14.0],
                [20.0, 3.0],
                [15.0, 3.0],
                [15.0, 14.0],
                [12.0, 17.0],
                [9.0, 14.0],
                [9.0, 3.0],
                [4.0, 3.0],
            ]);
            line(&[[4.0, 8.0], [9.0, 8.0]]);
            line(&[[15.0, 8.0], [20.0, 8.0]]);
        }
        Icon::ChevronLeft => line(&[[15.0, 6.0], [9.0, 12.0], [15.0, 18.0]]),
        Icon::ChevronRight => line(&[[9.0, 6.0], [15.0, 12.0], [9.0, 18.0]]),
        Icon::ChevronDown => line(&[[6.0, 9.0], [12.0, 15.0], [18.0, 9.0]]),
        Icon::Lore => {
            for (x, y) in [(0.2, 0.23), (0.78, 0.43), (0.32, 0.81)] {
                let point = egui::pos2(
                    rect.left() + rect.width() * x,
                    rect.top() + rect.height() * y,
                );
                painter.circle_stroke(
                    point,
                    rect.width() * 0.12,
                    egui::Stroke::new(1.5_f32, color),
                );
            }
            let pt = |x: f32, y: f32| {
                egui::pos2(
                    rect.left() + rect.width() * x,
                    rect.top() + rect.height() * y,
                )
            };
            painter.line_segment(
                [pt(0.32, 0.27), pt(0.66, 0.39)],
                egui::Stroke::new(1.4_f32, color),
            );
            painter.line_segment(
                [pt(0.68, 0.51), pt(0.42, 0.73)],
                egui::Stroke::new(1.4_f32, color),
            );
        }
        Icon::Chapter => {
            frame(5.0, 2.0, 19.0, 22.0);
            line(&[[8.0, 7.0], [16.0, 7.0]]);
            line(&[[8.0, 11.0], [16.0, 11.0]]);
            line(&[[8.0, 15.0], [16.0, 15.0]]);
        }
        Icon::Groups => {
            circle([8.0, 7.0], 3.0);
            circle([17.0, 9.0], 2.5);
            line(&[
                [2.0, 21.0],
                [3.0, 16.0],
                [8.0, 13.0],
                [13.0, 16.0],
                [14.0, 21.0],
            ]);
            line(&[[15.0, 14.0], [21.0, 17.0], [22.0, 21.0]]);
        }
        Icon::Timeline => {
            line(&[[3.0, 12.0], [21.0, 12.0]]);
            for x in [4.0, 12.0, 20.0] {
                circle([x, 12.0], 2.2);
            }
            line(&[[4.0, 5.0], [4.0, 8.0]]);
            line(&[[12.0, 16.0], [12.0, 20.0]]);
            line(&[[20.0, 5.0], [20.0, 8.0]]);
        }
        Icon::Characters => {
            painter.circle_stroke(p(12.0, 7.0), 4.0 * scale, stroke);
            line(&[
                [3.0, 22.0],
                [4.0, 17.0],
                [8.0, 14.0],
                [16.0, 14.0],
                [20.0, 17.0],
                [21.0, 22.0],
            ]);
        }
        Icon::Background | Icon::Image => {
            if icon == Icon::Image {
                frame(2.0, 3.0, 22.0, 21.0);
            }
            line(&[
                [3.0, 19.0],
                [9.0, 10.0],
                [14.0, 16.0],
                [17.0, 13.0],
                [21.0, 19.0],
            ]);
            painter.circle_stroke(p(17.0, 7.0), 2.0 * scale, stroke);
        }
        Icon::Settings => {
            for (x, knob) in [(5.0, 8.0), (12.0, 16.0), (19.0, 10.0)] {
                line(&[[x, 3.0], [x, knob - 2.5]]);
                line(&[[x, knob + 2.5], [x, 21.0]]);
                painter.circle_stroke(p(x, knob), 2.5 * scale, stroke);
            }
        }
    }
}
