use super::*;
#[derive(Clone)]
struct Camera {
    yaw: f32,
    pitch: f32,
    roll: f32,
    zoom: f32,
    focus: f32,
    ring: Option<usize>,
}
impl Default for Camera {
    fn default() -> Self {
        Self {
            yaw: 30.0,
            pitch: 0.0,
            roll: 0.0,
            zoom: 1.0,
            focus: 0.95,
            ring: None,
        }
    }
}
pub(super) struct CameraDialog {
    character: u64,
    camera: Camera,
}
impl Camera {
    fn frame(&self) -> &'static str {
        if self.zoom < 1.35 {
            "Full body"
        } else if self.zoom < 2.2 {
            "Waist up"
        } else if self.zoom < 3.2 {
            "Portrait"
        } else {
            "Close-up"
        }
    }
    fn prompt(&self) -> String {
        let yaw = self.yaw.rem_euclid(360.0);
        let side = if yaw > 180.0 { "left" } else { "right" };
        let turn = yaw.min(360.0 - yaw);
        let view = if turn < 15.0 {
            "front view"
        } else if turn < 65.0 {
            "front three-quarter view"
        } else if turn < 115.0 {
            "side profile"
        } else if turn < 165.0 {
            "rear three-quarter view"
        } else {
            "back view"
        };
        let elevation = if self.pitch > 60.0 {
            "overhead, looking down"
        } else if self.pitch > 15.0 {
            "high angle, looking down"
        } else if self.pitch < -60.0 {
            "extreme low angle, looking up"
        } else if self.pitch < -15.0 {
            "low angle, looking up"
        } else {
            "eye level"
        };
        format!("Camera and framing: {} from the {side}, {elevation}. Orbit {:.0} degrees around the character from the front; elevation {:.0} degrees; camera roll {:.0} degrees clockwise. {} composition, centered at {:.0}% of the character’s height. Preserve the character’s proportions and design; change the camera viewpoint, not their identity. Do not include a mannequin, rig, compass, or angle guides in the generated image.",view,self.yaw,self.pitch,self.roll,self.frame(),self.focus/1.9*100.0)
    }
    fn project(&self, p: [f32; 3], rect: egui::Rect) -> (egui::Pos2, f32) {
        let (s, c) = self.yaw.to_radians().sin_cos();
        let x = p[0] * c + p[2] * s;
        let z = -p[0] * s + p[2] * c;
        let y = p[1] - self.focus;
        let (s, c) = self.pitch.to_radians().sin_cos();
        let yp = y * c - z * s;
        let zp = y * s + z * c;
        let (s, c) = self.roll.to_radians().sin_cos();
        let xp = x * c - yp * s;
        let yp = x * s + yp * c;
        let perspective = 3.8 / (3.8 - zp).max(0.5);
        let scale = rect.height() * 0.39 * self.zoom * perspective;
        (rect.center() + egui::vec2(xp, -yp) * scale, zp)
    }
}
impl Studio {
    pub(super) fn open_camera(&mut self, ctx: &egui::Context, character: u64) {
        let key = egui::Id::new(("character_camera", self.document_epoch, character));
        let camera = ctx.data(|d| d.get_temp::<Camera>(key)).unwrap_or_default();
        self.camera_dialog = Some(CameraDialog { character, camera });
    }
    pub(super) fn camera_dialog_ui(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.camera_dialog.take() else {
            return;
        };
        let Some(character) = self
            .story
            .assets
            .iter()
            .find(|a| a.id == dialog.character)
            .cloned()
        else {
            return;
        };
        let mut close = false;
        let modal = egui::Modal::new(egui::Id::new("camera_dialog")).show(ctx, |ui| {
            ui.set_width(360.0);
            ui.horizontal(|ui| {
                ui.heading("Angle");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if motion::button(ui, "Done").clicked() {
                        close = true;
                    }
                    if motion::button(ui, "Reset").clicked() {
                        dialog.camera = Camera::default();
                    }
                });
            });
            let size = egui::vec2(ui.available_width(), 300.0);
            let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
            let center = rect.center();
            let radius = 138.0;
            if response.drag_started() {
                if let Some(p) = ctx.input(|i| i.pointer.press_origin()) {
                    let distance = p.distance(center);
                    dialog.camera.ring =
                        (0..3).find(|ring| (distance - (radius - *ring as f32 * 14.0)).abs() < 7.0);
                }
            }
            if response.dragged() {
                let delta = response.drag_delta();
                if ctx.input(|i| i.modifiers.shift) {
                    dialog.camera.focus = (dialog.camera.focus
                        + delta.y / (110.0 * dialog.camera.zoom))
                        .clamp(0.0, 1.9);
                } else if let (Some(ring), Some(p)) =
                    (dialog.camera.ring, response.interact_pointer_pos())
                {
                    let a = p - center;
                    let b = p - delta - center;
                    let difference = (a.angle() - b.angle() + std::f32::consts::PI)
                        .rem_euclid(std::f32::consts::TAU)
                        - std::f32::consts::PI;
                    let degrees = difference.to_degrees();
                    match ring {
                        0 => {
                            dialog.camera.yaw =
                                (dialog.camera.yaw + degrees + 180.0).rem_euclid(360.0) - 180.0
                        }
                        1 => {
                            dialog.camera.pitch = (dialog.camera.pitch + degrees).clamp(-90.0, 90.0)
                        }
                        _ => {
                            dialog.camera.roll =
                                (dialog.camera.roll + degrees + 180.0).rem_euclid(360.0) - 180.0
                        }
                    }
                } else {
                    dialog.camera.yaw =
                        (dialog.camera.yaw + delta.x * 0.6 + 180.0).rem_euclid(360.0) - 180.0;
                    dialog.camera.pitch = (dialog.camera.pitch + delta.y * 0.6).clamp(-90.0, 90.0);
                }
            }
            if response.hovered() {
                let scroll = ctx.input_mut(|i| {
                    let scroll = i.smooth_scroll_delta.y;
                    i.smooth_scroll_delta = egui::Vec2::ZERO;
                    i.raw_scroll_delta = egui::Vec2::ZERO;
                    scroll
                });
                if scroll != 0.0 {
                    dialog.camera.zoom =
                        (dialog.camera.zoom * (scroll * 0.006).exp()).clamp(0.65, 4.2);
                    dialog.camera.focus =
                        (0.95 + (dialog.camera.zoom - 1.0) * 0.25).clamp(0.7, 1.65);
                }
            }
            response.on_hover_text(
                "Drag to orbit · Scroll to frame · Shift-drag to reframe vertically",
            );
            paint_dummy(ui.painter(), rect, &dialog.camera);
            for (index, color) in [
                ACCENT,
                Color32::from_rgb(111, 194, 172),
                Color32::from_rgb(208, 161, 103),
            ]
            .into_iter()
            .enumerate()
            {
                let radius = radius - index as f32 * 14.0;
                ui.painter().circle_stroke(
                    center,
                    radius,
                    egui::Stroke::new(1.0_f32, color.gamma_multiply(0.35)),
                );
                let angle = [dialog.camera.yaw, dialog.camera.pitch, dialog.camera.roll][index]
                    .to_radians();
                let knob = center + egui::vec2(angle.cos(), angle.sin()) * radius;
                ui.painter().circle_filled(knob, 5.0, color);
            }
            ui.horizontal(|ui| {
                ui.colored_label(ACCENT, format!("Orbit {:.0}°", dialog.camera.yaw));
                ui.colored_label(
                    Color32::from_rgb(111, 194, 172),
                    format!("Tilt {:.0}°", dialog.camera.pitch),
                );
                ui.colored_label(
                    Color32::from_rgb(208, 161, 103),
                    format!("Roll {:.0}°", dialog.camera.roll),
                );
            });
            ui.horizontal(|ui| {
                for (label, zoom, focus) in [
                    ("Full", 1.0, 0.95),
                    ("Waist", 1.7, 1.25),
                    ("Portrait", 2.7, 1.55),
                    ("Close", 3.7, 1.65),
                ] {
                    if ui
                        .add(
                            motion::Button::new(label)
                                .selected((dialog.camera.zoom - zoom).abs() < 0.15),
                        )
                        .clicked()
                    {
                        dialog.camera.zoom = zoom;
                        dialog.camera.focus = focus;
                    }
                }
            });
            ui.horizontal(|ui| {
                for (label, yaw) in [("Front", 0.0), ("¾", 45.0), ("Side", 90.0), ("Back", 180.0)]
                {
                    if motion::button(ui, label).clicked() {
                        dialog.camera.yaw = yaw;
                    }
                }
            });
            ui.collapsing("Angles", |ui| {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::DragValue::new(&mut dialog.camera.yaw)
                            .prefix("Orbit ")
                            .suffix("°")
                            .range(-180.0..=180.0),
                    );
                    ui.add(
                        egui::DragValue::new(&mut dialog.camera.pitch)
                            .prefix("Tilt ")
                            .suffix("°")
                            .range(-90.0..=90.0),
                    );
                    ui.add(
                        egui::DragValue::new(&mut dialog.camera.roll)
                            .prefix("Roll ")
                            .suffix("°")
                            .range(-180.0..=180.0),
                    );
                });
            });
            ui.add_space(8.0);
            if ui
                .add_sized(
                    [ui.available_width(), 34.0],
                    motion::Button::new(RichText::new("Copy prompt").color(BG)).fill(ACCENT),
                )
                .clicked()
            {
                let character_prompt = scroll_studio::characters::compose_prompt(
                    &self.story,
                    &character,
                    self.character_prompt_kind,
                    &self.character_scene,
                );
                ctx.copy_text(format!("{character_prompt}\n\n{}", dialog.camera.prompt()));
                self.status = "Character and camera prompt copied".into();
            }
        });
        ctx.data_mut(|d| {
            d.insert_temp(
                egui::Id::new(("character_camera", self.document_epoch, dialog.character)),
                dialog.camera.clone(),
            )
        });
        if !close && !modal.should_close() {
            self.camera_dialog = Some(dialog);
        }
    }
}
fn paint_dummy(painter: &egui::Painter, rect: egui::Rect, camera: &Camera) {
    let painter = painter.with_clip_rect(rect.shrink(20.0));
    let bones = [
        ([-0.24, 1.38, 0.0], [-0.35, 1.04, 0.0], 0.07),
        ([-0.35, 1.04, 0.0], [-0.32, 0.78, 0.06], 0.055),
        ([0.24, 1.38, 0.0], [0.35, 1.04, 0.0], 0.07),
        ([0.35, 1.04, 0.0], [0.32, 0.78, 0.06], 0.055),
        ([-0.12, 0.85, 0.0], [-0.14, 0.43, 0.02], 0.1),
        ([-0.14, 0.43, 0.02], [-0.15, 0.05, 0.0], 0.065),
        ([0.12, 0.85, 0.0], [0.14, 0.43, 0.02], 0.1),
        ([0.14, 0.43, 0.02], [0.15, 0.05, 0.0], 0.065),
        ([0.0, 0.85, 0.0], [0.0, 1.36, 0.0], 0.22),
        ([0.0, 1.38, 0.0], [0.0, 1.53, 0.0], 0.07),
    ];
    let mut projected: Vec<_> = bones
        .iter()
        .map(|(a, b, r)| {
            let (a, z1) = camera.project(*a, rect);
            let (b, z2) = camera.project(*b, rect);
            (a, b, *r, (z1 + z2) * 0.5)
        })
        .collect();
    projected.sort_by(|a, b| a.3.total_cmp(&b.3));
    for (a, b, r, depth) in projected {
        let width = r * rect.height() * 0.7 * camera.zoom;
        let shade = (137.0 + depth * 32.0).clamp(90.0, 205.0) as u8;
        let color = Color32::from_rgb(shade, shade.saturating_add(10), shade.saturating_add(19));
        painter.line_segment([a, b], egui::Stroke::new(width, color));
        painter.circle_filled(a, width * 0.5, color);
        painter.circle_filled(b, width * 0.5, color);
    }
    let (head, _) = camera.project([0.0, 1.69, 0.0], rect);
    let r = rect.height() * 0.065 * camera.zoom;
    painter.circle_filled(head, r, Color32::from_rgb(182, 194, 209));
    let (nose, depth) = camera.project([0.0, 1.68, 0.18], rect);
    let (_, back) = camera.project([0.0, 1.68, 0.0], rect);
    if depth > back {
        painter.circle_filled(nose, 3.0 * camera.zoom, Color32::from_rgb(105, 119, 139));
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn camera_prompts_and_projection_follow_angle_and_framing() {
        let mut c = Camera::default();
        assert!(c.prompt().contains("three-quarter"));
        c.yaw = 90.0;
        c.pitch = 40.0;
        c.zoom = 2.8;
        assert!(c.prompt().contains("side profile"));
        assert!(c.prompt().contains("high angle"));
        assert!(c.prompt().contains("Portrait"));
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(300.0, 300.0));
        for yaw in [-180.0, -90.0, 0.0, 90.0, 180.0] {
            c.yaw = yaw;
            assert!(c.project([0.0, 1.5, 0.0], rect).0.is_finite());
        }
    }
}
