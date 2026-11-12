mod appearance;
mod artwork_color;
mod bubble_presets_ui;
mod bubble_preview_ui;
mod camera_ui;
mod canvas;
mod character_io;
mod characters_ui;
mod context_ui;
mod cutout_ui;
mod dialogue_ui;
mod editing;
mod effects_ui;
mod export_ui;
mod glass;
mod groups_ui;
mod healing_ui;
mod icons;
mod lasso_ui;
mod layers_ui;
mod lettering_ui;
mod location_workspace;
mod locations_ui;
mod lore_ui;
mod motion;
mod paint_ui;
mod panels;
mod previews;
mod reader_ui;
mod recovery_ui;
mod reference_ui;
mod script_ui;
mod settings;
mod settings_ui;
mod snippet_ui;
mod spacing_ui;
mod startup_ui;
mod story_setup_ui;
mod timeline_ui;

use eframe::egui::{self, Color32, RichText};
use scroll_studio::{
    commands::{self, Applied, Command, Field, Target},
    compositor, export,
    model::*,
    recovery, storage,
};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::PathBuf,
    sync::{mpsc, Arc},
    time::Instant,
};

/// How many `story.changed` notices wait for a reader before the oldest are dropped.
const MAX_STORY_CHANGES: usize = 256;

pub(super) const BG: Color32 = Color32::from_rgb(32, 33, 36);
pub(super) const PANEL: Color32 = Color32::from_rgb(41, 43, 47);
pub(super) const CARD: Color32 = Color32::from_rgb(52, 55, 60);
pub(super) const TEXT: Color32 = Color32::from_rgb(233, 234, 236);
pub(super) const MUTED: Color32 = Color32::from_rgb(157, 163, 173);
pub(super) const ACCENT: Color32 = Color32::from_rgb(122, 184, 245);

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Tool {
    Move,
    Select,
    Crop,
    Lasso,
    SmartLasso,
    MagicEraser,
    Brush,
    Heal,
    Blur,
    Glint,
    Wind,
    Text,
    Panel,
}
#[derive(Clone, Copy, PartialEq)]
pub(super) enum LibraryTab {
    Chapters,
    Bubbles,
    Characters,
    Backgrounds,
    Groups,
    Timeline,
    Lore,
    Images,
}
#[derive(Clone)]
enum Pending {
    Home,
    New,
    Open(Option<PathBuf>),
    Quit,
    Recover(recovery::RecoveryEntry),
}
enum Dialog {
    World,
    Chapter,
    Asset(AssetKind),
    Unsaved,
}
enum JobResult {
    Saved(PathBuf, u64, u64, storage::FileStamp),
    Exported(export::ExportReport),
    CharacterSheetDone(String),
    Healed(healing_ui::HealingResult),
    BackingColor(cutout_ui::SceneColorResult),
}

pub struct Studio {
    story: Story,
    assets: AssetBytes,
    chapter: usize,
    tab: LibraryTab,
    selected: Option<u64>,
    selected_asset: Option<u64>,
    selected_location: Option<u64>,
    selected_group: Option<u64>,
    startup: Option<startup_ui::Startup>,
    location_editor: Option<locations_ui::LocationEditor>,
    dialogue_draft: Option<dialogue_ui::DialogueDraft>,
    spacing_draft: Option<spacing_ui::SpacingDraft>,
    context: context_ui::ContextState,
    reader: Option<reader_ui::ReaderView>,
    pinned_reference: Option<reference_ui::PinnedReference>,
    snippet: Option<snippet_ui::SnippetPicker>,
    camera_dialog: Option<camera_ui::CameraDialog>,
    reference_rect: egui::Rect,
    selected_bubble_preset: Option<u64>,
    bubble_preview: bubble_preview_ui::BubblePreview,
    bubble_preset_name: Option<(u64, String)>,
    preset_slider_edit: Option<bubble_presets_ui::PresetSliderEdit>,
    block_preset_slider_until_release: bool,
    character_prompt_kind: CharacterPromptKind,
    character_scene: String,
    style_prompt_open: bool,
    setup: Option<story_setup_ui::StorySetup>,
    active_panel: Option<u64>,
    script_focus: Option<u64>,
    script_lane: script_ui::ScriptLane,
    tool: Tool,
    zoom: f32,
    auto_fit: bool,
    brush_size: f32,
    heal_brush_size: f32,
    brush_color: Color32,
    motion_brush: effects_ui::MotionBrush,
    previews: previews::PreviewCache,
    undo: VecDeque<Story>,
    redo: Vec<Story>,
    dirty: bool,
    revision: u64,
    /// `story.changed` notices, oldest first. Nothing reads them in 0.14.0 without addons;
    /// the addon host drains them. Bounded so an app without addons never grows it.
    story_changes: VecDeque<commands::StoryChanged>,
    /// The scope of the change `changed` is about to record, when a command knows it.
    pending_scope: Option<commands::ChangeScope>,
    document_epoch: u64,
    path: Option<PathBuf>,
    file_stamp: Option<storage::FileStamp>,
    welcome: bool,
    assets_only: bool,
    paint: Option<paint_ui::PaintStudio>,
    selected_lore: Option<u64>,
    lore_view: lore_ui::BoardView,
    status: String,
    error: Option<String>,
    dialog: Option<Dialog>,
    dialog_name: String,
    preset: usize,
    pending: Option<Pending>,
    continue_after_save: bool,
    allow_close: bool,
    job: Option<mpsc::Receiver<anyhow::Result<JobResult>>>,
    busy_label: String,
    lasso: Vec<[f32; 2]>,
    lasso_state: lasso_ui::LassoState,
    drag_layer: Option<u64>,
    drag_tail: Option<(u64, usize)>,
    resize_anchor: Option<(u64, [f32; 2], f32)>,
    snap_settings: scroll_studio::snapping::SnapSettings,
    snap_drag: Option<canvas::SnapDrag>,
    snap_guides: Vec<scroll_studio::snapping::Guide>,
    canvas_rect: egui::Rect,
    scroll_y: f32,
    jump_y: Option<f32>,
    show_script: bool,
    search: String,
    frame_started: Instant,
    recovery: recovery_ui::RecoveryState,
    export_options: export::ExportOptions,
    export_dialog: bool,
    export_report: Option<export::ExportReport>,
    settings_open: bool,
    preferences: settings::Preferences,
    preference_path: Option<PathBuf>,
    preferences_changed_at: Option<Instant>,
    glass: glass::GlassRenderer,
    toolbar_rect: egui::Rect,
    app_icon: egui::TextureHandle,
}

impl Studio {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut studio = Self::from_context(&cc.egui_ctx);
        studio.preference_path = settings::default_path();
        if let Some(path) = &studio.preference_path {
            match settings::load(path) {
                Ok(preferences) => studio.preferences = preferences,
                Err(error) => studio.status = format!("Settings could not be loaded: {error}"),
            }
        }
        studio.snap_settings = studio.preferences.snap_settings();
        studio.apply_preferences(&cc.egui_ctx);
        studio.startup = Some(startup_ui::Startup::new(&cc.egui_ctx));
        studio
    }

    fn from_context(ctx: &egui::Context) -> Self {
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "karla".into(),
            Arc::new(egui::FontData::from_static(include_bytes!(
                "../assets/Karla.ttf"
            ))),
        );
        fonts
            .families
            .get_mut(&egui::FontFamily::Proportional)
            .unwrap()
            .insert(0, "karla".into());
        for choice in [
            FontChoice::Lato,
            FontChoice::BalsamiqSans,
            FontChoice::KomikaHand,
        ] {
            let name = choice.label().to_owned();
            fonts.font_data.insert(
                name.clone(),
                Arc::new(egui::FontData::from_static(compositor::font_bytes(choice))),
            );
            fonts.families.insert(
                egui::FontFamily::Name(name.clone().into()),
                vec![name, "karla".into()],
            );
        }
        fonts
            .families
            .get_mut(&egui::FontFamily::Proportional)
            .unwrap()
            .insert(0, "Lato".into());
        ctx.set_fonts(fonts);
        let mut style = (*ctx.style()).clone();
        style.visuals = egui::Visuals::dark();
        style.visuals.panel_fill = PANEL;
        style.visuals.window_fill = PANEL;
        style.visuals.extreme_bg_color = BG;
        style.visuals.faint_bg_color = CARD;
        style.visuals.override_text_color = Some(TEXT);
        style.visuals.selection.bg_fill = Color32::from_rgb(49, 69, 91);
        style.visuals.selection.stroke = egui::Stroke::new(1.0_f32, ACCENT);
        style.visuals.widgets.inactive.bg_fill = CARD;
        style.visuals.widgets.inactive.weak_bg_fill = CARD;
        style.visuals.widgets.inactive.bg_stroke =
            egui::Stroke::new(1.0_f32, Color32::from_rgb(65, 68, 74));
        style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(62, 66, 73);
        style.visuals.widgets.active.bg_fill = Color32::from_rgb(55, 77, 101);
        style.visuals.window_stroke = egui::Stroke::new(1.0_f32, Color32::from_rgb(76, 80, 87));
        style.visuals.window_corner_radius = 12.into();
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(10.0, 6.0);
        style.visuals.widgets.inactive.corner_radius = 8.into();
        style.visuals.widgets.hovered.corner_radius = 8.into();
        style.visuals.widgets.active.corner_radius = 8.into();
        style
            .text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(14.0));
        style
            .text_styles
            .insert(egui::TextStyle::Button, egui::FontId::proportional(13.0));
        style
            .text_styles
            .insert(egui::TextStyle::Heading, egui::FontId::proportional(22.0));
        ctx.set_style(style);
        let icon = image::load_from_memory(include_bytes!("../assets/scroll-studio-icon.png"))
            .expect("bundled app icon")
            .into_rgba8();
        let app_icon = ctx.load_texture(
            "Scroll Studio icon",
            egui::ColorImage::from_rgba_unmultiplied(
                [icon.width() as usize, icon.height() as usize],
                icon.as_raw(),
            ),
            egui::TextureOptions::LINEAR,
        );
        let demo = std::env::args().any(|a| a == "--demo");
        let story = if demo {
            Story::demo()
        } else {
            Story::new("Untitled story")
        };
        let studio = Self {
            story,
            assets: HashMap::new(),
            chapter: 0,
            tab: LibraryTab::Chapters,
            selected: None,
            selected_asset: None,
            selected_location: None,
            selected_group: None,
            startup: None,
            location_editor: None,
            dialogue_draft: None,
            spacing_draft: None,
            context: context_ui::ContextState::default(),
            reader: None,
            pinned_reference: None,
            snippet: None,
            camera_dialog: None,
            reference_rect: egui::Rect::NOTHING,
            selected_bubble_preset: None,
            bubble_preview: bubble_preview_ui::BubblePreview::default(),
            bubble_preset_name: None,
            preset_slider_edit: None,
            block_preset_slider_until_release: false,
            character_prompt_kind: CharacterPromptKind::Turnaround,
            character_scene: String::new(),
            style_prompt_open: false,
            setup: None,
            active_panel: None,
            script_focus: None,
            script_lane: script_ui::ScriptLane::default(),
            tool: Tool::Move,
            zoom: 0.0,
            auto_fit: true,
            brush_size: 7.0,
            heal_brush_size: 48.0,
            motion_brush: effects_ui::MotionBrush::default(),
            brush_color: Color32::from_rgb(35, 38, 48),
            previews: previews::PreviewCache::new(),
            undo: VecDeque::new(),
            redo: Vec::new(),
            dirty: demo,
            revision: 0,
            story_changes: VecDeque::new(),
            pending_scope: None,
            document_epoch: 0,
            path: None,
            file_stamp: None,
            welcome: !demo,
            assets_only: false,
            paint: None,
            selected_lore: None,
            lore_view: lore_ui::BoardView::default(),
            status: String::new(),
            error: None,
            dialog: None,
            dialog_name: String::new(),
            preset: 0,
            pending: None,
            continue_after_save: false,
            allow_close: false,
            job: None,
            busy_label: String::new(),
            lasso: Vec::new(),
            lasso_state: lasso_ui::LassoState::default(),
            drag_layer: None,
            drag_tail: None,
            resize_anchor: None,
            snap_settings: scroll_studio::snapping::SnapSettings::default(),
            snap_drag: None,
            snap_guides: Vec::new(),
            canvas_rect: egui::Rect::NOTHING,
            scroll_y: 0.0,
            jump_y: None,
            show_script: true,
            search: String::new(),
            frame_started: Instant::now(),
            recovery: recovery_ui::RecoveryState::new(),
            export_options: export::ExportOptions::default(),
            export_dialog: false,
            export_report: None,
            settings_open: false,
            preferences: settings::Preferences::default(),
            preference_path: None,
            preferences_changed_at: None,
            glass: glass::GlassRenderer::default(),
            toolbar_rect: egui::Rect::NOTHING,
            app_icon,
        };
        #[cfg(not(test))]
        let mut studio = studio;
        #[cfg(not(test))]
        if let Some(path) = std::env::args().skip(1).find(|a| !a.starts_with('-')) {
            studio.open_path(PathBuf::from(path));
        }
        #[cfg(not(test))]
        studio.scan_recovery(ctx, false);
        studio
    }

    fn checkpoint(&mut self) {
        let snapshot = self.story.clone();
        self.record_history(snapshot);
    }

    /// Push `previous` as one undo step. Shared by `checkpoint` and `apply_command`.
    fn record_history(&mut self, previous: Story) {
        self.preset_slider_edit = None;
        if self.undo.back() != Some(&previous) {
            self.undo.push_back(previous);
            if self.undo.len() > 48 {
                self.undo.pop_front();
            }
        }
        self.redo.clear();
        let retained = retained_asset_ids(&self.story, &self.undo, &self.redo);
        self.assets.retain(|id, _| retained.contains(id));
        // An abandoned redo branch may free IDs that the next edit reuses.
        self.previews.retain_assets(&retained);
    }

    /// Apply a named edit as one undo step. Undo still restores the whole-story snapshot,
    /// exactly as before; a command that fails or changes nothing leaves history untouched.
    /// The caller decides whether to report an error; sheet and board edits stay silent
    /// as they always have.
    fn apply_command(&mut self, command: Command) -> anyhow::Result<Applied> {
        let previous = self.story.clone();
        let applied = self.story.apply(command)?;
        if self.story != previous {
            self.record_history(previous);
            self.pending_scope = Some(commands::ChangeScope::Objects(applied.touched.clone()));
            self.changed();
        }
        Ok(applied)
    }

    /// Apply `command` as one undo step. When there is no command for an edit, or it is
    /// refused, `fallback` runs the 0.13.2 code path instead (snapshot, edit, changed), so
    /// moving an edit onto commands can never block something the app used to allow.
    fn commit(
        &mut self,
        command: Option<Command>,
        fallback: impl FnOnce(&mut Self),
    ) -> Option<Applied> {
        if let Some(Ok(applied)) = command.map(|command| self.apply_command(command)) {
            return Some(applied);
        }
        self.checkpoint();
        fallback(self);
        self.changed();
        None
    }

    /// Apply a command inside a gesture that already took its undo step (a card drag).
    fn apply_in_gesture(&mut self, command: Command) -> anyhow::Result<Applied> {
        let applied = self.story.apply(command)?;
        self.pending_scope = Some(commands::ChangeScope::Objects(applied.touched.clone()));
        self.changed();
        Ok(applied)
    }

    fn changed(&mut self) {
        scroll_studio::thumbnails::prune(&mut self.story);
        self.dirty = true;
        self.revision += 1;
        let scope = self
            .pending_scope
            .take()
            .unwrap_or(commands::ChangeScope::Everything);
        if self.story_changes.len() == MAX_STORY_CHANGES {
            // A reader that falls this far behind must re-read the whole story anyway.
            self.story_changes.pop_front();
        }
        self.story_changes.push_back(commands::StoryChanged {
            revision: self.revision,
            scope,
        });
    }

    /// Take the pending `story.changed` notices. Used by the addon host (stage 5).
    #[cfg_attr(not(test), allow(dead_code))]
    fn take_story_changes(&mut self) -> Vec<commands::StoryChanged> {
        self.story_changes.drain(..).collect()
    }

    fn undo(&mut self) {
        self.lore_view.cancel_gesture();
        self.interrupt_preset_slider_edit();
        if let Some(story) = self.undo.pop_back() {
            self.redo.push(std::mem::replace(&mut self.story, story));
            self.chapter = self
                .chapter
                .min(self.story.chapters.len().saturating_sub(1));
            self.drag_tail = None;
            self.drag_layer = None;
            self.resize_anchor = None;
            self.snap_drag = None;
            self.snap_guides.clear();
            self.previews.clear();
            self.bubble_preset_name = None;
            self.sanitize_location_selection();
            self.changed();
            self.retain_selection_after_history();
        }
    }

    fn redo(&mut self) {
        self.lore_view.cancel_gesture();
        self.interrupt_preset_slider_edit();
        if let Some(story) = self.redo.pop() {
            self.undo
                .push_back(std::mem::replace(&mut self.story, story));
            self.chapter = self
                .chapter
                .min(self.story.chapters.len().saturating_sub(1));
            self.drag_tail = None;
            self.drag_layer = None;
            self.resize_anchor = None;
            self.snap_drag = None;
            self.snap_guides.clear();
            self.previews.clear();
            self.bubble_preset_name = None;
            self.sanitize_location_selection();
            self.changed();
            self.retain_selection_after_history();
        }
    }

    fn request(&mut self, action: Pending) {
        if self.job.is_some() {
            self.status = "Wait for the current file operation to finish.".into();
            return;
        }
        self.pending = Some(action);
        if self.dirty && !self.welcome {
            self.dialog = Some(Dialog::Unsaved);
        } else {
            self.perform_pending();
        }
    }

    fn perform_pending(&mut self) {
        match self.pending.take() {
            Some(Pending::Home) => {
                self.story = Story::new("Untitled story");
                self.assets.clear();
                self.path = None;
                self.assets_only = false;
                self.reset_view();
                self.welcome = true;
                self.dirty = false;
                self.status.clear();
            }
            Some(Pending::New) if self.assets_only => {
                self.dialog_name.clear();
                self.dialog = Some(Dialog::World);
            }
            Some(Pending::New) => {
                self.begin_story_setup();
            }
            Some(Pending::Open(path)) => {
                if let Some(path) = path.or_else(|| {
                    rfd::FileDialog::new()
                        .add_filter("Scroll Studio story", &["scro"])
                        .pick_file()
                }) {
                    self.open_path(path);
                }
            }
            Some(Pending::Quit) => {
                self.retire_recovery();
                self.allow_close = true;
            }
            Some(Pending::Recover(entry)) => self.restore_recovery(entry),
            None => (),
        }
    }

    fn open_path(&mut self, path: PathBuf) {
        match storage::load_story_with_stamp(&path) {
            Ok((story, assets, stamp)) => {
                self.file_stamp = Some(stamp);
                self.story = story;
                self.assets = assets;
                self.path = Some(path);
                self.reset_view();
                self.dirty = false;
                self.welcome = false;
                self.status = "Opened".into();
            }
            Err(e) => self.error = Some(format!("Could not open the story: {e:#}")),
        }
    }

    fn reset_view(&mut self) {
        self.interrupt_preset_slider_edit();
        self.settings_open = false;
        if self.path.is_none() {
            self.file_stamp = None;
        }
        self.retire_recovery();
        self.document_epoch += 1;
        self.zoom = 0.0;
        self.auto_fit = true;
        self.chapter = 0;
        self.selected = None;
        self.selected_asset = None;
        self.selected_location = None;
        self.selected_group = None;
        self.location_editor = None;
        self.dialogue_draft = None;
        self.spacing_draft = None;
        self.reader = None;
        self.pinned_reference = None;
        self.snippet = None;
        self.camera_dialog = None;
        self.reference_rect = egui::Rect::NOTHING;
        self.selected_bubble_preset = None;
        self.bubble_preview = bubble_preview_ui::BubblePreview::default();
        self.bubble_preset_name = None;
        self.character_prompt_kind = CharacterPromptKind::Turnaround;
        self.character_scene.clear();
        self.style_prompt_open = false;
        self.setup = None;
        self.tab = if self.assets_only {
            LibraryTab::Characters
        } else {
            LibraryTab::Chapters
        };
        self.selected_lore = None;
        self.lore_view = lore_ui::BoardView::default();
        self.canvas_rect = egui::Rect::NOTHING;
        self.active_panel = None;
        self.script_focus = None;
        self.script_lane = script_ui::ScriptLane::default();
        self.previews.clear();
        self.undo.clear();
        self.redo.clear();
        self.jump_y = Some(0.0);
        self.lasso.clear();
        self.lasso_state.cancel();
        self.drag_layer = None;
        self.drag_tail = None;
        self.resize_anchor = None;
        self.snap_drag = None;
        self.snap_guides.clear();
    }

    fn save(&mut self, save_as: bool) -> bool {
        if self.job.is_some() {
            return false;
        }
        let path = if save_as || self.path.is_none() {
            rfd::FileDialog::new()
                .add_filter("Scroll Studio story", &["scro"])
                .set_file_name(if self.assets_only {
                    "My world.scro"
                } else {
                    "My story.scro"
                })
                .save_file()
        } else {
            self.path.clone()
        };
        let Some(mut path) = path else {
            return false;
        };
        path.set_extension("scro");
        let expected = self
            .file_stamp
            .filter(|_| self.path.as_ref() == Some(&path));
        let story = self.story.clone();
        let assets = assets_for_story(&story, &self.assets);
        let revision = self.revision;
        let document_epoch = self.document_epoch;
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let result = storage::save_story_checked(&path, &story, &assets, expected)
                .map(|stamp| JobResult::Saved(path, revision, document_epoch, stamp));
            let _ = tx.send(result);
        });
        self.job = Some(rx);
        self.busy_label = "Saving story…".into();
        true
    }

    fn export(&mut self) {
        if self.job.is_some() {
            return;
        }
        let Some(base) = rfd::FileDialog::new()
            .set_title("Choose export folder")
            .pick_folder()
        else {
            return;
        };
        let chapter = self.story.chapters[self.chapter].clone();
        let assets = self.assets.clone();
        let options = self.export_options.clone();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let result = (|| {
                let stamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_millis();
                let directory = base.join(format!("Scroll-export-{stamp}"));
                std::fs::create_dir(&directory)?;
                export::export_chapter_with_options(&chapter, &assets, &directory, &options)
                    .map(JobResult::Exported)
            })();
            let _ = tx.send(result);
        });
        self.job = Some(rx);
        self.busy_label = "Exporting comic slices…".into();
    }

    fn poll_job(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.job {
            let result = match rx.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Disconnected) => Some(Err(anyhow::anyhow!(
                    "The file worker stopped unexpectedly."
                ))),
                Err(mpsc::TryRecvError::Empty) => None,
            };
            if let Some(result) = result {
                self.job = None;
                match result {
                    Ok(JobResult::Saved(path, revision, epoch, stamp))
                        if epoch == self.document_epoch =>
                    {
                        self.file_stamp = Some(stamp);
                        self.path = Some(path.clone());
                        if revision == self.revision {
                            self.dirty = false;
                            self.clear_saved_recovery();
                        }
                        self.status = format!("Saved {}", path.display());
                        if self.continue_after_save && !self.dirty {
                            self.dialog = None;
                            self.perform_pending();
                        }
                    }
                    Ok(JobResult::Saved(path, _, _, _)) => {
                        self.status = format!("Previous story saved to {}", path.display());
                    }
                    Ok(JobResult::Exported(report)) => {
                        self.status = format!(
                            "Exported {} {} slices · {:.1} MB{}",
                            report.files.len(),
                            report.format.label(),
                            report.total_bytes as f64 / 1_000_000.0,
                            if report.compliant {
                                ""
                            } else {
                                " · Check export warnings"
                            }
                        );
                        self.export_report = Some(report);
                    }
                    Ok(JobResult::CharacterSheetDone(message)) => self.status = message,
                    Ok(JobResult::Healed(result)) => self.apply_healing(result),
                    Ok(JobResult::BackingColor(result)) => self.apply_backing_color(result),
                    Err(e) => self.error = Some(format!("{e:#}")),
                }
                self.continue_after_save = false;
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(80));
            }
        }
    }

    fn import_picker(&mut self) {
        if let Some(paths) = rfd::FileDialog::new()
            .add_filter("Images", &["png", "jpg", "jpeg", "webp"])
            .pick_files()
        {
            for path in paths {
                self.import_file(&path, false, None);
            }
        }
    }

    fn import_file(&mut self, path: &std::path::Path, place: bool, pos: Option<[f32; 2]>) {
        let result = (|| -> anyhow::Result<Vec<u8>> {
            anyhow::ensure!(
                std::fs::metadata(path)?.len() <= 64 * 1024 * 1024,
                "Image exceeds the 64 MB import limit."
            );
            Ok(std::fs::read(path)?)
        })();
        match result {
            Ok(bytes) => self.import_bytes(
                bytes,
                path.file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                place,
                pos,
            ),
            Err(e) => self.error = Some(format!("Could not import image: {e:#}")),
        }
    }

    fn import_bytes(&mut self, bytes: Vec<u8>, name: String, place: bool, pos: Option<[f32; 2]>) {
        self.sanitize_location_selection();
        if self.assets_only
            && !matches!(
                self.tab,
                LibraryTab::Characters | LibraryTab::Backgrounds | LibraryTab::Groups
            )
        {
            return;
        }
        if self.tab == LibraryTab::Groups {
            if let Some(group) = self.selected_group {
                self.import_group_reference_bytes(group, bytes, name);
            } else {
                self.status = "Open a group to add references".into();
            }
            return;
        }
        if self.tab == LibraryTab::Characters {
            if let Some(character_id) = self.character_selected_id() {
                self.import_character_reference_bytes(character_id, bytes, name);
                return;
            }
        }
        if let Err(e) = self.check_import_budget(bytes.len()) {
            self.error = Some(e.to_string());
            return;
        }
        match compositor::decode_image(&bytes) {
            Ok(image) => {
                self.checkpoint();
                let id = self.story.alloc_id();
                let kind = match self.tab {
                    LibraryTab::Characters => AssetKind::Character,
                    LibraryTab::Backgrounds => AssetKind::Background,
                    _ => AssetKind::Image,
                };
                self.story.assets.push(Asset {
                    id,
                    name: name.clone(),
                    kind,
                    notes: String::new(),
                    prompt: String::new(),
                    width: image.width(),
                    height: image.height(),
                    character: CharacterProfile::default(),
                });
                self.assets.insert(id, Arc::new(bytes));
                if kind == AssetKind::Background {
                    self.assign_background_location(id, self.selected_location);
                }
                if kind == AssetKind::Character {
                    // A new asset may reuse an ID from an abandoned undo branch.
                    self.selected_asset = None;
                    self.select_character(id);
                } else {
                    self.selected_asset = Some(id);
                }
                self.changed();
                if place && self.tab != LibraryTab::Characters {
                    self.place_asset(id, pos);
                }
                self.status = format!("Imported {name}");
            }
            Err(e) => self.error = Some(format!("Could not decode image: {e:#}")),
        }
    }

    fn place_asset(&mut self, asset_id: u64, pos: Option<[f32; 2]>) {
        if self.assets_only {
            return;
        }
        let Some(asset) = self.story.assets.iter().find(|a| a.id == asset_id).cloned() else {
            return;
        };
        if !self.assets.contains_key(&asset_id) {
            self.status = "Add a reference image to this sheet first.".into();
            return;
        }
        self.checkpoint();
        let id = self.story.alloc_id();
        let ch = &mut self.story.chapters[self.chapter];
        let width = (ch.width as f32 * 0.8).min(asset.width as f32);
        let height = width * asset.height as f32 / asset.width.max(1) as f32;
        let [x, y] = pos.unwrap_or([(ch.width as f32 - width) * 0.5, self.scroll_y + 100.0]);
        ch.layers.push(Layer {
            id,
            name: asset.name,
            visible: true,
            locked: false,
            opacity: 1.0,
            x: x.clamp(0.0, ch.width as f32 - 1.0),
            y: y.clamp(0.0, ch.height as f32 - 1.0),
            width,
            height,
            kind: LayerKind::Image {
                source_rect: None,
                clip: None,
                asset_id,
                mask: Vec::new(),
            },
        });
        self.selected = Some(id);
        self.selected_asset = None;
        self.tool = Tool::Move;
        self.changed();
    }

    fn paste_image(&mut self) {
        self.paste_image_at(None);
    }

    fn paste_image_at(&mut self, at: Option<[f32; 2]>) {
        let result = (|| -> anyhow::Result<Vec<u8>> {
            let mut clipboard = arboard::Clipboard::new()?;
            let data = clipboard.get_image()?;
            anyhow::ensure!(
                data.width <= 16384
                    && data.height <= 16384
                    && data.width.saturating_mul(data.height) <= 40_000_000,
                "Clipboard image is too large."
            );
            let image = image::RgbaImage::from_raw(
                data.width as u32,
                data.height as u32,
                data.bytes.into_owned(),
            )
            .ok_or_else(|| anyhow::anyhow!("Invalid clipboard image"))?;
            let mut out = std::io::Cursor::new(Vec::new());
            image.write_to(&mut out, image::ImageFormat::Png)?;
            Ok(out.into_inner())
        })();
        match result {
            Ok(bytes) => self.import_bytes(bytes, "Pasted image".into(), true, at),
            Err(e) => {
                self.error = Some(format!(
                    "Copy an image in your other app, then paste here. {e}"
                ))
            }
        }
    }

    fn check_import_budget(&self, bytes: usize) -> anyhow::Result<()> {
        let current: usize = self
            .story
            .assets
            .iter()
            .filter_map(|a| self.assets.get(&a.id))
            .map(|b| b.len())
            .sum();
        anyhow::ensure!(
            self.story.assets.len() < 4096,
            "This story has reached its 4,096 asset limit."
        );
        anyhow::ensure!(bytes <= 64 * 1024 * 1024 && current.saturating_add(bytes) <= 496 * 1024 * 1024, "This image would exceed the story's 496 MB embedded-image budget. Use a smaller image or start another story.");
        Ok(())
    }

    fn copy_cutout(&mut self, layer: &Layer) {
        let result = (|| -> anyhow::Result<()> {
            let LayerKind::Image {
                asset_id,
                mask,
                clip,
                source_rect,
            } = &layer.kind
            else {
                return Ok(());
            };
            let bytes = self
                .assets
                .get(asset_id)
                .ok_or_else(|| anyhow::anyhow!("Image not found"))?;
            let source = compositor::decode_image(bytes)?;
            let mut cutout = compositor::masked_image(&source, mask);
            if let Some(clip) = clip {
                scroll_studio::clipping::apply(&mut cutout, clip)?;
            }
            if let Some([x, y, w, h]) = source_rect {
                let sw = cutout.width();
                let sh = cutout.height();
                let x0 = (x * sw as f32).floor() as u32;
                let y0 = (y * sh as f32).floor() as u32;
                let x1 = ((x + w) * sw as f32).ceil().min(sw as f32) as u32;
                let y1 = ((y + h) * sh as f32).ceil().min(sh as f32) as u32;
                cutout = image::imageops::crop_imm(&cutout, x0, y0, x1 - x0, y1 - y0).to_image();
            }
            for pixel in cutout.pixels_mut() {
                pixel[3] = (pixel[3] as f32 * layer.opacity) as u8;
            }
            arboard::Clipboard::new()?.set_image(arboard::ImageData {
                width: cutout.width() as usize,
                height: cutout.height() as usize,
                bytes: std::borrow::Cow::Owned(cutout.into_raw()),
            })?;
            Ok(())
        })();
        match result {
            Ok(()) => {
                self.status =
                    "Cutout copied with transparency. Paste it into your image app.".into()
            }
            Err(e) => self.error = Some(format!("Could not copy cutout: {e:#}")),
        }
    }

    fn delete_selection(&mut self) {
        if let Some(id) = self.selected {
            if self.story.chapters[self.chapter]
                .layers
                .iter()
                .any(|l| l.id == id && !l.locked)
            {
                self.checkpoint();
                self.story.chapters[self.chapter]
                    .layers
                    .retain(|l| l.id != id);
                self.selected = None;
                self.changed();
            }
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if self.setup.is_some()
            || self.location_editor.is_some()
            || self.snippet.is_some()
            || self.camera_dialog.is_some()
            || self.dialogue_draft.is_some()
            || self.spacing_draft.is_some()
            || context_ui::context_menu_open(ctx)
        {
            return;
        }
        let consume = |modifiers, key| {
            ctx.input_mut(|input| {
                input.consume_shortcut(&egui::KeyboardShortcut::new(modifiers, key))
            })
        };
        let command = egui::Modifiers::COMMAND;
        let shifted_command = command | egui::Modifiers::SHIFT;
        if self.dialog.is_none() && consume(command, egui::Key::Comma) {
            self.settings_open = !self.settings_open;
            return;
        }
        // A complete key chord can arrive within one frame. Its key event owns
        // the modifiers; the frame's final modifier state may already be empty.
        let native_style_copy = ctx.input(|input| {
            input.events.iter().any(|event| {
                matches!(event,
                egui::Event::Key { key: egui::Key::C, pressed: false, modifiers, .. }
                    if modifiers.matches_logically(shifted_command))
            }) || (input.modifiers.matches_logically(shifted_command)
                && input
                    .events
                    .iter()
                    .any(|event| matches!(event, egui::Event::Copy)))
        });
        if !self.welcome && (consume(shifted_command, egui::Key::C) || native_style_copy) {
            // egui-winit translates copy presses into Event::Copy, including
            // Ctrl+Shift+C. Keep that event from copying a focused text field.
            ctx.input_mut(|input| {
                input
                    .events
                    .retain(|event| !matches!(event, egui::Event::Copy))
            });
            if self.story.visual_style.trim().is_empty() {
                self.style_prompt_open = true;
            } else {
                ctx.copy_text(self.story.visual_style.clone());
                self.status = "Style copied".into();
            }
            return;
        }
        if self.dialog.is_none() {
            if !self.welcome {
                if consume(shifted_command, egui::Key::S) {
                    self.save(true);
                    return;
                }
                if consume(command, egui::Key::S) {
                    self.save(false);
                    return;
                }
            }
            if consume(command, egui::Key::O) {
                self.request(Pending::Open(None));
                return;
            }
            if consume(command, egui::Key::N) {
                self.request(Pending::New);
                return;
            }
        }
        // Native paste presses are intercepted by egui-winit. Image-only
        // clipboards can produce no Paste event, but still emit the V release.
        let paste_id = egui::Id::new("image_paste_press_handled");
        let (paste_press, paste_release, any_paste_release, paste_event) = ctx.input(|input| {
            let key_event = |pressed| {
                input.events.iter().any(|event| {
                    matches!(event,
                egui::Event::Key { key: egui::Key::V, pressed: actual, modifiers, .. }
                    if *actual == pressed && modifiers.matches_logically(command))
                })
            };
            (
                key_event(true),
                key_event(false),
                input.events.iter().any(|event| {
                    matches!(
                        event,
                        egui::Event::Key {
                            key: egui::Key::V,
                            pressed: false,
                            ..
                        }
                    )
                }),
                input
                    .events
                    .iter()
                    .any(|event| matches!(event, egui::Event::Paste(_))),
            )
        });
        let paste_handled = ctx.data_mut(|data| data.get_temp::<bool>(paste_id).unwrap_or(false));
        if any_paste_release {
            ctx.data_mut(|data| data.insert_temp(paste_id, false));
        }
        if self.settings_open {
            if (paste_press || paste_event) && !any_paste_release {
                ctx.data_mut(|data| data.insert_temp(paste_id, true));
            }
            if self.dialog.is_none() && consume(egui::Modifiers::NONE, egui::Key::Escape) {
                self.settings_open = false;
            }
            return;
        }
        if ctx.wants_keyboard_input()
            || self.dialog.is_some()
            || self.welcome
            || ctx
                .pointer_hover_pos()
                .is_some_and(|point| self.reference_contains(point))
        {
            if (paste_press || paste_event) && !any_paste_release {
                ctx.data_mut(|data| data.insert_temp(paste_id, true));
            }
            return;
        }
        if consume(shifted_command, egui::Key::Z) || consume(command, egui::Key::Y) {
            self.redo();
            return;
        }
        if consume(command, egui::Key::D) {
            self.deselect_pixels();
            return;
        }
        if consume(command, egui::Key::Z) {
            self.undo();
            return;
        }
        if consume(command, egui::Key::V) || (paste_release && !paste_handled) {
            ctx.data_mut(|data| data.insert_temp(paste_id, !any_paste_release));
            self.paste_image();
            return;
        }
        if matches!(
            self.tab,
            LibraryTab::Bubbles
                | LibraryTab::Characters
                | LibraryTab::Backgrounds
                | LibraryTab::Groups
                | LibraryTab::Timeline
                | LibraryTab::Lore
        ) || self.style_prompt_open
        {
            return;
        }
        if consume(egui::Modifiers::NONE, egui::Key::S) {
            self.snap_settings.enabled = !self.snap_settings.enabled;
            self.snap_guides.clear();
        }
        if consume(egui::Modifiers::NONE, egui::Key::G) {
            self.begin_snippet(ctx, snippet_ui::SnippetKind::Background);
        }
        if consume(egui::Modifiers::NONE, egui::Key::H) {
            self.begin_snippet(ctx, snippet_ui::SnippetKind::Character);
        }
        for (key, tool) in [
            (egui::Key::V, Tool::Move),
            (egui::Key::M, Tool::Select),
            (egui::Key::C, Tool::Crop),
            (egui::Key::L, Tool::Lasso),
            (egui::Key::W, Tool::SmartLasso),
            (egui::Key::E, Tool::MagicEraser),
            (egui::Key::B, Tool::Brush),
            (egui::Key::J, Tool::Heal),
            (egui::Key::K, Tool::Glint),
            (egui::Key::U, Tool::Wind),
            (egui::Key::O, Tool::Blur),
            (egui::Key::T, Tool::Text),
            (egui::Key::P, Tool::Panel),
        ] {
            if consume(egui::Modifiers::NONE, key) {
                self.tool = tool;
            }
        }
        if consume(egui::Modifiers::NONE, egui::Key::Delete) {
            self.delete_selection();
        }
        if consume(egui::Modifiers::NONE, egui::Key::Escape) {
            self.lasso.clear();
            self.drag_tail = None;
            self.drag_layer = None;
            self.resize_anchor = None;
            self.snap_drag = None;
            self.snap_guides.clear();
        }
    }
}

impl eframe::App for Studio {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.frame_started = Instant::now();
        let startup_alpha = self.startup_frame(ctx);
        if startup_alpha == Some(1.0) {
            self.paint_startup(ctx, 1.0);
            return;
        }
        if let Some(paint) = self.paint.as_mut() {
            if paint.update(ctx) {
                self.paint = None;
            }
            return;
        }
        self.begin_context_frame(ctx);
        self.poll_job(ctx);
        self.previews.begin_frame(ctx, _frame.info().cpu_usage);
        self.previews.poll(ctx);
        self.poll_selection(ctx);
        if let Some(message) = self.previews.take_error() {
            self.status = message;
        }
        self.poll_recovery(ctx);
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close {
            self.setup = None;
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.request(Pending::Quit);
        }
        if self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if self.reader.is_some() {
            self.reader_ui(ctx);
            return;
        }
        self.shortcuts(ctx);
        self.dispatch_text_context_command(ctx);
        motion::set_mode(ctx, self.preferences.motion);
        self.top_bar(ctx);
        self.status_bar(ctx);
        self.reference_window(ctx);
        if self.settings_open {
            if !self.welcome {
                self.library(ctx);
            }
            self.settings_workspace(ctx);
        } else if self.welcome {
            self.welcome_screen(ctx);
        } else {
            self.library(ctx);
            if self.tab == LibraryTab::Bubbles {
                self.bubble_preset_inspector(ctx);
                self.bubble_preset_workspace(ctx);
            } else if self.tab == LibraryTab::Characters {
                self.character_workspace(ctx);
            } else if self.tab == LibraryTab::Backgrounds {
                self.location_workspace(ctx);
            } else if self.tab == LibraryTab::Groups {
                self.groups_workspace(ctx);
            } else if self.tab == LibraryTab::Lore {
                self.lore_workspace(ctx);
            } else if self.tab == LibraryTab::Timeline {
                self.timeline_workspace(ctx);
            } else {
                if self.assets_only {
                    self.tab = LibraryTab::Characters;
                    self.character_workspace(ctx);
                } else {
                    self.inspector(ctx);
                    self.workspace(ctx);
                }
            }
        }
        self.dialogs(ctx);
        self.recovery_dialog(ctx);
        self.export_dialog_ui(ctx);
        self.style_prompt_ui(ctx);
        self.story_setup_ui(ctx);
        self.location_editor_ui(ctx);
        self.snippet_ui(ctx);
        self.camera_dialog_ui(ctx);
        self.dialogue_setup_ui(ctx);
        self.spacing_ui(ctx);
        self.finish_context_frame(ctx);
        self.finish_preset_slider_frame(ctx);
        let dropped = if self.setup.is_none()
            && self.dialog.is_none()
            && self.location_editor.is_none()
            && self.dialogue_draft.is_none()
            && self.spacing_draft.is_none()
            && self.snippet.is_none()
            && self.camera_dialog.is_none()
            && !self.settings_open
        {
            ctx.input(|i| i.raw.dropped_files.clone())
        } else {
            Vec::new()
        };
        for file in dropped {
            if let Some(path) = file.path {
                if path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("scro"))
                {
                    self.request(Pending::Open(Some(path)));
                } else if !self.welcome {
                    let pos = ctx.input(|i| i.pointer.hover_pos());
                    if pos.is_some_and(|point| self.reference_contains(point)) {
                        self.import_pinned_reference(ctx, &path);
                        continue;
                    }
                    let on_canvas =
                        !matches!(self.tab, LibraryTab::Bubbles | LibraryTab::Characters)
                            && pos.is_some_and(|p| {
                                self.canvas_rect.contains(p)
                                    && !self.toolbar_rect.contains(p)
                                    && !self.reference_contains(p)
                            });
                    let at = pos.filter(|_| on_canvas).map(|p| {
                        [
                            (p.x - self.canvas_rect.left()) / self.zoom,
                            (p.y - self.canvas_rect.top()) / self.zoom,
                        ]
                    });
                    self.import_file(&path, on_canvas, at);
                }
            }
        }
        if !self.settings_open && !ctx.input(|i| i.raw.hovered_files.is_empty()) {
            egui::Area::new(egui::Id::new("drop_hint"))
                .order(egui::Order::Tooltip)
                .anchor(egui::Align2::CENTER_TOP, [0.0, 90.0])
                .show(ctx, |ui| {
                    egui::Frame::new()
                        .fill(ACCENT)
                        .inner_margin(18.0)
                        .corner_radius(10.0)
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new(if self.tab == LibraryTab::Characters {
                                    "Add references"
                                } else {
                                    "Add images"
                                })
                                .color(BG)
                                .strong(),
                            );
                        });
                });
        }
        let previous = self.preferences.clone();
        self.preferences.set_snap_settings(&self.snap_settings);
        if self.preferences != previous {
            self.preferences_changed_at = Some(Instant::now());
        }
        self.flush_preferences(ctx, false);
        if let Some(alpha) = startup_alpha {
            self.paint_startup(ctx, alpha);
        }
    }

    fn on_exit(&mut self, gl: Option<&eframe::glow::Context>) {
        self.save_preferences();
        if let Some(gl) = gl {
            self.glass.destroy(gl);
        }
    }
}

pub(super) fn eyebrow(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).size(11.0).color(MUTED).strong());
}
pub(super) fn primary(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(motion::Button::new(RichText::new(text).color(BG).strong()).fill(ACCENT))
}

/// A save includes only the current document. Undo-only images stay in memory
/// for redo, but are not orphan archive entries in the saved story.
fn assets_for_story(story: &Story, pool: &AssetBytes) -> AssetBytes {
    story
        .assets
        .iter()
        .filter_map(|asset| {
            pool.get(&asset.id)
                .map(|bytes| (asset.id, Arc::clone(bytes)))
        })
        .collect()
}

fn retained_asset_ids(story: &Story, undo: &VecDeque<Story>, redo: &[Story]) -> HashSet<u64> {
    std::iter::once(story)
        .chain(undo.iter())
        .chain(redo.iter())
        .flat_map(|snapshot| snapshot.assets.iter().map(|asset| asset.id))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chord(key: egui::Key, modifiers: egui::Modifiers) -> Vec<egui::Event> {
        [true, false]
            .into_iter()
            .map(|pressed| egui::Event::Key {
                key,
                physical_key: Some(key),
                pressed,
                repeat: false,
                modifiers,
            })
            .collect()
    }

    fn shortcut_frame(
        ctx: &egui::Context,
        studio: &mut Studio,
        events: Vec<egui::Event>,
        focused: bool,
    ) -> egui::FullOutput {
        ctx.run(
            egui::RawInput {
                modifiers: egui::Modifiers::NONE,
                events,
                ..Default::default()
            },
            |ctx| {
                if focused {
                    ctx.memory_mut(|memory| {
                        memory.request_focus(egui::Id::new("test_text_editor"))
                    });
                }
                studio.shortcuts(ctx);
            },
        )
    }

    #[test]
    fn fast_new_story_chord_uses_key_event_modifiers_even_in_text_fields() {
        let ctx = egui::Context::default();
        let mut studio = Studio::from_context(&ctx);
        studio.welcome = false;
        let _ = shortcut_frame(
            &ctx,
            &mut studio,
            chord(
                egui::Key::N,
                egui::Modifiers::CTRL | egui::Modifiers::COMMAND,
            ),
            true,
        );
        assert!(studio.setup.is_some());
    }

    #[test]
    fn style_copy_handles_fast_chords_and_native_copy_translation() {
        let modifiers = egui::Modifiers::CTRL | egui::Modifiers::COMMAND | egui::Modifiers::SHIFT;
        let native = vec![
            egui::Event::Copy,
            egui::Event::Key {
                key: egui::Key::C,
                physical_key: Some(egui::Key::C),
                pressed: false,
                repeat: false,
                modifiers,
            },
        ];
        for events in [chord(egui::Key::C, modifiers), native] {
            let ctx = egui::Context::default();
            let mut studio = Studio::from_context(&ctx);
            studio.welcome = false;
            studio.story.visual_style = "Crisp contours, warm amber".into();
            let output = shortcut_frame(&ctx, &mut studio, events, true);
            assert!(output.platform_output.commands.iter().any(|command|
                matches!(command, egui::OutputCommand::CopyText(text) if text == "Crisp contours, warm amber")));
            assert!(!ctx.input(|input| input
                .events
                .iter()
                .any(|event| matches!(event, egui::Event::Copy))));
        }
    }

    #[test]
    fn setup_modal_blocks_story_shortcuts() {
        let ctx = egui::Context::default();
        let mut studio = Studio::from_context(&ctx);
        studio.welcome = false;
        studio.story.visual_style = "Original style".into();
        studio.begin_story_setup();
        let original = studio.story.clone();
        let output = shortcut_frame(
            &ctx,
            &mut studio,
            chord(
                egui::Key::C,
                egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
            ),
            false,
        );
        assert!(studio.setup.is_some());
        assert_eq!(studio.story, original);
        assert!(output.platform_output.commands.is_empty());
    }

    #[test]
    fn command_keys_do_not_fall_through_to_canvas_tools() {
        let ctx = egui::Context::default();
        let mut studio = Studio::from_context(&ctx);
        studio.welcome = false;
        let _ = shortcut_frame(
            &ctx,
            &mut studio,
            chord(egui::Key::B, egui::Modifiers::COMMAND),
            false,
        );
        assert!(studio.tool == Tool::Move);
        let _ = shortcut_frame(
            &ctx,
            &mut studio,
            chord(egui::Key::B, egui::Modifiers::NONE),
            false,
        );
        assert!(studio.tool == Tool::Brush);
    }

    #[test]
    fn text_focus_preserves_paste_and_undo_for_the_editor() {
        let ctx = egui::Context::default();
        let mut studio = Studio::from_context(&ctx);
        studio.welcome = false;
        studio.checkpoint();
        studio.story.title = "Edited".into();
        let previous_error = studio.error.clone();
        let mut events = chord(egui::Key::Z, egui::Modifiers::COMMAND);
        events.extend(chord(egui::Key::V, egui::Modifiers::COMMAND));
        events.push(egui::Event::Paste("Dialogue".into()));
        let _ = shortcut_frame(&ctx, &mut studio, events, true);
        assert_eq!(studio.story.title, "Edited");
        assert_eq!(studio.error, previous_error);
        assert!(ctx.input(|input| input
            .events
            .iter()
            .any(|event| matches!(event, egui::Event::Paste(text) if text == "Dialogue"))));
    }

    fn imported_story() -> (Story, Story, AssetBytes, u64) {
        let before = Story::new("Undo import regression");
        let mut imported = before.clone();
        let id = imported.alloc_id();
        imported.assets.push(Asset {
            id,
            name: "Reference".into(),
            kind: AssetKind::Image,
            notes: String::new(),
            prompt: String::new(),
            width: 2,
            height: 2,
            character: CharacterProfile::default(),
        });
        let pixels = image::RgbaImage::from_pixel(2, 2, image::Rgba([45, 90, 135, 255]));
        let mut encoded = std::io::Cursor::new(Vec::new());
        pixels
            .write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();
        (
            before,
            imported,
            HashMap::from([(id, Arc::new(encoded.into_inner()))]),
            id,
        )
    }

    #[test]
    fn character_image_import_routes_gallery_and_explicit_selection_separately() {
        let (_, _, pool, image_id) = imported_story();
        for selected in [false, true] {
            let ctx = egui::Context::default();
            let mut studio = Studio::from_context(&ctx);
            studio.story = Story::demo();
            studio.tab = LibraryTab::Characters;
            studio.error = None;
            let character_id = studio
                .story
                .assets
                .iter()
                .find(|asset| asset.kind == AssetKind::Character)
                .unwrap()
                .id;
            studio.selected_asset = selected.then_some(character_id);
            studio.character_scene = "Only Mira enters the old station".into();
            let original = studio.story.clone();
            studio.import_bytes(
                pool[&image_id].as_ref().clone(),
                "New reference".into(),
                true,
                None,
            );
            assert!(studio.error.is_none());
            assert_eq!(
                studio.story.chapters, original.chapters,
                "Importing in Characters must not place an image on the chapter"
            );
            let original_character = studio
                .story
                .assets
                .iter()
                .find(|asset| asset.id == character_id)
                .unwrap();
            let imported = studio.story.assets.last().unwrap();
            if selected {
                assert_eq!(studio.character_scene, "Only Mira enters the old station");
                assert_eq!(studio.selected_asset, Some(character_id));
                assert_eq!(imported.kind, AssetKind::Image);
                assert_eq!(original_character.character.references.len(), 1);
                assert_eq!(
                    original_character.character.references[0].asset_id,
                    imported.id
                );
            } else {
                assert!(studio.character_scene.is_empty());
                assert_eq!(imported.kind, AssetKind::Character);
                assert_ne!(imported.id, character_id);
                assert_eq!(studio.selected_asset, Some(imported.id));
                assert_eq!(
                    original_character,
                    original
                        .assets
                        .iter()
                        .find(|asset| asset.id == character_id)
                        .unwrap()
                );
            }
            assert_eq!((imported.width, imported.height), (2, 2));
            assert!(studio.assets.contains_key(&imported.id));
            assert_eq!(studio.undo.len(), 1);
            let edited = studio.story.clone();
            studio.undo();
            assert_eq!(studio.story, original);
            studio.redo();
            assert_eq!(studio.story, edited);
        }
    }

    #[test]
    fn saving_after_undo_excludes_orphans_and_redo_preserves_original_bytes() {
        let (before, imported, pool, id) = imported_story();
        let undo = VecDeque::new();
        let redo = vec![imported.clone()];
        assert!(retained_asset_ids(&before, &undo, &redo).contains(&id));
        let save_assets = assets_for_story(&before, &pool);
        assert!(save_assets.is_empty());
        let path = std::env::temp_dir().join(format!(
            "scroll-studio-undo-import-{}.scro",
            std::process::id()
        ));
        storage::save_story(&path, &before, &save_assets).unwrap();
        let (loaded, bytes) = storage::load_story(&path).unwrap();
        assert_eq!(loaded, before);
        assert!(bytes.is_empty());

        let redo_assets = assets_for_story(&imported, &pool);
        storage::save_story(&path, &imported, &redo_assets).unwrap();
        let (loaded, bytes) = storage::load_story(&path).unwrap();
        assert_eq!(loaded, imported);
        assert_eq!(bytes[&id], pool[&id]);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn abandoning_redo_releases_orphans_but_keeps_a_worker_snapshot_alive() {
        let (before, imported, mut pool, id) = imported_story();
        let worker_assets = assets_for_story(&imported, &pool);
        let retained = retained_asset_ids(&before, &VecDeque::new(), &[]);
        pool.retain(|id, _| retained.contains(id));
        assert!(pool.is_empty());
        assert!(storage::validate_story(&imported, &worker_assets).is_ok());

        // After abandoning the branch, this ID can be reused without carrying
        // its former asset bytes or cached preview into the replacement edit.
        let mut branch = before;
        assert_eq!(branch.alloc_id(), id);
        assert!(!retained.contains(&id));
    }

    #[test]
    fn commands_are_one_undo_step_and_failures_keep_redo() {
        let ctx = egui::Context::default();
        let mut studio = Studio::from_context(&ctx);
        let before = studio.story.clone();
        let title = |value: &str| Command::SetField {
            target: Target::Story,
            field: Field::Title,
            value: value.into(),
        };
        studio.apply_command(title("Rain")).unwrap();
        assert_eq!(studio.story.title, "Rain");
        assert_eq!(studio.undo.len(), 1);
        studio.undo();
        assert_eq!(studio.story, before);
        assert_eq!(studio.redo.len(), 1);
        // A failed command and a command that changes nothing leave history alone.
        assert!(studio
            .apply_command(Command::DisconnectLore {
                connection: 999_999
            })
            .is_err());
        studio.apply_command(title(&before.title)).unwrap();
        assert_eq!(studio.redo.len(), 1);
        studio.redo();
        assert_eq!(studio.story.title, "Rain");
        // A note created through the board is one step, card and all.
        let undo = studio.undo.len();
        studio.new_lore();
        assert_eq!(studio.undo.len(), undo + 1);
        assert_eq!(
            studio.story.lore_board.cards.len(),
            before.lore_board.cards.len() + 1
        );
        studio.undo();
        assert_eq!(studio.story.lore, before.lore);
        assert_eq!(studio.story.lore_board, before.lore_board);
    }

    #[test]
    fn sheet_edits_a_command_refuses_still_save_as_one_undo_step() {
        let ctx = egui::Context::default();
        let mut studio = Studio::from_context(&ctx);
        let id = scroll_studio::locations::add_location(&mut studio.story, "Pier").unwrap();
        let index = studio
            .story
            .locations
            .iter()
            .position(|l| l.id == id)
            .unwrap();
        let original = studio.story.locations[index].clone();
        // Over the command limit for location text, but 0.13.2 accepted it from the sheet.
        let mut edited = original.clone();
        edited.profile.notes = "x".repeat(64 * 1024 + 1);
        let undo = studio.undo.len();
        studio.commit_location_sheet(index, &original, edited.clone());
        assert_eq!(studio.story.locations[index], edited);
        assert_eq!(studio.undo.len(), undo + 1);
        studio.undo();
        assert_eq!(studio.story.locations[index], original);
        // An ordinary edit goes through commands, also as one step.
        let mut edited = original.clone();
        edited.profile.lighting = "Dawn".into();
        studio.commit_location_sheet(index, &original, edited.clone());
        assert_eq!(studio.story.locations[index], edited);
        assert_eq!(studio.undo.len(), undo + 1);
    }

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        image::RgbaImage::new(width, height)
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        bytes
    }

    #[test]
    fn story_changed_reports_command_objects_and_everything_for_undo() {
        let ctx = egui::Context::default();
        let mut studio = Studio::from_context(&ctx);
        studio.take_story_changes();
        studio
            .apply_command(Command::SetField {
                target: Target::Story,
                field: Field::Title,
                value: "Rain".into(),
            })
            .unwrap();
        let chapter = studio.story.chapters[0].id;
        studio
            .apply_command(Command::SetChapterHeight {
                chapter,
                height: 9000,
            })
            .unwrap();
        studio.undo();
        let changes = studio.take_story_changes();
        let scopes: Vec<_> = changes.iter().map(|c| c.scope.clone()).collect();
        assert_eq!(
            scopes,
            [
                commands::ChangeScope::Objects(vec![commands::STORY_ID]),
                commands::ChangeScope::Objects(vec![chapter]),
                commands::ChangeScope::Everything,
            ]
        );
        assert!(changes.windows(2).all(|w| w[0].revision < w[1].revision));
        assert_eq!(changes.last().unwrap().revision, studio.revision);
        // Without a reader the queue stays bounded.
        for i in 0..(MAX_STORY_CHANGES + 10) {
            studio
                .apply_command(Command::SetField {
                    target: Target::Story,
                    field: Field::Synopsis,
                    value: i.to_string(),
                })
                .unwrap();
        }
        assert_eq!(studio.take_story_changes().len(), MAX_STORY_CHANGES);
    }

    #[test]
    fn stage_two_edits_are_single_undo_steps() {
        let ctx = egui::Context::default();
        let mut studio = Studio::from_context(&ctx);
        let start = studio.story.clone();
        let step = |studio: &mut Studio, before: usize| assert_eq!(studio.undo.len(), before + 1);

        // A beat is added, sorted into place and selected.
        let undo = studio.undo.len();
        studio.add_beat(10.0);
        step(&mut studio, undo);
        let beat = studio.active_panel.unwrap();
        let panels = &studio.story.chapters[studio.chapter].panels;
        assert!(panels.iter().any(|p| p.id == beat && p.y == 10.0));
        assert!(panels.windows(2).all(|w| w[0].y <= w[1].y));

        // A group: create, join a character through its sheet, delete.
        let undo = studio.undo.len();
        studio.new_group();
        step(&mut studio, undo);
        let group = studio.selected_group.unwrap();
        let character = studio.story.alloc_id();
        studio.story.assets.push(Asset {
            id: character,
            name: "Mira".into(),
            kind: AssetKind::Character,
            notes: String::new(),
            prompt: String::new(),
            width: 0,
            height: 0,
            character: CharacterProfile::default(),
        });
        let index = studio
            .story
            .groups
            .iter()
            .position(|g| g.id == group)
            .unwrap();
        let original = studio.story.groups[index].clone();
        let mut edited = original.clone();
        edited.character_ids.push(character);
        edited.name = "Night shift".into();
        let undo = studio.undo.len();
        let command = commands::group_edit(&original, &edited);
        assert!(studio
            .commit(command, |_| panic!("the command applies"))
            .is_some());
        step(&mut studio, undo);
        assert_eq!(studio.story.groups[index], edited);
        let undo = studio.undo.len();
        studio
            .apply_command(Command::RemoveGroup { group })
            .unwrap();
        step(&mut studio, undo);
        assert!(studio.story.groups.is_empty());
        studio.undo();
        assert_eq!(studio.story.groups[index], edited);

        // A character reference import is one step; undo detaches it and keeps the bytes
        // for redo.
        let undo = studio.undo.len();
        studio.import_character_reference_bytes(character, png(4, 6), "Side".into());
        step(&mut studio, undo);
        let reference = studio
            .story
            .assets
            .iter()
            .find(|a| a.id == character)
            .unwrap()
            .character
            .references[0]
            .asset_id;
        assert!(studio.assets.contains_key(&reference));
        studio.undo();
        assert!(studio
            .story
            .assets
            .iter()
            .find(|a| a.id == character)
            .unwrap()
            .character
            .references
            .is_empty());
        studio.redo();
        assert!(studio.assets.contains_key(&reference));

        // Chronology moves only the timeline, never the reading order.
        let chapter = studio.story.chapters[0].id;
        let reading: Vec<_> = studio.story.chapters.iter().map(|c| c.id).collect();
        studio
            .apply_command(Command::AddChapter {
                name: "Two".into(),
                width: 800,
                height: 6400,
                opening: None,
            })
            .unwrap();
        let undo = studio.undo.len();
        studio.move_timeline(chapter, None);
        step(&mut studio, undo);
        assert_eq!(
            scroll_studio::timeline::entries(&studio.story)
                .last()
                .unwrap()
                .chapter_id,
            chapter
        );
        assert_eq!(studio.story.chapters[0].id, reading[0]);

        // Undoing everything returns to the start.
        while !studio.undo.is_empty() {
            studio.undo();
        }
        assert_eq!(studio.story, start);
    }

    #[test]
    fn history_retention_keeps_images_needed_by_older_undo_states() {
        let (before, imported, pool, id) = imported_story();
        let undo = VecDeque::from([imported]);
        let retained = retained_asset_ids(&before, &undo, &[]);
        assert!(retained.contains(&id));
        assert!(pool.contains_key(&id));
        assert!(!retained_asset_ids(&before, &VecDeque::new(), &[]).contains(&id));
    }
}
