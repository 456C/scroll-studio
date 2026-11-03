use super::*;

// egui 0.31 exposes BarState but keeps its shared context-menu ID private.
// Use the same root as Response::context_menu so Escape and context changes
// close one native hierarchy, including any open submenus.
fn native_menu_id() -> egui::Id {
    egui::Id::new("__egui::context_menu")
}

pub(super) fn context_menu_open(ctx: &egui::Context) -> bool {
    egui::menu::BarState::load(ctx, native_menu_id()).is_some()
}

fn close_context_menu(ctx: &egui::Context) {
    let mut state = egui::menu::BarState::load(ctx, native_menu_id());
    **state = None;
    state.store(ctx, native_menu_id());
}

fn blank_response(ctx: &egui::Context, point: egui::Pos2) -> egui::Response {
    let rect = egui::Rect::from_center_size(point, egui::Vec2::splat(1.0));
    let ui = egui::Ui::new(
        ctx.clone(),
        egui::Id::new("app_context_blank_ui"),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(ctx.screen_rect()),
    );
    ui.interact(
        rect,
        egui::Id::new("app_context_blank"),
        egui::Sense::click(),
    )
}

#[derive(Clone)]
struct TextTarget {
    id: egui::Id,
    selection: Option<egui::text::CCursorRange>,
    editable: bool,
}

#[derive(Clone)]
enum FallbackKind {
    App,
    Text(TextTarget),
}

#[derive(Clone)]
struct FallbackMenu {
    response: egui::Response,
    kind: FallbackKind,
    blank: bool,
}

pub(super) struct CanvasContext {
    pub chapter: u64,
    pub layer: Option<u64>,
    pub point: [f32; 2],
}

#[derive(Default)]
pub(super) struct ContextState {
    registered: HashSet<egui::Id>,
    scope: Option<(u64, u64, u8, bool, bool, bool)>,
    fallback: Option<FallbackMenu>,
    text_command: Option<(TextTarget, egui::Event)>,
    right_text_selection: Option<(egui::Id, egui::text::CCursorRange)>,
    pub canvas: Option<CanvasContext>,
}

fn action(ui: &mut egui::Ui, label: &str, enabled: bool) -> egui::Response {
    let response = ui.add_enabled(enabled, motion::Button::new(label));
    #[cfg(test)]
    ui.data_mut(|data| {
        data.insert_temp(
            egui::Id::new(("layer_context_action_bounds", label)),
            (response.rect, response.enabled()),
        );
        data.insert_temp(
            egui::Id::new(("context_action", label)),
            (response.rect, response.enabled()),
        );
    });
    if response.clicked() {
        ui.close_menu();
    }
    response
}

impl Studio {
    pub(super) fn register_context(&mut self, response: &egui::Response) {
        self.context.registered.insert(response.id);
    }

    fn context_is_modal(&self) -> bool {
        self.dialog.is_some()
            || self.camera_dialog.is_some()
            || self.snippet.is_some()
            || self.setup.is_some()
            || self.location_editor.is_some()
            || self.dialogue_draft.is_some()
            || self.spacing_draft.is_some()
            || self.export_dialog
            || self.style_prompt_open
            || self.error.is_some()
            || self.reader.is_some()
    }

    pub(super) fn begin_context_frame(&mut self, ctx: &egui::Context) {
        let scope = (
            self.document_epoch,
            self.story.chapters[self.chapter].id,
            self.tab as u8,
            self.settings_open,
            self.welcome,
            self.context_is_modal(),
        );
        if self.context.scope.is_some_and(|previous| previous != scope) {
            close_context_menu(ctx);
            self.context.fallback = None;
            self.context.canvas = None;
            self.context.text_command = None;
        }
        self.context.scope = Some(scope);
        self.context.registered.clear();
        self.context.right_text_selection = None;
        if ctx.input(|input| input.pointer.secondary_clicked()) {
            if let Some(id) = ctx.interaction_snapshot(|snapshot| snapshot.clicked) {
                if let Some(state) = egui::TextEdit::load_state(ctx, id) {
                    if let Some(range) = state
                        .cursor
                        .char_range()
                        .filter(|range| range.primary.index != range.secondary.index)
                    {
                        self.context.right_text_selection = Some((id, range));
                    }
                }
            }
        }
    }

    pub(super) fn dispatch_text_context_command(&mut self, ctx: &egui::Context) {
        let Some((target, event)) = self.context.text_command.take() else {
            return;
        };
        if let Some(mut state) = egui::TextEdit::load_state(ctx, target.id) {
            state.cursor.set_char_range(target.selection);
            state.store(ctx, target.id);
            ctx.memory_mut(|memory| memory.request_focus(target.id));
            // Dispatch after app shortcuts, before the target editor processes input.
            ctx.input_mut(|input| input.events.push(event));
        }
    }

    pub(super) fn finish_context_frame(&mut self, ctx: &egui::Context) {
        if ctx.input(|input| input.pointer.secondary_clicked()) {
            let clicked = ctx.interaction_snapshot(|snapshot| snapshot.clicked);
            if let Some(response) = clicked.and_then(|id| ctx.read_response(id)) {
                if !self.context.registered.contains(&response.id)
                    && response.enabled()
                    && response.layer_id.order != egui::Order::Tooltip
                {
                    let text = egui::TextEdit::load_state(ctx, response.id);
                    let in_menu = response.layer_id.order == egui::Order::Foreground
                        && context_menu_open(ctx);
                    if !in_menu && (text.is_some() || !self.context_is_modal()) {
                        let kind = if let Some(mut state) = text {
                            let selection = self
                                .context
                                .right_text_selection
                                .filter(|(id, _)| *id == response.id)
                                .map(|(_, range)| range)
                                .or_else(|| state.cursor.char_range());
                            state.cursor.set_char_range(selection);
                            state.store(ctx, response.id);
                            FallbackKind::Text(TextTarget {
                                id: response.id,
                                selection,
                                editable: ctx.output(|output| output.mutable_text_under_cursor),
                            })
                        } else {
                            FallbackKind::App
                        };
                        self.context.fallback = Some(FallbackMenu {
                            response,
                            kind,
                            blank: false,
                        });
                    }
                }
            } else if !self.context_is_modal() {
                if let Some(point) = ctx.pointer_interact_pos().filter(|point| {
                    ctx.layer_id_at(*point)
                        .is_none_or(|layer| layer.order == egui::Order::Background)
                }) {
                    // No widget owns this click. A one-point source lets native
                    // popup handling own dismissal without adding a screen-sized
                    // click target that could compete with a tall canvas.
                    let response = blank_response(ctx, point);
                    let mut state = egui::menu::BarState::load(ctx, native_menu_id());
                    **state = Some(egui::menu::MenuRoot::new(point, response.id));
                    state.store(ctx, native_menu_id());
                    self.context.fallback = Some(FallbackMenu {
                        response,
                        kind: FallbackKind::App,
                        blank: true,
                    });
                }
            }
        }
        if let Some(menu) = self.context.fallback.clone() {
            let response = if menu.blank {
                blank_response(ctx, menu.response.rect.center())
            } else if let Some(response) = ctx.read_response(menu.response.id) {
                response
            } else {
                self.context.fallback = None;
                close_context_menu(ctx);
                return;
            };
            appearance::context_menu(&response, |ui| match menu.kind {
                FallbackKind::App => self.app_context_actions(ui),
                FallbackKind::Text(target) => self.text_context_actions(ui, &target),
            });
            if !response.context_menu_opened() {
                self.context.fallback = None;
            }
        }
        let open_id = egui::menu::BarState::load(ctx, native_menu_id())
            .as_ref()
            .map(|root| root.id);
        if open_id.is_some_and(|id| {
            !self.context.registered.contains(&id)
                && self
                    .context
                    .fallback
                    .as_ref()
                    .is_none_or(|menu| menu.response.id != id)
        }) {
            close_context_menu(ctx);
        }
    }

    fn text_context_actions(&mut self, ui: &mut egui::Ui, target: &TextTarget) {
        let selected = target
            .selection
            .is_some_and(|range| range.primary.index != range.secondary.index);
        let mut event = None;
        if action(ui, "Cut", target.editable && selected).clicked() {
            event = Some(egui::Event::Cut);
        }
        if action(ui, "Copy", selected).clicked() {
            event = Some(egui::Event::Copy);
        }
        if action(ui, "Paste", target.editable).clicked() {
            match arboard::Clipboard::new().and_then(|mut clipboard| clipboard.get_text()) {
                Ok(text) => event = Some(egui::Event::Paste(text)),
                Err(_) => self.status = "No text on the clipboard".into(),
            }
        }
        ui.separator();
        if action(ui, "Select all", true).clicked() {
            event = Some(egui::Event::Key {
                key: egui::Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::COMMAND,
            });
        }
        if let Some(event) = event {
            self.context.text_command = Some((target.clone(), event));
            ui.ctx().request_repaint();
        }
    }

    pub(super) fn layer_context_actions(&mut self, ui: &mut egui::Ui, id: u64) {
        let Some(index) = self.story.chapters[self.chapter]
            .layers
            .iter()
            .position(|layer| layer.id == id)
        else {
            return;
        };
        let layer = self.story.chapters[self.chapter].layers[index].clone();
        match &layer.kind {
            LayerKind::Image {
                asset_id,
                mask,
                clip,
                source_rect,
            } => {
                self.selection_actions(ui);
                if action(ui, "Magic Eraser", !layer.locked).clicked() {
                    self.selected = Some(id);
                    self.selected_asset = None;
                    self.tool = Tool::MagicEraser;
                }
                if action(ui, "Blend edges…", !layer.locked).clicked() {
                    self.enable_backing(ui.ctx(), id);
                }
                if action(ui, "Add motion", !layer.locked).clicked() {
                    self.add_motion(id);
                }
                if action(ui, "Pin reference", self.can_pin_reference(*asset_id)).clicked() {
                    self.pin_reference(ui.ctx(), *asset_id, None);
                }
                if action(ui, "Copy cutout", self.assets.contains_key(asset_id)).clicked() {
                    self.copy_cutout(&layer);
                }
                if action(ui, "Lasso", !layer.locked).clicked() {
                    self.selected = Some(id);
                    self.selected_asset = None;
                    self.tool = Tool::Lasso;
                }
                if action(ui, "Smart Lasso", !layer.locked).clicked() {
                    self.selected = Some(id);
                    self.selected_asset = None;
                    self.tool = Tool::SmartLasso;
                }
                if (!mask.is_empty() || clip.is_some() || source_rect.is_some())
                    && action(ui, "Reset clip", !layer.locked).clicked()
                {
                    self.reset_image_clip(id);
                }
                ui.separator();
            }
            LayerKind::Text { style, bubble, .. } => {
                if *bubble && action(ui, "Add linked bubble", !layer.locked).clicked() {
                    self.add_linked_bubble(id);
                }
                if style.cluster_id.is_some()
                    && action(ui, "Detach bubble", !layer.locked).clicked()
                {
                    self.checkpoint();
                    if let LayerKind::Text { style, .. } =
                        &mut self.story.chapters[self.chapter].layers[index].kind
                    {
                        style.cluster_id = None;
                        style.joins.clear();
                    }
                    self.changed();
                }
                if let Some(preset) = style.preset_id {
                    if action(
                        ui,
                        "Edit bubble preset",
                        self.story
                            .bubble_presets
                            .iter()
                            .any(|item| item.id == preset),
                    )
                    .clicked()
                    {
                        self.selected_bubble_preset = Some(preset);
                        self.tab = LibraryTab::Bubbles;
                    }
                }
                ui.separator();
            }
            _ => {}
        }
        if action(
            ui,
            "Duplicate",
            self.story.chapters[self.chapter].layers.len() < 100_000,
        )
        .clicked()
        {
            self.checkpoint();
            let mut copy = layer.clone();
            copy.id = self.story.alloc_id();
            copy.x += 30.0;
            copy.y += 30.0;
            copy.name.push_str(" copy");
            self.selected = Some(copy.id);
            self.selected_asset = None;
            self.story.chapters[self.chapter].layers.push(copy);
            self.changed();
        }
        let raise = action(
            ui,
            "Raise",
            index + 1 < self.story.chapters[self.chapter].layers.len(),
        );
        let lower = action(ui, "Lower", index > 0);
        #[cfg(test)]
        ui.data_mut(|data| {
            data.insert_temp(
                egui::Id::new("layer_order_control_bounds"),
                [(raise.rect, raise.enabled()), (lower.rect, lower.enabled())],
            )
        });
        if raise.clicked() || lower.clicked() {
            self.checkpoint();
            let target = if raise.clicked() {
                index + 1
            } else {
                index - 1
            };
            self.story.chapters[self.chapter].layers.swap(index, target);
            self.changed();
        }
        ui.separator();
        if action(ui, if layer.visible { "Hide" } else { "Show" }, true).clicked() {
            self.checkpoint();
            self.story.chapters[self.chapter].layers[index].visible = !layer.visible;
            self.changed();
        }
        if action(ui, if layer.locked { "Unlock" } else { "Lock" }, true).clicked() {
            self.checkpoint();
            self.story.chapters[self.chapter].layers[index].locked = !layer.locked;
            self.changed();
        }
        if action(ui, "Delete", !layer.locked).clicked() {
            self.selected = Some(id);
            self.selected_asset = None;
            self.delete_selection();
        }
    }

    pub(super) fn asset_context_actions(&mut self, ui: &mut egui::Ui, id: u64) {
        let Some(asset) = self
            .story
            .assets
            .iter()
            .find(|asset| asset.id == id)
            .cloned()
        else {
            return;
        };
        let image = asset.width > 0 && asset.height > 0 && self.assets.contains_key(&id);
        if !self.assets_only && action(ui, "Place", image).clicked() {
            self.place_asset(id, None);
        }
        if action(ui, "Copy image", image).clicked() {
            self.copy_asset_image(id);
        }
        if action(ui, "Pin reference", image).clicked() {
            self.pin_reference(ui.ctx(), id, None);
        }
        if action(
            ui,
            "Copy prompt",
            !asset.prompt.trim().is_empty() || !self.story.visual_style.trim().is_empty(),
        )
        .clicked()
        {
            ui.ctx()
                .copy_text(scroll_studio::characters::with_story_style(
                    &self.story.visual_style,
                    &asset.prompt,
                ));
            self.status = "Prompt copied".into();
        }
        if asset.kind == AssetKind::Background {
            ui.separator();
            self.background_move_menu(ui, id);
        }
    }

    pub(super) fn copy_asset_image(&mut self, id: u64) {
        let result = (|| -> anyhow::Result<()> {
            let bytes = self
                .assets
                .get(&id)
                .ok_or_else(|| anyhow::anyhow!("Image not found"))?;
            let image = compositor::decode_image(bytes)?;
            arboard::Clipboard::new()?.set_image(arboard::ImageData {
                width: image.width() as usize,
                height: image.height() as usize,
                bytes: std::borrow::Cow::Owned(image.into_raw()),
            })?;
            Ok(())
        })();
        match result {
            Ok(()) => self.status = "Image copied".into(),
            Err(error) => self.error = Some(format!("Could not copy image: {error}")),
        }
    }

    pub(super) fn character_context_actions(&mut self, ui: &mut egui::Ui, id: u64) {
        let Some(character) = self
            .story
            .assets
            .iter()
            .find(|asset| asset.id == id && asset.kind == AssetKind::Character)
            .cloned()
        else {
            return;
        };
        if action(ui, "Open sheet", true).clicked() {
            self.tab = LibraryTab::Characters;
            self.select_character(id);
        }
        if action(ui, "Copy prompt", true).clicked() {
            let scene = if self.selected_asset == Some(id) {
                &self.character_scene
            } else {
                ""
            };
            ui.ctx()
                .copy_text(scroll_studio::characters::compose_prompt(
                    &self.story,
                    &character,
                    self.character_prompt_kind,
                    scene,
                ));
            self.status = "Prompt copied".into();
        }
        let image = scroll_studio::characters::reference_ids(&character)
            .into_iter()
            .find(|id| self.can_pin_reference(*id));
        if action(ui, "Pin reference", image.is_some()).clicked() {
            self.pin_reference(ui.ctx(), image.unwrap(), Some(id));
        }
        let references = !scroll_studio::characters::reference_ids(&character).is_empty();
        if action(ui, "Copy sheet", references && self.job.is_none()).clicked() {
            self.copy_character_sheet(id);
        }
        if action(ui, "Save sheet…", references && self.job.is_none()).clicked() {
            self.export_character_sheet(id);
        }
        ui.separator();
        if action(
            ui,
            "Add references…",
            character.character.references.len() < 24,
        )
        .clicked()
        {
            self.character_reference_picker(id);
        }
    }

    pub(super) fn card_thumbnail_action(&mut self, ui: &mut egui::Ui, target: u64, image: u64) {
        let current = scroll_studio::thumbnails::selected(&self.story, target) == Some(image);
        if action(
            ui,
            if current {
                "Card thumbnail"
            } else {
                "Use as thumbnail"
            },
            !current,
        )
        .clicked()
        {
            self.checkpoint();
            if scroll_studio::thumbnails::set(&mut self.story, target, image) {
                self.changed();
            }
        }
    }

    pub(super) fn reference_context_actions(
        &mut self,
        ui: &mut egui::Ui,
        character_id: u64,
        reference_id: u64,
    ) {
        let Some(character) = self
            .story
            .assets
            .iter()
            .find(|asset| asset.id == character_id && asset.kind == AssetKind::Character)
            .cloned()
        else {
            return;
        };
        if !scroll_studio::characters::reference_ids(&character).contains(&reference_id) {
            return;
        }
        self.card_thumbnail_action(ui, character_id, reference_id);
        if !self.assets_only && action(ui, "Place", true).clicked() {
            self.place_character_reference(character_id, reference_id);
        }
        if action(ui, "Copy image", self.assets.contains_key(&reference_id)).clicked() {
            self.copy_asset_image(reference_id);
        }
        if action(ui, "Pin reference", self.can_pin_reference(reference_id)).clicked() {
            self.pin_reference(ui.ctx(), reference_id, Some(character_id));
        }
        if action(ui, "Copy sheet", self.job.is_none()).clicked() {
            self.copy_character_sheet(character_id);
        }
        if action(ui, "Save sheet…", self.job.is_none()).clicked() {
            self.export_character_sheet(character_id);
        }
        if character
            .character
            .references
            .iter()
            .any(|reference| reference.asset_id == reference_id)
        {
            ui.separator();
            if action(ui, "Remove reference", true).clicked() {
                self.checkpoint();
                if let Some(character) = self
                    .story
                    .assets
                    .iter_mut()
                    .find(|asset| asset.id == character_id)
                {
                    character
                        .character
                        .references
                        .retain(|reference| reference.asset_id != reference_id);
                }
                self.changed();
            }
        }
    }

    pub(super) fn preset_context_actions(&mut self, ui: &mut egui::Ui, id: u64) {
        if !self
            .story
            .bubble_presets
            .iter()
            .any(|preset| preset.id == id)
        {
            return;
        }
        if action(ui, "Use in chapter", true).clicked() {
            self.selected_bubble_preset = Some(id);
            self.tab = LibraryTab::Chapters;
            self.selected = None;
            self.tool = Tool::Text;
        }
        if action(ui, "Duplicate", self.story.bubble_presets.len() < 4096).clicked() {
            self.selected_bubble_preset = Some(id);
            self.add_bubble_preset(true);
        }
        let used = self.story.chapters.iter().flat_map(|chapter| &chapter.layers).any(|layer| matches!(&layer.kind, LayerKind::Text { style, .. } if style.preset_id == Some(id)));
        if action(ui, "Delete", !used && self.story.bubble_presets.len() > 1).clicked() {
            self.checkpoint();
            self.story.bubble_presets.retain(|preset| preset.id != id);
            if self.selected_bubble_preset == Some(id) {
                self.selected_bubble_preset = None;
            }
            self.changed();
        }
    }

    pub(super) fn location_context_actions(&mut self, ui: &mut egui::Ui, id: Option<u64>) {
        if id.is_some_and(|id| {
            !self
                .story
                .locations
                .iter()
                .any(|location| location.id == id)
        }) {
            return;
        }
        if action(ui, "Import images here…", self.story.assets.len() < 4096).clicked() {
            self.tab = LibraryTab::Backgrounds;
            self.selected_location = id;
            self.import_picker();
        }
        if id.is_some() && action(ui, "Rename", true).clicked() {
            self.begin_location_editor(id);
        }
        let folder = egui::Id::new(("background_location", self.document_epoch, id));
        let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(
            ui.ctx(),
            folder,
            true,
        );
        if action(
            ui,
            if state.is_open() {
                "Collapse"
            } else {
                "Expand"
            },
            true,
        )
        .clicked()
        {
            state.set_open(!state.is_open());
            state.store(ui.ctx());
        }
        ui.separator();
        if action(
            ui,
            "New location…",
            self.story.locations.len() < scroll_studio::locations::MAX_LOCATIONS,
        )
        .clicked()
        {
            self.begin_location_editor(None);
        }
    }

    pub(super) fn chapter_context_actions(&mut self, ui: &mut egui::Ui, id: u64) {
        let Some(index) = self
            .story
            .chapters
            .iter()
            .position(|chapter| chapter.id == id)
        else {
            return;
        };
        if action(ui, "Open chapter", true).clicked() {
            self.chapter = index;
            self.tab = LibraryTab::Chapters;
            self.selected = None;
            self.selected_asset = None;
            self.jump_y = Some(0.0);
        }
        if action(ui, "Export chapter…", self.job.is_none()).clicked() {
            self.chapter = index;
            self.export_dialog = true;
        }
        if action(ui, "Read", true).clicked() {
            self.chapter = index;
            self.begin_reader();
        }
        if action(ui, "Spacing…", true).clicked() {
            self.begin_spacing(
                id,
                if index == self.chapter {
                    self.scroll_y
                } else {
                    0.0
                },
            );
        }
        ui.separator();
        if action(ui, "New chapter…", self.story.chapters.len() < 1000).clicked() {
            self.dialog_name = format!("Chapter {:02}", self.story.chapters.len() + 1);
            self.dialog = Some(Dialog::Chapter);
        }
    }

    pub(super) fn beat_context_actions(&mut self, ui: &mut egui::Ui, id: u64) {
        let Some(beat) = self.story.chapters[self.chapter]
            .panels
            .iter()
            .find(|beat| beat.id == id)
            .cloned()
        else {
            return;
        };
        if action(ui, "Make bubbles…", !beat.script.trim().is_empty()).clicked() {
            self.begin_dialogue_setup(id);
        }
        if action(
            ui,
            "Copy prompt",
            !beat.prompt.trim().is_empty() || !self.story.visual_style.trim().is_empty(),
        )
        .clicked()
        {
            ui.ctx()
                .copy_text(scroll_studio::characters::with_story_style(
                    &self.story.visual_style,
                    &beat.prompt,
                ));
        }
        if action(ui, "Go to beat", true).clicked() {
            self.jump_y = Some(beat.y);
            self.active_panel = Some(id);
            self.script_lane.reveal = Some(id);
        }
        if action(ui, "Spacing before beat…", true).clicked() {
            self.begin_spacing(self.story.chapters[self.chapter].id, beat.y);
        }
        ui.separator();
        if action(ui, "Remove beat", true).clicked() {
            self.checkpoint();
            self.story.chapters[self.chapter]
                .panels
                .retain(|beat| beat.id != id);
            if self.active_panel == Some(id) {
                self.active_panel = None;
            }
            self.changed();
        }
    }

    pub(super) fn canvas_context_actions(&mut self, ui: &mut egui::Ui) {
        let Some(target) = &self.context.canvas else {
            return;
        };
        if target.chapter != self.story.chapters[self.chapter].id {
            return;
        }
        let point = target.point;
        if let Some(id) = target.layer {
            self.layer_context_actions(ui, id);
            ui.separator();
            if action(ui, "Spacing here…", true).clicked() {
                self.begin_spacing(self.story.chapters[self.chapter].id, point[1]);
            }
            return;
        }
        if action(ui, "Paste image", true).clicked() {
            self.paste_image_at(Some(point));
        }
        if action(ui, "Import images…", true).clicked() {
            self.import_picker();
        }
        if action(
            ui,
            "Add script beat here",
            self.story.chapters[self.chapter].panels.len() < 10_000,
        )
        .clicked()
        {
            self.add_beat(point[1]);
        }
        ui.separator();
        if action(ui, "Spacing here…", true).clicked() {
            self.begin_spacing(self.story.chapters[self.chapter].id, point[1]);
        }
        if action(ui, "Fit canvas", true).clicked() {
            self.auto_fit = true;
        }
        if action(ui, "Read", true).clicked() {
            self.begin_reader();
        }
        if action(
            ui,
            if self.show_script {
                "Hide script"
            } else {
                "Show script"
            },
            true,
        )
        .clicked()
        {
            self.show_script = !self.show_script;
        }
    }

    fn app_context_actions(&mut self, ui: &mut egui::Ui) {
        if !self.welcome {
            if action(ui, "Undo", !self.undo.is_empty()).clicked() {
                self.undo();
            }
            if action(ui, "Redo", !self.redo.is_empty()).clicked() {
                self.redo();
            }
            ui.separator();
            if !self.settings_open {
                match self.tab {
                    LibraryTab::Characters => {
                        if action(ui, "New character…", self.story.assets.len() < 4096).clicked()
                        {
                            self.dialog_name.clear();
                            self.dialog = Some(Dialog::Asset(AssetKind::Character));
                        }
                    }
                    LibraryTab::Backgrounds => {
                        if action(
                            ui,
                            "New location…",
                            self.story.locations.len() < scroll_studio::locations::MAX_LOCATIONS,
                        )
                        .clicked()
                        {
                            self.begin_location_editor(None);
                        }
                        if action(ui, "Import images…", self.story.assets.len() < 4096).clicked()
                        {
                            self.import_picker();
                        }
                    }
                    LibraryTab::Groups => {
                        if action(
                            ui,
                            "New group",
                            self.story.groups.len() < scroll_studio::groups::MAX_GROUPS,
                        )
                        .clicked()
                        {
                            self.new_group();
                        }
                        if action(ui, "Add references…", self.selected_group.is_some()).clicked()
                        {
                            self.import_picker();
                        }
                    }
                    LibraryTab::Lore => {
                        if action(ui, "New entry", self.story.lore.len() < 4096).clicked() {
                            self.new_lore();
                        }
                    }
                    LibraryTab::Timeline => {
                        if !self.assets_only
                            && action(ui, "New chapter…", self.story.chapters.len() < 1000)
                                .clicked()
                        {
                            self.dialog = Some(Dialog::Chapter);
                            self.dialog_name =
                                format!("Chapter {:02}", self.story.chapters.len() + 1);
                        }
                    }
                    LibraryTab::Bubbles => {
                        if action(
                            ui,
                            "New bubble preset",
                            self.story.bubble_presets.len() < 4096,
                        )
                        .clicked()
                        {
                            self.add_bubble_preset(false);
                        }
                    }
                    _ => {
                        if action(ui, "Paste image", self.story.assets.len() < 4096).clicked() {
                            self.paste_image();
                        }
                        if action(ui, "Import images…", self.story.assets.len() < 4096).clicked()
                        {
                            self.import_picker();
                        }
                    }
                }
                ui.separator();
            }
            if action(ui, "Save", self.job.is_none()).clicked() {
                self.save(false);
            }
            if action(ui, "Story style", true).clicked() {
                self.style_prompt_open = true;
            }
        } else {
            if action(
                ui,
                if self.assets_only {
                    "New world…"
                } else {
                    "New story…"
                },
                self.job.is_none(),
            )
            .clicked()
            {
                self.request(Pending::New);
            }
            if action(
                ui,
                if self.assets_only {
                    "Open world…"
                } else {
                    "Open story…"
                },
                self.job.is_none(),
            )
            .clicked()
            {
                self.request(Pending::Open(None));
            }
        }
        ui.separator();
        if action(ui, "Settings", true).clicked() {
            self.settings_open = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn events(pos: egui::Pos2, button: egui::PointerButton) -> Vec<egui::Event> {
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::PointerButton {
                pos,
                button,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            },
        ]
    }

    fn frame(
        ctx: &egui::Context,
        studio: &mut Studio,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        let mut frame = eframe::Frame::_new_kittest();
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280.0, 800.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| eframe::App::update(studio, ctx, &mut frame),
        )
    }

    fn fixture() -> (egui::Context, Studio, [u64; 2]) {
        let ctx = egui::Context::default();
        let mut studio = Studio::from_context(&ctx);
        studio.story = Story::new("Context actions");
        studio.welcome = false;
        studio.error = None;
        studio.preferences.motion = settings::Motion::Off;
        studio.preferences.glass_enabled = false;
        studio.show_script = false;
        let ids = [studio.story.alloc_id(), studio.story.alloc_id()];
        for (index, id) in ids.into_iter().enumerate() {
            studio.story.chapters[0].layers.push(Layer {
                id,
                name: format!("Shape {index}"),
                visible: true,
                locked: index == 1,
                opacity: 1.0,
                x: 150.0 + index as f32 * 400.0,
                y: 200.0,
                width: 250.0,
                height: 250.0,
                kind: LayerKind::Shape {
                    color: [120, 150, 170, 255],
                },
            });
        }
        studio.selected = Some(ids[0]);
        let _ = frame(&ctx, &mut studio, vec![]);
        let _ = frame(&ctx, &mut studio, vec![]);
        (ctx, studio, ids)
    }

    fn control(ctx: &egui::Context, label: &str) -> (egui::Rect, bool) {
        ctx.data(|data| {
            data.get_temp(egui::Id::new(("context_action", label)))
                .unwrap()
        })
    }

    #[test]
    fn beat_context_area_does_not_intercept_its_menu_or_script_editor() {
        let (ctx, mut studio, _) = fixture();
        studio.show_script = true;
        let id = studio.story.chapters[0].panels[0].id;
        studio.story.chapters[0].panels[0].script = "A line of dialogue".into();
        for _ in 0..3 {
            frame(&ctx, &mut studio, Vec::new());
        }
        let menu = ctx
            .data(|data| data.get_temp::<egui::Rect>(egui::Id::new(("beat_menu_bounds", id))))
            .unwrap();
        for pressed in [true, false] {
            frame(
                &ctx,
                &mut studio,
                vec![
                    egui::Event::PointerMoved(menu.center()),
                    egui::Event::PointerButton {
                        pos: menu.center(),
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        let output = frame(&ctx, &mut studio, Vec::new());
        assert!(
            output
                .shapes
                .iter()
                .any(|shape| text_rect(&shape.shape, "Make bubbles…").is_some()),
            "Beat overflow must receive the click"
        );
        frame(
            &ctx,
            &mut studio,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        frame(&ctx, &mut studio, Vec::new());
        let script = ctx
            .data(|data| data.get_temp::<egui::Rect>(egui::Id::new(("beat_script_bounds", id))))
            .unwrap();
        for pressed in [true, false] {
            frame(
                &ctx,
                &mut studio,
                vec![
                    egui::Event::PointerMoved(script.center()),
                    egui::Event::PointerButton {
                        pos: script.center(),
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        frame(&ctx, &mut studio, vec![egui::Event::Text(" EDITED".into())]);
        assert!(
            studio.story.chapters[0].panels[0].script.contains("EDITED"),
            "Script editor must receive focus"
        );
    }

    fn text_rect(shape: &egui::Shape, label: &str) -> Option<egui::Rect> {
        match shape {
            egui::Shape::Text(text) if text.galley.job.text == label => {
                Some(text.visual_bounding_rect())
            }
            egui::Shape::Vec(shapes) => shapes.iter().find_map(|shape| text_rect(shape, label)),
            _ => None,
        }
    }

    #[test]
    fn gallery_context_open_cannot_copy_another_characters_scene_or_outfit() {
        let (ctx, mut studio, _) = fixture();
        let ids = [studio.story.alloc_id(), studio.story.alloc_id()];
        for (id, name, outfit) in [
            (ids[0], "Character A", "A private coat"),
            (ids[1], "Character B", "B uniform"),
        ] {
            studio.story.assets.push(Asset {
                id,
                name: name.into(),
                kind: AssetKind::Character,
                notes: String::new(),
                prompt: String::new(),
                width: 0,
                height: 0,
                character: CharacterProfile {
                    outfits: vec![CharacterOutfit {
                        name: outfit.into(),
                        description: String::new(),
                    }],
                    active_outfit: Some(0),
                    ..Default::default()
                },
            });
        }
        studio.tab = LibraryTab::Characters;
        studio.select_character(ids[0]);
        studio.character_prompt_kind = CharacterPromptKind::Scene;
        studio.character_scene = "A private pose at the station".into();
        studio.select_character(ids[0]);
        assert_eq!(
            studio.character_scene, "A private pose at the station",
            "Reselecting the same character preserves its scene"
        );
        studio.selected_asset = None;
        let _ = frame(&ctx, &mut studio, vec![]);
        let _ = frame(&ctx, &mut studio, vec![]);
        let cards: Vec<(u64, egui::Rect)> = ctx.data(|data| {
            data.get_temp(egui::Id::new("character_gallery_cards"))
                .unwrap()
        });
        let card = cards.iter().find(|(id, _)| *id == ids[1]).unwrap().1;
        let _ = frame(
            &ctx,
            &mut studio,
            events(card.center(), egui::PointerButton::Secondary),
        );
        let _ = frame(&ctx, &mut studio, vec![]);
        let open = control(&ctx, "Open sheet").0.center();
        let _ = frame(
            &ctx,
            &mut studio,
            events(open, egui::PointerButton::Primary),
        );
        assert_eq!(studio.character_selected_id(), Some(ids[1]));
        assert!(studio.character_scene.is_empty());
        let output = frame(&ctx, &mut studio, vec![]);
        let copy = output
            .shapes
            .iter()
            .find_map(|shape| text_rect(&shape.shape, "Copy prompt"))
            .unwrap()
            .center();
        let output = frame(
            &ctx,
            &mut studio,
            events(copy, egui::PointerButton::Primary),
        );
        let prompt = output
            .platform_output
            .commands
            .iter()
            .find_map(|command| match command {
                egui::OutputCommand::CopyText(text) => Some(text),
                _ => None,
            })
            .expect("Copy prompt did not write text");
        assert!(prompt.contains("Character B") && prompt.contains("B uniform"));
        assert!(!prompt.contains("A private pose") && !prompt.contains("A private coat"));
    }

    #[test]
    fn canvas_context_targets_the_right_clicked_locked_layer_and_unlocks_once() {
        let (ctx, mut studio, ids) = fixture();
        let original = studio.story.clone();
        let point = studio.canvas_rect.min + egui::vec2(650.0, 300.0) * studio.zoom;
        let _ = frame(
            &ctx,
            &mut studio,
            events(point, egui::PointerButton::Secondary),
        );
        let _ = frame(&ctx, &mut studio, vec![]);
        assert_eq!(
            studio.selected,
            Some(ids[1]),
            "point={point:?} toolbar={:?} canvas={:?} target={:?}",
            studio.toolbar_rect,
            studio.canvas_rect,
            studio
                .context
                .canvas
                .as_ref()
                .map(|target| (target.layer, target.point))
        );
        assert_eq!(studio.story, original);
        assert!(studio.undo.is_empty());
        assert!(context_menu_open(&ctx));
        assert!(!control(&ctx, "Delete").1);
        let _ = frame(
            &ctx,
            &mut studio,
            events(
                control(&ctx, "Unlock").0.center(),
                egui::PointerButton::Primary,
            ),
        );
        assert!(!studio.story.chapters[0].layers[1].locked);
        assert_eq!(
            studio.story.chapters[0].layers[0],
            original.chapters[0].layers[0]
        );
        assert_eq!(studio.undo.len(), 1);
        studio.undo();
        assert_eq!(studio.story, original);
    }

    #[test]
    fn escape_dismisses_context_without_deleting_selection_or_moving_artwork() {
        let (ctx, mut studio, ids) = fixture();
        let original = studio.story.clone();
        studio.tool = Tool::Brush;
        let point = studio.canvas_rect.min + egui::vec2(250.0, 300.0) * studio.zoom;
        let _ = frame(
            &ctx,
            &mut studio,
            events(point, egui::PointerButton::Secondary),
        );
        assert!(context_menu_open(&ctx));
        let _ = frame(
            &ctx,
            &mut studio,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert!(!context_menu_open(&ctx));
        assert_eq!(studio.selected, Some(ids[0]));
        assert_eq!(studio.story, original);
        assert!(studio.undo.is_empty());
    }

    #[test]
    fn empty_canvas_context_uses_opening_coordinates_and_closes_on_context_switch() {
        let (ctx, mut studio, _) = fixture();
        let point = studio.canvas_rect.min + egui::vec2(1000.0, 720.0) * studio.zoom;
        let _ = frame(
            &ctx,
            &mut studio,
            events(point, egui::PointerButton::Secondary),
        );
        let _ = frame(&ctx, &mut studio, vec![]);
        assert_eq!(studio.selected, None);
        let count = studio.story.chapters[0].panels.len();
        let _ = frame(
            &ctx,
            &mut studio,
            events(
                control(&ctx, "Add script beat here").0.center(),
                egui::PointerButton::Primary,
            ),
        );
        assert_eq!(studio.story.chapters[0].panels.len(), count + 1);
        let added = studio.story.chapters[0]
            .panels
            .iter()
            .find(|beat| Some(beat.id) == studio.active_panel)
            .unwrap();
        assert!((added.y - 720.0).abs() < 0.1);
        assert_eq!(studio.undo.len(), 1);
        let _ = frame(
            &ctx,
            &mut studio,
            events(point, egui::PointerButton::Secondary),
        );
        assert!(context_menu_open(&ctx));
        studio.tab = LibraryTab::Characters;
        let _ = frame(&ctx, &mut studio, vec![]);
        assert!(!context_menu_open(&ctx));
    }

    fn text_frame(
        ctx: &egui::Context,
        studio: &mut Studio,
        value: &mut String,
        readonly: bool,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, 600.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                studio.begin_context_frame(ctx);
                studio.shortcuts(ctx);
                studio.dispatch_text_context_command(ctx);
                egui::CentralPanel::default().show(ctx, |ui| {
                    let response = if readonly {
                        let mut text = value.as_str();
                        ui.add(
                            egui::TextEdit::multiline(&mut text).id(egui::Id::new("context_text")),
                        )
                    } else {
                        ui.add(egui::TextEdit::multiline(value).id(egui::Id::new("context_text")))
                    };
                    ui.data_mut(|data| {
                        data.insert_temp(egui::Id::new("context_text_bounds"), response.rect)
                    });
                });
                studio.finish_context_frame(ctx);
            },
        )
    }

    #[test]
    fn text_menu_keeps_selection_and_routes_cut_and_paste_to_the_editor_only() {
        let ctx = egui::Context::default();
        let mut studio = Studio::from_context(&ctx);
        studio.welcome = false;
        studio.error = None;
        let mut text = "Hello world".to_owned();
        let _ = text_frame(&ctx, &mut studio, &mut text, false, vec![]);
        let id = egui::Id::new("context_text");
        let range =
            egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(5));
        let mut state = egui::TextEdit::load_state(&ctx, id).unwrap();
        state.cursor.set_char_range(Some(range));
        state.store(&ctx, id);
        ctx.memory_mut(|memory| memory.request_focus(id));
        let rect: egui::Rect =
            ctx.data(|data| data.get_temp(egui::Id::new("context_text_bounds")).unwrap());
        let _ = text_frame(
            &ctx,
            &mut studio,
            &mut text,
            false,
            events(rect.center(), egui::PointerButton::Secondary),
        );
        let _ = text_frame(&ctx, &mut studio, &mut text, false, vec![]);
        assert!(context_menu_open(&ctx));
        assert!(
            control(&ctx, "Cut").1,
            "target={:?} cursor={:?}",
            studio
                .context
                .fallback
                .as_ref()
                .map(|menu| match &menu.kind {
                    FallbackKind::Text(target) => (target.editable, target.selection),
                    _ => (false, None),
                }),
            egui::TextEdit::load_state(&ctx, id)
                .unwrap()
                .cursor
                .char_range()
        );
        assert!(control(&ctx, "Paste").1);
        let _ = text_frame(
            &ctx,
            &mut studio,
            &mut text,
            false,
            events(
                control(&ctx, "Cut").0.center(),
                egui::PointerButton::Primary,
            ),
        );
        let output = text_frame(&ctx, &mut studio, &mut text, false, vec![]);
        assert_eq!(text, " world");
        assert!(output.platform_output.commands.iter().any(
            |command| matches!(command, egui::OutputCommand::CopyText(text) if text == "Hello")
        ));
        studio.context.text_command = Some((
            TextTarget {
                id,
                selection: Some(egui::text::CCursorRange::one(egui::text::CCursor::new(0))),
                editable: true,
            },
            egui::Event::Paste("New".into()),
        ));
        let _ = text_frame(&ctx, &mut studio, &mut text, false, vec![]);
        assert_eq!(text, "New world");
        assert!(studio.story.assets.is_empty());
        assert!(studio.error.is_none());
    }

    #[test]
    fn readonly_text_and_blank_space_get_distinct_menus() {
        let ctx = egui::Context::default();
        let mut studio = Studio::from_context(&ctx);
        studio.error = None;
        let mut text = "Preview text".to_owned();
        let _ = text_frame(&ctx, &mut studio, &mut text, true, vec![]);
        let rect: egui::Rect =
            ctx.data(|data| data.get_temp(egui::Id::new("context_text_bounds")).unwrap());
        let _ = text_frame(
            &ctx,
            &mut studio,
            &mut text,
            true,
            events(rect.center(), egui::PointerButton::Secondary),
        );
        let _ = text_frame(&ctx, &mut studio, &mut text, true, vec![]);
        assert!(!control(&ctx, "Cut").1);
        assert!(!control(&ctx, "Paste").1);
        let _ = text_frame(
            &ctx,
            &mut studio,
            &mut text,
            true,
            events(egui::pos2(600.0, 450.0), egui::PointerButton::Secondary),
        );
        let _ = text_frame(&ctx, &mut studio, &mut text, true, vec![]);
        assert!(
            ctx.data(|data| data
                .get_temp::<(egui::Rect, bool)>(egui::Id::new(("context_action", "Open story…"))))
                .is_some(),
            "clicked={:?} fallback={:?}",
            ctx.interaction_snapshot(|snapshot| snapshot.clicked),
            studio
                .context
                .fallback
                .as_ref()
                .map(|menu| (menu.response.id, matches!(menu.kind, FallbackKind::App)))
        );
        assert_eq!(text, "Preview text");
    }

    #[test]
    fn virtualized_source_disappearing_dismisses_its_context_menu() {
        let ctx = egui::Context::default();
        let mut studio = Studio::from_context(&ctx);
        studio.error = None;
        let mut draw = |show: bool, events: Vec<egui::Event>| {
            let _ = ctx.run(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ctx| {
                    studio.begin_context_frame(ctx);
                    egui::CentralPanel::default().show(ctx, |ui| {
                        if show {
                            let response = ui.button("A gallery card");
                            ui.data_mut(|data| {
                                data.insert_temp(egui::Id::new("orphan_source"), response.rect)
                            });
                            studio.register_context(&response);
                            appearance::context_menu(&response, |ui| {
                                action(ui, "Open sheet", true);
                            });
                        }
                    });
                    studio.finish_context_frame(ctx);
                },
            );
        };
        draw(true, vec![]);
        let rect: egui::Rect =
            ctx.data(|data| data.get_temp(egui::Id::new("orphan_source")).unwrap());
        draw(true, events(rect.center(), egui::PointerButton::Secondary));
        assert!(context_menu_open(&ctx));
        draw(false, vec![]);
        assert!(!context_menu_open(&ctx));
    }
}
