//! The .scro container is a ZIP with a versioned JSON manifest and original image bytes.
//! Images are stored once and referenced by stable IDs, even when reused in many chapters.
use crate::model::{
    AssetBytes, AssetKind, LayerKind, Story, TextStyle, FORMAT_VERSION, MAX_BUBBLE_POINTERS,
    MAX_POINTER_OFFSET,
};
use anyhow::{bail, ensure, Context, Result};
use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipArchive, ZipWriter};

const MAX_MANIFEST_BYTES: u64 = 16 * 1024 * 1024;
const MAX_ASSET_BYTES: u64 = 64 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 512 * 1024 * 1024;
const MAX_ENTRIES: usize = 4097;
const MAX_IMAGE_PIXELS: u64 = 32_000_000;
const MAX_IMAGE_SIDE: u32 = 16_384;
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct FileStamp {
    length: u64,
    modified: std::time::SystemTime,
}
impl FileStamp {
    fn read(metadata: fs::Metadata) -> Result<Self> {
        Ok(Self {
            length: metadata.len(),
            modified: metadata.modified()?,
        })
    }
}

pub fn save_story(path: &Path, story: &Story, assets: &AssetBytes) -> Result<()> {
    save_story_checked(path, story, assets, None).map(|_| ())
}

pub fn save_story_checked(
    path: &Path,
    story: &Story,
    assets: &AssetBytes,
    expected: Option<FileStamp>,
) -> Result<FileStamp> {
    validate_story(story, assets)?;
    let manifest = serde_json::to_vec(story).context("Could not encode the story")?;
    ensure!(
        manifest.len() as u64 <= MAX_MANIFEST_BYTES,
        "Story manifest is too large"
    );
    let (temporary_path, file) = create_temporary(path)?;
    let result = (|| -> Result<FileStamp> {
        let mut zip = ZipWriter::new(file);
        zip.start_file(
            "manifest.json",
            SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
        )?;
        zip.write_all(&manifest)?;
        // Already compressed PNG/JPEG/WebP data does not benefit from ZIP recompression.
        let image_options =
            SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        for asset in &story.assets {
            if let Some(bytes) = assets.get(&asset.id) {
                zip.start_file(format!("assets/{}.bin", asset.id), image_options)?;
                zip.write_all(bytes.as_slice())?;
            }
        }
        let file = zip.finish().context("Could not finish the story archive")?;
        file.sync_all()
            .context("Could not flush the story archive")?;
        let stamp = FileStamp::read(file.metadata()?)?;
        drop(file);
        // Serialize cooperating editors on the original file while checking and replacing it.
        let _guard = if let Some(expected) = expected {
            let original = File::open(path)
                .context("The original file is unavailable. Use Save as to keep your changes.")?;
            original.lock()?;
            ensure!(FileStamp::read(fs::metadata(path)?)? == expected,
                "Another window changed this file. Use Save as to keep your edits in a separate copy.");
            Some(original)
        } else {
            None
        };
        // Same-directory rename replaces the old file only after the new archive is complete.
        fs::rename(&temporary_path, path).with_context(|| {
            format!(
                "Could not save {}. The previous file is unchanged.",
                path.display()
            )
        })?;
        Ok(stamp)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary_path);
    }
    result
}

pub fn load_story(path: &Path) -> Result<(Story, AssetBytes)> {
    load_story_with_stamp(path).map(|(story, assets, _)| (story, assets))
}

pub fn load_story_with_stamp(path: &Path) -> Result<(Story, AssetBytes, FileStamp)> {
    let file = File::open(path).with_context(|| format!("Could not open {}", path.display()))?;
    let stamp = FileStamp::read(file.metadata()?)?;
    ensure!(
        stamp.length <= MAX_TOTAL_BYTES,
        "Story file exceeds the 512 MiB safety limit"
    );
    let mut zip = ZipArchive::new(file).context("This is not a valid .scro story archive")?;
    ensure!(
        zip.len() <= MAX_ENTRIES,
        "Story contains too many archive entries"
    );
    let mut names = HashSet::new();
    let mut total_size = 0u64;
    for index in 0..zip.len() {
        let entry = zip.by_index(index)?;
        ensure!(
            names.insert(entry.name().to_string()),
            "Story contains duplicate archive entries"
        );
        let limit = if entry.name() == "manifest.json" {
            MAX_MANIFEST_BYTES
        } else {
            MAX_ASSET_BYTES
        };
        ensure!(
            entry.size() <= limit,
            "Archive entry is too large: {}",
            entry.name()
        );
        total_size = total_size
            .checked_add(entry.size())
            .context("Invalid archive size")?;
        ensure!(
            total_size <= MAX_TOTAL_BYTES,
            "Story expands beyond the 512 MiB safety limit"
        );
        ensure!(!entry.is_dir(), "Unexpected directory in story archive");
        ensure!(
            entry.name() == "manifest.json" || parse_asset_path(entry.name()).is_some(),
            "Unexpected entry in story archive: {}",
            entry.name()
        );
    }
    let manifest = read_entry(&mut zip, "manifest.json", MAX_MANIFEST_BYTES)?;
    let mut story: Story =
        serde_json::from_slice(&manifest).context("Story manifest is invalid")?;
    ensure!(
        (1..=FORMAT_VERSION).contains(&story.version),
        "Unsupported .scro version {} (this app supports versions 1 through {})",
        story.version,
        FORMAT_VERSION
    );
    // Missing lettering settings use their legacy defaults. Current saves reject
    // older applications rather than silently losing fonts, fitting behavior,
    // location membership or explicit character gender.
    let migrate_lore = story.version < 11;
    story.version = FORMAT_VERSION;
    let mut assets = AssetBytes::new();
    let asset_ids: HashSet<_> = story.assets.iter().map(|asset| asset.id).collect();
    for name in names {
        if let Some(id) = parse_asset_path(&name) {
            ensure!(
                asset_ids.contains(&id),
                "Archive contains an image absent from the manifest"
            );
            let bytes = read_entry(&mut zip, &name, MAX_ASSET_BYTES)?;
            assets.insert(id, Arc::new(bytes));
        }
    }
    validate_story(&story, &assets)?;
    crate::bubbles::migrate_story_presets(&mut story)?;
    if migrate_lore {
        crate::lore::migrate_board(&mut story)?;
    }
    validate_story(&story, &assets)?;
    Ok((story, assets, stamp))
}

fn parse_asset_path(name: &str) -> Option<u64> {
    let value = name.strip_prefix("assets/")?.strip_suffix(".bin")?;
    let id: u64 = value.parse().ok()?;
    (id > 0 && value == id.to_string()).then_some(id)
}

fn read_entry(zip: &mut ZipArchive<File>, name: &str, maximum: u64) -> Result<Vec<u8>> {
    let entry = zip
        .by_name(name)
        .with_context(|| format!("Story is missing {name}"))?;
    ensure!(
        entry.size() <= maximum,
        "Archive entry exceeds its size limit"
    );
    let mut bytes = Vec::with_capacity(entry.size().min(maximum) as usize);
    entry.take(maximum + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= maximum,
        "Archive entry exceeds its size limit"
    );
    Ok(bytes)
}

pub fn save_painting_png(path: &Path, image: &image::RgbaImage) -> Result<()> {
    let (temporary, mut file) = create_temporary(path)?;
    let result = (|| -> Result<()> {
        image.write_to(&mut file, image::ImageFormat::Png)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

fn create_temporary(destination: &Path) -> Result<(PathBuf, File)> {
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let name = destination
        .file_name()
        .context("Choose a story filename")?
        .to_string_lossy();
    for _ in 0..100 {
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(".{name}.{}.{}.tmp", std::process::id(), counter));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error).context("Could not create a temporary story file"),
        }
    }
    bail!("Could not allocate a temporary story file")
}

/// Validate both manifests loaded from disk and in-memory stories before saving.
pub fn validate_story(story: &Story, assets: &AssetBytes) -> Result<()> {
    ensure!(
        story.version == FORMAT_VERSION,
        "Unsupported .scro version {}",
        story.version
    );
    ensure!(
        !story.chapters.is_empty(),
        "A story must contain at least one chapter"
    );
    ensure!(
        story.assets.len() < MAX_ENTRIES
            && story.chapters.len() <= 1000
            && story.bubble_presets.len() <= 4096,
        "Story has too many assets or chapters"
    );
    ensure!(
        story.title.len() <= 4096
            && story.synopsis.len() <= 1_000_000
            && story.visual_style.len() <= 1_000_000,
        "Story metadata is too large"
    );
    crate::locations::validate_locations(story)?;
    crate::groups::validate(story)?;
    crate::timeline::validate(story)?;
    let mut ids = HashSet::new();
    let mut max_id = 0;
    let mut insert_id = |id| -> Result<()> {
        ensure!(
            id > 0 && ids.insert(id),
            "Story contains a missing or duplicate ID"
        );
        max_id = max_id.max(id);
        Ok(())
    };
    let asset_ids: HashSet<_> = story.assets.iter().map(|asset| asset.id).collect();
    let preset_ids: HashSet<_> = story
        .bubble_presets
        .iter()
        .map(|preset| preset.id)
        .collect();
    let reference_image_ids: HashSet<_> = story
        .assets
        .iter()
        .filter(|asset| {
            asset.kind == AssetKind::Image
                && asset.width > 0
                && asset.height > 0
                && assets.contains_key(&asset.id)
        })
        .map(|asset| asset.id)
        .collect();
    ensure!(story.lore.len() <= 4096, "Too many lore entries");
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
                .filter(|a| a.kind == AssetKind::Character)
                .map(|a| a.id),
        )
        .collect();
    for entry in &story.lore {
        insert_id(entry.id)?;
        ensure!(
            entry.title.len() <= 4096 && entry.text.len() <= 100_000,
            "Lore entry is too long"
        );
        let mut unique = HashSet::new();
        ensure!(
            entry
                .links
                .iter()
                .all(|id| *id != entry.id && link_targets.contains(id) && unique.insert(id)),
            "Invalid lore connection"
        );
    }
    crate::lore::validate_board(story)?;
    for card in &story.lore_board.cards {
        insert_id(card.id)?;
    }
    for connection in &story.lore_board.connections {
        insert_id(connection.id)?;
    }
    for preset in &story.bubble_presets {
        insert_id(preset.id)?;
        ensure!(
            preset.name.len() <= 4096 && !preset.name.trim().is_empty(),
            "Invalid bubble preset name"
        );
        ensure!(
            preset.size.is_finite() && (1.0..=4096.0).contains(&preset.size),
            "Invalid bubble preset text size"
        );
        ensure!(
            preset.opacity.is_finite() && (0.0..=1.0).contains(&preset.opacity),
            "Invalid bubble preset opacity"
        );
        ensure!(
            preset.style.preset_id.is_none()
                && preset.style.extra_tails.is_empty()
                && preset.style.cluster_id.is_none(),
            "A bubble preset cannot link to another preset or own extra speaker targets"
        );
        validate_text_style(&preset.style, 640.0, 160.0)?;
    }
    for location in &story.locations {
        insert_id(location.id)?;
    }
    for group in &story.groups {
        insert_id(group.id)?;
    }
    let mut total_bytes = 0u64;
    for asset in &story.assets {
        insert_id(asset.id)?;
        ensure!(
            asset.name.len() <= 4096
                && asset.notes.len() <= 1_000_000
                && asset.prompt.len() <= 1_000_000,
            "Asset metadata is too large"
        );
        let profile = &asset.character;
        ensure!(
            [
                &profile.tagline,
                &profile.role,
                &profile.age,
                &profile.pronouns,
                &profile.personality,
                &profile.backstory,
                &profile.relationships,
                &profile.face,
                &profile.hair,
                &profile.eyes,
                &profile.skin,
                &profile.build,
                &profile.distinguishing_features,
                &profile.consistency,
                &profile.avoid
            ]
            .iter()
            .all(|text| text.len() <= 1_000_000),
            "Character details are too large"
        );
        ensure!(
            profile.references.len() <= 24
                && profile.outfits.len() <= 32
                && profile.palette.len() <= 16,
            "Character has too many references, outfits or colors"
        );
        ensure!(
            profile
                .active_outfit
                .is_none_or(|index| index < profile.outfits.len()),
            "Character's selected outfit is missing"
        );
        ensure!(
            profile
                .outfits
                .iter()
                .all(|outfit| outfit.name.len() <= 4096 && outfit.description.len() <= 1_000_000),
            "Character outfit text is too large"
        );
        ensure!(
            profile
                .palette
                .iter()
                .all(|swatch| swatch.name.len() <= 4096),
            "Character palette name is too large"
        );
        let mut references = HashSet::new();
        for reference in &profile.references {
            ensure!(
                asset.kind == AssetKind::Character
                    && reference.asset_id != asset.id
                    && references.insert(reference.asset_id),
                "Invalid or duplicate character reference"
            );
            ensure!(
                reference.label.len() <= 4096 && reference.notes.len() <= 1_000_000,
                "Character reference text is too large"
            );
            ensure!(
                reference_image_ids.contains(&reference.asset_id),
                "Character reference image is missing"
            );
        }
        ensure!(
            (asset.width == 0 && asset.height == 0)
                || (asset.width > 0
                    && asset.height > 0
                    && asset.width <= MAX_IMAGE_SIDE
                    && asset.height <= MAX_IMAGE_SIDE
                    && u64::from(asset.width) * u64::from(asset.height) <= MAX_IMAGE_PIXELS),
            "Invalid image dimensions for {}",
            asset.name
        );
        ensure!(
            assets.contains_key(&asset.id)
                || (asset.width == 0 && asset.height == 0 && asset.kind != AssetKind::Image),
            "Story is missing the image for {}",
            asset.name
        );
        if let Some(bytes) = assets.get(&asset.id) {
            ensure!(
                !bytes.is_empty() && bytes.len() as u64 <= MAX_ASSET_BYTES,
                "Image is empty or larger than 64 MiB: {}",
                asset.name
            );
            total_bytes += bytes.len() as u64;
            ensure!(
                total_bytes <= MAX_TOTAL_BYTES - MAX_MANIFEST_BYTES,
                "Story image data exceeds its memory limit"
            );
            let reader =
                image::ImageReader::new(Cursor::new(bytes.as_slice())).with_guessed_format()?;
            ensure!(
                matches!(
                    reader.format(),
                    Some(
                        image::ImageFormat::Png
                            | image::ImageFormat::Jpeg
                            | image::ImageFormat::WebP
                    )
                ),
                "Unsupported image format for {}",
                asset.name
            );
            let dimensions = reader
                .into_dimensions()
                .with_context(|| format!("Invalid image data for {}", asset.name))?;
            ensure!(
                dimensions == (asset.width, asset.height),
                "Image dimensions do not match its sheet: {}",
                asset.name
            );
        }
    }
    ensure!(
        assets.keys().all(|id| asset_ids.contains(id)),
        "Image bytes have no matching asset sheet"
    );
    for chapter in &story.chapters {
        insert_id(chapter.id)?;
        ensure!(
            (64..=8192).contains(&chapter.width) && (64..=200_000).contains(&chapter.height),
            "Chapter canvas dimensions are outside supported limits"
        );
        ensure!(
            chapter.name.len() <= 4096
                && chapter.panels.len() <= 10_000
                && chapter.layers.len() <= 100_000,
            "Chapter is too large"
        );
        for panel in &chapter.panels {
            insert_id(panel.id)?;
            ensure!(
                finite(panel.y) && finite(panel.height) && panel.height > 0.0,
                "Invalid script beat dimensions"
            );
            ensure!(
                panel.title.len() <= 4096
                    && panel.script.len() <= 1_000_000
                    && panel.notes.len() <= 1_000_000
                    && panel.prompt.len() <= 1_000_000,
                "Script beat text is too large"
            );
        }
        for layer in &chapter.layers {
            insert_id(layer.id)?;
            ensure!(
                layer.name.len() <= 4096
                    && finite(layer.x)
                    && finite(layer.y)
                    && finite(layer.width)
                    && finite(layer.height)
                    && layer.width > 0.0
                    && layer.height > 0.0,
                "Invalid layer dimensions"
            );
            ensure!(
                layer.opacity.is_finite() && (0.0..=1.0).contains(&layer.opacity),
                "Invalid layer opacity"
            );
            match &layer.kind {
                LayerKind::Effect { effect } => {
                    ensure!(effect.validate(), "Invalid effect parameters")
                }
                LayerKind::Image {
                    asset_id,
                    mask,
                    clip,
                    source_rect,
                } => {
                    if let Some([x, y, w, h]) = source_rect {
                        ensure!(
                            [x, y, w, h].iter().all(|n| n.is_finite())
                                && *x >= 0.0
                                && *y >= 0.0
                                && *w > 0.0
                                && *h > 0.0
                                && x + w <= 1.00001
                                && y + h <= 1.00001,
                            "Invalid image snippet crop"
                        );
                    }

                    if let Some(clip) = clip {
                        let asset = story
                            .assets
                            .iter()
                            .find(|asset| asset.id == *asset_id)
                            .context("Missing selection asset")?;
                        clip.validate(asset.width, asset.height)?;
                        ensure!(
                            mask.is_empty(),
                            "An image cannot contain both legacy and patterned selections"
                        );
                    }
                    ensure!(
                        asset_ids.contains(asset_id) && assets.contains_key(asset_id),
                        "Image layer references a missing image asset"
                    );
                    ensure!(
                        (mask.is_empty() || mask.len() >= 3) && mask.len() <= 100_000,
                        "Invalid lasso polygon"
                    );
                    ensure!(
                        mask.iter()
                            .flatten()
                            .all(|n| n.is_finite() && (0.0..=1.0).contains(n)),
                        "Lasso coordinates must be normalized"
                    );
                }
                LayerKind::Stroke { points, size, .. } => {
                    ensure!(
                        points.len() <= 1_000_000 && points.iter().flatten().all(|n| finite(*n)),
                        "Invalid brush stroke"
                    );
                    ensure!(
                        size.is_finite() && (0.1..=4096.0).contains(size),
                        "Invalid brush size"
                    );
                }
                LayerKind::Text {
                    text, size, style, ..
                } => {
                    ensure!(
                        text.len() <= 1_000_000
                            && size.is_finite()
                            && (1.0..=4096.0).contains(size),
                        "Invalid text layer"
                    );
                    ensure!(
                        style.preset_id.is_none_or(|id| preset_ids.contains(&id)),
                        "Text layer references a missing bubble preset"
                    );
                    validate_text_style(style, layer.width, layer.height)?;
                }
                LayerKind::Shape { .. } => {}
            }
        }
    }
    ensure!(
        story.next_id > max_id && story.next_id < u64::MAX,
        "Story ID counter is invalid"
    );
    Ok(())
}

fn validate_text_style(style: &TextStyle, width: f32, height: f32) -> Result<()> {
    ensure!(
        style.outline_width.is_finite()
            && (0.0..=32.0).contains(&style.outline_width)
            && style.padding.is_finite()
            && (0.0..=256.0).contains(&style.padding)
            && style.line_spacing.is_finite()
            && (0.8..=3.0).contains(&style.line_spacing)
            && style.max_width.is_finite()
            && (40.0..=16384.0).contains(&style.max_width),
        "Invalid bubble spacing or outline"
    );
    ensure!(
        style.extra_tails.len() < MAX_BUBBLE_POINTERS,
        "A bubble has too many pointers"
    );
    for tail in style.tails() {
        ensure!(
            tail.tip
                .iter()
                .zip([width, height])
                .all(|(n, dimension)| n.is_finite() && (n * dimension).abs() <= MAX_POINTER_OFFSET)
                && tail.width.is_finite()
                && (1.0..=512.0).contains(&tail.width)
                && tail.bend.is_finite()
                && (-1.0..=1.0).contains(&tail.bend),
            "Invalid bubble pointer"
        );
    }
    Ok(())
}

fn finite(value: f32) -> bool {
    value.is_finite() && value.abs() <= 10_000_000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Asset, AssetKind, Layer};
    use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};

    fn test_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "scroll-studio-{label}-{}-{}.scro",
            std::process::id(),
            TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn image_story() -> (Story, AssetBytes) {
        let mut story = Story::new("A portable story");
        let asset_id = story.alloc_id();
        story.assets.push(Asset {
            id: asset_id,
            name: "Mira".into(),
            kind: AssetKind::Character,
            notes: "Same coat in every chapter".into(),
            prompt: "Reference".into(),
            width: 2,
            height: 2,
            character: Default::default(),
        });
        let id = story.alloc_id();
        story.chapters[0].layers.push(Layer {
            id,
            name: "Portrait".into(),
            visible: true,
            locked: false,
            opacity: 0.8,
            x: 10.0,
            y: 20.0,
            width: 200.0,
            height: 200.0,
            kind: LayerKind::Image {
                source_rect: None,
                clip: None,
                asset_id,
                mask: vec![[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]],
            },
        });
        let mut buffer = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(RgbaImage::from_pixel(2, 2, Rgba([1, 2, 3, 255])))
            .write_to(&mut buffer, ImageFormat::Png)
            .unwrap();
        (
            story,
            HashMap::from([(asset_id, Arc::new(buffer.into_inner()))]),
        )
    }

    use std::collections::HashMap;

    #[test]
    fn round_trip_contains_all_story_data_and_original_image_bytes() {
        let path = test_path("round-trip");
        let (story, assets) = image_story();
        save_story(&path, &story, &assets).unwrap();
        let (loaded, loaded_assets) = load_story(&path).unwrap();
        assert_eq!(loaded, story);
        assert_eq!(loaded_assets, assets);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn editing_world_data_preserves_original_chapters_and_embedded_images() {
        use crate::model::LoreEntry;
        let path = test_path("world-round-trip");
        let (original, assets) = image_story();
        save_story(&path, &original, &assets).unwrap();
        let (mut world, images, stamp) = load_story_with_stamp(&path).unwrap();
        let location = crate::locations::add_location(&mut world, "Old bridge").unwrap();
        let id = world.alloc_id();
        world.lore.push(LoreEntry {
            id,
            title: "First crossing".into(),
            text: "Mira remembers the flood.".into(),
            links: vec![world.assets[0].id, location],
        });
        let updated = save_story_checked(&path, &world, &images, Some(stamp)).unwrap();
        let (loaded, raw, actual) = load_story_with_stamp(&path).unwrap();
        assert_eq!(updated, actual);
        assert_eq!(loaded, world);
        assert_eq!(loaded.chapters, original.chapters);
        assert_eq!(raw, assets);
        assert!(save_story_checked(&path, &original, &assets, Some(stamp)).is_err());
        assert_eq!(load_story(&path).unwrap().0, world);
        world.lore[0].links.push(999_999);
        assert!(validate_story(&world, &images).is_err());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn painting_png_replaces_atomically_and_preserves_alpha() {
        let path = test_path("painting").with_extension("png");
        let mut source = RgbaImage::from_pixel(4, 4, Rgba([30, 90, 160, 0]));
        save_painting_png(&path, &source).unwrap();
        source[(2, 2)] = Rgba([200, 40, 10, 78]);
        save_painting_png(&path, &source).unwrap();
        assert_eq!(image::open(&path).unwrap().into_rgba8(), source);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn background_locations_round_trip_with_images_and_unsorted_assets() {
        let path = test_path("background-locations");
        let (mut story, mut assets) = image_story();
        story.assets[0].kind = AssetKind::Background;
        story.assets[0].name = "Station day".into();
        let station_image = story.assets[0].id;
        for name in ["Cafe interior", "Unsorted city view"] {
            let mut image = story.assets[0].clone();
            image.id = story.alloc_id();
            image.name = name.into();
            assets.insert(image.id, Arc::clone(&assets[&station_image]));
            story.assets.push(image);
        }
        let cafe_image = story.assets[1].id;
        let unsorted_image = story.assets[2].id;
        let station = crate::locations::add_location(&mut story, "Station").unwrap();
        let cafe = crate::locations::add_location(&mut story, "Cafe").unwrap();
        crate::locations::move_asset(&mut story, station_image, Some(station)).unwrap();
        crate::locations::move_asset(&mut story, cafe_image, Some(cafe)).unwrap();
        save_story(&path, &story, &assets).unwrap();
        let (loaded, loaded_assets) = load_story(&path).unwrap();
        assert_eq!(loaded, story);
        assert_eq!(loaded_assets, assets);
        assert_eq!(
            crate::locations::location_for_asset(&loaded, unsorted_image),
            None
        );
        assert_eq!(
            ZipArchive::new(File::open(&path).unwrap()).unwrap().len(),
            assets.len() + 1
        );
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn legacy_notes_become_connected_cards_and_board_saves_keep_source_images() {
        use crate::model::LoreEntry;
        let path = test_path("legacy-lore-board");
        let (mut original, images) = image_story();
        let note_id = original.alloc_id();
        original.lore.push(LoreEntry {
            id: note_id,
            title: "A clue".into(),
            text: "Keep the original text.".into(),
            links: vec![original.assets[0].id],
        });
        let mut manifest = serde_json::to_value(&original).unwrap();
        manifest["version"] = 10.into();
        manifest.as_object_mut().unwrap().remove("lore_board");
        let mut zip = ZipWriter::new(File::create(&path).unwrap());
        zip.start_file("manifest.json", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&serde_json::to_vec(&manifest).unwrap())
            .unwrap();
        for (id, bytes) in &images {
            zip.start_file(format!("assets/{id}.bin"), SimpleFileOptions::default())
                .unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
        let (mut loaded, raw) = load_story(&path).unwrap();
        assert_eq!(loaded.lore, original.lore);
        assert_eq!(loaded.chapters, original.chapters);
        assert_eq!(raw, images);
        assert_eq!(loaded.lore_board.cards.len(), 2);
        assert_eq!(loaded.lore_board.connections.len(), 1);
        loaded.lore_board.cards[0].position = [-600.5, 830.0];
        loaded.lore_board.connections[0].label = "Siblings".into();
        save_story(&path, &loaded, &raw).unwrap();
        assert_eq!(load_story(&path).unwrap(), (loaded.clone(), raw));
        let target = loaded.lore_board.cards[0].target;
        crate::lore::unlink_target(&mut loaded, target);
        assert!(loaded.lore_board.connections.is_empty());
        assert_eq!(loaded.lore_board.cards.len(), 1);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn version_five_without_locations_preserves_backgrounds_as_unsorted() {
        let path = test_path("legacy-background-locations");
        let original = Story::demo();
        let mut json = serde_json::to_value(&original).unwrap();
        json["version"] = 5.into();
        json.as_object_mut().unwrap().remove("locations");
        let mut zip = ZipWriter::new(File::create(&path).unwrap());
        zip.start_file("manifest.json", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&serde_json::to_vec(&json).unwrap()).unwrap();
        zip.finish().unwrap();

        let (loaded, assets) = load_story(&path).unwrap();
        assert_eq!(loaded, original);
        assert_eq!(loaded.version, FORMAT_VERSION);
        assert!(loaded.locations.is_empty());
        for background in loaded
            .assets
            .iter()
            .filter(|asset| asset.kind == AssetKind::Background)
        {
            assert_eq!(
                crate::locations::location_for_asset(&loaded, background.id),
                None
            );
        }
        save_story(&path, &loaded, &assets).unwrap();
        let mut archive = ZipArchive::new(File::open(&path).unwrap()).unwrap();
        let manifest: serde_json::Value = serde_json::from_slice(
            &read_entry(&mut archive, "manifest.json", MAX_MANIFEST_BYTES).unwrap(),
        )
        .unwrap();
        assert_eq!(manifest["version"], FORMAT_VERSION);
        drop(archive);
        assert_eq!(load_story(&path).unwrap().0, original);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn every_pre_location_version_loads_without_inventing_gender_or_folders() {
        for version in 1..=5 {
            let path = test_path(&format!("legacy-version-{version}"));
            let expected = Story::demo();
            let mut json = serde_json::to_value(&expected).unwrap();
            json["version"] = version.into();
            json.as_object_mut().unwrap().remove("locations");
            for asset in json["assets"].as_array_mut().unwrap() {
                asset["character"].as_object_mut().unwrap().remove("gender");
            }
            let mut zip = ZipWriter::new(File::create(&path).unwrap());
            zip.start_file("manifest.json", SimpleFileOptions::default())
                .unwrap();
            zip.write_all(&serde_json::to_vec(&json).unwrap()).unwrap();
            zip.finish().unwrap();
            let (loaded, _) = load_story(&path).unwrap();
            assert_eq!(loaded, expected, "Version {version} changed story content");
            assert_eq!(loaded.version, FORMAT_VERSION);
            assert!(loaded.locations.is_empty());
            assert!(loaded
                .assets
                .iter()
                .all(|asset| asset.character.gender.is_none()));
            fs::remove_file(path).unwrap();
        }
    }

    #[test]
    fn invalid_background_locations_cannot_replace_an_existing_save() {
        let path = test_path("invalid-background-locations");
        let (mut story, assets) = image_story();
        story.assets[0].kind = AssetKind::Background;
        let image = story.assets[0].id;
        let location = crate::locations::add_location(&mut story, "Station").unwrap();
        crate::locations::move_asset(&mut story, image, Some(location)).unwrap();
        save_story(&path, &story, &assets).unwrap();
        let bytes = fs::read(&path).unwrap();
        let mut invalid = Vec::new();
        let mut bad = story.clone();
        bad.locations[0].name = "   ".into();
        invalid.push(bad);
        let mut bad = story.clone();
        bad.locations[0].name = "x".repeat(crate::locations::MAX_LOCATION_NAME_BYTES + 1);
        invalid.push(bad);
        let mut bad = story.clone();
        bad.locations[0].asset_ids.push(image);
        invalid.push(bad);
        let mut bad = story.clone();
        bad.locations[0].asset_ids.push(999_999);
        invalid.push(bad);
        let mut bad = story.clone();
        bad.assets[0].kind = AssetKind::Image;
        invalid.push(bad);
        let mut bad = story.clone();
        bad.locations[0].id = image;
        invalid.push(bad);
        let mut bad = story.clone();
        bad.locations[0].id = bad.chapters[0].id;
        invalid.push(bad);
        let mut bad = story.clone();
        bad.locations.push(bad.locations[0].clone());
        invalid.push(bad);
        let mut bad = story.clone();
        let second = crate::locations::add_location(&mut bad, "City").unwrap();
        bad.locations
            .iter_mut()
            .find(|folder| folder.id == second)
            .unwrap()
            .asset_ids
            .push(image);
        invalid.push(bad);
        let mut bad = story.clone();
        bad.next_id = location;
        invalid.push(bad);
        let mut bad = story.clone();
        bad.locations[0].asset_ids = vec![image; crate::locations::MAX_LOCATION_ASSETS + 1];
        invalid.push(bad);
        for bad in invalid {
            assert!(save_story(&path, &bad, &assets).is_err());
            assert_eq!(fs::read(&path).unwrap(), bytes);
        }
        assert_eq!(load_story(&path).unwrap().0, story);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn rich_lettering_round_trips_and_invalid_pointer_cannot_replace_save() {
        use crate::model::{BubbleShape, FontChoice, TextStyle};
        let path = test_path("lettering");
        let mut story = Story::demo();
        let layer = story.chapters[0]
            .layers
            .iter_mut()
            .find(|l| matches!(l.kind, LayerKind::Text { .. }))
            .unwrap();
        if let LayerKind::Text { style, .. } = &mut layer.kind {
            *style = TextStyle::dialogue();
            style.font = FontChoice::DeliusUnicase;
            style.shape = BubbleShape::Thought;
            style.tail.tip = [-0.8, 2.1];
            style.tail.bend = -0.5;
            style.outline_width = 5.0;
            style.fill = [230, 240, 250, 180];
        }
        crate::bubbles::migrate_story_presets(&mut story).unwrap();
        save_story(&path, &story, &AssetBytes::new()).unwrap();
        assert_eq!(load_story(&path).unwrap().0, story);
        let before = fs::read(&path).unwrap();
        let layer = story.chapters[0]
            .layers
            .iter_mut()
            .find(|l| matches!(l.kind, LayerKind::Text { .. }))
            .unwrap();
        if let LayerKind::Text { style, .. } = &mut layer.kind {
            style.tail.tip[0] = f32::INFINITY;
        }
        assert!(save_story(&path, &story, &AssetBytes::new()).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn version_one_without_lettering_settings_opens_with_legacy_appearance() {
        use crate::model::TextStyle;
        let path = test_path("legacy-lettering");
        let mut json = serde_json::to_value(Story::demo()).unwrap();
        json["version"] = 1.into();
        for layer in json["chapters"][0]["layers"].as_array_mut().unwrap() {
            if let Some(text) = layer["kind"].get_mut("Text") {
                text.as_object_mut().unwrap().remove("style");
            }
        }
        let mut zip = ZipWriter::new(File::create(&path).unwrap());
        zip.start_file("manifest.json", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&serde_json::to_vec(&json).unwrap()).unwrap();
        zip.finish().unwrap();
        let (story, assets) = load_story(&path).unwrap();
        assert_eq!(story.version, FORMAT_VERSION);
        assert!(story.chapters[0]
            .layers
            .iter()
            .filter_map(|layer| {
                if let LayerKind::Text { style, .. } = &layer.kind {
                    Some(style)
                } else {
                    None
                }
            })
            .all(|style| {
                assert!(style.preset_id.is_some());
                let mut appearance = style.clone();
                appearance.preset_id = None;
                appearance == TextStyle::default()
            }));
        save_story(&path, &story, &assets).unwrap();
        assert_eq!(load_story(&path).unwrap().0, story);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn metadata_only_character_and_background_sheets_are_portable() {
        let path = test_path("demo");
        let story = Story::demo();
        save_story(&path, &story, &AssetBytes::new()).unwrap();
        assert_eq!(load_story(&path).unwrap().0, story);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn version_four_character_notes_and_prompts_survive_new_profile_defaults() {
        let path = test_path("v4-characters");
        let mut original = Story::demo();
        // Version four predates story-wide visual style.
        original.visual_style.clear();
        let mut json = serde_json::to_value(&original).unwrap();
        json["version"] = 4.into();
        json.as_object_mut().unwrap().remove("visual_style");
        for asset in json["assets"].as_array_mut().unwrap() {
            asset.as_object_mut().unwrap().remove("character");
        }
        let mut zip = ZipWriter::new(File::create(&path).unwrap());
        zip.start_file("manifest.json", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&serde_json::to_vec(&json).unwrap()).unwrap();
        zip.finish().unwrap();
        let (loaded, _) = load_story(&path).unwrap();
        assert_eq!(loaded, original);
        assert!(loaded.visual_style.is_empty());
        assert!(loaded
            .assets
            .iter()
            .all(|asset| asset.character == Default::default()));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn comprehensive_character_and_reference_images_round_trip_and_validate() {
        use crate::model::{CharacterGender, CharacterOutfit, CharacterReference, CharacterSwatch};
        let path = test_path("character-sheet");
        let (mut story, mut assets) = image_story();
        story.visual_style = "Soft watercolor shadows and crisp ink outlines.".into();
        let reference_id = story.alloc_id();
        let mut reference_image = story.assets[0].clone();
        reference_image.id = reference_id;
        reference_image.kind = AssetKind::Image;
        reference_image.name = "Mira side view".into();
        assets.insert(reference_id, assets[&story.assets[0].id].clone());
        story.assets.push(reference_image);
        let profile = &mut story.assets[0].character;
        profile.tagline = "The last commuter".into();
        profile.hair = "Short dark bob".into();
        profile.face = "Angular chin".into();
        profile.eyes = "Brown, almond shaped".into();
        profile.skin = "Warm brown".into();
        profile.build = "Tall, narrow shoulders".into();
        profile.distinguishing_features = "Silver ear cuff on the left".into();
        profile.personality = "Quiet but decisive".into();
        profile.backstory = "Returned after many years".into();
        profile.relationships = "Works with Theo".into();
        profile.role = "Lead".into();
        profile.age = "Adult, 28".into();
        profile.pronouns = "she/her".into();
        profile.gender = Some(CharacterGender::Female);
        profile.consistency = "Keep the left ear cuff".into();
        profile.avoid = "Long hair".into();
        profile.outfits.push(CharacterOutfit {
            name: "Raincoat".into(),
            description: "Charcoal coat, tan bag".into(),
        });
        profile.active_outfit = Some(0);
        profile.palette.push(CharacterSwatch {
            name: "Coat".into(),
            color: [24, 32, 40, 255],
        });
        profile.references.push(CharacterReference {
            asset_id: reference_id,
            label: "Side view".into(),
            notes: "Use for the nose profile".into(),
        });
        save_story(&path, &story, &assets).unwrap();
        let (loaded, loaded_assets) = load_story(&path).unwrap();
        assert_eq!(loaded, story);
        assert_eq!(loaded.version, FORMAT_VERSION);
        assert_eq!(
            loaded.assets[0].character.gender,
            Some(CharacterGender::Female)
        );
        assert_eq!(loaded_assets, assets);
        let original_file = fs::read(&path).unwrap();
        for mutation in [
            |story: &mut Story| story.assets[0].character.references[0].asset_id = u64::MAX,
            |story: &mut Story| {
                let duplicate = story.assets[0].character.references[0].clone();
                story.assets[0].character.references.push(duplicate);
            },
            |story: &mut Story| story.assets[0].character.active_outfit = Some(7),
            |story: &mut Story| {
                story.assets[0].character.references[0].asset_id = story.assets[0].id
            },
            |story: &mut Story| story.assets[1].kind = AssetKind::Character,
        ] {
            let mut invalid = story.clone();
            mutation(&mut invalid);
            assert!(save_story(&path, &invalid, &assets).is_err());
            assert_eq!(fs::read(&path).unwrap(), original_file);
        }
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn version_two_keeps_its_font_and_geometry_without_enabling_auto_fit() {
        use crate::model::FontChoice;
        let path = test_path("v2-themes");
        let mut story = Story::demo();
        for layer in &mut story.chapters[0].layers {
            if let LayerKind::Text { style, .. } = &mut layer.kind {
                // A legacy fixture must not inherit the current demo's modern layout.
                *style = crate::model::TextStyle {
                    font: FontChoice::PatrickHand,
                    ..Default::default()
                };
            }
        }
        story.bubble_presets.clear();
        let mut expected = story.clone();
        crate::bubbles::migrate_story_presets(&mut expected).unwrap();
        let mut json = serde_json::to_value(story).unwrap();
        json["version"] = 2.into();
        json.as_object_mut().unwrap().remove("bubble_presets");
        for layer in json["chapters"][0]["layers"].as_array_mut().unwrap() {
            if let Some(style) = layer["kind"]
                .get_mut("Text")
                .and_then(|t| t.get_mut("style"))
            {
                for name in [
                    "auto_fit",
                    "max_width",
                    "uppercase",
                    "modern_layout",
                    "preset_id",
                    "extra_tails",
                ] {
                    style.as_object_mut().unwrap().remove(name);
                }
            }
        }
        let mut zip = ZipWriter::new(File::create(&path).unwrap());
        zip.start_file("manifest.json", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&serde_json::to_vec(&json).unwrap()).unwrap();
        zip.finish().unwrap();
        let (loaded, _) = load_story(&path).unwrap();
        assert_eq!(loaded, expected);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn new_theme_and_fitting_preferences_are_portable_and_validated() {
        use crate::model::{LetteringTheme, TextStyle};
        let path = test_path("theme-preferences");
        let mut story = Story::demo();
        for (layer, theme) in story.chapters[0]
            .layers
            .iter_mut()
            .filter(|l| matches!(l.kind, LayerKind::Text { .. }))
            .zip(LetteringTheme::ALL)
        {
            if let LayerKind::Text { style, .. } = &mut layer.kind {
                *style = TextStyle::for_theme(theme);
                style.max_width = 720.0;
            }
        }
        crate::bubbles::migrate_story_presets(&mut story).unwrap();
        save_story(&path, &story, &AssetBytes::new()).unwrap();
        assert_eq!(load_story(&path).unwrap().0, story);
        let original = fs::read(&path).unwrap();
        for layer in &mut story.chapters[0].layers {
            if let LayerKind::Text { style, .. } = &mut layer.kind {
                style.max_width = f32::NAN;
            }
        }
        assert!(save_story(&path, &story, &AssetBytes::new()).is_err());
        assert_eq!(original, fs::read(&path).unwrap());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn linked_multiple_negative_pointers_round_trip_and_invalid_links_are_rejected() {
        use crate::model::{BubbleTail, MAX_BUBBLE_POINTERS};
        let path = test_path("shared-multi-pointer");
        let mut story = Story::demo();
        let layer = story.chapters[0]
            .layers
            .iter_mut()
            .find(|layer| matches!(layer.kind, LayerKind::Text { bubble: true, .. }))
            .unwrap();
        if let LayerKind::Text { style, .. } = &mut layer.kind {
            style.tail.enabled = true;
            style.tail.tip = [-1.5, -2.0];
            style.extra_tails = vec![BubbleTail {
                enabled: true,
                tip: [1.4, -0.8],
                ..Default::default()
            }];
        }
        save_story(&path, &story, &AssetBytes::new()).unwrap();
        assert_eq!(load_story(&path).unwrap().0, story);
        let original_bytes = fs::read(&path).unwrap();
        let mut bad = story.clone();
        let LayerKind::Text { style, .. } = &mut bad.chapters[0]
            .layers
            .iter_mut()
            .find(|layer| matches!(layer.kind, LayerKind::Text { .. }))
            .unwrap()
            .kind
        else {
            panic!();
        };
        style.preset_id = Some(u64::MAX);
        assert!(save_story(&path, &bad, &AssetBytes::new()).is_err());
        let mut bad = story.clone();
        let LayerKind::Text { style, .. } = &mut bad.chapters[0]
            .layers
            .iter_mut()
            .find(|layer| matches!(layer.kind, LayerKind::Text { .. }))
            .unwrap()
            .kind
        else {
            panic!();
        };
        style.extra_tails = vec![BubbleTail::default(); MAX_BUBBLE_POINTERS];
        assert!(save_story(&path, &bad, &AssetBytes::new()).is_err());
        let mut bad = story;
        bad.bubble_presets[0].style.max_width = f32::NAN;
        assert!(save_story(&path, &bad, &AssetBytes::new()).is_err());
        assert_eq!(fs::read(&path).unwrap(), original_bytes);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn old_archive_migration_rejects_bad_id_counter_without_panicking() {
        let path = test_path("bad-migration-counter");
        let mut json = serde_json::to_value(Story::demo()).unwrap();
        json["version"] = 3.into();
        json["next_id"] = u64::MAX.into();
        json.as_object_mut().unwrap().remove("bubble_presets");
        for layer in json["chapters"][0]["layers"].as_array_mut().unwrap() {
            if let Some(style) = layer["kind"]
                .get_mut("Text")
                .and_then(|text| text.get_mut("style"))
            {
                style.as_object_mut().unwrap().remove("preset_id");
                style.as_object_mut().unwrap().remove("extra_tails");
            }
        }
        let mut zip = ZipWriter::new(File::create(&path).unwrap());
        zip.start_file("manifest.json", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&serde_json::to_vec(&json).unwrap()).unwrap();
        zip.finish().unwrap();
        assert!(load_story(&path).is_err());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn empty_legacy_story_receives_three_ready_to_use_presets() {
        use crate::model::LetteringTheme;
        let path = test_path("empty-legacy-presets");
        let original = Story::new("No dialogue yet");
        let mut json = serde_json::to_value(&original).unwrap();
        json["version"] = 3.into();
        json.as_object_mut().unwrap().remove("bubble_presets");
        let mut zip = ZipWriter::new(File::create(&path).unwrap());
        zip.start_file("manifest.json", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&serde_json::to_vec(&json).unwrap()).unwrap();
        zip.finish().unwrap();
        let (migrated, assets) = load_story(&path).unwrap();
        assert_eq!(migrated.chapters, original.chapters);
        assert_eq!(migrated.bubble_presets.len(), 3);
        assert_eq!(
            migrated.bubble_presets[0].style.theme(),
            Some(LetteringTheme::WebtoonClassic)
        );
        assert!(migrated
            .bubble_presets
            .iter()
            .all(|preset| preset.id >= original.next_id));
        assert_eq!(migrated.next_id, original.next_id + 3);
        save_story(&path, &migrated, &assets).unwrap();
        assert_eq!(load_story(&path).unwrap().0, migrated);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn failed_validation_preserves_previous_save() {
        let path = test_path("preserve");
        let (mut story, assets) = image_story();
        save_story(&path, &story, &assets).unwrap();
        let before = fs::read(&path).unwrap();
        story.chapters[0].layers[0].opacity = f32::NAN;
        assert!(save_story(&path, &story, &assets).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn saving_over_an_existing_story_replaces_it() {
        let path = test_path("overwrite");
        let (mut story, assets) = image_story();
        save_story(&path, &story, &assets).unwrap();
        story.title = "Revised".into();
        save_story(&path, &story, &assets).unwrap();
        assert_eq!(load_story(&path).unwrap().0.title, "Revised");
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn rejects_missing_asset_future_version_duplicate_ids_and_bad_dimensions() {
        let (story, assets) = image_story();
        assert!(validate_story(&story, &AssetBytes::new()).is_err());
        let mut bad = story.clone();
        bad.version += 1;
        assert!(validate_story(&bad, &assets).is_err());
        let mut bad = story.clone();
        bad.assets[0].id = bad.chapters[0].id;
        assert!(validate_story(&bad, &assets).is_err());
        let mut bad = story.clone();
        bad.chapters.clear();
        assert!(validate_story(&bad, &assets).is_err());
        let mut bad = story;
        bad.chapters[0].height = u32::MAX;
        assert!(validate_story(&bad, &assets).is_err());
    }

    #[test]
    fn rejects_corrupt_archives_and_path_traversal_entries() {
        let path = test_path("bad-archive");
        fs::write(&path, b"not a zip").unwrap();
        assert!(load_story(&path).is_err());
        let mut zip = ZipWriter::new(File::create(&path).unwrap());
        zip.start_file("../outside", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"never extracted").unwrap();
        zip.finish().unwrap();
        assert!(load_story(&path).is_err());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn rejects_an_oversized_compressed_manifest_without_expanding_it() {
        let path = test_path("zip-bomb");
        let mut zip = ZipWriter::new(File::create(&path).unwrap());
        zip.start_file(
            "manifest.json",
            SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
        )
        .unwrap();
        let chunk = [b' '; 8192];
        for _ in 0..(MAX_MANIFEST_BYTES / chunk.len() as u64 + 1) {
            zip.write_all(&chunk).unwrap();
        }
        zip.finish().unwrap();
        assert!(load_story(&path)
            .unwrap_err()
            .to_string()
            .contains("too large"));
        fs::remove_file(path).unwrap();
    }
}
