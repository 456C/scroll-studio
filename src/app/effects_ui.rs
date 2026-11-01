use super::*;
use scroll_studio::effects::{Effect, Glint, Motion, Wind};

pub(super) struct MotionBrush {
    pub size: f32,
    pub length: f32,
    pub angle: f32,
    pub automatic: bool,
}
impl Default for MotionBrush {
    fn default() -> Self {
        Self {
            size: 48.0,
            length: 28.0,
            angle: 0.0,
            automatic: true,
        }
    }
}
impl Studio {
    pub(super) fn active_brush_size(&self) -> f32 {
        match self.tool {
            Tool::Heal => self.heal_brush_size,
            Tool::Blur => self.motion_brush.size,
            _ => self.brush_size,
        }
    }
    pub(super) fn add_glint(&mut self, p: [f32; 2]) {
        self.add_effect(
            "Glint",
            [p[0] - 40.0, p[1] - 40.0, 80.0, 80.0],
            Effect::Glint(Glint::default()),
            None,
        );
    }
    pub(super) fn add_wind(&mut self, points: &[[f32; 2]]) {
        let p = points.first().copied().unwrap_or([0.0, self.scroll_y]);
        let end = points.last().copied().unwrap_or(p);
        let size = [
            (end[0] - p[0]).abs().max(220.0),
            (end[1] - p[1]).abs().max(160.0),
        ];
        self.add_effect(
            "Wind",
            [p[0].min(end[0]), p[1].min(end[1]), size[0], size[1]],
            Effect::Wind(Wind::default()),
            None,
        );
    }
    fn add_effect(&mut self, name: &str, rect: [f32; 4], effect: Effect, before: Option<usize>) {
        self.checkpoint();
        let id = self.story.alloc_id();
        let [x, y, width, height] = rect;
        let layer = Layer {
            id,
            name: name.into(),
            visible: true,
            locked: false,
            opacity: 1.0,
            x,
            y,
            width,
            height,
            kind: LayerKind::Effect { effect },
        };
        let layers = &mut self.story.chapters[self.chapter].layers;
        layers.insert(before.unwrap_or(layers.len()), layer);
        self.selected = Some(id);
        self.selected_asset = None;
        self.changed();
    }
    pub(super) fn add_motion(&mut self, id: u64) {
        let Some((index, layer)) = self.story.chapters[self.chapter]
            .layers
            .iter()
            .enumerate()
            .find(|(_, l)| l.id == id && matches!(l.kind, LayerKind::Image { .. }))
            .map(|(i, l)| (i, l.clone()))
        else {
            return;
        };
        let pad = layer.width.max(layer.height) * 0.25;
        self.add_effect(
            "Action lines",
            [
                layer.x - pad,
                layer.y - pad,
                layer.width + pad * 2.0,
                layer.height + pad * 2.0,
            ],
            Effect::Motion(Motion {
                source_layer: id,
                ..Default::default()
            }),
            Some(index),
        );
    }
    pub(super) fn effect_editor(&mut self, ui: &mut egui::Ui, effect: &mut Effect) {
        ui.spacing_mut().slider_width = (ui.available_width() - 145.0).clamp(55.0, 100.0);
        match effect {
            Effect::Glint(g) => {
                ui.color_edit_button_srgba_unmultiplied(&mut g.color);
                ui.add(
                    egui::Slider::new(&mut g.angle, -180.0..=180.0)
                        .text("Angle")
                        .suffix("°"),
                );
                ui.add(egui::Slider::new(&mut g.glow, 0.0..=1.0).text("Glow"));
                ui.horizontal(|ui| {
                    for n in [4, 6, 8] {
                        ui.selectable_value(&mut g.rays, n, format!("{n} rays"));
                    }
                });
            }
            Effect::Wind(w) => {
                ui.color_edit_button_srgba_unmultiplied(&mut w.color);
                ui.add(
                    egui::Slider::new(&mut w.angle, -180.0..=180.0)
                        .text("Direction")
                        .suffix("°"),
                );
                ui.add(egui::Slider::new(&mut w.curve, -1.0..=1.0).text("Curve"));
                ui.add(egui::Slider::new(&mut w.length, 0.05..=1.0).text("Length"));
                ui.add(egui::Slider::new(&mut w.density, 4..=128).text("Density"));
                ui.collapsing("Line", |ui| {
                    ui.add(egui::Slider::new(&mut w.thickness, 0.1..=12.0).text("Width"));
                });
            }
            Effect::Motion(m) => {
                ui.color_edit_button_srgba_unmultiplied(&mut m.color);
                ui.add(
                    egui::Slider::new(&mut m.angle, -180.0..=180.0)
                        .text("Direction")
                        .suffix("°"),
                );
                ui.add(egui::Slider::new(&mut m.length, 0.05..=1.0).text("Reach"));
                ui.add(egui::Slider::new(&mut m.density, 4..=128).text("Lines"));
                ui.collapsing("Smear", |ui| {
                    ui.add(egui::Slider::new(&mut m.smear_threshold, 0..=255).text("Threshold"));
                    if ui
                        .add_enabled(self.job.is_none(), motion::Button::new("Generate smear"))
                        .clicked()
                    {
                        self.generate_smear(ui.ctx(), m);
                    }
                });
            }
        }
    }
    pub(super) fn start_motion_blur(&mut self, ctx: &egui::Context, points: Vec<[f32; 2]>) {
        let angle = if self.motion_brush.automatic && points.len() > 1 {
            let a = points[0];
            let b = points[points.len() - 1];
            (b[1] - a[1]).atan2(b[0] - a[0]).to_degrees()
        } else {
            self.motion_brush.angle
        };
        self.start_healing(
            ctx,
            super::healing_ui::HealInput::MotionStroke(
                points,
                self.motion_brush.size,
                self.motion_brush.length,
                angle,
            ),
        );
    }
}
