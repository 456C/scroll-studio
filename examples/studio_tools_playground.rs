//! Integrated native QA story and export proofs. Generated in target/, never packaged.
use scroll_studio::{bubbles, clipping::*, compositor, effects::*, model::*, storage};
use std::{io::Cursor, path::PathBuf, sync::Arc, time::Instant};

fn asset(
    story: &mut Story,
    assets: &mut AssetBytes,
    name: &str,
    kind: AssetKind,
    image: image::RgbaImage,
) -> anyhow::Result<u64> {
    let id = story.alloc_id();
    let (width, height) = image.dimensions();
    let mut bytes = Cursor::new(Vec::new());
    image.write_to(&mut bytes, image::ImageFormat::Png)?;
    assets.insert(id, Arc::new(bytes.into_inner()));
    story.assets.push(Asset {
        extensions: Default::default(),
        provenance: None,
        id,
        name: name.into(),
        kind,
        notes: String::new(),
        prompt: String::new(),
        width,
        height,
        character: CharacterProfile::default(),
    });
    Ok(id)
}
fn layer(story: &mut Story, name: &str, rect: [f32; 4], kind: LayerKind) -> u64 {
    let id = story.alloc_id();
    story.chapters[0].layers.push(Layer {
        extensions: Default::default(),
        id,
        name: name.into(),
        visible: true,
        locked: false,
        opacity: 1.0,
        x: rect[0],
        y: rect[1],
        width: rect[2],
        height: rect[3],
        kind,
    });
    id
}
fn main() -> anyhow::Result<()> {
    let directory = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/tools-review"));
    std::fs::create_dir_all(&directory)?;
    let mut story = Story::with_canvas("Studio tools review", 800, 2200);
    story.visual_style =
        "Cool graphite and turquoise, clean webtoon linework, restrained cinematic light.".into();
    let mut assets = AssetBytes::new();
    let mut scene = image::RgbaImage::new(1600, 1000);
    for (x, y, p) in scene.enumerate_pixels_mut() {
        let grain = ((x * 17 + y * 31) % 13) as u8;
        let rail = y > 700 && (y % 70 < 6 || x % 140 < 5);
        let window = x % 260 < 140 && y > 150 && y < 610;
        *p = image::Rgba(if rail {
            [165, 205, 218, 255]
        } else if window {
            [70 + grain, 125 + grain, 145 + grain, 255]
        } else {
            [28 + grain, 48 + grain, 69 + grain, 255]
        });
    }
    let background = asset(
        &mut story,
        &mut assets,
        "Station · wide",
        AssetKind::Background,
        scene.clone(),
    )?;
    let tall = asset(
        &mut story,
        &mut assets,
        "Platform · vertical",
        AssetKind::Background,
        image::imageops::crop_imm(&scene, 500, 0, 600, 1000).to_image(),
    )?;
    let detail = asset(
        &mut story,
        &mut assets,
        "Window detail",
        AssetKind::Background,
        image::imageops::crop_imm(&scene, 250, 130, 600, 400).to_image(),
    )?;
    let location = story.alloc_id();
    story.locations.push(BackgroundLocation {
        extensions: Default::default(),
        id: location,
        name: "Midnight station".into(),
        asset_ids: vec![background, tall, detail],
        profile: LocationProfile {
            description: "An empty elevated station over the city.".into(),
            atmosphere: "Quiet, rainy, expectant".into(),
            lighting: "Cold overhead lights, warm windows in the distance".into(),
            consistency: "Teal columns, three tracks, glass canopy".into(),
            ..Default::default()
        },
    });
    let mut figure = image::RgbaImage::new(800, 1200);
    for (x, y, p) in figure.enumerate_pixels_mut() {
        let head = ((x as f32 - 400.0) / 95.0).powi(2) + ((y as f32 - 200.0) / 120.0).powi(2) < 1.0;
        let torso = (280..520).contains(&x) && (320..720).contains(&y);
        let legs = ((285..375).contains(&x) || (425..515).contains(&x)) && (700..1100).contains(&y);
        let arms = ((195..285).contains(&x) || (515..605).contains(&x)) && (350..760).contains(&y);
        if head {
            *p = image::Rgba([226, 198, 174, 255]);
        } else if torso || arms {
            *p = image::Rgba([58, 137 + (y % 5) as u8, 151, 255]);
        } else if legs {
            *p = image::Rgba([40, 54, 72, 255]);
        }
        if head && (190..200).contains(&y) && ((350..375).contains(&x) || (425..450).contains(&x)) {
            *p = image::Rgba([20, 30, 50, 255]);
        }
    }
    let character = asset(
        &mut story,
        &mut assets,
        "Ren",
        AssetKind::Character,
        figure.clone(),
    )?;
    let portrait = asset(
        &mut story,
        &mut assets,
        "Ren · portrait",
        AssetKind::Image,
        image::imageops::crop_imm(&figure, 200, 50, 400, 430).to_image(),
    )?;
    let wide = asset(
        &mut story,
        &mut assets,
        "Ren · color study",
        AssetKind::Image,
        image::imageops::resize(&figure, 1200, 600, image::imageops::FilterType::Nearest),
    )?;
    let c = story.assets.iter_mut().find(|a| a.id == character).unwrap();
    c.character.role = "Lead".into();
    c.character.consistency = "Teal jacket, dark trousers".into();
    c.character.references = vec![
        CharacterReference {
            asset_id: portrait,
            label: "Portrait".into(),
            notes: "Eye color and fringe".into(),
        },
        CharacterReference {
            asset_id: wide,
            label: "Palette study".into(),
            notes: String::new(),
        },
    ];
    layer(
        &mut story,
        "Station crop",
        [20.0, 20.0, 760.0, 700.0],
        LayerKind::Image {
            asset_id: background,
            mask: vec![],
            clip: None,
            source_rect: Some([0.15, 0.0, 0.65, 1.0]),
        },
    );
    let figure_layer = layer(
        &mut story,
        "Ren snippet",
        [240.0, 120.0, 300.0, 520.0],
        LayerKind::Image {
            asset_id: character,
            mask: vec![],
            clip: Some(Arc::new(ImageClip {
                selection: Arc::new(SelectionMask::subject(&figure, [400, 500], 28)?),
                edge: EdgeSettings::default(),
            })),
            source_rect: Some([0.20, 0.04, 0.60, 0.90]),
        },
    );
    layer(
        &mut story,
        "Wind",
        [40.0, 80.0, 720.0, 600.0],
        LayerKind::Effect {
            effect: Effect::Wind(Wind {
                curve: 0.7,
                ..Default::default()
            }),
        },
    );
    layer(
        &mut story,
        "Eye glint",
        [355.0, 157.0, 45.0, 45.0],
        LayerKind::Effect {
            effect: Effect::Glint(Glint::default()),
        },
    );
    layer(
        &mut story,
        "Action lines",
        [160.0, 80.0, 430.0, 600.0],
        LayerKind::Effect {
            effect: Effect::Motion(Motion {
                source_layer: figure_layer,
                ..Default::default()
            }),
        },
    );
    for (i, shape) in [
        BubbleShape::Intensity,
        BubbleShape::Impact,
        BubbleShape::Speech,
    ]
    .into_iter()
    .enumerate()
    {
        let mut preset = story.bubble_presets[0].clone();
        preset.id = story.alloc_id();
        preset.name = format!("{shape:?}");
        preset.style.shape = shape;
        preset.style.tail.enabled = false;
        story.bubble_presets.push(preset.clone());
        let id = layer(
            &mut story,
            &format!("{shape:?}"),
            [
                55.0 + (i % 2) as f32 * 360.0,
                770.0 + (i / 2) as f32 * 350.0,
                300.0,
                230.0,
            ],
            LayerKind::Text {
                text: if i == 0 {
                    "Not a single opening."
                } else if i == 1 {
                    "?!"
                } else {
                    "Stay close."
                }
                .into(),
                color: preset.color,
                size: 32.0,
                bubble: true,
                style: preset.style.clone(),
            },
        );
        bubbles::initialize_layer(story.chapters[0].layers.last_mut().unwrap(), &preset)?;
        story.chapters[0].layers.last_mut().unwrap().x = 35.0 + (i % 2) as f32 * 515.0;
        if i == 2 {
            bubbles::add_cluster_bubble(&mut story, 0, id)?;
        }
    }
    for (i, pattern) in [EdgePattern::Dotted, EdgePattern::Fade]
        .into_iter()
        .enumerate()
    {
        let mask = SelectionMask::polygon(
            1600,
            1000,
            &[[0.25, 0.1], [0.85, 0.2], [0.7, 0.9], [0.1, 0.7]],
        )?;
        layer(
            &mut story,
            pattern.label(),
            [30.0 + i as f32 * 385.0, 1540.0, 355.0, 350.0],
            LayerKind::Image {
                asset_id: background,
                mask: vec![],
                clip: Some(Arc::new(ImageClip {
                    selection: Arc::new(mask),
                    edge: EdgeSettings {
                        pattern,
                        width: 130,
                        contour: Contour::Outside,
                        dot_scale: 24,
                        background: [130, 199, 219, 100],
                        backing: Default::default(),
                    },
                })),
                source_rect: None,
            },
        );
    }
    let before = assets.clone();
    let start = Instant::now();
    let path = directory.join("Studio tools.scro");
    storage::save_story(&path, &story, &assets)?;
    let (loaded, data) = storage::load_story(&path)?;
    anyhow::ensure!(
        loaded == story && data == before,
        "Round trip changed original assets"
    );
    let recovery_root = if std::env::args().any(|arg| arg == "--native-recovery") {
        scroll_studio::recovery::default_directory()
    } else {
        directory.join("recovery-check")
    };
    let recovery = scroll_studio::recovery::RecoverySession::new(&recovery_root)?;
    match recovery.save(&loaded, &data, Some(&path), 1) {
        Ok(_) => {
            println!("Recovery round trip saved");
            recovery.clear()?;
        }
        Err(error) => {
            anyhow::bail!("Recovery check failed: {error:#}");
        }
    }
    let proof = compositor::render_region(&loaded.chapters[0], &data, 0, 2000, 800)?;
    proof.save(directory.join("Tools proof.png"))?;
    let fill_started = Instant::now();
    let patch = scroll_studio::healing::heal_stroke(
        &loaded.chapters[0],
        &data,
        &[[550.0, 210.0], [607.0, 228.0]],
        130.0,
    )?;
    let mut healed = compositor::render_region(&loaded.chapters[0], &data, 0, 720, 800)?;
    image::imageops::overlay(
        &mut healed,
        &patch.image,
        i64::from(patch.x),
        i64::from(patch.y),
    );
    healed.save(directory.join("Fill proof.png"))?;
    println!(
        "Composite fill + proof: {:.2}s",
        fill_started.elapsed().as_secs_f32()
    );

    let first = compositor::render_region(&loaded.chapters[0], &data, 0, 1000, 800)?;
    let second = compositor::render_region(&loaded.chapters[0], &data, 1000, 1000, 800)?;
    anyhow::ensure!(
        proof.as_raw() == &[first.as_raw().as_slice(), second.as_raw().as_slice()].concat(),
        "Export slices differ"
    );
    println!(
        "{} · {} assets · exact round trip and slice parity · {:.2}s",
        path.display(),
        assets.len(),
        start.elapsed().as_secs_f32()
    );
    Ok(())
}
