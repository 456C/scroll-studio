//! Synthetic cutout proof and isolated native review story; generated in target/.
use image::{Rgba, RgbaImage};
use scroll_studio::{clipping::*, compositor, cutout::BackingSettings, model::*, storage};
use std::{io::Cursor, path::PathBuf, sync::Arc, time::Instant};

fn main() -> anyhow::Result<()> {
    let directory = PathBuf::from("target/cutout-review");
    std::fs::create_dir_all(&directory)?;
    let mut source = RgbaImage::from_pixel(512, 640, Rgba([255, 255, 255, 255]));
    // A head, shoulders and separated hair strands, including enclosed gaps.
    for (x, y, p) in source.enumerate_pixels_mut() {
        let xf = x as f32;
        let yf = y as f32;
        let head = ((xf - 256.0) / 83.0).powi(2) + ((yf - 205.0) / 112.0).powi(2) < 1.0;
        let body = (330..580).contains(&y) && (xf - 256.0).abs() < 110.0 + (yf - 330.0) * 0.12;
        let hair = (150..390).contains(&y)
            && (0..9).any(|n| {
                let center = 150.0 + n as f32 * 26.0 + ((yf - 150.0) * 0.012).sin() * 24.0;
                (xf - center).abs() < 7.0 + (390.0 - yf) * 0.008
            });
        let crown = ((xf - 256.0) / 122.0).powi(2) + ((yf - 158.0) / 100.0).powi(2) < 1.0;
        if body {
            *p = Rgba([40, 69, 85, 255]);
        }
        if head {
            *p = Rgba([205, 165, 133, 255]);
        }
        if hair && !head || crown && y < 160 {
            *p = Rgba([29, 33, 47, 255]);
        }
    }
    source.save(directory.join("source.png"))?;
    let start = Instant::now();
    let background = SelectionMask::color_region(&source, [0, 0], 20, false)?;
    let mut mask = SelectionMask::polygon(512, 640, &[])?;
    mask.combine(&background, SelectionMode::Subtract)?;
    let clip = ImageClip {
        selection: Arc::new(mask),
        edge: Default::default(),
    };
    let mut softened = clip.clone();
    softened.edge.backing = BackingSettings {
        enabled: true,
        color: [155, 185, 190, 185],
        reach: 28,
        gaps: 24,
        softness: 100,
    };
    let mut cutout = source.clone();
    apply(&mut cutout, &clip)?;
    let mut blended = source.clone();
    apply(&mut blended, &softened)?;
    cutout.save(directory.join("cutout.png"))?;
    blended.save(directory.join("backing.png"))?;
    let mut proof = RgbaImage::from_pixel(1536, 640, Rgba([67, 100, 113, 255]));
    image::imageops::overlay(&mut proof, &source, 0, 0);
    image::imageops::overlay(&mut proof, &cutout, 512, 0);
    image::imageops::overlay(&mut proof, &blended, 1024, 0);
    proof.save(directory.join("before-after.png"))?;
    println!("512x640 erase + backing + proof: {:?}", start.elapsed());
    let mut story = Story::with_canvas("Cutout review", 800, 900);
    let asset_id = story.alloc_id();
    story.assets.push(Asset {
        id: asset_id,
        name: "Hair study".into(),
        kind: AssetKind::Image,
        width: 512,
        height: 640,
        notes: String::new(),
        prompt: String::new(),
        character: Default::default(),
    });
    let mut bytes = Cursor::new(Vec::new());
    source.write_to(&mut bytes, image::ImageFormat::Png)?;
    let mut assets = AssetBytes::new();
    assets.insert(asset_id, Arc::new(bytes.into_inner()));
    for (name, x, y, width, height, kind) in [
        (
            "Scene",
            0.0,
            0.0,
            800.0,
            900.0,
            LayerKind::Shape {
                color: [67, 100, 113, 255],
            },
        ),
        (
            "Try Magic Eraser",
            40.0,
            90.0,
            340.0,
            425.0,
            LayerKind::Image {
                asset_id,
                mask: vec![],
                clip: None,
                source_rect: None,
            },
        ),
        (
            "Soft backing",
            420.0,
            90.0,
            340.0,
            425.0,
            LayerKind::Image {
                asset_id,
                mask: vec![],
                clip: Some(Arc::new(softened)),
                source_rect: None,
            },
        ),
    ] {
        let id = story.alloc_id();
        story.chapters[0].layers.push(Layer {
            id,
            name: name.into(),
            x,
            y,
            width,
            height,
            visible: true,
            locked: false,
            opacity: 1.0,
            kind,
        });
    }
    let path = directory.join("Cutout review.scro");
    storage::save_story(&path, &story, &assets)?;
    let (loaded, originals) = storage::load_story(&path)?;
    assert_eq!(story, loaded);
    assert_eq!(assets, originals);
    compositor::render_region(&loaded.chapters[0], &originals, 0, 900, 800)?
        .save(directory.join("export.png"))?;
    println!("Saved {} with one original image", path.display());
    Ok(())
}
