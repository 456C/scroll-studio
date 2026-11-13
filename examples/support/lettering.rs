use scroll_studio::{bubbles, model::*};

pub fn label_preset(story: &mut Story) -> BubblePreset {
    let mut preset = BubblePreset::for_canvas(story.alloc_id(), LetteringTheme::Mature, 1600);
    preset.name = "Labels".into();
    preset.bubble = false;
    preset.size = 64.0;
    preset.style.max_width = 1400.0;
    preset.style.tail.enabled = false;
    preset.color = [65, 80, 78, 255];
    story.bubble_presets.push(preset.clone());
    preset
}

pub fn paper(story: &mut Story, chapter: usize) {
    let id = story.alloc_id();
    let ch = &mut story.chapters[chapter];
    ch.layers.push(Layer {
        extensions: Default::default(),
        id,
        name: "Paper".into(),
        visible: true,
        locked: true,
        opacity: 1.0,
        x: 0.0,
        y: 0.0,
        width: ch.width as f32,
        height: ch.height as f32,
        kind: LayerKind::Shape {
            color: [235, 239, 235, 255],
        },
    });
}

pub fn text(
    story: &mut Story,
    chapter: usize,
    preset: &BubblePreset,
    text: &str,
    center: [f32; 2],
    targets: &[[f32; 2]],
) -> anyhow::Result<()> {
    let mut layer = Layer {
        extensions: Default::default(),
        id: story.alloc_id(),
        name: text.chars().take(32).collect(),
        visible: true,
        locked: !preset.bubble,
        opacity: 1.0,
        x: 0.0,
        y: 0.0,
        width: 420.0,
        height: 180.0,
        kind: LayerKind::Text {
            text: text.into(),
            color: preset.color,
            size: preset.size,
            bubble: preset.bubble,
            style: TextStyle::dialogue(),
        },
    };
    bubbles::initialize_layer(&mut layer, preset)?;
    layer.x = center[0] - layer.width * 0.5;
    layer.y = center[1] - layer.height * 0.5;
    if let LayerKind::Text { style, .. } = &mut layer.kind {
        style.tail.enabled = !targets.is_empty();
        for (index, target) in targets.iter().enumerate() {
            let tail = BubbleTail {
                enabled: true,
                tip: [
                    (target[0] - layer.x) / layer.width,
                    (target[1] - layer.y) / layer.height,
                ],
                width: preset.style.tail.width,
                bend: preset.style.tail.bend,
            };
            if index == 0 {
                style.tail = tail;
            } else {
                style.extra_tails.push(tail);
            }
        }
    }
    story.chapters[chapter].layers.push(layer);
    Ok(())
}

pub fn beat(story: &mut Story, chapter: usize, title: &str, script: &str, y: f32, height: f32) {
    let id = story.alloc_id();
    story.chapters[chapter].panels.push(Panel {
        id,
        title: title.into(),
        y,
        height,
        script: script.into(),
        notes: String::new(),
        prompt: String::new(),
    });
}
