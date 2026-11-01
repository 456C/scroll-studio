//! Temporary lettering proof. Reading scale never depends on the sample's length.
use super::*;

pub(super) struct BubblePreview {
    pub text: String,
    phone: bool,
    chapter_id: Option<u64>,
}

impl Default for BubblePreview {
    fn default() -> Self {
        Self {
            text: "I thought you might be here.".into(),
            phone: true,
            chapter_id: None,
        }
    }
}

impl BubblePreview {
    pub(super) fn mode_controls(&mut self, ui: &mut egui::Ui) -> bool {
        let mut clicked = false;
        for (phone, label) in [(true, "Phone"), (false, "Fit")] {
            let response = ui.add(motion::Button::new(label).selected(self.phone == phone));
            #[cfg(test)]
            ui.data_mut(|data| {
                data.insert_temp(egui::Id::new(("bubble_preview_mode", phone)), response.rect)
            });
            if response
                .on_hover_text(if phone {
                    "Preview at 360 points wide using the chapter resolution"
                } else {
                    "Fit the entire bubble in the preview"
                })
                .clicked()
            {
                self.phone = phone;
                clicked = true;
            }
        }
        clicked
    }

    fn chapter<'a>(&self, chapters: &'a [Chapter], active: usize) -> Option<&'a Chapter> {
        self.chapter_id
            .and_then(|id| chapters.iter().find(|chapter| chapter.id == id))
            .or_else(|| chapters.get(active))
            .or_else(|| chapters.first())
    }

    pub(super) fn chapter_control(
        &mut self,
        ui: &mut egui::Ui,
        chapters: &[Chapter],
        active: usize,
    ) {
        if !self.phone || chapters.len() < 2 {
            return;
        }
        let Some(chapter) = self.chapter(chapters, active) else {
            return;
        };
        let mut selected = chapter.id;
        let width = chapter.width;
        let name = chapter.name.clone();
        let response = egui::ComboBox::from_id_salt("bubble_preview_chapter")
            .width(ui.available_width().min(360.0))
            .truncate()
            .selected_text(format!("{name} · {width} px"))
            .show_ui(ui, |ui| {
                ui.set_max_width(360.0);
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                for chapter in chapters {
                    let response = ui.selectable_value(
                        &mut selected,
                        chapter.id,
                        format!("{} · {} px", chapter.name, chapter.width),
                    );
                    #[cfg(test)]
                    ui.data_mut(|data| {
                        data.insert_temp(
                            egui::Id::new(("bubble_preview_chapter_choice", chapter.id)),
                            response.rect,
                        )
                    });
                    response.on_hover_text(&chapter.name);
                }
            })
            .response;
        #[cfg(test)]
        ui.data_mut(|data| {
            data.insert_temp(
                egui::Id::new("bubble_preview_chapter_control"),
                response.rect,
            )
        });
        response.on_hover_text("Preview resolution only; the active chapter stays selected");
        if selected != chapter.id {
            self.chapter_id = Some(selected);
        }
        ui.add_space(6.0);
    }
}

fn sample_layer(preset: &BubblePreset, text: &str) -> anyhow::Result<Layer> {
    let mut layer = Layer {
        id: 0,
        name: "Preset preview".into(),
        visible: true,
        locked: false,
        opacity: preset.opacity,
        x: 0.0,
        y: 0.0,
        width: 440.0,
        height: 160.0,
        kind: LayerKind::Text {
            text: text.into(),
            color: preset.color,
            size: preset.size,
            bubble: preset.bubble,
            style: preset.style.clone(),
        },
    };
    scroll_studio::bubbles::initialize_layer(&mut layer, preset)?;
    Ok(layer)
}

struct PhoneGeometry {
    #[cfg(test)]
    scale: f32,
    width: f32,
    height: f32,
    // Bubble bounds relative to the reading strip, including its pointer.
    bubble: egui::Rect,
    outside: bool,
}

fn phone_geometry(layer: &Layer, chapter_width: u32, available: egui::Vec2) -> PhoneGeometry {
    let width = 360.0_f32.min((available.x - 16.0).max(40.0));
    let scale = width / chapter_width.max(1) as f32;
    let [x, _, w, h] = compositor::text_bounds(layer);
    // Center the body, not its pointer-inclusive bounds. A long pointer must
    // remain visibly off-canvas instead of disguising a too-wide preset.
    let left = (chapter_width as f32 - layer.width) * 0.5 + x - layer.x;
    let bubble =
        egui::Rect::from_min_size(egui::pos2(left * scale, 24.0), egui::vec2(w, h) * scale);
    PhoneGeometry {
        #[cfg(test)]
        scale,
        width,
        height: available.y.max(bubble.bottom() + 24.0),
        bubble,
        outside: left < 0.0 || left + w > chapter_width as f32,
    }
}

impl Studio {
    pub(super) fn paint_preset_preview(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        preset: &BubblePreset,
        preview_height: f32,
    ) {
        let layer = match sample_layer(preset, &self.bubble_preview.text) {
            Ok(layer) => layer,
            Err(error) => {
                ui.label(format!("Preview unavailable: {error}"));
                return;
            }
        };
        let (area, response) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), preview_height),
            egui::Sense::click(),
        );
        self.register_context(&response);
        ui.painter().rect_filled(area, 6.0, PANEL);
        let viewport = area.shrink(12.0);
        if self.bubble_preview.phone {
            let chapter = self
                .bubble_preview
                .chapter(&self.story.chapters, self.chapter);
            let chapter_id = chapter.map_or(0, |chapter| chapter.id);
            let width = chapter.map_or(1600, |chapter| chapter.width);
            let geometry = phone_geometry(&layer, width, viewport.size());
            let texture = self.previews.text(
                ctx,
                &layer,
                geometry.width / width.max(1) as f32 * ctx.pixels_per_point(),
            );
            let column = egui::Rect::from_min_size(
                egui::pos2(
                    viewport.center().x - (geometry.width + 16.0) * 0.5,
                    viewport.top(),
                ),
                egui::vec2(geometry.width + 16.0, viewport.height()),
            );
            // The stage was already allocated above. A detached child avoids
            // moving the sample editor when the reading strip ends earlier.
            let mut preview_ui = ui.new_child(egui::UiBuilder::new().max_rect(column));
            egui::ScrollArea::vertical()
                .id_salt((
                    "bubble_phone_preview",
                    self.document_epoch,
                    preset.id,
                    chapter_id,
                ))
                .auto_shrink([false, false])
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
                .show(&mut preview_ui, |ui| {
                    let (paper, _) = ui.allocate_exact_size(
                        egui::vec2(geometry.width, geometry.height),
                        egui::Sense::hover(),
                    );
                    let painter = ui
                        .painter()
                        .with_clip_rect(ui.clip_rect().intersect(paper).intersect(viewport));
                    painter.rect_filled(paper, 0.0, Color32::WHITE);
                    let bubble = geometry.bubble.translate(paper.min.to_vec2());
                    paint_sample(&painter, texture.as_ref(), bubble);
                    #[cfg(test)]
                    ctx.data_mut(|data| {
                        data.insert_temp(
                            egui::Id::new("bubble_preview_geometry"),
                            (area, paper, bubble, painter.clip_rect(), geometry.scale),
                        )
                    });
                });
            if geometry.outside {
                ui.painter().text(
                    area.right_top() + egui::vec2(-14.0, 12.0),
                    egui::Align2::RIGHT_TOP,
                    "Outside canvas",
                    egui::FontId::proportional(11.0),
                    Color32::from_rgb(243, 183, 108),
                );
            }
        } else {
            let [_, _, width, height] = compositor::text_bounds(&layer);
            let inset = viewport.shrink(12.0);
            let scale = (inset.width() / width)
                .min(inset.height() / height)
                .min(1.0);
            let texture = self
                .previews
                .text(ctx, &layer, scale * ctx.pixels_per_point());
            let bubble =
                egui::Rect::from_center_size(viewport.center(), egui::vec2(width, height) * scale);
            let painter = ui
                .painter()
                .with_clip_rect(viewport.intersect(ui.clip_rect()));
            painter.rect_filled(viewport, 1.0, Color32::WHITE);
            paint_sample(&painter, texture.as_ref(), bubble);
            #[cfg(test)]
            ctx.data_mut(|data| {
                data.insert_temp(
                    egui::Id::new("bubble_preview_geometry"),
                    (area, viewport, bubble, painter.clip_rect(), scale),
                )
            });
        }
        appearance::context_menu(&response, |ui| {
            if self.bubble_preview.mode_controls(ui) {
                ui.close_menu();
            }
            ui.separator();
            self.preset_context_actions(ui, preset.id);
        });
    }
}

fn paint_sample(
    painter: &egui::Painter,
    texture: Option<&super::previews::TextTexture>,
    rect: egui::Rect,
) {
    if let Some(texture) = texture {
        texture.paint(painter, rect);
    } else {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "…",
            egui::FontId::proportional(14.0),
            MUTED,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    type Geometry = (egui::Rect, egui::Rect, egui::Rect, egui::Rect, f32);

    fn fixture() -> (egui::Context, Studio) {
        let ctx = egui::Context::default();
        let mut studio = Studio::from_context(&ctx);
        studio.story = Story::new("Lettering proof");
        studio.tab = LibraryTab::Bubbles;
        studio.selected_bubble_preset = Some(studio.story.bubble_presets[0].id);
        studio.welcome = false;
        studio.dirty = false;
        studio.preferences.motion = settings::Motion::Off;
        studio.apply_preferences(&ctx);
        (ctx, studio)
    }

    fn frame(
        ctx: &egui::Context,
        studio: &mut Studio,
        size: egui::Vec2,
        density: f32,
        events: Vec<egui::Event>,
    ) {
        let mut frame = eframe::Frame::_new_kittest();
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            events,
            ..Default::default()
        };
        input
            .viewports
            .entry(egui::ViewportId::ROOT)
            .or_default()
            .native_pixels_per_point = Some(density);
        let _ = ctx.run(input, |ctx| eframe::App::update(studio, ctx, &mut frame));
    }

    fn bounds(ctx: &egui::Context, id: impl std::hash::Hash) -> egui::Rect {
        ctx.data(|data| data.get_temp(egui::Id::new(id))).unwrap()
    }

    fn geometry(ctx: &egui::Context) -> Geometry {
        ctx.data(|data| data.get_temp(egui::Id::new("bubble_preview_geometry")))
            .unwrap()
    }

    fn click(point: egui::Pos2) -> Vec<egui::Event> {
        vec![
            egui::Event::PointerMoved(point),
            egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            },
        ]
    }

    #[test]
    fn phone_scale_stays_fixed_when_text_grows_and_exposes_wide_bubbles() {
        for width in [800, 940, 1600] {
            let mut preset = BubblePreset::for_canvas(1, LetteringTheme::Mature, width);
            let short = sample_layer(&preset, "Wait.").unwrap();
            let long = sample_layer(&preset, &"I'll wait for you here.\n".repeat(100)).unwrap();
            let available = egui::vec2(500.0, 400.0);
            let a = phone_geometry(&short, width, available);
            let b = phone_geometry(&long, width, available);
            assert_eq!(a.width, 360.0);
            assert_eq!(a.scale, b.scale);
            assert!(b.height > 400.0 && b.bubble.height() > a.bubble.height());
            assert!(!a.outside && !b.outside);
            preset.size *= 2.0;
            preset.style.max_width *= 4.0;
            let wide =
                sample_layer(&preset, &"A wide bubble with a lot to say. ".repeat(30)).unwrap();
            let c = phone_geometry(&wide, width, available);
            assert_eq!(
                c.scale, a.scale,
                "A large bubble must not become deceptively small"
            );
            assert!(c.outside && c.bubble.width() > c.width);
        }
    }

    #[test]
    fn modes_and_sample_editing_fit_supported_sizes_without_editing_the_story() {
        for (size, density) in [
            (egui::vec2(1100.0, 720.0), 1.0),
            (egui::vec2(1280.0, 760.0), 1.0),
            (egui::vec2(1707.0, 960.0), 1.5),
        ] {
            let (ctx, mut studio) = fixture();
            let before = studio.story.clone();
            for _ in 0..3 {
                frame(&ctx, &mut studio, size, density, vec![]);
            }
            let (area, paper, _, clip, scale) = geometry(&ctx);
            assert!(egui::Rect::from_min_size(egui::Pos2::ZERO, size).contains_rect(area));
            assert!(area.contains_rect(clip));
            assert_eq!(paper.width(), 360.0);
            assert_eq!(scale, 360.0 / 1600.0);
            let fit = bounds(&ctx, ("bubble_preview_mode", false));
            let original_editor = bounds(&ctx, "bubble_preview_text");
            frame(&ctx, &mut studio, size, density, click(fit.center()));
            frame(&ctx, &mut studio, size, density, vec![]);
            assert!(!studio.bubble_preview.phone);
            let (_, paper, bubble, _, _) = geometry(&ctx);
            assert!(paper.contains_rect(bubble));
            assert_eq!(
                bounds(&ctx, "bubble_preview_text"),
                original_editor,
                "Mode switches must not move the sample editor"
            );
            let phone = bounds(&ctx, ("bubble_preview_mode", true));
            frame(&ctx, &mut studio, size, density, click(phone.center()));
            assert!(studio.bubble_preview.phone);
            let editor = bounds(&ctx, "bubble_preview_text");
            assert!(egui::Rect::from_min_size(egui::Pos2::ZERO, size).contains_rect(editor));
            frame(&ctx, &mut studio, size, density, click(editor.center()));
            frame(
                &ctx,
                &mut studio,
                size,
                density,
                vec![egui::Event::Text(" Proof only.".into())],
            );
            assert!(studio.bubble_preview.text.contains("Proof only."));
            assert_eq!(studio.story, before);
            assert!(!studio.dirty && studio.undo.is_empty());
        }
    }

    #[test]
    fn preview_chapter_changes_scale_without_switching_or_mutating_the_active_chapter() {
        let (ctx, mut studio) = fixture();
        let second = Chapter::new(
            studio.story.alloc_id(),
            "A long chapter name ".repeat(50),
            800,
            6400,
        );
        let second_id = second.id;
        studio.story.chapters.push(second);
        let before = studio.story.clone();
        let size = egui::vec2(1100.0, 720.0);
        for _ in 0..3 {
            frame(&ctx, &mut studio, size, 1.0, vec![]);
        }
        let picker = bounds(&ctx, "bubble_preview_chapter_control");
        frame(&ctx, &mut studio, size, 1.0, click(picker.center()));
        frame(&ctx, &mut studio, size, 1.0, vec![]);
        let choice = bounds(&ctx, ("bubble_preview_chapter_choice", second_id));
        assert!(choice.width() <= 380.0);
        frame(&ctx, &mut studio, size, 1.0, click(choice.center()));
        frame(&ctx, &mut studio, size, 1.0, vec![]);
        assert_eq!(studio.bubble_preview.chapter_id, Some(second_id));
        assert_eq!(geometry(&ctx).4, 360.0 / 800.0);
        assert_eq!(studio.chapter, 0);
        assert_eq!(studio.story, before);
        assert!(!studio.dirty && studio.undo.is_empty());
        studio.story.chapters.pop();
        frame(&ctx, &mut studio, size, 1.0, vec![]);
        assert_eq!(geometry(&ctx).4, 360.0 / 1600.0);
        studio.reset_view();
        assert!(studio.bubble_preview.chapter_id.is_none() && studio.bubble_preview.phone);
        assert_eq!(studio.bubble_preview.text, BubblePreview::default().text);
    }

    #[test]
    fn long_phone_proof_scrolls_inside_the_stage_and_fit_reveals_the_whole_shape() {
        let (ctx, mut studio) = fixture();
        studio.bubble_preview.text = "Still waiting for the last train.\n".repeat(80);
        let before = studio.story.clone();
        let size = egui::vec2(1100.0, 720.0);
        for _ in 0..3 {
            frame(&ctx, &mut studio, size, 1.0, vec![]);
        }
        let (area, paper, _, clip, scale) = geometry(&ctx);
        assert!(paper.height() > area.height());
        assert!(egui::Rect::from_min_size(egui::Pos2::ZERO, size)
            .contains_rect(bounds(&ctx, "bubble_preview_text")));
        frame(
            &ctx,
            &mut studio,
            size,
            1.0,
            vec![
                egui::Event::PointerMoved(clip.center()),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, -200.0),
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        for _ in 0..3 {
            frame(&ctx, &mut studio, size, 1.0, vec![]);
        }
        let (_, scrolled, _, after_clip, after_scale) = geometry(&ctx);
        assert!(scrolled.top() < paper.top() - 20.0);
        assert_eq!(after_clip, clip);
        assert_eq!(after_scale, scale);
        assert_eq!(studio.scroll_y, 0.0);
        let fit = bounds(&ctx, ("bubble_preview_mode", false));
        frame(&ctx, &mut studio, size, 1.0, click(fit.center()));
        frame(&ctx, &mut studio, size, 1.0, vec![]);
        let (_, paper, bubble, _, _) = geometry(&ctx);
        assert!(paper.expand(0.01).contains_rect(bubble));
        assert_eq!(studio.story, before);
        assert!(!studio.dirty && studio.undo.is_empty());
    }
}
