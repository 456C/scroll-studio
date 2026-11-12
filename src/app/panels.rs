use super::*;
use egui::{vec2, Align, Layout};

fn layer_properties_header(ui: &mut egui::Ui, layer: &mut Layer) {
    appearance::surface_frame(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            let name = ui.add_sized(
                [(ui.available_width() - 66.0).max(80.0), 30.0],
                egui::TextEdit::singleline(&mut layer.name)
                    .frame(false)
                    .font(egui::FontId::proportional(15.0))
                    .hint_text("Layer name"),
            );
            let visibility = icons::button(
                ui,
                if layer.visible {
                    icons::Icon::Eye
                } else {
                    icons::Icon::EyeOff
                },
                !layer.visible,
                if layer.visible {
                    "Hide layer"
                } else {
                    "Show layer"
                },
            );
            if visibility.clicked() {
                layer.visible = !layer.visible;
            }
            let lock = icons::button(
                ui,
                if layer.locked {
                    icons::Icon::Lock
                } else {
                    icons::Icon::Unlock
                },
                layer.locked,
                if layer.locked {
                    "Unlock layer"
                } else {
                    "Lock layer"
                },
            );
            if lock.clicked() {
                layer.locked = !layer.locked;
            }
            #[cfg(test)]
            ui.data_mut(|data| {
                data.insert_temp(
                    egui::Id::new("inspector_layer_controls"),
                    [name.rect, visibility.rect, lock.rect],
                );
            });
            let _ = name;
        });
        if !matches!(layer.kind, LayerKind::Text { .. }) {
            ui.add_space(4.0);
            ui.add_enabled_ui(!layer.locked, |ui| {
                ui.label(RichText::new("Opacity").size(11.0).color(MUTED));
                ui.spacing_mut().slider_width = (ui.available_width() - 68.0).max(72.0);
                let mut percent = layer.opacity * 100.0;
                let opacity = ui.add(
                    egui::Slider::new(&mut percent, 0.0..=100.0)
                        .suffix("%")
                        .max_decimals(0),
                );
                if opacity.changed() {
                    layer.opacity = percent / 100.0;
                }
                #[cfg(test)]
                ui.data_mut(|data| {
                    data.insert_temp(
                        egui::Id::new("inspector_opacity_bounds"),
                        (opacity.rect, opacity.enabled(), ui.spacing().slider_width),
                    )
                });
            });
        }
    });
}

impl Studio {
    pub(super) fn top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top")
            .exact_height(48.0)
            .frame(
                egui::Frame::new()
                    .fill(PANEL)
                    .stroke(egui::Stroke::new(1.0_f32, BG))
                    .inner_margin(egui::Margin::symmetric(12, 5)),
            )
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    motion::menu_button(ui, "File", |ui| {
                        if motion::button(
                            ui,
                            if self.assets_only {
                                "New world          Ctrl+N"
                            } else {
                                "New story          Ctrl+N"
                            },
                        )
                        .clicked()
                        {
                            ui.close_menu();
                            self.request(Pending::New);
                        }
                        if motion::button(
                            ui,
                            if self.assets_only {
                                "Open world         Ctrl+O"
                            } else {
                                "Open story         Ctrl+O"
                            },
                        )
                        .clicked()
                        {
                            ui.close_menu();
                            self.request(Pending::Open(None));
                        }
                        if motion::button(ui, "Recover previous work…").clicked() {
                            ui.close_menu();
                            self.scan_recovery(ctx, true);
                        }
                        ui.separator();
                        if ui
                            .add_enabled(
                                !self.welcome,
                                motion::Button::new("Save                    Ctrl+S"),
                            )
                            .clicked()
                        {
                            ui.close_menu();
                            self.save(false);
                        }
                        if ui
                            .add_enabled(
                                !self.welcome,
                                motion::Button::new("Save as…    Ctrl+Shift+S"),
                            )
                            .clicked()
                        {
                            ui.close_menu();
                            self.save(true);
                        }
                        ui.separator();
                        if !self.welcome && motion::button(ui, "Start screen").clicked() {
                            ui.close_menu();
                            self.request(Pending::Home);
                        }
                        if motion::button(ui, "Settings          Ctrl+,").clicked() {
                            self.settings_open = true;
                            ui.close_menu();
                        }
                    });
                    if self.welcome || self.settings_open {
                        ui.label(RichText::new("Scroll Studio").size(18.0).strong());
                    }
                    if !self.welcome && !self.settings_open {
                        motion::menu_button(ui, RichText::new("+ New").color(ACCENT), |ui| {
                            if !self.assets_only && motion::button(ui, "Chapter").clicked() {
                                self.dialog_name =
                                    format!("Chapter {:02}", self.story.chapters.len() + 1);
                                self.dialog = Some(Dialog::Chapter);
                                ui.close_menu();
                            }
                            if motion::button(ui, "Character").clicked() {
                                self.dialog_name = String::new();
                                self.dialog = Some(Dialog::Asset(AssetKind::Character));
                                ui.close_menu();
                            }
                            if motion::button(ui, "Location").clicked() {
                                self.tab = LibraryTab::Backgrounds;
                                self.begin_location_editor(None);
                                ui.close_menu();
                            }
                            if motion::button(ui, "Group").clicked() {
                                self.new_group();
                                ui.close_menu();
                            }
                            if motion::button(ui, "Lore entry").clicked() {
                                self.new_lore();
                                ui.close_menu();
                            }
                            if !self.assets_only && motion::button(ui, "Bubble preset").clicked() {
                                self.add_bubble_preset(false);
                                ui.close_menu();
                            }
                        });
                        ui.add_space(8.0);
                        let title_width = (ui.available_width() - 348.0).clamp(80.0, 400.0);
                        ui.add_sized(
                            [title_width, 28.0],
                            egui::Label::new(RichText::new(&self.story.title).strong()).truncate(),
                        );
                        if self.dirty {
                            ui.label(RichText::new("•").color(ACCENT))
                                .on_hover_text("Unsaved changes");
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if !self.assets_only
                                && ui
                                    .add_enabled(
                                        self.job.is_none(),
                                        motion::Button::new(RichText::new("Export").color(BG))
                                            .fill(ACCENT),
                                    )
                                    .clicked()
                            {
                                self.export_dialog = true;
                            }
                            if ui
                                .add_enabled(self.job.is_none(), motion::Button::new("Save"))
                                .clicked()
                            {
                                self.save(false);
                            }
                            ui.separator();
                            let copy_style = if self.story.visual_style.trim().is_empty() {
                                motion::button(ui, "Copy style")
                            } else {
                                motion::copy_button(
                                    ui,
                                    ("copy_style", self.document_epoch),
                                    "Copy style",
                                    false,
                                )
                            };
                            if copy_style
                                .on_hover_text("Copy story style · Ctrl+Shift+C")
                                .clicked()
                            {
                                if self.story.visual_style.trim().is_empty() {
                                    self.style_prompt_open = true;
                                } else {
                                    ctx.copy_text(self.story.visual_style.clone());
                                    self.status = "Style copied".into();
                                }
                            }
                            if motion::button(ui, "Style").clicked() {
                                self.style_prompt_open = true;
                            }
                        });
                    }
                });
            });
    }

    pub(super) fn status_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("status")
            .exact_height(22.0)
            .frame(
                egui::Frame::new()
                    .fill(BG)
                    .inner_margin(egui::Margin::symmetric(12, 2)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if self.job.is_some() {
                        ui.spinner();
                        ui.label(RichText::new(&self.busy_label).size(12.0).color(ACCENT));
                    } else {
                        ui.add(
                            egui::Label::new(RichText::new(&self.status).size(11.0).color(MUTED))
                                .truncate(),
                        );
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let failed = self.recovery.notice.contains("failed")
                            || self.recovery.notice.contains("unavailable")
                            || self.recovery.notice.contains("stopped");
                        ui.label(
                            RichText::new(if failed {
                                "Recovery needs attention"
                            } else {
                                "Recovery on"
                            })
                            .size(11.0)
                            .color(if failed {
                                Color32::from_rgb(242, 193, 121)
                            } else {
                                MUTED
                            }),
                        )
                        .on_hover_text(&self.recovery.notice);
                    });
                });
            });
    }

    pub(super) fn welcome_screen(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG))
            .show(ctx, |ui| {
                let center = ui.max_rect().center();
                let line = egui::Stroke::new(1.0_f32, CARD);
                ui.painter()
                    .line_segment([center - vec2(0.0, 132.0), center + vec2(0.0, 132.0)], line);
                let column = |x, y, height| {
                    egui::UiBuilder::new()
                        .max_rect(egui::Rect::from_min_size(
                            center + vec2(x, y),
                            vec2(238.0, height),
                        ))
                        .layout(Layout::top_down(Align::Center))
                };
                ui.scope_builder(column(-294.0, -137.0, 274.0), |ui| {
                    ui.image((self.app_icon.id(), vec2(56.0, 56.0)));
                    ui.add_space(9.0);
                    ui.heading(RichText::new("Your stories").size(28.0));
                    ui.add_space(16.0);
                    if ui
                        .add_sized(
                            [224.0, 42.0],
                            motion::Button::new(RichText::new("+ New story").color(BG))
                                .fill(ACCENT),
                        )
                        .clicked()
                    {
                        self.assets_only = false;
                        self.request(Pending::New);
                    }
                    if ui
                        .add_sized([224.0, 40.0], motion::Button::new("Open story…"))
                        .clicked()
                    {
                        self.assets_only = false;
                        self.request(Pending::Open(None));
                    }
                    if motion::button(ui, "Open sample").clicked() {
                        self.assets_only = false;
                        self.story = Story::demo();
                        self.assets.clear();
                        self.path = None;
                        self.reset_view();
                        self.welcome = false;
                        self.changed();
                    }
                });
                ui.scope_builder(column(56.0, -70.0, 180.0), |ui| {
                    if self.assets_only {
                        ui.heading("Edit Assets");
                        ui.add_space(8.0);
                        if ui
                            .add_sized([224.0, 40.0], motion::Button::new("Open .scro…"))
                            .clicked()
                        {
                            self.request(Pending::Open(None));
                        }
                        if ui
                            .add_sized([224.0, 40.0], motion::Button::new("+ New world"))
                            .clicked()
                        {
                            self.request(Pending::New);
                        }
                        if motion::button(ui, "Back").clicked() {
                            self.assets_only = false;
                        }
                    } else {
                        for (label, icon) in [
                            ("Edit Assets", icons::Icon::Groups),
                            ("Image Editing", icons::Icon::Brush),
                        ] {
                            let response = ui.add_sized([224.0, 52.0], motion::Button::new(label));
                            icons::draw(
                                ui.painter(),
                                egui::Rect::from_center_size(
                                    egui::pos2(
                                        response.rect.left() + 25.0,
                                        response.rect.center().y,
                                    ),
                                    vec2(19.0, 19.0),
                                ),
                                icon,
                                MUTED,
                            );
                            if response.clicked() {
                                if label == "Edit Assets" {
                                    self.assets_only = true;
                                } else {
                                    self.paint = Some(paint_ui::PaintStudio::default());
                                }
                            }
                            ui.add_space(3.0);
                        }
                    }
                });
            });
    }

    pub(super) fn library(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("library")
            .exact_width(200.0)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(PANEL)
                    .stroke(egui::Stroke::new(1.0_f32, BG))
                    .inner_margin(10.0),
            )
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                for (tab, label, icon) in [
                    (LibraryTab::Chapters, "Chapters", icons::Icon::Chapter),
                    (LibraryTab::Bubbles, "Bubbles", icons::Icon::Bubble),
                    (
                        LibraryTab::Characters,
                        "Characters",
                        icons::Icon::Characters,
                    ),
                    (
                        LibraryTab::Backgrounds,
                        "Locations",
                        icons::Icon::Background,
                    ),
                    (LibraryTab::Groups, "Groups", icons::Icon::Groups),
                    (LibraryTab::Timeline, "Timeline", icons::Icon::Timeline),
                    (LibraryTab::Lore, "Lore", icons::Icon::Lore),
                    (LibraryTab::Images, "Images", icons::Icon::Image),
                ] {
                    if self.assets_only
                        && !matches!(
                            tab,
                            LibraryTab::Characters
                                | LibraryTab::Backgrounds
                                | LibraryTab::Groups
                                | LibraryTab::Timeline
                                | LibraryTab::Lore
                        )
                    {
                        continue;
                    }
                    if list_row(
                        ui,
                        label,
                        !self.settings_open && self.tab == tab,
                        34.0,
                        Some(icon),
                    )
                    .clicked()
                    {
                        if tab == LibraryTab::Backgrounds && self.tab != tab {
                            self.selected_location = None;
                        }
                        self.tab = tab;
                        self.settings_open = false;
                        self.selected_asset = None;
                        self.search.clear();
                    }
                }
                ui.add_space(4.0);
                ui.separator();
                if list_row(
                    ui,
                    "Settings",
                    self.settings_open,
                    34.0,
                    Some(icons::Icon::Settings),
                )
                .on_hover_text("Settings · Ctrl+,")
                .clicked()
                {
                    self.settings_open = true;
                }
                ui.separator();
                if self.settings_open {
                    return;
                }
                ui.add_space(4.0);
                if self.tab == LibraryTab::Chapters {
                    ui.horizontal(|ui| {
                        if motion::button(ui, "+ Chapter").clicked() {
                            self.dialog_name =
                                format!("Chapter {:02}", self.story.chapters.len() + 1);
                            self.dialog = Some(Dialog::Chapter);
                        }
                        motion::menu_button(ui, "Notes", |ui| {
                            ui.set_width(300.0);
                            let mut synopsis = self.story.synopsis.clone();
                            egui::ScrollArea::vertical()
                                .max_height(220.0)
                                .show(ui, |ui| {
                                    ui.add(
                                        egui::TextEdit::multiline(&mut synopsis)
                                            .hint_text("Story notes")
                                            .desired_rows(8)
                                            .desired_width(f32::INFINITY),
                                    );
                                });
                            if synopsis != self.story.synopsis {
                                let command = Command::SetField {
                                    target: Target::Story,
                                    field: Field::Synopsis,
                                    value: synopsis.clone(),
                                };
                                self.commit(Some(command), |studio| {
                                    studio.story.synopsis = synopsis
                                });
                            }
                        });
                    });
                    let chapters: Vec<_> = self
                        .story
                        .chapters
                        .iter()
                        .map(|c| (c.id, c.name.clone(), c.panels.len()))
                        .collect();
                    egui::ScrollArea::vertical()
                        .id_salt("chapters")
                        .show(ui, |ui| {
                            for (index, (id, name, count)) in chapters.iter().enumerate() {
                                let response =
                                    list_row(ui, name, index == self.chapter, 34.0, None)
                                        .on_hover_text(format!("{count} script beats"));
                                if response.clicked() || response.secondary_clicked() {
                                    self.chapter = index;
                                    self.selected = None;
                                    self.active_panel = None;
                                    self.jump_y = Some(0.0);
                                }
                                self.register_context(&response);
                                appearance::context_menu(&response, |ui| {
                                    self.chapter_context_actions(ui, *id)
                                });
                            }
                        });
                } else if self.tab == LibraryTab::Bubbles {
                    self.bubble_preset_list(ui);
                } else if self.tab == LibraryTab::Characters {
                    self.character_list(ui);
                } else if self.tab == LibraryTab::Backgrounds {
                    self.background_library(ui);
                } else if self.tab == LibraryTab::Lore {
                    self.lore_list(ui);
                } else if matches!(self.tab, LibraryTab::Groups | LibraryTab::Timeline) {
                    // These workspaces own their lists; leave navigation uncluttered.
                } else {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.search)
                            .hint_text("Search")
                            .desired_width(f32::INFINITY),
                    );
                    if ui
                        .add_sized(
                            [ui.available_width(), 34.0],
                            motion::Button::new("+ Import"),
                        )
                        .clicked()
                    {
                        self.import_picker();
                    }
                    let kind = match self.tab {
                        LibraryTab::Characters => AssetKind::Character,
                        LibraryTab::Backgrounds => AssetKind::Background,
                        _ => AssetKind::Image,
                    };
                    let assets: Vec<_> = self
                        .story
                        .assets
                        .iter()
                        .filter(|a| {
                            a.kind == kind
                                && a.name.to_lowercase().contains(&self.search.to_lowercase())
                        })
                        .cloned()
                        .collect();
                    egui::ScrollArea::vertical()
                        .id_salt("asset_list")
                        .show(ui, |ui| {
                            if assets.is_empty() {
                                ui.add_space(20.0);
                                ui.label(RichText::new("No images").color(MUTED));
                            }
                            for asset in assets {
                                self.asset_library_card(ui, &asset);
                            }
                        });
                }
            });
    }

    pub(super) fn asset_library_card(&mut self, ui: &mut egui::Ui, asset: &Asset) {
        let ctx = ui.ctx().clone();
        let card_id = egui::Id::new(("asset", asset.id));
        let dragging = ctx.is_being_dragged(card_id);
        let layer_id = if dragging {
            egui::LayerId::new(egui::Order::Tooltip, card_id)
        } else {
            ui.layer_id()
        };
        let source = ui.scope_builder(egui::UiBuilder::new().layer_id(layer_id), |ui| {
            let mut card = egui::Frame::new()
                .corner_radius(7.0)
                .inner_margin(10.0)
                .stroke(egui::Stroke::new(1.0_f32, Color32::TRANSPARENT))
                .begin(ui);
            {
                let ui = &mut card.content_ui;
                let width = ui.available_width().max(1.0);
                ui.set_width(width);
                if asset.width > 0 && asset.height > 0 {
                    let size = vec2(asset.width as f32, asset.height as f32);
                    let scale = (width / size.x).min(100.0 / size.y);
                    let (rect, _) = ui.allocate_exact_size(size * scale, egui::Sense::hover());
                    if ui.is_rect_visible(rect) {
                        if let Some(texture) = self.asset_thumbnail(&ctx, asset.id) {
                            ui.painter().image(
                                texture.id(),
                                rect,
                                egui::Rect::from_min_max(
                                    egui::pos2(0.0, 0.0),
                                    egui::pos2(1.0, 1.0),
                                ),
                                Color32::WHITE,
                            );
                        }
                    }
                }
                ui.add(egui::Label::new(&asset.name).truncate());
            }
            let bounds = card.allocate_space(ui).rect;
            let response = ui
                .interact(bounds, card_id, egui::Sense::click_and_drag())
                .on_hover_cursor(egui::CursorIcon::Grab);
            let selected = self.selected_asset == Some(asset.id);
            response.widget_info(|| {
                egui::WidgetInfo::selected(
                    egui::WidgetType::SelectableLabel,
                    ui.is_enabled(),
                    selected,
                    &asset.name,
                )
            });
            let feedback = motion::feedback(ui, &response, selected);
            let fill = motion::mix(CARD, Color32::from_rgb(65, 69, 77), feedback.hover * 0.6);
            card.frame.fill = motion::mix(
                fill,
                ui.visuals().selection.bg_fill,
                feedback.selected * 0.65,
            );
            card.frame.stroke = egui::Stroke::new(
                1.0_f32,
                motion::mix(
                    Color32::from_white_alpha((feedback.hover * 20.0) as u8),
                    ACCENT,
                    feedback.selected,
                ),
            );
            card.paint(ui);
            #[cfg(test)]
            ctx.data_mut(|data| {
                data.insert_temp(
                    egui::Id::new(("asset_card_bounds", asset.id)),
                    response.rect,
                )
            });
            response
        });
        if dragging {
            egui::DragAndDrop::set_payload(&ctx, asset.id);
            if let Some(pointer) = ctx.pointer_interact_pos() {
                ctx.transform_layer_shapes(
                    layer_id,
                    egui::emath::TSTransform::from_translation(
                        pointer - source.response.rect.center(),
                    ),
                );
            }
        }
        if source.inner.clicked() || source.inner.secondary_clicked() {
            self.selected_asset = Some(asset.id);
            self.selected = None;
            if asset.kind == AssetKind::Background {
                self.selected_location =
                    scroll_studio::locations::location_for_asset(&self.story, asset.id);
            }
        }
        if source.inner.double_clicked() {
            self.place_asset(asset.id, None);
        }
        self.register_context(&source.inner);
        appearance::context_menu(&source.inner, |ui| self.asset_context_actions(ui, asset.id));
    }

    fn replace_library_image(&mut self, index: usize, bytes: Vec<u8>, width: u32, height: u32) {
        let command = Command::AddImageVersion {
            asset: self.story.assets[index].id,
            width,
            height,
        };
        let mut fallback_id = None;
        let applied = self.commit(Some(command), |studio| {
            fallback_id = Some(studio.replace_library_image_directly(index, width, height));
        });
        let new_id = applied
            .and_then(|a| a.created.first().copied())
            .or(fallback_id)
            .expect("an image was added");
        // Image bytes live with the app, outside the command layer.
        self.assets.insert(new_id, Arc::new(bytes));
        self.selected_asset = Some(new_id);
        self.status = "Reference added".into();
    }

    /// The 0.13.2 code, used if the command refuses. The caller records history.
    fn replace_library_image_directly(&mut self, index: usize, width: u32, height: u32) -> u64 {
        let original = self.story.assets[index].clone();
        let location = scroll_studio::locations::location_for_asset(&self.story, original.id);
        // New asset identity preserves existing placements and undo snapshots.
        let new_id = self.story.alloc_id();
        let mut replacement = original.clone();
        replacement.id = new_id;
        replacement.width = width;
        replacement.height = height;
        if original.width == 0 {
            self.story.assets[index] = replacement;
            for card in &mut self.story.lore_board.cards {
                if card.target == original.id {
                    card.target = new_id;
                }
            }
            for link in self
                .story
                .lore
                .iter_mut()
                .flat_map(|entry| &mut entry.links)
            {
                if *link == original.id {
                    *link = new_id;
                }
            }
            for folder in &mut self.story.locations {
                folder.asset_ids.retain(|id| *id != original.id);
            }
        } else {
            self.story.assets.push(replacement);
        }
        if original.kind == AssetKind::Background {
            self.assign_background_location(new_id, location);
        }
        for group in &mut self.story.groups {
            if group.character_ids.contains(&original.id) {
                if original.width == 0 {
                    group.character_ids.retain(|id| *id != original.id);
                }
                group.character_ids.push(new_id);
            }
        }
        new_id
    }

    pub(super) fn inspector(&mut self, ctx: &egui::Context) {
        egui::SidePanel::right("inspector")
            .exact_width(248.0)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(PANEL)
                    .stroke(egui::Stroke::new(1.0_f32, BG))
                    .inner_margin(14.0),
            )
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("inspector_scroll")
                    .show(ui, |ui| {
                        ui.add_space(8.0);
                        if let Some(id) = self.selected_asset {
                            if let Some(index) = self.story.assets.iter().position(|a| a.id == id) {
                                let original = self.story.assets[index].clone();
                                let mut asset = original.clone();
                                ui.text_edit_singleline(&mut asset.name);
                                if asset.kind == AssetKind::Background {
                                    self.background_move_menu(ui, id);
                                }
                                // Keep the source visible while writing notes or prompts.
                                // One cached preview stays in the same place across tabs.
                                if let Some(texture) = self.asset_thumbnail(ctx, id) {
                                    let preview = ui.add(
                                        egui::Image::new(&texture)
                                            .max_size(vec2(ui.available_width(), 160.0))
                                            .sense(egui::Sense::click()),
                                    );
                                    if preview.secondary_clicked() {
                                        self.selected_asset = Some(id);
                                        self.selected = None;
                                    }
                                    self.register_context(&preview);
                                    appearance::context_menu(&preview, |ui| {
                                        self.asset_context_actions(ui, id)
                                    });
                                }
                                let section_id =
                                    egui::Id::new(("asset_details", self.document_epoch, id));
                                let mut section = ui
                                    .data_mut(|data| data.get_temp::<u8>(section_id).unwrap_or(0));
                                ui.horizontal(|ui| {
                                    for (value, label) in
                                        [(0, "Image"), (1, "Notes"), (2, "Prompt")]
                                    {
                                        ui.selectable_value(&mut section, value, label);
                                    }
                                });
                                ui.data_mut(|data| data.insert_temp(section_id, section));
                                match section {
                                    0 => {}
                                    1 => {
                                        egui::ScrollArea::vertical()
                                            .id_salt(("asset_notes", id))
                                            .max_height(180.0)
                                            .show(ui, |ui| {
                                                ui.add(
                                                    egui::TextEdit::multiline(&mut asset.notes)
                                                        .hint_text("Notes")
                                                        .desired_width(f32::INFINITY)
                                                        .desired_rows(6),
                                                );
                                            });
                                    }
                                    _ => {
                                        egui::ScrollArea::vertical()
                                            .id_salt(("asset_prompt", id))
                                            .max_height(180.0)
                                            .show(ui, |ui| {
                                                ui.add(
                                                    egui::TextEdit::multiline(&mut asset.prompt)
                                                        .hint_text("Prompt")
                                                        .desired_width(f32::INFINITY)
                                                        .desired_rows(6),
                                                );
                                            });
                                        if motion::copy_button(
                                            ui,
                                            ("copy_background", self.document_epoch, id),
                                            "Copy prompt",
                                            false,
                                        )
                                        .on_hover_text("Includes story style")
                                        .clicked()
                                        {
                                            ctx.copy_text(
                                                scroll_studio::characters::with_story_style(
                                                    &self.story.visual_style,
                                                    &asset.prompt,
                                                ),
                                            );
                                            self.status = "Prompt copied".into();
                                        }
                                    }
                                }
                                if asset != original {
                                    let command = commands::asset_edit(&original, &asset);
                                    self.commit(command, |studio| {
                                        studio.story.assets[index] = asset
                                    });
                                }
                                if section == 0
                                    && motion::button(
                                        ui,
                                        if original.width == 0 {
                                            "+ Image"
                                        } else {
                                            "+ Version"
                                        },
                                    )
                                    .clicked()
                                {
                                    if let Some(path) = rfd::FileDialog::new()
                                        .add_filter("Images", &["png", "jpg", "jpeg", "webp"])
                                        .pick_file()
                                    {
                                        let result = (|| -> anyhow::Result<_> {
                                            anyhow::ensure!(
                                                std::fs::metadata(&path)?.len() <= 64 * 1024 * 1024,
                                                "Image is too large"
                                            );
                                            let bytes = std::fs::read(path)?;
                                            let image = compositor::decode_image(&bytes)?;
                                            self.check_import_budget(bytes.len())?;
                                            Ok((bytes, image.width(), image.height()))
                                        })();
                                        match result {
                                            Ok((bytes, width, height)) => {
                                                self.replace_library_image(
                                                    index, bytes, width, height,
                                                );
                                            }
                                            Err(e) => {
                                                self.error =
                                                    Some(format!("Could not add reference: {e:#}"))
                                            }
                                        }
                                    }
                                }
                                if ui
                                    .add_enabled(
                                        original.width > 0,
                                        motion::Button::new(RichText::new("Place").color(BG))
                                            .fill(ACCENT),
                                    )
                                    .clicked()
                                {
                                    self.place_asset(id, None);
                                }
                                ui.add_space(15.0);
                                ui.separator();
                            }
                        } else if let Some(id) = self.selected {
                            if let Some(index) = self.story.chapters[self.chapter]
                                .layers
                                .iter()
                                .position(|l| l.id == id)
                            {
                                let original =
                                    self.story.chapters[self.chapter].layers[index].clone();
                                let mut layer = original.clone();
                                ui.push_id(("layer_properties", self.document_epoch, id), |ui| {
                                    layer_properties_header(ui, &mut layer);
                                });
                                ui.add_space(8.0);
                                ui.add_enabled_ui(!layer.locked, |ui| {
                                    if matches!(layer.kind, LayerKind::Text { .. }) {
                                        if let Some(preset_id) = super::lettering_ui::bubble_editor(
                                            ui,
                                            &mut layer,
                                            &self.story.bubble_presets,
                                        ) {
                                            self.selected_bubble_preset = Some(preset_id);
                                            self.tab = LibraryTab::Bubbles;
                                        }
                                    } else {
                                        ui.collapsing("Position & size", |ui| {
                                            egui::Grid::new("transform")
                                                .num_columns(2)
                                                .spacing([12.0, 8.0])
                                                .show(ui, |ui| {
                                                    ui.label("X");
                                                    ui.add(
                                                        egui::DragValue::new(&mut layer.x)
                                                            .speed(2.0),
                                                    );
                                                    ui.end_row();
                                                    ui.label("Y");
                                                    ui.add(
                                                        egui::DragValue::new(&mut layer.y)
                                                            .speed(2.0),
                                                    );
                                                    ui.end_row();
                                                    if !matches!(
                                                        layer.kind,
                                                        LayerKind::Stroke { .. }
                                                    ) {
                                                        ui.label("Width");
                                                        ui.add(
                                                            egui::DragValue::new(&mut layer.width)
                                                                .range(1.0..=16384.0)
                                                                .speed(2.0),
                                                        );
                                                        ui.end_row();
                                                        ui.label("Height");
                                                        ui.add(
                                                            egui::DragValue::new(&mut layer.height)
                                                                .range(1.0..=200000.0)
                                                                .speed(2.0),
                                                        );
                                                        ui.end_row();
                                                    }
                                                });
                                        });
                                        match &mut layer.kind {
                                            LayerKind::Image { .. } => {
                                                ui.horizontal(|ui| {
                                                    if motion::button(ui, "Lasso")
                                                        .on_hover_text("Clip image · L")
                                                        .clicked()
                                                    {
                                                        self.tool = Tool::Lasso;
                                                    }
                                                    if motion::button(ui, "Copy cutout").clicked() {
                                                        self.copy_cutout(&original);
                                                    }
                                                });
                                                self.backing_editor(ui, &mut layer);
                                            }
                                            LayerKind::Text { .. } => {}
                                            LayerKind::Effect { effect } => {
                                                self.effect_editor(ui, effect)
                                            }
                                            LayerKind::Shape { color } => {
                                                ui.color_edit_button_srgba_unmultiplied(color);
                                            }
                                            LayerKind::Stroke { color, size, .. } => {
                                                ui.color_edit_button_srgba_unmultiplied(color);
                                                ui.add(
                                                    egui::Slider::new(size, 1.0..=100.0)
                                                        .text("Brush size"),
                                                );
                                            }
                                        }
                                    }
                                });
                                if layer != original {
                                    let newly_enabled = !super::cutout_ui::settings(&original)
                                        .enabled
                                        && super::cutout_ui::settings(&layer).enabled;
                                    self.set_layer(layer);
                                    if newly_enabled {
                                        self.start_backing_match(ui.ctx(), id, false);
                                    }
                                }
                                ui.add_space(12.0);
                                ui.separator();
                            }
                        } else {
                            eyebrow(ui, "CHAPTER");
                            let mut name = self.story.chapters[self.chapter].name.clone();
                            if ui.text_edit_singleline(&mut name).changed() {
                                let command = Command::SetField {
                                    target: Target::Object(self.story.chapters[self.chapter].id),
                                    field: Field::Name,
                                    value: name.clone(),
                                };
                                self.commit(Some(command), |studio| {
                                    studio.story.chapters[studio.chapter].name = name
                                });
                            }
                            let ch = &self.story.chapters[self.chapter];
                            ui.label(
                                RichText::new(format!("{} × {} px", ch.width, ch.height))
                                    .size(13.0)
                                    .color(ACCENT),
                            );
                            let mut height = ch.height;
                            ui.horizontal(|ui| {
                                ui.label("Canvas height");
                                ui.add(
                                    egui::DragValue::new(&mut height)
                                        .range(800..=200000)
                                        .speed(100),
                                );
                            });
                            if height != self.story.chapters[self.chapter].height {
                                let command = Command::SetChapterHeight {
                                    chapter: self.story.chapters[self.chapter].id,
                                    height,
                                };
                                self.commit(Some(command), |studio| {
                                    studio.story.chapters[studio.chapter].height = height
                                });
                            }
                            if motion::button(ui, "Spacing…").clicked() {
                                self.begin_spacing(
                                    self.story.chapters[self.chapter].id,
                                    self.scroll_y,
                                );
                            }
                            ui.add_space(4.0);
                            ui.separator();
                        }
                        ui.add_space(6.0);
                        self.layer_list(ui);
                    });
            });
    }

    pub(super) fn dialogs(&mut self, ctx: &egui::Context) {
        if let Some(message) = self.error.clone() {
            egui::Window::new("Something needs attention")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.set_max_width(500.0);
                    ui.label(message);
                    if motion::button(ui, "Got it").clicked() {
                        self.error = None;
                    }
                });
        }
        let Some(dialog) = &self.dialog else {
            return;
        };
        let (title, mode, kind) = match dialog {
            Dialog::World => ("New world", 4, AssetKind::Image),
            Dialog::Chapter => ("New chapter", 1, AssetKind::Image),
            Dialog::Asset(kind) => (
                if *kind == AssetKind::Character {
                    "New character"
                } else {
                    "New background"
                },
                2,
                *kind,
            ),
            Dialog::Unsaved => ("Save your story?", 3, AssetKind::Image),
        };
        egui::Modal::new(egui::Id::new("studio_dialog")).show(ctx, |ui| {
            ui.set_width(420.0);
            ui.heading(title);
            ui.add_space(12.0);
            if mode == 3 {
                ui.label("This story has unsaved changes. Save them before continuing.");
                ui.add_space(16.0);
                ui.add_enabled_ui(self.job.is_none(), |ui| {
                    ui.horizontal(|ui| {
                        if primary(ui, "Save & continue").clicked() {
                            self.continue_after_save = self.save(false);
                        }
                        if motion::button(ui, "Discard").clicked() {
                            self.dialog = None;
                            self.perform_pending();
                        }
                        if motion::button(ui, "Cancel").clicked() {
                            self.dialog = None;
                            self.pending = None;
                        }
                    });
                });
                return;
            }
            ui.label("Name");
            ui.add(
                egui::TextEdit::singleline(&mut self.dialog_name)
                    .desired_width(f32::INFINITY)
                    .hint_text("Name"),
            );
            if mode == 1 {
                ui.add_space(12.0);
                ui.label("Canvas");
                for (i, label) in [
                    "WEBTOON 2×   ·   1600 × 9600",
                    "WEBTOON   ·   800 × 6400",
                    "Tapas   ·   940 × 8000",
                ]
                .iter()
                .enumerate()
                {
                    ui.radio_value(&mut self.preset, i, *label);
                }
            }
            ui.add_space(20.0);
            ui.horizontal(|ui| {
                let valid = !self.dialog_name.trim().is_empty();
                if ui
                    .add_enabled(
                        valid && self.job.is_none(),
                        motion::Button::new(RichText::new("Create").color(BG)).fill(ACCENT),
                    )
                    .clicked()
                {
                    let name = self.dialog_name.trim().to_owned();
                    let (width, height) = [(1600, 9600), (800, 6400), (940, 8000)][self.preset];
                    match mode {
                        4 => {
                            self.story = Story::new(name);
                            self.assets.clear();
                            self.path = None;
                            self.reset_view();
                            self.welcome = false;
                            self.changed();
                        }
                        1 => {
                            let command = Command::AddChapter {
                                name: name.clone(),
                                width,
                                height,
                                opening: Some(commands::PanelDraft {
                                    title: "The opening".into(),
                                    y: 120.0,
                                    height: 1400.0,
                                    script: String::new(),
                                    notes: String::new(),
                                    prompt: String::new(),
                                }),
                            };
                            self.commit(Some(command), |studio| {
                                let id = studio.story.alloc_id();
                                let mut ch = Chapter::new(id, name, width, height);
                                ch.panels.push(Panel {
                                    id: studio.story.alloc_id(),
                                    title: "The opening".into(),
                                    y: 120.0,
                                    height: 1400.0,
                                    script: String::new(),
                                    notes: String::new(),
                                    prompt: String::new(),
                                });
                                studio.story.chapters.push(ch);
                            });
                            self.chapter = self.story.chapters.len() - 1;
                            self.selected = None;
                            self.active_panel = None;
                            self.tab = LibraryTab::Chapters;
                            self.jump_y = Some(0.0);
                        }
                        _ => {
                            let background = kind == AssetKind::Background;
                            if background {
                                self.sanitize_location_selection();
                            }
                            let command = Command::AddAsset {
                                name: name.clone(),
                                kind,
                                location: self.selected_location.filter(|_| background),
                            };
                            let id = match self.apply_command(command) {
                                Ok(applied) => applied.created[0],
                                Err(_) => {
                                    // 0.13.2 behaviour for anything the command refuses.
                                    self.checkpoint();
                                    let id = self.story.alloc_id();
                                    self.story.assets.push(Asset {
                                        id,
                                        name,
                                        kind,
                                        notes: String::new(),
                                        prompt: String::new(),
                                        width: 0,
                                        height: 0,
                                        character: CharacterProfile::default(),
                                    });
                                    if background {
                                        self.assign_background_location(id, self.selected_location);
                                    }
                                    self.changed();
                                    id
                                }
                            };
                            self.tab = if kind == AssetKind::Character {
                                LibraryTab::Characters
                            } else {
                                LibraryTab::Backgrounds
                            };
                            if kind == AssetKind::Character {
                                self.selected_asset = None;
                                self.select_character(id);
                            } else {
                                self.selected_asset = Some(id);
                            }
                            self.selected = None;
                        }
                    }
                    self.dialog = None;
                }
                if motion::button(ui, "Cancel").clicked() {
                    self.dialog = None;
                }
            });
        });
    }
}

/// A quiet library row shared by the story, cast, presets, and layer lists.
pub(super) fn list_row(
    ui: &mut egui::Ui,
    label: &str,
    selected: bool,
    height: f32,
    icon: Option<icons::Icon>,
) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), height), egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            ui.is_enabled(),
            selected,
            label,
        )
    });
    if ui.is_rect_visible(rect) {
        let feedback = motion::feedback(ui, &response, selected);
        let painted = feedback.paint_rect(rect);
        let fill = motion::mix(
            Color32::TRANSPARENT,
            CARD,
            feedback.hover.max(feedback.selected),
        );
        ui.painter().rect_filled(painted, 7.0, fill);
        if selected {
            let stripe = egui::Rect::from_min_max(
                rect.left_top() + vec2(0.0, 6.0),
                rect.left_bottom() + vec2(2.0, -6.0),
            );
            let stripe =
                motion::animate_rect(ui.ctx(), ui.id().with("selection_indicator"), stripe);
            ui.painter().rect_filled(stripe, 1.0, ACCENT);
        }
        let mut text_x = rect.left() + 10.0;
        if let Some(icon) = icon {
            icons::draw(
                ui.painter(),
                egui::Rect::from_center_size(
                    egui::pos2(rect.left() + 21.0, rect.center().y),
                    vec2(18.0, 18.0),
                ),
                icon,
                motion::mix(MUTED, ACCENT, feedback.selected),
            );
            text_x += 27.0;
        }
        let color = motion::mix(MUTED, TEXT, feedback.hover.max(feedback.selected));
        let galley = egui::WidgetText::from(RichText::new(label).color(color)).into_galley(
            ui,
            Some(egui::TextWrapMode::Truncate),
            (rect.right() - text_x - 7.0).max(0.0),
            egui::TextStyle::Button,
        );
        ui.painter().galley(
            egui::pos2(text_x, rect.center().y - galley.size().y * 0.5),
            galley,
            color,
        );
        if response.has_focus() {
            ui.painter().rect_stroke(
                rect,
                3.0,
                egui::Stroke::new(1.0_f32, ACCENT),
                egui::StrokeKind::Inside,
            );
        }
    }
    response
}

/// Workspace tabs keep their shape steady; an underline marks the active pane.
pub(super) fn section_tab(ui: &mut egui::Ui, label: &str, selected: bool) -> egui::Response {
    let response = ui.add(
        motion::Button::new(RichText::new(label).color(if selected { TEXT } else { MUTED }))
            .frame(false)
            .min_size(vec2(0.0, 30.0)),
    );
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            ui.is_enabled(),
            selected,
            label,
        )
    });
    if selected {
        let target = egui::Rect::from_min_max(
            egui::pos2(response.rect.left(), response.rect.bottom() - 2.0),
            response.rect.right_bottom(),
        );
        let underline = motion::animate_rect(ui.ctx(), ui.id().with("section_underline"), target);
        ui.painter().rect_filled(underline, 1.0, ACCENT);
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    fn library_frame(
        ctx: &egui::Context,
        studio: &mut Studio,
        time: f64,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        let mut frame = eframe::Frame::_new_kittest();
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    vec2(1100.0, 720.0),
                )),
                time: Some(time),
                events,
                ..Default::default()
            },
            |ctx| eframe::App::update(studio, ctx, &mut frame),
        )
    }

    fn click_events(pos: egui::Pos2) -> Vec<egui::Event> {
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

    fn asset_library(ctx: &egui::Context, kind: AssetKind) -> (Studio, u64) {
        let mut studio = Studio::from_context(ctx);
        studio.story = Story::new("Asset cards");
        studio.story.chapters[0].layers.clear();
        studio.welcome = false;
        studio.error = None;
        studio.dirty = false;
        studio.preferences.motion = settings::Motion::Off;
        studio.tab = if kind == AssetKind::Background {
            LibraryTab::Backgrounds
        } else {
            LibraryTab::Images
        };
        let id = studio.story.alloc_id();
        studio.story.assets.push(Asset {
            id,
            name: "Reference artwork".into(),
            kind,
            notes: String::new(),
            prompt: String::new(),
            width: 32,
            height: 20,
            character: CharacterProfile::default(),
        });
        let image = image::RgbaImage::from_pixel(32, 20, image::Rgba([120, 180, 225, 255]));
        let mut bytes = std::io::Cursor::new(Vec::new());
        image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
        studio.assets.insert(id, Arc::new(bytes.into_inner()));
        (studio, id)
    }

    fn card_rect(ctx: &egui::Context, id: u64) -> egui::Rect {
        ctx.data(|data| {
            data.get_temp(egui::Id::new(("asset_card_bounds", id)))
                .unwrap()
        })
    }

    #[test]
    fn character_image_replacement_preserves_affiliation_and_undo() {
        for placeholder in [true, false] {
            let ctx = egui::Context::default();
            let (mut studio, id) = asset_library(&ctx, AssetKind::Character);
            scroll_studio::groups::add(&mut studio.story).unwrap();
            studio.story.groups[0].character_ids.push(id);
            if placeholder {
                studio.story.assets[0].width = 0;
                studio.story.assets[0].height = 0;
                studio.assets.remove(&id);
            }
            let original = studio.story.clone();
            let mut bytes = std::io::Cursor::new(Vec::new());
            image::RgbaImage::from_pixel(8, 4, image::Rgba([10, 20, 30, 255]))
                .write_to(&mut bytes, image::ImageFormat::Png)
                .unwrap();
            studio.replace_library_image(0, bytes.into_inner(), 8, 4);
            let new_id = studio.selected_asset.unwrap();
            // The edit went through the command, not the 0.13.2 fallback.
            let mut expected = original.clone();
            expected
                .apply(Command::AddImageVersion {
                    asset: id,
                    width: 8,
                    height: 4,
                })
                .unwrap();
            assert_eq!(studio.story, expected);
            assert!(studio.story.groups[0].character_ids.contains(&new_id));
            assert_eq!(
                studio.story.groups[0].character_ids.contains(&id),
                !placeholder
            );
            storage::validate_story(
                &studio.story,
                &assets_for_story(&studio.story, &studio.assets),
            )
            .unwrap();
            studio.undo();
            assert_eq!(studio.story, original);
            studio.redo();
            assert!(studio.story.groups[0].character_ids.contains(&new_id));
        }
    }

    #[test]
    fn background_replacements_and_versions_preserve_folder_membership_and_history() {
        for placeholder in [true, false] {
            let ctx = egui::Context::default();
            let (mut studio, id) = asset_library(&ctx, AssetKind::Background);
            let folder =
                scroll_studio::locations::add_location(&mut studio.story, "Station").unwrap();
            scroll_studio::locations::move_asset(&mut studio.story, id, Some(folder)).unwrap();
            if placeholder {
                studio.story.assets[0].width = 0;
                studio.story.assets[0].height = 0;
                studio.assets.remove(&id);
            }
            let original = studio.story.clone();
            let image = image::RgbaImage::from_pixel(8, 4, image::Rgba([10, 20, 30, 255]));
            let mut bytes = std::io::Cursor::new(Vec::new());
            image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
            studio.replace_library_image(0, bytes.into_inner(), 8, 4);
            let replacement = studio.selected_asset.unwrap();
            assert_ne!(replacement, id);
            assert_eq!(
                scroll_studio::locations::location_for_asset(&studio.story, replacement),
                Some(folder)
            );
            assert_eq!(
                studio.story.locations[0].asset_ids.contains(&id),
                !placeholder
            );
            storage::validate_story(
                &studio.story,
                &assets_for_story(&studio.story, &studio.assets),
            )
            .unwrap();
            assert_eq!(studio.undo.len(), 1);
            let edited = studio.story.clone();
            studio.undo();
            assert_eq!(studio.story, original);
            studio.redo();
            assert_eq!(studio.story, edited);
        }
    }

    fn has_selection_outline(shape: &egui::Shape, rect: egui::Rect) -> bool {
        match shape {
            egui::Shape::Rect(paint) => {
                paint.stroke.color == ACCENT
                    && paint.rect.expand(1.0).contains_rect(rect.shrink(1.0))
            }
            egui::Shape::Vec(shapes) => shapes
                .iter()
                .any(|shape| has_selection_outline(shape, rect)),
            _ => false,
        }
    }

    #[test]
    fn artwork_cards_select_from_thumbnail_and_padding_and_double_click_to_place() {
        for kind in [AssetKind::Image, AssetKind::Background] {
            let ctx = egui::Context::default();
            let (mut studio, id) = asset_library(&ctx, kind);
            let _ = library_frame(&ctx, &mut studio, 0.0, Vec::new());
            let _ = library_frame(&ctx, &mut studio, 0.1, Vec::new());
            let rect = card_rect(&ctx, id);
            let thumbnail = rect.center_top() + vec2(0.0, 35.0);
            let _ = library_frame(&ctx, &mut studio, 0.2, click_events(thumbnail));
            assert_eq!(studio.selected_asset, Some(id));
            assert!(studio.story.chapters[0].layers.is_empty());
            assert!(studio.undo.is_empty());
            assert!(!studio.dirty);
            let output = library_frame(&ctx, &mut studio, 0.25, Vec::new());
            assert!(output
                .shapes
                .iter()
                .any(|shape| has_selection_outline(&shape.shape, rect)));

            studio.selected_asset = None;
            let _ = library_frame(
                &ctx,
                &mut studio,
                1.0,
                click_events(rect.min + vec2(4.0, 4.0)),
            );
            assert_eq!(studio.selected_asset, Some(id));
            assert!(studio.story.chapters[0].layers.is_empty());
            let _ = library_frame(&ctx, &mut studio, 2.0, click_events(thumbnail));
            let _ = library_frame(&ctx, &mut studio, 2.1, click_events(thumbnail));
            assert_eq!(studio.story.chapters[0].layers.len(), 1);
            assert!(
                matches!(studio.story.chapters[0].layers[0].kind, LayerKind::Image { asset_id, .. } if asset_id == id)
            );
            assert_eq!(studio.undo.len(), 1);
        }
    }

    #[test]
    fn dragging_a_thumbnail_preserves_its_payload_and_places_exactly_one_layer() {
        let ctx = egui::Context::default();
        let (mut studio, id) = asset_library(&ctx, AssetKind::Image);
        let _ = library_frame(&ctx, &mut studio, 0.0, Vec::new());
        let _ = library_frame(&ctx, &mut studio, 0.1, Vec::new());
        let from = card_rect(&ctx, id).center_top() + vec2(0.0, 35.0);
        let _ = library_frame(
            &ctx,
            &mut studio,
            0.2,
            vec![
                egui::Event::PointerMoved(from),
                egui::Event::PointerButton {
                    pos: from,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        let _ = library_frame(
            &ctx,
            &mut studio,
            0.3,
            vec![egui::Event::PointerMoved(from + vec2(20.0, 0.0))],
        );
        let target = studio.canvas_rect.min + vec2(50.0, 80.0);
        let _ = library_frame(
            &ctx,
            &mut studio,
            0.4,
            vec![egui::Event::PointerMoved(target)],
        );
        assert_eq!(
            *egui::DragAndDrop::payload::<u64>(&ctx).expect("Native drag payload"),
            id
        );
        let _ = library_frame(
            &ctx,
            &mut studio,
            0.5,
            vec![egui::Event::PointerButton {
                pos: target,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert_eq!(studio.story.chapters[0].layers.len(), 1);
        assert_eq!(studio.undo.len(), 1);
        assert!(!egui::DragAndDrop::has_any_payload(&ctx));
        studio.undo();
        assert!(studio.story.chapters[0].layers.is_empty());
    }

    fn find_text(shape: &egui::Shape, expected: &str) -> Option<egui::Rect> {
        match shape {
            egui::Shape::Text(text) if text.galley.job.text == expected => {
                Some(text.visual_bounding_rect())
            }
            egui::Shape::Vec(shapes) => shapes.iter().find_map(|shape| find_text(shape, expected)),
            _ => None,
        }
    }

    fn property_fixture(ctx: &egui::Context) -> Studio {
        let (mut studio, _) = asset_library(ctx, AssetKind::Image);
        for y in [100.0, 300.0] {
            let id = studio.story.alloc_id();
            studio.story.chapters[0].layers.push(Layer {
                id,
                name: format!("Panel {id}"),
                visible: true,
                locked: false,
                opacity: 1.0,
                x: 100.0,
                y,
                width: 100.0,
                height: 100.0,
                kind: LayerKind::Shape {
                    color: [90, 100, 110, 255],
                },
            });
        }
        studio.selected = Some(studio.story.chapters[0].layers[0].id);
        studio.apply_preferences(ctx);
        let _ = library_frame(ctx, &mut studio, 0.0, Vec::new());
        let _ = library_frame(ctx, &mut studio, 0.1, Vec::new());
        studio
    }

    #[test]
    fn creating_a_character_discards_the_previous_characters_scene() {
        let ctx = egui::Context::default();
        let (mut studio, _) = asset_library(&ctx, AssetKind::Character);
        studio.tab = LibraryTab::Characters;
        let previous = studio.story.assets[0].id;
        studio.selected_asset = Some(previous);
        studio.character_prompt_kind = CharacterPromptKind::Scene;
        studio.character_scene = "Previous character's private pose".into();
        studio.dialog = Some(Dialog::Asset(AssetKind::Character));
        studio.dialog_name = "A new lead".into();
        let _ = library_frame(&ctx, &mut studio, 0.0, Vec::new());
        let output = library_frame(&ctx, &mut studio, 0.1, Vec::new());
        let create = output
            .shapes
            .iter()
            .find_map(|shape| find_text(&shape.shape, "Create"))
            .unwrap()
            .center();
        let _ = library_frame(&ctx, &mut studio, 0.2, click_events(create));
        assert!(studio.dialog.is_none());
        assert_ne!(studio.character_selected_id(), Some(previous));
        assert!(studio.character_scene.is_empty());
        assert_eq!(studio.story.assets.last().unwrap().name, "A new lead");
    }

    fn property_controls(ctx: &egui::Context) -> [egui::Rect; 3] {
        ctx.data(|data| {
            data.get_temp(egui::Id::new("inspector_layer_controls"))
                .unwrap()
        })
    }

    fn open_layer_actions(ctx: &egui::Context, studio: &mut Studio, time: f64) {
        let rect: egui::Rect = ctx.data(|data| {
            data.get_temp(egui::Id::new(("icon_button_bounds", "Layer actions")))
                .unwrap()
        });
        let _ = library_frame(ctx, studio, time, click_events(rect.center()));
        let _ = library_frame(ctx, studio, time + 0.05, Vec::new());
    }

    #[test]
    fn inspector_icons_toggle_visibility_and_lock_and_opacity_uses_percent() {
        let ctx = egui::Context::default();
        let mut studio = property_fixture(&ctx);
        let before = studio.story.clone();
        let controls = property_controls(&ctx);
        assert!(controls
            .windows(2)
            .all(|pair| pair[0].right() <= pair[1].left()));
        assert!(controls.iter().all(|rect| rect.right() <= 1100.0));
        let _ = library_frame(&ctx, &mut studio, 0.2, click_events(controls[1].center()));
        assert!(!studio.story.chapters[0].layers[0].visible);
        let _ = library_frame(
            &ctx,
            &mut studio,
            0.4,
            click_events(property_controls(&ctx)[2].center()),
        );
        assert!(studio.story.chapters[0].layers[0].locked);
        let (opacity, enabled, _): (egui::Rect, bool, f32) = ctx.data(|data| {
            data.get_temp(egui::Id::new("inspector_opacity_bounds"))
                .unwrap()
        });
        assert!(!enabled);
        let locked = studio.story.clone();
        let _ = library_frame(&ctx, &mut studio, 0.6, click_events(opacity.center()));
        assert_eq!(studio.story, locked);
        let _ = library_frame(
            &ctx,
            &mut studio,
            0.8,
            click_events(property_controls(&ctx)[2].center()),
        );
        let (opacity, enabled, width): (egui::Rect, bool, f32) = ctx.data(|data| {
            data.get_temp(egui::Id::new("inspector_opacity_bounds"))
                .unwrap()
        });
        assert!(enabled);
        let point = egui::pos2(opacity.left() + width * 0.4, opacity.center().y);
        let _ = library_frame(
            &ctx,
            &mut studio,
            1.0,
            vec![
                egui::Event::PointerMoved(point),
                egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        let output = library_frame(
            &ctx,
            &mut studio,
            1.1,
            vec![egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert!((studio.story.chapters[0].layers[0].opacity - 0.4).abs() < 0.1,
            "Opacity stayed at {} after clicking {point:?} within {opacity:?} (track width {width})",studio.story.chapters[0].layers[0].opacity);
        assert!(!output
            .shapes
            .iter()
            .any(|shape| find_text(&shape.shape, "Visible").is_some()
                || find_text(&shape.shape, "Locked").is_some()
                || find_text(&shape.shape, "1.000").is_some()));
        assert!(output.shapes.iter().any(|shape| match &shape.shape {
            egui::Shape::Text(text) => text.galley.job.text.contains('%'),
            _ => false,
        }));
        while !studio.undo.is_empty() {
            studio.undo();
        }
        assert_eq!(studio.story, before);
    }

    #[test]
    fn layer_actions_overflow_keeps_boundary_and_locked_states() {
        let ctx = egui::Context::default();
        let mut studio = property_fixture(&ctx);
        let original = studio.story.clone();
        open_layer_actions(&ctx, &mut studio, 0.2);
        let controls = |ctx: &egui::Context| {
            ctx.data(|data| {
                data.get_temp::<[(egui::Rect, bool); 2]>(egui::Id::new(
                    "layer_order_control_bounds",
                ))
                .unwrap()
            })
        };
        assert_eq!(controls(&ctx).map(|(_, enabled)| enabled), [true, false]);
        let _ = library_frame(
            &ctx,
            &mut studio,
            0.4,
            click_events(controls(&ctx)[1].0.center()),
        );
        assert_eq!(studio.story, original);
        let _ = library_frame(
            &ctx,
            &mut studio,
            0.6,
            click_events(controls(&ctx)[0].0.center()),
        );
        assert_eq!(studio.undo.len(), 1);
        open_layer_actions(&ctx, &mut studio, 0.8);
        assert_eq!(controls(&ctx).map(|(_, enabled)| enabled), [false, true]);
        let _ = library_frame(
            &ctx,
            &mut studio,
            1.0,
            click_events(egui::pos2(800.0, 680.0)),
        );
        studio.undo();
        assert_eq!(studio.story, original);
        let id = studio.story.chapters[0].layers[0].id;
        studio.selected = Some(id);
        studio.story.chapters[0].layers[0].locked = true;
        let _ = library_frame(&ctx, &mut studio, 1.2, Vec::new());
        open_layer_actions(&ctx, &mut studio, 1.4);
        let action = |name| {
            ctx.data(|data| {
                data.get_temp::<(egui::Rect, bool)>(egui::Id::new((
                    "layer_context_action_bounds",
                    name,
                )))
                .unwrap()
            })
        };
        assert!(!action("Delete").1);
        assert!(action("Duplicate").1);
    }

    #[test]
    fn layer_title_keeps_native_text_undo_and_layer_folder_controls_fit() {
        let ctx = egui::Context::default();
        let mut studio = property_fixture(&ctx);
        let original_name = studio.story.chapters[0].layers[0].name.clone();
        let name = property_controls(&ctx)[0];
        let _ = library_frame(&ctx, &mut studio, 0.2, click_events(name.center()));
        let keys = |key, modifiers| {
            [true, false]
                .into_iter()
                .map(|pressed| egui::Event::Key {
                    key,
                    physical_key: Some(key),
                    pressed,
                    repeat: false,
                    modifiers,
                })
                .collect::<Vec<_>>()
        };
        let _ = library_frame(
            &ctx,
            &mut studio,
            0.3,
            keys(egui::Key::A, egui::Modifiers::COMMAND),
        );
        let _ = library_frame(
            &ctx,
            &mut studio,
            0.4,
            vec![egui::Event::Text("Renamed panel".into())],
        );
        assert_eq!(studio.story.chapters[0].layers[0].name, "Renamed panel");
        let selected = studio.selected;
        let _ = library_frame(
            &ctx,
            &mut studio,
            0.5,
            keys(egui::Key::Z, egui::Modifiers::COMMAND),
        );
        assert_eq!(
            studio.selected, selected,
            "Text undo must not invoke whole-story undo"
        );
        assert_eq!(studio.story.chapters[0].layers[0].name, original_name);
        for label in [
            "Collapse layer folders",
            "Expand layer folders",
            "Layer actions",
        ] {
            let rect: egui::Rect = ctx.data(|data| {
                data.get_temp(egui::Id::new(("icon_button_bounds", label)))
                    .unwrap()
            });
            assert!(rect.left() >= 850.0 && rect.right() <= 1100.0);
        }
    }

    #[test]
    fn layer_row_eye_and_folder_icons_keep_selection_and_history_intact() {
        let ctx = egui::Context::default();
        let mut studio = property_fixture(&ctx);
        let selected = studio.selected;
        let id = studio.story.chapters[0].layers[1].id;
        let row_key = egui::Id::new(("layer_row_bounds", id));
        let (eye, _, _): (egui::Rect, egui::Rect, egui::Rect) =
            ctx.data(|data| data.get_temp(row_key).unwrap());
        let _ = library_frame(&ctx, &mut studio, 0.2, click_events(eye.center()));
        assert!(!studio.story.chapters[0].layers[1].visible);
        assert_eq!(studio.selected, selected);
        assert_eq!(studio.undo.len(), 1);
        let control = |label| {
            ctx.data(|data| {
                data.get_temp::<egui::Rect>(egui::Id::new(("icon_button_bounds", label)))
                    .unwrap()
            })
        };
        ctx.data_mut(|data| data.remove::<(egui::Rect, egui::Rect, egui::Rect)>(row_key));
        let _ = library_frame(
            &ctx,
            &mut studio,
            0.4,
            click_events(control("Collapse layer folders").center()),
        );
        assert!(ctx
            .data(|data| data.get_temp::<(egui::Rect, egui::Rect, egui::Rect)>(row_key))
            .is_none());
        let _ = library_frame(
            &ctx,
            &mut studio,
            0.6,
            click_events(control("Expand layer folders").center()),
        );
        assert!(ctx
            .data(|data| data.get_temp::<(egui::Rect, egui::Rect, egui::Rect)>(row_key))
            .is_some());
        assert_eq!(studio.undo.len(), 1, "Folder controls only change the view");
        assert_eq!(studio.selected, selected);
    }

    #[test]
    fn small_window_keeps_navigation_and_story_actions_visible() {
        let ctx = egui::Context::default();
        let mut studio = Studio::from_context(&ctx);
        studio.welcome = false;
        studio.story.title = "A long story title ".repeat(40);
        for index in 2..42 {
            studio
                .story
                .chapters
                .push(Chapter::new(index, format!("Chapter {index}"), 800, 6400));
        }
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1024.0, 700.0));
        // Floating areas measure once before painting their final controls.
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |ctx| {
                studio.top_bar(ctx);
                studio.status_bar(ctx);
                studio.library(ctx);
                studio.inspector(ctx);
                studio.workspace(ctx);
            },
        );
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |ctx| {
                studio.top_bar(ctx);
                studio.status_bar(ctx);
                studio.library(ctx);
                studio.inspector(ctx);
                studio.workspace(ctx);
            },
        );
        let mut controls = Vec::new();
        for label in [
            "File",
            "+ New",
            "Style",
            "Copy style",
            "Save",
            "Export",
            "Chapters",
            "Bubbles",
            "Characters",
            "Locations",
            "Images",
            "Settings",
            "Fit",
        ] {
            let (bounds, clip) = output
                .shapes
                .iter()
                .find_map(|shape| {
                    find_text(&shape.shape, label).map(|bounds| (bounds, shape.clip_rect))
                })
                .unwrap_or_else(|| panic!("Missing control: {label}"));
            assert!(
                screen.contains_rect(bounds),
                "{label} outside screen: {bounds:?}"
            );
            assert!(
                clip.expand(1.0).contains_rect(bounds),
                "{label} clipped: {bounds:?} / {clip:?}"
            );
            controls.push((label, bounds));
        }
        for label in ["Move", "Lasso", "Brush", "Bubble", "Beat", "Undo", "Redo"] {
            let bounds = ctx
                .data_mut(|data| {
                    data.get_temp::<egui::Rect>(egui::Id::new(("icon_button_bounds", label)))
                })
                .unwrap_or_else(|| panic!("Missing tool: {label}"));
            assert!(
                screen.contains_rect(bounds),
                "{label} outside screen: {bounds:?}"
            );
            controls.push((label, bounds));
        }
        for (index, (label, bounds)) in controls.iter().enumerate() {
            for (other, other_bounds) in &controls[index + 1..] {
                assert!(
                    !bounds.intersects(*other_bounds),
                    "{label} overlaps {other}"
                );
            }
        }
    }
}
