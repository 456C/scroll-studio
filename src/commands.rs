//! Named, undoable story edits.
//!
//! Every command goes through [`Story::apply`], which checks the command against the
//! current story before changing anything, applies it, and returns the command that
//! undoes it. A command that fails leaves the story exactly as it was.
//!
//! The app still keeps whole-story snapshots for Undo/Redo (see `Studio::apply_command`);
//! the inverse returned here is what later stages (the read API and addons) build on.
//! Commands only describe text and structure. They never save, open, export or delete
//! files.

use crate::{
    groups, locations, lore,
    model::{
        Asset, AssetKind, BackgroundLocation, CardThumbnail, Chapter, CharacterGender,
        CharacterOutfit, CharacterProfile, CharacterReference, CharacterSwatch, ChronologyEntry,
        LayerKind, LoreBoard, LoreEntry, Panel, Story, WorldGroup,
    },
    thumbnails, timeline,
};
use anyhow::{bail, ensure, Context, Result};
use std::collections::HashSet;

// Byte limits mirror what `storage::validate_story` accepts, so a command can never produce
// a story that fails to save.
const SHORT: usize = 4096;
const LONG: usize = 1_000_000;
const LORE_TEXT: usize = 100_000;
const LOCATION_TEXT: usize = 64 * 1024;
const GROUP_TEXT: usize = 20_000;
const MAX_OUTFITS: usize = 32;
const MAX_SWATCHES: usize = 16;
const MAX_LORE_NOTES: usize = 4096;
const MAX_REFERENCES: usize = 24;
const MAX_PANELS: usize = 10_000;
const MAX_CHAPTERS: usize = 1000;
const MIN_CHAPTER_SIDE: u32 = 64;
const MAX_CHAPTER_WIDTH: u32 = 8192;
const MAX_CHAPTER_HEIGHT: u32 = 200_000;
// storage::validate_story allows fewer than 4097 archive entries: one manifest plus assets.
const MAX_ASSETS: usize = 4096;
const MAX_IMAGE_SIDE: u32 = 16_384;
const MAX_IMAGE_PIXELS: u64 = 32_000_000;
const TIMELINE_WHEN: usize = 1024;
const TIMELINE_SUMMARY: usize = 20_000;
const COORDINATE_LIMIT: f32 = 1_000_000.0;

/// What a field edit targets. The story itself has no numeric ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Target {
    Story,
    Object(u64),
}

/// The editable text fields. This is a whitelist: anything not listed here cannot be set
/// by a command. Which fields exist depends on the object; see [`Field::allowed_on`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Field {
    // Story, panels and lore notes
    Title,
    Synopsis,
    VisualStyle,
    Script,
    // Shared by several objects
    Name,
    Notes,
    Prompt,
    Consistency,
    Avoid,
    // Characters
    Tagline,
    Role,
    Age,
    /// Kept for older stories. The app no longer shows it, but it is still saved.
    Pronouns,
    Personality,
    Backstory,
    Relationships,
    Face,
    Hair,
    Eyes,
    Skin,
    Build,
    DistinguishingFeatures,
    // Locations
    Description,
    Atmosphere,
    Lighting,
    // Lore notes and lore board cards
    Text,
    BoardNote,
}

/// The kinds of objects a [`Target::Object`] can name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectKind {
    Story,
    Panel,
    Character,
    /// A background, image or other non-character asset.
    Asset,
    Location,
    LoreNote,
    LoreCard,
    LoreConnection,
    Chapter,
    Group,
}

impl Field {
    pub const ALL: [Field; 27] = [
        Field::Title,
        Field::Synopsis,
        Field::VisualStyle,
        Field::Script,
        Field::Name,
        Field::Notes,
        Field::Prompt,
        Field::Consistency,
        Field::Avoid,
        Field::Tagline,
        Field::Role,
        Field::Age,
        Field::Pronouns,
        Field::Personality,
        Field::Backstory,
        Field::Relationships,
        Field::Face,
        Field::Hair,
        Field::Eyes,
        Field::Skin,
        Field::Build,
        Field::DistinguishingFeatures,
        Field::Description,
        Field::Atmosphere,
        Field::Lighting,
        Field::Text,
        Field::BoardNote,
    ];

    /// Stable lowercase names, used later by the read API and addons.
    pub fn as_str(self) -> &'static str {
        match self {
            Field::Title => "title",
            Field::Synopsis => "synopsis",
            Field::VisualStyle => "visual_style",
            Field::Script => "script",
            Field::Name => "name",
            Field::Notes => "notes",
            Field::Prompt => "prompt",
            Field::Consistency => "consistency",
            Field::Avoid => "avoid",
            Field::Tagline => "tagline",
            Field::Role => "role",
            Field::Age => "age",
            Field::Pronouns => "pronouns",
            Field::Personality => "personality",
            Field::Backstory => "backstory",
            Field::Relationships => "relationships",
            Field::Face => "face",
            Field::Hair => "hair",
            Field::Eyes => "eyes",
            Field::Skin => "skin",
            Field::Build => "build",
            Field::DistinguishingFeatures => "distinguishing_features",
            Field::Description => "description",
            Field::Atmosphere => "atmosphere",
            Field::Lighting => "lighting",
            Field::Text => "text",
            Field::BoardNote => "board_note",
        }
    }

    pub fn parse(name: &str) -> Option<Field> {
        Field::ALL.into_iter().find(|field| field.as_str() == name)
    }

    /// The fields an object kind accepts, in a stable order.
    pub fn allowed_on(kind: ObjectKind) -> &'static [Field] {
        use Field::*;
        match kind {
            ObjectKind::Story => &[Title, Synopsis, VisualStyle],
            ObjectKind::Panel => &[Title, Script, Notes, Prompt],
            ObjectKind::Character => &[
                Name,
                Tagline,
                Role,
                Age,
                Pronouns,
                Personality,
                Backstory,
                Relationships,
                Face,
                Hair,
                Eyes,
                Skin,
                Build,
                DistinguishingFeatures,
                Consistency,
                Avoid,
                Notes,
                Prompt,
            ],
            ObjectKind::Asset => &[Name, Notes, Prompt],
            ObjectKind::Location => &[
                Name,
                Description,
                Atmosphere,
                Lighting,
                Consistency,
                Avoid,
                Prompt,
                Notes,
            ],
            ObjectKind::LoreNote => &[Title, Text],
            ObjectKind::LoreCard => &[BoardNote],
            ObjectKind::LoreConnection => &[],
            ObjectKind::Chapter => &[Name],
            ObjectKind::Group => &[Name, Description],
        }
    }
}

/// Where [`Command::ReplaceText`] looks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// Every whitelisted text field in the story.
    Story,
    /// Only the whitelisted fields of one object (or the story's own fields).
    Target(Target),
}

/// A new panel. Its ID is assigned when the command is applied.
#[derive(Clone, Debug, PartialEq)]
pub struct PanelDraft {
    pub title: String,
    pub y: f32,
    pub height: f32,
    pub script: String,
    pub notes: String,
    pub prompt: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// Replace one whitelisted text field.
    SetField {
        target: Target,
        field: Field,
        value: String,
    },
    /// Find and replace literal text (case-sensitive) in whitelisted fields.
    ReplaceText {
        find: String,
        replace: String,
        scope: Scope,
    },
    /// Insert a panel after `after`, or at the end of the chapter when `after` is `None`.
    AddPanel {
        chapter: u64,
        after: Option<u64>,
        panel: PanelDraft,
    },
    SetGender {
        character: u64,
        gender: Option<CharacterGender>,
    },
    /// Append an outfit. The first outfit also becomes the active look, as in the sheet.
    AddOutfit {
        character: u64,
        outfit: CharacterOutfit,
    },
    SetOutfit {
        character: u64,
        index: usize,
        outfit: CharacterOutfit,
    },
    RemoveOutfit {
        character: u64,
        index: usize,
    },
    SetActiveOutfit {
        character: u64,
        index: Option<usize>,
    },
    AddPaletteColor {
        character: u64,
        name: String,
        color: [u8; 4],
    },
    SetPaletteColor {
        character: u64,
        index: usize,
        name: String,
        color: [u8; 4],
    },
    RemovePaletteColor {
        character: u64,
        index: usize,
    },
    /// Edit an additional reference's label and notes. The image itself is not touched.
    SetReference {
        character: u64,
        asset: u64,
        label: String,
        notes: String,
    },
    /// Detach an additional reference from a character. The image stays in the story.
    RemoveReference {
        character: u64,
        asset: u64,
    },
    /// Create a lore note and its card on the board.
    AddLoreNote {
        title: String,
        position: [f32; 2],
    },
    /// Pin an existing character, location, group or note to the lore board.
    AddLoreCard {
        target: u64,
        position: [f32; 2],
    },
    /// Remove a card and its connections. With `delete_note`, a note card's note is also
    /// deleted and unlinked everywhere.
    RemoveLoreCard {
        card: u64,
        delete_note: bool,
    },
    MoveLoreCard {
        card: u64,
        position: [f32; 2],
    },
    SetLoreCardColor {
        card: u64,
        color: [u8; 3],
    },
    ConnectLore {
        from: u64,
        to: u64,
        label: String,
    },
    SetLoreConnection {
        connection: u64,
        label: String,
        color: [u8; 3],
    },
    DisconnectLore {
        connection: u64,
    },
    /// Change a chapter's canvas height. Panels and layers keep their positions.
    SetChapterHeight {
        chapter: u64,
        height: u32,
    },
    /// Append a chapter, optionally with an opening panel.
    AddChapter {
        name: String,
        width: u32,
        height: u32,
        opening: Option<PanelDraft>,
    },
    SetPanelGeometry {
        panel: u64,
        y: f32,
        height: f32,
    },
    DeletePanel {
        panel: u64,
    },
    /// Order a chapter's panels top to bottom (stable for equal positions).
    SortPanels {
        chapter: u64,
    },
    /// Create an asset sheet without an image (characters, backgrounds, images). A new
    /// background can be filed under a location at the same time.
    AddAsset {
        name: String,
        kind: AssetKind,
        location: Option<u64>,
    },
    /// Create the sheet for an imported image and attach it. The image bytes stay with the
    /// app; commands only describe the story.
    AddImageAsset {
        name: String,
        width: u32,
        height: u32,
        attach: Attach,
    },
    AddGroup,
    SetGroupColor {
        group: u64,
        color: [u8; 3],
    },
    /// Replace a group's member and reference lists.
    SetGroupMembers {
        group: u64,
        characters: Vec<u64>,
        locations: Vec<u64>,
        references: Vec<u64>,
    },
    /// Delete a group, its lore links and its board card.
    RemoveGroup {
        group: u64,
    },
    /// Move a chapter in the chronology (not the reading order), before `before` or last.
    MoveChronology {
        chapter: u64,
        before: Option<u64>,
    },
    SetChronologyEntry {
        chapter: u64,
        when: String,
        summary: String,
    },
    /// Arrange the chronology to match the reading order.
    OrderChronologyByReading,
    /// Applied and undone as one step. If any part fails, nothing changes.
    Batch(Vec<Command>),

    // The commands below exist so every command has an exact inverse. They restore state
    // captured by `apply` and are not part of the surface offered to addons.
    #[doc(hidden)]
    RemovePanel {
        chapter: u64,
        panel: u64,
        next_id: u64,
    },
    #[doc(hidden)]
    InsertOutfit {
        character: u64,
        index: usize,
        outfit: CharacterOutfit,
        active_outfit: Option<usize>,
    },
    #[doc(hidden)]
    InsertPaletteColor {
        character: u64,
        index: usize,
        swatch: CharacterSwatch,
    },
    #[doc(hidden)]
    InsertReference {
        character: u64,
        index: usize,
        reference: CharacterReference,
        /// Where a card cover chosen from this reference sat in `card_thumbnails`.
        thumbnail: Option<usize>,
    },
    #[doc(hidden)]
    RestoreLore {
        lore: Vec<LoreEntry>,
        board: LoreBoard,
        next_id: u64,
    },
    #[doc(hidden)]
    RemoveLastChapter {
        chapter: u64,
        next_id: u64,
    },
    /// Put back whole sections of the story captured before a structural edit.
    #[doc(hidden)]
    Restore(Box<Sections>),
}

/// Where [`Command::AddImageAsset`] attaches the new image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Attach {
    /// A standalone image in the library.
    Library,
    CharacterReference(u64),
    GroupReference(u64),
}

/// Story sections captured for [`Command::Restore`]. `None` leaves a section alone.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sections {
    /// One chapter's panels, by chapter ID.
    pub panels: Option<(u64, Vec<Panel>)>,
    pub assets: Option<Vec<Asset>>,
    pub locations: Option<Vec<BackgroundLocation>>,
    pub groups: Option<Vec<WorldGroup>>,
    pub timeline: Option<Vec<ChronologyEntry>>,
    pub lore: Option<Vec<LoreEntry>>,
    pub lore_board: Option<LoreBoard>,
    pub card_thumbnails: Option<Vec<CardThumbnail>>,
    pub next_id: u64,
}

/// The story itself, in [`Applied::touched`] and [`StoryChanged`]. Object IDs start at 1.
pub const STORY_ID: u64 = 0;

/// What a [`StoryChanged`] notice covers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChangeScope {
    /// These objects changed ([`STORY_ID`] for the story's own fields).
    Objects(Vec<u64>),
    /// Anything may have changed: undo, redo, opening a file, or an edit made outside
    /// the command layer.
    Everything,
}

/// A notice that the story changed, for addons that keep their own view of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoryChanged {
    /// Increases with every change, in step with the app's revision counter.
    pub revision: u64,
    pub scope: ChangeScope,
}

impl Command {
    /// Commands that only exist to undo other commands. Later stages must refuse these
    /// when they arrive from outside the app.
    pub fn is_internal(&self) -> bool {
        match self {
            Command::RemovePanel { .. }
            | Command::InsertOutfit { .. }
            | Command::InsertPaletteColor { .. }
            | Command::InsertReference { .. }
            | Command::RestoreLore { .. }
            | Command::RemoveLastChapter { .. }
            | Command::Restore(_) => true,
            Command::Batch(commands) => commands.iter().any(Command::is_internal),
            _ => false,
        }
    }

    /// A short, stable name for logs and status messages.
    pub fn name(&self) -> &'static str {
        match self {
            Command::SetField { .. } => "set_field",
            Command::ReplaceText { .. } => "replace_text",
            Command::AddPanel { .. } => "add_panel",
            Command::SetGender { .. } => "set_gender",
            Command::AddOutfit { .. } => "add_outfit",
            Command::SetOutfit { .. } => "set_outfit",
            Command::RemoveOutfit { .. } => "remove_outfit",
            Command::SetActiveOutfit { .. } => "set_active_outfit",
            Command::AddPaletteColor { .. } => "add_palette_color",
            Command::SetPaletteColor { .. } => "set_palette_color",
            Command::RemovePaletteColor { .. } => "remove_palette_color",
            Command::SetReference { .. } => "set_reference",
            Command::RemoveReference { .. } => "remove_reference",
            Command::AddLoreNote { .. } => "add_lore_note",
            Command::AddLoreCard { .. } => "add_lore_card",
            Command::RemoveLoreCard { .. } => "remove_lore_card",
            Command::MoveLoreCard { .. } => "move_lore_card",
            Command::SetLoreCardColor { .. } => "set_lore_card_color",
            Command::ConnectLore { .. } => "connect_lore",
            Command::SetLoreConnection { .. } => "set_lore_connection",
            Command::DisconnectLore { .. } => "disconnect_lore",
            Command::Batch(_) => "batch",
            Command::RemovePanel { .. } => "remove_panel",
            Command::InsertOutfit { .. } => "insert_outfit",
            Command::InsertPaletteColor { .. } => "insert_palette_color",
            Command::InsertReference { .. } => "insert_reference",
            Command::RestoreLore { .. } => "restore_lore",
            Command::SetChapterHeight { .. } => "set_chapter_height",
            Command::AddChapter { .. } => "add_chapter",
            Command::SetPanelGeometry { .. } => "set_panel_geometry",
            Command::DeletePanel { .. } => "delete_panel",
            Command::SortPanels { .. } => "sort_panels",
            Command::AddAsset { .. } => "add_asset",
            Command::AddImageAsset { .. } => "add_image_asset",
            Command::AddGroup => "add_group",
            Command::SetGroupColor { .. } => "set_group_color",
            Command::SetGroupMembers { .. } => "set_group_members",
            Command::RemoveGroup { .. } => "remove_group",
            Command::MoveChronology { .. } => "move_chronology",
            Command::SetChronologyEntry { .. } => "set_chronology_entry",
            Command::OrderChronologyByReading => "order_chronology_by_reading",
            Command::RemoveLastChapter { .. } => "remove_last_chapter",
            Command::Restore(_) => "restore",
        }
    }
}

/// The result of a successful [`Story::apply`].
#[derive(Clone, Debug, PartialEq)]
pub struct Applied {
    /// Applying this restores the story to its state before the command.
    pub inverse: Command,
    /// IDs of objects whose content changed (the story itself is not listed).
    pub touched: Vec<u64>,
    /// IDs created by the command, in creation order.
    pub created: Vec<u64>,
}

impl Applied {
    fn new(inverse: Command) -> Self {
        Self {
            inverse,
            touched: Vec::new(),
            created: Vec::new(),
        }
    }
    fn touch(mut self, id: u64) -> Self {
        if !self.touched.contains(&id) {
            self.touched.push(id);
        }
        self
    }
    fn create(mut self, id: u64) -> Self {
        self.created.push(id);
        self.touch(id)
    }
}

#[derive(Clone, Copy)]
enum Slot {
    Story,
    Panel(usize, usize),
    Asset(usize),
    Location(usize),
    LoreNote(usize),
    LoreCard(usize),
    LoreConnection,
    Chapter(usize),
    Group(usize),
}

impl Story {
    /// Check `command`, apply it, and return its inverse. On error the story is unchanged.
    pub fn apply(&mut self, command: Command) -> Result<Applied> {
        match command {
            // One command is already atomic; sheet typing sends one per keystroke, so skip
            // the backup copy.
            Command::Batch(mut commands) if commands.len() == 1 => {
                let applied = self.apply(commands.pop().expect("one command"))?;
                Ok(Applied {
                    inverse: Command::Batch(vec![applied.inverse]),
                    ..applied
                })
            }
            Command::Batch(commands) => {
                // A failure part-way through restores the original story exactly.
                let backup = self.clone();
                let mut inverses = Vec::with_capacity(commands.len());
                let mut touched = Vec::new();
                let mut created = Vec::new();
                for command in commands {
                    match self.apply(command) {
                        Ok(applied) => {
                            for id in applied.touched {
                                if !touched.contains(&id) {
                                    touched.push(id);
                                }
                            }
                            created.extend(applied.created);
                            inverses.push(applied.inverse);
                        }
                        Err(error) => {
                            *self = backup;
                            return Err(error);
                        }
                    }
                }
                inverses.reverse();
                Ok(Applied {
                    inverse: Command::Batch(inverses),
                    touched,
                    created,
                })
            }
            command => self.apply_one(command),
        }
    }

    /// What kind of object an ID names, if any.
    pub fn object_kind(&self, target: Target) -> Option<ObjectKind> {
        let slot = self.slot(target).ok()?;
        Some(match slot {
            Slot::Story => ObjectKind::Story,
            Slot::Panel(..) => ObjectKind::Panel,
            Slot::Asset(index) => {
                if self.assets[index].kind == crate::model::AssetKind::Character {
                    ObjectKind::Character
                } else {
                    ObjectKind::Asset
                }
            }
            Slot::Location(_) => ObjectKind::Location,
            Slot::LoreNote(_) => ObjectKind::LoreNote,
            Slot::LoreCard(_) => ObjectKind::LoreCard,
            Slot::LoreConnection => ObjectKind::LoreConnection,
            Slot::Chapter(_) => ObjectKind::Chapter,
            Slot::Group(_) => ObjectKind::Group,
        })
    }

    /// Read a whitelisted field.
    pub fn field(&self, target: Target, field: Field) -> Result<&str> {
        let kind = self
            .object_kind(target)
            .context("That object no longer exists")?;
        ensure!(
            Field::allowed_on(kind).contains(&field),
            "{} cannot be edited on this object",
            field.as_str()
        );
        let slot = self.slot(target)?;
        Ok(match (slot, field) {
            (Slot::Story, Field::Title) => &self.title,
            (Slot::Story, Field::Synopsis) => &self.synopsis,
            (Slot::Story, Field::VisualStyle) => &self.visual_style,
            (Slot::Panel(c, p), field) => {
                let panel = &self.chapters[c].panels[p];
                match field {
                    Field::Title => &panel.title,
                    Field::Script => &panel.script,
                    Field::Notes => &panel.notes,
                    _ => &panel.prompt,
                }
            }
            (Slot::Asset(i), field) => asset_text(&self.assets[i], field),
            (Slot::Location(i), field) => location_text(&self.locations[i], field),
            (Slot::Chapter(c), _) => &self.chapters[c].name,
            (Slot::Group(g), Field::Name) => &self.groups[g].name,
            (Slot::Group(g), _) => &self.groups[g].description,
            (Slot::LoreNote(i), Field::Title) => &self.lore[i].title,
            (Slot::LoreNote(i), _) => &self.lore[i].text,
            (Slot::LoreCard(i), _) => &self.lore_board.cards[i].note,
            _ => bail!("{} cannot be edited on this object", field.as_str()),
        })
    }

    /// Every whitelisted (target, field) pair in the story, in a stable order.
    pub fn text_fields(&self) -> Vec<(Target, Field)> {
        let mut targets = vec![Target::Story];
        targets.extend(
            self.chapters
                .iter()
                .flat_map(|chapter| chapter.panels.iter().map(|panel| Target::Object(panel.id))),
        );
        targets.extend(self.chapters.iter().map(|c| Target::Object(c.id)));
        targets.extend(self.assets.iter().map(|asset| Target::Object(asset.id)));
        targets.extend(self.groups.iter().map(|g| Target::Object(g.id)));
        targets.extend(self.locations.iter().map(|l| Target::Object(l.id)));
        targets.extend(self.lore.iter().map(|n| Target::Object(n.id)));
        targets.extend(self.lore_board.cards.iter().map(|c| Target::Object(c.id)));
        let mut fields = Vec::new();
        for target in targets {
            if let Some(kind) = self.object_kind(target) {
                fields.extend(Field::allowed_on(kind).iter().map(|field| (target, *field)));
            }
        }
        fields
    }

    fn slot(&self, target: Target) -> Result<Slot> {
        let id = match target {
            Target::Story => return Ok(Slot::Story),
            Target::Object(id) => id,
        };
        if let Some(c) = self.chapters.iter().position(|chapter| chapter.id == id) {
            return Ok(Slot::Chapter(c));
        }
        if let Some(g) = self.groups.iter().position(|group| group.id == id) {
            return Ok(Slot::Group(g));
        }
        for (c, chapter) in self.chapters.iter().enumerate() {
            if let Some(p) = chapter.panels.iter().position(|panel| panel.id == id) {
                return Ok(Slot::Panel(c, p));
            }
        }
        if let Some(i) = self.assets.iter().position(|a| a.id == id) {
            return Ok(Slot::Asset(i));
        }
        if let Some(i) = self.locations.iter().position(|l| l.id == id) {
            return Ok(Slot::Location(i));
        }
        if let Some(i) = self.lore.iter().position(|n| n.id == id) {
            return Ok(Slot::LoreNote(i));
        }
        if let Some(i) = self.lore_board.cards.iter().position(|c| c.id == id) {
            return Ok(Slot::LoreCard(i));
        }
        if self.lore_board.connections.iter().any(|e| e.id == id) {
            return Ok(Slot::LoreConnection);
        }
        bail!("Object {id} no longer exists")
    }

    fn field_mut(&mut self, target: Target, field: Field) -> Result<&mut String> {
        // `field` validates the combination; the match below only maps it to storage.
        self.field(target, field)?;
        let slot = self.slot(target)?;
        Ok(match (slot, field) {
            (Slot::Story, Field::Title) => &mut self.title,
            (Slot::Story, Field::Synopsis) => &mut self.synopsis,
            (Slot::Story, _) => &mut self.visual_style,
            (Slot::Panel(c, p), field) => {
                let panel = &mut self.chapters[c].panels[p];
                match field {
                    Field::Title => &mut panel.title,
                    Field::Script => &mut panel.script,
                    Field::Notes => &mut panel.notes,
                    _ => &mut panel.prompt,
                }
            }
            (Slot::Asset(i), field) => {
                let asset = &mut self.assets[i];
                let p = &mut asset.character;
                match field {
                    Field::Name => &mut asset.name,
                    Field::Notes => &mut asset.notes,
                    Field::Prompt => &mut asset.prompt,
                    Field::Tagline => &mut p.tagline,
                    Field::Role => &mut p.role,
                    Field::Age => &mut p.age,
                    Field::Pronouns => &mut p.pronouns,
                    Field::Personality => &mut p.personality,
                    Field::Backstory => &mut p.backstory,
                    Field::Relationships => &mut p.relationships,
                    Field::Face => &mut p.face,
                    Field::Hair => &mut p.hair,
                    Field::Eyes => &mut p.eyes,
                    Field::Skin => &mut p.skin,
                    Field::Build => &mut p.build,
                    Field::DistinguishingFeatures => &mut p.distinguishing_features,
                    Field::Consistency => &mut p.consistency,
                    _ => &mut p.avoid,
                }
            }
            (Slot::Location(i), field) => {
                let location = &mut self.locations[i];
                let p = &mut location.profile;
                match field {
                    Field::Name => &mut location.name,
                    Field::Description => &mut p.description,
                    Field::Atmosphere => &mut p.atmosphere,
                    Field::Lighting => &mut p.lighting,
                    Field::Consistency => &mut p.consistency,
                    Field::Avoid => &mut p.avoid,
                    Field::Prompt => &mut p.prompt,
                    _ => &mut p.notes,
                }
            }
            (Slot::Chapter(c), _) => &mut self.chapters[c].name,
            (Slot::Group(g), Field::Name) => &mut self.groups[g].name,
            (Slot::Group(g), _) => &mut self.groups[g].description,
            (Slot::LoreNote(i), Field::Title) => &mut self.lore[i].title,
            (Slot::LoreNote(i), _) => &mut self.lore[i].text,
            (Slot::LoreCard(i), _) => &mut self.lore_board.cards[i].note,
            (Slot::LoreConnection, _) => unreachable!("connections have no text fields"),
        })
    }

    fn character_index(&self, id: u64) -> Result<usize> {
        let index = self
            .assets
            .iter()
            .position(|a| a.id == id)
            .context("That character no longer exists")?;
        ensure!(
            self.assets[index].kind == crate::model::AssetKind::Character,
            "That asset is not a character"
        );
        Ok(index)
    }

    fn card_index(&self, id: u64) -> Result<usize> {
        self.lore_board
            .cards
            .iter()
            .position(|c| c.id == id)
            .context("That card is no longer on the board")
    }

    fn connection_index(&self, id: u64) -> Result<usize> {
        self.lore_board
            .connections
            .iter()
            .position(|e| e.id == id)
            .context("That connection no longer exists")
    }

    fn chapter_index(&self, id: u64) -> Result<usize> {
        self.chapters
            .iter()
            .position(|c| c.id == id)
            .context("That chapter no longer exists")
    }

    fn group_index(&self, id: u64) -> Result<usize> {
        self.groups
            .iter()
            .position(|g| g.id == id)
            .context("That group no longer exists")
    }

    /// A restore command capturing `sections` and the current ID counter.
    fn restore(&self, sections: Sections) -> Command {
        Command::Restore(Box::new(Sections {
            next_id: self.next_id,
            ..sections
        }))
    }

    fn restore_panels(&self, chapter: usize) -> Command {
        self.restore(Sections {
            panels: Some((
                self.chapters[chapter].id,
                self.chapters[chapter].panels.clone(),
            )),
            ..Default::default()
        })
    }

    /// Replace the sections present in `sections` and return what they replaced. The ID
    /// counter is left to the caller. Fails before changing anything if a chapter is gone.
    fn swap_sections(&mut self, sections: Sections) -> Result<Sections> {
        let chapter = match &sections.panels {
            Some((id, _)) => Some(self.chapter_index(*id)?),
            None => None,
        };
        let mut old = Sections {
            next_id: self.next_id,
            ..Default::default()
        };
        if let (Some(c), Some((id, panels))) = (chapter, sections.panels) {
            old.panels = Some((id, std::mem::replace(&mut self.chapters[c].panels, panels)));
        }
        macro_rules! swap {
            ($field:ident) => {
                if let Some(value) = sections.$field {
                    old.$field = Some(std::mem::replace(&mut self.$field, value));
                }
            };
        }
        swap!(assets);
        swap!(locations);
        swap!(groups);
        swap!(timeline);
        swap!(lore);
        swap!(lore_board);
        swap!(card_thumbnails);
        Ok(old)
    }

    /// The structural rules a restore must keep: unique IDs below the counter, the
    /// validators storage runs for each section, and references between sections.
    fn check_sections(&self) -> Result<()> {
        check_restored_lore(self)?;
        ensure!(self.assets.len() <= MAX_ASSETS, "Too many assets");
        for chapter in &self.chapters {
            ensure!(chapter.panels.len() <= MAX_PANELS, "Chapter is too large");
            for panel in &chapter.panels {
                ensure!(
                    panel.y.is_finite() && panel.height.is_finite() && panel.height > 0.0,
                    "Invalid panel geometry"
                );
            }
        }
        let assets: HashSet<_> = self.assets.iter().map(|a| a.id).collect();
        let images: HashSet<_> = self
            .assets
            .iter()
            .filter(|a| a.kind == AssetKind::Image)
            .map(|a| a.id)
            .collect();
        for layer in self.chapters.iter().flat_map(|c| &c.layers) {
            if let LayerKind::Image { asset_id, .. } = &layer.kind {
                ensure!(
                    assets.contains(asset_id),
                    "A layer uses an image that is missing"
                );
            }
        }
        for asset in &self.assets {
            ensure!(
                asset.character.references.len() <= MAX_REFERENCES
                    && asset
                        .character
                        .references
                        .iter()
                        .all(|r| images.contains(&r.asset_id)),
                "A character reference is missing"
            );
        }
        locations::validate_locations(self)?;
        groups::validate(self)?;
        timeline::validate(self)?;
        thumbnails::validate(self)
    }

    /// Every object ID in the story, matching the ID kinds `storage::validate_story`
    /// requires to be unique.
    fn all_ids(&self) -> impl Iterator<Item = u64> + '_ {
        let chapters = self.chapters.iter().flat_map(|c| {
            std::iter::once(c.id)
                .chain(c.panels.iter().map(|p| p.id))
                .chain(c.layers.iter().map(|l| l.id))
        });
        chapters
            .chain(self.assets.iter().map(|a| a.id))
            .chain(self.locations.iter().map(|l| l.id))
            .chain(self.groups.iter().map(|g| g.id))
            .chain(self.lore.iter().map(|n| n.id))
            .chain(self.lore_board.cards.iter().map(|c| c.id))
            .chain(self.lore_board.connections.iter().map(|e| e.id))
            .chain(self.bubble_presets.iter().map(|p| p.id))
    }

    /// The ID counter an inverse may restore. Moving it forward is always safe. Rewinding
    /// lets a redo reuse the same IDs, but only while nothing newer exists; otherwise the
    /// current counter is kept.
    fn safe_next_id(&self, restored: u64) -> u64 {
        if restored >= self.next_id || self.all_ids().all(|id| id < restored) {
            restored
        } else {
            self.next_id
        }
    }

    fn lore_snapshot(&self) -> Command {
        Command::RestoreLore {
            lore: self.lore.clone(),
            board: self.lore_board.clone(),
            next_id: self.next_id,
        }
    }

    fn apply_one(&mut self, command: Command) -> Result<Applied> {
        match command {
            Command::Batch(_) => unreachable!("batches are handled by apply"),
            Command::SetField {
                target,
                field,
                value,
            } => {
                let kind = self
                    .object_kind(target)
                    .context("That object no longer exists")?;
                check_text(&value, field_limit(kind, field))?;
                if field == Field::Name && kind == ObjectKind::Group {
                    ensure!(!value.trim().is_empty(), "Enter a group name");
                }
                if field == Field::Name && kind == ObjectKind::Location {
                    // Validate, but store the text as given: trimming here would make the
                    // inverse of an edit to an older, untrimmed name inexact.
                    locations::checked_name(&value)?;
                }
                let slot = self.field_mut(target, field)?;
                let old = std::mem::replace(slot, value);
                let applied = Applied::new(Command::SetField {
                    target,
                    field,
                    value: old,
                });
                Ok(match target {
                    Target::Story => applied.touch(STORY_ID),
                    Target::Object(id) => applied.touch(id),
                })
            }
            Command::ReplaceText {
                find,
                replace,
                scope,
            } => {
                ensure!(!find.is_empty(), "Enter the text to find");
                check_text(&find, LONG)?;
                check_text(&replace, LONG)?;
                let fields: Vec<_> = match scope {
                    Scope::Story => self.text_fields(),
                    Scope::Target(target) => {
                        let kind = self
                            .object_kind(target)
                            .context("That object no longer exists")?;
                        Field::allowed_on(kind)
                            .iter()
                            .map(|f| (target, *f))
                            .collect()
                    }
                };
                let mut edits = Vec::new();
                for (target, field) in fields {
                    let current = self.field(target, field)?;
                    if current.contains(&find) {
                        // SetField validates each replacement; the batch rolls back on error.
                        let value = current.replace(&find, &replace);
                        edits.push(Command::SetField {
                            target,
                            field,
                            value,
                        });
                    }
                }
                self.apply(Command::Batch(edits))
            }
            Command::AddPanel {
                chapter,
                after,
                panel,
            } => {
                let c = self
                    .chapters
                    .iter()
                    .position(|ch| ch.id == chapter)
                    .context("That chapter no longer exists")?;
                ensure!(
                    self.chapters[c].panels.len() < MAX_PANELS,
                    "A chapter can have at most {MAX_PANELS} panels"
                );
                check_panel_draft(&panel, self.chapters[c].height)?;
                let index = match after {
                    None => self.chapters[c].panels.len(),
                    Some(after) => {
                        self.chapters[c]
                            .panels
                            .iter()
                            .position(|p| p.id == after)
                            .context("The panel to insert after no longer exists")?
                            + 1
                    }
                };
                let next_id = self.next_id;
                let id = self.alloc_id();
                self.chapters[c].panels.insert(index, panel.into_panel(id));
                Ok(Applied::new(Command::RemovePanel {
                    chapter,
                    panel: id,
                    next_id,
                })
                .touch(chapter)
                .create(id))
            }
            Command::RemovePanel {
                chapter,
                panel,
                next_id,
            } => {
                let c = self
                    .chapters
                    .iter()
                    .position(|ch| ch.id == chapter)
                    .context("That chapter no longer exists")?;
                let index = self.chapters[c]
                    .panels
                    .iter()
                    .position(|p| p.id == panel)
                    .context("That panel no longer exists")?;
                let removed = self.chapters[c].panels.remove(index);
                // Restoring the counter makes a redo assign the same panel ID again, but
                // only when no newer object would then have its ID handed out twice.
                self.next_id = self.safe_next_id(next_id);
                let after = index.checked_sub(1).map(|i| self.chapters[c].panels[i].id);
                Ok(Applied::new(Command::AddPanel {
                    chapter,
                    after,
                    panel: PanelDraft::from(removed),
                })
                .touch(chapter))
            }
            Command::SetGender { character, gender } => {
                let i = self.character_index(character)?;
                let old = std::mem::replace(&mut self.assets[i].character.gender, gender);
                Ok(Applied::new(Command::SetGender {
                    character,
                    gender: old,
                })
                .touch(character))
            }
            Command::AddOutfit { character, outfit } => {
                let i = self.character_index(character)?;
                check_outfit(&outfit)?;
                let profile = &mut self.assets[i].character;
                ensure!(
                    profile.outfits.len() < MAX_OUTFITS,
                    "A character can have at most {MAX_OUTFITS} outfits"
                );
                let active = profile.active_outfit;
                profile.outfits.push(outfit);
                if profile.active_outfit.is_none() {
                    profile.active_outfit = Some(profile.outfits.len() - 1);
                }
                let index = profile.outfits.len() - 1;
                // Undo removes the outfit and restores the previous active look exactly.
                let inverse = Command::Batch(vec![
                    Command::RemoveOutfit { character, index },
                    Command::SetActiveOutfit {
                        character,
                        index: active,
                    },
                ]);
                Ok(Applied::new(inverse).touch(character))
            }
            Command::SetOutfit {
                character,
                index,
                outfit,
            } => {
                let i = self.character_index(character)?;
                check_outfit(&outfit)?;
                let slot = self.assets[i]
                    .character
                    .outfits
                    .get_mut(index)
                    .context("That outfit no longer exists")?;
                let old = std::mem::replace(slot, outfit);
                Ok(Applied::new(Command::SetOutfit {
                    character,
                    index,
                    outfit: old,
                })
                .touch(character))
            }
            Command::RemoveOutfit { character, index } => {
                let i = self.character_index(character)?;
                let profile = &mut self.assets[i].character;
                ensure!(
                    index < profile.outfits.len(),
                    "That outfit no longer exists"
                );
                let active = profile.active_outfit;
                let outfit = profile.outfits.remove(index);
                profile.active_outfit = match active {
                    Some(a) if a == index => None,
                    Some(a) if a > index => Some(a - 1),
                    other => other,
                };
                Ok(Applied::new(Command::InsertOutfit {
                    character,
                    index,
                    outfit,
                    active_outfit: active,
                })
                .touch(character))
            }
            Command::InsertOutfit {
                character,
                index,
                outfit,
                active_outfit,
            } => {
                let i = self.character_index(character)?;
                check_outfit(&outfit)?;
                let profile = &mut self.assets[i].character;
                ensure!(index <= profile.outfits.len(), "Invalid outfit position");
                ensure!(
                    profile.outfits.len() < MAX_OUTFITS,
                    "A character can have at most {MAX_OUTFITS} outfits"
                );
                ensure!(
                    active_outfit.is_none_or(|a| a <= profile.outfits.len()),
                    "Invalid active outfit"
                );
                let active = profile.active_outfit;
                profile.outfits.insert(index, outfit);
                profile.active_outfit = active_outfit;
                Ok(Applied::new(Command::Batch(vec![
                    Command::RemoveOutfit { character, index },
                    Command::SetActiveOutfit {
                        character,
                        index: active,
                    },
                ]))
                .touch(character))
            }
            Command::SetActiveOutfit { character, index } => {
                let i = self.character_index(character)?;
                let profile = &mut self.assets[i].character;
                ensure!(
                    index.is_none_or(|index| index < profile.outfits.len()),
                    "That outfit no longer exists"
                );
                let old = std::mem::replace(&mut profile.active_outfit, index);
                Ok(Applied::new(Command::SetActiveOutfit {
                    character,
                    index: old,
                })
                .touch(character))
            }
            Command::AddPaletteColor {
                character,
                name,
                color,
            } => {
                let i = self.character_index(character)?;
                check_text(&name, SHORT)?;
                let palette = &mut self.assets[i].character.palette;
                ensure!(
                    palette.len() < MAX_SWATCHES,
                    "A palette can have at most {MAX_SWATCHES} colors"
                );
                palette.push(CharacterSwatch { name, color });
                Ok(Applied::new(Command::RemovePaletteColor {
                    character,
                    index: palette.len() - 1,
                })
                .touch(character))
            }
            Command::SetPaletteColor {
                character,
                index,
                name,
                color,
            } => {
                let i = self.character_index(character)?;
                check_text(&name, SHORT)?;
                let swatch = self.assets[i]
                    .character
                    .palette
                    .get_mut(index)
                    .context("That color no longer exists")?;
                let old = std::mem::replace(swatch, CharacterSwatch { name, color });
                Ok(Applied::new(Command::SetPaletteColor {
                    character,
                    index,
                    name: old.name,
                    color: old.color,
                })
                .touch(character))
            }
            Command::RemovePaletteColor { character, index } => {
                let i = self.character_index(character)?;
                let palette = &mut self.assets[i].character.palette;
                ensure!(index < palette.len(), "That color no longer exists");
                let swatch = palette.remove(index);
                Ok(Applied::new(Command::InsertPaletteColor {
                    character,
                    index,
                    swatch,
                })
                .touch(character))
            }
            Command::InsertPaletteColor {
                character,
                index,
                swatch,
            } => {
                let i = self.character_index(character)?;
                check_text(&swatch.name, SHORT)?;
                let palette = &mut self.assets[i].character.palette;
                ensure!(index <= palette.len(), "Invalid color position");
                ensure!(
                    palette.len() < MAX_SWATCHES,
                    "A palette can have at most {MAX_SWATCHES} colors"
                );
                palette.insert(index, swatch);
                Ok(Applied::new(Command::RemovePaletteColor { character, index }).touch(character))
            }
            Command::SetReference {
                character,
                asset,
                label,
                notes,
            } => {
                let i = self.character_index(character)?;
                check_text(&label, SHORT)?;
                check_text(&notes, LONG)?;
                let reference = self.assets[i]
                    .character
                    .references
                    .iter_mut()
                    .find(|r| r.asset_id == asset)
                    .context("That reference no longer exists")?;
                let old_label = std::mem::replace(&mut reference.label, label);
                let old_notes = std::mem::replace(&mut reference.notes, notes);
                Ok(Applied::new(Command::SetReference {
                    character,
                    asset,
                    label: old_label,
                    notes: old_notes,
                })
                .touch(character))
            }
            Command::RemoveReference { character, asset } => {
                let i = self.character_index(character)?;
                let references = &mut self.assets[i].character.references;
                let index = references
                    .iter()
                    .position(|r| r.asset_id == asset)
                    .context("That reference no longer exists")?;
                let reference = references.remove(index);
                // A card cover chosen from this reference stops being valid; keep it so
                // undo can restore it exactly.
                let thumbnail = self
                    .card_thumbnails
                    .iter()
                    .position(|t| t.target == character && t.asset_id == asset);
                if let Some(position) = thumbnail {
                    self.card_thumbnails.remove(position);
                }
                Ok(Applied::new(Command::InsertReference {
                    character,
                    index,
                    reference,
                    thumbnail,
                })
                .touch(character))
            }
            Command::InsertReference {
                character,
                index,
                reference,
                thumbnail,
            } => {
                let i = self.character_index(character)?;
                check_text(&reference.label, SHORT)?;
                check_text(&reference.notes, LONG)?;
                ensure!(
                    thumbnail.is_none_or(|t| t <= self.card_thumbnails.len())
                        && !(thumbnail.is_some()
                            && self.card_thumbnails.iter().any(|t| t.target == character)),
                    "Invalid card cover restore"
                );
                ensure!(
                    self.assets
                        .iter()
                        .any(|a| a.id == reference.asset_id
                            && a.kind == crate::model::AssetKind::Image),
                    "That reference image no longer exists"
                );
                let references = &mut self.assets[i].character.references;
                ensure!(index <= references.len(), "Invalid reference position");
                ensure!(
                    !references.iter().any(|r| r.asset_id == reference.asset_id),
                    "That reference is already attached"
                );
                ensure!(
                    references.len() < MAX_REFERENCES,
                    "A character can have at most {MAX_REFERENCES} references"
                );
                let asset = reference.asset_id;
                references.insert(index, reference);
                if let Some(position) = thumbnail {
                    self.card_thumbnails.insert(
                        position,
                        crate::model::CardThumbnail {
                            target: character,
                            asset_id: asset,
                        },
                    );
                }
                Ok(Applied::new(Command::RemoveReference { character, asset }).touch(character))
            }
            Command::AddLoreNote { title, position } => {
                check_text(&title, SHORT)?;
                let position = board_position(position)?;
                ensure!(
                    self.lore.len() < MAX_LORE_NOTES
                        && self.lore_board.cards.len() < lore::MAX_CARDS,
                    "The lore board is full"
                );
                let inverse = self.lore_snapshot();
                let id = self.alloc_id();
                self.lore.push(LoreEntry {
                    id,
                    title,
                    text: String::new(),
                    links: Vec::new(),
                });
                let card = lore::pin(self, id, position).expect("a new note can always be pinned");
                Ok(Applied::new(inverse).create(id).create(card))
            }
            Command::AddLoreCard { target, position } => {
                let position = board_position(position)?;
                ensure!(
                    lore::targets(self).contains_key(&target),
                    "Only characters, locations, groups and notes can be pinned"
                );
                if let Some(card) = self.lore_board.cards.iter().find(|c| c.target == target) {
                    // Already pinned: nothing changes, and the existing card is reported.
                    let id = card.id;
                    return Ok(Applied {
                        inverse: Command::Batch(Vec::new()),
                        touched: vec![id],
                        created: Vec::new(),
                    });
                }
                ensure!(
                    self.lore_board.cards.len() < lore::MAX_CARDS,
                    "The lore board is full"
                );
                let inverse = self.lore_snapshot();
                let card = lore::pin(self, target, position).context("Could not pin that card")?;
                Ok(Applied::new(inverse).create(card))
            }
            Command::RemoveLoreCard { card, delete_note } => {
                let index = self.card_index(card)?;
                let target = self.lore_board.cards[index].target;
                let inverse = self.lore_snapshot();
                lore::unpin(self, card);
                let mut applied = Applied::new(inverse).touch(card);
                if delete_note && self.lore.iter().any(|n| n.id == target) {
                    self.lore.retain(|n| n.id != target);
                    lore::unlink_target(self, target);
                    applied = applied.touch(target);
                }
                Ok(applied)
            }
            Command::MoveLoreCard { card, position } => {
                let position = board_position(position)?;
                let index = self.card_index(card)?;
                let old = std::mem::replace(&mut self.lore_board.cards[index].position, position);
                Ok(Applied::new(Command::MoveLoreCard {
                    card,
                    position: old,
                })
                .touch(card))
            }
            Command::SetLoreCardColor { card, color } => {
                let index = self.card_index(card)?;
                let old = std::mem::replace(&mut self.lore_board.cards[index].color, color);
                Ok(Applied::new(Command::SetLoreCardColor { card, color: old }).touch(card))
            }
            Command::ConnectLore { from, to, label } => {
                check_text(&label, SHORT)?;
                ensure!(from != to, "A card cannot connect to itself");
                self.card_index(from)?;
                self.card_index(to)?;
                if let Some(edge) = self
                    .lore_board
                    .connections
                    .iter()
                    .find(|e| (e.from == from && e.to == to) || (e.from == to && e.to == from))
                {
                    let id = edge.id;
                    return Ok(Applied {
                        inverse: Command::Batch(Vec::new()),
                        touched: vec![id],
                        created: Vec::new(),
                    });
                }
                ensure!(
                    self.lore_board.connections.len() < lore::MAX_CONNECTIONS,
                    "The lore board has too many connections"
                );
                let inverse = self.lore_snapshot();
                let id = lore::connect(self, from, to).context("Could not connect those cards")?;
                let index = self.connection_index(id)?;
                self.lore_board.connections[index].label = label;
                Ok(Applied::new(inverse).create(id))
            }
            Command::SetLoreConnection {
                connection,
                label,
                color,
            } => {
                check_text(&label, SHORT)?;
                let index = self.connection_index(connection)?;
                let edge = &mut self.lore_board.connections[index];
                Ok(Applied::new(Command::SetLoreConnection {
                    connection,
                    label: std::mem::replace(&mut edge.label, label),
                    color: std::mem::replace(&mut edge.color, color),
                })
                .touch(connection))
            }
            Command::DisconnectLore { connection } => {
                self.connection_index(connection)?;
                let inverse = self.lore_snapshot();
                self.lore_board.connections.retain(|e| e.id != connection);
                Ok(Applied::new(inverse).touch(connection))
            }
            Command::SetChapterHeight { chapter, height } => {
                let c = self.chapter_index(chapter)?;
                ensure!(
                    (MIN_CHAPTER_SIDE..=MAX_CHAPTER_HEIGHT).contains(&height),
                    "Chapter height must be between {MIN_CHAPTER_SIDE} and {MAX_CHAPTER_HEIGHT} px"
                );
                let old = std::mem::replace(&mut self.chapters[c].height, height);
                Ok(Applied::new(Command::SetChapterHeight {
                    chapter,
                    height: old,
                })
                .touch(chapter))
            }
            Command::AddChapter {
                name,
                width,
                height,
                opening,
            } => {
                check_text(&name, SHORT)?;
                ensure!(
                    (MIN_CHAPTER_SIDE..=MAX_CHAPTER_WIDTH).contains(&width)
                        && (MIN_CHAPTER_SIDE..=MAX_CHAPTER_HEIGHT).contains(&height),
                    "Chapter canvas dimensions are outside supported limits"
                );
                ensure!(
                    self.chapters.len() < MAX_CHAPTERS,
                    "This story has reached its chapter limit"
                );
                if let Some(panel) = &opening {
                    check_panel_draft(panel, height)?;
                }
                let next_id = self.next_id;
                let id = self.alloc_id();
                let mut chapter = Chapter::new(id, name, width, height);
                let mut applied = Applied::new(Command::RemoveLastChapter {
                    chapter: id,
                    next_id,
                });
                applied = applied.create(id);
                if let Some(panel) = opening {
                    let panel_id = self.alloc_id();
                    chapter.panels.push(panel.into_panel(panel_id));
                    applied = applied.create(panel_id);
                }
                self.chapters.push(chapter);
                Ok(applied)
            }
            Command::RemoveLastChapter { chapter, next_id } => {
                let last = self
                    .chapters
                    .last()
                    .context("There is no chapter to remove")?;
                ensure!(
                    last.id == chapter && last.layers.is_empty() && last.panels.len() <= 1,
                    "Only a newly added chapter can be removed this way"
                );
                ensure!(
                    !self.timeline.iter().any(|e| e.chapter_id == chapter),
                    "That chapter has timeline notes"
                );
                let removed = self.chapters.pop().expect("checked above");
                self.next_id = self.safe_next_id(next_id);
                let opening = removed.panels.into_iter().next().map(PanelDraft::from);
                Ok(Applied::new(Command::AddChapter {
                    name: removed.name,
                    width: removed.width,
                    height: removed.height,
                    opening,
                })
                .touch(chapter))
            }
            Command::SetPanelGeometry { panel, y, height } => {
                ensure!(
                    y.is_finite() && height.is_finite() && height > 0.0,
                    "A panel needs a finite position and a positive height"
                );
                let Slot::Panel(c, p) = self.slot(Target::Object(panel))? else {
                    bail!("That panel no longer exists")
                };
                let target = &mut self.chapters[c].panels[p];
                let old = (
                    std::mem::replace(&mut target.y, y),
                    std::mem::replace(&mut target.height, height),
                );
                Ok(Applied::new(Command::SetPanelGeometry {
                    panel,
                    y: old.0,
                    height: old.1,
                })
                .touch(panel))
            }
            Command::DeletePanel { panel } => {
                let Slot::Panel(c, p) = self.slot(Target::Object(panel))? else {
                    bail!("That panel no longer exists")
                };
                let chapter = self.chapters[c].id;
                let inverse = self.restore_panels(c);
                self.chapters[c].panels.remove(p);
                Ok(Applied::new(inverse).touch(chapter).touch(panel))
            }
            Command::SortPanels { chapter } => {
                let c = self.chapter_index(chapter)?;
                let inverse = self.restore_panels(c);
                self.chapters[c].panels.sort_by(|a, b| a.y.total_cmp(&b.y));
                Ok(Applied::new(inverse).touch(chapter))
            }
            Command::AddAsset {
                name,
                kind,
                location,
            } => {
                check_text(&name, SHORT)?;
                ensure!(
                    self.assets.len() < MAX_ASSETS,
                    "This story has reached its asset limit"
                );
                // An image sheet needs its pixels; use AddImageAsset.
                ensure!(kind != AssetKind::Image, "An image needs its picture");
                if let Some(location) = location {
                    ensure!(
                        kind == AssetKind::Background,
                        "Only backgrounds belong to locations"
                    );
                    ensure!(
                        self.locations.iter().any(|l| l.id == location),
                        "That location no longer exists"
                    );
                }
                let inverse = self.restore(Sections {
                    assets: Some(self.assets.clone()),
                    locations: location.map(|_| self.locations.clone()),
                    ..Default::default()
                });
                let id = self.alloc_id();
                self.assets.push(new_asset(id, name, kind, 0, 0));
                if let Some(location) = location {
                    locations::move_asset(self, id, Some(location))?;
                }
                Ok(Applied::new(inverse).create(id))
            }
            Command::AddImageAsset {
                name,
                width,
                height,
                attach,
            } => {
                check_text(&name, SHORT)?;
                ensure!(
                    self.assets.len() < MAX_ASSETS,
                    "This story has reached its asset limit"
                );
                ensure!(
                    width > 0
                        && height > 0
                        && width <= MAX_IMAGE_SIDE
                        && height <= MAX_IMAGE_SIDE
                        && u64::from(width) * u64::from(height) <= MAX_IMAGE_PIXELS,
                    "That image is too large"
                );
                let group = match attach {
                    Attach::Library => None,
                    Attach::CharacterReference(character) => {
                        let i = self.character_index(character)?;
                        ensure!(
                            self.assets[i].character.references.len() < MAX_REFERENCES,
                            "Each character can hold {MAX_REFERENCES} additional references"
                        );
                        None
                    }
                    Attach::GroupReference(group) => {
                        let g = self.group_index(group)?;
                        ensure!(
                            self.groups[g].reference_ids.len() < groups::MAX_REFERENCES,
                            "Each group supports {} references",
                            groups::MAX_REFERENCES
                        );
                        Some(g)
                    }
                };
                let inverse = self.restore(Sections {
                    assets: Some(self.assets.clone()),
                    groups: group.map(|_| self.groups.clone()),
                    ..Default::default()
                });
                let id = self.alloc_id();
                self.assets
                    .push(new_asset(id, name.clone(), AssetKind::Image, width, height));
                let mut applied = Applied::new(inverse).create(id);
                match attach {
                    Attach::Library => {}
                    Attach::CharacterReference(character) => {
                        let i = self.character_index(character)?;
                        self.assets[i]
                            .character
                            .references
                            .push(CharacterReference {
                                asset_id: id,
                                label: name,
                                notes: String::new(),
                            });
                        applied = applied.touch(character);
                    }
                    Attach::GroupReference(group_id) => {
                        let g = group.expect("checked above");
                        self.groups[g].reference_ids.push(id);
                        applied = applied.touch(group_id);
                    }
                }
                Ok(applied)
            }
            Command::AddGroup => {
                let inverse = self.restore(Sections {
                    groups: Some(self.groups.clone()),
                    ..Default::default()
                });
                let id = groups::add(self)?;
                Ok(Applied::new(inverse).create(id))
            }
            Command::SetGroupColor { group, color } => {
                let g = self.group_index(group)?;
                let old = std::mem::replace(&mut self.groups[g].color, color);
                Ok(Applied::new(Command::SetGroupColor { group, color: old }).touch(group))
            }
            Command::SetGroupMembers {
                group,
                characters,
                locations,
                references,
            } => {
                let g = self.group_index(group)?;
                let entry = &mut self.groups[g];
                let old = (
                    std::mem::replace(&mut entry.character_ids, characters),
                    std::mem::replace(&mut entry.location_ids, locations),
                    std::mem::replace(&mut entry.reference_ids, references),
                );
                if let Err(error) = groups::validate(self) {
                    let entry = &mut self.groups[g];
                    entry.character_ids = old.0;
                    entry.location_ids = old.1;
                    entry.reference_ids = old.2;
                    return Err(error);
                }
                Ok(Applied::new(Command::SetGroupMembers {
                    group,
                    characters: old.0,
                    locations: old.1,
                    references: old.2,
                })
                .touch(group))
            }
            Command::RemoveGroup { group } => {
                let g = self.group_index(group)?;
                let inverse = self.restore(Sections {
                    groups: Some(self.groups.clone()),
                    lore: Some(self.lore.clone()),
                    lore_board: Some(self.lore_board.clone()),
                    card_thumbnails: Some(self.card_thumbnails.clone()),
                    ..Default::default()
                });
                self.groups.remove(g);
                lore::unlink_target(self, group);
                self.card_thumbnails.retain(|t| t.target != group);
                Ok(Applied::new(inverse).touch(group))
            }
            Command::MoveChronology { chapter, before } => {
                let inverse = self.restore(Sections {
                    timeline: Some(self.timeline.clone()),
                    ..Default::default()
                });
                timeline::move_before(self, chapter, before)?;
                Ok(Applied::new(inverse).touch(chapter))
            }
            Command::SetChronologyEntry {
                chapter,
                when,
                summary,
            } => {
                ensure!(
                    when.len() <= TIMELINE_WHEN && summary.len() <= TIMELINE_SUMMARY,
                    "Timeline notes are too long"
                );
                let mut next = timeline::entries(self);
                let entry = next
                    .iter_mut()
                    .find(|e| e.chapter_id == chapter)
                    .context("That chapter no longer exists")?;
                let inverse = self.restore(Sections {
                    timeline: Some(self.timeline.clone()),
                    ..Default::default()
                });
                if entry.when != when || entry.summary != summary {
                    entry.when = when;
                    entry.summary = summary;
                    self.timeline = next;
                }
                Ok(Applied::new(inverse).touch(chapter))
            }
            Command::OrderChronologyByReading => {
                let old = timeline::entries(self);
                let next: Vec<_> = self
                    .chapters
                    .iter()
                    .filter_map(|c| old.iter().find(|e| e.chapter_id == c.id).cloned())
                    .collect();
                let inverse = self.restore(Sections {
                    timeline: Some(self.timeline.clone()),
                    ..Default::default()
                });
                if old != next {
                    self.timeline = next;
                }
                Ok(Applied::new(inverse))
            }
            Command::Restore(sections) => {
                let previous_next = self.next_id;
                let restored_next = sections.next_id;
                let old = self.swap_sections(*sections)?;
                // Decide the counter after the swap, once the restored objects are in place.
                self.next_id = self.safe_next_id(restored_next);
                if let Err(error) = self.check_sections() {
                    self.swap_sections(old.clone())
                        .expect("restoring the sections just replaced");
                    self.next_id = previous_next;
                    return Err(error);
                }
                Ok(Applied::new(Command::Restore(Box::new(Sections {
                    next_id: previous_next,
                    ..old
                }))))
            }
            Command::RestoreLore {
                lore: entries,
                board,
                next_id,
            } => {
                // Swap the lore in, validate, and swap back if it is not valid.
                let inverse = self.lore_snapshot();
                let old_lore = std::mem::replace(&mut self.lore, entries);
                let old_board = std::mem::replace(&mut self.lore_board, board);
                let old_next = self.next_id;
                self.next_id = self.safe_next_id(next_id);
                if let Err(error) = check_restored_lore(self) {
                    self.lore = old_lore;
                    self.lore_board = old_board;
                    self.next_id = old_next;
                    return Err(error);
                }
                Ok(Applied::new(inverse))
            }
        }
    }
}

fn check_text(value: &str, limit: usize) -> Result<()> {
    ensure!(value.len() <= limit, "That text is too long");
    Ok(())
}

/// The largest value (in bytes) a whitelisted field may hold.
pub fn field_limit(kind: ObjectKind, field: Field) -> usize {
    match (kind, field) {
        (_, Field::Title | Field::Name) => SHORT,
        (ObjectKind::Location, _) => LOCATION_TEXT,
        (ObjectKind::LoreNote | ObjectKind::LoreCard, _) => LORE_TEXT,
        (ObjectKind::Group, _) => GROUP_TEXT,
        _ => LONG,
    }
}

fn check_outfit(outfit: &CharacterOutfit) -> Result<()> {
    check_text(&outfit.name, SHORT)?;
    check_text(&outfit.description, LONG)
}

/// Board positions must be finite; like the board itself, far positions are clamped.
fn board_position(position: [f32; 2]) -> Result<[f32; 2]> {
    ensure!(
        position.iter().all(|v| v.is_finite()),
        "That position is outside the board"
    );
    Ok(position.map(|v| v.clamp(-COORDINATE_LIMIT, COORDINATE_LIMIT)))
}

/// The rules `storage::validate_story` applies to lore, checked before a restore.
fn check_restored_lore(story: &Story) -> Result<()> {
    ensure!(story.lore.len() <= MAX_LORE_NOTES, "Too many lore entries");
    ensure!(story.next_id < u64::MAX, "Invalid ID counter");
    let mut ids = HashSet::new();
    for id in story.all_ids() {
        ensure!(
            id > 0 && id < story.next_id && ids.insert(id),
            "Lore IDs are not valid"
        );
    }
    let link_targets: HashSet<_> = story
        .lore
        .iter()
        .map(|e| e.id)
        .chain(story.groups.iter().map(|g| g.id))
        .chain(story.locations.iter().map(|l| l.id))
        .chain(
            story
                .assets
                .iter()
                .filter(|a| a.kind == crate::model::AssetKind::Character)
                .map(|a| a.id),
        )
        .collect();
    for entry in &story.lore {
        ensure!(
            entry.title.len() <= SHORT && entry.text.len() <= LORE_TEXT,
            "Lore entry is too long"
        );
        let mut unique = HashSet::new();
        ensure!(
            entry
                .links
                .iter()
                .all(|id| *id != entry.id && link_targets.contains(id) && unique.insert(*id)),
            "Invalid lore link"
        );
    }
    lore::validate_board(story)
}

/// Commands that turn `before` into `after` for one character sheet edit.
///
/// The sheet edits a copy of the character each frame; this describes the difference as
/// named commands. Changes the commands cannot express (none today) return `None`, and
/// the caller keeps its previous behaviour.
pub fn character_edit(
    before: &crate::model::Asset,
    after: &crate::model::Asset,
) -> Option<Command> {
    use crate::model::Asset;
    let id = before.id;
    // Fields no sheet edits: if any of these differ, this is not a sheet edit.
    let same_frame = |a: &Asset, b: &Asset| {
        a.id == b.id && a.kind == b.kind && a.width == b.width && a.height == b.height
    };
    if !same_frame(before, after) {
        return None;
    }
    let mut commands = Vec::new();
    let character = Target::Object(id);
    for field in Field::allowed_on(ObjectKind::Character) {
        let (old, new) = (asset_text(before, *field), asset_text(after, *field));
        if old != new {
            commands.push(Command::SetField {
                target: character,
                field: *field,
                value: new.to_owned(),
            });
        }
    }
    let (b, a) = (&before.character, &after.character);
    if b.gender != a.gender {
        commands.push(Command::SetGender {
            character: id,
            gender: a.gender,
        });
    }
    // Outfits: one added, one removed, or edits in place.
    let mut outfits = b.outfits.clone();
    let mut active = b.active_outfit;
    if a.outfits.len() == outfits.len() + 1 && a.outfits[..outfits.len()] == outfits[..] {
        commands.push(Command::AddOutfit {
            character: id,
            outfit: a.outfits[outfits.len()].clone(),
        });
        outfits.push(a.outfits[outfits.len()].clone());
        if active.is_none() {
            active = Some(outfits.len() - 1);
        }
    } else if a.outfits.len() + 1 == outfits.len() {
        let index = (0..outfits.len()).find(|&i| {
            let mut rest = outfits.clone();
            rest.remove(i);
            rest == a.outfits
        })?;
        commands.push(Command::RemoveOutfit {
            character: id,
            index,
        });
        outfits.remove(index);
        active = match active {
            Some(x) if x == index => None,
            Some(x) if x > index => Some(x - 1),
            other => other,
        };
    }
    if outfits.len() != a.outfits.len() {
        return None;
    }
    for (index, (old, new)) in outfits.iter().zip(&a.outfits).enumerate() {
        if old != new {
            commands.push(Command::SetOutfit {
                character: id,
                index,
                outfit: new.clone(),
            });
        }
    }
    if active != a.active_outfit {
        commands.push(Command::SetActiveOutfit {
            character: id,
            index: a.active_outfit,
        });
    }
    // Palette: one added, one removed, or edits in place.
    let mut palette = b.palette.clone();
    if a.palette.len() == palette.len() + 1 && a.palette[..palette.len()] == palette[..] {
        let swatch = a.palette[palette.len()].clone();
        commands.push(Command::AddPaletteColor {
            character: id,
            name: swatch.name.clone(),
            color: swatch.color,
        });
        palette.push(swatch);
    } else if a.palette.len() + 1 == palette.len() {
        let index = (0..palette.len()).find(|&i| {
            let mut rest = palette.clone();
            rest.remove(i);
            rest == a.palette
        })?;
        commands.push(Command::RemovePaletteColor {
            character: id,
            index,
        });
        palette.remove(index);
    }
    if palette.len() != a.palette.len() {
        return None;
    }
    for (index, (old, new)) in palette.iter().zip(&a.palette).enumerate() {
        if old != new {
            commands.push(Command::SetPaletteColor {
                character: id,
                index,
                name: new.name.clone(),
                color: new.color,
            });
        }
    }
    // References: removals, then label and note edits. Adding needs an image import,
    // which happens outside the sheet.
    let mut references = b.references.clone();
    for reference in &b.references {
        if !a
            .references
            .iter()
            .any(|r| r.asset_id == reference.asset_id)
        {
            commands.push(Command::RemoveReference {
                character: id,
                asset: reference.asset_id,
            });
            references.retain(|r| r.asset_id != reference.asset_id);
        }
    }
    if references.len() != a.references.len()
        || references
            .iter()
            .zip(&a.references)
            .any(|(old, new)| old.asset_id != new.asset_id)
    {
        return None;
    }
    for (old, new) in references.iter().zip(&a.references) {
        if old != new {
            commands.push(Command::SetReference {
                character: id,
                asset: new.asset_id,
                label: new.label.clone(),
                notes: new.notes.clone(),
            });
        }
    }
    Some(Command::Batch(commands))
}

/// An asset's whitelisted text, by field. Callers check the field is allowed first.
fn asset_text(asset: &crate::model::Asset, field: Field) -> &str {
    let p = &asset.character;
    match field {
        Field::Name => &asset.name,
        Field::Notes => &asset.notes,
        Field::Prompt => &asset.prompt,
        Field::Tagline => &p.tagline,
        Field::Role => &p.role,
        Field::Age => &p.age,
        Field::Pronouns => &p.pronouns,
        Field::Personality => &p.personality,
        Field::Backstory => &p.backstory,
        Field::Relationships => &p.relationships,
        Field::Face => &p.face,
        Field::Hair => &p.hair,
        Field::Eyes => &p.eyes,
        Field::Skin => &p.skin,
        Field::Build => &p.build,
        Field::DistinguishingFeatures => &p.distinguishing_features,
        Field::Consistency => &p.consistency,
        Field::Avoid => &p.avoid,
        _ => "",
    }
}

/// Commands for one library sheet edit: a character sheet, or the name, notes and prompt
/// of a background or image.
pub fn asset_edit(before: &Asset, after: &Asset) -> Option<Command> {
    if before.kind == AssetKind::Character {
        return character_edit(before, after);
    }
    let same_frame = before.id == after.id
        && before.kind == after.kind
        && before.width == after.width
        && before.height == after.height
        && before.character == after.character;
    if !same_frame {
        return None;
    }
    let commands = Field::allowed_on(ObjectKind::Asset)
        .iter()
        .filter(|field| asset_text(before, **field) != asset_text(after, **field))
        .map(|field| Command::SetField {
            target: Target::Object(before.id),
            field: *field,
            value: asset_text(after, *field).to_owned(),
        })
        .collect();
    Some(Command::Batch(commands))
}

/// Commands for one script-panel edit: text fields and position.
pub fn panel_edit(before: &Panel, after: &Panel) -> Option<Command> {
    if before.id != after.id {
        return None;
    }
    let text = |panel: &Panel, field: Field| -> String {
        match field {
            Field::Title => panel.title.clone(),
            Field::Script => panel.script.clone(),
            Field::Notes => panel.notes.clone(),
            _ => panel.prompt.clone(),
        }
    };
    let mut commands: Vec<_> = Field::allowed_on(ObjectKind::Panel)
        .iter()
        .filter(|field| text(before, **field) != text(after, **field))
        .map(|field| Command::SetField {
            target: Target::Object(before.id),
            field: *field,
            value: text(after, *field),
        })
        .collect();
    // Compare bits so a NaN or signed-zero change is never silently dropped.
    if before.y.to_bits() != after.y.to_bits() || before.height.to_bits() != after.height.to_bits()
    {
        commands.push(Command::SetPanelGeometry {
            panel: before.id,
            y: after.y,
            height: after.height,
        });
    }
    Some(Command::Batch(commands))
}

/// Commands for one group sheet edit: name, description, colour and lists.
pub fn group_edit(before: &WorldGroup, after: &WorldGroup) -> Option<Command> {
    if before.id != after.id {
        return None;
    }
    let target = Target::Object(before.id);
    let mut commands = Vec::new();
    if before.name != after.name {
        commands.push(Command::SetField {
            target,
            field: Field::Name,
            value: after.name.clone(),
        });
    }
    if before.description != after.description {
        commands.push(Command::SetField {
            target,
            field: Field::Description,
            value: after.description.clone(),
        });
    }
    if before.color != after.color {
        commands.push(Command::SetGroupColor {
            group: before.id,
            color: after.color,
        });
    }
    if before.character_ids != after.character_ids
        || before.location_ids != after.location_ids
        || before.reference_ids != after.reference_ids
    {
        commands.push(Command::SetGroupMembers {
            group: before.id,
            characters: after.character_ids.clone(),
            locations: after.location_ids.clone(),
            references: after.reference_ids.clone(),
        });
    }
    Some(Command::Batch(commands))
}

/// A location's whitelisted text, by field. Callers check the field is allowed first.
fn location_text(location: &crate::model::BackgroundLocation, field: Field) -> &str {
    let p = &location.profile;
    match field {
        Field::Name => &location.name,
        Field::Description => &p.description,
        Field::Atmosphere => &p.atmosphere,
        Field::Lighting => &p.lighting,
        Field::Consistency => &p.consistency,
        Field::Avoid => &p.avoid,
        Field::Prompt => &p.prompt,
        Field::Notes => &p.notes,
        _ => "",
    }
}

/// Commands that turn `before` into `after` for one location sheet edit.
pub fn location_edit(
    before: &crate::model::BackgroundLocation,
    after: &crate::model::BackgroundLocation,
) -> Option<Command> {
    if before.id != after.id || before.asset_ids != after.asset_ids {
        return None;
    }
    let commands = Field::allowed_on(ObjectKind::Location)
        .iter()
        .filter(|field| location_text(before, **field) != location_text(after, **field))
        .map(|field| Command::SetField {
            target: Target::Object(before.id),
            field: *field,
            value: location_text(after, *field).to_owned(),
        })
        .collect();
    Some(Command::Batch(commands))
}

impl PanelDraft {
    fn into_panel(self, id: u64) -> Panel {
        Panel {
            id,
            title: self.title,
            y: self.y,
            height: self.height,
            script: self.script,
            notes: self.notes,
            prompt: self.prompt,
        }
    }
}

impl From<Panel> for PanelDraft {
    fn from(panel: Panel) -> Self {
        Self {
            title: panel.title,
            y: panel.y,
            height: panel.height,
            script: panel.script,
            notes: panel.notes,
            prompt: panel.prompt,
        }
    }
}

fn check_panel_draft(panel: &PanelDraft, chapter_height: u32) -> Result<()> {
    check_text(&panel.title, SHORT)?;
    for text in [&panel.script, &panel.notes, &panel.prompt] {
        check_text(text, LONG)?;
    }
    ensure!(
        panel.y.is_finite()
            && panel.height.is_finite()
            && panel.y >= 0.0
            && panel.height >= 1.0
            && panel.y + panel.height <= chapter_height as f32,
        "The panel must fit inside the chapter"
    );
    Ok(())
}

fn new_asset(id: u64, name: String, kind: AssetKind, width: u32, height: u32) -> Asset {
    Asset {
        id,
        name,
        kind,
        notes: String::new(),
        prompt: String::new(),
        width,
        height,
        character: CharacterProfile::default(),
    }
}

#[cfg(test)]
mod tests;
