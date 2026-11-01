//! Compare the same dialogue in three themes at a consistent reading size.
#[path = "support/lettering.rs"]
mod lettering;
mod support;
use scroll_studio::model::*;
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let directory = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/sample-package"));
    let mut story = Story::with_canvas("Lettering playground", 1600, 3200);
    story.synopsis = "One exchange, three voices.".into();
    story.chapters.clear();
    let labels = lettering::label_preset(&mut story);
    for (index, theme) in LetteringTheme::ALL.into_iter().enumerate() {
        let id = story.alloc_id();
        story.chapters.push(Chapter::new(
            id,
            format!("{:02} · {}", index + 1, theme.label()),
            1600,
            3200,
        ));
        let preset = story
            .bubble_presets
            .iter()
            .find(|p| p.name == theme.label())
            .unwrap()
            .clone();
        lettering::paper(&mut story, index);
        lettering::text(
            &mut story,
            index,
            &labels,
            if theme == LetteringTheme::WebtoonClassic {
                "Classic"
            } else {
                theme.label()
            },
            [800.0, 180.0],
            &[],
        )?;
        for (title, words, center, target, y, height) in [
            (
                "Pause",
                "Wait.",
                [480.0, 610.0],
                [380.0, 900.0],
                350.0,
                650.0,
            ),
            (
                "Warning",
                "Stay close. Something is moving in the dark.",
                [800.0, 1420.0],
                [1230.0, 1870.0],
                1100.0,
                850.0,
            ),
            (
                "Resolve",
                "We came here for answers. If the gate closes, keep moving and don't look back.",
                [800.0, 2450.0],
                [360.0, 2940.0],
                2100.0,
                950.0,
            ),
        ] {
            lettering::text(&mut story, index, &preset, words, center, &[target])?;
            lettering::beat(&mut story, index, title, words, y, height);
        }
    }
    support::save(
        &directory,
        "Lettering playground",
        &story,
        &AssetBytes::new(),
    )
}
