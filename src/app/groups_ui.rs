use super::*;
use egui::{vec2, Align, Layout, Rect, Sense};
use scroll_studio::groups::{self, Subject};

impl Studio {
    pub(super) fn new_group(&mut self) {
        let mut next = self.story.clone();
        match groups::add(&mut next) {
            Ok(id) => {
                self.checkpoint();
                self.story = next;
                self.selected_group = Some(id);
                self.tab = LibraryTab::Groups;
                self.search.clear();
                self.changed();
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }
    pub(super) fn affiliation_picker(&mut self, ui: &mut egui::Ui, subject: Subject) {
        let mut changes = Vec::new();
        ui.horizontal(|ui| {
            motion::menu_button(ui, "Groups", |ui| {
                ui.set_min_width(180.0);
                egui::ScrollArea::vertical()
                    .max_height(240.0)
                    .show(ui, |ui| {
                        if self.story.groups.is_empty() {
                            ui.weak("No groups yet");
                        }
                        for group in &self.story.groups {
                            let mut enabled = groups::affiliated(group, subject);
                            if ui.checkbox(&mut enabled, &group.name).changed() {
                                changes.push((group.id, enabled));
                            }
                        }
                    });
                if motion::button(ui, "+ Group").clicked() {
                    self.new_group();
                    ui.close_menu();
                }
            });
            let count = self
                .story
                .groups
                .iter()
                .filter(|g| groups::affiliated(g, subject))
                .count();
            for group in self
                .story
                .groups
                .iter()
                .filter(|g| groups::affiliated(g, subject))
                .take(2)
            {
                if ui
                    .add_sized(
                        [140.0, 28.0],
                        egui::Button::new(RichText::new(&group.name).color(Color32::from_rgb(
                            group.color[0],
                            group.color[1],
                            group.color[2],
                        )))
                        .truncate(),
                    )
                    .clicked()
                {
                    self.selected_group = Some(group.id);
                    self.tab = LibraryTab::Groups;
                }
            }
            if count > 2 {
                ui.weak(format!("+{}", count - 2));
            }
        });
        if !changes.is_empty() {
            self.checkpoint();
            for (id, enabled) in changes {
                if let Some(group) = self.story.groups.iter_mut().find(|g| g.id == id) {
                    groups::set_affiliation(group, subject, enabled);
                }
            }
            self.changed();
        }
    }
    pub(super) fn import_group_reference(&mut self, id: u64, path: &std::path::Path) {
        let result = (|| -> anyhow::Result<Vec<u8>> {
            anyhow::ensure!(
                std::fs::metadata(path)?.len() <= 64 * 1024 * 1024,
                "Reference exceeds 64 MB"
            );
            Ok(std::fs::read(path)?)
        })();
        match result {
            Ok(bytes) => self.import_group_reference_bytes(
                id,
                bytes,
                path.file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
            ),
            Err(e) => self.error = Some(e.to_string()),
        }
    }
    pub(super) fn import_group_reference_bytes(&mut self, id: u64, bytes: Vec<u8>, name: String) {
        let result = (|| -> anyhow::Result<_> {
            let index = self
                .story
                .groups
                .iter()
                .position(|g| g.id == id)
                .ok_or_else(|| anyhow::anyhow!("Choose a group"))?;
            anyhow::ensure!(
                self.story.groups[index].reference_ids.len() < groups::MAX_REFERENCES,
                "Each group supports 64 references"
            );
            self.check_import_budget(bytes.len())?;
            let image = compositor::decode_image(&bytes)?;
            Ok((index, image.width(), image.height()))
        })();
        match result {
            Ok((index, width, height)) => {
                self.checkpoint();
                let asset_id = self.story.alloc_id();
                self.story.assets.push(Asset {
                    id: asset_id,
                    name,
                    kind: AssetKind::Image,
                    width,
                    height,
                    notes: String::new(),
                    prompt: String::new(),
                    character: Default::default(),
                });
                self.assets.insert(asset_id, Arc::new(bytes));
                self.story.groups[index].reference_ids.push(asset_id);
                self.changed();
                self.status = "Group reference added".into();
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }
    pub(super) fn groups_workspace(&mut self, ctx: &egui::Context) {
        self.canvas_rect = Rect::NOTHING;
        if self
            .selected_group
            .is_some_and(|id| !self.story.groups.iter().any(|g| g.id == id))
        {
            self.selected_group = None;
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG).inner_margin(22.0))
            .show(ctx, |ui| {
                let selected = self
                    .selected_group
                    .and_then(|id| self.story.groups.iter().position(|g| g.id == id));
                let Some(index) = selected else {
                    self.group_gallery(ui);
                    return;
                };
                let original = self.story.groups[index].clone();
                let mut group = original.clone();
                let mut remove = false;
                ui.horizontal(|ui| {
                    if motion::button(ui, "‹ Groups").clicked() {
                        self.selected_group = None;
                    }
                    ui.color_edit_button_srgb(&mut group.color);
                    ui.add(
                        egui::TextEdit::singleline(&mut group.name)
                            .font(egui::FontId::proportional(22.0))
                            .desired_width((ui.available_width() - 180.0).clamp(150.0, 430.0))
                            .char_limit(4096),
                    );
                    motion::menu_button(ui, "•••", |ui| {
                        if motion::button(ui, "Copy prompt").clicked() {
                            ctx.copy_text(scroll_studio::characters::with_story_style(
                                &self.story.visual_style,
                                &format!("{}\n{}", group.name, group.description),
                            ));
                            ui.close_menu();
                        }
                        if motion::button(ui, "Delete group").clicked() {
                            remove = true;
                            ui.close_menu();
                        }
                    });
                });
                ui.add_space(14.0);
                let key = egui::Id::new(("group_tab", self.document_epoch, group.id));
                let mut tab = ctx.data(|d| d.get_temp::<usize>(key)).unwrap_or(0);
                ui.horizontal(|ui| {
                    for (i, label) in ["References", "Members", "Details"].iter().enumerate() {
                        ui.selectable_value(&mut tab, i, *label);
                    }
                });
                ctx.data_mut(|d| d.insert_temp(key, tab));
                ui.add_space(14.0);
                match tab {
                    0 => self.group_references(ui, ctx, &mut group),
                    1 => self.group_members(ui, &mut group),
                    _ => {
                        ui.add_sized(
                            ui.available_size(),
                            egui::TextEdit::multiline(&mut group.description)
                                .hint_text("Culture, traits, visual identity…")
                                .char_limit(20_000),
                        );
                    }
                }
                if remove {
                    self.checkpoint();
                    self.story.groups.remove(index);
                    self.unlink_lore_target(group.id);
                    self.selected_group = None;
                    self.changed();
                } else if group != self.story.groups[index] {
                    if group.name.trim().is_empty() {
                        group.name = original.name;
                    }
                    self.checkpoint();
                    self.story.groups[index] = group;
                    self.changed();
                }
            });
    }
    fn group_gallery(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Groups");
            if !self.story.groups.is_empty() {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if primary(ui, "+ Group").clicked() {
                        self.new_group();
                    }
                });
            }
        });
        if self.story.groups.is_empty() {
            let rect = Rect::from_center_size(
                ui.available_rect_before_wrap().center(),
                vec2(220.0, 140.0),
            );
            ui.scope_builder(
                egui::UiBuilder::new()
                    .max_rect(rect)
                    .layout(Layout::top_down(Align::Center)),
                |ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(64.0, 64.0), Sense::hover());
                    icons::draw(ui.painter(), r.shrink(8.0), icons::Icon::Groups, MUTED);
                    let add = ui.add_sized([145.0, 36.0], motion::Button::new("+ Group"));
                    #[cfg(test)]
                    ui.data_mut(|d| d.insert_temp(egui::Id::new("empty_group_action"), add.rect));
                    if add.clicked() {
                        self.new_group();
                    }
                },
            );
            return;
        }
        ui.add(
            egui::TextEdit::singleline(&mut self.search)
                .hint_text("Search groups")
                .desired_width(240.0),
        );
        ui.add_space(14.0);
        let list: Vec<_> = self
            .story
            .groups
            .iter()
            .filter(|g| g.name.to_lowercase().contains(&self.search.to_lowercase()))
            .cloned()
            .collect();
        let columns = ((ui.available_width() + 14.0) / 238.0).floor().max(1.0) as usize;
        let width = (ui.available_width() - 14.0 * (columns - 1) as f32) / columns as f32;
        egui::ScrollArea::vertical().show_rows(
            ui,
            222.0,
            list.len().div_ceil(columns),
            |ui, rows| {
                for row in rows {
                    ui.horizontal(|ui| {
                        for group in list.iter().skip(row * columns).take(columns) {
                            let (rect, response) =
                                ui.allocate_exact_size(vec2(width, 208.0), Sense::click());
                            ui.painter().rect_filled(rect, 10.0, PANEL);
                            let art = Rect::from_min_max(
                                rect.min + vec2(8.0, 8.0),
                                rect.max - vec2(8.0, 64.0),
                            );
                            if let Some(id) = group.reference_ids.first() {
                                self.paint_character_reference(ui, ui.ctx(), *id, art, true);
                            } else {
                                icons::draw(
                                    ui.painter(),
                                    Rect::from_center_size(art.center(), vec2(54.0, 54.0)),
                                    icons::Icon::Groups,
                                    Color32::from_rgb(
                                        group.color[0],
                                        group.color[1],
                                        group.color[2],
                                    ),
                                );
                            }
                            let painter = ui.painter().with_clip_rect(rect.shrink(10.0));
                            painter.text(
                                rect.left_bottom() + vec2(14.0, -45.0),
                                egui::Align2::LEFT_CENTER,
                                &group.name,
                                egui::FontId::proportional(18.0),
                                TEXT,
                            );
                            painter.text(
                                rect.left_bottom() + vec2(14.0, -20.0),
                                egui::Align2::LEFT_CENTER,
                                format!(
                                    "{} character{} · {} location{}",
                                    group.character_ids.len(),
                                    if group.character_ids.len() == 1 {
                                        ""
                                    } else {
                                        "s"
                                    },
                                    group.location_ids.len(),
                                    if group.location_ids.len() == 1 {
                                        ""
                                    } else {
                                        "s"
                                    }
                                ),
                                egui::FontId::proportional(12.0),
                                MUTED,
                            );
                            if response.clicked() {
                                self.selected_group = Some(group.id);
                            }
                        }
                    });
                }
            },
        );
    }
    fn group_members(&mut self, ui: &mut egui::Ui, group: &mut WorldGroup) {
        ui.columns(2, |columns| {
            columns[0].heading("Characters");
            egui::ScrollArea::vertical()
                .id_salt("group_characters")
                .show(&mut columns[0], |ui| {
                    for asset in self
                        .story
                        .assets
                        .iter()
                        .filter(|a| a.kind == AssetKind::Character)
                    {
                        let subject = Subject::Character(asset.id);
                        let mut enabled = groups::affiliated(group, subject);
                        ui.horizontal(|ui| {
                            if ui.checkbox(&mut enabled, &asset.name).changed() {
                                groups::set_affiliation(group, subject, enabled);
                            }
                            if ui
                                .small_button("↗")
                                .on_hover_text("Open character")
                                .clicked()
                            {
                                self.selected_asset = Some(asset.id);
                                self.tab = LibraryTab::Characters;
                            }
                        });
                    }
                });
            columns[1].heading("Locations");
            egui::ScrollArea::vertical()
                .id_salt("group_locations")
                .show(&mut columns[1], |ui| {
                    for location in &self.story.locations {
                        let subject = Subject::Location(location.id);
                        let mut enabled = groups::affiliated(group, subject);
                        ui.horizontal(|ui| {
                            if ui.checkbox(&mut enabled, &location.name).changed() {
                                groups::set_affiliation(group, subject, enabled);
                            }
                            if ui
                                .small_button("↗")
                                .on_hover_text("Open location")
                                .clicked()
                            {
                                self.selected_location = Some(location.id);
                                self.tab = LibraryTab::Backgrounds;
                            }
                        });
                    }
                });
        });
    }
    fn group_references(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, group: &mut WorldGroup) {
        let mut import = false;
        motion::menu_button(ui, "+ Reference", |ui| {
            ui.add_enabled_ui(group.reference_ids.len() < groups::MAX_REFERENCES, |ui| {
                if motion::button(ui, "Import images…").clicked() {
                    import = true;
                    ui.close_menu();
                }
                ui.separator();
                egui::ScrollArea::vertical()
                    .max_height(240.0)
                    .show(ui, |ui| {
                        for asset in self
                            .story
                            .assets
                            .iter()
                            .filter(|a| {
                                self.assets.contains_key(&a.id)
                                    && !group.reference_ids.contains(&a.id)
                            })
                            .collect::<Vec<_>>()
                        {
                            if motion::button(ui, &asset.name).clicked() {
                                group.reference_ids.push(asset.id);
                                ui.close_menu();
                            }
                        }
                    });
            });
        });
        if import {
            if let Some(paths) = rfd::FileDialog::new()
                .set_title("Add group references")
                .add_filter("Images", &["png", "jpg", "jpeg", "webp"])
                .pick_files()
            {
                // Commit imported assets through the ordinary image budget and undo path.
                for path in paths {
                    self.import_group_reference(group.id, &path);
                }
                if let Some(saved) = self.story.groups.iter().find(|g| g.id == group.id) {
                    group.reference_ids = saved.reference_ids.clone();
                }
            }
        }
        ui.add_space(12.0);
        let sizes: Vec<_> = group
            .reference_ids
            .iter()
            .map(|id| {
                self.story
                    .assets
                    .iter()
                    .find(|a| a.id == *id)
                    .map_or([1, 1], |a| [a.width, a.height])
            })
            .collect();
        let mut remove = None;
        let area = egui::ScrollArea::vertical()
            .id_salt(("group_refs", group.id))
            .auto_shrink([false, false])
            .show_viewport(ui, |ui, viewport| {
                let tiles = characters_ui::masonry_rects(&sizes, ui.available_width());
                let origin = ui.cursor().min;
                ui.set_min_height(tiles.iter().map(|r| r.bottom()).fold(180.0, f32::max));
                if tiles.is_empty() {
                    ui.weak("Drop reference images");
                }
                for (&id, tile) in group.reference_ids.iter().zip(tiles) {
                    if !tile.intersects(viewport) {
                        continue;
                    }
                    let rect = tile.translate(origin.to_vec2());
                    self.paint_character_reference(ui, ctx, id, rect, false);
                    let response = ui.interact(rect, ui.id().with(id), Sense::click());
                    if response.clicked() {
                        self.pin_reference(ctx, id, None);
                    }
                    appearance::context_menu(&response, |ui| {
                        if motion::button(ui, "Remove reference").clicked() {
                            remove = Some(id);
                            ui.close_menu();
                        }
                    });
                }
            });
        if let Some(id) = ui
            .interact(
                area.inner_rect,
                ui.id().with("group_ref_drop"),
                Sense::hover(),
            )
            .dnd_release_payload::<u64>()
        {
            if group.reference_ids.len() < groups::MAX_REFERENCES
                && !group.reference_ids.contains(&id)
                && self.can_pin_reference(*id)
            {
                group.reference_ids.push(*id);
            }
        }
        if let Some(id) = remove {
            group.reference_ids.retain(|value| *value != id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn references_keep_original_bytes_through_save_and_undo_and_empty_actions_are_bounded() {
        let ctx = egui::Context::default();
        let mut studio = Studio::from_context(&ctx);
        studio.welcome = false;
        studio.tab = LibraryTab::Groups;
        for size in [
            vec2(1100.0, 720.0),
            vec2(1280.0, 760.0),
            vec2(2400.0, 1400.0),
        ] {
            for _ in 0..2 {
                let _ = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, size)),
                        ..Default::default()
                    },
                    |ctx| {
                        studio.library(ctx);
                        studio.groups_workspace(ctx);
                    },
                );
            }
            let action: Rect =
                ctx.data(|d| d.get_temp(egui::Id::new("empty_group_action")).unwrap());
            assert!(action.width() <= 150.0 && action.height() <= 40.0);
            assert!(Rect::from_min_size(egui::Pos2::ZERO, size).contains_rect(action));
        }
        studio.new_group();
        let group = studio.selected_group.unwrap();
        let image = image::RgbaImage::from_pixel(24, 32, image::Rgba([12, 55, 77, 255]));
        let mut png = std::io::Cursor::new(Vec::new());
        image.write_to(&mut png, image::ImageFormat::Png).unwrap();
        let bytes = png.into_inner();
        let before = studio.story.clone();
        studio.import_group_reference_bytes(group, bytes.clone(), "Original".into());
        let asset = studio.story.groups[0].reference_ids[0];
        assert_eq!(*studio.assets[&asset], bytes);
        let path =
            std::env::temp_dir().join(format!("scroll-world-test-{}.scro", std::process::id()));
        storage::save_story(&path, &studio.story, &studio.assets).unwrap();
        let (loaded, assets) = storage::load_story(&path).unwrap();
        assert_eq!(loaded, studio.story);
        assert_eq!(*assets[&asset], bytes);
        std::fs::remove_file(path).unwrap();
        studio.undo();
        assert_eq!(studio.story, before);
        studio.redo();
        assert_eq!(studio.story, loaded);
        let mut legacy = serde_json::to_value(&before).unwrap();
        legacy.as_object_mut().unwrap().remove("groups");
        legacy.as_object_mut().unwrap().remove("timeline");
        let legacy: Story = serde_json::from_value(legacy).unwrap();
        assert!(legacy.groups.is_empty() && legacy.timeline.is_empty());
    }
}
