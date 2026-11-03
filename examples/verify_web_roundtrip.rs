//! Verify browser exports using the actual desktop loader, not a duplicate schema.
use anyhow::{Context, Result};
use scroll_studio::storage;
use std::path::PathBuf;
fn main() -> Result<()> {
    let directory = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| "target/web-tests".into());
    for entry in std::fs::read_dir(directory)? {
        let path = entry?.path();
        if path.extension().is_some_and(|e| e == "scro")
            && path.file_name().unwrap() != "invalid.scro"
        {
            let (story, assets) = storage::load_story(&path)
                .with_context(|| format!("Loading {}", path.display()))?;
            storage::validate_story(&story, &assets)?;
            println!(
                "Desktop loaded {}: {} assets, {} chapters",
                path.file_name().unwrap().to_string_lossy(),
                story.assets.len(),
                story.chapters.len()
            );
        }
    }
    Ok(())
}
