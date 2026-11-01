use super::*;
use egui::collapsing_header::CollapsingState;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum LayerGroup {
    Bubbles,
    Characters,
    Backgrounds,
    Images,
    Strokes,
    Shapes,
    Effects,
}

impl LayerGroup {
    const ALL: [Self; 7] = [
        Self::Bubbles,
        Self::Characters,
        Self::Backgrounds,
        Self::Images,
        Self::Strokes,
        Self::Shapes,
        Self::Effects,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Bubbles => "Bubbles & text",
            Self::Characters => "Characters",
            Self::Backgrounds => "Backgrounds",
            Self::Images => "Images",
            Self::Strokes => "Brush strokes",
            Self::Shapes => "Shapes",
            Self::Effects => "Effects",
        }
    }

    fn index(self) -> usize {
        Self::ALL.iter().position(|group| *group == self).unwrap()
    }
}

struct LayerRow {
    id: u64,
    name: String,
    visible: bool,
    locked: bool,
}

/// Grouping is a view of the layer stack; it never changes painting order.
fn grouped_layers(chapter: &Chapter, assets: &[Asset]) -> [Vec<LayerRow>; 7] {
    let mut image_groups: HashMap<_, _> = assets
        .iter()
        .map(|asset| {
            let group = match asset.kind {
                AssetKind::Character => LayerGroup::Characters,
                AssetKind::Background => LayerGroup::Backgrounds,
                AssetKind::Image => LayerGroup::Images,
            };
            (asset.id, group)
        })
        .collect();
    // Additional character references are ordinary image assets in storage.
    // Their membership in a character sheet is what makes them character art.
    for character in assets
        .iter()
        .filter(|asset| asset.kind == AssetKind::Character)
    {
        for reference in &character.character.references {
            image_groups.insert(reference.asset_id, LayerGroup::Characters);
        }
    }
    let mut groups: [Vec<LayerRow>; 7] = std::array::from_fn(|_| Vec::new());
    for layer in chapter.layers.iter().rev() {
        let group = match &layer.kind {
            LayerKind::Text { .. } => LayerGroup::Bubbles,
            LayerKind::Image { asset_id, .. } => image_groups
                .get(asset_id)
                .copied()
                .unwrap_or(LayerGroup::Images),
            LayerKind::Stroke { .. } => LayerGroup::Strokes,
            LayerKind::Effect { .. } => LayerGroup::Effects,
            LayerKind::Shape { .. } => LayerGroup::Shapes,
        };
        groups[group.index()].push(LayerRow {
            id: layer.id,
            name: layer.name.clone(),
            visible: layer.visible,
            locked: layer.locked,
        });
    }
    groups
}

impl Studio {
    pub(super) fn layer_list(&mut self, ui: &mut egui::Ui) {
        let chapter = &self.story.chapters[self.chapter];
        let chapter_id = chapter.id;
        let groups = grouped_layers(chapter, &self.story.assets);
        let empty = chapter.layers.is_empty();
        let mut open_all = None;
        ui.horizontal(|ui| {
            ui.label(RichText::new("Layers").size(14.0).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let selected = self.selected.filter(|id| {
                    self.story.chapters[self.chapter]
                        .layers
                        .iter()
                        .any(|layer| layer.id == *id)
                });
                ui.add_enabled_ui(selected.is_some(), |ui| {
                    icons::menu_button(ui, "Layer actions", |ui| {
                        if let Some(id) = selected {
                            self.layer_context_actions(ui, id);
                        }
                    });
                });
                ui.add_enabled_ui(!empty, |ui| {
                    if icons::button(ui, icons::Icon::ChevronsDown, false, "Expand layer folders")
                        .clicked()
                    {
                        open_all = Some(true);
                    }
                    if icons::button(ui, icons::Icon::ChevronsUp, false, "Collapse layer folders")
                        .clicked()
                    {
                        open_all = Some(false);
                    }
                });
            });
        });
        if empty {
            ui.add_space(12.0);
            ui.label(RichText::new("No layers").color(MUTED));
            return;
        }
        for (group, rows) in LayerGroup::ALL.into_iter().zip(groups) {
            let id = egui::Id::new((
                "layer_object_folder",
                self.document_epoch,
                chapter_id,
                group,
            ));
            let mut state = CollapsingState::load_with_default_open(ui.ctx(), id, true);
            if let Some(open) = open_all {
                state.set_open(open);
                state.store(ui.ctx());
            }
            if rows.is_empty() {
                continue;
            }
            let mut header_clicked = false;
            let mut header = state.show_header(ui, |ui| {
                header_clicked = ui
                    .add(
                        motion::Button::new(
                            RichText::new(format!("{}  {}", group.label(), rows.len()))
                                .size(12.0)
                                .strong(),
                        )
                        .frame(false)
                        .truncate(),
                    )
                    .clicked();
            });
            if header_clicked {
                header.toggle();
            }
            header.body(|ui| {
                for row in rows {
                    // The property editor above this list changes widget count
                    // when a layer is selected. Give this row an independent
                    // ID seed so its original controls keep an open menu alive.
                    let mut row_ui = egui::Ui::new(
                        ui.ctx().clone(),
                        egui::Id::new(("layer_row", self.document_epoch, chapter_id, row.id)),
                        egui::UiBuilder::new()
                            .layer_id(ui.layer_id())
                            .max_rect(ui.available_rect_before_wrap()),
                    );
                    row_ui.set_style(ui.style().clone());
                    row_ui.set_clip_rect(ui.clip_rect());
                    if !ui.is_enabled() {
                        row_ui.disable();
                    }
                    row_ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        let eye = icons::small_button(
                            ui,
                            if row.visible {
                                icons::Icon::Eye
                            } else {
                                icons::Icon::EyeOff
                            },
                            false,
                            if row.visible {
                                "Hide layer"
                            } else {
                                "Show layer"
                            },
                        );
                        if eye.clicked() {
                            self.checkpoint();
                            if let Some(layer) = self.story.chapters[self.chapter]
                                .layers
                                .iter_mut()
                                .find(|layer| layer.id == row.id)
                            {
                                layer.visible = !row.visible;
                            }
                            self.changed();
                        }
                        let name_width = (ui.available_width() - 20.0).max(40.0);
                        let response = ui
                            .allocate_ui_with_layout(
                                egui::vec2(name_width, 26.0),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    panels::list_row(
                                        ui,
                                        &row.name,
                                        self.selected == Some(row.id),
                                        26.0,
                                        None,
                                    )
                                },
                            )
                            .inner
                            .on_hover_text(if row.locked {
                                format!("{} · Locked", row.name)
                            } else {
                                row.name.clone()
                            });
                        let (lock_rect, lock_response) =
                            ui.allocate_exact_size(egui::vec2(16.0, 24.0), egui::Sense::click());
                        let targets = [&response, &eye, &lock_response];
                        if response.clicked()
                            || targets.iter().any(|target| target.secondary_clicked())
                        {
                            self.selected = Some(row.id);
                            self.selected_asset = None;
                        }
                        // Native context_menu upgrades its response's sense.
                        // Keep each real control's identity: union responses
                        // cannot safely be used for further interaction.
                        for target in targets {
                            self.register_context(target);
                            appearance::context_menu(target, |ui| {
                                self.layer_context_actions(ui, row.id)
                            });
                        }
                        if row.locked {
                            icons::draw(
                                ui.painter(),
                                lock_rect.shrink(2.0),
                                icons::Icon::Lock,
                                MUTED,
                            );
                        }
                        #[cfg(test)]
                        ui.data_mut(|data| {
                            data.insert_temp(
                                egui::Id::new(("layer_row_bounds", row.id)),
                                (eye.rect, response.rect, lock_rect),
                            )
                        });
                    });
                    ui.advance_cursor_after_rect(row_ui.min_rect());
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(id: u64, kind: AssetKind) -> Asset {
        Asset {
            id,
            name: format!("Asset {id}"),
            kind,
            notes: String::new(),
            prompt: String::new(),
            width: 16,
            height: 16,
            character: CharacterProfile::default(),
        }
    }

    fn layer(id: u64, kind: LayerKind) -> Layer {
        Layer {
            id,
            name: format!("Layer {id}"),
            visible: true,
            locked: false,
            opacity: 1.0,
            x: 0.0,
            y: 0.0,
            width: 16.0,
            height: 16.0,
            kind,
        }
    }

    fn image(id: u64, asset_id: u64) -> Layer {
        layer(
            id,
            LayerKind::Image {
                source_rect: None,
                clip: None,
                asset_id,
                mask: Vec::new(),
            },
        )
    }

    #[test]
    fn every_object_appears_once_and_character_references_stay_with_characters() {
        let mut character = asset(1, AssetKind::Character);
        character.character.references.push(CharacterReference {
            asset_id: 4,
            label: "Side view".into(),
            notes: String::new(),
        });
        let assets = vec![
            character,
            asset(2, AssetKind::Background),
            asset(3, AssetKind::Image),
            asset(4, AssetKind::Image),
        ];
        let mut chapter = Chapter::new(10, "All object types", 800, 1200);
        chapter.layers = vec![
            image(11, 1),
            image(12, 2),
            image(13, 3),
            image(14, 4),
            image(15, 99),
            layer(
                16,
                LayerKind::Text {
                    text: "Hello".into(),
                    color: [0, 0, 0, 255],
                    size: 32.0,
                    bubble: true,
                    style: TextStyle::dialogue(),
                },
            ),
            layer(
                17,
                LayerKind::Text {
                    text: "Caption".into(),
                    color: [0, 0, 0, 255],
                    size: 32.0,
                    bubble: false,
                    style: TextStyle::default(),
                },
            ),
            layer(
                18,
                LayerKind::Stroke {
                    points: vec![[0.0, 0.0]],
                    color: [0, 0, 0, 255],
                    size: 4.0,
                },
            ),
            layer(19, LayerKind::Shape { color: [255; 4] }),
        ];
        let before = chapter.clone();
        let groups = grouped_layers(&chapter, &assets);
        let ids = |group: LayerGroup| {
            groups[group.index()]
                .iter()
                .map(|row| row.id)
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(LayerGroup::Bubbles), vec![17, 16]);
        assert_eq!(ids(LayerGroup::Characters), vec![14, 11]);
        assert_eq!(ids(LayerGroup::Backgrounds), vec![12]);
        assert_eq!(ids(LayerGroup::Images), vec![15, 13]);
        assert_eq!(ids(LayerGroup::Strokes), vec![18]);
        assert_eq!(ids(LayerGroup::Shapes), vec![19]);
        let flattened: Vec<_> = groups.iter().flatten().map(|row| row.id).collect();
        assert_eq!(flattened.len(), chapter.layers.len());
        assert_eq!(
            flattened.iter().copied().collect::<HashSet<_>>().len(),
            chapter.layers.len()
        );
        assert_eq!(chapter, before, "Folder display cannot reorder the chapter");
    }

    #[test]
    fn grouped_rows_preserve_visibility_and_lock_state() {
        let mut chapter = Chapter::new(1, "Layer flags", 800, 1200);
        let mut hidden = image(2, 99);
        hidden.visible = false;
        hidden.locked = true;
        chapter.layers.push(hidden);
        let groups = grouped_layers(&chapter, &[]);
        let row = &groups[LayerGroup::Images.index()][0];
        assert!(!row.visible && row.locked);
    }

    #[test]
    fn right_clicking_name_eye_or_lock_targets_the_row_without_toggling_it() {
        fn frame(ctx: &egui::Context, studio: &mut Studio, events: Vec<egui::Event>) {
            let mut frame = eframe::Frame::_new_kittest();
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1100.0, 720.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| eframe::App::update(studio, ctx, &mut frame),
            );
        }
        for part in 0..3 {
            for select_first in [false, true] {
                let ctx = egui::Context::default();
                let mut studio = Studio::from_context(&ctx);
                studio.story = Story::new("Layer contexts");
                studio.welcome = false;
                studio.error = None;
                studio.preferences.motion = settings::Motion::Off;
                studio.preferences.glass_enabled = false;
                let first = studio.story.alloc_id();
                let target = studio.story.alloc_id();
                studio.story.chapters[0].layers = vec![
                    layer(first, LayerKind::Shape { color: [150; 4] }),
                    layer(target, LayerKind::Shape { color: [200; 4] }),
                ];
                studio.story.chapters[0].layers[1].locked = true;
                studio.selected = select_first.then_some(first);
                frame(&ctx, &mut studio, vec![]);
                frame(&ctx, &mut studio, vec![]);
                let (eye, name, lock): (egui::Rect, egui::Rect, egui::Rect) = ctx.data(|data| {
                    data.get_temp(egui::Id::new(("layer_row_bounds", target)))
                        .unwrap()
                });
                let point = [eye, name, lock][part].center();
                let original = studio.story.clone();
                frame(
                    &ctx,
                    &mut studio,
                    vec![
                        egui::Event::PointerMoved(point),
                        egui::Event::PointerButton {
                            pos: point,
                            button: egui::PointerButton::Secondary,
                            pressed: true,
                            modifiers: egui::Modifiers::NONE,
                        },
                        egui::Event::PointerButton {
                            pos: point,
                            button: egui::PointerButton::Secondary,
                            pressed: false,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                );
                frame(&ctx, &mut studio, vec![]);
                assert_eq!(studio.selected, Some(target), "Control {part}");
                assert!(
                    context_ui::context_menu_open(&ctx),
                    "Control {part} must retain its menu after selection changes"
                );
                assert_eq!(studio.story, original);
                assert!(studio.undo.is_empty());
                let enabled = |label| {
                    ctx.data(|data| {
                        data.get_temp::<(egui::Rect, bool)>(egui::Id::new((
                            "layer_context_action_bounds",
                            label,
                        )))
                        .unwrap()
                        .1
                    })
                };
                assert!(enabled("Unlock"));
                assert!(enabled("Duplicate"));
                assert!(!enabled("Delete"));
            }
        }
    }
}
