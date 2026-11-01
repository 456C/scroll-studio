use super::*;
use egui::{pos2, vec2, Align2, FontId, Rect};

pub(super) struct Startup {
    started: Instant,
    logo: egui::TextureHandle,
}
impl Startup {
    pub(super) fn new(ctx: &egui::Context) -> Self {
        let image = image::load_from_memory(include_bytes!("../../assets/456c-author.png"))
            .expect("Bundled author mark is PNG")
            .into_rgba8();
        Self {
            started: Instant::now(),
            logo: ctx.load_texture(
                "456C author",
                egui::ColorImage::from_rgba_unmultiplied(
                    [image.width() as usize, image.height() as usize],
                    image.as_raw(),
                ),
                egui::TextureOptions::LINEAR,
            ),
        }
    }
}
fn opacity(seconds: f32, motion: settings::Motion) -> Option<f32> {
    if motion == settings::Motion::Off {
        return (seconds < 0.35).then_some(1.0);
    }
    (seconds < 0.85).then(|| (1.0 - ((seconds - 0.55) / 0.30).clamp(0.0, 1.0)).powi(2))
}
impl Studio {
    pub(super) fn startup_frame(&mut self, ctx: &egui::Context) -> Option<f32> {
        let startup = self.startup.as_ref()?;
        let skip = ctx.input(|i| {
            i.pointer.any_pressed()
                || i.key_pressed(egui::Key::Escape)
                || i.key_pressed(egui::Key::Enter)
        });
        let alpha = if skip {
            None
        } else {
            opacity(
                startup.started.elapsed().as_secs_f32(),
                self.preferences.motion,
            )
        };
        if alpha.is_none() {
            self.startup = None;
        }
        if skip || alpha.is_some() {
            // A splash-dismiss click must not also press a control underneath.
            ctx.input_mut(|i| {
                i.events.clear();
                i.pointer = egui::PointerState::default();
            });
        }
        if alpha.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }
        alpha
    }
    pub(super) fn paint_startup(&self, ctx: &egui::Context, alpha: f32) {
        let Some(startup) = &self.startup else {
            return;
        };
        let area = ctx.screen_rect();
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Tooltip,
            egui::Id::new("startup_mark"),
        ));
        painter.rect_filled(
            area,
            0.0,
            Color32::from_rgba_unmultiplied(20, 24, 28, (255.0 * alpha) as u8),
        );
        let center = area.center() - vec2(0.0, 35.0);
        for radius in [146.0, 178.0, 211.0] {
            painter.circle_stroke(
                center,
                radius,
                egui::Stroke::new(
                    1.0_f32,
                    Color32::from_rgba_unmultiplied(93, 190, 179, (14.0 * alpha) as u8),
                ),
            );
        }
        painter.image(
            startup.logo.id(),
            Rect::from_center_size(center, vec2(190.0, 190.0)),
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            Color32::WHITE.gamma_multiply(alpha),
        );
        painter.text(
            center + vec2(0.0, 132.0),
            Align2::CENTER_CENTER,
            "Scroll Studio",
            FontId::proportional(28.0),
            TEXT.gamma_multiply(alpha),
        );
        painter.text(
            center + vec2(0.0, 164.0),
            Align2::CENTER_CENTER,
            "by 456C",
            FontId::proportional(14.0),
            Color32::from_rgb(135, 196, 189).gamma_multiply(alpha),
        );
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn startup_is_brief_reduced_motion_is_static_and_animation_stops() {
        assert_eq!(opacity(0.0, settings::Motion::Lively), Some(1.0));
        assert!(opacity(0.7, settings::Motion::Lively).unwrap() < 0.5);
        assert_eq!(opacity(0.9, settings::Motion::Lively), None);
        assert_eq!(opacity(0.2, settings::Motion::Off), Some(1.0));
        assert_eq!(opacity(0.4, settings::Motion::Off), None);
    }
}
