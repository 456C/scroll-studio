//! Reviewed script passages become ordinary, preset-linked bubbles in one batch.
use crate::{bubbles, compositor, model::*};
use anyhow::{ensure, Context, Result};

pub const MAX_BLOCKS: usize = 256;
pub const MAX_BLOCK_BYTES: usize = 16_384;
pub const MAX_BATCH_BYTES: usize = 128_000;
const MARGIN: f32 = 16.0;
const GAP: f32 = 32.0;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DialogueBlock {
    pub included: bool,
    pub text: String,
}

/// Split only at blank lines. Speaker names, punctuation, and interior line
/// breaks remain untouched so the creator can explicitly review each passage.
pub fn script_blocks(script: &str) -> Result<Vec<DialogueBlock>> {
    ensure!(script.len() <= 1_000_000, "Script is too long");
    let mut blocks = Vec::new();
    let mut paragraph = String::new();
    let flush = |paragraph: &mut String, blocks: &mut Vec<DialogueBlock>| {
        if !paragraph.trim().is_empty() {
            blocks.push(DialogueBlock {
                included: true,
                text: paragraph.trim_end_matches(['\r', '\n']).to_owned(),
            });
        }
        paragraph.clear();
    };
    for line in script.split_inclusive('\n') {
        if line.trim().is_empty() {
            flush(&mut paragraph, &mut blocks);
        } else {
            paragraph.push_str(line);
        }
        ensure!(
            blocks.len() <= MAX_BLOCKS,
            "Review up to {MAX_BLOCKS} passages at a time"
        );
    }
    flush(&mut paragraph, &mut blocks);
    ensure!(
        blocks.len() <= MAX_BLOCKS,
        "Review up to {MAX_BLOCKS} passages at a time"
    );
    if blocks.is_empty() {
        blocks.push(DialogueBlock {
            included: true,
            text: String::new(),
        });
    }
    Ok(blocks)
}

pub struct PreparedDialogue {
    pub chapter_index: usize,
    pub layers: Vec<Layer>,
    pub next_id: u64,
}

/// Prepare fully fitted, non-overlapping bubbles without changing any story
/// state. Callers can checkpoint once and append only after every block fits.
pub fn prepare_layers(
    story: &Story,
    chapter_id: u64,
    beat_id: u64,
    preset_id: u64,
    blocks: &[DialogueBlock],
) -> Result<PreparedDialogue> {
    ensure!(
        blocks.len() <= MAX_BLOCKS,
        "Review up to {MAX_BLOCKS} passages at a time"
    );
    let chapter_index = story
        .chapters
        .iter()
        .position(|chapter| chapter.id == chapter_id)
        .context("This chapter is no longer in the story")?;
    let chapter = &story.chapters[chapter_index];
    let beat = chapter
        .panels
        .iter()
        .find(|beat| beat.id == beat_id)
        .context("This script beat is no longer in the chapter")?;
    let preset = story
        .bubble_presets
        .iter()
        .find(|preset| preset.id == preset_id)
        .context("Choose a bubble preset that is still in this story")?;
    ensure!(
        beat.y.is_finite()
            && (64..=8192).contains(&chapter.width)
            && (64..=200_000).contains(&chapter.height),
        "Invalid chapter or beat dimensions"
    );
    let selected: Vec<_> = blocks
        .iter()
        .filter(|block| block.included && !block.text.trim().is_empty())
        .collect();
    ensure!(
        !selected.is_empty(),
        "Include at least one passage with text"
    );
    ensure!(
        selected
            .iter()
            .all(|block| block.text.len() <= MAX_BLOCK_BYTES),
        "Keep each bubble below {MAX_BLOCK_BYTES} bytes of text"
    );
    ensure!(
        selected.iter().map(|block| block.text.len()).sum::<usize>() <= MAX_BATCH_BYTES,
        "This batch contains too much text"
    );
    ensure!(
        chapter.layers.len().saturating_add(selected.len()) <= 100_000,
        "Chapter layer limit reached"
    );
    let max_id = story
        .chapters
        .iter()
        .flat_map(|chapter| {
            std::iter::once(chapter.id)
                .chain(chapter.panels.iter().map(|beat| beat.id))
                .chain(chapter.layers.iter().map(|layer| layer.id))
        })
        .chain(story.assets.iter().map(|asset| asset.id))
        .chain(story.locations.iter().map(|location| location.id))
        .chain(story.bubble_presets.iter().map(|preset| preset.id))
        .max()
        .unwrap_or(0);
    let next_id = story
        .next_id
        .checked_add(selected.len() as u64)
        .filter(|next| *next < u64::MAX)
        .context("Story ID space exhausted")?;
    ensure!(
        story.next_id > max_id,
        "Story IDs need repair before adding bubbles"
    );
    let mut cursor = beat.y.max(MARGIN);
    let mut layers = Vec::with_capacity(selected.len());
    for (index, block) in selected.iter().enumerate() {
        let mut layer = Layer {
            id: story.next_id + index as u64,
            name: format!("Dialogue {}", index + 1),
            visible: true,
            locked: false,
            opacity: 1.0,
            x: 0.0,
            y: 0.0,
            width: 420.0,
            height: 160.0,
            kind: LayerKind::Text {
                text: block.text.clone(),
                color: [28, 31, 38, 255],
                size: 38.0,
                bubble: true,
                style: TextStyle::dialogue(),
            },
        };
        bubbles::initialize_layer(&mut layer, preset)?;
        ensure!(
            !compositor::text_overflows(&layer),
            "Bubble {} needs more text space. Widen the preset or reduce padding.",
            index + 1
        );
        let [left, top, width, height] = compositor::text_bounds(&layer);
        ensure!(
            [left, top, width, height].iter().all(|v| v.is_finite()),
            "Invalid bubble geometry"
        );
        ensure!(
            width <= chapter.width as f32 - MARGIN * 2.0,
            "Bubble {} is wider than the canvas. Choose a narrower preset.",
            index + 1
        );
        // Include outline and pointers when spacing; translate the whole bubble
        // so its fitted shape and default pointer relationship stay unchanged.
        layer.x += (chapter.width as f32 - width) * 0.5 - left;
        layer.y += cursor - top;
        cursor += height + GAP;
        layers.push(layer);
    }
    let bottom = cursor - GAP;
    ensure!(
        bottom <= chapter.height as f32 - MARGIN,
        "These bubbles need {} more px. Increase canvas height or include fewer passages.",
        (bottom + MARGIN - chapter.height as f32).ceil().max(0.0) as u32
    );
    Ok(PreparedDialogue {
        chapter_index,
        layers,
        next_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (Story, u64, u64, u64) {
        let story = Story::new("Dialogue");
        let chapter = story.chapters[0].id;
        let beat = story.chapters[0].panels[0].id;
        let preset = story.bubble_presets[0].id;
        (story, chapter, beat, preset)
    }

    #[test]
    fn paragraph_review_preserves_labels_lines_and_punctuation() {
        let blocks =
            script_blocks("  MIRA: Wait—\r\nwhat?\r\n \r\n[The door opens.]\r\n\r\nKAI: Yes.\n")
                .unwrap();
        assert_eq!(
            blocks
                .iter()
                .map(|block| block.text.as_str())
                .collect::<Vec<_>>(),
            ["  MIRA: Wait—\r\nwhat?", "[The door opens.]", "KAI: Yes."]
        );
        assert!(blocks.iter().all(|block| block.included));
        assert_eq!(script_blocks(" \n\n").unwrap()[0].text, "");
        assert!(script_blocks(&"a\n\n".repeat(MAX_BLOCKS + 1)).is_err());
    }

    #[test]
    fn fitted_batch_keeps_text_preset_pointer_and_reading_order() {
        let (mut story, chapter_id, beat_id, preset_id) = fixture();
        story.chapters[0].panels[0].y = 300.0;
        let mut blocks = script_blocks("MIRA: Hi.\n\n[Do not letter this.]\n\nA longer answer with enough words to need a larger bubble.").unwrap();
        blocks[1].included = false;
        let original = story.clone();
        let batch = prepare_layers(&story, chapter_id, beat_id, preset_id, &blocks).unwrap();
        assert_eq!(story, original);
        assert_eq!(batch.layers.len(), 2);
        assert_eq!(batch.next_id, story.next_id + 2);
        assert!(
            batch.layers[1].width * batch.layers[1].height
                > batch.layers[0].width * batch.layers[0].height
        );
        let mut bottom = 300.0;
        for (index, layer) in batch.layers.iter().enumerate() {
            assert_eq!(layer.id, story.next_id + index as u64);
            let [x, y, width, height] = compositor::text_bounds(layer);
            assert!(x >= MARGIN && x + width <= story.chapters[0].width as f32 - MARGIN);
            assert!(y >= bottom && y + height <= story.chapters[0].height as f32 - MARGIN);
            bottom = y + height + GAP;
            let LayerKind::Text { text, style, .. } = &layer.kind else {
                panic!("Not a bubble")
            };
            assert_eq!(text, &blocks[if index == 0 { 0 } else { 2 }].text);
            assert_eq!(style.preset_id, Some(preset_id));
            assert_eq!(style.tail.tip, story.bubble_presets[0].style.tail.tip);
            assert!(style.auto_fit && style.extra_tails.is_empty());
            assert!(!compositor::text_overflows(layer));
        }
    }

    #[test]
    fn stale_targets_bad_ids_and_overflow_fail_without_partial_changes() {
        let (mut story, chapter, beat, preset) = fixture();
        let blocks = script_blocks("Hello.\n\nGoodbye.").unwrap();
        for (chapter, beat, preset) in [
            (999, beat, preset),
            (chapter, 999, preset),
            (chapter, beat, 999),
        ] {
            assert!(prepare_layers(&story, chapter, beat, preset, &blocks).is_err());
        }
        story.chapters[0].height = 64;
        let before = story.clone();
        assert!(prepare_layers(&story, chapter, beat, preset, &blocks).is_err());
        assert_eq!(story, before);
        story.chapters[0].height = 9600;
        story.next_id = u64::MAX - 1;
        assert!(prepare_layers(&story, chapter, beat, preset, &blocks).is_err());
        story.next_id = 1;
        assert!(prepare_layers(&story, chapter, beat, preset, &blocks).is_err());
        let empty = vec![DialogueBlock {
            included: false,
            text: "ignored".into(),
        }];
        assert!(prepare_layers(&story, chapter, beat, preset, &empty).is_err());
    }

    #[test]
    fn valid_narrow_preset_with_large_padding_cannot_create_clipped_dialogue() {
        let (mut story, chapter, beat, preset) = fixture();
        // Both values are available through the preset controls and valid in a
        // saved story, but together leave no usable horizontal text area.
        story.bubble_presets[0].style.max_width = 120.0;
        story.bubble_presets[0].style.padding = 100.0;
        crate::storage::validate_story(&story, &AssetBytes::new()).unwrap();
        let before = story.clone();
        let blocks = script_blocks("Hello.").unwrap();
        let error = prepare_layers(&story, chapter, beat, preset, &blocks)
            .err()
            .expect("Clipped lettering must be rejected before committing");
        assert!(error
            .to_string()
            .contains("Widen the preset or reduce padding"));
        assert_eq!(
            story, before,
            "A failed batch cannot consume IDs or add layers"
        );
        story.bubble_presets[0].style.max_width = 640.0;
        let batch = prepare_layers(&story, chapter, beat, preset, &blocks).unwrap();
        assert_eq!(batch.layers.len(), 1);
        assert!(!compositor::text_overflows(&batch.layers[0]));
    }
}
