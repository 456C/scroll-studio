use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};

pub const FORMAT_VERSION: u32 = 9;
pub const MAX_POINTER_OFFSET: f32 = 10_000_000.0;
pub const MAX_BUBBLE_POINTERS: usize = 8;
pub type AssetBytes = HashMap<u64, Arc<Vec<u8>>>;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Story {
    pub version: u32,
    pub title: String,
    pub synopsis: String,
    #[serde(default)]
    pub visual_style: String,
    pub next_id: u64,
    pub chapters: Vec<Chapter>,
    pub assets: Vec<Asset>,
    #[serde(default)]
    pub locations: Vec<BackgroundLocation>,
    #[serde(default)]
    pub groups: Vec<WorldGroup>,
    #[serde(default)]
    pub timeline: Vec<ChronologyEntry>,
    #[serde(default)]
    pub bubble_presets: Vec<BubblePreset>,
}

/// A story-owned affiliation. References reuse existing original image assets.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorldGroup {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub color: [u8; 3],
    #[serde(default)]
    pub reference_ids: Vec<u64>,
    #[serde(default)]
    pub character_ids: Vec<u64>,
    #[serde(default)]
    pub location_ids: Vec<u64>,
}

/// Chronological order is independent of the chapters' reading order.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChronologyEntry {
    pub chapter_id: u64,
    pub when: String,
    pub summary: String,
}

/// Story-owned background folders. Assets outside these lists remain unsorted.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BackgroundLocation {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub asset_ids: Vec<u64>,
    #[serde(default)]
    pub profile: LocationProfile,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LocationProfile {
    pub description: String,
    pub atmosphere: String,
    pub lighting: String,
    pub consistency: String,
    pub notes: String,
    pub prompt: String,
    pub avoid: String,
}

/// Story-owned appearance. Layers keep their text, geometry and speaker targets.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BubblePreset {
    pub id: u64,
    pub name: String,
    pub style: TextStyle,
    pub color: [u8; 4],
    pub size: f32,
    pub bubble: bool,
    pub opacity: f32,
}

impl BubblePreset {
    /// Original 800 px lettering scale; kept stable for older story migration.
    pub const BASE_SIZE: f32 = 38.0;

    pub fn for_theme(id: u64, theme: LetteringTheme) -> Self {
        Self {
            id,
            name: theme.label().into(),
            style: TextStyle::for_theme(theme),
            color: [28, 31, 38, 255],
            size: Self::BASE_SIZE,
            bubble: true,
            opacity: 1.0,
        }
    }

    /// New presets keep the same reading size across working resolutions.
    /// Never apply this constructor to existing presets or loaded lettering.
    pub fn for_canvas(id: u64, theme: LetteringTheme, width: u32) -> Self {
        let mut preset = Self::for_theme(id, theme);
        let scale = width.clamp(64, 8192) as f32 / 800.0;
        preset.size *= scale;
        preset.style.max_width *= scale;
        preset.style.padding *= scale;
        preset.style.outline_width *= scale;
        preset.style.tail.width *= scale;
        preset
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Chapter {
    pub id: u64,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub panels: Vec<Panel>,
    pub layers: Vec<Layer>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Panel {
    pub id: u64,
    pub title: String,
    pub y: f32,
    pub height: f32,
    pub script: String,
    pub notes: String,
    pub prompt: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Asset {
    pub id: u64,
    pub name: String,
    pub kind: AssetKind,
    pub notes: String,
    pub prompt: String,
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub character: CharacterProfile,
}

/// Optional depth for a character. Empty fields never become prompt boilerplate.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CharacterProfile {
    pub tagline: String,
    pub role: String,
    pub age: String,
    #[serde(default)]
    pub gender: Option<CharacterGender>,
    /// Retained for older stories; no longer edited or included in generated prompts.
    pub pronouns: String,
    pub personality: String,
    pub backstory: String,
    pub relationships: String,
    pub face: String,
    pub hair: String,
    pub eyes: String,
    pub skin: String,
    pub build: String,
    pub distinguishing_features: String,
    pub consistency: String,
    pub avoid: String,
    pub outfits: Vec<CharacterOutfit>,
    pub active_outfit: Option<usize>,
    pub palette: Vec<CharacterSwatch>,
    /// Additional references. A character's own image remains its primary reference.
    pub references: Vec<CharacterReference>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CharacterGender {
    Female,
    Male,
}

impl CharacterGender {
    pub fn label(self) -> &'static str {
        match self {
            Self::Female => "Female",
            Self::Male => "Male",
        }
    }
}

#[cfg(test)]
mod character_gender_tests {
    use super::*;

    #[test]
    fn legacy_character_profile_retains_names_without_inventing_gender() {
        let legacy = serde_json::json!({
            "tagline": "A returning commuter",
            "pronouns": "she / her",
            "palette": [{ "name": "Coat · charcoal", "color": [40, 49, 54, 255] }],
            "outfits": [{ "name": "Rain commute", "description": "Charcoal coat" }]
        });
        let profile: CharacterProfile = serde_json::from_value(legacy).unwrap();
        assert_eq!(profile.gender, None);
        assert_eq!(profile.pronouns, "she / her");
        assert_eq!(profile.palette[0].name, "Coat · charcoal");
        assert_eq!(profile.outfits[0].name, "Rain commute");

        let saved = serde_json::to_value(&profile).unwrap();
        assert_eq!(saved["pronouns"], "she / her");
        assert_eq!(saved["palette"][0]["name"], "Coat · charcoal");
        assert_eq!(
            serde_json::from_value::<CharacterProfile>(saved).unwrap(),
            profile
        );
    }

    #[test]
    fn explicit_character_genders_round_trip() {
        assert_eq!(CharacterProfile::default().gender, None);
        for gender in [CharacterGender::Female, CharacterGender::Male] {
            let profile = CharacterProfile {
                gender: Some(gender),
                ..Default::default()
            };
            let saved = serde_json::to_value(&profile).unwrap();
            assert_eq!(saved["gender"], gender.label());
            assert_eq!(
                serde_json::from_value::<CharacterProfile>(saved).unwrap(),
                profile
            );
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CharacterOutfit {
    pub name: String,
    pub description: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CharacterSwatch {
    pub name: String,
    pub color: [u8; 4],
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CharacterReference {
    pub asset_id: u64,
    pub label: String,
    pub notes: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CharacterPromptKind {
    Portrait,
    Turnaround,
    Expressions,
    Scene,
}

impl CharacterPromptKind {
    pub const ALL: [Self; 4] = [
        Self::Portrait,
        Self::Turnaround,
        Self::Expressions,
        Self::Scene,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Portrait => "Portrait",
            Self::Turnaround => "Turnaround",
            Self::Expressions => "Expressions",
            Self::Scene => "Scene",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssetKind {
    Character,
    Background,
    Image,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    pub id: u64,
    pub name: String,
    pub visible: bool,
    pub locked: bool,
    pub opacity: f32,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub kind: LayerKind,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum LayerKind {
    Effect {
        effect: crate::effects::Effect,
    },
    Image {
        asset_id: u64,
        /// A non-destructive polygon in image-local normalized coordinates.
        /// Empty means the entire image is visible.
        mask: Vec<[f32; 2]>,
        /// Source-resolution selection and edge treatment; original bytes stay intact.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        clip: Option<Arc<crate::clipping::ImageClip>>,
        /// A view into the original source, used by reusable background snippets.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source_rect: Option<[f32; 4]>,
    },
    Stroke {
        /// Points begin in chapter coordinates; x/y provide a later move offset.
        points: Vec<[f32; 2]>,
        color: [u8; 4],
        size: f32,
    },
    Text {
        text: String,
        color: [u8; 4],
        size: f32,
        bubble: bool,
        #[serde(default)]
        style: TextStyle,
    },
    Shape {
        color: [u8; 4],
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FontChoice {
    Lato,
    BalsamiqSans,
    KomikaHand,
    Karla,
    ComicNeue,
    PatrickHand,
    DeliusUnicase,
}

impl FontChoice {
    pub const ALL: [Self; 7] = [
        Self::KomikaHand,
        Self::Lato,
        Self::BalsamiqSans,
        Self::ComicNeue,
        Self::PatrickHand,
        Self::DeliusUnicase,
        Self::Karla,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Lato => "Lato",
            Self::BalsamiqSans => "Balsamiq Sans",
            Self::KomikaHand => "Komika Hand",
            Self::Karla => "Karla",
            Self::ComicNeue => "Comic Neue",
            Self::PatrickHand => "Patrick Hand",
            Self::DeliusUnicase => "Delius Unicase",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LetteringTheme {
    Mature,
    Casual,
    WebtoonClassic,
}

impl LetteringTheme {
    pub const ALL: [Self; 3] = [Self::Mature, Self::Casual, Self::WebtoonClassic];

    pub fn label(self) -> &'static str {
        match self {
            Self::Mature => "Mature",
            Self::Casual => "Casual",
            Self::WebtoonClassic => "Webtoon Classic",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BubbleShape {
    Speech,
    Rounded,
    Thought,
    Shout,
    Whisper,
    Narration,
    Electronic,
    Intensity,
    Impact,
}

impl BubbleShape {
    pub const ALL: [Self; 9] = [
        Self::Speech,
        Self::Rounded,
        Self::Thought,
        Self::Shout,
        Self::Whisper,
        Self::Narration,
        Self::Electronic,
        Self::Intensity,
        Self::Impact,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Speech => "Speech",
            Self::Rounded => "Rounded",
            Self::Thought => "Thought",
            Self::Shout => "Shout",
            Self::Whisper => "Whisper",
            Self::Narration => "Narration",
            Self::Electronic => "Electronic",
            Self::Intensity => "Intensity",
            Self::Impact => "Impact",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

impl TextAlign {
    pub const ALL: [Self; 3] = [Self::Left, Self::Center, Self::Right];
    pub fn label(self) -> &'static str {
        match self {
            Self::Left => "Left",
            Self::Center => "Center",
            Self::Right => "Right",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BubbleTail {
    pub enabled: bool,
    /// Tip relative to the bubble body. Values outside 0..1 extend beyond it.
    pub tip: [f32; 2],
    pub width: f32,
    pub bend: f32,
}

impl Default for BubbleTail {
    fn default() -> Self {
        Self {
            enabled: false,
            tip: [0.7, 1.5],
            width: 36.0,
            bend: 0.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BubbleJoin {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub shape: BubbleShape,
    pub painted_before: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextStyle {
    pub preset_id: Option<u64>,
    pub cluster_id: Option<u64>,
    /// Derived render geometry. Story files store only the cluster association.
    #[serde(skip)]
    pub joins: Vec<BubbleJoin>,
    pub font: FontChoice,
    /// Case changes are for display only; the typed dialogue stays editable.
    pub uppercase: bool,
    /// Natural fitting is opt-in for older stories to preserve their layout.
    pub auto_fit: bool,
    pub max_width: f32,
    /// Preserve legacy wrapping until an older text layer is explicitly refitted.
    pub modern_layout: bool,
    pub shape: BubbleShape,
    pub fill: [u8; 4],
    pub outline: [u8; 4],
    pub outline_width: f32,
    pub padding: f32,
    pub line_spacing: f32,
    pub align: TextAlign,
    pub tail: BubbleTail,
    /// Additional independently aimed pointers, after the original first pointer.
    pub extra_tails: Vec<BubbleTail>,
}

impl Default for TextStyle {
    /// Older .scro files retain their original font, shape and tail-free layout.
    fn default() -> Self {
        Self {
            preset_id: None,
            cluster_id: None,
            joins: Vec::new(),
            font: FontChoice::Karla,
            uppercase: false,
            auto_fit: false,
            max_width: 640.0,
            modern_layout: false,
            shape: BubbleShape::Rounded,
            fill: [255; 4],
            outline: [35, 35, 40, 255],
            outline_width: 2.0,
            padding: 12.0,
            line_spacing: 1.2,
            align: TextAlign::Left,
            tail: BubbleTail::default(),
            extra_tails: Vec::new(),
        }
    }
}

impl TextStyle {
    pub fn dialogue() -> Self {
        Self::for_theme(LetteringTheme::WebtoonClassic)
    }

    pub fn for_theme(theme: LetteringTheme) -> Self {
        let (font, uppercase, padding, line_spacing) = match theme {
            LetteringTheme::Mature => (FontChoice::Lato, false, 22.0, 1.15),
            LetteringTheme::Casual => (FontChoice::BalsamiqSans, false, 24.0, 1.2),
            LetteringTheme::WebtoonClassic => (FontChoice::KomikaHand, true, 22.0, 1.12),
        };
        Self {
            font,
            uppercase,
            auto_fit: true,
            modern_layout: true,
            shape: BubbleShape::Speech,
            padding,
            line_spacing,
            align: TextAlign::Center,
            tail: BubbleTail {
                enabled: true,
                ..BubbleTail::default()
            },
            ..Self::default()
        }
    }

    pub fn theme(&self) -> Option<LetteringTheme> {
        match (self.font, self.uppercase) {
            (FontChoice::Lato, false) => Some(LetteringTheme::Mature),
            (FontChoice::BalsamiqSans, false) => Some(LetteringTheme::Casual),
            (FontChoice::KomikaHand, true) => Some(LetteringTheme::WebtoonClassic),
            _ => None,
        }
    }
    pub fn tail_enabled(&self, bubble: bool) -> bool {
        bubble && self.tail.enabled && self.shape != BubbleShape::Narration
    }

    pub fn tails(&self) -> impl Iterator<Item = &BubbleTail> {
        std::iter::once(&self.tail).chain(self.extra_tails.iter())
    }

    pub fn tails_mut(&mut self) -> impl Iterator<Item = &mut BubbleTail> {
        std::iter::once(&mut self.tail).chain(self.extra_tails.iter_mut())
    }

    pub fn tail_at(&self, index: usize) -> Option<&BubbleTail> {
        if index == 0 {
            Some(&self.tail)
        } else {
            self.extra_tails.get(index - 1)
        }
    }

    pub fn tail_at_mut(&mut self, index: usize) -> Option<&mut BubbleTail> {
        if index == 0 {
            Some(&mut self.tail)
        } else {
            self.extra_tails.get_mut(index - 1)
        }
    }
}

impl Chapter {
    pub fn new(id: u64, name: impl Into<String>, width: u32, height: u32) -> Self {
        Self {
            id,
            name: name.into(),
            width,
            height,
            panels: Vec::new(),
            layers: Vec::new(),
        }
    }
}

impl Story {
    pub fn new(title: impl Into<String>) -> Self {
        Self::with_canvas(title, 1600, 9600)
    }

    pub fn with_canvas(title: impl Into<String>, width: u32, height: u32) -> Self {
        let mut story = Self {
            version: FORMAT_VERSION,
            title: title.into(),
            synopsis: String::new(),
            visual_style: String::new(),
            next_id: 1,
            chapters: Vec::new(),
            assets: Vec::new(),
            locations: Vec::new(),
            groups: Vec::new(),
            timeline: Vec::new(),
            bubble_presets: Vec::new(),
        };
        let mut chapter = Chapter::new(story.alloc_id(), "Chapter 01", width, height);
        for (title, y) in [
            ("The opening", 200.0),
            ("The moment", 2000.0),
            ("What happens next", 3900.0),
        ] {
            chapter.panels.push(Panel {
                id: story.alloc_id(),
                title: title.into(),
                y,
                height: 1500.0,
                script: String::new(),
                notes: String::new(),
                prompt: String::new(),
            });
        }
        story.chapters.push(chapter);
        for theme in [
            LetteringTheme::WebtoonClassic,
            LetteringTheme::Mature,
            LetteringTheme::Casual,
        ] {
            let id = story.alloc_id();
            story
                .bubble_presets
                .push(BubblePreset::for_canvas(id, theme, width));
        }
        story
    }

    pub fn alloc_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id = self
            .next_id
            .checked_add(1)
            .expect("story ID space exhausted");
        id
    }

    /// An editable sample made entirely from small vector elements.
    pub fn demo() -> Self {
        let mut story = Self::new("After the rain");
        story.synopsis = "A quiet city. An unexpected letter. A new beginning.".into();
        story.chapters[0].name = "01 · The last train".into();
        story.chapters[0].height = 5800;
        story.visual_style = "Rainy urban mystery. Clean expressive webtoon linework, restrained adult proportions, muted teal shadows and warm amber lights. Keep the charcoal coat and cream envelope consistent. No lettering or watermarks.".into();
        let scripts = [
            ("01 · A city holding its breath", "WIDE SHOT. Rain hangs over the city. The station is almost empty. One last train is about to leave.", "Let the opening breathe. Keep the palette muted teal with one warm light.", "Vertical webtoon establishing shot, rainy train station at blue hour, atmospheric teal, warm amber window light, clean expressive linework, no text."),
            ("02 · The letter", "CLOSE-UP. Mira finds an envelope tucked into her coat. On the front: ‘If you miss the last train, find me.’", "Keep the envelope readable. Use the character sheet to match Mira’s coat.", "Close-up of a young woman’s hand holding a cream envelope, rain-speckled dark coat, soft cinematic light, webtoon illustration, no lettering."),
            ("03 · One more stop", "MIRA: ‘Then I guess I’m walking.’ She turns away from the platform. Across the street, a café light flickers on.", "Leave a generous gutter before the reveal. A small smile is enough.", "Young woman in a dark coat turning toward a tiny warmly lit café in a rainy city, expressive webtoon composition, teal and amber palette, no text."),
        ];
        for (panel, (title, script, notes, prompt)) in
            story.chapters[0].panels.iter_mut().zip(scripts)
        {
            panel.title = title.into();
            panel.script = script.into();
            panel.notes = notes.into();
            panel.prompt = prompt.into();
        }
        let mut add = |name: &str, x, y, width, height, kind| {
            let id = story.alloc_id();
            story.chapters[0].layers.push(Layer {
                id,
                name: name.into(),
                visible: true,
                locked: false,
                opacity: 1.0,
                x,
                y,
                width,
                height,
                kind,
            });
        };
        add(
            "Blue hour",
            100.0,
            200.0,
            1400.0,
            1500.0,
            LayerKind::Shape {
                color: [33, 61, 68, 255],
            },
        );
        add(
            "Distant skyline",
            100.0,
            830.0,
            1400.0,
            870.0,
            LayerKind::Shape {
                color: [24, 44, 51, 255],
            },
        );
        for (i, (x, top, width)) in [
            (160.0, 610.0, 170.0),
            (380.0, 450.0, 220.0),
            (640.0, 700.0, 260.0),
            (980.0, 500.0, 180.0),
            (1230.0, 740.0, 170.0),
        ]
        .into_iter()
        .enumerate()
        {
            add(
                &format!("Building {}", i + 1),
                x,
                top,
                width,
                1150.0 - top,
                LayerKind::Shape {
                    color: [39, 72, 77, 255],
                },
            );
            add(
                &format!("Window {}", i + 1),
                x + 35.0,
                top + 55.0,
                35.0,
                62.0,
                LayerKind::Shape {
                    color: [226, 181, 112, 255],
                },
            );
        }
        add(
            "Platform",
            100.0,
            1420.0,
            1400.0,
            280.0,
            LayerKind::Shape {
                color: [18, 33, 41, 255],
            },
        );
        add(
            "Station light",
            0.0,
            0.0,
            1400.0,
            1500.0,
            LayerKind::Stroke {
                points: vec![[180.0, 1250.0], [1420.0, 1250.0]],
                color: [232, 183, 103, 255],
                size: 10.0,
            },
        );
        for i in 0..24 {
            let x = 140.0 + (i as f32 * 139.0) % 1290.0;
            let y = 260.0 + (i as f32 * 173.0) % 1120.0;
            add(
                &format!("Rain {}", i + 1),
                0.0,
                0.0,
                1400.0,
                1500.0,
                LayerKind::Stroke {
                    points: vec![[x, y], [x - 20.0, y + 100.0]],
                    color: [122, 162, 169, 100],
                    size: 3.0,
                },
            );
        }
        add(
            "Opening caption",
            260.0,
            350.0,
            950.0,
            180.0,
            LayerKind::Text {
                text: "The city always felt different\nafter the rain.".into(),
                color: [243, 236, 218, 255],
                size: 42.0,
                bubble: false,
                style: TextStyle::default(),
            },
        );
        add(
            "Letter scene",
            100.0,
            2000.0,
            1400.0,
            1500.0,
            LayerKind::Shape {
                color: [215, 210, 196, 255],
            },
        );
        add(
            "Coat",
            400.0,
            2250.0,
            820.0,
            1250.0,
            LayerKind::Shape {
                color: [46, 62, 66, 255],
            },
        );
        add(
            "Envelope",
            300.0,
            2570.0,
            990.0,
            500.0,
            LayerKind::Shape {
                color: [242, 229, 200, 255],
            },
        );
        add(
            "Envelope fold",
            0.0,
            0.0,
            990.0,
            500.0,
            LayerKind::Stroke {
                points: vec![[310.0, 2580.0], [795.0, 2850.0], [1280.0, 2580.0]],
                color: [190, 170, 135, 255],
                size: 5.0,
            },
        );
        add(
            "Letter caption",
            390.0,
            3140.0,
            840.0,
            200.0,
            LayerKind::Text {
                text: "If you miss the last train,\nfind me.".into(),
                color: [40, 47, 49, 255],
                size: 42.0,
                bubble: true,
                style: TextStyle::default(),
            },
        );
        add(
            "The café",
            100.0,
            3900.0,
            1400.0,
            1500.0,
            LayerKind::Shape {
                color: [38, 66, 70, 255],
            },
        );
        add(
            "Café window",
            540.0,
            4200.0,
            610.0,
            800.0,
            LayerKind::Shape {
                color: [224, 170, 93, 255],
            },
        );
        add(
            "Window frame",
            810.0,
            4200.0,
            32.0,
            800.0,
            LayerKind::Shape {
                color: [58, 63, 57, 255],
            },
        );
        add(
            "Mira silhouette",
            250.0,
            4590.0,
            240.0,
            810.0,
            LayerKind::Shape {
                color: [20, 38, 43, 255],
            },
        );
        add(
            "Mira dialogue",
            190.0,
            4020.0,
            930.0,
            190.0,
            LayerKind::Text {
                text: "Then I guess I’m walking.".into(),
                color: [40, 47, 49, 255],
                size: 40.0,
                bubble: true,
                style: TextStyle::default(),
            },
        );
        for (name, kind, notes, prompt) in [
            ("Mira", AssetKind::Character, "Observant, quietly stubborn. Short dark hair, oversized charcoal coat, canvas shoulder bag. Keep her silhouette consistent.", "Character reference sheet, young adult woman, short dark hair, oversized charcoal raincoat, canvas bag, front side and back views, clean webtoon linework, neutral background."),
            ("Rainy station", AssetKind::Background, "A nearly empty station at blue hour. Muted teal structures, warm platform lights, reflective pavement.", "Rainy city train station background, blue hour, teal shadows, warm amber station lights, atmospheric webtoon style, no characters, no text."),
        ] {
            let id = story.alloc_id();
            story.assets.push(Asset { id, name: name.into(), kind, notes: notes.into(), prompt: prompt.into(), width: 0, height: 0, character: CharacterProfile::default() });
        }
        // This is a newly generated demonstration, not migration of a saved story.
        // Judge its lettering at the same reading scale as new 1600 px chapters.
        let classic = story.bubble_presets[0].clone();
        let mut caption = story.bubble_presets[1].clone();
        caption.id = story.alloc_id();
        caption.name = "Caption".into();
        caption.bubble = false;
        caption.color = [243, 236, 218, 255];
        caption.style.max_width = 1120.0;
        caption.style.tail.enabled = false;
        let mut letter = story.bubble_presets[1].clone();
        letter.id = story.alloc_id();
        letter.name = "Letter".into();
        letter.style.shape = BubbleShape::Narration;
        letter.style.max_width = 1150.0;
        letter.style.tail.enabled = false;
        letter.style.fill = [242, 229, 200, 255];
        for layer in &mut story.chapters[0].layers {
            let preset = match layer.name.as_str() {
                "Opening caption" => &caption,
                "Letter caption" => &letter,
                "Mira dialogue" => &classic,
                _ => continue,
            };
            crate::bubbles::initialize_layer(layer, preset).expect("valid demo lettering");
            layer.x = (1600.0 - layer.width) * 0.5;
            if layer.name == "Mira dialogue" {
                if let LayerKind::Text { style, .. } = &mut layer.kind {
                    style.tail.tip = [
                        (370.0 - layer.x) / layer.width,
                        (4620.0 - layer.y) / layer.height,
                    ];
                }
            }
        }
        story.bubble_presets.extend([caption, letter]);
        story
    }
}

#[cfg(test)]
mod demo_tests {
    use super::*;

    #[test]
    fn new_demo_lettering_is_readable_fitted_and_linked_inside_its_scenes() {
        let story = Story::demo();
        let chapter = &story.chapters[0];
        let mut count = 0;
        for layer in &chapter.layers {
            let LayerKind::Text { size, style, .. } = &layer.kind else {
                continue;
            };
            count += 1;
            assert!(*size * 360.0 / chapter.width as f32 >= 16.0);
            assert!(!crate::compositor::text_overflows(layer));
            assert!(story
                .bubble_presets
                .iter()
                .any(|preset| Some(preset.id) == style.preset_id));
            let [x, y, width, height] = crate::compositor::text_bounds(layer);
            let (top, bottom) = match layer.name.as_str() {
                "Opening caption" => (200.0, 1700.0),
                "Letter caption" => (2000.0, 3500.0),
                "Mira dialogue" => (3900.0, 5400.0),
                _ => panic!("Unexpected demo text"),
            };
            assert!(
                x >= 100.0 && x + width <= 1500.0,
                "{} exceeds its panel horizontally",
                layer.name
            );
            assert!(
                y >= top && y + height <= bottom,
                "{} exceeds its scene: {y} + {height}",
                layer.name
            );
        }
        assert_eq!(count, 3);
        assert!(!story.visual_style.is_empty());
    }
}
