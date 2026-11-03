use super::*;
use scroll_studio::characters;

/// Empty workspaces keep their real controls compact at every window size.
/// Center the composition, rather than justifying a button across the page.
fn empty_sheet_action(ui: &mut egui::Ui, cast: bool) -> egui::Response {
    let available = ui.available_rect_before_wrap();
    let size = egui::vec2(260.0, 228.0).min(available.size());
    let content = egui::Rect::from_center_size(available.center(), size);
    let response = ui
        .scope_builder(
            egui::UiBuilder::new()
                .max_rect(content)
                .layout(egui::Layout::top_down(egui::Align::Center)),
            |ui| {
                let (art, _) =
                    ui.allocate_exact_size(egui::vec2(160.0, 120.0), egui::Sense::hover());
                let painter = ui.painter();
                let card_size = if cast {
                    egui::vec2(78.0, 96.0)
                } else {
                    egui::vec2(134.0, 92.0)
                };
                let card =
                    egui::Rect::from_center_size(art.center() + egui::vec2(5.0, 4.0), card_size);
                let line = egui::Stroke::new(1.0_f32, Color32::from_rgb(72, 77, 85));
                painter.rect(
                    card.translate(egui::vec2(-10.0, -8.0)),
                    10.0,
                    PANEL,
                    line,
                    egui::StrokeKind::Inside,
                );
                painter.rect(
                    card,
                    10.0,
                    Color32::from_rgb(43, 47, 54),
                    line,
                    egui::StrokeKind::Inside,
                );
                icons::draw(
                    painter,
                    egui::Rect::from_center_size(card.center(), egui::vec2(40.0, 40.0)),
                    if cast {
                        icons::Icon::Characters
                    } else {
                        icons::Icon::Image
                    },
                    Color32::from_rgb(144, 164, 186),
                );
                ui.add_space(10.0);
                ui.label(
                    RichText::new(if cast { "Characters" } else { "References" })
                        .size(18.0)
                        .color(TEXT),
                );
                ui.add_space(12.0);
                let label = if cast { "+ Character" } else { "+ Add image" };
                let button =
                    motion::Button::new(RichText::new(label).color(if cast { BG } else { TEXT }));
                ui.add_sized(
                    [148.0, 36.0],
                    if cast { button.fill(ACCENT) } else { button },
                )
            },
        )
        .inner;
    #[cfg(test)]
    ui.ctx().data_mut(|data| {
        data.insert_temp(egui::Id::new(("empty_sheet_action", cast)), response.rect)
    });
    response
}

enum ReferenceAction {
    Add,
    CopySheet,
    ExportSheet,
    Place(u64),
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum SheetTab {
    #[default]
    References,
    Appearance,
    Outfits,
    Palette,
    Identity,
    Notes,
    Prompt,
}

impl SheetTab {
    const ALL: [(Self, &'static str); 7] = [
        (Self::References, "References"),
        (Self::Appearance, "Appearance"),
        (Self::Outfits, "Outfits"),
        (Self::Palette, "Palette"),
        (Self::Identity, "Identity"),
        (Self::Notes, "Notes"),
        (Self::Prompt, "Prompt"),
    ];
}

#[derive(Clone, Default)]
struct SheetView {
    tab: SheetTab,
    appearance: usize,
    identity: usize,
    notes: usize,
    reference: usize,
    reference_id: Option<u64>,
    reference_details: bool,
    reference_focus: bool,
    outfit: usize,
    swatch: usize,
}

struct CharacterCard {
    id: u64,
    name: String,
    subtitle: String,
    portrait: Option<(u64, egui::Vec2)>,
    palette: Vec<[u8; 4]>,
}

const GALLERY_CARD_HEIGHT: f32 = 292.0;
const GALLERY_GAP: f32 = 16.0;

impl Studio {
    pub(super) fn select_character(&mut self, id: u64) {
        if self.character_selected_id() != Some(id) {
            self.character_scene.clear();
        }
        self.selected_asset = Some(id);
        self.selected = None;
    }

    pub(super) fn character_selected_id(&self) -> Option<u64> {
        self.story
            .assets
            .iter()
            .find(|asset| {
                asset.kind == AssetKind::Character && Some(asset.id) == self.selected_asset
            })
            .map(|asset| asset.id)
    }

    pub(super) fn character_list(&mut self, ui: &mut egui::Ui) {
        // The gallery owns its search and create action. The sidebar becomes
        // a quick cast switcher only while a particular sheet is open.
        if self.character_selected_id().is_none() {
            return;
        }
        let back = motion::button(ui, "‹ All characters");
        #[cfg(test)]
        ui.ctx()
            .data_mut(|data| data.insert_temp(egui::Id::new("character_gallery_back"), back.rect));
        if back.clicked() {
            self.selected_asset = None;
            return;
        }
        ui.add_space(8.0);
        if primary(ui, "+ Character").clicked() {
            self.dialog_name.clear();
            self.dialog = Some(Dialog::Asset(AssetKind::Character));
        }
        ui.add(
            egui::TextEdit::singleline(&mut self.search)
                .hint_text("Search")
                .desired_width(f32::INFINITY),
        );
        let selected = self.character_selected_id();
        let query = self.search.to_lowercase();
        let entries: Vec<_> = self
            .story
            .assets
            .iter()
            .filter(|asset| asset.kind == AssetKind::Character)
            .filter(|asset| {
                format!(
                    "{} {} {}",
                    asset.name, asset.character.tagline, asset.character.role
                )
                .to_lowercase()
                .contains(&query)
            })
            .map(|asset| (asset.id, asset.name.clone()))
            .collect();
        if entries.is_empty() {
            ui.label(
                RichText::new(if query.is_empty() {
                    "No characters"
                } else {
                    "No results"
                })
                .color(MUTED),
            );
            return;
        }
        let page_id = ui.id().with(("cast_page", self.document_epoch, &query));
        let mut page = ui
            .ctx()
            .data_mut(|data| data.get_temp::<usize>(page_id).unwrap_or(0));
        let capacity = ((ui.available_height() - 40.0) / 42.0).floor().max(1.0) as usize;
        let pages = entries.len().div_ceil(capacity);
        page = page.min(pages - 1);
        let list_height = (ui.available_height() - if pages > 1 { 38.0 } else { 0.0 }).max(0.0);
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), list_height),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                for (id, name) in entries.iter().skip(page * capacity).take(capacity) {
                    if panels::list_row(
                        ui,
                        if name.trim().is_empty() {
                            "Unnamed"
                        } else {
                            name
                        },
                        selected == Some(*id),
                        34.0,
                        None,
                    )
                    .clicked()
                    {
                        self.select_character(*id);
                    }
                }
            },
        );
        if pages > 1 {
            pager(ui, &mut page, pages);
        }
        ui.ctx().data_mut(|data| data.insert_temp(page_id, page));
    }

    fn character_gallery(&mut self, ui: &mut egui::Ui) {
        let count = self
            .story
            .assets
            .iter()
            .filter(|asset| asset.kind == AssetKind::Character)
            .count();
        if count == 0 {
            if empty_sheet_action(ui, true).clicked() {
                self.dialog_name.clear();
                self.dialog = Some(Dialog::Asset(AssetKind::Character));
            }
            return;
        }
        ui.horizontal(|ui| {
            ui.heading("Characters");
            ui.label(RichText::new(count.to_string()).color(MUTED));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if primary(ui, "+ Character").clicked() {
                    self.dialog_name.clear();
                    self.dialog = Some(Dialog::Asset(AssetKind::Character));
                }
                let response = ui.add_sized(
                    [ui.available_width().min(240.0), 32.0],
                    egui::TextEdit::singleline(&mut self.search).hint_text("Search characters"),
                );
                #[cfg(test)]
                ui.ctx().data_mut(|data| {
                    data.insert_temp(egui::Id::new("character_gallery_search"), response.rect)
                });
                #[cfg(not(test))]
                let _ = response;
            });
        });
        ui.add_space(16.0);
        let query = self.search.trim().to_lowercase();
        let indices: Vec<_> = self
            .story
            .assets
            .iter()
            .enumerate()
            .filter(|(_, asset)| asset.kind == AssetKind::Character)
            .filter(|(_, asset)| {
                query.is_empty()
                    || [&asset.name, &asset.character.role, &asset.character.tagline]
                        .iter()
                        .any(|text| text.to_lowercase().contains(&query))
            })
            .map(|(index, _)| index)
            .collect();
        if indices.is_empty() {
            ui.add_space(24.0);
            ui.label(RichText::new("No matches").color(MUTED));
            return;
        }
        let width = (ui.available_width() - 12.0).max(1.0);
        let columns = ((width + GALLERY_GAP) / (220.0 + GALLERY_GAP))
            .floor()
            .max(1.0) as usize;
        let card_width = ((width - GALLERY_GAP * (columns - 1) as f32) / columns as f32).min(280.0);
        let rows = indices.len().div_ceil(columns);
        let scroll_id = egui::Id::new(("character_gallery_scroll", self.document_epoch, &query));
        #[cfg(test)]
        ui.ctx().data_mut(|data| {
            data.insert_temp(
                egui::Id::new("character_gallery_cards"),
                Vec::<(u64, egui::Rect)>::new(),
            );
            data.insert_temp(
                egui::Id::new("character_gallery_thumbnail_requests"),
                Vec::<u64>::new(),
            );
            data.insert_temp(egui::Id::new("character_gallery_scroll_id"), scroll_id);
        });
        let mut selected = None;
        ui.spacing_mut().item_spacing = egui::vec2(GALLERY_GAP, GALLERY_GAP);
        let _output = egui::ScrollArea::vertical()
            .id_salt(scroll_id)
            .auto_shrink([false, false])
            .show_rows(ui, GALLERY_CARD_HEIGHT, rows, |ui, row_range| {
                for row in row_range {
                    ui.horizontal_top(|ui| {
                        for column in 0..columns {
                            let Some(index) = indices.get(row * columns + column).copied() else {
                                break;
                            };
                            let asset = &self.story.assets[index];
                            let portrait_id =
                                scroll_studio::thumbnails::image(&self.story, asset.id);
                            let portrait = portrait_id
                                .and_then(|id| {
                                    self.story.assets.iter().find(|asset| asset.id == id)
                                })
                                .filter(|asset| asset.width > 0 && asset.height > 0)
                                .map(|asset| {
                                    (
                                        asset.id,
                                        egui::vec2(asset.width as f32, asset.height as f32),
                                    )
                                });
                            let card = CharacterCard {
                                id: asset.id,
                                name: if asset.name.trim().is_empty() {
                                    "Unnamed".into()
                                } else {
                                    asset.name.clone()
                                },
                                subtitle: if asset.character.role.trim().is_empty() {
                                    asset.character.tagline.clone()
                                } else {
                                    asset.character.role.clone()
                                },
                                portrait,
                                palette: asset
                                    .character
                                    .palette
                                    .iter()
                                    .take(5)
                                    .map(|swatch| swatch.color)
                                    .collect(),
                            };
                            if ui
                                .push_id(card.id, |ui| {
                                    self.character_gallery_card(ui, &card, card_width)
                                })
                                .inner
                                .clicked()
                            {
                                selected = Some(card.id);
                            }
                        }
                    });
                }
            });
        #[cfg(test)]
        ui.ctx().data_mut(|data| {
            data.insert_temp(
                egui::Id::new("character_gallery_scroll_state"),
                (_output.id, _output.state.offset.y),
            )
        });
        if let Some(id) = selected {
            self.select_character(id);
        }
    }

    fn character_gallery_card(
        &mut self,
        ui: &mut egui::Ui,
        card: &CharacterCard,
        width: f32,
    ) -> egui::Response {
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(width, GALLERY_CARD_HEIGHT), egui::Sense::click());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), &card.name)
        });
        if !ui.is_rect_visible(rect) {
            return response;
        }
        #[cfg(test)]
        ui.ctx().data_mut(|data| {
            let id = egui::Id::new("character_gallery_cards");
            let cards = data.get_temp_mut_or_default::<Vec<(u64, egui::Rect)>>(id);
            cards.push((card.id, rect));
            data.insert_temp(
                egui::Id::new(("character_gallery_focus", card.id)),
                response.id,
            );
        });
        let feedback = motion::feedback(ui, &response, false);
        let painted = feedback.paint_rect(rect);
        let painter = ui.painter();
        let fill = motion::mix(PANEL, CARD, feedback.hover * 0.75);
        let border = if response.has_focus() {
            ACCENT
        } else {
            motion::mix(Color32::from_rgb(53, 57, 64), ACCENT, feedback.hover * 0.45)
        };
        painter.rect(
            painted,
            12.0,
            fill,
            egui::Stroke::new(1.0_f32, border),
            egui::StrokeKind::Inside,
        );
        let photo = egui::Rect::from_min_size(
            painted.min + egui::vec2(10.0, 10.0),
            egui::vec2(painted.width() - 20.0, 208.0),
        );
        let tint = card.palette.first().map_or(ACCENT, |color| {
            Color32::from_rgba_unmultiplied(color[0], color[1], color[2], color[3])
        });
        painter.rect_filled(photo, 8.0, motion::mix(BG, tint, 0.10));
        let texture = card.portrait.and_then(|(id, size)| {
            #[cfg(test)]
            ui.ctx().data_mut(|data| {
                data.get_temp_mut_or_default::<Vec<u64>>(egui::Id::new(
                    "character_gallery_thumbnail_requests",
                ))
                .push(id)
            });
            self.asset_thumbnail(ui.ctx(), id)
                .map(|texture| (texture, size))
        });
        if let Some((texture, size)) = texture {
            let uv = portrait_uv(size, photo.size());
            egui::Image::new((texture.id(), photo.size()))
                .uv(uv)
                .corner_radius(8.0)
                .paint_at(ui, photo);
        } else {
            icons::draw(
                ui.painter(),
                egui::Rect::from_center_size(photo.center(), egui::vec2(58.0, 58.0)),
                icons::Icon::Characters,
                Color32::from_rgb(111, 126, 146),
            );
        }
        let title =
            egui::WidgetText::from(RichText::new(&card.name).size(15.0).strong().color(TEXT))
                .into_galley(
                    ui,
                    Some(egui::TextWrapMode::Truncate),
                    painted.width() - 28.0,
                    egui::TextStyle::Body,
                );
        ui.painter().galley(
            egui::pos2(painted.left() + 14.0, painted.top() + 231.0),
            title,
            TEXT,
        );
        let dots_width = if card.palette.is_empty() {
            0.0
        } else {
            card.palette.len() as f32 * 13.0 + 8.0
        };
        let subtitle =
            egui::WidgetText::from(RichText::new(&card.subtitle).size(12.0).color(MUTED))
                .into_galley(
                    ui,
                    Some(egui::TextWrapMode::Truncate),
                    (painted.width() - 28.0 - dots_width).max(1.0),
                    egui::TextStyle::Body,
                );
        ui.painter().galley(
            egui::pos2(painted.left() + 14.0, painted.top() + 257.0),
            subtitle,
            MUTED,
        );
        for (index, color) in card.palette.iter().enumerate() {
            ui.painter().circle_filled(
                egui::pos2(
                    painted.right() - 18.0 - index as f32 * 13.0,
                    painted.top() + 264.0,
                ),
                4.0,
                Color32::from_rgba_unmultiplied(color[0], color[1], color[2], color[3]),
            );
        }
        self.register_context(&response);
        appearance::context_menu(&response, |ui| self.character_context_actions(ui, card.id));
        response
    }

    pub(super) fn character_workspace(&mut self, ctx: &egui::Context) {
        self.canvas_rect = egui::Rect::NOTHING;
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG).inner_margin(16.0))
            .show(ctx, |ui| {
                let Some(id) = self.character_selected_id() else {
                    self.character_gallery(ui);
                    return;
                };
                let Some(index) = self.story.assets.iter().position(|asset| asset.id == id) else {
                    return;
                };
                let original = self.story.assets[index].clone();
                let mut character = original.clone();
                let state_id = egui::Id::new(("character_sheet_view", self.document_epoch, id));
                let mut view =
                    ctx.data_mut(|data| data.get_temp::<SheetView>(state_id).unwrap_or_default());
                let mut action = None;
                let mut copy_prompt = false;
                let mut history = 0;
                ui.push_id((self.document_epoch, id), |ui| {
                    let _header = ui
                        .horizontal(|ui| {
                            let name_width = (ui.available_width() - 370.0).max(120.0);
                            ui.add(
                                egui::TextEdit::singleline(&mut character.name)
                                    .font(egui::FontId::proportional(23.0))
                                    .frame(false)
                                    .hint_text("Character name")
                                    .char_limit(1024)
                                    .desired_width(name_width),
                            );
                            let previous_kind = self.character_prompt_kind;
                            egui::ComboBox::from_id_salt("quick_prompt_kind")
                                .selected_text(self.character_prompt_kind.label())
                                .width(104.0)
                                .show_ui(ui, |ui| {
                                    for kind in CharacterPromptKind::ALL {
                                        ui.selectable_value(
                                            &mut self.character_prompt_kind,
                                            kind,
                                            kind.label(),
                                        );
                                    }
                                });
                            if previous_kind != self.character_prompt_kind
                                && self.character_prompt_kind == CharacterPromptKind::Scene
                            {
                                view.tab = SheetTab::Prompt;
                            }
                            if motion::button(ui, "Angle").clicked() {
                                self.open_camera(ctx, id);
                            }
                            if motion::copy_button(
                                ui,
                                ("copy_prompt", self.document_epoch, id),
                                "Copy prompt",
                                true,
                            )
                            .clicked()
                            {
                                copy_prompt = true;
                            }
                            motion::menu_button(ui, "•••", |ui| {
                                if ui
                                    .add_enabled(!self.undo.is_empty(), motion::Button::new("Undo"))
                                    .clicked()
                                {
                                    history = -1;
                                    ui.close_menu();
                                }
                                if ui
                                    .add_enabled(!self.redo.is_empty(), motion::Button::new("Redo"))
                                    .clicked()
                                {
                                    history = 1;
                                    ui.close_menu();
                                }
                                ui.separator();
                                if motion::button(ui, "Story style").clicked() {
                                    self.style_prompt_open = true;
                                    ui.close_menu();
                                }
                            });
                        })
                        .response
                        .rect;
                    ui.add(
                        egui::TextEdit::singleline(&mut character.character.tagline)
                            .hint_text("Concept")
                            .frame(false)
                            .char_limit(20000)
                            .desired_width(f32::INFINITY),
                    );
                    self.affiliation_picker(
                        ui,
                        scroll_studio::groups::Subject::Character(character.id),
                    );
                    ui.add_space(5.0);
                    let _tabs = ui
                        .horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 18.0;
                            for (tab, label) in SheetTab::ALL {
                                if panels::section_tab(ui, label, view.tab == tab).clicked() {
                                    view.tab = tab;
                                }
                            }
                        })
                        .response
                        .rect;
                    ui.add_space(5.0);
                    let body = ui.available_rect_before_wrap();
                    let _pane = ui
                        .scope_builder(
                            egui::UiBuilder::new()
                                .max_rect(body)
                                .layout(egui::Layout::top_down(egui::Align::Min)),
                            |ui| {
                                ui.set_clip_rect(body.intersect(ui.clip_rect()));
                                let opacity = motion::reveal(
                                    ctx,
                                    ui.id().with("character_pane_reveal"),
                                    egui::Id::new((character.id, view.tab as u8)),
                                );
                                ui.set_opacity(opacity);
                                egui::Frame::new()
                                    .fill(PANEL)
                                    .corner_radius(4.0)
                                    .inner_margin(16.0)
                                    .show(ui, |ui| {
                                        ui.set_min_size(
                                            (body.size() - egui::vec2(32.0, 32.0))
                                                .max(egui::vec2(0.0, 0.0)),
                                        );
                                        match view.tab {
                                            SheetTab::References => self.character_references(
                                                ui,
                                                ctx,
                                                &mut character,
                                                &mut view,
                                                &mut action,
                                            ),
                                            SheetTab::Appearance => field_editor(
                                                ui,
                                                "appearance",
                                                &mut view.appearance,
                                                &mut [
                                                    ("Face", &mut character.character.face),
                                                    ("Hair", &mut character.character.hair),
                                                    ("Eyes", &mut character.character.eyes),
                                                    ("Skin", &mut character.character.skin),
                                                    ("Build", &mut character.character.build),
                                                    (
                                                        "Signature details",
                                                        &mut character
                                                            .character
                                                            .distinguishing_features,
                                                    ),
                                                ],
                                            ),
                                            SheetTab::Outfits => outfit_editor(
                                                ui,
                                                &mut character.character,
                                                &mut view.outfit,
                                            ),
                                            SheetTab::Palette => palette_editor(
                                                ui,
                                                &mut character.character,
                                                &mut view.swatch,
                                            ),
                                            SheetTab::Identity => identity_editor(
                                                ui,
                                                &mut character.character,
                                                &mut view.identity,
                                            ),
                                            SheetTab::Notes => field_editor(
                                                ui,
                                                "notes",
                                                &mut view.notes,
                                                &mut [
                                                    (
                                                        "Keep consistent",
                                                        &mut character.character.consistency,
                                                    ),
                                                    ("Avoid", &mut character.character.avoid),
                                                    ("Continuity", &mut character.notes),
                                                    ("Prompt instructions", &mut character.prompt),
                                                ],
                                            ),
                                            SheetTab::Prompt => {
                                                self.character_prompt(ui, &mut character)
                                            }
                                        }
                                    });
                            },
                        )
                        .response
                        .rect;
                    #[cfg(test)]
                    ctx.data_mut(|data| {
                        data.insert_temp(
                            state_id.with("layout_bounds"),
                            [_header, _tabs, body, _pane],
                        )
                    });
                });
                ctx.data_mut(|data| data.insert_temp(state_id, view));
                if character != original {
                    self.checkpoint();
                    self.story.assets[index] = character.clone();
                    self.changed();
                }
                if copy_prompt {
                    ctx.copy_text(characters::compose_prompt(
                        &self.story,
                        &character,
                        self.character_prompt_kind,
                        &self.character_scene,
                    ));
                    self.status = "Prompt copied".into();
                }
                match action {
                    Some(ReferenceAction::Add) => {
                        let previous_count = characters::reference_ids(&character).len();
                        self.character_reference_picker(id);
                        let references = self
                            .story
                            .assets
                            .iter()
                            .find(|asset| asset.id == id)
                            .map(characters::reference_ids)
                            .unwrap_or_default();
                        let count = references.len();
                        if count > previous_count {
                            ctx.data_mut(|data| {
                                let mut view =
                                    data.get_temp::<SheetView>(state_id).unwrap_or_default();
                                view.reference = count - 1;
                                view.reference_id = references.last().copied();
                                data.insert_temp(state_id, view);
                            });
                        }
                    }
                    Some(ReferenceAction::CopySheet) => self.copy_character_sheet(id),
                    Some(ReferenceAction::ExportSheet) => self.export_character_sheet(id),
                    Some(ReferenceAction::Place(reference)) => {
                        self.place_character_reference(id, reference)
                    }
                    None => {}
                }
                if history < 0 {
                    self.undo();
                } else if history > 0 {
                    self.redo();
                }
            });
    }

    fn character_references(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        character: &mut Asset,
        view: &mut SheetView,
        action: &mut Option<ReferenceAction>,
    ) {
        let character_id = character.id;
        let references = characters::reference_ids(character);
        if references.is_empty() {
            view.reference = 0;
            view.reference_id = None;
            if empty_sheet_action(ui, false).clicked() {
                *action = Some(ReferenceAction::Add);
            }
            return;
        }
        // Membership may change through a context menu or undo while this view
        // is loaded. Keep the same image when an earlier thumbnail disappears.
        view.reference = view
            .reference_id
            .and_then(|id| references.iter().position(|reference| *reference == id))
            .unwrap_or(view.reference)
            .min(references.len() - 1);
        let reference_id = references[view.reference];
        let entry_index = character
            .character
            .references
            .iter()
            .position(|reference| reference.asset_id == reference_id);
        let multiple = references.len() > 1;
        let mut remove = None;
        ui.horizontal(|ui| {
            if multiple
                && (view.reference_focus || view.reference_details)
                && motion::button(ui, "Board").clicked()
            {
                view.reference_focus = false;
                view.reference_details = false;
            }
            let add = ui.add_enabled(
                character.character.references.len() < 24,
                motion::Button::new("+ Reference"),
            );
            #[cfg(test)]
            reference_control(ctx, "Add", &add);
            if add.clicked() {
                *action = Some(ReferenceAction::Add);
            }
            let copy = ui.add_enabled(self.job.is_none(), motion::Button::new("Copy sheet"));
            #[cfg(test)]
            reference_control(ctx, "Copy", &copy);
            if copy.clicked() {
                *action = Some(ReferenceAction::CopySheet);
            }
            if !self.assets_only
                && motion::button(ui, "Place")
                    .on_hover_text("Place in chapter")
                    .clicked()
            {
                *action = Some(ReferenceAction::Place(reference_id));
            }
            if ui
                .add_enabled(
                    self.can_pin_reference(reference_id),
                    motion::Button::new("Pin"),
                )
                .on_hover_text("Keep this reference beside the canvas")
                .clicked()
            {
                self.pin_reference(ctx, reference_id, Some(character_id));
            }
            if entry_index.is_some() {
                let details =
                    ui.add(motion::Button::new("Details").selected(view.reference_details));
                #[cfg(test)]
                reference_control(ctx, "Details", &details);
                if details.clicked() {
                    view.reference_details = !view.reference_details;
                    view.reference_focus = true;
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let _menu = motion::menu_button(ui, "•••", |ui| {
                    let save = ui.add_enabled(self.job.is_none(), motion::Button::new("Save PNG"));
                    #[cfg(test)]
                    reference_control(ctx, "Save", &save);
                    if save.clicked() {
                        *action = Some(ReferenceAction::ExportSheet);
                        ui.close_menu();
                    }
                    if let Some(index) = entry_index {
                        ui.separator();
                        if motion::button(ui, "Remove reference").clicked() {
                            remove = Some(index);
                            ui.close_menu();
                        }
                    }
                });
                #[cfg(test)]
                reference_control(ctx, "More", &_menu.response);
                if multiple && (view.reference_focus || view.reference_details) {
                    pager(ui, &mut view.reference, references.len());
                }
            });
        });
        if multiple && !view.reference_focus && !view.reference_details {
            self.reference_board(ui, ctx, character_id, &references, view);
            if let Some(index) = remove {
                character.character.references.remove(index);
            }
            view.reference_id = references.get(view.reference).copied();
            return;
        }
        let reference_id = references[view.reference];
        let entry = character
            .character
            .references
            .iter_mut()
            .find(|reference| reference.asset_id == reference_id);
        let show_details = view.reference_details && entry.is_some();
        let main_height = (ui.available_height()
            - if multiple {
                68.0 + ui.spacing().item_spacing.y
            } else {
                0.0
            })
        .max(0.0);
        let side_width = if show_details {
            206.0_f32.min(ui.available_width() * 0.35)
        } else {
            0.0
        };
        let image_width = (ui.available_width()
            - side_width
            - if show_details {
                ui.spacing().item_spacing.x
            } else {
                0.0
            })
        .max(0.0);
        ui.horizontal_top(|ui| {
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(image_width, main_height), egui::Sense::hover());
            #[cfg(test)]
            ctx.data_mut(|data| data.insert_temp(egui::Id::new("character_reference_image"), rect));
            self.paint_character_reference(ui, ctx, reference_id, rect, false);
            self.register_context(&response);
            appearance::context_menu(&response, |ui| {
                self.reference_context_actions(ui, character_id, reference_id)
            });
            if let Some(reference) = entry.filter(|_| show_details) {
                ui.allocate_ui_with_layout(
                    egui::vec2(side_width, main_height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_max_width(side_width);
                        ui.add(
                            egui::TextEdit::singleline(&mut reference.label)
                                .hint_text("Label")
                                .char_limit(1024)
                                .desired_width(f32::INFINITY),
                        );
                        text_editor(
                            ui,
                            ("reference_notes", reference_id),
                            &mut reference.notes,
                            "Notes",
                        );
                    },
                );
            }
        });
        if multiple {
            let capacity = ((ui.available_width() + ui.spacing().item_spacing.x)
                / (68.0 + ui.spacing().item_spacing.x))
                .floor()
                .max(1.0) as usize;
            let start = view.reference / capacity * capacity;
            ui.horizontal(|ui| {
                for (index, &reference_id) in
                    references.iter().enumerate().skip(start).take(capacity)
                {
                    let (rect, response) =
                        ui.allocate_exact_size(egui::vec2(68.0, 68.0), egui::Sense::click());
                    #[cfg(test)]
                    reference_control(ctx, &format!("Thumbnail {index}"), &response);
                    self.paint_character_reference(ui, ctx, reference_id, rect, true);
                    if index == view.reference {
                        ui.painter().rect_stroke(
                            rect,
                            6.0,
                            egui::Stroke::new(2.0_f32, ACCENT),
                            egui::StrokeKind::Inside,
                        );
                    }
                    if response.clicked() {
                        view.reference = index;
                    }
                    self.register_context(&response);
                    appearance::context_menu(&response, |ui| {
                        self.reference_context_actions(ui, character_id, reference_id)
                    });
                }
            });
        }
        if let Some(index) = remove {
            character.character.references.remove(index);
        }
        // Record after paging/thumbnail input. If this image was removed, the
        // next frame falls back to its former slot (or the preceding last one).
        view.reference_id = references.get(view.reference).copied();
    }

    fn reference_board(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        character_id: u64,
        references: &[u64],
        view: &mut SheetView,
    ) {
        let sizes: Vec<_> = references
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
            .id_salt(("reference_board", character_id))
            .auto_shrink([false, false])
            .show_viewport(ui, |ui, viewport| {
                let tiles = masonry_rects(&sizes, ui.available_width());
                let origin = ui.cursor().min;
                let height = tiles.iter().map(|r| r.bottom()).fold(0.0_f32, f32::max);
                ui.set_min_height(height);
                for (index, (&id, tile)) in references.iter().zip(&tiles).enumerate() {
                    if !tile.intersects(viewport) {
                        continue;
                    }
                    let rect = tile.translate(origin.to_vec2());
                    let response = ui.interact(rect, ui.id().with(id), egui::Sense::click());
                    #[cfg(test)]
                    reference_control(ctx, &format!("Board {index}"), &response);
                    self.paint_character_reference(ui, ctx, id, rect, false);
                    if response.hovered() || index == view.reference {
                        ui.painter().rect_stroke(
                            rect,
                            8.0,
                            egui::Stroke::new(
                                if index == view.reference {
                                    2.0_f32
                                } else {
                                    1.0_f32
                                },
                                if index == view.reference {
                                    ACCENT
                                } else {
                                    MUTED
                                },
                            ),
                            egui::StrokeKind::Inside,
                        );
                    }
                    if response.clicked() {
                        view.reference = index;
                        view.reference_id = Some(id);
                        view.reference_focus = true;
                    }
                    self.register_context(&response);
                    appearance::context_menu(&response, |ui| {
                        self.reference_context_actions(ui, character_id, id)
                    });
                }
            });
    }

    pub(super) fn paint_character_reference(
        &mut self,
        ui: &egui::Ui,
        ctx: &egui::Context,
        id: u64,
        rect: egui::Rect,
        thumbnail: bool,
    ) {
        ui.painter().rect_filled(rect, 3.0, BG);
        let Some(asset) = self
            .story
            .assets
            .iter()
            .find(|asset| asset.id == id && asset.width > 0 && asset.height > 0)
        else {
            return;
        };
        let source_size = [asset.width, asset.height];
        let size = egui::vec2(asset.width as f32, asset.height as f32);
        let bounds = rect.shrink(6.0);
        let scale = (bounds.width() / size.x).min(bounds.height() / size.y);
        let image_rect = egui::Rect::from_center_size(rect.center(), size * scale);
        let painter = ui
            .painter()
            .with_clip_rect(ui.clip_rect().intersect(bounds));
        if thumbnail {
            if let Some(texture) = self.asset_thumbnail(ctx, id) {
                super::artwork_color::transparency_grid(&painter, image_rect);
                painter.image(
                    texture.id(),
                    image_rect,
                    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
                return;
            }
        } else if let Some(image) = self.assets.get(&id).and_then(|source| {
            self.previews.viewer_image(
                ctx,
                id,
                source,
                super::previews::ImageViewport {
                    source_size,
                    image: image_rect,
                    clip: painter.clip_rect(),
                    mirrored: false,
                },
            )
        }) {
            image.paint(&painter);
            return;
        }
        painter.circle_stroke(rect.center(), 8.0, egui::Stroke::new(1.5_f32, MUTED));
    }

    fn character_prompt(&mut self, ui: &mut egui::Ui, character: &mut Asset) {
        ui.horizontal(|ui| {
            ui.label("Outfit");
            egui::ComboBox::from_id_salt("prompt_outfit")
                .width(170.0)
                .selected_text(
                    character
                        .character
                        .active_outfit
                        .and_then(|index| character.character.outfits.get(index))
                        .map_or("None", |outfit| outfit.name.as_str()),
                )
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut character.character.active_outfit, None, "None");
                    for (index, outfit) in character.character.outfits.iter().enumerate() {
                        ui.selectable_value(
                            &mut character.character.active_outfit,
                            Some(index),
                            &outfit.name,
                        );
                    }
                });
            if motion::button(ui, "Story style").clicked() {
                self.style_prompt_open = true;
            }
        });
        let preview = characters::compose_prompt(
            &self.story,
            character,
            self.character_prompt_kind,
            &self.character_scene,
        );
        if self.character_prompt_kind == CharacterPromptKind::Scene {
            ui.columns(2, |columns| {
                columns[0].label("Scene");
                text_editor(
                    &mut columns[0],
                    "character_scene",
                    &mut self.character_scene,
                    "Scene or pose",
                );
                columns[1].label("Preview");
                readonly_editor(&mut columns[1], "scene_preview", &preview);
            });
        } else {
            readonly_editor(ui, "character_preview", &preview);
        }
    }
}

#[cfg(test)]
fn reference_control(ctx: &egui::Context, label: &str, response: &egui::Response) {
    ctx.data_mut(|data| {
        data.insert_temp(
            egui::Id::new(("character_reference_control", label)),
            (response.rect, response.enabled()),
        )
    });
}

fn pager(ui: &mut egui::Ui, current: &mut usize, count: usize) {
    *current = (*current).min(count.saturating_sub(1));
    let label = format!("{} / {count}", *current + 1);
    let label_width = ui.fonts(|fonts| {
        fonts
            .layout_no_wrap(
                label.clone(),
                egui::TextStyle::Body.resolve(ui.style()),
                MUTED,
            )
            .size()
            .x
    });
    ui.allocate_ui_with_layout(
        egui::vec2(label_width + 76.0, 26.0),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            if ui
                .add_enabled(*current > 0, motion::Button::new("‹").small())
                .clicked()
            {
                *current -= 1;
            }
            ui.label(RichText::new(label).color(MUTED));
            if ui
                .add_enabled(*current + 1 < count, motion::Button::new("›").small())
                .clicked()
            {
                *current += 1;
            }
        },
    );
}

fn identity_editor(ui: &mut egui::Ui, profile: &mut CharacterProfile, selected: &mut usize) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Gender").color(MUTED));
        ui.add_space(8.0);
        ui.spacing_mut().item_spacing.x = 4.0;
        for gender in [CharacterGender::Female, CharacterGender::Male] {
            let response = ui.add_sized(
                [88.0, 32.0],
                motion::Button::new(gender.label()).selected(profile.gender == Some(gender)),
            );
            #[cfg(test)]
            ui.ctx().data_mut(|data| {
                data.insert_temp(
                    egui::Id::new(("character_gender", gender.label())),
                    response.rect,
                )
            });
            if response.clicked() {
                profile.gender = Some(gender);
            }
        }
    });
    ui.add_space(12.0);
    field_editor(
        ui,
        "identity",
        selected,
        &mut [
            ("Role", &mut profile.role),
            ("Age", &mut profile.age),
            ("Personality", &mut profile.personality),
            ("Backstory", &mut profile.backstory),
            ("Relationships", &mut profile.relationships),
        ],
    );
}

fn field_editor(
    ui: &mut egui::Ui,
    id: &str,
    selected: &mut usize,
    fields: &mut [(&str, &mut String)],
) {
    *selected = (*selected).min(fields.len() - 1);
    let height = ui.available_height();
    let editor_width = (ui.available_width() - 172.0).max(100.0);
    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(156.0, height),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_max_width(156.0);
                for (index, (label, value)) in fields.iter().enumerate() {
                    if panels::list_row(
                        ui,
                        &if value.trim().is_empty() {
                            (*label).to_owned()
                        } else {
                            format!("{label}  ·")
                        },
                        *selected == index,
                        34.0,
                        None,
                    )
                    .clicked()
                    {
                        *selected = index;
                    }
                }
            },
        );
        ui.allocate_ui_with_layout(
            egui::vec2(editor_width, height),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                let (label, value) = &mut fields[*selected];
                ui.label(RichText::new(*label).strong());
                text_editor(ui, (id, *selected), value, "");
            },
        );
    });
}

fn outfit_editor(ui: &mut egui::Ui, profile: &mut CharacterProfile, selected: &mut usize) {
    ui.horizontal(|ui| {
        if ui
            .add_enabled(profile.outfits.len() < 32, motion::Button::new("+ Outfit"))
            .clicked()
        {
            profile.outfits.push(CharacterOutfit {
                name: format!("Look {}", profile.outfits.len() + 1),
                description: String::new(),
            });
            *selected = profile.outfits.len() - 1;
            if profile.active_outfit.is_none() {
                profile.active_outfit = Some(*selected);
            }
        }
        if !profile.outfits.is_empty() {
            pager(ui, selected, profile.outfits.len());
        }
    });
    if profile.outfits.is_empty() {
        return;
    }
    let mut remove = false;
    ui.horizontal(|ui| {
        let width = (ui.available_width() - 210.0).max(100.0);
        ui.add(
            egui::TextEdit::singleline(&mut profile.outfits[*selected].name)
                .hint_text("Outfit name")
                .char_limit(1024)
                .desired_width(width),
        );
        let mut active = profile.active_outfit == Some(*selected);
        if ui.toggle_value(&mut active, "Use in prompt").changed() {
            profile.active_outfit = if active { Some(*selected) } else { None };
        }
        motion::menu_button(ui, "•••", |ui| {
            if motion::button(ui, "Delete outfit").clicked() {
                remove = true;
                ui.close_menu();
            }
        });
    });
    text_editor(
        ui,
        ("outfit_description", *selected),
        &mut profile.outfits[*selected].description,
        "Clothing, colors, accessories",
    );
    if remove {
        remove_outfit(profile, *selected);
        *selected = (*selected).min(profile.outfits.len().saturating_sub(1));
    }
}

fn palette_editor(ui: &mut egui::Ui, profile: &mut CharacterProfile, selected: &mut usize) {
    if ui
        .add_enabled(profile.palette.len() < 16, motion::Button::new("+ Color"))
        .clicked()
    {
        profile.palette.push(CharacterSwatch {
            name: String::new(),
            color: [130, 155, 145, 255],
        });
        *selected = profile.palette.len() - 1;
    }
    if profile.palette.is_empty() {
        return;
    }
    *selected = (*selected).min(profile.palette.len() - 1);
    ui.horizontal_wrapped(|ui| {
        for (index, swatch) in profile.palette.iter().enumerate() {
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(40.0, 40.0), egui::Sense::click());
            ui.painter().rect_filled(
                rect.shrink(3.0),
                6.0,
                Color32::from_rgba_unmultiplied(
                    swatch.color[0],
                    swatch.color[1],
                    swatch.color[2],
                    swatch.color[3],
                ),
            );
            if *selected == index {
                ui.painter().rect_stroke(
                    rect,
                    8.0,
                    egui::Stroke::new(1.5_f32, ACCENT),
                    egui::StrokeKind::Inside,
                );
            }
            if response.on_hover_text(&swatch.name).clicked() {
                *selected = index;
            }
        }
    });
    ui.add_space(12.0);
    let swatch = &mut profile.palette[*selected];
    let mut remove = false;
    ui.horizontal(|ui| {
        ui.color_edit_button_srgba_unmultiplied(&mut swatch.color);
        ui.label(
            RichText::new(format!(
                "#{:02X}{:02X}{:02X}",
                swatch.color[0], swatch.color[1], swatch.color[2]
            ))
            .color(MUTED),
        );
        motion::menu_button(ui, "•••", |ui| {
            if motion::button(ui, "Delete color").clicked() {
                remove = true;
                ui.close_menu();
            }
        });
    });
    if remove {
        profile.palette.remove(*selected);
        *selected = (*selected).min(profile.palette.len().saturating_sub(1));
    }
}

fn portrait_uv(image: egui::Vec2, viewport: egui::Vec2) -> egui::Rect {
    let ratio = image.x / image.y;
    let target = viewport.x / viewport.y;
    let visible = if ratio > target {
        egui::vec2(target / ratio, 1.0)
    } else {
        egui::vec2(1.0, ratio / target)
    };
    egui::Rect::from_center_size(
        egui::pos2(
            0.5,
            if ratio < target {
                visible.y * 0.5 + (1.0 - visible.y) * 0.12
            } else {
                0.5
            },
        ),
        visible,
    )
}

/// Only the contents of a text field scroll. Sheet navigation and actions stay put.
fn text_editor(ui: &mut egui::Ui, id: impl std::hash::Hash, value: &mut String, hint: &str) {
    let size = ui.available_size().max(egui::vec2(1.0, 1.0));
    egui::ScrollArea::vertical()
        .id_salt(id)
        .max_height(size.y)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add(
                egui::TextEdit::multiline(value)
                    .hint_text(hint)
                    .char_limit(20000)
                    .desired_width(f32::INFINITY)
                    .min_size(egui::vec2(0.0, (size.y - 8.0).max(1.0)))
                    .desired_rows(1),
            );
        });
}

fn readonly_editor(ui: &mut egui::Ui, id: impl std::hash::Hash, value: &str) {
    let size = ui.available_size().max(egui::vec2(1.0, 1.0));
    let mut value = value;
    egui::ScrollArea::vertical()
        .id_salt(id)
        .max_height(size.y)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add(
                egui::TextEdit::multiline(&mut value)
                    .desired_width(f32::INFINITY)
                    .frame(false)
                    .min_size(egui::vec2(0.0, (size.y - 8.0).max(1.0)))
                    .desired_rows(1),
            );
        });
}

fn remove_outfit(profile: &mut CharacterProfile, index: usize) {
    if index >= profile.outfits.len() {
        return;
    }
    profile.outfits.remove(index);
    profile.active_outfit = match profile.active_outfit {
        Some(active) if active == index => None,
        Some(active) if active > index => Some(active - 1),
        other => other,
    };
}

pub(super) fn masonry_rects(sizes: &[[u32; 2]], width: f32) -> Vec<egui::Rect> {
    let gap = 10.0;
    let columns = ((width + gap) / 220.0).floor().clamp(1.0, 5.0) as usize;
    let cell = ((width - gap * (columns - 1) as f32) / columns as f32).max(1.0);
    let mut skyline = vec![0.0_f32; columns];
    sizes
        .iter()
        .map(|size| {
            let aspect = size[0].max(1) as f32 / size[1].max(1) as f32;
            let span = if aspect >= 1.65 && columns >= 3 { 2 } else { 1 };
            let column = (0..=columns - span)
                .min_by(|a, b| {
                    let a = skyline[*a..*a + span].iter().copied().fold(0.0, f32::max);
                    let b = skyline[*b..*b + span].iter().copied().fold(0.0, f32::max);
                    a.total_cmp(&b)
                })
                .unwrap_or(0);
            let top = skyline[column..column + span]
                .iter()
                .copied()
                .fold(0.0, f32::max);
            let w = cell * span as f32 + gap * (span - 1) as f32;
            let h = (w / aspect).clamp(72.0, 640.0);
            let rect = egui::Rect::from_min_size(
                egui::pos2(column as f32 * (cell + gap), top),
                egui::vec2(w, h),
            );
            skyline[column..column + span].fill(rect.bottom() + gap);
            rect
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removing_an_outfit_keeps_the_intended_active_look() {
        let mut profile = CharacterProfile {
            outfits: ["School", "Travel", "Formal"]
                .into_iter()
                .map(|name| CharacterOutfit {
                    name: name.into(),
                    description: String::new(),
                })
                .collect(),
            active_outfit: Some(2),
            ..Default::default()
        };
        remove_outfit(&mut profile, 0);
        assert_eq!(profile.active_outfit, Some(1));
        assert_eq!(profile.outfits[1].name, "Formal");
        remove_outfit(&mut profile, 1);
        assert_eq!(profile.active_outfit, None);
        assert_eq!(profile.outfits[0].name, "Travel");
    }

    #[test]
    fn every_sheet_section_fits_without_page_scrolling_even_with_long_content() {
        for viewport in [egui::vec2(1024.0, 700.0), egui::vec2(1366.0, 768.0)] {
            let ctx = egui::Context::default();
            let mut studio = Studio::from_context(&ctx);
            studio.story = Story::demo();
            studio.welcome = false;
            studio.tab = LibraryTab::Characters;
            studio.selected_asset = studio
                .story
                .assets
                .iter()
                .find(|asset| asset.kind == AssetKind::Character)
                .map(|asset| asset.id);
            let id = studio.character_selected_id().unwrap();
            let index = studio
                .story
                .assets
                .iter()
                .position(|asset| asset.id == id)
                .unwrap();
            let long = "A detailed visual note that wraps across several lines.\n".repeat(160);
            let profile = &mut studio.story.assets[index].character;
            profile.face = long.clone();
            profile.backstory = long.clone();
            profile.consistency = long.clone();
            profile.outfits = (0..32)
                .map(|index| CharacterOutfit {
                    name: format!("Look {index}"),
                    description: long.clone(),
                })
                .collect();
            profile.palette = (0..16)
                .map(|index| CharacterSwatch {
                    name: format!("Color {index}"),
                    color: [70, 110, 150, 255],
                })
                .collect();
            profile.references = (0..24)
                .map(|index| CharacterReference {
                    asset_id: 10000 + index,
                    label: format!("Reference {index}"),
                    notes: long.clone(),
                })
                .collect();
            for reference in profile.references.clone() {
                studio.story.assets.push(Asset {
                    id: reference.asset_id,
                    name: reference.label,
                    kind: AssetKind::Image,
                    notes: String::new(),
                    prompt: String::new(),
                    width: 16,
                    height: 16,
                    character: CharacterProfile::default(),
                });
            }
            studio.character_scene = long;
            studio.character_prompt_kind = CharacterPromptKind::Scene;
            let before = studio.story.clone();
            let state_id = egui::Id::new(("character_sheet_view", studio.document_epoch, id));
            for (tab, label) in SheetTab::ALL {
                ctx.data_mut(|data| {
                    data.insert_temp(
                        state_id,
                        SheetView {
                            tab,
                            identity: 3,
                            reference: 23,
                            reference_details: true,
                            outfit: 31,
                            swatch: 15,
                            ..Default::default()
                        },
                    )
                });
                for _ in 0..2 {
                    let input = egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, viewport)),
                        ..Default::default()
                    };
                    let _ = ctx.run(input, |ctx| {
                        studio.top_bar(ctx);
                        studio.status_bar(ctx);
                        studio.library(ctx);
                        studio.character_workspace(ctx);
                    });
                }
                let bounds = ctx.data_mut(|data| {
                    data.get_temp::<[egui::Rect; 4]>(state_id.with("layout_bounds"))
                        .unwrap()
                });
                let [header, tabs, available, used] = bounds;
                assert!(
                    header.max.x <= viewport.x + 1.0,
                    "{label}: header exceeds viewport: {header:?}"
                );
                assert!(
                    tabs.max.x <= viewport.x + 1.0,
                    "{label}: tabs exceed viewport: {tabs:?}"
                );
                assert!(
                    used.max.x <= available.max.x + 1.0 && used.max.y <= available.max.y + 1.0,
                    "{label} at {viewport:?}: content {used:?} exceeds available {available:?}"
                );
                assert_eq!(
                    studio.story, before,
                    "Viewing {label} must preserve the sheet"
                );
            }
        }
    }

    fn empty_sheet_studio(ctx: &egui::Context, cast: bool) -> Studio {
        let mut studio = Studio::from_context(ctx);
        studio.story = Story::new("Empty-state layout");
        studio.welcome = false;
        studio.error = None;
        studio.tab = LibraryTab::Characters;
        studio.dirty = false;
        if !cast {
            let id = studio.story.alloc_id();
            studio.story.assets.push(Asset {
                id,
                name: "Mira".into(),
                kind: AssetKind::Character,
                notes: String::new(),
                prompt: String::new(),
                width: 0,
                height: 0,
                character: CharacterProfile::default(),
            });
            studio.selected_asset = Some(id);
        }
        studio
    }

    fn empty_sheet_input(
        size: egui::Vec2,
        density: f32,
        events: Vec<egui::Event>,
    ) -> egui::RawInput {
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
        input
    }

    fn empty_sheet_frame(
        ctx: &egui::Context,
        studio: &mut Studio,
        input: egui::RawInput,
    ) -> egui::FullOutput {
        ctx.run(input, |ctx| {
            studio.top_bar(ctx);
            studio.status_bar(ctx);
            studio.library(ctx);
            studio.character_workspace(ctx);
        })
    }

    fn empty_sheet_click(pos: egui::Pos2) -> Vec<egui::Event> {
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

    fn painted_rectangles(shape: &egui::Shape, result: &mut Vec<(egui::Rect, Color32)>) {
        match shape {
            egui::Shape::Rect(rect) => result.push((rect.rect, rect.fill)),
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    painted_rectangles(shape, result);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn empty_cast_and_references_keep_paint_and_hit_targets_compact_at_desktop_sizes() {
        for (size, density) in [
            (egui::vec2(1100.0, 720.0), 1.0),
            (egui::vec2(1280.0, 760.0), 1.0),
            (egui::vec2(2560.0 / 1.5, 1440.0 / 1.5), 1.5),
        ] {
            for cast in [true, false] {
                let ctx = egui::Context::default();
                let mut studio = empty_sheet_studio(&ctx, cast);
                let before = studio.story.clone();
                let _ =
                    empty_sheet_frame(&ctx, &mut studio, empty_sheet_input(size, density, vec![]));
                let output =
                    empty_sheet_frame(&ctx, &mut studio, empty_sheet_input(size, density, vec![]));
                let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
                let action = ctx
                    .data(|data| {
                        data.get_temp::<egui::Rect>(egui::Id::new(("empty_sheet_action", cast)))
                    })
                    .unwrap();
                assert!(screen.contains_rect(action));
                assert!(
                    action.width() <= 200.0 && action.height() <= 44.0,
                    "Oversized empty action at {size:?}/{density}: {action:?}"
                );
                assert!(action.width() >= 100.0 && action.height() >= 32.0);
                assert!((ctx.pixels_per_point() - density).abs() < 0.01);
                let mut rectangles = Vec::new();
                for shape in &output.shapes {
                    painted_rectangles(&shape.shape, &mut rectangles);
                }
                assert!(
                    rectangles
                        .iter()
                        .any(|(rect, fill)| rect.center().distance(action.center()) < 0.1
                            && rect.size() == action.size()
                            && fill.a() > 0),
                    "Empty action must paint its bounded hit rectangle"
                );
                for (rect, fill) in &rectangles {
                    if *fill == ACCENT {
                        assert!(
                            rect.area() <= 200.0 * 44.0,
                            "Accent fill floods workspace: {rect:?}"
                        );
                    }
                }
                // A blank part of the page must never behave like the creation button.
                let outside = screen.right_bottom() - egui::vec2(32.0, 44.0);
                assert!(!action.contains(outside));
                let _ = empty_sheet_frame(
                    &ctx,
                    &mut studio,
                    empty_sheet_input(size, density, empty_sheet_click(outside)),
                );
                assert!(studio.dialog.is_none());
                assert_eq!(studio.story, before);
                assert!(!studio.dirty);
            }
        }
    }

    #[test]
    fn empty_cast_action_opens_creation_only_when_its_button_is_clicked() {
        let ctx = egui::Context::default();
        let mut studio = empty_sheet_studio(&ctx, true);
        let size = egui::vec2(1100.0, 720.0);
        let _ = empty_sheet_frame(&ctx, &mut studio, empty_sheet_input(size, 1.0, vec![]));
        let action = ctx
            .data(|data| data.get_temp::<egui::Rect>(egui::Id::new(("empty_sheet_action", true))))
            .unwrap();
        let _ = empty_sheet_frame(
            &ctx,
            &mut studio,
            empty_sheet_input(size, 1.0, empty_sheet_click(action.center())),
        );
        assert!(matches!(
            studio.dialog,
            Some(Dialog::Asset(AssetKind::Character))
        ));
        assert!(
            studio.story.assets.is_empty(),
            "Opening the name dialog must not create an asset yet"
        );
        assert!(!studio.dirty);
    }

    #[test]
    fn empty_reference_action_click_is_confined_to_button_without_opening_a_picker() {
        let ctx = egui::Context::default();
        let size = egui::vec2(1100.0, 720.0);
        let render = |events| {
            let mut response = None;
            let _ = ctx.run(empty_sheet_input(size, 1.0, events), |ctx| {
                egui::CentralPanel::default()
                    .show(ctx, |ui| response = Some(empty_sheet_action(ui, false)));
            });
            response.unwrap()
        };
        let original = render(vec![]).rect;
        assert!(!render(empty_sheet_click(egui::pos2(1040.0, 660.0))).clicked());
        let clicked = render(empty_sheet_click(original.center()));
        assert!(clicked.clicked());
        assert_eq!(clicked.rect, original);
    }

    fn gallery_studio(ctx: &egui::Context, count: usize) -> Studio {
        let mut studio = empty_sheet_studio(ctx, true);
        for index in 0..count {
            let id = studio.story.alloc_id();
            studio.story.assets.push(Asset {
                id,
                name: format!("Cast {index:04}"),
                kind: AssetKind::Character,
                notes: String::new(),
                prompt: String::new(),
                width: 0,
                height: 0,
                character: CharacterProfile {
                    role: if index % 2 == 0 {
                        "Guide".into()
                    } else {
                        "Rival".into()
                    },
                    ..Default::default()
                },
            });
        }
        studio.undo.push_back(Story::new("Before viewing"));
        studio.redo.push(Story::new("After viewing"));
        studio
    }

    fn gallery_cards(ctx: &egui::Context) -> Vec<(u64, egui::Rect)> {
        ctx.data(|data| data.get_temp(egui::Id::new("character_gallery_cards")))
            .unwrap_or_default()
    }

    fn gallery_scroll(ctx: &egui::Context) -> (egui::Id, f32) {
        ctx.data(|data| data.get_temp(egui::Id::new("character_gallery_scroll_state")))
            .unwrap()
    }

    #[test]
    fn character_gallery_uses_multiple_bounded_cards_and_explicit_sheet_selection() {
        for (size, density) in [
            (egui::vec2(1100.0, 720.0), 1.0),
            (egui::vec2(1280.0, 760.0), 1.0),
            (egui::vec2(2560.0 / 1.5, 1440.0 / 1.5), 1.5),
        ] {
            let ctx = egui::Context::default();
            let mut studio = gallery_studio(&ctx, 24);
            assert_eq!(studio.character_selected_id(), None);
            for _ in 0..2 {
                let _ =
                    empty_sheet_frame(&ctx, &mut studio, empty_sheet_input(size, density, vec![]));
            }
            let cards = gallery_cards(&ctx);
            assert!(cards.len() >= 3 && cards.len() < 24);
            let first_row = cards
                .iter()
                .filter(|(_, rect)| (rect.top() - cards[0].1.top()).abs() < 1.0)
                .collect::<Vec<_>>();
            assert!(
                first_row.len() >= 3,
                "Gallery only has one large card at {size:?}"
            );
            for (_, rect) in first_row {
                assert!((220.0..=280.0).contains(&rect.width()));
                assert_eq!(rect.height(), GALLERY_CARD_HEIGHT);
                assert!(rect.left() >= 200.0 && rect.right() <= size.x);
            }
            assert!(ctx
                .data(|data| data.get_temp::<egui::Rect>(egui::Id::new("character_gallery_back")))
                .is_none());
        }
    }

    #[test]
    fn gallery_search_filters_cards_from_real_text_input_and_keyboard_opens_a_card() {
        let ctx = egui::Context::default();
        let mut studio = gallery_studio(&ctx, 12);
        let size = egui::vec2(1100.0, 720.0);
        let before = studio.story.clone();
        let _ = empty_sheet_frame(&ctx, &mut studio, empty_sheet_input(size, 1.0, vec![]));
        let search = ctx
            .data(|data| data.get_temp::<egui::Rect>(egui::Id::new("character_gallery_search")))
            .unwrap();
        let _ = empty_sheet_frame(
            &ctx,
            &mut studio,
            empty_sheet_input(size, 1.0, empty_sheet_click(search.center())),
        );
        let _ = empty_sheet_frame(
            &ctx,
            &mut studio,
            empty_sheet_input(size, 1.0, vec![egui::Event::Text("Guide".into())]),
        );
        let _ = empty_sheet_frame(&ctx, &mut studio, empty_sheet_input(size, 1.0, vec![]));
        assert_eq!(studio.search, "Guide");
        let cards = gallery_cards(&ctx);
        assert!(!cards.is_empty());
        assert!(cards.iter().all(|(id, _)| studio
            .story
            .assets
            .iter()
            .find(|asset| asset.id == *id)
            .unwrap()
            .character
            .role
            == "Guide"));
        let id = cards[0].0;
        let focus = ctx
            .data(|data| data.get_temp::<egui::Id>(egui::Id::new(("character_gallery_focus", id))))
            .unwrap();
        ctx.memory_mut(|memory| memory.request_focus(focus));
        let events = [true, false]
            .into_iter()
            .map(|pressed| egui::Event::Key {
                key: egui::Key::Space,
                physical_key: Some(egui::Key::Space),
                pressed,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            })
            .collect();
        let _ = empty_sheet_frame(&ctx, &mut studio, empty_sheet_input(size, 1.0, events));
        assert_eq!(studio.selected_asset, Some(id));
        assert_eq!(studio.story, before);
        assert!(!studio.dirty);
    }

    #[test]
    fn gallery_scroll_and_sheet_back_preserve_query_sheet_category_and_history() {
        let ctx = egui::Context::default();
        let mut studio = gallery_studio(&ctx, 100);
        let size = egui::vec2(1100.0, 720.0);
        studio.search = "Cast".into();
        let before = studio.story.clone();
        let before_undo = studio.undo.clone();
        let before_redo = studio.redo.clone();
        let _ = empty_sheet_frame(&ctx, &mut studio, empty_sheet_input(size, 1.0, vec![]));
        let first = gallery_cards(&ctx)[0].0;
        let _ = empty_sheet_frame(
            &ctx,
            &mut studio,
            empty_sheet_input(
                size,
                1.0,
                vec![
                    egui::Event::PointerMoved(egui::pos2(650.0, 480.0)),
                    egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: egui::vec2(0.0, -1100.0),
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            ),
        );
        for _ in 0..40 {
            let _ = empty_sheet_frame(&ctx, &mut studio, empty_sheet_input(size, 1.0, vec![]));
        }
        let offset = gallery_scroll(&ctx).1;
        assert!(offset > 500.0, "Gallery did not scroll: {offset}");
        let cards = gallery_cards(&ctx);
        let (id, rect) = cards
            .iter()
            .find(|(_, rect)| rect.top() > 150.0 && rect.bottom() < size.y - 22.0)
            .copied()
            .unwrap();
        assert_ne!(id, first);
        let state_id = egui::Id::new(("character_sheet_view", studio.document_epoch, id));
        ctx.data_mut(|data| {
            data.insert_temp(
                state_id,
                SheetView {
                    tab: SheetTab::Appearance,
                    ..Default::default()
                },
            )
        });
        let _ = empty_sheet_frame(
            &ctx,
            &mut studio,
            empty_sheet_input(size, 1.0, empty_sheet_click(rect.center())),
        );
        assert_eq!(studio.selected_asset, Some(id));
        let _ = empty_sheet_frame(&ctx, &mut studio, empty_sheet_input(size, 1.0, vec![]));
        let back = ctx
            .data(|data| data.get_temp::<egui::Rect>(egui::Id::new("character_gallery_back")))
            .unwrap();
        let _ = empty_sheet_frame(
            &ctx,
            &mut studio,
            empty_sheet_input(size, 1.0, empty_sheet_click(back.center())),
        );
        assert_eq!(studio.selected_asset, None);
        assert_eq!(studio.search, "Cast");
        assert!((gallery_scroll(&ctx).1 - offset).abs() < 1.0);
        assert!(
            ctx.data(|data| data.get_temp::<SheetView>(state_id))
                .unwrap()
                .tab
                == SheetTab::Appearance
        );
        assert_eq!(studio.story, before);
        assert_eq!(studio.undo, before_undo);
        assert_eq!(studio.redo, before_redo);
        assert!(!studio.dirty);
    }

    #[test]
    fn large_gallery_virtualizes_rows_and_uses_metadata_characters_reference_portraits() {
        let ctx = egui::Context::default();
        let mut studio = gallery_studio(&ctx, 1000);
        let portrait_id = studio.story.alloc_id();
        studio.story.assets.push(Asset {
            id: portrait_id,
            name: "Shared reference".into(),
            kind: AssetKind::Image,
            notes: String::new(),
            prompt: String::new(),
            width: 8,
            height: 8,
            character: CharacterProfile::default(),
        });
        for asset in studio
            .story
            .assets
            .iter_mut()
            .filter(|asset| asset.kind == AssetKind::Character)
        {
            asset.character.references.push(CharacterReference {
                asset_id: portrait_id,
                label: String::new(),
                notes: String::new(),
            });
        }
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::RgbaImage::from_pixel(8, 8, image::Rgba([70, 120, 170, 255]))
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        studio
            .assets
            .insert(portrait_id, Arc::new(bytes.into_inner()));
        let size = egui::vec2(1100.0, 720.0);
        for _ in 0..2 {
            let _ = empty_sheet_frame(&ctx, &mut studio, empty_sheet_input(size, 1.0, vec![]));
        }
        let initial_cards = gallery_cards(&ctx);
        let requests = ctx
            .data(|data| {
                data.get_temp::<Vec<u64>>(egui::Id::new("character_gallery_thumbnail_requests"))
            })
            .unwrap();
        assert!(
            !requests.is_empty() && requests.len() <= 9,
            "Offscreen thumbnails requested: {}",
            requests.len()
        );
        assert!(requests.iter().all(|id| *id == portrait_id));
        assert_eq!(requests.len(), initial_cards.len());
        let scroll_id = gallery_scroll(&ctx).0;
        let mut state = egui::scroll_area::State::load(&ctx, scroll_id).unwrap();
        state.offset.y = 20000.0;
        state.store(&ctx, scroll_id);
        let _ = empty_sheet_frame(&ctx, &mut studio, empty_sheet_input(size, 1.0, vec![]));
        let later_cards = gallery_cards(&ctx);
        assert!(!later_cards.is_empty() && later_cards.len() <= 9);
        assert!(later_cards[0].0 > initial_cards.last().unwrap().0 + 100);
        let requests = ctx
            .data(|data| {
                data.get_temp::<Vec<u64>>(egui::Id::new("character_gallery_thumbnail_requests"))
            })
            .unwrap();
        assert_eq!(requests.len(), later_cards.len());
    }

    #[test]
    fn portrait_preview_crops_without_stretching_or_leaving_texture_bounds() {
        for image in [
            egui::vec2(2000.0, 600.0),
            egui::vec2(600.0, 2000.0),
            egui::vec2(600.0, 600.0),
        ] {
            let viewport = egui::vec2(230.0, 208.0);
            let uv = portrait_uv(image, viewport);
            assert!(
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)).contains_rect(uv)
            );
            let sampled = uv.size() * image;
            assert!((sampled.x / sampled.y - viewport.x / viewport.y).abs() < 0.0001);
        }
    }

    #[test]
    fn identity_gender_switch_changes_prompt_and_undoes_as_one_edit_per_click() {
        let ctx = egui::Context::default();
        let mut studio = empty_sheet_studio(&ctx, false);
        let id = studio.selected_asset.unwrap();
        studio.story.assets[0].character.pronouns = "legacy value".into();
        let state_id = egui::Id::new(("character_sheet_view", studio.document_epoch, id));
        ctx.data_mut(|data| {
            data.insert_temp(
                state_id,
                SheetView {
                    tab: SheetTab::Identity,
                    identity: 5,
                    ..Default::default()
                },
            )
        });
        let size = egui::vec2(1100.0, 720.0);
        let before = studio.story.clone();
        let output = empty_sheet_frame(&ctx, &mut studio, empty_sheet_input(size, 1.0, vec![]));
        assert_eq!(studio.story, before);
        assert_eq!(studio.story.assets[0].character.gender, None);
        assert!(studio.undo.is_empty());
        assert!(
            ctx.data(|data| data.get_temp::<SheetView>(state_id))
                .unwrap()
                .identity
                <= 4
        );
        fn contains_pronouns(shape: &egui::Shape) -> bool {
            match shape {
                egui::Shape::Text(text) => text.galley.job.text.contains("Pronouns"),
                egui::Shape::Vec(shapes) => shapes.iter().any(contains_pronouns),
                _ => false,
            }
        }
        assert!(!output
            .shapes
            .iter()
            .any(|shape| contains_pronouns(&shape.shape)));
        for (gender, expected_history) in [(CharacterGender::Female, 1), (CharacterGender::Male, 2)]
        {
            let button = ctx
                .data(|data| {
                    data.get_temp::<egui::Rect>(egui::Id::new(("character_gender", gender.label())))
                })
                .unwrap();
            assert_eq!(button.size(), egui::vec2(88.0, 32.0));
            let _ = empty_sheet_frame(
                &ctx,
                &mut studio,
                empty_sheet_input(size, 1.0, empty_sheet_click(button.center())),
            );
            let character = studio
                .story
                .assets
                .iter()
                .find(|asset| asset.id == id)
                .unwrap();
            assert_eq!(character.character.gender, Some(gender));
            assert_eq!(character.character.pronouns, "legacy value");
            assert_eq!(studio.undo.len(), expected_history);
            let prompt = characters::compose_prompt(
                &studio.story,
                character,
                CharacterPromptKind::Portrait,
                "",
            );
            assert!(prompt.contains(&format!("Gender: {}", gender.label())));
            assert!(!prompt.contains("Pronouns") && !prompt.contains("legacy value"));
        }
        studio.undo();
        assert_eq!(
            studio
                .story
                .assets
                .iter()
                .find(|asset| asset.id == id)
                .unwrap()
                .character
                .gender,
            Some(CharacterGender::Female)
        );
        studio.undo();
        assert_eq!(studio.story, before);
    }

    fn reference_studio(ctx: &egui::Context, original: bool, additional: usize) -> Studio {
        let mut studio = empty_sheet_studio(ctx, false);
        if original {
            studio.story.assets[0].width = 16;
            studio.story.assets[0].height = 16;
        }
        for index in 0..additional {
            let id = studio.story.alloc_id();
            studio.story.assets.push(Asset {
                id,
                name: format!("Reference {index}"),
                kind: AssetKind::Image,
                notes: String::new(),
                prompt: String::new(),
                width: 16,
                height: 16,
                character: CharacterProfile::default(),
            });
            studio.story.assets[0]
                .character
                .references
                .push(CharacterReference {
                    asset_id: id,
                    label: format!("Reference {index}"),
                    notes: "Keep this reference note.\n".repeat(100),
                });
        }
        studio
    }

    #[test]
    fn masonry_keeps_tiles_separate_and_opens_selected_reference_without_editing_story() {
        let sizes = [
            [480, 960],
            [1200, 600],
            [720, 720],
            [600, 900],
            [1600, 700],
            [500, 500],
        ];
        for width in [330.0, 650.0, 850.0, 1200.0] {
            let rects = masonry_rects(&sizes, width);
            for (i, rect) in rects.iter().enumerate() {
                assert!(rect.is_positive() && rect.left() >= 0.0 && rect.right() <= width + 0.1);
                for other in &rects[i + 1..] {
                    assert!(!rect.intersects(*other));
                }
            }
            assert!(rects.iter().any(|r| r.top() > 0.0));
        }
        let ctx = egui::Context::default();
        let mut studio = reference_studio(&ctx, false, 3);
        let original = studio.story.clone();
        let size = egui::vec2(1100.0, 720.0);
        for _ in 0..2 {
            let _ = empty_sheet_frame(&ctx, &mut studio, empty_sheet_input(size, 1.0, vec![]));
        }
        let card = reference_control_state(&ctx, "Board 1").unwrap().0;
        let _ = empty_sheet_frame(
            &ctx,
            &mut studio,
            empty_sheet_input(size, 1.0, empty_sheet_click(card.center())),
        );
        let id = studio.selected_asset.unwrap();
        let key = egui::Id::new(("character_sheet_view", studio.document_epoch, id));
        let view = ctx.data(|d| d.get_temp::<SheetView>(key)).unwrap();
        assert!(view.reference_focus);
        assert_eq!(view.reference, 1);
        assert_eq!(studio.story, original);
        assert!(studio.undo.is_empty());
    }

    fn reference_control_state(ctx: &egui::Context, label: &str) -> Option<(egui::Rect, bool)> {
        ctx.data(|data| data.get_temp(egui::Id::new(("character_reference_control", label))))
    }

    #[test]
    fn large_reference_requests_display_pixels_instead_of_the_gallery_thumbnail() {
        for density in [1.0, 1.5, 2.0] {
            let ctx = egui::Context::default();
            let mut studio = reference_studio(&ctx, true, 0);
            let id = studio.selected_asset.unwrap();
            studio.story.assets[0].width = 3000;
            studio.story.assets[0].height = 2000;
            // Rendering must request the right size before the async decoder completes.
            studio.assets.insert(id, Arc::new(vec![]));
            let before = studio.story.clone();
            for _ in 0..2 {
                empty_sheet_frame(
                    &ctx,
                    &mut studio,
                    empty_sheet_input(egui::vec2(1280.0, 760.0), density, vec![]),
                );
            }
            let requested = ctx
                .data(|data| data.get_temp::<u32>(egui::Id::new(("viewer_resolution", id))))
                .unwrap();
            let bounds = reference_image_rect(&ctx).shrink(6.0);
            let displayed =
                (bounds.width() / 3000.0).min(bounds.height() / 2000.0) * 3000.0 * density;
            assert!(requested as f32 >= displayed.min(2048.0));
            assert!(requested > 384);
            assert_eq!(studio.story, before);
            assert!(studio.undo.is_empty());
        }
    }

    fn reference_image_rect(ctx: &egui::Context) -> egui::Rect {
        ctx.data(|data| data.get_temp(egui::Id::new("character_reference_image")))
            .unwrap()
    }

    fn painted_text_contains(shape: &egui::Shape, needle: &str) -> bool {
        match shape {
            egui::Shape::Text(text) => text.galley.job.text.contains(needle),
            egui::Shape::Vec(shapes) => shapes
                .iter()
                .any(|shape| painted_text_contains(shape, needle)),
            _ => false,
        }
    }

    #[test]
    fn single_reference_uses_the_pane_without_a_filmstrip_pager_or_initial_details() {
        for (size, density) in [
            (egui::vec2(1100.0, 720.0), 1.0),
            (egui::vec2(1280.0, 760.0), 1.0),
            (egui::vec2(2560.0 / 1.5, 1440.0 / 1.5), 1.5),
        ] {
            for original in [false, true] {
                let ctx = egui::Context::default();
                let mut studio = reference_studio(&ctx, original, usize::from(!original));
                let before = studio.story.clone();
                let id = studio.selected_asset.unwrap();
                let state_id = egui::Id::new(("character_sheet_view", studio.document_epoch, id));
                let _ =
                    empty_sheet_frame(&ctx, &mut studio, empty_sheet_input(size, density, vec![]));
                let output =
                    empty_sheet_frame(&ctx, &mut studio, empty_sheet_input(size, density, vec![]));
                let image = reference_image_rect(&ctx);
                let [_, _, available, used] = ctx
                    .data(|data| data.get_temp::<[egui::Rect; 4]>(state_id.with("layout_bounds")))
                    .unwrap();
                assert!((image.width() - (available.width() - 32.0)).abs() < 1.0);
                assert!((image.bottom() - (available.bottom() - 16.0)).abs() < 1.0);
                assert!(used.max.x <= available.max.x + 1.0 && used.max.y <= available.max.y + 1.0);
                assert!(reference_control_state(&ctx, "Thumbnail 0").is_none());
                assert_eq!(
                    reference_control_state(&ctx, "Details").is_some(),
                    !original
                );
                for hidden in ["Save PNG", "1 / 1", "Original", "Keep this reference note."] {
                    assert!(
                        !output
                            .shapes
                            .iter()
                            .any(|shape| painted_text_contains(&shape.shape, hidden)),
                        "Unexpected visible {hidden}"
                    );
                }
                assert_eq!(studio.story, before);
                assert!(!studio.dirty && studio.undo.is_empty());
            }
        }
    }

    #[test]
    fn reference_details_reveal_preserves_notes_selection_and_fixed_workspace() {
        let ctx = egui::Context::default();
        let mut studio = reference_studio(&ctx, false, 3);
        let before = studio.story.clone();
        let size = egui::vec2(1100.0, 720.0);
        let id = studio.selected_asset.unwrap();
        let state_id = egui::Id::new(("character_sheet_view", studio.document_epoch, id));
        ctx.data_mut(|data| {
            data.insert_temp(
                state_id,
                SheetView {
                    reference_focus: true,
                    ..Default::default()
                },
            )
        });
        let _ = empty_sheet_frame(&ctx, &mut studio, empty_sheet_input(size, 1.0, vec![]));
        let _ = empty_sheet_frame(&ctx, &mut studio, empty_sheet_input(size, 1.0, vec![]));
        let full_image = reference_image_rect(&ctx);
        let thumbnail = reference_control_state(&ctx, "Thumbnail 2").unwrap().0;
        let _ = empty_sheet_frame(
            &ctx,
            &mut studio,
            empty_sheet_input(size, 1.0, empty_sheet_click(thumbnail.center())),
        );
        let details = reference_control_state(&ctx, "Details").unwrap().0;
        let _ = empty_sheet_frame(
            &ctx,
            &mut studio,
            empty_sheet_input(size, 1.0, empty_sheet_click(details.center())),
        );
        let output = empty_sheet_frame(&ctx, &mut studio, empty_sheet_input(size, 1.0, vec![]));
        let view = ctx
            .data(|data| data.get_temp::<SheetView>(state_id))
            .unwrap();
        assert_eq!(view.reference, 2);
        assert!(view.reference_details);
        let detail_image = reference_image_rect(&ctx);
        assert!(full_image.width() - detail_image.width() >= 206.0);
        assert_eq!(full_image.height(), detail_image.height());
        assert!(output
            .shapes
            .iter()
            .any(|shape| painted_text_contains(&shape.shape, "Keep this reference note.")));
        let [_, _, available, used] = ctx
            .data(|data| data.get_temp::<[egui::Rect; 4]>(state_id.with("layout_bounds")))
            .unwrap();
        assert!(used.max.x <= available.max.x + 1.0 && used.max.y <= available.max.y + 1.0);
        let _ = empty_sheet_frame(
            &ctx,
            &mut studio,
            empty_sheet_input(size, 1.0, empty_sheet_click(details.center())),
        );
        let view = ctx
            .data(|data| data.get_temp::<SheetView>(state_id))
            .unwrap();
        assert_eq!(view.reference, 2);
        assert!(!view.reference_details);
        assert_eq!(reference_image_rect(&ctx), full_image);
        assert_eq!(studio.story, before);
        assert!(!studio.dirty && studio.undo.is_empty());
    }

    #[test]
    fn context_reference_removal_preserves_selected_identity_notes_and_undo() {
        let ctx = egui::Context::default();
        let mut studio = reference_studio(&ctx, false, 4);
        let character_id = studio.selected_asset.unwrap();
        let refs = characters::reference_ids(&studio.story.assets[0]);
        let selected = refs[2];
        studio.story.assets[0].character.references[2].notes = "C must keep its own notes".into();
        let original = studio.story.clone();
        let key = egui::Id::new(("character_sheet_view", studio.document_epoch, character_id));
        ctx.data_mut(|data| {
            data.insert_temp(
                key,
                SheetView {
                    reference: 2,
                    reference_id: Some(selected),
                    reference_details: true,
                    ..Default::default()
                },
            )
        });
        let frame = |studio: &mut Studio, events| {
            empty_sheet_frame(
                &ctx,
                studio,
                empty_sheet_input(egui::vec2(1100.0, 720.0), 1.0, events),
            )
        };
        let _ = frame(&mut studio, vec![]);
        let _ = frame(&mut studio, vec![]);
        let remove = |studio: &mut Studio, thumbnail: &str| {
            let pos = reference_control_state(&ctx, thumbnail).unwrap().0.center();
            let events = empty_sheet_click(pos)
                .into_iter()
                .map(|mut event| {
                    if let egui::Event::PointerButton { button, .. } = &mut event {
                        *button = egui::PointerButton::Secondary;
                    }
                    event
                })
                .collect();
            let _ = frame(studio, events);
            let _ = frame(studio, vec![]);
            let (action, _): (egui::Rect, bool) = ctx.data(|data| {
                data.get_temp(egui::Id::new(("context_action", "Remove reference")))
                    .unwrap()
            });
            let _ = frame(studio, empty_sheet_click(action.center()));
            let _ = frame(studio, vec![]);
        };
        remove(&mut studio, "Thumbnail 0");
        assert_eq!(
            characters::reference_ids(&studio.story.assets[0]),
            vec![refs[1], refs[2], refs[3]]
        );
        let view = ctx.data(|data| data.get_temp::<SheetView>(key)).unwrap();
        assert_eq!((view.reference, view.reference_id), (1, Some(selected)));
        assert_eq!(
            studio.story.assets[0].character.references[1].notes,
            "C must keep its own notes"
        );
        assert_eq!(studio.undo.len(), 1);
        let removed = studio.story.clone();
        studio.undo();
        let _ = frame(&mut studio, vec![]);
        assert_eq!(studio.story, original);
        let view = ctx.data(|data| data.get_temp::<SheetView>(key)).unwrap();
        assert_eq!((view.reference, view.reference_id), (2, Some(selected)));
        studio.redo();
        let _ = frame(&mut studio, vec![]);
        assert_eq!(studio.story, removed);
        let view = ctx.data(|data| data.get_temp::<SheetView>(key)).unwrap();
        assert_eq!((view.reference, view.reference_id), (1, Some(selected)));
        remove(&mut studio, "Thumbnail 1");
        let view = ctx.data(|data| data.get_temp::<SheetView>(key)).unwrap();
        assert_eq!(
            (view.reference, view.reference_id),
            (1, Some(refs[3])),
            "Removing the selected image selects its next neighbor"
        );
        assert_eq!(studio.undo.len(), 2);
        studio.undo();
        let _ = frame(&mut studio, vec![]);
        assert_eq!(
            studio.story, removed,
            "Undo restores the removed image and its notes together"
        );
    }

    fn reference_pane_frame(
        ctx: &egui::Context,
        studio: &mut Studio,
        character: &mut Asset,
        view: &mut SheetView,
        events: Vec<egui::Event>,
    ) -> Option<ReferenceAction> {
        let mut action = None;
        let _ = ctx.run(
            empty_sheet_input(egui::vec2(900.0, 600.0), 1.0, events),
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    studio.character_references(ui, ctx, character, view, &mut action);
                });
            },
        );
        action
    }

    #[test]
    fn reference_toolbar_disables_capacity_and_busy_actions_without_starting_file_work() {
        let ctx = egui::Context::default();
        let mut studio = reference_studio(&ctx, true, 24);
        let mut character = studio.story.assets[0].clone();
        let before = character.clone();
        let mut view = SheetView::default();
        let (_tx, rx) = mpsc::channel();
        studio.job = Some(rx);
        let _ = reference_pane_frame(&ctx, &mut studio, &mut character, &mut view, vec![]);
        let _ = reference_pane_frame(&ctx, &mut studio, &mut character, &mut view, vec![]);
        for label in ["Add", "Copy"] {
            let (rect, enabled) = reference_control_state(&ctx, label).unwrap();
            assert!(!enabled);
            assert!(reference_pane_frame(
                &ctx,
                &mut studio,
                &mut character,
                &mut view,
                empty_sheet_click(rect.center())
            )
            .is_none());
        }
        let more = reference_control_state(&ctx, "More").unwrap().0;
        let _ = reference_pane_frame(
            &ctx,
            &mut studio,
            &mut character,
            &mut view,
            empty_sheet_click(more.center()),
        );
        let _ = reference_pane_frame(&ctx, &mut studio, &mut character, &mut view, vec![]);
        let (save, enabled) = reference_control_state(&ctx, "Save").unwrap();
        assert!(!enabled);
        assert!(reference_pane_frame(
            &ctx,
            &mut studio,
            &mut character,
            &mut view,
            empty_sheet_click(save.center())
        )
        .is_none());
        assert_eq!(character, before);
        studio.job = None;
        let _ = reference_pane_frame(&ctx, &mut studio, &mut character, &mut view, vec![]);
        assert!(reference_control_state(&ctx, "Copy").unwrap().1);
        assert!(reference_control_state(&ctx, "Save").unwrap().1);
        // Only inspect the emitted action: tests never touch the OS clipboard or file picker.
        assert!(matches!(
            reference_pane_frame(
                &ctx,
                &mut studio,
                &mut character,
                &mut view,
                empty_sheet_click(save.center())
            ),
            Some(ReferenceAction::ExportSheet)
        ));
        character.character.references.pop();
        let _ = reference_pane_frame(&ctx, &mut studio, &mut character, &mut view, vec![]);
        assert!(reference_control_state(&ctx, "Add").unwrap().1);
    }
}
