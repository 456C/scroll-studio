//! Small, geometry-only magnetic alignment for the editor. Guides are transient
//! UI data; they never enter the story or its exported artwork.

use crate::model::{Layer, LayerKind};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Bounds {
    pub fn right(self) -> f32 {
        self.x + self.width
    }

    pub fn bottom(self) -> f32 {
        self.y + self.height
    }

    pub fn center(self) -> [f32; 2] {
        [self.x + self.width * 0.5, self.y + self.height * 0.5]
    }

    pub fn translated(self, offset: [f32; 2]) -> Self {
        Self {
            x: self.x + offset[0],
            y: self.y + offset[1],
            ..self
        }
    }

    pub fn is_valid(self) -> bool {
        self.x.is_finite()
            && self.y.is_finite()
            && self.width.is_finite()
            && self.height.is_finite()
            && self.width > 0.0
            && self.height > 0.0
            && self.right().is_finite()
            && self.bottom().is_finite()
    }

    fn range(self, axis: Axis) -> [f32; 2] {
        match axis {
            Axis::X => [self.x, self.right()],
            Axis::Y => [self.y, self.bottom()],
        }
    }

    fn anchors(self, axis: Axis) -> [f32; 3] {
        let [start, end] = self.range(axis);
        [start, start + (end - start) * 0.5, end]
    }
}

/// Bounds of the visible geometry, without decoding source image pixels.
/// Visibility, locking and exclusion of the moving object are the caller's
/// responsibility. Locked objects can still be useful alignment references.
pub fn layer_bounds(layer: &Layer) -> Option<Bounds> {
    let body = Bounds {
        x: layer.x,
        y: layer.y,
        width: layer.width,
        height: layer.height,
    };
    if !body.is_valid() {
        return None;
    }
    let bounds = match &layer.kind {
        LayerKind::Image {
            clip: Some(clip),
            source_rect,
            ..
        } => {
            let [x, y, w, h] = clip.selection.edge_bounds(clip.edge)?;
            let sw = clip.selection.width as f32;
            let sh = clip.selection.height as f32;
            let [cx, cy, cw, ch] = source_rect.unwrap_or([0.0, 0.0, 1.0, 1.0]);
            let left = (x as f32 / sw).max(cx);
            let top = (y as f32 / sh).max(cy);
            let right = ((x + w) as f32 / sw).min(cx + cw);
            let bottom = ((y + h) as f32 / sh).min(cy + ch);
            if right <= left || bottom <= top {
                return None;
            }
            Bounds {
                x: layer.x + (left - cx) / cw * layer.width,
                y: layer.y + (top - cy) / ch * layer.height,
                width: (right - left) / cw * layer.width,
                height: (bottom - top) / ch * layer.height,
            }
        }
        LayerKind::Image { mask, .. } if !mask.is_empty() => {
            if mask.len() < 3 || mask.iter().flatten().any(|point| !point.is_finite()) {
                return None;
            }
            let clipped = crate::compositor::clip_polygon_to_unit_square(mask);
            let [min, max] = point_extents(&clipped)?;
            Bounds {
                x: layer.x + min[0] * layer.width,
                y: layer.y + min[1] * layer.height,
                width: (max[0] - min[0]) * layer.width,
                height: (max[1] - min[1]) * layer.height,
            }
        }
        LayerKind::Stroke { points, size, .. } => {
            if !size.is_finite() || *size <= 0.0 {
                return None;
            }
            let [min, max] = point_extents(points)?;
            let radius = size * 0.5;
            Bounds {
                x: layer.x + min[0] - radius,
                y: layer.y + min[1] - radius,
                width: max[0] - min[0] + size,
                height: max[1] - min[1] + size,
            }
        }
        // Speech pointers intentionally do not change the body's alignment.
        _ => body,
    };
    bounds.is_valid().then_some(bounds)
}

fn point_extents(points: &[[f32; 2]]) -> Option<[[f32; 2]; 2]> {
    let first = *points.first()?;
    let mut min = first;
    let mut max = first;
    for point in points {
        for axis in 0..2 {
            if !point[axis].is_finite() {
                return None;
            }
            min[axis] = min[axis].min(point[axis]);
            max[axis] = max[axis].max(point[axis]);
        }
    }
    Some([min, max])
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SnapTarget {
    pub id: u64,
    pub bounds: Bounds,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SnapSettings {
    pub enabled: bool,
    pub canvas: bool,
    pub objects: bool,
    pub spacing: bool,
    /// Logical screen points, independent of canvas zoom. Capped at 24 points.
    pub threshold_px: f32,
}

impl Default for SnapSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            canvas: true,
            objects: true,
            spacing: true,
            threshold_px: 8.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
}

impl Axis {
    fn other(self) -> Self {
        match self {
            Self::X => Self::Y,
            Self::Y => Self::X,
        }
    }

    fn index(self) -> usize {
        match self {
            Self::X => 0,
            Self::Y => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Guide {
    /// X: vertical x=position, from/to are y. Y: horizontal y=position,
    /// from/to are x. Canvas guides span the canvas; the UI clips them.
    Alignment {
        axis: Axis,
        position: f32,
        from: f32,
        to: f32,
    },
    /// X: horizontal distance from x=start to x=end at y=cross.
    /// Y: vertical distance from y=start to y=end at x=cross.
    Spacing {
        axis: Axis,
        start: f32,
        end: f32,
        cross: f32,
    },
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SnapResult {
    /// Add this to the raw pointer-derived layer position. Never feed a prior
    /// snapped position back in as the next raw position: that would stick.
    pub offset: [f32; 2],
    pub guides: Vec<Guide>,
}

#[derive(Clone, Copy)]
enum GuideSeed {
    Alignment { position: f32, reference: Bounds },
    Spacing { gaps: [[f32; 2]; 2] },
}

#[derive(Clone, Copy)]
struct Candidate {
    offset: f32,
    priority: u8,
    ids: [u64; 2],
    seed: GuideSeed,
}

fn offer(best: &mut Option<Candidate>, candidate: Candidate, threshold: f32) {
    if !candidate.offset.is_finite() || candidate.offset.abs() > threshold {
        return;
    }
    let preferred = best.is_none_or(|current| {
        candidate
            .offset
            .abs()
            .total_cmp(&current.offset.abs())
            .then_with(|| candidate.priority.cmp(&current.priority))
            .then_with(|| candidate.ids.cmp(&current.ids))
            .then_with(|| candidate.offset.total_cmp(&current.offset))
            .is_lt()
    });
    if preferred {
        *best = Some(candidate);
    }
}

/// Resolve each translation axis independently against a cached set of targets.
/// Work is linear in target count and does not depend on chapter pixel size.
/// Targets should be the visible objects near the viewport, excluding the
/// dragged object. The caller can retain them for the duration of a gesture.
pub fn snap_translation(
    moving: Bounds,
    canvas: Bounds,
    targets: &[SnapTarget],
    zoom: f32,
    settings: &SnapSettings,
) -> SnapResult {
    if !settings.enabled
        || !moving.is_valid()
        || !zoom.is_finite()
        || zoom <= 0.0
        || !settings.threshold_px.is_finite()
        || settings.threshold_px <= 0.0
    {
        return SnapResult::default();
    }
    let threshold = settings.threshold_px.min(24.0) / zoom;
    if !threshold.is_finite() {
        return SnapResult::default();
    }
    let mut best = [None, None];
    for axis in [Axis::X, Axis::Y] {
        let selected = &mut best[axis.index()];
        if settings.canvas && canvas.is_valid() {
            align_candidates(selected, moving, canvas, axis, 0, true, threshold);
        }
        if settings.objects {
            for target in targets.iter().filter(|target| target.bounds.is_valid()) {
                align_candidates(
                    selected,
                    moving,
                    target.bounds,
                    axis,
                    target.id,
                    false,
                    threshold,
                );
            }
        }
        if settings.spacing {
            spacing_candidates(selected, moving, targets, axis, threshold);
        }
    }
    let offset = [
        best[0].map_or(0.0, |c| c.offset),
        best[1].map_or(0.0, |c| c.offset),
    ];
    let snapped = moving.translated(offset);
    if !snapped.is_valid() {
        return SnapResult::default();
    }
    let mut guides = Vec::with_capacity(4);
    for axis in [Axis::X, Axis::Y] {
        if let Some(candidate) = best[axis.index()] {
            match candidate.seed {
                GuideSeed::Alignment {
                    position,
                    reference,
                } => {
                    let moving_span = snapped.range(axis.other());
                    let target_span = reference.range(axis.other());
                    guides.push(Guide::Alignment {
                        axis,
                        position,
                        from: moving_span[0].min(target_span[0]),
                        to: moving_span[1].max(target_span[1]),
                    });
                }
                GuideSeed::Spacing { gaps } => {
                    let cross = snapped.center()[axis.other().index()];
                    for [start, end] in gaps {
                        guides.push(Guide::Spacing {
                            axis,
                            start,
                            end,
                            cross,
                        });
                    }
                }
            }
        }
    }
    SnapResult { offset, guides }
}

fn align_candidates(
    best: &mut Option<Candidate>,
    moving: Bounds,
    reference: Bounds,
    axis: Axis,
    id: u64,
    canvas: bool,
    threshold: f32,
) {
    for (moving_index, anchor) in moving.anchors(axis).into_iter().enumerate() {
        for (target_index, position) in reference.anchors(axis).into_iter().enumerate() {
            // Centering a canvas means center-to-center. Its edges remain useful
            // on either side, including when moving an object partly off-canvas.
            if canvas && (moving_index == 1) != (target_index == 1) {
                continue;
            }
            let priority = match (canvas, moving_index == target_index, moving_index == 1) {
                (true, true, true) => 0,
                (true, _, _) => 1,
                (false, true, true) => 2,
                (false, true, false) => 3,
                (false, false, _) => 4,
            };
            offer(
                best,
                Candidate {
                    offset: position - anchor,
                    priority,
                    ids: [id, 0],
                    seed: GuideSeed::Alignment {
                        position,
                        reference,
                    },
                },
                threshold,
            );
        }
    }
}

fn overlaps_cross_axis(a: Bounds, b: Bounds, axis: Axis) -> bool {
    let a = a.range(axis.other());
    let b = b.range(axis.other());
    a[0] < b[1] && b[0] < a[1]
}

/// Find the immediate non-overlapping neighbor in one direction. Stable IDs
/// break ties, so changing layer stacking order does not move the magnet.
fn neighbor(
    moving: Bounds,
    targets: &[SnapTarget],
    axis: Axis,
    before: bool,
    boundary: f32,
    excluded: Option<u64>,
) -> Option<SnapTarget> {
    targets
        .iter()
        .copied()
        .filter(|target| {
            if Some(target.id) == excluded
                || !target.bounds.is_valid()
                || !overlaps_cross_axis(moving, target.bounds, axis)
            {
                return false;
            }
            let span = target.bounds.range(axis);
            if before {
                span[1] <= boundary
            } else {
                span[0] >= boundary
            }
        })
        .min_by(|a, b| {
            let a_span = a.bounds.range(axis);
            let b_span = b.bounds.range(axis);
            let ordering = if before {
                b_span[1].total_cmp(&a_span[1])
            } else {
                a_span[0].total_cmp(&b_span[0])
            };
            ordering.then_with(|| a.id.cmp(&b.id))
        })
}

fn spacing_candidates(
    best: &mut Option<Candidate>,
    moving: Bounds,
    targets: &[SnapTarget],
    axis: Axis,
    threshold: f32,
) {
    let [start, end] = moving.range(axis);
    let length = end - start;
    let before = neighbor(moving, targets, axis, true, start, None);
    let after = neighbor(moving, targets, axis, false, end, None);
    if let (Some(before), Some(after)) = (before, after) {
        let gap_start = before.bounds.range(axis)[1];
        let gap_end = after.bounds.range(axis)[0];
        let free = gap_end - gap_start - length;
        if free.is_finite() && free >= 0.0 {
            let snapped_start = gap_start + free * 0.5;
            offer(
                best,
                Candidate {
                    offset: snapped_start - start,
                    priority: 5,
                    ids: [before.id, after.id],
                    seed: GuideSeed::Spacing {
                        gaps: [
                            [gap_start, snapped_start],
                            [snapped_start + length, gap_end],
                        ],
                    },
                },
                threshold,
            );
        }
    }
    if let Some(before) = before {
        let [near_start, near_end] = before.bounds.range(axis);
        if let Some(previous) = neighbor(moving, targets, axis, true, near_start, Some(before.id)) {
            let previous_end = previous.bounds.range(axis)[1];
            let gap = near_start - previous_end;
            let snapped_start = near_end + gap;
            offer(
                best,
                Candidate {
                    offset: snapped_start - start,
                    priority: 6,
                    ids: [previous.id, before.id],
                    seed: GuideSeed::Spacing {
                        gaps: [[previous_end, near_start], [near_end, snapped_start]],
                    },
                },
                threshold,
            );
        }
    }
    if let Some(after) = after {
        let [near_start, near_end] = after.bounds.range(axis);
        if let Some(next) = neighbor(moving, targets, axis, false, near_end, Some(after.id)) {
            let next_start = next.bounds.range(axis)[0];
            let gap = next_start - near_end;
            let snapped_end = near_start - gap;
            offer(
                best,
                Candidate {
                    offset: snapped_end - end,
                    priority: 6,
                    ids: [after.id, next.id],
                    seed: GuideSeed::Spacing {
                        gaps: [[snapped_end, near_start], [near_end, next_start]],
                    },
                },
                threshold,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::TextStyle;

    fn bounds(x: f32, y: f32, width: f32, height: f32) -> Bounds {
        Bounds {
            x,
            y,
            width,
            height,
        }
    }

    fn canvas() -> Bounds {
        bounds(0.0, 0.0, 800.0, 4000.0)
    }

    fn targets(boxes: &[Bounds]) -> Vec<SnapTarget> {
        boxes
            .iter()
            .copied()
            .enumerate()
            .map(|(index, bounds)| SnapTarget {
                id: index as u64 + 1,
                bounds,
            })
            .collect()
    }

    fn spacing_only() -> SnapSettings {
        SnapSettings {
            canvas: false,
            objects: false,
            ..Default::default()
        }
    }

    #[test]
    fn canvas_centers_and_edges_work_on_both_axes_without_integer_rounding() {
        let centered = snap_translation(
            bounds(294.5, 1944.0, 200.0, 100.0),
            canvas(),
            &[],
            1.0,
            &SnapSettings::default(),
        );
        assert_eq!(centered.offset, [5.5, 6.0]);
        assert_eq!(centered.guides.len(), 2);
        assert!(centered.guides.contains(&Guide::Alignment {
            axis: Axis::X,
            position: 400.0,
            from: 0.0,
            to: 4000.0
        }));
        let edge = snap_translation(
            bounds(593.0, -4.0, 200.0, 100.0),
            canvas(),
            &[],
            1.0,
            &SnapSettings::default(),
        );
        assert_eq!(edge.offset, [7.0, 4.0]);
    }

    #[test]
    fn sensitivity_is_screen_space_and_excessive_values_are_bounded() {
        for zoom in [0.1, 0.5, 1.0, 1.5, 4.0] {
            let moving = bounds(300.0 - 7.0 / zoom, 500.0, 200.0, 100.0);
            assert!(
                (snap_translation(moving, canvas(), &[], zoom, &SnapSettings::default()).offset[0]
                    * zoom
                    - 7.0)
                    .abs()
                    < 0.001
            );
            let moving = bounds(300.0 - 9.0 / zoom, 500.0, 200.0, 100.0);
            assert_eq!(
                snap_translation(moving, canvas(), &[], zoom, &SnapSettings::default()).offset,
                [0.0, 0.0]
            );
        }
        let settings = SnapSettings {
            threshold_px: 1000.0,
            ..Default::default()
        };
        assert_eq!(
            snap_translation(
                bounds(270.0, 500.0, 200.0, 100.0),
                canvas(),
                &[],
                1.0,
                &settings
            )
            .offset,
            [0.0, 0.0]
        );
    }

    #[test]
    fn object_edges_centers_and_ties_are_independent_of_stacking_order() {
        let settings = SnapSettings {
            canvas: false,
            spacing: false,
            ..Default::default()
        };
        let mut references = targets(&[
            bounds(-40.0, 100.0, 120.0, 100.0),
            bounds(-30.0, 400.0, 120.0, 100.0),
        ]);
        let moving = bounds(-35.0, 205.0, 120.0, 100.0);
        let result = snap_translation(moving, canvas(), &references, 1.0, &settings);
        assert_eq!(result.offset, [-5.0, -5.0]);
        references.reverse();
        assert_eq!(
            result,
            snap_translation(moving, canvas(), &references, 1.0, &settings)
        );
        assert!(moving.translated(result.offset).x < 0.0);
    }

    #[test]
    fn equal_spacing_between_neighbors_and_repeated_gaps_in_both_directions() {
        let references = targets(&[
            bounds(100.0, 100.0, 200.0, 100.0),
            bounds(100.0, 400.0, 200.0, 100.0),
        ]);
        let result = snap_translation(
            bounds(100.0, 256.0, 200.0, 100.0),
            canvas(),
            &references,
            1.0,
            &spacing_only(),
        );
        assert_eq!(result.offset, [0.0, -6.0]);
        assert_eq!(
            result.guides,
            vec![
                Guide::Spacing {
                    axis: Axis::Y,
                    start: 200.0,
                    end: 250.0,
                    cross: 200.0
                },
                Guide::Spacing {
                    axis: Axis::Y,
                    start: 350.0,
                    end: 400.0,
                    cross: 200.0
                },
            ]
        );
        let result = snap_translation(
            bounds(100.0, 694.0, 200.0, 100.0),
            canvas(),
            &references,
            1.0,
            &spacing_only(),
        );
        assert_eq!(result.offset, [0.0, 6.0]);
        let result = snap_translation(
            bounds(100.0, -194.0, 200.0, 100.0),
            canvas(),
            &references,
            1.0,
            &spacing_only(),
        );
        assert_eq!(result.offset, [0.0, -6.0]);
        let horizontal = targets(&[
            bounds(100.0, 100.0, 100.0, 200.0),
            bounds(400.0, 100.0, 100.0, 200.0),
        ]);
        assert_eq!(
            snap_translation(
                bounds(256.0, 100.0, 100.0, 200.0),
                canvas(),
                &horizontal,
                1.0,
                &spacing_only()
            )
            .offset,
            [-6.0, 0.0]
        );
    }

    #[test]
    fn spacing_uses_nearest_non_overlapping_neighbors_and_a_common_column() {
        let references = targets(&[
            bounds(100.0, 100.0, 200.0, 100.0),
            bounds(100.0, 400.0, 200.0, 100.0),
            bounds(600.0, 280.0, 100.0, 100.0),
        ]);
        assert_eq!(
            snap_translation(
                bounds(100.0, 256.0, 200.0, 100.0),
                canvas(),
                &references,
                1.0,
                &spacing_only()
            )
            .offset[1],
            -6.0
        );
        assert!(snap_translation(
            bounds(400.0, 256.0, 100.0, 100.0),
            canvas(),
            &references,
            1.0,
            &spacing_only()
        )
        .guides
        .is_empty());
    }

    #[test]
    fn disabled_targets_invalid_geometry_and_invalid_scales_do_not_snap() {
        let moving = bounds(294.0, 500.0, 200.0, 100.0);
        let disabled = SnapSettings {
            enabled: false,
            ..Default::default()
        };
        assert_eq!(
            snap_translation(moving, canvas(), &[], 1.0, &disabled),
            SnapResult::default()
        );
        let disabled = SnapSettings {
            canvas: false,
            objects: false,
            spacing: false,
            ..Default::default()
        };
        assert_eq!(
            snap_translation(moving, canvas(), &targets(&[moving]), 1.0, &disabled),
            SnapResult::default()
        );
        for zoom in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            assert_eq!(
                snap_translation(moving, canvas(), &[], zoom, &SnapSettings::default()),
                SnapResult::default()
            );
        }
        let malformed = bounds(f32::NAN, 0.0, 100.0, 100.0);
        assert_eq!(
            snap_translation(malformed, canvas(), &[], 1.0, &SnapSettings::default()),
            SnapResult::default()
        );
        let no_canvas = SnapSettings {
            canvas: false,
            ..Default::default()
        };
        assert_eq!(
            snap_translation(moving, canvas(), &targets(&[malformed]), 1.0, &no_canvas),
            SnapResult::default()
        );
    }

    fn layer(kind: LayerKind) -> Layer {
        Layer {
            id: 1,
            name: String::new(),
            visible: true,
            locked: false,
            opacity: 1.0,
            x: 100.0,
            y: 200.0,
            width: 400.0,
            height: 600.0,
            kind,
        }
    }

    #[test]
    fn mask_geometry_is_clipped_before_bounding_and_empty_clips_are_ignored() {
        let image = layer(LayerKind::Image {
            source_rect: None,
            clip: None,
            asset_id: 2,
            mask: vec![[0.25, 0.1], [0.75, 0.1], [0.75, 0.9], [0.25, 0.9]],
        });
        let actual = layer_bounds(&image).unwrap();
        assert_eq!([actual.x, actual.y, actual.width], [200.0, 260.0, 200.0]);
        assert!((actual.height - 480.0).abs() < 0.001);
        let outside = layer(LayerKind::Image {
            source_rect: None,
            clip: None,
            asset_id: 2,
            mask: vec![[2.0, 0.5], [0.5, 2.0], [2.0, 2.0]],
        });
        assert!(layer_bounds(&outside).is_none());
        let crossing = layer(LayerKind::Image {
            source_rect: None,
            clip: None,
            asset_id: 2,
            mask: vec![[-1.0, 0.25], [0.5, 0.25], [0.5, 0.75], [-1.0, 0.75]],
        });
        assert_eq!(
            layer_bounds(&crossing),
            Some(bounds(100.0, 350.0, 200.0, 300.0))
        );
    }

    #[test]
    fn stroke_bounds_use_points_radius_and_move_offset_while_bubbles_ignore_tails() {
        let stroke = layer(LayerKind::Stroke {
            points: vec![[30.0, 40.0], [90.0, 110.0]],
            color: [0; 4],
            size: 10.0,
        });
        assert_eq!(
            layer_bounds(&stroke),
            Some(bounds(125.0, 235.0, 70.0, 80.0))
        );
        let mut style = TextStyle::dialogue();
        style.tail.tip = [-10.0, 50.0];
        let bubble = layer(LayerKind::Text {
            text: "Hello".into(),
            color: [0; 4],
            size: 32.0,
            bubble: true,
            style,
        });
        assert_eq!(
            layer_bounds(&bubble),
            Some(bounds(100.0, 200.0, 400.0, 600.0))
        );
    }

    #[test]
    fn raw_pointer_positions_release_the_magnet_without_accumulating_offset() {
        let settings = SnapSettings::default();
        for raw_x in [294.0, 297.0, 302.0, 307.0] {
            let raw = bounds(raw_x, 600.0, 200.0, 100.0);
            assert_eq!(
                raw.translated(snap_translation(raw, canvas(), &[], 1.0, &settings).offset)
                    .x,
                300.0
            );
        }
        let outside = bounds(310.0, 600.0, 200.0, 100.0);
        assert_eq!(
            snap_translation(outside, canvas(), &[], 1.0, &settings).offset,
            [0.0, 0.0]
        );
    }
}
