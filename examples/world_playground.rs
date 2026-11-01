//! Isolated world-building and chronology fixture, generated locally for UI review.
use scroll_studio::{groups, locations, model::*, storage};
use std::{io::Cursor, path::PathBuf, sync::Arc};
fn main() -> anyhow::Result<()> {
    let directory = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/world-review"));
    std::fs::create_dir_all(&directory)?;
    let mut story = Story::with_canvas("The quiet citadel", 800, 2400);
    story.chapters[0].name = "01 · The arrival".into();
    for name in ["02 · The signal", "03 · The promise"] {
        let id = story.alloc_id();
        story.chapters.push(Chapter::new(id, name, 800, 2400));
    }
    let character = story.alloc_id();
    story.assets.push(Asset {
        id: character,
        name: "Ari".into(),
        kind: AssetKind::Character,
        notes: String::new(),
        prompt: String::new(),
        width: 0,
        height: 0,
        character: CharacterProfile {
            tagline: "The returning messenger".into(),
            ..Default::default()
        },
    });
    let location = locations::add_location(&mut story, "The quiet citadel")?;
    let group_id = groups::add(&mut story)?;
    story.groups[0].name = "The Dawnkeepers".into();
    story.groups[0].description =
        "Keepers of the citadel. Teal cloaks, pale silver insignia, a quiet resolve.".into();
    groups::set_affiliation(
        &mut story.groups[0],
        groups::Subject::Character(character),
        true,
    );
    groups::set_affiliation(
        &mut story.groups[0],
        groups::Subject::Location(location),
        true,
    );
    let mut assets = AssetBytes::new();
    for (width, height, name) in [
        (480, 720, "Cloak silhouette"),
        (720, 420, "Citadel light"),
        (400, 400, "Insignia study"),
    ] {
        let id = story.alloc_id();
        let image = image::RgbaImage::from_fn(width, height, |x, y| {
            let horizon = y > height * 2 / 3;
            let tower = x > width / 3 && x < width * 2 / 3 && y > height / 4;
            let value = if horizon || tower {
                24
            } else {
                (90 - 55 * y / height) as u8
            };
            image::Rgba([value, value + 30, value + 35, 255])
        });
        let mut png = Cursor::new(Vec::new());
        image.write_to(&mut png, image::ImageFormat::Png)?;
        assets.insert(id, Arc::new(png.into_inner()));
        story.assets.push(Asset {
            id,
            name: name.into(),
            kind: AssetKind::Background,
            width,
            height,
            notes: String::new(),
            prompt: String::new(),
            character: Default::default(),
        });
        story.groups[0].reference_ids.push(id);
        locations::move_asset(&mut story, id, Some(location))?;
    }
    story.timeline = [
        (2, "Before the fall", "Ari makes a promise at the citadel."),
        (0, "Three years later", "The messenger returns before dawn."),
        (1, "The next morning", "A light appears in the tower."),
    ]
    .map(|(i, when, summary)| ChronologyEntry {
        chapter_id: story.chapters[i].id,
        when: when.into(),
        summary: summary.into(),
    })
    .to_vec();
    assert_eq!(story.groups[0].id, group_id);
    storage::save_story(&directory.join("World review.scro"), &story, &assets)?;
    storage::save_story(
        &directory.join("Empty review.scro"),
        &Story::new("Empty review"),
        &AssetBytes::new(),
    )?;
    Ok(())
}
