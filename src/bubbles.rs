//! Story-wide lettering definitions with per-bubble text and speaker targets.
use crate::{compositor, model::*};
use anyhow::{ensure, Context, Result};

/// Independent layers share only their cluster identity. Render-time neighbors
/// remove internal outlines without flattening dialogue into one text asset.
pub fn joined_layer(layer: &Layer, layers: &[Layer]) -> Layer {
    let mut rendered = layer.clone();
    let LayerKind::Text {
        style,
        bubble: true,
        ..
    } = &layer.kind
    else {
        return rendered;
    };
    let Some(cluster) = style.cluster_id else {
        return rendered;
    };
    let index = layers.iter().position(|l| l.id == layer.id).unwrap_or(0);
    let mut joins = Vec::new();
    for (other_index, other) in layers.iter().enumerate() {
        if other.id == layer.id || !other.visible || other.opacity != layer.opacity {
            continue;
        }
        let LayerKind::Text {
            style: appearance,
            bubble: true,
            ..
        } = &other.kind
        else {
            continue;
        };
        if appearance.cluster_id != Some(cluster)
            || appearance.fill != style.fill
            || appearance.outline != style.outline
            || appearance.outline_width != style.outline_width
        {
            continue;
        }
        // Distant bubbles stay in the same speech cluster but need no geometry.
        if other.x > layer.x + layer.width
            || other.x + other.width < layer.x
            || other.y > layer.y + layer.height
            || other.y + other.height < layer.y
        {
            continue;
        }
        joins.push(BubbleJoin {
            x: other.x - layer.x,
            y: other.y - layer.y,
            width: other.width,
            height: other.height,
            shape: appearance.shape,
            painted_before: other_index < index,
        });
        if joins.len() >= 16 {
            break;
        }
    }
    if let LayerKind::Text { style, .. } = &mut rendered.kind {
        style.joins = joins;
    }
    rendered
}

pub fn add_cluster_bubble(story: &mut Story, chapter: usize, parent: u64) -> Result<u64> {
    let index = story
        .chapters
        .get(chapter)
        .context("Chapter missing")?
        .layers
        .iter()
        .position(|l| l.id == parent && !l.locked)
        .context("Select an unlocked bubble")?;
    let original = story.chapters[chapter].layers[index].clone();
    let LayerKind::Text {
        style,
        bubble: true,
        ..
    } = &original.kind
    else {
        anyhow::bail!("Select a speech bubble");
    };
    let cluster = style.cluster_id.unwrap_or(parent);
    let id = story.alloc_id();
    let mut child = original.clone();
    child.id = id;
    child.name = "Linked dialogue".into();
    if let LayerKind::Text { text, style, .. } = &mut child.kind {
        *text = "More dialogue…".into();
        style.cluster_id = Some(cluster);
        style.tail.enabled = false;
        style.extra_tails.clear();
        style.joins.clear();
    }
    fit_layer(&mut child)?;
    child.x = original.x + original.width * 0.22;
    child.y = original.y + original.height * 0.80;
    // Reusing + on an earlier bubble appends to the chain rather than placing
    // two independent dialogue layers on exactly the same rectangle.
    for _ in 0..32 {
        let collision = story.chapters[chapter].layers.iter().find(|other| {
            let LayerKind::Text { style, .. } = &other.kind else {
                return false;
            };
            if style.cluster_id != Some(cluster) {
                return false;
            }
            let width = (child.x + child.width).min(other.x + other.width) - child.x.max(other.x);
            let height =
                (child.y + child.height).min(other.y + other.height) - child.y.max(other.y);
            width.max(0.0) * height.max(0.0) > child.width * child.height * 0.5
        });
        let Some(other) = collision else {
            break;
        };
        child.x = other.x + other.width * 0.22;
        child.y = other.y + other.height * 0.80;
    }
    if let LayerKind::Text { style, .. } = &mut story.chapters[chapter].layers[index].kind {
        style.cluster_id = Some(cluster);
    }
    story.chapters[chapter].layers.insert(index + 1, child);
    Ok(id)
}

/// Capture appearance without accidentally making speaker placement a shared setting.
pub fn preset_from_layer(id: u64, name: impl Into<String>, layer: &Layer) -> Option<BubblePreset> {
    let LayerKind::Text {
        color,
        size,
        bubble,
        style,
        ..
    } = &layer.kind
    else {
        return None;
    };
    let mut style = style.clone();
    style.preset_id = None;
    style.cluster_id = None;
    style.joins.clear();
    style.extra_tails.clear();
    style.tail.tip = BubbleTail::default().tip;
    // A preset describes a pointer's appearance, not whether a speaker needs one.
    style.tail.enabled = true;
    style.auto_fit = true;
    Some(BubblePreset {
        id,
        name: name.into(),
        style,
        color: *color,
        size: *size,
        bubble: *bubble,
        opacity: layer.opacity,
    })
}

fn same_appearance(a: &BubblePreset, b: &BubblePreset) -> bool {
    a.style == b.style
        && a.color == b.color
        && a.size == b.size
        && a.bubble == b.bubble
        && a.opacity == b.opacity
}

/// Fit around the current words, preserving the body center and every speaker target.
/// This also opts old lettering into the modern layout and enables automatic fitting.
pub fn fit_layer(layer: &mut Layer) -> Result<()> {
    let LayerKind::Text { style, .. } = &layer.kind else {
        return Ok(());
    };
    let targets: Vec<_> = style
        .tails()
        .map(|tail| {
            [
                layer.x + tail.tip[0] * layer.width,
                layer.y + tail.tip[1] * layer.height,
            ]
        })
        .collect();
    let mut fitted = layer.clone();
    let LayerKind::Text { style, .. } = &mut fitted.kind else {
        unreachable!();
    };
    style.auto_fit = true;
    style.modern_layout = true;
    let maximum = style.max_width;
    let [width, height] = compositor::fit_text_size(&fitted, maximum)?;
    let center = [layer.x + layer.width * 0.5, layer.y + layer.height * 0.5];
    fitted.width = width.clamp(40.0, 16384.0);
    fitted.height = height.clamp(20.0, 200000.0);
    fitted.x = center[0] - fitted.width * 0.5;
    fitted.y = center[1] - fitted.height * 0.5;
    if let LayerKind::Text { style, .. } = &mut fitted.kind {
        for (tail, target) in style.tails_mut().zip(targets) {
            tail.tip = [
                (target[0] - fitted.x).clamp(-MAX_POINTER_OFFSET, MAX_POINTER_OFFSET)
                    / fitted.width,
                (target[1] - fitted.y).clamp(-MAX_POINTER_OFFSET, MAX_POINTER_OFFSET)
                    / fitted.height,
            ];
        }
    }
    *layer = fitted;
    Ok(())
}

/// Assign an existing bubble to a preset. Text and all independent pointers survive.
pub fn apply_preset(layer: &mut Layer, preset: &BubblePreset) -> Result<()> {
    let mut updated = layer.clone();
    let LayerKind::Text {
        color,
        size,
        bubble,
        style,
        ..
    } = &mut updated.kind
    else {
        return Ok(());
    };
    let first = style.tail.clone();
    let cluster = style.cluster_id;
    let extra = style.extra_tails.clone();
    *style = preset.style.clone();
    style.preset_id = Some(preset.id);
    style.cluster_id = cluster;
    style.tail.enabled = first.enabled;
    style.tail.tip = first.tip;
    style.extra_tails = extra;
    for tail in &mut style.extra_tails {
        tail.width = preset.style.tail.width;
        tail.bend = preset.style.tail.bend;
    }
    *color = preset.color;
    *size = preset.size;
    *bubble = preset.bubble;
    updated.opacity = preset.opacity;
    fit_layer(&mut updated)?;
    *layer = updated;
    Ok(())
}

/// Start a new bubble with a preset's default pointer, then size it around its words.
pub fn initialize_layer(layer: &mut Layer, preset: &BubblePreset) -> Result<()> {
    let mut updated = layer.clone();
    if let LayerKind::Text {
        color,
        size,
        bubble,
        style,
        ..
    } = &mut updated.kind
    {
        *style = preset.style.clone();
        style.preset_id = Some(preset.id);
        style.extra_tails.clear();
        *color = preset.color;
        *size = preset.size;
        *bubble = preset.bubble;
        updated.opacity = preset.opacity;
        fit_layer(&mut updated)?;
        if let LayerKind::Text { style, .. } = &mut updated.kind {
            style.tail.tip = preset.style.tail.tip;
        }
        *layer = updated;
    }
    Ok(())
}

/// Commit an edited definition and update every linked chapter as one undoable change.
/// Stage fitted layers before touching the story so a layout failure is atomic.
pub fn update_preset(story: &mut Story, mut preset: BubblePreset) -> Result<()> {
    let preset_index = story
        .bubble_presets
        .iter()
        .position(|p| p.id == preset.id)
        .context("This bubble preset is no longer in the story")?;
    preset.style.preset_id = None;
    preset.style.extra_tails.clear();
    preset.style.tail.tip = BubbleTail::default().tip;
    preset.style.auto_fit = true;
    if same_appearance(&story.bubble_presets[preset_index], &preset) {
        story.bubble_presets[preset_index] = preset;
        return Ok(());
    }
    let mut updates = Vec::new();
    for (chapter_index, chapter) in story.chapters.iter().enumerate() {
        for (layer_index, layer) in chapter.layers.iter().enumerate() {
            if matches!(&layer.kind, LayerKind::Text {style, ..} if style.preset_id == Some(preset.id))
            {
                let mut updated = layer.clone();
                apply_preset(&mut updated, &preset)?;
                updates.push((chapter_index, layer_index, updated));
            }
        }
    }
    story.bubble_presets[preset_index] = preset;
    for (chapter_index, layer_index, layer) in updates {
        story.chapters[chapter_index].layers[layer_index] = layer;
    }
    Ok(())
}

/// Link old loose lettering to story presets without changing a single painted setting.
/// Validate the archive first; checked IDs still make this safe for in-memory callers.
pub fn migrate_story_presets(story: &mut Story) -> Result<()> {
    let mut next_id = story.next_id;
    let mut presets = story.bubble_presets.clone();
    // Old stories may contain no dialogue yet. They still need the same usable
    // starting choices as a newly created story, without consuming existing IDs.
    if presets.is_empty() {
        for theme in [
            LetteringTheme::WebtoonClassic,
            LetteringTheme::Mature,
            LetteringTheme::Casual,
        ] {
            ensure!(
                next_id > 0 && next_id < u64::MAX - 1,
                "Story ID space exhausted"
            );
            presets.push(BubblePreset::for_theme(next_id, theme));
            next_id = next_id.checked_add(1).context("Story ID space exhausted")?;
        }
    }
    let mut links = Vec::new();
    for (chapter_index, chapter) in story.chapters.iter().enumerate() {
        for (layer_index, layer) in chapter.layers.iter().enumerate() {
            if !matches!(&layer.kind, LayerKind::Text {style, ..} if style.preset_id.is_none()) {
                continue;
            }
            let candidate = preset_from_layer(0, "", layer).expect("text layer");
            let id = if let Some(existing) = presets.iter().find(|p| same_appearance(p, &candidate))
            {
                existing.id
            } else {
                ensure!(presets.len() < 4096, "Story has too many bubble presets");
                let id = next_id;
                ensure!(id > 0 && id < u64::MAX - 1, "Story ID space exhausted");
                next_id = next_id.checked_add(1).context("Story ID space exhausted")?;
                let mut candidate = candidate;
                candidate.id = id;
                candidate.name = format!(
                    "Imported · {}",
                    layer.name.chars().take(80).collect::<String>()
                );
                presets.push(candidate);
                id
            };
            links.push((chapter_index, layer_index, id));
        }
    }
    story.bubble_presets = presets;
    story.next_id = next_id;
    for (chapter_index, layer_index, id) in links {
        if let LayerKind::Text { style, .. } =
            &mut story.chapters[chapter_index].layers[layer_index].kind
        {
            style.preset_id = Some(id);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dialogue(id: u64) -> Layer {
        Layer {
            id,
            name: "Dialogue".into(),
            visible: true,
            locked: false,
            opacity: 1.0,
            x: 200.0,
            y: 300.0,
            width: 450.0,
            height: 150.0,
            kind: LayerKind::Text {
                text: "Wait.".into(),
                color: [0, 0, 0, 255],
                size: 38.0,
                bubble: true,
                style: TextStyle {
                    tail: BubbleTail {
                        enabled: true,
                        tip: [-1.0, -1.5],
                        ..Default::default()
                    },
                    extra_tails: vec![BubbleTail {
                        enabled: true,
                        tip: [2.0, 3.0],
                        ..Default::default()
                    }],
                    ..TextStyle::dialogue()
                },
            },
        }
    }

    fn targets(layer: &Layer) -> Vec<[f32; 2]> {
        let LayerKind::Text { style, .. } = &layer.kind else {
            panic!();
        };
        style
            .tails()
            .map(|tail| {
                [
                    layer.x + tail.tip[0] * layer.width,
                    layer.y + tail.tip[1] * layer.height,
                ]
            })
            .collect()
    }

    fn assert_targets(a: &[[f32; 2]], b: &[[f32; 2]]) {
        assert_eq!(a.len(), b.len());
        for (a, b) in a.iter().flatten().zip(b.iter().flatten()) {
            assert!((a - b).abs() < 0.001);
        }
    }

    #[test]
    fn fitting_preserves_multiple_signed_speaker_targets() {
        let mut layer = dialogue(1);
        let before = targets(&layer);
        fit_layer(&mut layer).unwrap();
        assert_targets(&before, &targets(&layer));
        let short_width = layer.width;
        if let LayerKind::Text { text, .. } = &mut layer.kind {
            *text = "There are two speakers here, and both of them must remain connected to this growing bubble.".into();
        }
        fit_layer(&mut layer).unwrap();
        assert!(layer.width > short_width);
        assert_targets(&before, &targets(&layer));
    }

    #[test]
    fn preset_edit_updates_locked_hidden_bubbles_in_every_chapter() {
        let mut story = Story::new("Shared dialogue");
        let preset = story.bubble_presets[0].clone();
        let mut layer = dialogue(story.alloc_id());
        apply_preset(&mut layer, &preset).unwrap();
        let original_targets = targets(&layer);
        story.chapters[0].layers.push(layer.clone());
        layer.id = story.alloc_id();
        layer.locked = true;
        layer.visible = false;
        let mut chapter = Chapter::new(story.alloc_id(), "Two", 1600, 9600);
        chapter.layers.push(layer);
        story.chapters.push(chapter);
        let mut edited = preset;
        edited.style.shape = BubbleShape::Shout;
        edited.style.fill = [240, 230, 220, 255];
        edited.style.tail.width = 60.0;
        edited.size = 52.0;
        update_preset(&mut story, edited.clone()).unwrap();
        for chapter in &story.chapters {
            let layer = &chapter.layers[0];
            let LayerKind::Text {
                style, size, text, ..
            } = &layer.kind
            else {
                panic!();
            };
            assert_eq!(*size, 52.0);
            assert_eq!(text, "Wait.");
            assert_eq!(style.shape, BubbleShape::Shout);
            assert_eq!(style.fill, edited.style.fill);
            assert!(style.tails().all(|tail| tail.width == 60.0));
            assert_targets(&original_targets, &targets(layer));
        }
    }

    #[test]
    fn migration_groups_appearance_without_changing_geometry_or_pointer_count() {
        let mut story = Story::new("Old lettering");
        let first = dialogue(story.alloc_id());
        let mut second = first.clone();
        second.id = story.alloc_id();
        if let LayerKind::Text { style, .. } = &mut second.kind {
            style.extra_tails.clear();
            style.tail.tip = [2.0, -0.5];
            style.auto_fit = false;
        }
        story.chapters[0].layers = vec![first, second];
        let before = story.chapters[0].layers.clone();
        migrate_story_presets(&mut story).unwrap();
        let mut ids = Vec::new();
        for (a, b) in before.iter().zip(&story.chapters[0].layers) {
            let mut b = b.clone();
            if let LayerKind::Text { style, .. } = &mut b.kind {
                ids.push(style.preset_id.take());
            }
            assert_eq!(a, &b);
        }
        assert_eq!(ids[0], ids[1]);
        let migrated = story.clone();
        migrate_story_presets(&mut story).unwrap();
        assert_eq!(story, migrated);
    }

    #[test]
    fn new_canvas_presets_keep_glyphs_and_bubbles_consistent_at_phone_width() {
        for theme in LetteringTheme::ALL {
            let mut reference: Option<[f32; 3]> = None;
            for width in [800, 940, 1600] {
                let preset = BubblePreset::for_canvas(2, theme, width);
                let mut layer = dialogue(1);
                if let LayerKind::Text { text, .. } = &mut layer.kind {
                    *text = "Wait for me.".into();
                }
                initialize_layer(&mut layer, &preset).unwrap();
                assert!(!compositor::text_overflows(&layer));
                let phone_scale = 360.0 / width as f32;
                let body = [layer.width * phone_scale, layer.height * phone_scale];
                // Measure actual rendered ink, not just the nominal font size.
                if let LayerKind::Text { bubble, .. } = &mut layer.kind {
                    *bubble = false;
                }
                let bitmap = compositor::text_preview(&layer).unwrap();
                let rows: Vec<_> = (0..bitmap.height())
                    .filter(|&y| (0..bitmap.width()).any(|x| bitmap.get_pixel(x, y)[3] > 127))
                    .collect();
                let ink_height = (rows.last().unwrap() - rows[0] + 1) as f32 * phone_scale;
                assert!(
                    ink_height >= 9.0,
                    "Tiny phone lettering: {theme:?}, {width}, {ink_height}"
                );
                let current = [body[0], body[1], ink_height];
                if let Some(reference) = reference {
                    // Natural fitting uses whole-pixel safety margins and a
                    // discrete width search; allow two phone pixels for bodies.
                    for (index, (a, b)) in current.into_iter().zip(reference).enumerate() {
                        assert!(
                            (a - b).abs() < if index == 2 { 1.0 } else { 2.0 },
                            "Resolution changes reading size: {theme:?}, {width}: {a} vs {b}"
                        );
                    }
                } else {
                    reference = Some(current);
                }
            }
        }
    }

    #[test]
    fn existing_presets_survive_loading_and_migration_at_their_saved_scale() {
        let mut story = Story::new("Existing 1600 px story");
        for preset in &mut story.bubble_presets {
            *preset = BubblePreset::for_theme(preset.id, preset.style.theme().unwrap());
        }
        let preset = story.bubble_presets[0].clone();
        let mut layer = dialogue(story.alloc_id());
        initialize_layer(&mut layer, &preset).unwrap();
        story.chapters[0].layers.push(layer);
        let mut loaded: Story =
            serde_json::from_slice(&serde_json::to_vec(&story).unwrap()).unwrap();
        migrate_story_presets(&mut loaded).unwrap();
        assert_eq!(loaded, story);
        crate::storage::validate_story(&loaded, &AssetBytes::new()).unwrap();
    }

    #[test]
    fn narration_round_trip_preserves_hidden_speaker_targets_and_pointer_flags() {
        let mut layer = dialogue(1);
        if let LayerKind::Text { style, .. } = &mut layer.kind {
            style.extra_tails[0].enabled = false;
        }
        let original_targets = targets(&layer);
        let mut preset = BubblePreset::for_theme(2, LetteringTheme::Mature);
        preset.style.shape = BubbleShape::Narration;
        preset.style.tail.enabled = false;
        apply_preset(&mut layer, &preset).unwrap();
        assert_targets(&original_targets, &targets(&layer));
        preset.style.shape = BubbleShape::Speech;
        preset.style.tail.enabled = true;
        apply_preset(&mut layer, &preset).unwrap();
        assert_targets(&original_targets, &targets(&layer));
        let LayerKind::Text { style, .. } = &layer.kind else {
            panic!();
        };
        assert!(style.tail.enabled);
        assert_eq!(style.extra_tails.len(), 1);
        assert!(!style.extra_tails[0].enabled);
    }
    #[test]
    fn linked_dialogue_stays_independent_and_follows_its_story_preset() {
        let mut story = Story::new("Cluster");
        let preset = story.bubble_presets[0].clone();
        let id = story.alloc_id();
        let mut parent = dialogue(id);
        initialize_layer(&mut parent, &preset).unwrap();
        story.chapters[0].layers.push(parent);
        let child = add_cluster_bubble(&mut story, 0, id).unwrap();
        assert_ne!(id, child);
        if let LayerKind::Text { text, .. } = &mut story.chapters[0].layers[1].kind {
            *text = "A separate thought.".into();
        }
        let mut changed = preset;
        changed.style.shape = BubbleShape::Intensity;
        update_preset(&mut story, changed).unwrap();
        for l in &story.chapters[0].layers {
            let LayerKind::Text { style, .. } = &l.kind else {
                panic!()
            };
            assert_eq!(style.cluster_id, Some(id));
            assert_eq!(style.shape, BubbleShape::Intensity);
        }
        let rendered = joined_layer(&story.chapters[0].layers[0], &story.chapters[0].layers);
        let LayerKind::Text { style, .. } = &rendered.kind else {
            panic!()
        };
        assert_eq!(style.joins.len(), 1);
        let json = serde_json::to_string(&story).unwrap();
        assert!(!json.contains("painted_before"));
        assert_eq!(serde_json::from_str::<Story>(&json).unwrap(), story);
        let LayerKind::Text { text, .. } = &story.chapters[0].layers[0].kind else {
            panic!()
        };
        assert_eq!(text, "Wait.");
    }
    #[test]
    fn overlap_has_no_internal_border_and_detaching_restores_it() {
        let mut chapter = Chapter::new(1, "Union", 400, 400);
        for (id, x, y) in [(2, 50.0, 70.0), (3, 170.0, 100.0)] {
            let mut l = dialogue(id);
            l.x = x;
            l.y = y;
            l.width = 200.0;
            l.height = 160.0;
            if let LayerKind::Text { text, style, .. } = &mut l.kind {
                text.clear();
                style.shape = BubbleShape::Speech;
                style.tail.enabled = false;
                style.extra_tails.clear();
                style.cluster_id = Some(2);
                style.fill = [255; 4];
                style.outline = [0, 0, 0, 255];
                style.outline_width = 4.0;
            }
            chapter.layers.push(l);
        }
        let joined = compositor::render_region(&chapter, &AssetBytes::new(), 0, 400, 400).unwrap();
        assert!(
            joined.get_pixel(248, 150)[0] > 240,
            "Internal border must disappear"
        );
        // Paint the first bubble last, so its otherwise hidden internal outline is measurable.
        chapter.layers.reverse();
        for l in &mut chapter.layers {
            if let LayerKind::Text { style, .. } = &mut l.kind {
                style.cluster_id = None;
            }
        }
        let detached =
            compositor::render_region(&chapter, &AssetBytes::new(), 0, 400, 400).unwrap();
        assert!(
            detached.get_pixel(248, 150)[0] < 80,
            "Detached border must return"
        );
    }
    #[test]
    fn adding_twice_from_the_same_bubble_does_not_stack_dialogue() {
        let mut story = Story::new("Chain");
        let id = story.alloc_id();
        story.chapters[0].layers.push(dialogue(id));
        add_cluster_bubble(&mut story, 0, id).unwrap();
        add_cluster_bubble(&mut story, 0, id).unwrap();
        let a = &story.chapters[0].layers[1];
        let b = &story.chapters[0].layers[2];
        assert!((a.y - b.y).abs() > a.height * 0.5);
    }
}
