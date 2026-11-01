use super::*;
use egui::collapsing_header::CollapsingState;
use scroll_studio::locations;

pub(super) struct LocationEditor {
    id: Option<u64>,
    name: String,
    focus: bool,
    error: Option<String>,
}

fn folder_id(epoch: u64, location: Option<u64>) -> egui::Id {
    egui::Id::new(("background_location", epoch, location))
}

impl Studio {
    pub(super) fn sanitize_location_selection(&mut self) {
        if self.selected_location.is_some_and(|id| {
            !self
                .story
                .locations
                .iter()
                .any(|location| location.id == id)
        }) {
            self.selected_location = None;
        }
    }

    pub(super) fn begin_location_editor(&mut self, id: Option<u64>) {
        let name = id
            .and_then(|id| {
                self.story
                    .locations
                    .iter()
                    .find(|location| location.id == id)
                    .map(|location| location.name.clone())
            })
            .unwrap_or_default();
        self.location_editor = Some(LocationEditor {
            id,
            name,
            focus: true,
            error: None,
        });
    }

    fn save_location_name(&mut self, id: Option<u64>, name: &str) -> anyhow::Result<u64> {
        // Stage validation before touching history. Empty or unchanged names
        // must not discard the redo branch.
        let mut updated = self.story.clone();
        let id = if let Some(id) = id {
            if !locations::rename_location(&mut updated, id, name)? {
                return Ok(id);
            }
            id
        } else {
            locations::add_location(&mut updated, name)?
        };
        self.checkpoint();
        self.story = updated;
        self.selected_location = Some(id);
        self.selected_asset = None;
        self.changed();
        Ok(id)
    }

    pub(super) fn location_editor_ui(&mut self, ctx: &egui::Context) {
        let Some(mut editor) = self.location_editor.take() else {
            return;
        };
        let mut submit = false;
        let mut cancel = false;
        let modal = egui::Modal::new(egui::Id::new("location_editor")).show(ctx, |ui| {
            ui.set_width(280.0);
            ui.heading(if editor.id.is_some() {
                "Rename location"
            } else {
                "New location"
            });
            ui.add_space(8.0);
            let response = ui.add(
                egui::TextEdit::singleline(&mut editor.name)
                    .hint_text("Location name")
                    .char_limit(locations::MAX_LOCATION_NAME_BYTES)
                    .desired_width(f32::INFINITY),
            );
            if editor.focus {
                response.request_focus();
                editor.focus = false;
            }
            submit |=
                response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
            #[cfg(test)]
            ui.data_mut(|data| {
                data.insert_temp(egui::Id::new("location_name_bounds"), response.rect)
            });
            if let Some(error) = &editor.error {
                ui.colored_label(Color32::LIGHT_RED, error);
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let save = ui.add_enabled(
                    !editor.name.trim().is_empty(),
                    motion::Button::new(if editor.id.is_some() {
                        "Save"
                    } else {
                        "Create"
                    }),
                );
                submit |= save.clicked();
                #[cfg(test)]
                ui.data_mut(|data| {
                    data.insert_temp(egui::Id::new("location_save_bounds"), save.rect)
                });
                cancel = motion::button(ui, "Cancel").clicked();
            });
        });
        if cancel || modal.should_close() {
            return;
        }
        if submit {
            match self.save_location_name(editor.id, &editor.name) {
                Ok(id) => {
                    let mut folder = CollapsingState::load_with_default_open(
                        ctx,
                        folder_id(self.document_epoch, Some(id)),
                        true,
                    );
                    folder.set_open(true);
                    folder.store(ctx);
                    return;
                }
                Err(error) => editor.error = Some(error.to_string()),
            }
        }
        self.location_editor = Some(editor);
    }

    pub(super) fn assign_background_location(&mut self, asset_id: u64, location: Option<u64>) {
        if let Err(error) = locations::move_asset(&mut self.story, asset_id, location) {
            self.error = Some(format!("Could not move background: {error}"));
        }
    }

    pub(super) fn move_background(
        &mut self,
        asset_id: u64,
        location: Option<u64>,
    ) -> anyhow::Result<()> {
        let mut updated = self.story.clone();
        if locations::move_asset(&mut updated, asset_id, location)? {
            self.checkpoint();
            self.story = updated;
            self.selected_location = location;
            self.changed();
        }
        Ok(())
    }

    pub(super) fn background_move_menu(&mut self, ui: &mut egui::Ui, asset_id: u64) {
        let current = locations::location_for_asset(&self.story, asset_id);
        let folders: Vec<_> = std::iter::once((None, "Unsorted".to_owned()))
            .chain(
                self.story
                    .locations
                    .iter()
                    .map(|location| (Some(location.id), location.name.clone())),
            )
            .collect();
        let mut target = None;
        motion::menu_button(ui, "Move to", |ui| {
            egui::ScrollArea::vertical()
                .max_height(240.0)
                .show(ui, |ui| {
                    for (id, name) in &folders {
                        if ui.selectable_label(*id == current, name).clicked() {
                            target = Some(*id);
                            ui.close_menu();
                        }
                    }
                });
        });
        if let Some(target) = target {
            if let Err(error) = self.move_background(asset_id, target) {
                self.error = Some(error.to_string());
            } else {
                let mut folder = CollapsingState::load_with_default_open(
                    ui.ctx(),
                    folder_id(self.document_epoch, target),
                    true,
                );
                folder.set_open(true);
                folder.store(ui.ctx());
            }
        }
    }

    pub(super) fn background_library(&mut self, ui: &mut egui::Ui) {
        self.sanitize_location_selection();
        if super::panels::list_row(
            ui,
            "All locations",
            self.selected_location.is_none(),
            32.0,
            Some(icons::Icon::Background),
        )
        .clicked()
        {
            self.selected_location = None;
            self.selected_asset = None;
        }
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            // The gallery owns creation while browsing. Keep this shortcut
            // only when a particular location sheet is open.
            if self.selected_location.is_some() && motion::button(ui, "+ Location").clicked() {
                self.begin_location_editor(None);
            }
            let name = self
                .selected_location
                .and_then(|id| {
                    self.story
                        .locations
                        .iter()
                        .find(|location| location.id == id)
                        .map(|location| location.name.as_str())
                })
                .unwrap_or("Unsorted");
            if motion::button(ui, "+ Import")
                .on_hover_text(format!("Add images to {name}"))
                .clicked()
            {
                self.import_picker();
            }
        });
        ui.add(
            egui::TextEdit::singleline(&mut self.search)
                .hint_text("Search backgrounds")
                .desired_width(f32::INFINITY),
        );
        let search = self.search.to_lowercase();
        let membership: HashMap<_, _> = self
            .story
            .locations
            .iter()
            .flat_map(|location| location.asset_ids.iter().map(move |id| (*id, location.id)))
            .collect();
        let mut groups: HashMap<Option<u64>, Vec<Asset>> = HashMap::new();
        for asset in self
            .story
            .assets
            .iter()
            .filter(|asset| asset.kind == AssetKind::Background)
        {
            groups
                .entry(membership.get(&asset.id).copied())
                .or_default()
                .push(asset.clone());
        }
        let folders: Vec<_> = self
            .story
            .locations
            .iter()
            .map(|location| (Some(location.id), location.name.clone()))
            .chain(std::iter::once((None, "Unsorted".to_owned())))
            .collect();
        egui::ScrollArea::vertical()
            .id_salt("background_locations")
            .show(ui, |ui| {
                for (id, name) in folders {
                    let assets = groups.remove(&id).unwrap_or_default();
                    let count = assets.len();
                    let folder_matches = name.to_lowercase().contains(&search);
                    let visible: Vec<_> = assets
                        .into_iter()
                        .filter(|asset| {
                            search.is_empty()
                                || folder_matches
                                || asset.name.to_lowercase().contains(&search)
                        })
                        .collect();
                    if !search.is_empty() && !folder_matches && visible.is_empty() {
                        continue;
                    }
                    ui.push_id(id, |ui| {
                        let state = CollapsingState::load_with_default_open(
                            ui.ctx(),
                            folder_id(self.document_epoch, id),
                            id.is_none() || self.selected_location == id,
                        );
                        let mut selected = false;
                        let mut rename = false;
                        let mut header = state.show_header(ui, |ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            let width = (ui.available_width()
                                - if id.is_some() { 29.0 } else { 0.0 })
                            .max(40.0);
                            let response = ui.add_sized(
                                [width, 30.0],
                                motion::Button::new(format!("{name}  {count}"))
                                    .frame(false)
                                    .selected(self.selected_location == id)
                                    .truncate(),
                            );
                            selected = response.clicked();
                            if response.secondary_clicked() {
                                self.selected_location = id;
                            }
                            self.register_context(&response);
                            appearance::context_menu(&response, |ui| {
                                self.location_context_actions(ui, id)
                            });
                            #[cfg(test)]
                            ui.data_mut(|data| {
                                data.insert_temp(
                                    egui::Id::new(("location_header_bounds", id)),
                                    response.rect,
                                )
                            });
                            if id.is_some() {
                                motion::menu_button(ui, "…", |ui| {
                                    if motion::button(ui, "Rename").clicked() {
                                        rename = true;
                                        ui.close_menu();
                                    }
                                });
                            }
                        });
                        if selected {
                            self.selected_location = id;
                            self.selected_asset = None;
                            header.set_open(true);
                        }
                        if !search.is_empty() {
                            header.set_open(true);
                        }
                        header.body(|ui| {
                            for asset in visible {
                                self.asset_library_card(ui, &asset);
                            }
                        });
                        if rename {
                            self.begin_location_editor(id);
                        }
                    });
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (egui::Context, Studio) {
        let ctx = egui::Context::default();
        let mut studio = Studio::from_context(&ctx);
        studio.story = Story::new("Location workflow");
        studio.tab = LibraryTab::Backgrounds;
        studio.welcome = false;
        studio.error = None;
        studio.dirty = false;
        studio.preferences.motion = settings::Motion::Off;
        (ctx, studio)
    }

    fn image_bytes() -> Vec<u8> {
        let image = image::RgbaImage::from_pixel(4, 8, image::Rgba([20, 40, 60, 255]));
        let mut bytes = std::io::Cursor::new(Vec::new());
        image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
        bytes.into_inner()
    }

    fn frame(ctx: &egui::Context, studio: &mut Studio, size: egui::Vec2, events: Vec<egui::Event>) {
        let mut frame = eframe::Frame::_new_kittest();
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                events,
                ..Default::default()
            },
            |ctx| eframe::App::update(studio, ctx, &mut frame),
        );
    }

    fn bounds(ctx: &egui::Context, key: impl std::hash::Hash) -> egui::Rect {
        ctx.data(|data| data.get_temp(egui::Id::new(key)).unwrap())
    }

    fn click(pos: egui::Pos2) -> Vec<egui::Event> {
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            },
        ]
    }

    #[test]
    fn named_folder_creation_is_compact_and_opens_location_workspace() {
        for size in [egui::vec2(1100.0, 720.0), egui::vec2(1366.0, 768.0)] {
            let (ctx, mut studio) = fixture();
            frame(&ctx, &mut studio, size, Vec::new());
            frame(&ctx, &mut studio, size, Vec::new());
            let add = bounds(&ctx, "add_location_bounds");
            frame(&ctx, &mut studio, size, click(add.center()));
            assert!(studio.location_editor.is_some());
            frame(&ctx, &mut studio, size, Vec::new());
            let field = bounds(&ctx, "location_name_bounds");
            assert!(field.width() <= 281.0);
            frame(
                &ctx,
                &mut studio,
                size,
                vec![egui::Event::Text("Rainy station".into())],
            );
            let create = bounds(&ctx, "location_save_bounds");
            frame(&ctx, &mut studio, size, click(create.center()));
            assert!(studio.location_editor.is_none());
            assert_eq!(studio.story.locations.len(), 1);
            let location = studio.story.locations[0].id;
            assert_eq!(studio.story.locations[0].name, "Rainy station");
            assert!(studio.story.assets.is_empty());
            assert_eq!(studio.selected_location, Some(location));
            frame(&ctx, &mut studio, size, Vec::new());
            let header = bounds(&ctx, ("location_header_bounds", Some(location)));
            assert!(header.right() <= 200.0 && header.width() < 180.0);
            assert!(!studio.canvas_rect.is_positive());
            assert!(studio.tab == LibraryTab::Backgrounds);
            assert_eq!(studio.undo.len(), 1);
            studio.undo();
            assert!(studio.story.locations.is_empty());
            assert_eq!(studio.selected_location, None);
            studio.redo();
            assert_eq!(studio.story.locations[0].name, "Rainy station");
        }
    }

    #[test]
    fn imports_and_canvas_placement_follow_active_folder_but_images_stay_global() {
        let (_, mut studio) = fixture();
        let location = studio.save_location_name(None, "Station").unwrap();
        studio.import_bytes(image_bytes(), "Platform".into(), true, Some([64.0, 128.0]));
        assert!(studio.error.is_none());
        let asset = studio.story.assets.last().unwrap().id;
        assert_eq!(
            locations::location_for_asset(&studio.story, asset),
            Some(location)
        );
        assert!(
            matches!(studio.story.chapters[0].layers.last().unwrap().kind,
            LayerKind::Image { asset_id, .. } if asset_id == asset)
        );
        let layer = studio.story.chapters[0].layers.last().unwrap();
        assert_eq!((layer.x, layer.y), (64.0, 128.0));
        let before_global = studio.story.locations.clone();
        studio.tab = LibraryTab::Images;
        studio.import_bytes(image_bytes(), "Unrelated image".into(), false, None);
        assert_eq!(studio.story.assets.last().unwrap().kind, AssetKind::Image);
        assert_eq!(studio.story.locations, before_global);
        studio.undo();
        studio.undo();
        assert!(studio.story.chapters[0].layers.is_empty());
        studio.undo();
        assert!(studio.story.assets.is_empty());
        assert!(studio.story.locations[0].asset_ids.is_empty());
    }

    #[test]
    fn rename_and_move_are_separate_undo_steps_without_empty_history_entries() {
        let (_, mut studio) = fixture();
        let station = studio.save_location_name(None, "Station").unwrap();
        studio.import_bytes(image_bytes(), "Platform".into(), false, None);
        let asset = studio.selected_asset.unwrap();
        let cafe = studio.save_location_name(None, "Cafe").unwrap();
        let initial = studio.story.clone();
        studio.save_location_name(Some(cafe), "Night cafe").unwrap();
        let renamed = studio.story.clone();
        studio.move_background(asset, Some(cafe)).unwrap();
        assert_eq!(
            locations::location_for_asset(&studio.story, asset),
            Some(cafe)
        );
        assert!(!studio
            .story
            .locations
            .iter()
            .find(|folder| folder.id == station)
            .unwrap()
            .asset_ids
            .contains(&asset));
        studio.undo();
        assert_eq!(studio.story, renamed);
        studio.undo();
        assert_eq!(studio.story, initial);
        let history = studio.undo.len();
        let redo = studio.redo.clone();
        assert!(studio.save_location_name(Some(cafe), "  ").is_err());
        studio.save_location_name(Some(cafe), "Cafe").unwrap();
        studio.move_background(asset, Some(station)).unwrap();
        assert_eq!(studio.undo.len(), history);
        assert_eq!(studio.redo, redo);
        studio.move_background(asset, None).unwrap();
        assert_eq!(locations::location_for_asset(&studio.story, asset), None);
    }

    #[test]
    fn old_story_backgrounds_and_stale_folder_targets_use_unsorted() {
        let (ctx, mut studio) = fixture();
        studio.selected_location = Some(u64::MAX);
        studio.import_bytes(image_bytes(), "Old background".into(), false, None);
        assert!(studio.error.is_none());
        assert_eq!(studio.selected_location, None);
        let asset = studio.selected_asset.unwrap();
        let mut legacy = serde_json::to_value(&studio.story).unwrap();
        legacy.as_object_mut().unwrap().remove("locations");
        studio.story = serde_json::from_value(legacy).unwrap();
        assert!(studio.story.locations.is_empty());
        frame(&ctx, &mut studio, egui::vec2(1100.0, 720.0), Vec::new());
        frame(&ctx, &mut studio, egui::vec2(1100.0, 720.0), Vec::new());
        assert!(bounds(&ctx, ("location_header_bounds", None::<u64>)).width() > 40.0);
        assert!(bounds(&ctx, ("asset_card_bounds", asset)).height() > 40.0);
    }
}
