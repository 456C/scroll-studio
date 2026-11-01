//! Export the built-in demonstration for native and reading-size review.
mod support;
use scroll_studio::model::*;
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let directory = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/sample-package"));
    std::fs::create_dir_all(&directory)?;
    let story = Story::demo();
    support::save(&directory, "After the rain", &story, &AssetBytes::new())
}
