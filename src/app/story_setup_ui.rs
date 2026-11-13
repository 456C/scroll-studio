use super::*;

/// Temporary setup choices never alter the open document before Create or Skip.
pub(super) struct StorySetup {
    title: String,
    canvas_preset: usize,
    visual_style: String,
    synopsis: String,
    character_name: String,
    character_tagline: String,
    step: usize,
}

impl Default for StorySetup {
    fn default() -> Self {
        Self {
            title: "Untitled story".into(),
            canvas_preset: 0,
            visual_style: String::new(),
            synopsis: String::new(),
            character_name: String::new(),
            character_tagline: String::new(),
            step: 0,
        }
    }
}

const CANVASES: [(&str, u32, u32); 3] = [
    ("WEBTOON 2×", 1600, 9600),
    ("WEBTOON", 800, 6400),
    ("Tapas", 940, 8000),
];

impl StorySetup {
    fn build_story(&self, skip: bool) -> Story {
        let title = self.title.trim();
        let (_, width, height) = CANVASES[self.canvas_preset.min(CANVASES.len() - 1)];
        let mut story = Story::with_canvas(
            if title.is_empty() {
                "Untitled story"
            } else {
                title
            },
            width,
            height,
        );
        story.chapters[0].panels.clear();
        if !skip {
            story.synopsis = self.synopsis.trim().into();
            story.visual_style = self.visual_style.trim().into();
            if !self.character_name.trim().is_empty() {
                let id = story.alloc_id();
                story.assets.push(Asset {
                    extensions: Default::default(),
                    provenance: None,
                    id,
                    name: self.character_name.trim().into(),
                    kind: AssetKind::Character,
                    notes: String::new(),
                    prompt: String::new(),
                    width: 0,
                    height: 0,
                    character: CharacterProfile {
                        tagline: self.character_tagline.trim().into(),
                        ..CharacterProfile::default()
                    },
                });
            }
        }
        story
    }
}

#[derive(Clone, Copy, PartialEq)]
enum SetupAction {
    Continue,
    Create,
    Skip,
    Cancel,
}

impl Studio {
    pub(super) fn begin_story_setup(&mut self) {
        self.dialog = None;
        self.style_prompt_open = false;
        self.setup = Some(StorySetup::default());
    }

    pub(super) fn story_setup_ui(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.setup.take() else {
            return;
        };
        let mut action = SetupAction::Continue;
        egui::Modal::new(egui::Id::new("new_story_setup")).show(ctx, |ui| {
            ui.set_width((ctx.screen_rect().width() - 64.0).clamp(300.0, 520.0));
            ui.heading("New story");
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                for (step, label) in ["1 · Story", "2 · Style", "3 · Character"]
                    .into_iter()
                    .enumerate()
                {
                    if ui.selectable_label(draft.step == step, label).clicked() {
                        draft.step = step;
                    }
                }
            });
            ui.separator();
            ui.allocate_ui(egui::vec2(ui.available_width(), 330.0), |ui| {
                ui.set_min_height(330.0);
                ui.add_space(8.0);
                match draft.step {
                    0 => {
                        ui.label("Name");
                        ui.add(
                            egui::TextEdit::singleline(&mut draft.title)
                                .hint_text("Story name")
                                .char_limit(1024)
                                .desired_width(f32::INFINITY),
                        );
                        ui.add_space(6.0);
                        ui.label("Canvas");
                        for (index, (name, width, height)) in CANVASES.iter().enumerate() {
                            ui.radio_value(
                                &mut draft.canvas_preset,
                                index,
                                format!("{name}   {width} × {height} px"),
                            );
                        }
                        ui.add_space(6.0);
                        ui.label(RichText::new("Notes · optional").color(MUTED));
                        fixed_text_area(
                            ui,
                            "setup_notes",
                            &mut draft.synopsis,
                            74.0,
                            "Premise or notes",
                        );
                    }
                    1 => {
                        ui.label("Visual style");
                        style_starters(ui, &mut draft.visual_style);
                        fixed_text_area(
                            ui,
                            "setup_style",
                            &mut draft.visual_style,
                            238.0,
                            "Linework, palette, lighting, mood…",
                        );
                    }
                    _ => {
                        ui.label("First character · optional");
                        ui.add(
                            egui::TextEdit::singleline(&mut draft.character_name)
                                .hint_text("Character name")
                                .char_limit(1024)
                                .desired_width(f32::INFINITY),
                        );
                        fixed_text_area(
                            ui,
                            "setup_character",
                            &mut draft.character_tagline,
                            212.0,
                            "Concept or signature details",
                        );
                    }
                }
            });
            ui.separator();
            ui.add_enabled_ui(self.job.is_none(), |ui| {
                ui.horizontal_wrapped(|ui| {
                    if motion::button(ui, "Cancel").clicked() {
                        action = SetupAction::Cancel;
                    }
                    if motion::button(ui, "Start blank")
                        .on_hover_text("Skip setup; keep only the name and canvas.")
                        .clicked()
                    {
                        action = SetupAction::Skip;
                    }
                    if draft.step > 0 && motion::button(ui, "Back").clicked() {
                        draft.step -= 1;
                    }
                    if draft.step < 2 {
                        if primary(ui, "Next").clicked() {
                            draft.step += 1;
                        }
                    } else if primary(ui, "Create story").clicked() {
                        action = SetupAction::Create;
                    }
                });
            });
        });
        self.finish_story_setup(draft, action);
    }

    fn finish_story_setup(&mut self, draft: StorySetup, action: SetupAction) {
        match action {
            SetupAction::Continue => self.setup = Some(draft),
            SetupAction::Cancel => {}
            SetupAction::Create | SetupAction::Skip => {
                let story = draft.build_story(action == SetupAction::Skip);
                let character = story.assets.first().map(|asset| asset.id);
                self.story = story;
                self.assets.clear();
                self.path = None;
                self.reset_view();
                self.welcome = false;
                if let Some(id) = character {
                    self.tab = LibraryTab::Characters;
                    self.selected_asset = Some(id);
                }
                self.status = "Story created".into();
                self.changed();
            }
        }
    }

    pub(super) fn style_prompt_ui(&mut self, ctx: &egui::Context) {
        if !self.style_prompt_open || self.welcome || self.setup.is_some() {
            return;
        }
        let mut open = true;
        let original = self.story.visual_style.clone();
        let mut prompt = original.clone();
        egui::Window::new("Story style")
            .id(egui::Id::new("story_visual_style"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(520.0)
            .show(ctx, |ui| {
                style_starters(ui, &mut prompt);
                fixed_text_area(
                    ui,
                    "story_style_text",
                    &mut prompt,
                    (ctx.screen_rect().height() - 260.0).clamp(180.0, 320.0),
                    "Linework, palette, lighting, mood…",
                );
                // Laid out as before unless an addon adds its star beside the button.
                let copy_style = |ui: &mut egui::Ui, studio: &mut Studio| {
                    if ui
                        .add_enabled_ui(!prompt.trim().is_empty(), |ui| {
                            motion::copy_button(
                                ui,
                                ("copy_style_prompt", studio.document_epoch),
                                "Copy style",
                                true,
                            )
                        })
                        .inner
                        .clicked()
                    {
                        ctx.copy_text(prompt.clone());
                        studio.status = "Style copied".into();
                    }
                };
                if self.addons.assistant().is_some() {
                    ui.horizontal(|ui| {
                        copy_style(ui, self);
                        self.assist_star(ui, serde_json::json!({ "kind": "story" }));
                    });
                } else {
                    copy_style(ui, self);
                }
            });
        self.style_prompt_open = open;
        if prompt != original {
            let command = Command::SetField {
                target: Target::Story,
                field: Field::VisualStyle,
                value: prompt.clone(),
            };
            self.commit(Some(command), |studio| studio.story.visual_style = prompt);
        }
    }
}

fn style_starters(ui: &mut egui::Ui, prompt: &mut String) {
    motion::menu_button(ui, "Templates", |ui| {
        for (label, text) in [
                ("Cinematic", "Cinematic webtoon illustration, confident clean linework, grounded anatomy, expressive faces, muted teal shadows and warm amber highlights, dramatic but readable lighting, consistent character proportions, clear focal point."),
                ("Soft color", "Expressive webtoon illustration, soft clean contours, gentle pastel colors, warm natural light, subtle painted textures, readable silhouettes, expressive faces, consistent character proportions, uncluttered composition."),
                ("Bold ink", "Bold inked webtoon illustration, crisp variable-weight lines, simple cel shading, limited high-contrast palette, strong silhouettes, expressive faces, dynamic composition, consistent character proportions, clean readable details."),
            ] {
                if motion::button(ui, label).clicked() { *prompt = text.into(); ui.close_menu(); }
            }
    });
}

fn fixed_text_area(ui: &mut egui::Ui, id: &str, text: &mut String, height: f32, hint: &str) {
    egui::ScrollArea::vertical()
        .id_salt(id)
        .max_height(height)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add_sized(
                [ui.available_width(), height],
                egui::TextEdit::multiline(text)
                    .hint_text(hint)
                    .char_limit(100_000)
                    .desired_width(f32::INFINITY),
            );
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn completed_draft() -> StorySetup {
        StorySetup {
            title: "  Night train  ".into(),
            canvas_preset: 2,
            visual_style: "Muted teal, warm amber, crisp contours.".into(),
            synopsis: "Two strangers at the final station.".into(),
            character_name: "  Mira  ".into(),
            character_tagline: "Short dark hair and a charcoal coat.".into(),
            step: 2,
        }
    }

    #[test]
    fn cancelling_setup_preserves_the_existing_story_assets_and_save_destination() {
        let ctx = egui::Context::default();
        let mut studio = Studio::from_context(&ctx);
        studio.story = Story::demo();
        studio.assets.insert(9999, Arc::new(vec![1, 2, 3]));
        studio.path = Some(PathBuf::from("Current story.scro"));
        studio.dirty = true;
        let original = studio.story.clone();
        let assets = studio.assets.clone();
        let path = studio.path.clone();
        let epoch = studio.document_epoch;
        studio.begin_story_setup();
        assert_eq!(studio.story, original);
        studio.setup = Some(completed_draft());
        let draft = studio.setup.take().unwrap();
        studio.finish_story_setup(draft, SetupAction::Cancel);
        assert!(studio.setup.is_none());
        assert_eq!(studio.story, original);
        assert_eq!(studio.assets, assets);
        assert_eq!(studio.path, path);
        assert_eq!(studio.document_epoch, epoch);
        assert!(studio.dirty);
    }

    #[test]
    fn opening_final_setup_step_does_not_commit_on_an_unfocused_enter_key() {
        let ctx = egui::Context::default();
        let mut studio = Studio::from_context(&ctx);
        let original = studio.story.clone();
        studio.setup = Some(completed_draft());
        for events in [
            Vec::new(),
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        ] {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1000.0, 900.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| studio.story_setup_ui(ctx),
            );
        }
        assert!(studio.setup.is_some());
        assert_eq!(studio.story, original);
    }

    #[test]
    fn guided_setup_saves_style_and_optional_character_with_clean_canvas() {
        let story = completed_draft().build_story(false);
        assert_eq!(story.title, "Night train");
        assert_eq!(
            (story.chapters[0].width, story.chapters[0].height),
            (940, 8000)
        );
        assert!(story.chapters[0].layers.is_empty() && story.chapters[0].panels.is_empty());
        assert_eq!(
            story.visual_style,
            "Muted teal, warm amber, crisp contours."
        );
        assert_eq!(story.assets.len(), 1);
        assert_eq!(story.assets[0].name, "Mira");
        assert_eq!(
            story.assets[0].character.tagline,
            "Short dark hair and a charcoal coat."
        );
        storage::validate_story(&story, &AssetBytes::new()).unwrap();
    }

    #[test]
    fn skipping_from_later_step_discards_optional_fields_but_keeps_name_and_format() {
        let story = completed_draft().build_story(true);
        assert_eq!(story.title, "Night train");
        assert_eq!(
            (story.chapters[0].width, story.chapters[0].height),
            (940, 8000)
        );
        assert!(
            story.visual_style.is_empty() && story.synopsis.is_empty() && story.assets.is_empty()
        );
        assert!(story.chapters[0].layers.is_empty() && story.chapters[0].panels.is_empty());
        assert_eq!(story.bubble_presets.len(), 3);
        storage::validate_story(&story, &AssetBytes::new()).unwrap();
    }

    #[test]
    fn every_setup_format_starts_with_matching_lettering_for_create_and_skip() {
        for (canvas_preset, (_, width, height)) in CANVASES.iter().enumerate() {
            for skip in [false, true] {
                let story = StorySetup {
                    canvas_preset,
                    ..StorySetup::default()
                }
                .build_story(skip);
                assert_eq!(
                    (story.chapters[0].width, story.chapters[0].height),
                    (*width, *height)
                );
                for preset in &story.bubble_presets {
                    assert!((preset.size * 800.0 / *width as f32 - 38.0).abs() < 0.001);
                }
                storage::validate_story(&story, &AssetBytes::new()).unwrap();
            }
        }
    }

    #[test]
    fn empty_character_name_does_not_create_an_incomplete_sheet() {
        let draft = StorySetup {
            character_tagline: "A character idea for later.".into(),
            ..StorySetup::default()
        };
        let story = draft.build_story(false);
        assert_eq!(story.title, "Untitled story");
        assert!(story.assets.is_empty());
    }
}
