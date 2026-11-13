//! Atomic vertical pacing edits. Image bytes, masks and object dimensions stay intact.
use crate::{compositor, model::*, snapping};
use anyhow::{ensure, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpaceMode {
    Insert,
    Remove,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Crossing {
    Keep,
    Move,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpaceRequest {
    pub at: u32,
    pub amount: u32,
    pub mode: SpaceMode,
    pub crossing: Crossing,
}

impl SpaceRequest {
    pub fn delta(self) -> f32 {
        match self.mode {
            SpaceMode::Insert => self.amount as f32,
            SpaceMode::Remove => -(self.amount as f32),
        }
    }

    /// Map independent speaker targets and script range endpoints through the gap.
    fn map_y(self, y: f32, end: bool) -> f32 {
        let at = self.at as f32;
        match self.mode {
            SpaceMode::Insert if y > at || (!end && y == at) => y + self.delta(),
            SpaceMode::Insert => y,
            SpaceMode::Remove if y <= at => y,
            SpaceMode::Remove => (y - self.amount as f32).max(at),
        }
    }
}

pub struct PreparedSpacing {
    pub chapter: Chapter,
    pub moved_objects: usize,
    pub changed_beats: usize,
    pub crossing_objects: usize,
    pub locked_objects: usize,
    pub newly_clipped: usize,
}

/// Build a private preview without changing the source. The caller must obtain
/// an explicit choice before applying a plan with `locked_objects > 0`.
pub fn prepare(chapter: &Chapter, request: SpaceRequest) -> Result<PreparedSpacing> {
    ensure!(
        (64..=200_000).contains(&chapter.height),
        "Invalid chapter height"
    );
    ensure!(
        request.at <= chapter.height,
        "Position is outside the chapter"
    );
    ensure!(request.amount > 0, "Choose an amount of space");
    let height = match request.mode {
        SpaceMode::Insert => chapter
            .height
            .checked_add(request.amount)
            .filter(|height| *height <= 200_000),
        SpaceMode::Remove => {
            ensure!(
                request
                    .at
                    .checked_add(request.amount)
                    .is_some_and(|end| end <= chapter.height),
                "The removed strip extends beyond the chapter"
            );
            chapter
                .height
                .checked_sub(request.amount)
                .filter(|height| *height >= 64)
        }
    }
    .ok_or_else(|| anyhow::anyhow!("Keep the chapter between 64 and 200,000 px tall"))?;
    let mut result = PreparedSpacing {
        chapter: chapter.clone(),
        moved_objects: 0,
        changed_beats: 0,
        crossing_objects: 0,
        locked_objects: 0,
        newly_clipped: 0,
    };
    result.chapter.height = height;
    let at = request.at as f32;
    let below = if request.mode == SpaceMode::Remove {
        at + request.amount as f32
    } else {
        at
    };
    for (source, layer) in chapter.layers.iter().zip(&mut result.chapter.layers) {
        let body = snapping::layer_bounds(source).unwrap_or(snapping::Bounds {
            x: source.x,
            y: source.y,
            width: source.width,
            height: source.height,
        });
        ensure!(
            body.y.is_finite() && body.height.is_finite() && body.height > 0.0,
            "Invalid object geometry: {}",
            layer.name
        );
        let crossing = body.y < below && body.bottom() > at;
        result.crossing_objects += usize::from(crossing);
        let shift = body.y >= below || (crossing && request.crossing == Crossing::Move);
        if shift {
            layer.y += request.delta();
        }
        ensure!(
            layer.y.is_finite() && layer.y.abs() <= 10_000_000.0,
            "Position limit reached for {}",
            layer.name
        );
        let mut changed = shift;
        if let LayerKind::Text {
            style,
            bubble: true,
            ..
        } = &mut layer.kind
        {
            if style.shape != BubbleShape::Narration {
                for tail in style.tails_mut().filter(|tail| tail.enabled) {
                    let target = source.y + tail.tip[1] * source.height;
                    let target_shift = request.map_y(target, false) - target;
                    let body_shift = layer.y - source.y;
                    if target_shift == body_shift {
                        continue;
                    }
                    let tip = tail.tip[1] + (target_shift - body_shift) / source.height;
                    let relative = tip * source.height;
                    ensure!(
                        relative.is_finite() && relative.abs() <= MAX_POINTER_OFFSET,
                        "Pointer position limit reached for {}",
                        layer.name
                    );
                    changed |= tip != tail.tip[1];
                    tail.tip[1] = tip;
                }
            }
        }
        if changed {
            result.moved_objects += 1;
            result.locked_objects += usize::from(layer.locked);
        }
        let vertical_bounds = |layer: &Layer| {
            if matches!(layer.kind, LayerKind::Text { .. }) {
                let [_, y, _, h] = compositor::text_bounds(layer);
                [y, y + h]
            } else if let Some(bounds) = snapping::layer_bounds(layer) {
                [bounds.y, bounds.bottom()]
            } else {
                [layer.y, layer.y + layer.height]
            }
        };
        let old = vertical_bounds(source);
        let new = vertical_bounds(layer);
        let old_clip = (-old[0]).max(0.0) + (old[1] - chapter.height as f32).max(0.0);
        let new_clip = (-new[0]).max(0.0) + (new[1] - height as f32).max(0.0);
        result.newly_clipped += usize::from(new_clip > old_clip + 0.01);
    }
    for panel in &mut result.chapter.panels {
        let y = request.map_y(panel.y, false);
        let end = request.map_y(panel.y + panel.height, true);
        // Removing a whole beat's range never deletes its script or notes.
        let height = (end - y).max(1.0);
        ensure!(
            y.is_finite()
                && y.abs() <= 10_000_000.0
                && height.is_finite()
                && height <= 10_000_000.0,
            "Invalid script beat range"
        );
        if y != panel.y || height != panel.height {
            panel.y = y;
            panel.height = height;
            result.changed_beats += 1;
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shape(id: u64, y: f32, height: f32) -> Layer {
        Layer {
            extensions: Default::default(),
            id,
            name: format!("Object {id}"),
            x: 20.0,
            y,
            width: 100.0,
            height,
            visible: true,
            locked: false,
            opacity: 1.0,
            kind: LayerKind::Shape {
                color: [20, 30, 40, 255],
            },
        }
    }

    fn request(mode: SpaceMode) -> SpaceRequest {
        SpaceRequest {
            at: 500,
            amount: 200,
            mode,
            crossing: Crossing::Keep,
        }
    }

    #[test]
    fn inserting_space_moves_lower_art_and_scripts_as_an_atomic_preview() {
        let mut chapter = Chapter::new(1, "Pacing", 800, 2400);
        chapter.layers = vec![
            shape(2, 100.0, 200.0),
            shape(3, 450.0, 100.0),
            shape(4, 700.0, 200.0),
        ];
        chapter.layers[2].visible = false;
        chapter.panels = vec![
            Panel {
                id: 5,
                title: "Before".into(),
                y: 100.0,
                height: 400.0,
                script: "Keep every word.".into(),
                notes: "Keep the notes.".into(),
                prompt: "Keep the prompt.".into(),
            },
            Panel {
                id: 6,
                title: "Across".into(),
                y: 400.0,
                height: 400.0,
                script: String::new(),
                notes: String::new(),
                prompt: String::new(),
            },
        ];
        let before = chapter.clone();
        let result = prepare(&chapter, request(SpaceMode::Insert)).unwrap();
        assert_eq!(chapter, before);
        assert_eq!(result.chapter.height, 2600);
        assert_eq!(
            result
                .chapter
                .layers
                .iter()
                .map(|l| l.y)
                .collect::<Vec<_>>(),
            [100.0, 450.0, 900.0]
        );
        assert_eq!((result.moved_objects, result.crossing_objects), (1, 1));
        assert_eq!(result.chapter.panels[0], chapter.panels[0]);
        assert_eq!(result.chapter.panels[1].height, 600.0);
        let moved = prepare(
            &chapter,
            SpaceRequest {
                crossing: Crossing::Move,
                ..request(SpaceMode::Insert)
            },
        )
        .unwrap();
        assert_eq!(moved.chapter.layers[1].y, 650.0);
    }

    #[test]
    fn removing_space_preserves_art_masks_lock_flags_and_collapsed_script_text() {
        let mut chapter = Chapter::new(1, "Pacing", 800, 2400);
        let mut image = shape(2, 600.0, 1000.0);
        image.kind = LayerKind::Image {
            source_rect: None,
            clip: None,
            asset_id: 10,
            mask: vec![[0.0, 0.2], [1.0, 0.2], [1.0, 1.0], [0.0, 1.0]],
        };
        image.locked = true;
        let mut stroke = shape(3, 100.0, 10.0);
        stroke.kind = LayerKind::Stroke {
            points: vec![[10.0, 900.0], [20.0, 920.0]],
            color: [0, 0, 0, 255],
            size: 4.0,
        };
        chapter.layers = vec![image, stroke, shape(4, 400.0, 200.0)];
        chapter.panels.push(Panel {
            id: 5,
            title: "Inside".into(),
            y: 520.0,
            height: 100.0,
            script: "The script survives.".into(),
            notes: "Notes".into(),
            prompt: "Prompt".into(),
        });
        let result = prepare(&chapter, request(SpaceMode::Remove)).unwrap();
        assert_eq!(result.chapter.height, 2200);
        assert_eq!(result.locked_objects, 1);
        assert_eq!(
            result
                .chapter
                .layers
                .iter()
                .map(|l| l.y)
                .collect::<Vec<_>>(),
            [400.0, -100.0, 400.0]
        );
        for (source, changed) in chapter.layers.iter().zip(&result.chapter.layers) {
            assert_eq!(source.kind, changed.kind);
            assert_eq!(source.locked, changed.locked);
            assert_eq!(
                (source.width, source.height),
                (changed.width, changed.height)
            );
        }
        assert_eq!(
            (result.chapter.panels[0].y, result.chapter.panels[0].height),
            (500.0, 1.0)
        );
        assert_eq!(result.chapter.panels[0].script, chapter.panels[0].script);
    }

    #[test]
    fn independent_pointers_follow_speakers_on_both_sides_of_the_gap() {
        let mut chapter = Chapter::new(1, "Pointers", 800, 2400);
        let mut bubble = shape(2, 200.0, 100.0);
        bubble.kind = LayerKind::Text {
            text: "Wait.".into(),
            color: [0, 0, 0, 255],
            size: 38.0,
            bubble: true,
            style: TextStyle {
                tail: BubbleTail {
                    enabled: true,
                    tip: [-0.5, 8.0],
                    ..Default::default()
                },
                extra_tails: vec![BubbleTail {
                    enabled: true,
                    tip: [1.5, -1.0],
                    ..Default::default()
                }],
                ..TextStyle::dialogue()
            },
        };
        chapter.layers.push(bubble);
        for (mode, expected) in [(SpaceMode::Insert, 1200.0), (SpaceMode::Remove, 800.0)] {
            let result = prepare(&chapter, request(mode)).unwrap();
            let layer = &result.chapter.layers[0];
            let LayerKind::Text { style, .. } = &layer.kind else {
                panic!();
            };
            assert_eq!(layer.y, 200.0);
            assert_eq!(layer.y + style.tail.tip[1] * layer.height, expected);
            assert_eq!(style.tail.tip[0], -0.5);
            assert_eq!(style.extra_tails[0].tip, [1.5, -1.0]);
        }
    }

    #[test]
    fn invalid_limits_fail_without_changes_and_new_clipping_is_reported() {
        let mut chapter = Chapter::new(1, "Bounds", 800, 2400);
        chapter.layers.push(shape(2, 0.0, 2400.0));
        let original = chapter.clone();
        for invalid in [
            SpaceRequest {
                amount: 0,
                ..request(SpaceMode::Insert)
            },
            SpaceRequest {
                amount: u32::MAX,
                ..request(SpaceMode::Insert)
            },
            SpaceRequest {
                amount: 2400,
                ..request(SpaceMode::Remove)
            },
            SpaceRequest {
                at: 2300,
                ..request(SpaceMode::Remove)
            },
        ] {
            assert!(prepare(&chapter, invalid).is_err());
            assert_eq!(chapter, original);
        }
        let result = prepare(&chapter, request(SpaceMode::Remove)).unwrap();
        assert_eq!(result.newly_clipped, 1);
        assert_eq!(result.chapter.layers[0], chapter.layers[0]);
    }

    #[test]
    fn stationary_fractional_pointers_do_not_touch_locked_objects() {
        let mut chapter = Chapter::new(1, "Stationary", 800, 2400);
        let mut bubble = shape(2, 123.45, 77.13);
        bubble.locked = true;
        bubble.kind = LayerKind::Text {
            text: "Wait.".into(),
            color: [0, 0, 0, 255],
            size: 38.0,
            bubble: true,
            style: TextStyle {
                tail: BubbleTail {
                    enabled: true,
                    tip: [0.21, 1.23],
                    ..Default::default()
                },
                ..TextStyle::dialogue()
            },
        };
        chapter.layers.push(bubble);
        let plan = prepare(&chapter, request(SpaceMode::Insert)).unwrap();
        assert_eq!(plan.chapter.layers, chapter.layers);
        assert_eq!((plan.locked_objects, plan.moved_objects), (0, 0));
    }

    #[test]
    fn moving_bubbles_keep_targets_above_the_cut_and_relative_targets_below() {
        let mut chapter = Chapter::new(1, "Moving pointers", 800, 2400);
        let mut bubble = shape(2, 1000.0, 200.0);
        bubble.kind = LayerKind::Text {
            text: "Up here.".into(),
            color: [0, 0, 0, 255],
            size: 38.0,
            bubble: true,
            style: TextStyle {
                tail: BubbleTail {
                    enabled: true,
                    tip: [-0.5, -3.0],
                    ..Default::default()
                },
                extra_tails: vec![
                    BubbleTail {
                        enabled: true,
                        tip: [0.8, 2.0],
                        ..Default::default()
                    },
                    BubbleTail {
                        enabled: false,
                        tip: [1.2, -3.0],
                        ..Default::default()
                    },
                ],
                ..TextStyle::dialogue()
            },
        };
        chapter.layers.push(bubble);
        for mode in [SpaceMode::Insert, SpaceMode::Remove] {
            let plan = prepare(&chapter, request(mode)).unwrap();
            let layer = &plan.chapter.layers[0];
            let LayerKind::Text { style, .. } = &layer.kind else {
                panic!();
            };
            assert_eq!(layer.y + style.tail.tip[1] * layer.height, 400.0);
            assert_eq!(style.tail.tip[0], -0.5);
            assert_eq!(style.extra_tails[0].tip, [0.8, 2.0]);
            assert_eq!(style.extra_tails[1].tip, [1.2, -3.0]);
        }
    }
}
