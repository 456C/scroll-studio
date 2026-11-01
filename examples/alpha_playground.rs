//! Reproducible transparency fixture and export proof. QA only, not packaged.
use scroll_studio::{compositor, model::*, storage};
use std::{io::Cursor, path::PathBuf, sync::Arc};

fn main() -> anyhow::Result<()> {
    let directory = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/alpha-review"));
    std::fs::create_dir_all(&directory)?;
    let mut story = Story::with_canvas("Transparency proof", 800, 1440);
    story.chapters[0].panels.clear();
    let mut assets = AssetBytes::new();
    add(
        &mut story,
        "Dark paper",
        400.0,
        0.0,
        400.0,
        1440.0,
        1.0,
        LayerKind::Shape {
            color: [25, 35, 45, 255],
        },
    );
    let solid = image::RgbaImage::from_pixel(640, 160, image::Rgba([70, 180, 240, 255]));
    let solid_id = asset(&mut story, &mut assets, "Solid cyan", solid)?;
    let ramp = image::RgbaImage::from_fn(1280, 320, |x, _| {
        image::Rgba([70, 180, 240, ((x % 640) * 255 / 639) as u8])
    });
    let ramp_id = asset(&mut story, &mut assets, "Alpha ramp", ramp)?;
    // Hidden red RGB must never bleed into the blue cutout while shrinking.
    let edge = image::RgbaImage::from_fn(2560, 640, |x, y| {
        let distance = (y as f32 - 320.0).abs() + ((x % 1280) as f32 - 640.0).abs() * 0.25;
        if distance < 180.0 {
            image::Rgba([70, 180, 240, 255])
        } else {
            image::Rgba([255, 0, 0, 0])
        }
    });
    let edge_id = asset(&mut story, &mut assets, "Transparent cutout", edge)?;
    add(
        &mut story,
        "Shape at 50%",
        80.0,
        100.0,
        640.0,
        160.0,
        0.5,
        LayerKind::Shape {
            color: [70, 180, 240, 255],
        },
    );
    add(
        &mut story,
        "Image at 50%",
        80.0,
        360.0,
        640.0,
        160.0,
        0.5,
        LayerKind::Image {
            source_rect: None,
            clip: None,
            asset_id: solid_id,
            mask: vec![],
        },
    );
    add(
        &mut story,
        "Alpha ramp at 75%",
        80.0,
        620.0,
        640.0,
        160.0,
        0.75,
        LayerKind::Image {
            source_rect: None,
            clip: None,
            asset_id: ramp_id,
            mask: vec![],
        },
    );
    add(
        &mut story,
        "Masked cutout at 60%",
        80.0,
        880.0,
        640.0,
        160.0,
        0.6,
        LayerKind::Image {
            source_rect: None,
            clip: None,
            asset_id: edge_id,
            mask: vec![[0.05, 0.1], [0.95, 0.2], [0.9, 0.9], [0.1, 0.8]],
        },
    );
    add(
        &mut story,
        "Stroke at 50%",
        80.0,
        1200.0,
        640.0,
        80.0,
        0.5,
        LayerKind::Stroke {
            points: vec![[0.0, 40.0], [640.0, 40.0]],
            color: [70, 180, 240, 255],
            size: 80.0,
        },
    );
    let path = directory.join("Transparency proof.scro");
    storage::validate_story(&story, &assets)?;
    storage::save_story(&path, &story, &assets)?;
    let (loaded, loaded_assets) = storage::load_story(&path)?;
    anyhow::ensure!(
        loaded == story && loaded_assets == assets,
        "Fixture changed on load"
    );
    for width in [360, 540, 800] {
        let height = (1440.0 * width as f32 / 800.0).ceil() as u32;
        compositor::render_region(&story.chapters[0], &assets, 0, height, width)?
            .save(directory.join(format!("Transparency proof-{width}.png")))?;
    }
    println!("{}", path.display());
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn add(
    story: &mut Story,
    name: &str,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    opacity: f32,
    kind: LayerKind,
) {
    let id = story.alloc_id();
    story.chapters[0].layers.push(Layer {
        id,
        name: name.into(),
        visible: true,
        locked: name == "Dark paper",
        opacity,
        x,
        y,
        width,
        height,
        kind,
    });
}

fn asset(
    story: &mut Story,
    assets: &mut AssetBytes,
    name: &str,
    image: image::RgbaImage,
) -> anyhow::Result<u64> {
    let id = story.alloc_id();
    let mut bytes = Cursor::new(Vec::new());
    image.write_to(&mut bytes, image::ImageFormat::Png)?;
    assets.insert(id, Arc::new(bytes.into_inner()));
    story.assets.push(Asset {
        id,
        name: name.into(),
        kind: AssetKind::Image,
        notes: String::new(),
        prompt: String::new(),
        width: image.width(),
        height: image.height(),
        character: CharacterProfile::default(),
    });
    Ok(id)
}
