//! Portable character direction and reference sheets. No provider or network required.
use crate::{compositor, model::*};
use ab_glyph::{point, Font, FontArc, ScaleFont};
use anyhow::{ensure, Context, Result};
use image::{Rgba, RgbaImage};
use std::{collections::HashSet, sync::OnceLock};

/// Assemble a fresh prompt so a story style edit is reflected at the next copy.
pub fn with_story_style(visual_style: &str, prompt: &str) -> String {
    let mut sections = Vec::new();
    section(&mut sections, "STORY VISUAL STYLE", visual_style);
    if !prompt.trim().is_empty() {
        sections.push(prompt.trim().to_owned());
    }
    sections.join("\n\n")
}

fn section(sections: &mut Vec<String>, heading: &str, value: &str) {
    if !value.trim().is_empty() {
        sections.push(format!("{heading}\n{}", value.trim()));
    }
}

/// Build instructions from the details the creator actually supplied. Story-only
/// backstory and relationships remain notes rather than accidental visual instructions.
pub fn compose_prompt(
    story: &Story,
    character: &Asset,
    kind: CharacterPromptKind,
    scene: &str,
) -> String {
    let profile = &character.character;
    let mut sections = Vec::new();
    let mut identity = Vec::new();
    for (label, value) in [
        ("Name", &character.name),
        ("Concept", &profile.tagline),
        ("Role", &profile.role),
        ("Age", &profile.age),
        ("Personality / body language", &profile.personality),
    ] {
        if !value.trim().is_empty() {
            identity.push(format!("{label}: {}", value.trim()));
        }
    }
    if let Some(gender) = profile.gender {
        identity.push(format!("Gender: {}", gender.label()));
    }
    section(&mut sections, "CHARACTER", &identity.join("\n"));
    let mut appearance = Vec::new();
    for (label, value) in [
        ("Face", &profile.face),
        ("Hair", &profile.hair),
        ("Eyes", &profile.eyes),
        ("Skin", &profile.skin),
        ("Build / silhouette", &profile.build),
        ("Distinguishing features", &profile.distinguishing_features),
    ] {
        if !value.trim().is_empty() {
            appearance.push(format!("{label}: {}", value.trim()));
        }
    }
    section(&mut sections, "APPEARANCE", &appearance.join("\n"));
    if let Some(outfit) = profile
        .active_outfit
        .and_then(|index| profile.outfits.get(index))
    {
        let description = match (outfit.name.trim(), outfit.description.trim()) {
            ("", description) => description.to_owned(),
            (name, "") => name.to_owned(),
            (name, description) => format!("{name}: {description}"),
        };
        section(&mut sections, "SELECTED OUTFIT", &description);
    }
    let palette = profile
        .palette
        .iter()
        .map(|swatch| {
            let [r, g, b, _] = swatch.color;
            let color = format!("#{r:02X}{g:02X}{b:02X}");
            if swatch.name.trim().is_empty() {
                color
            } else {
                format!("{}: {color}", swatch.name.trim())
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    section(&mut sections, "COLOR PALETTE", &palette);
    section(&mut sections, "CONTINUITY NOTES", &character.notes);
    section(&mut sections, "KEEP CONSISTENT", &profile.consistency);
    section(&mut sections, "ADDITIONAL DIRECTION", &character.prompt);
    section(&mut sections, "AVOID", &profile.avoid);
    let request = match kind {
        CharacterPromptKind::Portrait => "Create a clear chest-up character portrait with a readable face and silhouette, a neutral background, and no lettering. Preserve the specified identity, proportions, palette, and outfit.",
        CharacterPromptKind::Turnaround => "Create a character turnaround sheet: full-body front, side, and back views of the SAME character at the same scale, with consistent proportions and the selected outfit. Use neutral poses and a plain background. No lettering or speech bubbles.",
        CharacterPromptKind::Expressions => "Create an expression reference sheet of the SAME character: neutral, happy, angry, sad, surprised, and determined. Keep the face, hairstyle, age, and palette consistent across all views. Plain background; no lettering.",
        CharacterPromptKind::Scene => "Create a webtoon illustration of this character in the scene below. Preserve the character's identity, proportions, palette, and selected outfit. Leave dialogue and speech bubbles for lettering in Scroll Studio.",
    };
    section(&mut sections, "IMAGE REQUEST", request);
    if kind == CharacterPromptKind::Scene {
        section(&mut sections, "SCENE / POSE", scene);
    }
    if !reference_ids(character).is_empty() {
        sections.push("REFERENCE IMAGES\nUse the character reference images I attach as visual anchors. Match the same character rather than redesigning them.".into());
        let reference_notes = profile
            .references
            .iter()
            .filter(|reference| !reference.notes.trim().is_empty())
            .map(|reference| {
                format!(
                    "{}: {}",
                    if reference.label.trim().is_empty() {
                        "Reference"
                    } else {
                        reference.label.trim()
                    },
                    reference.notes.trim()
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        if !reference_notes.is_empty() {
            sections.push(reference_notes);
        }
    }
    with_story_style(&story.visual_style, &sections.join("\n\n"))
}

/// Stable image identities; a character's original image remains first.
pub fn reference_ids(character: &Asset) -> Vec<u64> {
    let mut seen = HashSet::new();
    let mut ids = Vec::new();
    if character.width > 0 && character.height > 0 {
        seen.insert(character.id);
        ids.push(character.id);
    }
    for reference in &character.character.references {
        if seen.insert(reference.asset_id) {
            ids.push(reference.asset_id);
        }
    }
    ids
}

/// A bounded contact sheet suitable for pasting or uploading into an image generator.
/// Only one full source image is decoded at a time; originals remain untouched.
pub fn reference_sheet(story: &Story, assets: &AssetBytes, character: &Asset) -> Result<RgbaImage> {
    ensure!(
        character.kind == AssetKind::Character,
        "Select a character to create a reference sheet"
    );
    let ids = reference_ids(character);
    ensure!(
        !ids.is_empty(),
        "Add a reference image to this character first"
    );
    ensure!(
        ids.len() <= 25,
        "A reference sheet supports an original image and up to 24 additional references"
    );
    let columns = (ids.len() as f32).sqrt().ceil() as u32;
    let rows = (ids.len() as u32).div_ceil(columns);
    let width: u32 = if columns == 1 { 1024 } else { 2048 };
    let margin = 24;
    let gap = 16;
    let header = 100;
    let footer = 36;
    let cell_width = (width - margin * 2 - gap * (columns - 1)) / columns;
    let cell_height = cell_width.min((2048 - header - footer - gap * (rows - 1)) / rows);
    let height = header + cell_height * rows + gap * (rows - 1) + footer;
    let mut sheet = RgbaImage::from_pixel(width, height, Rgba([244, 246, 245, 255]));
    draw_label(
        &mut sheet,
        &character.name,
        margin,
        18,
        38.0,
        width - margin * 2,
    );
    draw_label(
        &mut sheet,
        "Character references · Scroll Studio",
        margin,
        63,
        20.0,
        width - margin * 2,
    );
    for (index, id) in ids.into_iter().enumerate() {
        let asset = story
            .assets
            .iter()
            .find(|asset| asset.id == id)
            .with_context(|| format!("Reference image {id} is missing from this story"))?;
        let bytes = assets
            .get(&id)
            .with_context(|| format!("Reference image is missing: {}", asset.name))?;
        let source = compositor::decode_image(bytes)
            .with_context(|| format!("Could not read reference: {}", asset.name))?;
        let x = margin + index as u32 % columns * (cell_width + gap);
        let y = header + index as u32 / columns * (cell_height + gap);
        for pixel_y in y..y + cell_height {
            for pixel_x in x..x + cell_width {
                sheet.put_pixel(pixel_x, pixel_y, Rgba([255; 4]));
            }
        }
        let image_width = cell_width - 24;
        let image_height = cell_height - 58;
        let thumbnail = image::imageops::thumbnail(&source, image_width, image_height);
        image::imageops::overlay(
            &mut sheet,
            &thumbnail,
            i64::from(x + (cell_width - thumbnail.width()) / 2),
            i64::from(y + 12 + (image_height - thumbnail.height()) / 2),
        );
        let label = if id == character.id {
            "Original reference"
        } else {
            character
                .character
                .references
                .iter()
                .find(|reference| reference.asset_id == id)
                .map(|reference| reference.label.trim())
                .filter(|label| !label.is_empty())
                .unwrap_or(&asset.name)
        };
        draw_label(
            &mut sheet,
            label,
            x + 12,
            y + cell_height - 36,
            22.0,
            cell_width - 24,
        );
    }
    Ok(sheet)
}

fn draw_label(image: &mut RgbaImage, value: &str, x: u32, y: u32, size: f32, maximum: u32) {
    static FONT: OnceLock<FontArc> = OnceLock::new();
    let font =
        FONT.get_or_init(|| FontArc::try_from_slice(compositor::FONT_BYTES).expect("bundled font"));
    let scaled = font.as_scaled(size);
    let mut cursor = x as f32;
    let mut previous = None;
    for character in value.chars().take(512) {
        let character = if character.is_control() {
            ' '
        } else {
            character
        };
        let glyph_id = font.glyph_id(character);
        if let Some(previous) = previous {
            cursor += scaled.kern(previous, glyph_id);
        }
        let advance = scaled.h_advance(glyph_id);
        if cursor + advance > (x + maximum) as f32 {
            break;
        }
        let glyph =
            glyph_id.with_scale_and_position(size, point(cursor, y as f32 + scaled.ascent()));
        if let Some(outline) = font.outline_glyph(glyph) {
            let bounds = outline.px_bounds();
            outline.draw(|pixel_x, pixel_y, coverage| {
                let px = bounds.min.x as i32 + pixel_x as i32;
                let py = bounds.min.y as i32 + pixel_y as i32;
                if px >= 0 && py >= 0 && px < image.width() as i32 && py < image.height() as i32 {
                    let pixel = image.get_pixel_mut(px as u32, py as u32);
                    for (channel, ink) in pixel.0[..3].iter_mut().zip([33.0, 45.0, 43.0]) {
                        *channel =
                            (*channel as f32 * (1.0 - coverage) + ink * coverage).round() as u8;
                    }
                }
            });
        }
        cursor += advance;
        previous = Some(glyph_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Cursor, sync::Arc};

    fn character() -> Asset {
        Asset {
            id: 9,
            name: "Mira".into(),
            kind: AssetKind::Character,
            notes: String::new(),
            prompt: String::new(),
            width: 0,
            height: 0,
            character: CharacterProfile::default(),
        }
    }

    #[test]
    fn prompt_includes_live_style_selected_outfit_and_visual_identity() {
        let mut story = Story::new("After rain");
        story.visual_style = "Muted teal, warm amber highlights.".into();
        let mut character = character();
        character.character.hair = "Short black bob".into();
        character.character.outfits = vec![
            CharacterOutfit {
                name: "Uniform".into(),
                description: "Blue jacket".into(),
            },
            CharacterOutfit {
                name: "Rain".into(),
                description: "Charcoal coat".into(),
            },
        ];
        character.character.active_outfit = Some(1);
        character.character.palette.push(CharacterSwatch {
            name: "Coat".into(),
            color: [32, 48, 64, 255],
        });
        character.character.consistency = "Small scar over left eyebrow".into();
        character.character.avoid = "Long hair".into();
        character.character.backstory = "A plot secret must not enter a portrait prompt".into();
        let prompt = compose_prompt(
            &story,
            &character,
            CharacterPromptKind::Turnaround,
            "Ignored scene",
        );
        assert!(prompt.starts_with("STORY VISUAL STYLE\nMuted teal"));
        for expected in [
            "Hair: Short black bob",
            "Rain: Charcoal coat",
            "Coat: #203040",
            "Small scar",
            "AVOID\nLong hair",
            "front, side, and back",
        ] {
            assert!(prompt.contains(expected));
        }
        for excluded in ["Blue jacket", "plot secret", "Ignored scene"] {
            assert!(!prompt.contains(excluded));
        }
        story.visual_style = "Black and white ink.".into();
        assert!(
            compose_prompt(&story, &character, CharacterPromptKind::Portrait, "")
                .starts_with("STORY VISUAL STYLE\nBlack and white ink.")
        );
    }

    #[test]
    fn gender_prompt_requires_explicit_selection_and_omits_legacy_pronouns() {
        let story = Story::new("Character choices");
        let mut character = character();
        character.character = serde_json::from_value(serde_json::json!({
            "pronouns": "she / her"
        }))
        .unwrap();
        let prompt = compose_prompt(&story, &character, CharacterPromptKind::Portrait, "");
        assert!(!prompt.contains("Gender:"));
        assert!(!prompt.contains("Pronouns:"));
        assert!(!prompt.contains("she / her"));

        for gender in [CharacterGender::Female, CharacterGender::Male] {
            character.character.gender = Some(gender);
            let prompt = compose_prompt(&story, &character, CharacterPromptKind::Portrait, "");
            assert!(prompt.contains(&format!("Gender: {}", gender.label())));
            assert!(prompt.contains(&format!("Name: {}", character.name)));
            assert!(!prompt.contains("Pronouns:"));
            assert!(!prompt.contains("she / her"));
        }
        assert_eq!(character.character.pronouns, "she / her");
    }

    #[test]
    fn blank_prompt_fields_do_not_emit_empty_sections_or_fake_references() {
        let prompt = compose_prompt(
            &Story::new("Blank"),
            &character(),
            CharacterPromptKind::Scene,
            "Running through the station.",
        );
        for excluded in [
            "STORY VISUAL STYLE",
            "APPEARANCE",
            "SELECTED OUTFIT",
            "COLOR PALETTE",
            "REFERENCE IMAGES",
            "AVOID",
        ] {
            assert!(!prompt.contains(excluded));
        }
        assert!(prompt.contains("SCENE / POSE\nRunning through the station."));
        assert_eq!(with_story_style("  ", "  Scene prompt.  "), "Scene prompt.");
        assert_eq!(
            with_story_style("Soft shading.", ""),
            "STORY VISUAL STYLE\nSoft shading."
        );
        assert_eq!(with_story_style("", ""), "");
    }

    #[test]
    fn reference_sheet_contains_each_original_and_is_bounded() {
        let mut story = Story::new("References");
        let mut character = character();
        character.width = 40;
        character.height = 80;
        let mut bytes = Cursor::new(Vec::new());
        RgbaImage::from_pixel(40, 80, Rgba([220, 30, 20, 255]))
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        let encoded = Arc::new(bytes.into_inner());
        let mut assets = AssetBytes::new();
        assets.insert(character.id, encoded.clone());
        for id in 10..34 {
            character.character.references.push(CharacterReference {
                asset_id: id,
                label: format!("View {id}"),
                notes: String::new(),
            });
            assets.insert(id, encoded.clone());
            story.assets.push(Asset {
                id,
                name: format!("View {id}"),
                kind: AssetKind::Image,
                notes: String::new(),
                prompt: String::new(),
                width: 40,
                height: 80,
                character: Default::default(),
            });
        }
        story.assets.push(character.clone());
        assert_eq!(reference_ids(&character).len(), 25);
        let sheet = reference_sheet(&story, &assets, &character).unwrap();
        assert!(sheet.width() <= 2048 && sheet.height() <= 2048);
        assert!(
            sheet
                .pixels()
                .filter(|pixel| pixel.0 == [220, 30, 20, 255])
                .count()
                > 25 * 40 * 80
        );
        assert_eq!(assets[&9].as_slice(), encoded.as_slice());
        assets.remove(&10);
        assert!(reference_sheet(&story, &assets, &character)
            .unwrap_err()
            .to_string()
            .contains("Reference image is missing"));
    }

    #[test]
    fn reference_sheet_requires_an_image_and_ids_are_deduplicated() {
        let mut character = character();
        assert!(reference_sheet(&Story::new("Blank"), &AssetBytes::new(), &character).is_err());
        character.width = 10;
        character.height = 10;
        character.character.references = vec![
            CharacterReference {
                asset_id: 9,
                label: String::new(),
                notes: String::new(),
            },
            CharacterReference {
                asset_id: 10,
                label: String::new(),
                notes: String::new(),
            },
            CharacterReference {
                asset_id: 10,
                label: String::new(),
                notes: String::new(),
            },
        ];
        assert_eq!(reference_ids(&character), vec![9, 10]);
    }
}
