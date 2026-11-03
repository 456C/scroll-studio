use super::*;
use scroll_studio::locations;

#[derive(Clone, Default)]
struct LocationView {
    tab: usize,
    field: usize,
    angle: String,
    reference: Option<u64>,
}

impl Studio {
    pub(super) fn location_workspace(&mut self, ctx: &egui::Context) {
        self.canvas_rect = egui::Rect::NOTHING;
        self.sanitize_location_selection();
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG).inner_margin(18.0))
            .show(ctx, |ui| {
                let index = self
                    .selected_location
                    .and_then(|id| self.story.locations.iter().position(|l| l.id == id));
                let Some(index) = index else {
                    self.location_gallery(ui, ctx);
                    return;
                };
                let mut location = self.story.locations[index].clone();
                let original = location.clone();
                let key = egui::Id::new(("location_view", self.document_epoch, location.id));
                let mut view = ctx
                    .data(|d| d.get_temp::<LocationView>(key))
                    .unwrap_or_default();
                ui.horizontal(|ui| {
                    if motion::button(ui, "‹ Locations").clicked() {
                        self.selected_location = None;
                        self.selected_asset = None;
                    }
                    ui.add(egui::Label::new(RichText::new(&location.name).size(22.0)).truncate());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if motion::button(ui, "Copy prompt").clicked() {
                            ctx.copy_text(locations::compose_prompt(
                                &self.story,
                                &location,
                                &view.angle,
                            ));
                            self.status = "Location prompt copied".into();
                        }
                        motion::menu_button(ui, "•••", |ui| {
                            self.location_context_actions(ui, Some(location.id));
                        });
                    });
                });
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    for (index, label) in ["References", "Details", "Prompt"].iter().enumerate() {
                        ui.selectable_value(&mut view.tab, index, *label);
                    }
                });
                self.affiliation_picker(ui, scroll_studio::groups::Subject::Location(location.id));
                ui.add_space(12.0);
                let body = ui.available_rect_before_wrap();
                egui::Frame::new()
                    .fill(PANEL)
                    .corner_radius(10.0)
                    .inner_margin(12.0)
                    .show(ui, |ui| {
                        ui.set_min_size(
                            (body.size() - egui::vec2(24.0, 24.0)).max(egui::Vec2::ZERO),
                        );
                        ui.set_max_height(body.height() - 24.0);
                        match view.tab {
                            0 => self.location_references(ui, ctx, &location, &mut view),
                            1 => {
                                let mut fields = [
                                    ("Description", &mut location.profile.description),
                                    ("Atmosphere", &mut location.profile.atmosphere),
                                    ("Lighting", &mut location.profile.lighting),
                                    ("Keep consistent", &mut location.profile.consistency),
                                    ("Notes", &mut location.profile.notes),
                                    ("Avoid", &mut location.profile.avoid),
                                ];
                                view.field = view.field.min(fields.len() - 1);
                                ui.horizontal_top(|ui| {
                                    ui.vertical(|ui| {
                                        ui.set_width(145.0);
                                        for (i, (label, _)) in fields.iter().enumerate() {
                                            if ui
                                                .add_sized(
                                                    [145.0, 34.0],
                                                    motion::Button::new(*label)
                                                        .selected(view.field == i),
                                                )
                                                .clicked()
                                            {
                                                view.field = i;
                                            }
                                        }
                                    });
                                    let height = ui.available_height();
                                    egui::ScrollArea::vertical()
                                        .id_salt(("location_field", location.id, view.field))
                                        .max_height(height)
                                        .show(ui, |ui| {
                                            ui.add_sized(
                                                [ui.available_width(), height.max(60.0)],
                                                egui::TextEdit::multiline(fields[view.field].1)
                                                    .hint_text(fields[view.field].0)
                                                    .char_limit(16000),
                                            );
                                        });
                                });
                            }
                            _ => {
                                ui.horizontal(|ui| {
                                    egui::ComboBox::from_id_salt("location_angle")
                                        .selected_text(if view.angle.is_empty() {
                                            "Choose angle"
                                        } else {
                                            &view.angle
                                        })
                                        .show_ui(ui, |ui| {
                                            for angle in [
                                                "Establishing shot",
                                                "Eye level",
                                                "Low angle",
                                                "High angle",
                                                "Reverse angle",
                                                "Bird’s-eye view",
                                                "Interior",
                                                "Exterior",
                                            ] {
                                                ui.selectable_value(
                                                    &mut view.angle,
                                                    angle.to_owned(),
                                                    angle,
                                                );
                                            }
                                        });
                                    ui.add(
                                        egui::TextEdit::singleline(&mut view.angle)
                                            .hint_text("Custom angle")
                                            .desired_width(250.0)
                                            .char_limit(1024),
                                    );
                                });
                                ui.add_space(8.0);
                                let height = (ui.available_height() * 0.35).clamp(60.0, 150.0);
                                ui.add_sized(
                                    [ui.available_width(), height],
                                    egui::TextEdit::multiline(&mut location.profile.prompt)
                                        .hint_text("Location instructions")
                                        .char_limit(16000),
                                );
                                ui.add_space(8.0);
                                let prompt =
                                    locations::compose_prompt(&self.story, &location, &view.angle);
                                egui::ScrollArea::vertical()
                                    .id_salt(("location_prompt", location.id))
                                    .max_height(ui.available_height())
                                    .show(ui, |ui| {
                                        ui.label(RichText::new(prompt).color(MUTED));
                                    });
                            }
                        }
                    });
                if location != original {
                    self.checkpoint();
                    self.story.locations[index] = location;
                    self.changed();
                }
                ctx.data_mut(|d| d.insert_temp(key, view));
            });
    }
    fn location_gallery(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            ui.heading("Locations");
            ui.label(RichText::new(self.story.locations.len().to_string()).color(MUTED));
            if !self.story.locations.is_empty() {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let add = primary(ui, "+ Location");
                    #[cfg(test)]
                    ui.data_mut(|data| {
                        data.insert_temp(egui::Id::new("add_location_bounds"), add.rect)
                    });
                    if add.clicked() {
                        self.begin_location_editor(None);
                    }
                });
            }
        });
        ui.add_space(16.0);
        if self.story.locations.is_empty() {
            let area = ui.available_rect_before_wrap();
            let rect = egui::Rect::from_center_size(
                area.center(),
                egui::vec2(240.0, 150.0).min(area.size()),
            );
            ui.scope_builder(
                egui::UiBuilder::new()
                    .max_rect(rect)
                    .layout(egui::Layout::top_down(egui::Align::Center)),
                |ui| {
                    let (icon, _) =
                        ui.allocate_exact_size(egui::vec2(72.0, 72.0), egui::Sense::hover());
                    icons::draw(
                        ui.painter(),
                        icon.shrink(10.0),
                        icons::Icon::Background,
                        MUTED,
                    );
                    ui.add_space(10.0);
                    let add = ui.add_sized([150.0, 36.0], motion::Button::new("+ Location"));
                    #[cfg(test)]
                    ui.data_mut(|data| {
                        data.insert_temp(egui::Id::new("add_location_bounds"), add.rect)
                    });
                    if add.clicked() {
                        self.begin_location_editor(None);
                    }
                },
            );
            return;
        }
        let cards: Vec<_> = self
            .story
            .locations
            .iter()
            .map(|l| {
                (
                    l.id,
                    l.name.clone(),
                    l.asset_ids.first().copied(),
                    l.asset_ids.len(),
                )
            })
            .collect();
        let columns = ((ui.available_width() + 14.0) / 230.0).floor().max(1.0) as usize;
        let width = (ui.available_width() - 14.0 * (columns - 1) as f32) / columns as f32;
        let rows = cards.len().div_ceil(columns);
        let height = 234.0;
        egui::ScrollArea::vertical()
            .id_salt("locations_gallery")
            .auto_shrink([false, false])
            .show_rows(ui, height + 14.0, rows, |ui, rows| {
                for row in rows {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 14.0;
                        for (id, name, cover, count) in
                            cards.iter().skip(row * columns).take(columns)
                        {
                            let (rect, response) = ui.allocate_exact_size(
                                egui::vec2(width, height),
                                egui::Sense::click(),
                            );
                            ui.painter().rect_filled(
                                rect,
                                12.0,
                                if response.hovered() { CARD } else { PANEL },
                            );
                            let image = egui::Rect::from_min_size(
                                rect.min + egui::vec2(8.0, 8.0),
                                egui::vec2(width - 16.0, 174.0),
                            );
                            if let Some(asset) = cover {
                                self.paint_character_reference(ui, ctx, *asset, image, false);
                            } else {
                                icons::draw(
                                    ui.painter(),
                                    egui::Rect::from_center_size(
                                        image.center(),
                                        egui::vec2(48.0, 48.0),
                                    ),
                                    icons::Icon::Background,
                                    MUTED,
                                );
                            }
                            ui.painter()
                                .with_clip_rect(rect.shrink(10.0).intersect(ui.clip_rect()))
                                .text(
                                    rect.min + egui::vec2(14.0, 198.0),
                                    egui::Align2::LEFT_CENTER,
                                    name,
                                    egui::FontId::proportional(15.0),
                                    TEXT,
                                );
                            ui.painter().text(
                                rect.right_bottom() - egui::vec2(12.0, 14.0),
                                egui::Align2::RIGHT_BOTTOM,
                                format!("{count}"),
                                egui::FontId::proportional(11.0),
                                MUTED,
                            );
                            if response.clicked() {
                                self.selected_location = Some(*id);
                                self.selected_asset = None;
                            }
                            self.register_context(&response);
                            appearance::context_menu(&response, |ui| {
                                self.location_context_actions(ui, Some(*id))
                            });
                            if let Some(asset) = response.dnd_release_payload::<u64>() {
                                if let Err(error) = self.move_background(*asset, Some(*id)) {
                                    self.error = Some(error.to_string());
                                }
                            }
                        }
                    });
                }
            });
    }
    fn location_references(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        location: &BackgroundLocation,
        view: &mut LocationView,
    ) {
        ui.horizontal(|ui| {
            if motion::button(ui, "+ Image").clicked() {
                self.selected_location = Some(location.id);
                self.import_picker();
            }
            if let Some(id) = view.reference.filter(|id| location.asset_ids.contains(id)) {
                if !self.assets_only && motion::button(ui, "Place").clicked() {
                    self.place_asset(id, None);
                    self.tab = LibraryTab::Chapters;
                }
                if motion::button(ui, "Pin").clicked() {
                    self.pin_reference(ctx, id, None);
                }
            }
        });
        if location.asset_ids.is_empty() {
            let area = ui.available_rect_before_wrap();
            let rect = egui::Rect::from_center_size(area.center(), egui::vec2(148.0, 38.0));
            if ui.put(rect, motion::Button::new("+ Add image")).clicked() {
                self.import_picker();
            }
            return;
        }
        let sizes: Vec<_> = location
            .asset_ids
            .iter()
            .map(|id| {
                self.story
                    .assets
                    .iter()
                    .find(|a| a.id == *id)
                    .map_or([1, 1], |a| [a.width.max(1), a.height.max(1)])
            })
            .collect();
        egui::ScrollArea::vertical()
            .id_salt(("location_references", location.id))
            .auto_shrink([false, false])
            .show_viewport(ui, |ui, viewport| {
                let tiles = super::characters_ui::masonry_rects(&sizes, ui.available_width());
                let origin = ui.cursor().min;
                ui.set_min_height(tiles.iter().map(|r| r.bottom()).fold(0.0_f32, f32::max));
                for (&id, tile) in location.asset_ids.iter().zip(tiles) {
                    if !tile.intersects(viewport) {
                        continue;
                    }
                    let rect = tile.translate(origin.to_vec2());
                    let response =
                        ui.interact(rect, ui.id().with(id), egui::Sense::click_and_drag());
                    self.paint_character_reference(ui, ctx, id, rect, false);
                    if view.reference == Some(id) {
                        ui.painter().rect_stroke(
                            rect,
                            8.0,
                            egui::Stroke::new(2.0_f32, ACCENT),
                            egui::StrokeKind::Inside,
                        );
                    }
                    if response.clicked() {
                        view.reference = Some(id);
                        self.selected_asset = Some(id);
                        self.pin_reference(ctx, id, None);
                    }
                    response.dnd_set_drag_payload(id);
                    self.register_context(&response);
                    appearance::context_menu(&response, |ui| self.asset_context_actions(ui, id));
                }
            });
    }
}
