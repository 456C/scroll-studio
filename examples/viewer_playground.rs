//! Fine-detail reference fixture for native image-viewer QA. Not packaged.
use scroll_studio::{compositor, model::*, storage};
use std::{io::Cursor, path::PathBuf, sync::Arc};

fn main() -> anyhow::Result<()> {
    let directory = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/viewer-review"));
    std::fs::create_dir_all(&directory)?;
    let mut chart = Chapter::new(1, "Detail", 3072, 2048);
    let rows = [
        "REFERENCE DETAIL",
        "Eyes, fringe, collar and jewelry",
        "Fine lines should stay distinct",
        "Left / Center / Right",
        "0123456789  ABCDEFGHIJKLMNOP",
        "Zoom in to inspect the original pixels",
    ];
    for (index, text) in rows.iter().enumerate() {
        let mut style = TextStyle::dialogue();
        style.font = FontChoice::Karla;
        style.tail.enabled = false;
        chart.layers.push(Layer {
            id: index as u64 + 2,
            name: text.to_string(),
            visible: true,
            locked: false,
            opacity: 1.0,
            x: 100.0,
            y: 70.0 + index as f32 * 280.0,
            width: 2850.0,
            height: 180.0,
            kind: LayerKind::Text {
                text: text.to_string(),
                color: [25, 35, 45, 255],
                size: if index == 0 { 132.0 } else { 64.0 },
                bubble: false,
                style,
            },
        });
    }
    let mut image = compositor::render_region(&chart, &AssetBytes::new(), 0, 2048, 3072)?;
    for y in 1780..1940 {
        for x in 100..2972 {
            let spacing = if x < 1000 {
                4
            } else if x < 2000 {
                8
            } else {
                16
            };
            let color = if x % spacing < spacing / 2 {
                [30, 90, 130, 255]
            } else {
                [238, 242, 245, 255]
            };
            image.put_pixel(x, y, image::Rgba(color));
        }
    }
    image.save(directory.join("Reference detail.png"))?;
    let mut encoded = Cursor::new(Vec::new());
    image.write_to(&mut encoded, image::ImageFormat::Png)?;
    let mut story = Story::with_canvas("Viewer detail", 800, 1600);
    let id = story.alloc_id();
    story.assets.push(Asset {
        id,
        name: "Reference detail".into(),
        kind: AssetKind::Character,
        notes: String::new(),
        prompt: String::new(),
        width: 3072,
        height: 2048,
        character: CharacterProfile::default(),
    });
    let mut assets = AssetBytes::new();
    assets.insert(id, Arc::new(encoded.into_inner()));
    let path = directory.join("Viewer detail.scro");
    storage::save_story(&path, &story, &assets)?;
    let (loaded, loaded_assets) = storage::load_story(&path)?;
    anyhow::ensure!(
        loaded == story && loaded_assets == assets,
        "Fixture must round trip exactly"
    );
    println!("{}", path.display());
    Ok(())
}
