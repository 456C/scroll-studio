use super::*;

impl Studio {
    pub(super) fn new_lore(&mut self) {
        if self.story.lore.len() >= 4096 {
            return;
        }
        self.checkpoint();
        let id = self.story.alloc_id();
        self.story.lore.push(LoreEntry {
            id,
            title: "Untitled entry".into(),
            text: String::new(),
            links: Vec::new(),
        });
        self.tab = LibraryTab::Lore;
        self.selected_lore = Some(id);
        self.search.clear();
        self.changed();
    }

    pub(super) fn unlink_lore_target(&mut self, id: u64) {
        for entry in &mut self.story.lore {
            entry.links.retain(|link| *link != id);
        }
    }

    pub(super) fn lore_list(&mut self, ui: &mut egui::Ui) {
        if self.story.lore.is_empty() {
            return;
        }
        if primary(ui, "+ Entry").clicked() {
            self.new_lore();
        }
        ui.add(egui::TextEdit::singleline(&mut self.search).hint_text("Search lore"));
        let query = self.search.to_lowercase();
        let entries: Vec<_> = self
            .story
            .lore
            .iter()
            .filter(|e| e.title.to_lowercase().contains(&query))
            .collect();
        egui::ScrollArea::vertical().show_rows(ui, 30.0, entries.len(), |ui, rows| {
            for entry in &entries[rows] {
                if ui
                    .selectable_label(self.selected_lore == Some(entry.id), &entry.title)
                    .clicked()
                {
                    self.selected_lore = Some(entry.id);
                }
            }
        });
    }

    pub(super) fn lore_workspace(&mut self, ctx: &egui::Context) {
        self.canvas_rect = egui::Rect::NOTHING;
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG).inner_margin(22.0))
            .show(ctx, |ui| {
                let Some(index) = self
                    .story
                    .lore
                    .iter()
                    .position(|e| Some(e.id) == self.selected_lore)
                    .or_else(|| (!self.story.lore.is_empty()).then_some(0))
                else {
                    ui.heading("Lore");
                    let rect = egui::Rect::from_center_size(
                        ui.available_rect_before_wrap().center(),
                        egui::vec2(180.0, 95.0),
                    );
                    ui.scope_builder(
                        egui::UiBuilder::new()
                            .max_rect(rect)
                            .layout(egui::Layout::top_down(egui::Align::Center)),
                        |ui| {
                            let (mark, _) = ui
                                .allocate_exact_size(egui::vec2(38.0, 38.0), egui::Sense::hover());
                            icons::draw(ui.painter(), mark, icons::Icon::Lore, MUTED);
                            if primary(ui, "+ Entry").clicked() {
                                self.new_lore();
                            }
                        },
                    );
                    return;
                };
                let mut entry = self.story.lore[index].clone();
                self.selected_lore = Some(entry.id);
                let mut remove = false;
                ui.push_id(entry.id, |ui| {
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut entry.title)
                                .font(egui::TextStyle::Heading)
                                .desired_width((ui.available_width() - 70.0).max(80.0))
                                .char_limit(1024)
                                .hint_text("Entry title"),
                        );
                        motion::menu_button(ui, "•••", |ui| {
                            if motion::button(ui, "Delete entry").clicked() {
                                remove = true;
                                ui.close_menu();
                            }
                        });
                    });
                    ui.add_space(8.0);
                    let targets = self
                        .story
                        .lore
                        .iter()
                        .filter(|n| n.id != entry.id)
                        .map(|n| (n.id, LibraryTab::Lore, n.title.as_str()))
                        .chain(
                            self.story
                                .assets
                                .iter()
                                .filter(|a| a.kind == AssetKind::Character)
                                .map(|a| (a.id, LibraryTab::Characters, a.name.as_str())),
                        )
                        .chain(
                            self.story
                                .locations
                                .iter()
                                .map(|l| (l.id, LibraryTab::Backgrounds, l.name.as_str())),
                        )
                        .chain(
                            self.story
                                .groups
                                .iter()
                                .map(|g| (g.id, LibraryTab::Groups, g.name.as_str())),
                        )
                        .collect::<Vec<_>>();
                    let mut open = None;
                    egui::ScrollArea::vertical()
                        .id_salt("connections")
                        .max_height(100.0)
                        .show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                motion::menu_button(ui, "+ Link", |ui| {
                                    if targets.is_empty() {
                                        ui.label("No entries to link yet");
                                    }
                                    egui::ScrollArea::vertical()
                                        .max_height(260.0)
                                        .show(ui, |ui| {
                                            for &(id, tab, name) in &targets {
                                                let kind = match tab {
                                                    LibraryTab::Lore => "Lore",
                                                    LibraryTab::Characters => "Character",
                                                    LibraryTab::Backgrounds => "Location",
                                                    _ => "Group",
                                                };
                                                let mut linked = entry.links.contains(&id);
                                                if ui
                                                    .push_id(id, |ui| {
                                                        ui.checkbox(
                                                            &mut linked,
                                                            format!("{kind} · {name}"),
                                                        )
                                                    })
                                                    .inner
                                                    .changed()
                                                {
                                                    if linked {
                                                        entry.links.push(id);
                                                    } else {
                                                        entry.links.retain(|link| *link != id);
                                                    }
                                                }
                                            }
                                        });
                                });
                                for &(id, tab, name) in &targets {
                                    if entry.links.contains(&id)
                                        && ui.push_id(id, |ui| ui.link(name)).inner.clicked()
                                    {
                                        open = Some((id, tab));
                                    }
                                }
                            });
                        });
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .id_salt("lore_text")
                        .show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut entry.text)
                                    .desired_rows(12)
                                    .desired_width(f32::INFINITY)
                                    .char_limit(25_000)
                                    .hint_text("Write a note…"),
                            );
                        });
                    if let Some((id, tab)) = open {
                        self.tab = tab;
                        self.search.clear();
                        match tab {
                            LibraryTab::Characters => self.select_character(id),
                            LibraryTab::Backgrounds => self.selected_location = Some(id),
                            LibraryTab::Groups => self.selected_group = Some(id),
                            _ => self.selected_lore = Some(id),
                        }
                    }
                });
                if remove {
                    self.checkpoint();
                    self.story.lore.remove(index);
                    self.unlink_lore_target(entry.id);
                    self.selected_lore = None;
                    self.changed();
                } else if entry != self.story.lore[index] {
                    self.checkpoint();
                    self.story.lore[index] = entry;
                    self.changed();
                }
            });
    }
}
