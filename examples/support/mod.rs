//! Build-time sample checks and reading-size proofs, never part of the app.
use anyhow::{ensure, Result};
use scroll_studio::{compositor, model::*, storage};
use std::path::Path;

pub fn save(directory: &Path, name: &str, story: &Story, assets: &AssetBytes) -> Result<()> {
    storage::validate_story(story, assets)?;
    for chapter in &story.chapters {
        let mut lettering: Vec<(&str, [f32; 4])> = Vec::new();
        for layer in &chapter.layers {
            if let LayerKind::Text { size, style, .. } = &layer.kind {
                ensure!(
                    size * 360.0 / chapter.width as f32 >= 14.0,
                    "{} / {} is too small at reading width",
                    chapter.name,
                    layer.name
                );
                ensure!(
                    !compositor::text_overflows(layer),
                    "{} overflows",
                    layer.name
                );
                ensure!(
                    style
                        .preset_id
                        .is_some_and(|id| story.bubble_presets.iter().any(|p| p.id == id)),
                    "{} has no shared preset",
                    layer.name
                );
                let [x, y, w, h] = compositor::text_bounds(layer);
                ensure!(
                    x >= 0.0
                        && y >= 0.0
                        && x + w <= chapter.width as f32
                        && y + h <= chapter.height as f32,
                    "{} extends beyond its chapter: {x}, {y}, {w}, {h}",
                    layer.name
                );
                for (name, [a, b, c, d]) in &lettering {
                    ensure!(
                        x + w <= *a || a + c <= x || y + h <= *b || b + d <= y,
                        "{} overlaps {name}",
                        layer.name
                    );
                }
                lettering.push((&layer.name, [x, y, w, h]));
            }
        }
    }
    std::fs::create_dir_all(directory)?;
    let path = directory.join(format!("{name}.scro"));
    storage::save_story(&path, story, assets)?;
    let (loaded, loaded_assets) = storage::load_story(&path)?;
    ensure!(
        loaded == *story && loaded_assets == *assets,
        "{name} did not round-trip exactly"
    );
    for (index, chapter) in story.chapters.iter().enumerate() {
        let height = (chapter.height as f32 * 360.0 / chapter.width as f32).ceil() as u32;
        compositor::render_region(chapter, assets, 0, height, 360)?
            .save(directory.join(format!("{name}-{:02}.png", index + 1)))?;
    }
    println!("{}", path.display());
    Ok(())
}
