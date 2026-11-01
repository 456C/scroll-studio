//! Shared voices and independent speaker targets in two vertical chapters.
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
    let mut story = Story::with_canvas("Bubble presets playground", 1600, 4000);
    story.synopsis = "A group at a locked gate. One voice answers from the other side.".into();
    story.visual_style = "Atmospheric urban mystery, clean expressive ink, soft cel shading, muted teal shadows and warm amber light. Clear adult silhouettes and spacious vertical framing. No rendered text.".into();
    story.chapters[0].name = "01 · The signal".into();
    story.chapters[0].panels.clear();
    let id = story.alloc_id();
    story
        .chapters
        .push(Chapter::new(id, "02 · An answer", 1600, 4000));
    let classic = story
        .bubble_presets
        .iter()
        .find(|p| p.name == "Webtoon Classic")
        .unwrap()
        .clone();
    let casual = story
        .bubble_presets
        .iter_mut()
        .find(|p| p.name == "Casual")
        .unwrap();
    casual.style.shape = BubbleShape::Rounded;
    let casual = casual.clone();
    let mut thought = story
        .bubble_presets
        .iter()
        .find(|p| p.name == "Mature")
        .unwrap()
        .clone();
    thought.id = story.alloc_id();
    thought.name = "Thought".into();
    thought.style.shape = BubbleShape::Thought;
    thought.style.fill = [249, 248, 242, 255];
    story.bubble_presets.push(thought.clone());
    let labels = lettering::label_preset(&mut story);
    for index in 0..2 {
        lettering::paper(&mut story, index);
        lettering::text(
            &mut story,
            index,
            &labels,
            if index == 0 { "Signal" } else { "Answer" },
            [800.0, 180.0],
            &[],
        )?;
        let short = if index == 0 {
            "Hold on."
        } else {
            "We're ready."
        };
        let long = if index == 0 {
            "We can get through this. Stay close, and watch each other's backs."
        } else {
            "The door is open. Whatever happens next, we face it together."
        };
        let inner = if index == 0 {
            "Did we both hear that?"
        } else {
            "Maybe this is our chance."
        };
        let call = if index == 0 {
            "Okay, okay. I'm coming!"
        } else {
            "Hey, save a place for me!"
        };
        lettering::text(
            &mut story,
            index,
            &classic,
            short,
            [800.0, 630.0],
            &[[450.0, 850.0], [800.0, 380.0], [1150.0, 850.0]],
        )?;
        lettering::text(
            &mut story,
            index,
            &classic,
            long,
            [800.0, 1500.0],
            &[[260.0, 1920.0], [1330.0, 1920.0]],
        )?;
        lettering::text(
            &mut story,
            index,
            &thought,
            inner,
            [800.0, 2530.0],
            &[[340.0, 2940.0], [1280.0, 2940.0]],
        )?;
        lettering::text(
            &mut story,
            index,
            &casual,
            call,
            [800.0, 3400.0],
            &[[430.0, 3720.0], [1170.0, 3720.0]],
        )?;
        for (title, script, y, height) in [
            ("A chorus", short, 330.0, 600.0),
            ("Together", long, 1100.0, 900.0),
            ("A shared thought", inner, 2200.0, 800.0),
            ("A voice behind us", call, 3100.0, 750.0),
        ] {
            lettering::beat(&mut story, index, title, script, y, height);
        }
    }
    support::save(
        &directory,
        "Bubble presets playground",
        &story,
        &AssetBytes::new(),
    )
}
