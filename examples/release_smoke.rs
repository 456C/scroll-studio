//! Release smoke test for packaged sample stories, using the desktop library.
//!
//! For every `.scro` in the given folder: open it, export its first chapter as
//! WEBTOON PNG slices, save it under a new name, reopen the copy and check that
//! the story and its image bytes came back unchanged.
//!
//! Usage: release_smoke <stories folder> <empty work folder>
use anyhow::{bail, ensure, Context, Result};
use scroll_studio::{export, storage};
use std::path::PathBuf;

fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let stories: PathBuf = args.next().context("Missing stories folder")?.into();
    let work: PathBuf = args.next().context("Missing work folder")?.into();
    std::fs::create_dir_all(&work)?;
    let mut checked = 0;
    let mut entries: Vec<_> = std::fs::read_dir(&stories)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|e| e == "scro"))
        .collect();
    entries.sort();
    for (index, path) in entries.iter().enumerate() {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let (story, assets) =
            storage::load_story(path).with_context(|| format!("Opening {name}"))?;
        storage::validate_story(&story, &assets).with_context(|| format!("Validating {name}"))?;

        let exported = match story.chapters.first() {
            Some(chapter) => {
                let directory = work.join(format!("{index}-export"));
                let report = export::export_chapter_with_options(
                    chapter,
                    &assets,
                    &directory,
                    &export::ExportOptions::default(),
                )
                .with_context(|| format!("Exporting the first chapter of {name}"))?;
                ensure!(!report.files.is_empty(), "{name}: export wrote no images");
                for file in &report.files {
                    ensure!(
                        file.path.is_file(),
                        "{name}: missing {}",
                        file.path.display()
                    );
                }
                report.files.len()
            }
            None => 0,
        };

        let copy = work.join(format!("{index}-saved.scro"));
        storage::save_story(&copy, &story, &assets).with_context(|| format!("Saving {name}"))?;
        let (reopened, reopened_assets) =
            storage::load_story(&copy).with_context(|| format!("Reopening saved {name}"))?;
        storage::validate_story(&reopened, &reopened_assets)?;
        if serde_json::to_value(&story)? != serde_json::to_value(&reopened)? {
            bail!("{name}: the story changed after save and reopen");
        }
        ensure!(
            assets.len() == reopened_assets.len()
                && assets
                    .iter()
                    .all(|(id, bytes)| reopened_assets.get(id).is_some_and(|b| b == bytes)),
            "{name}: image bytes changed after save and reopen"
        );
        println!(
            "OK {name}: {} chapters, {} assets, first chapter exported to {exported} image(s), save and reopen identical",
            story.chapters.len(),
            story.assets.len()
        );
        checked += 1;
    }
    ensure!(
        checked > 0,
        "No .scro stories found in {}",
        stories.display()
    );
    println!("Release smoke passed for {checked} stories.");
    Ok(())
}
