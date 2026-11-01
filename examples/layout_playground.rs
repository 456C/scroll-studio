//! An editable, image-backed story for trying magnetic alignment.
mod support;
use scroll_studio::model::*;
use std::{path::PathBuf, sync::Arc};

fn main() -> anyhow::Result<()> {
    let directory = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/sample-package"));
    std::fs::create_dir_all(&directory)?;
    let mut story = Story::with_canvas("Layout playground", 800, 1600);
    story.synopsis = "Move a blue panel near the canvas center, another panel's edge, or an equal gap. Hold Alt to move freely; S toggles snapping.".into();
    story.chapters[0].name = "01 · Center".into();
    story.chapters[0].width = 800;
    story.chapters[0].height = 1600;
    story.chapters[0].panels.clear();
    for name in ["02 · Align", "03 · Spacing"] {
        let id = story.alloc_id();
        story.chapters.push(Chapter::new(id, name, 800, 1600));
    }
    let mut assets = AssetBytes::new();
    let asset_id = story.alloc_id();
    let image = image::RgbaImage::from_pixel(240, 180, image::Rgba([122, 184, 245, 255]));
    let mut bytes = std::io::Cursor::new(Vec::new());
    image.write_to(&mut bytes, image::ImageFormat::Png)?;
    assets.insert(asset_id, Arc::new(bytes.into_inner()));
    story.assets.push(Asset {
        id: asset_id,
        name: "Blue panel".into(),
        kind: AssetKind::Image,
        notes: String::new(),
        prompt: String::new(),
        width: 240,
        height: 180,
        character: CharacterProfile::default(),
    });
    let id = story.alloc_id();
    story.chapters[0]
        .layers
        .push(image_layer(id, asset_id, 90.0, 180.0));
    for (chapter, x, y, width, height) in [
        (1, 90.0, 150.0, 240.0, 180.0),
        (1, 470.0, 500.0, 240.0, 180.0),
        (2, 80.0, 120.0, 240.0, 180.0),
        (2, 80.0, 420.0, 240.0, 180.0),
    ] {
        let id = story.alloc_id();
        story.chapters[chapter].layers.push(Layer {
            id,
            name: "Reference panel".into(),
            visible: true,
            locked: true,
            opacity: 1.0,
            x,
            y,
            width,
            height,
            kind: LayerKind::Shape {
                color: [188, 196, 207, 255],
            },
        });
    }
    for (chapter, x, y) in [(1, 370.0, 160.0), (2, 410.0, 700.0)] {
        let id = story.alloc_id();
        story.chapters[chapter]
            .layers
            .push(image_layer(id, asset_id, x, y));
    }
    support::save(&directory, "Layout playground", &story, &assets)
}

fn image_layer(id: u64, asset_id: u64, x: f32, y: f32) -> Layer {
    Layer {
        id,
        name: "Blue panel".into(),
        visible: true,
        locked: false,
        opacity: 1.0,
        x,
        y,
        width: 240.0,
        height: 180.0,
        kind: LayerKind::Image {
            source_rect: None,
            clip: None,
            asset_id,
            mask: Vec::new(),
        },
    }
}
