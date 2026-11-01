//! Shared lettering geometry and rasterization. Layout is in chapter coordinates,
//! so preview zoom and export slice boundaries cannot change line wrapping.
use std::{
    borrow::Cow,
    f32::consts::{PI, TAU},
    sync::OnceLock,
};

use ab_glyph::{point, Font, FontArc, ScaleFont};
use anyhow::{bail, Result};
use image::RgbaImage;

use crate::model::{BubbleShape, BubbleTail, FontChoice, Layer, LayerKind, TextAlign, TextStyle};

type Point = [f32; 2];

pub fn font_bytes(font: FontChoice) -> &'static [u8] {
    match font {
        FontChoice::Lato => include_bytes!("../../assets/Lato-Regular.ttf"),
        FontChoice::BalsamiqSans => include_bytes!("../../assets/BalsamiqSans-Regular.ttf"),
        FontChoice::KomikaHand => include_bytes!("../../assets/KomikaHand-Regular.ttf"),
        FontChoice::Karla => super::FONT_BYTES,
        FontChoice::ComicNeue => include_bytes!("../../assets/ComicNeue-Regular.ttf"),
        FontChoice::PatrickHand => include_bytes!("../../assets/PatrickHand-Regular.ttf"),
        FontChoice::DeliusUnicase => include_bytes!("../../assets/DeliusUnicase-Regular.ttf"),
    }
}

fn font(choice: FontChoice) -> &'static FontArc {
    static FONTS: OnceLock<[FontArc; 7]> = OnceLock::new();
    let fonts = FONTS.get_or_init(|| {
        [
            FontChoice::Karla,
            FontChoice::ComicNeue,
            FontChoice::PatrickHand,
            FontChoice::DeliusUnicase,
            FontChoice::Lato,
            FontChoice::BalsamiqSans,
            FontChoice::KomikaHand,
        ]
        .map(|f| FontArc::try_from_slice(font_bytes(f)).expect("Bundled font is valid"))
    });
    &fonts[match choice {
        FontChoice::Karla => 0,
        FontChoice::ComicNeue => 1,
        FontChoice::PatrickHand => 2,
        FontChoice::DeliusUnicase => 3,
        FontChoice::Lato => 4,
        FontChoice::BalsamiqSans => 5,
        FontChoice::KomikaHand => 6,
    }]
}

fn legacy(style: &TextStyle) -> bool {
    if style.modern_layout {
        return false;
    }
    let defaults = TextStyle::default();
    let mut appearance = style.clone();
    // Editing behavior does not affect the appearance of older documents.
    appearance.auto_fit = defaults.auto_fit;
    appearance.max_width = defaults.max_width;
    appearance.preset_id = None;
    appearance.cluster_id = None;
    appearance.joins.clear();
    appearance == defaults
}

fn display_text<'a>(text: &'a str, style: &TextStyle) -> Cow<'a, str> {
    let text = if style.uppercase {
        Cow::Owned(text.to_uppercase())
    } else {
        Cow::Borrowed(text)
    };
    let face = font(style.font);
    let substitute = |ch| {
        if face.glyph_id(ch).0 == 0 {
            match ch {
                '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}' => Some("'"),
                '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{201F}' => Some("\""),
                '\u{2010}'..='\u{2015}' | '\u{2212}' => Some("-"),
                '\u{2026}' => Some("..."),
                '\u{00A0}' => Some(" "),
                _ => None,
            }
        } else {
            None
        }
    };
    if !text.chars().any(|ch| substitute(ch).is_some()) {
        return text;
    }
    let mut normalized = String::with_capacity(text.len());
    for ch in text.chars() {
        if let Some(replacement) = substitute(ch) {
            normalized.push_str(replacement);
        } else {
            normalized.push(ch);
        }
    }
    Cow::Owned(normalized)
}

fn shape_inset(shape: BubbleShape) -> f32 {
    match shape {
        BubbleShape::Speech | BubbleShape::Whisper => 0.15,
        BubbleShape::Thought => 0.21,
        BubbleShape::Shout => 0.23,
        BubbleShape::Electronic => 0.21,
        BubbleShape::Intensity => 0.24,
        BubbleShape::Impact => 0.22,
        _ => 0.0,
    }
}

fn text_insets(layer: &Layer, style: &TextStyle, bubble: bool) -> [f32; 2] {
    if !bubble {
        return [0.0, 0.0];
    }
    let fraction = shape_inset(style.shape);
    [
        style.padding + layer.width * fraction,
        style.padding + layer.height * fraction,
    ]
}

struct Line {
    text: String,
    width: f32,
}

fn word_metrics<F: Font>(font: &impl ScaleFont<F>, text: &str) -> (f32, f32) {
    let mut last = None;
    let mut width = 0.0_f32;
    let mut peak = 0.0_f32;
    for ch in text.chars().filter(|c| !c.is_control()) {
        let id = font.glyph_id(ch);
        width += font.h_advance(id) + last.map_or(0.0, |prev| font.kern(prev, id));
        peak = peak.max(width);
        last = Some(id);
    }
    (width, peak)
}

fn lines(text: &str, choice: FontChoice, size: f32, width: f32) -> Vec<Line> {
    let font = font(choice).as_scaled(size);
    let mut result = Vec::new();
    let width = width.max(1.0);
    for paragraph in text.split('\n') {
        let mut line = String::new();
        let mut used = 0.0;
        let mut previous = None;
        for word in paragraph.split_whitespace() {
            let (_, word_width) = word_metrics(&font, word);
            let space = font.glyph_id(' ');
            let first = word
                .chars()
                .find(|c| !c.is_control())
                .map(|ch| font.glyph_id(ch));
            let gap = if line.is_empty() {
                0.0
            } else {
                font.h_advance(space)
                    + previous.map_or(0.0, |prev| font.kern(prev, space))
                    + first.map_or(0.0, |id| font.kern(space, id))
            };
            if !line.is_empty() && used + gap + word_width > width {
                result.push(Line {
                    width: used,
                    text: std::mem::take(&mut line),
                });
                used = 0.0;
                previous = None;
            } else if !line.is_empty() {
                line.push(' ');
                used += font.h_advance(space) + previous.map_or(0.0, |prev| font.kern(prev, space));
                previous = Some(space);
            }
            for ch in word.chars().filter(|c| !c.is_control()) {
                let id = font.glyph_id(ch);
                let mut advance =
                    font.h_advance(id) + previous.map_or(0.0, |prev| font.kern(prev, id));
                if !line.is_empty() && used + advance > width {
                    result.push(Line {
                        width: used,
                        text: std::mem::take(&mut line),
                    });
                    used = 0.0;
                    advance = font.h_advance(id);
                }
                line.push(ch);
                used += advance;
                previous = Some(id);
            }
        }
        result.push(Line {
            width: used,
            text: line,
        });
    }
    result
}

pub fn fit_text_height(layer: &Layer) -> Result<f32> {
    let LayerKind::Text {
        text,
        size,
        bubble,
        style,
        ..
    } = &layer.kind
    else {
        bail!("Select a text layer");
    };
    let inset = text_insets(layer, style, *bubble);
    let text = display_text(text, style);
    let count = lines(&text, style.font, *size, layer.width - 2.0 * inset[0])
        .len()
        .max(1);
    Ok(required_height(count, *size, style, *bubble).clamp(1.0, 200_000.0))
}

struct MeasuredGlyph {
    advance: f32,
    kern: f32,
}
struct MeasuredWord {
    glyphs: Vec<MeasuredGlyph>,
    width: f32,
    peak: f32,
    gap: f32,
}
struct MeasuredText {
    paragraphs: Vec<Vec<MeasuredWord>>,
    glyph_count: usize,
    widest_paragraph: f32,
    widest_glyph: f32,
    widest_word: f32,
}

impl MeasuredText {
    fn new(text: &str, choice: FontChoice, size: f32) -> Self {
        let face = font(choice).as_scaled(size);
        let space = face.glyph_id(' ');
        let mut measured = Self {
            paragraphs: Vec::new(),
            glyph_count: 0,
            widest_paragraph: 0.0,
            widest_glyph: 0.0,
            widest_word: 0.0,
        };
        for paragraph in text.split('\n') {
            let mut words = Vec::new();
            let mut prior_word = None;
            let mut paragraph_width = 0.0;
            for word in paragraph.split_whitespace() {
                let mut glyphs = Vec::new();
                let mut previous = None;
                let mut width = 0.0;
                let mut peak = 0.0_f32;
                let mut gap = 0.0;
                for ch in word.chars().filter(|ch| !ch.is_control()) {
                    let id = face.glyph_id(ch);
                    let advance = face.h_advance(id);
                    let kern = previous.map_or(0.0, |last| face.kern(last, id));
                    if previous.is_none() {
                        gap = prior_word.map_or(0.0, |last| {
                            face.h_advance(space) + face.kern(last, space) + face.kern(space, id)
                        });
                    }
                    glyphs.push(MeasuredGlyph { advance, kern });
                    width += advance + kern;
                    peak = peak.max(width);
                    measured.widest_glyph = measured.widest_glyph.max(advance);
                    measured.glyph_count += 1;
                    previous = Some(id);
                }
                if let Some(last) = previous {
                    prior_word = Some(last);
                    paragraph_width += gap + width;
                    measured.widest_word = measured.widest_word.max(peak);
                    words.push(MeasuredWord {
                        glyphs,
                        width,
                        peak,
                        gap,
                    });
                }
            }
            measured.widest_paragraph = measured.widest_paragraph.max(paragraph_width);
            measured.paragraphs.push(words);
        }
        measured
    }

    fn wrap(&self, width: f32) -> WrapMeasure {
        let width = width.max(1.0);
        let mut summary = WrapMeasure::default();
        for paragraph in &self.paragraphs {
            let mut used = 0.0;
            let mut words_on_line = 0;
            let mut has_glyph = false;
            let start_line = summary.count;
            let mut widest = 0.0_f32;
            for word in paragraph {
                let gap = if has_glyph { word.gap } else { 0.0 };
                if has_glyph && used + gap + word.peak > width {
                    summary.add_line(used);
                    widest = widest.max(used);
                    used = 0.0;
                    words_on_line = 0;
                    has_glyph = false;
                }
                if word.peak <= width {
                    used += if has_glyph { word.gap } else { 0.0 };
                    used += word.width;
                    words_on_line += 1;
                    has_glyph = true;
                } else {
                    for glyph in &word.glyphs {
                        let mut advance = glyph.advance + if has_glyph { glyph.kern } else { 0.0 };
                        if has_glyph && used + advance > width {
                            summary.add_line(used);
                            widest = widest.max(used);
                            used = 0.0;
                            advance = glyph.advance;
                        }
                        used += advance;
                        has_glyph = true;
                        words_on_line = 1;
                    }
                }
            }
            summary.add_line(used);
            widest = widest.max(used);
            // Manual line breaks are intentional. Penalize only an automatically
            // stranded final word in a paragraph that has several other words.
            if paragraph.len() > 3
                && summary.count - start_line > 1
                && words_on_line == 1
                && widest > 0.0
            {
                summary.orphan_penalty += (0.55 - used / widest).max(0.0);
            }
        }
        summary
    }
}

#[derive(Default)]
struct WrapMeasure {
    count: usize,
    widest: f32,
    orphan_penalty: f32,
}
impl WrapMeasure {
    fn add_line(&mut self, width: f32) {
        self.count += 1;
        self.widest = self.widest.max(width);
    }
}

/// Choose a compact, balanced body around the actual lettering. This changes
/// both dimensions and never uses the previous body size, so deleting text also
/// shrinks a bubble. User-authored newlines remain hard breaks.
pub fn fit_text_size(layer: &Layer, max_width: f32) -> Result<[f32; 2]> {
    let LayerKind::Text {
        text,
        size,
        bubble,
        style,
        ..
    } = &layer.kind
    else {
        bail!("Select a text layer");
    };
    if !max_width.is_finite() || max_width <= 0.0 || !size.is_finite() || *size <= 0.0 {
        bail!("Invalid lettering size");
    }
    let text = display_text(text, style);
    let measured = MeasuredText::new(&text, style.font, *size);
    let padding = if *bubble { style.padding } else { 0.0 };
    let fraction = if *bubble {
        shape_inset(style.shape)
    } else {
        0.0
    };
    let usable = 1.0 - 2.0 * fraction;
    let limit = max_width.clamp(1.0, 16_384.0);
    let target = if measured.glyph_count <= 12 {
        1.3
    } else if *bubble && style.shape == BubbleShape::Narration {
        2.1
    } else {
        1.8
    };
    let one_line_height = required_height(1, *size, style, *bubble);
    // A normal word should never be broken merely to obtain a prettier aspect
    // ratio. Character wrapping is reserved for words wider than the user's cap.
    let minimum = ((measured.widest_word.max(measured.widest_glyph) + 2.0 * padding + 2.0)
        / usable)
        .ceil()
        .clamp(1.0, limit);
    let maximum = ((measured.widest_paragraph + 2.0 * padding + 2.0) / usable)
        .max(one_line_height * target * 1.15)
        .ceil()
        .clamp(minimum, limit);
    // Font metrics are measured once. Candidate wrapping then visits only words
    // (or premeasured glyphs for unusually long unbroken words).
    let candidates: usize = if maximum - minimum < 1.0 {
        1
    } else if measured.glyph_count > 16_384 {
        8
    } else if measured.glyph_count > 4096 {
        16
    } else {
        32
    };
    let mut best = [maximum, one_line_height];
    let mut best_score = f32::INFINITY;
    for index in 0..candidates {
        let width = (minimum + (maximum - minimum) * index as f32 / (candidates - 1).max(1) as f32)
            .ceil()
            .min(limit);
        let available = (width * usable - 2.0 * padding).max(1.0);
        let wrapped = measured.wrap(available);
        let height = required_height(wrapped.count, *size, style, *bubble);
        let aspect_error = ((width / height) / target).ln().abs();
        let spare_width = (available / wrapped.widest.max(size * 0.25)).max(1.0).ln();
        let score = aspect_error + spare_width * 0.16 + wrapped.orphan_penalty * 0.8;
        if score < best_score {
            best_score = score;
            best = [width, height.clamp(1.0, 200_000.0)];
        }
    }
    // Reuse the drawing layout for the final height. This also protects against
    // floating-point threshold differences at an exact word-wrap boundary.
    let available = (best[0] * usable - 2.0 * padding).max(1.0);
    let count = lines(&text, style.font, *size, available).len();
    best[1] = required_height(count, *size, style, *bubble).clamp(1.0, 200_000.0);
    Ok(best)
}

fn required_height(count: usize, size: f32, style: &TextStyle, bubble: bool) -> f32 {
    let content = size + count.saturating_sub(1) as f32 * size * style.line_spacing;
    let padding = if bubble { style.padding } else { 0.0 };
    let fraction = if bubble {
        shape_inset(style.shape)
    } else {
        0.0
    };
    ((content + padding * 2.0 + 2.0) / (1.0 - 2.0 * fraction)).ceil()
}

pub fn text_overflows(layer: &Layer) -> bool {
    let LayerKind::Text {
        text,
        size,
        bubble,
        style,
        ..
    } = &layer.kind
    else {
        return false;
    };
    let inset = text_insets(layer, style, *bubble);
    let available = layer.width - 2.0 * inset[0];
    let text = display_text(text, style);
    let layout = lines(&text, style.font, *size, available);
    required_height(layout.len(), *size, style, *bubble) > layer.height + 0.1
        || layout.iter().any(|line| line.width > available + 0.1)
}

fn cross(a: Point, b: Point) -> f32 {
    a[0] * b[1] - a[1] * b[0]
}
fn sub(a: Point, b: Point) -> Point {
    [a[0] - b[0], a[1] - b[1]]
}
fn lerp(a: Point, b: Point, t: f32) -> Point {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}

fn body(layer: &Layer, shape: BubbleShape) -> Vec<Point> {
    let w = layer.width;
    let h = layer.height;
    if shape == BubbleShape::Narration {
        return vec![[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]];
    }
    if shape == BubbleShape::Rounded {
        let r = 20.0_f32.min(w * 0.5).min(h * 0.5);
        let mut points = Vec::new();
        for (center, start) in [
            ([w - r, h - r], 0.0),
            ([r, h - r], PI * 0.5),
            ([r, r], PI),
            ([w - r, r], PI * 1.5),
        ] {
            for step in 0..=8 {
                let a = start + step as f32 / 8.0 * PI * 0.5;
                points.push([center[0] + a.cos() * r, center[1] + a.sin() * r]);
            }
        }
        return points;
    }
    if shape == BubbleShape::Impact {
        let tips = [
            [0.0, 0.0],
            [0.66, 0.08],
            [1.0, 0.0],
            [0.96, 0.36],
            [1.0, 1.0],
            [0.47, 0.92],
            [0.0, 1.0],
            [0.08, 0.4],
        ];
        let mut points = Vec::new();
        for (a, b) in tips
            .iter()
            .zip(tips.iter().cycle().skip(1))
            .take(tips.len())
        {
            let midpoint = [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5];
            let control = [midpoint[0] * 0.68 + 0.16, midpoint[1] * 0.68 + 0.16];
            for n in 0..12 {
                let t = n as f32 / 12.0;
                points.push([
                    ((1.0 - t).powi(2) * a[0] + 2.0 * (1.0 - t) * t * control[0] + t * t * b[0])
                        * w,
                    ((1.0 - t).powi(2) * a[1] + 2.0 * (1.0 - t) * t * control[1] + t * t * b[1])
                        * h,
                ]);
            }
        }
        return points;
    }
    let count = match shape {
        BubbleShape::Intensity => 1080,
        BubbleShape::Shout => 40,
        BubbleShape::Electronic => 64,
        _ => 120,
    };
    (0..count)
        .map(|i| {
            let angle = i as f32 / count as f32 * TAU;
            let radius = match shape {
                BubbleShape::Intensity => {
                    let mut hash = (i as u32).wrapping_mul(0x45d9f3b);
                    hash ^= hash >> 16;
                    hash = hash.wrapping_mul(0x45d9f3b);
                    hash ^= hash >> 16;
                    let noise = (hash & 0xffff) as f32 / 65535.0;
                    if i % 2 == 0 {
                        0.90 + 0.10 * noise
                    } else {
                        0.83 + 0.035 * noise
                    }
                }
                BubbleShape::Thought => 0.94 + 0.06 * (angle * 12.0).cos(),
                BubbleShape::Shout if i % 2 != 0 => 0.77,
                BubbleShape::Shout => 0.96 + 0.04 * (angle * 7.0).cos(),
                BubbleShape::Electronic if i % 2 != 0 => 0.84,
                _ => 1.0,
            };
            [
                w * 0.5 + angle.cos() * w * 0.5 * radius,
                h * 0.5 + angle.sin() * h * 0.5 * radius,
            ]
        })
        .collect()
}

fn ray_boundary(points: &[Point], center: Point, angle: f32) -> Point {
    let direction = [angle.cos(), angle.sin()];
    let mut distance = f32::MAX;
    for (a, b) in points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .take(points.len())
    {
        let edge = sub(*b, *a);
        let det = cross(direction, edge);
        if det.abs() < 1e-6 {
            continue;
        }
        let delta = sub(*a, center);
        let t = cross(delta, edge) / det;
        let u = cross(delta, direction) / det;
        if t >= 0.0 && (0.0..=1.0).contains(&u) {
            distance = distance.min(t);
        }
    }
    if distance == f32::MAX {
        return center;
    }
    [
        center[0] + direction[0] * distance,
        center[1] + direction[1] * distance,
    ]
}

fn paths(layer: &Layer, style: &TextStyle) -> Vec<Vec<Point>> {
    if style.shape == BubbleShape::Narration {
        return vec![body(layer, style.shape)];
    }
    let mut result = Vec::new();
    for tail in style.tails().filter(|tail| tail.enabled) {
        for path in pointer_paths(layer, style.shape, tail) {
            if !result.contains(&path) {
                result.push(path);
            }
        }
    }
    if result.is_empty() {
        result.push(body(layer, style.shape));
    }
    result
}

// Each pointer shares a complete body. Fill and outline are unioned when drawn,
// so overlapping attachments never leave a line across the bubble or each other.
fn pointer_paths(layer: &Layer, shape: BubbleShape, tail: &BubbleTail) -> Vec<Vec<Point>> {
    let outline = body(layer, shape);
    let tip = [tail.tip[0] * layer.width, tail.tip[1] * layer.height];
    if super::point_in_polygon(tip[0], tip[1], &outline) {
        return vec![outline];
    }
    let center = [layer.width * 0.5, layer.height * 0.5];
    let delta = sub(tip, center);
    let angle = delta[1].atan2(delta[0]);
    let anchor = ray_boundary(&outline, center, angle);
    let distance = (tip[0] - anchor[0]).hypot(tip[1] - anchor[1]);
    if distance < 1.0 {
        return vec![outline];
    }
    if shape == BubbleShape::Thought {
        let mut result = vec![outline];
        for (t, r) in [(0.23, 0.23), (0.59, 0.15), (1.0, 0.08)] {
            let c = lerp(anchor, tip, t);
            let radius = (tail.width * r).min(distance * 0.13).max(1.0);
            result.push(
                (0..24)
                    .map(|i| {
                        let a = i as f32 / 24.0 * TAU;
                        [c[0] + a.cos() * radius, c[1] + a.sin() * radius]
                    })
                    .collect(),
            );
        }
        return result;
    }
    let radius = (anchor[0] - center[0])
        .hypot(anchor[1] - center[1])
        .max(1.0);
    let half_angle = (tail.width.min(layer.width.min(layer.height) * 0.65) * 0.5 / radius)
        .atan()
        .clamp(0.015, 0.65);
    let start_angle = angle + half_angle;
    let end_angle = angle - half_angle;
    let start = ray_boundary(&outline, center, start_angle);
    let end = ray_boundary(&outline, center, end_angle);
    let mut ordered: Vec<_> = outline
        .iter()
        .map(|p| {
            (
                ((p[1] - center[1]).atan2(p[0] - center[0]) - start_angle).rem_euclid(TAU),
                *p,
            )
        })
        .filter(|(a, _)| *a < TAU - 2.0 * half_angle)
        .collect();
    ordered.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut result = vec![start];
    result.extend(ordered.into_iter().map(|(_, p)| p));
    result.push(end);
    let length = delta[0].hypot(delta[1]).max(1.0);
    let normal = [-delta[1] / length, delta[0] / length];
    let bend = tail.bend * distance * 0.35;
    let edge_curve = |a: Point, b: Point| -> Vec<Point> {
        if shape == BubbleShape::Electronic {
            [0.28, 0.48, 0.65, 1.0]
                .into_iter()
                .map(|t| {
                    let p = lerp(a, b, t);
                    let zig = if t == 0.28 {
                        -1.0
                    } else if t == 0.48 {
                        1.0
                    } else {
                        0.0
                    };
                    [
                        p[0] + normal[0] * (bend + zig * tail.width * 0.3) * (1.0 - t),
                        p[1] + normal[1] * (bend + zig * tail.width * 0.3) * (1.0 - t),
                    ]
                })
                .collect()
        } else {
            let middle = lerp(a, b, 0.5);
            let control = [middle[0] + normal[0] * bend, middle[1] + normal[1] * bend];
            (1..=12)
                .map(|i| {
                    let t = i as f32 / 12.0;
                    let c = lerp(a, control, t);
                    let d = lerp(control, b, t);
                    lerp(c, d, t)
                })
                .collect()
        }
    };
    result.extend(edge_curve(end, tip));
    let mut back = edge_curve(start, tip);
    back.pop();
    back.reverse();
    result.extend(back);
    vec![result]
}

/// Exact painted bounds, including tails extending beyond the editable body.
pub fn text_bounds(layer: &Layer) -> [f32; 4] {
    let LayerKind::Text {
        bubble: true,
        style,
        ..
    } = &layer.kind
    else {
        return [layer.x, layer.y, layer.width, layer.height];
    };
    if legacy(style) {
        return [layer.x, layer.y, layer.width, layer.height];
    }
    let mut extent = [0.0_f32, 0.0_f32, layer.width, layer.height];
    for p in paths(layer, style).into_iter().flatten() {
        extent[0] = extent[0].min(p[0]);
        extent[1] = extent[1].min(p[1]);
        extent[2] = extent[2].max(p[0]);
        extent[3] = extent[3].max(p[1]);
    }
    let margin = style.outline_width * 0.5 + 1.0;
    // Integer chapter bounds stabilize the preview's origin and avoid fractional
    // translation differences at 1:1 zoom.
    let x0 = (extent[0] - margin).floor();
    let y0 = (extent[1] - margin).floor();
    let x1 = (extent[2] + margin).ceil();
    let y1 = (extent[3] + margin).ceil();
    [layer.x + x0, layer.y + y0, x1 - x0, y1 - y0]
}

pub fn text_hit_test(layer: &Layer, point: Point) -> bool {
    let local = [point[0] - layer.x, point[1] - layer.y];
    let LayerKind::Text {
        bubble: true,
        style,
        ..
    } = &layer.kind
    else {
        return local[0] >= 0.0
            && local[1] >= 0.0
            && local[0] <= layer.width
            && local[1] <= layer.height;
    };
    paths(layer, style)
        .iter()
        .any(|p| super::point_in_polygon(local[0], local[1], p))
}

fn fill_polygon(coverage: &mut [u8], width: usize, height: usize, points: &[Point], origin: Point) {
    let mut intersections = Vec::with_capacity(points.len());
    for y in 0..height {
        for offset in [0.25, 0.75] {
            let py = y as f32 + origin[1] + offset;
            intersections.clear();
            for (a, b) in points
                .iter()
                .zip(points.iter().cycle().skip(1))
                .take(points.len())
            {
                if (a[1] > py) != (b[1] > py) {
                    intersections.push(a[0] + (b[0] - a[0]) * (py - a[1]) / (b[1] - a[1]));
                }
            }
            intersections.sort_by(f32::total_cmp);
            for pair in intersections.as_chunks::<2>().0 {
                let left = pair[0].max(origin[0]);
                let right = pair[1].min(origin[0] + width as f32);
                for x in (left - origin[0]).floor().max(0.0) as usize
                    ..(right - origin[0]).ceil().max(0.0) as usize
                {
                    if x >= width {
                        break;
                    }
                    let amount = ((right.min(x as f32 + origin[0] + 1.0)
                        - left.max(x as f32 + origin[0]))
                    .max(0.0)
                        * 127.5)
                        .round() as u8;
                    let index = y * width + x;
                    coverage[index] = coverage[index].saturating_add(amount);
                }
            }
        }
    }
}

fn stroke_segment(
    coverage: &mut [u8],
    width: usize,
    height: usize,
    a: Point,
    b: Point,
    radius: f32,
    origin: Point,
) {
    let x0 = (a[0].min(b[0]) - radius - 1.0 - origin[0])
        .floor()
        .max(0.0)
        .min(width as f32) as usize;
    let y0 = (a[1].min(b[1]) - radius - 1.0 - origin[1])
        .floor()
        .max(0.0)
        .min(height as f32) as usize;
    let x1 = (a[0].max(b[0]) + radius + 1.0 - origin[0])
        .ceil()
        .max(0.0)
        .min(width as f32) as usize;
    let y1 = (a[1].max(b[1]) + radius + 1.0 - origin[1])
        .ceil()
        .max(0.0)
        .min(height as f32) as usize;
    let delta = sub(b, a);
    let length2 = delta[0] * delta[0] + delta[1] * delta[1];
    for y in y0..y1 {
        for x in x0..x1 {
            let p = [x as f32 + origin[0] + 0.5, y as f32 + origin[1] + 0.5];
            let t = if length2 > 0.0 {
                ((p[0] - a[0]) * delta[0] + (p[1] - a[1]) * delta[1]) / length2
            } else {
                0.0
            }
            .clamp(0.0, 1.0);
            let near = lerp(a, b, t);
            let alpha = ((radius + 0.5 - (p[0] - near[0]).hypot(p[1] - near[1])).clamp(0.0, 1.0)
                * 255.0)
                .round() as u8;
            let index = y * width + x;
            coverage[index] = coverage[index].max(alpha);
        }
    }
}

fn visible_segment(
    a: Point,
    b: Point,
    origin: Point,
    width: f32,
    height: f32,
    padding: f32,
) -> Option<[f32; 2]> {
    let mut range = [0.0_f32, 1.0_f32];
    for axis in 0..2 {
        let low = origin[axis] - padding;
        let high = origin[axis] + if axis == 0 { width } else { height } + padding;
        let delta = b[axis] - a[axis];
        if delta.abs() < 1e-6 {
            if a[axis] < low || a[axis] > high {
                return None;
            }
        } else {
            let t0 = (low - a[axis]) / delta;
            let t1 = (high - a[axis]) / delta;
            range[0] = range[0].max(t0.min(t1));
            range[1] = range[1].min(t0.max(t1));
            if range[0] > range[1] {
                return None;
            }
        }
    }
    Some(range)
}

/// Split a polygon edge at crossings and retain only the union's exterior.
/// This also handles coincident edges and overlapping thought dots. Keeping the
/// original edge parameter makes whisper dashes independent of export slices.
fn exterior_ranges(paths: &[Vec<Point>], current: usize, a: Point, b: Point) -> Vec<[f32; 2]> {
    if paths.len() == 1 {
        return vec![[0.0, 1.0]];
    }
    let edge = sub(b, a);
    let length2 = edge[0] * edge[0] + edge[1] * edge[1];
    if length2 < 1e-10 {
        return Vec::new();
    }
    let mut cuts = vec![0.0, 1.0];
    for (index, path) in paths.iter().enumerate() {
        if index == current {
            continue;
        }
        for (&c, &d) in path
            .iter()
            .zip(path.iter().cycle().skip(1))
            .take(path.len())
        {
            if a[0].max(b[0]) < c[0].min(d[0])
                || c[0].max(d[0]) < a[0].min(b[0])
                || a[1].max(b[1]) < c[1].min(d[1])
                || c[1].max(d[1]) < a[1].min(b[1])
            {
                continue;
            }
            let other = sub(d, c);
            let offset = sub(c, a);
            let determinant = cross(edge, other);
            if determinant.abs() > 1e-6 {
                let t = cross(offset, other) / determinant;
                let u = cross(offset, edge) / determinant;
                if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u) {
                    cuts.push(t);
                }
            } else if cross(offset, edge).abs() <= 1e-3 {
                for point in [c, d] {
                    let delta = sub(point, a);
                    let t = (delta[0] * edge[0] + delta[1] * edge[1]) / length2;
                    if (0.0..=1.0).contains(&t) {
                        cuts.push(t);
                    }
                }
            }
        }
    }
    cuts.sort_by(f32::total_cmp);
    cuts.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    let length = length2.sqrt();
    let normal = [-edge[1] / length, edge[0] / length];
    cuts.windows(2)
        .filter_map(|range| {
            let middle = lerp(a, b, (range[0] + range[1]) * 0.5);
            // Stay representable even for distant (signed) pointer coordinates.
            let epsilon = (middle[0].abs().max(middle[1].abs()) * f32::EPSILON * 2.0).max(0.001);
            let inside = |sign: f32| {
                let x = middle[0] + normal[0] * epsilon * sign;
                let y = middle[1] + normal[1] * epsilon * sign;
                paths.iter().any(|path| super::point_in_polygon(x, y, path))
            };
            (inside(-1.0) != inside(1.0)).then_some([range[0], range[1]])
        })
        .collect()
}

fn draw_bubble(image: &mut RgbaImage, layer: &Layer, style: &TextStyle, scale: f32, origin: Point) {
    let w = image.width() as usize;
    let h = image.height() as usize;
    let mut paths: Vec<Vec<Point>> = paths(layer, style)
        .into_iter()
        .map(|p| {
            p.into_iter()
                .map(|p| [(layer.x + p[0]) * scale, (layer.y + p[1]) * scale])
                .collect()
        })
        .collect();
    let mut coverage = vec![0u8; w * h];
    let mut path_coverage = (paths.len() > 1).then(|| vec![0u8; w * h]);
    for path in &paths {
        if let Some(path_coverage) = &mut path_coverage {
            path_coverage.fill(0);
            fill_polygon(path_coverage, w, h, path, origin);
            for (union, part) in coverage.iter_mut().zip(path_coverage) {
                *union = (*union).max(*part);
            }
        } else {
            fill_polygon(&mut coverage, w, h, path, origin);
        }
    }
    let own_paths = paths.len();
    for join in &style.joins {
        let mut neighbor = layer.clone();
        neighbor.x = layer.x + join.x;
        neighbor.y = layer.y + join.y;
        neighbor.width = join.width;
        neighbor.height = join.height;
        let path: Vec<_> = body(&neighbor, join.shape)
            .into_iter()
            .map(|p| [(neighbor.x + p[0]) * scale, (neighbor.y + p[1]) * scale])
            .collect();
        if join.painted_before {
            let mut covered = vec![0_u8; w * h];
            fill_polygon(&mut covered, w, h, &path, origin);
            for (a, b) in coverage.iter_mut().zip(covered) {
                *a = a.saturating_sub(b);
            }
        }
        paths.push(path);
    }
    for (pixel, amount) in image.pixels_mut().zip(&coverage) {
        if *amount > 0 {
            super::blend(pixel, style.fill, *amount as f32 / 255.0);
        }
    }
    if style.outline_width <= 0.0 {
        return;
    }
    coverage.fill(0);
    for (path_index, path) in paths.iter().take(own_paths).enumerate() {
        let mut position = 0.0;
        for (&a, &b) in path
            .iter()
            .zip(path.iter().cycle().skip(1))
            .take(path.len())
        {
            let length = (b[0] - a[0]).hypot(b[1] - a[1]);
            let ranges = exterior_ranges(&paths, path_index, a, b);
            for [outer_first, outer_last] in ranges {
                if style.shape == BubbleShape::Whisper && length > 0.0 {
                    let period = 14.0 * scale;
                    let dash = 8.0 * scale;
                    if let Some([first, last]) = visible_segment(
                        a,
                        b,
                        origin,
                        w as f32,
                        h as f32,
                        style.outline_width * scale * 0.5 + 1.0,
                    ) {
                        let first = first.max(outer_first);
                        let last = last.min(outer_last);
                        let first_dash = ((position + first * length) / period).floor() as i64;
                        let last_dash = ((position + last * length) / period).floor() as i64;
                        for index in first_dash..=last_dash {
                            let start = index as f32 * period - position;
                            let end = (start + dash).min(last * length);
                            let start = start.max(first * length);
                            if end > start {
                                stroke_segment(
                                    &mut coverage,
                                    w,
                                    h,
                                    lerp(a, b, start / length),
                                    lerp(a, b, end / length),
                                    style.outline_width * scale * 0.5,
                                    origin,
                                );
                            }
                        }
                    }
                } else {
                    stroke_segment(
                        &mut coverage,
                        w,
                        h,
                        lerp(a, b, outer_first),
                        lerp(a, b, outer_last),
                        style.outline_width * scale * 0.5,
                        origin,
                    );
                }
            }
            position += length;
        }
    }
    for (pixel, amount) in image.pixels_mut().zip(coverage) {
        if amount > 0 {
            super::blend(pixel, style.outline, amount as f32 / 255.0);
        }
    }
}

pub(super) fn draw_text(output: &mut RgbaImage, y: u32, scale: f32, layer: &Layer) -> Result<()> {
    let LayerKind::Text {
        text,
        color,
        size,
        bubble,
        style,
    } = &layer.kind
    else {
        return Ok(());
    };
    if !size.is_finite() || *size <= 0.0 || layer.width <= 0.0 || layer.height <= 0.0 {
        return Ok(());
    }
    if legacy(style) {
        return super::draw_legacy_text(output, y, scale, layer, text, *color, *size, *bubble);
    }
    let [bx, by, bw, bh] = text_bounds(layer);
    let x0 = (bx * scale).floor().clamp(0.0, output.width() as f32) as u32;
    let y0 = (by * scale - y as f32)
        .floor()
        .clamp(0.0, output.height() as f32) as u32;
    let x1 = ((bx + bw) * scale).ceil().clamp(0.0, output.width() as f32) as u32;
    let y1 = ((by + bh) * scale - y as f32)
        .ceil()
        .clamp(0.0, output.height() as f32) as u32;
    if x1 <= x0 || y1 <= y0 {
        return Ok(());
    }
    let mut tile = RgbaImage::new(x1 - x0, y1 - y0);
    let origin = [x0 as f32, (y + y0) as f32];
    if *bubble {
        draw_bubble(&mut tile, layer, style, scale, origin);
    }
    let inset = text_insets(layer, style, *bubble);
    let available = layer.width - inset[0] * 2.0;
    let text = display_text(text, style);
    let layout = lines(&text, style.font, *size, available);
    let content_height = *size + layout.len().saturating_sub(1) as f32 * size * style.line_spacing;
    let top = layer.y
        + inset[1]
        + if *bubble {
            (layer.height - inset[1] * 2.0 - content_height).max(0.0) * 0.5
        } else {
            0.0
        };
    let font = font(style.font).as_scaled(size * scale);
    for (line_index, line) in layout.iter().enumerate() {
        let indent = match style.align {
            TextAlign::Left => 0.0,
            TextAlign::Center => (available - line.width).max(0.0) * 0.5,
            TextAlign::Right => (available - line.width).max(0.0),
        };
        let mut cursor = (layer.x + inset[0] + indent) * scale;
        let baseline =
            (top + line_index as f32 * size * style.line_spacing) * scale + font.ascent();
        if baseline + size * scale < origin[1] {
            continue;
        }
        if baseline - size * scale * 2.0 > origin[1] + tile.height() as f32 {
            break;
        }
        let mut previous = None;
        for ch in line.text.chars() {
            let id = font.glyph_id(ch);
            if let Some(prev) = previous {
                cursor += font.kern(prev, id);
            }
            let glyph = id.with_scale_and_position(
                size * scale,
                point(cursor - origin[0], baseline - origin[1]),
            );
            if let Some(outline) = ((cursor + size * scale >= origin[0])
                && (cursor - size * scale <= origin[0] + tile.width() as f32))
                .then(|| font.outline_glyph(glyph))
                .flatten()
            {
                let bounds = outline.px_bounds();
                outline.draw(|gx, gy, coverage| {
                    let px = bounds.min.x as i32 + gx as i32;
                    let py = bounds.min.y as i32 + gy as i32;
                    let world_x = (px as f32 + origin[0]) / scale;
                    let world_y = (py as f32 + origin[1]) / scale;
                    if px >= 0
                        && py >= 0
                        && px < tile.width() as i32
                        && py < tile.height() as i32
                        && world_x >= layer.x + inset[0]
                        && world_x < layer.x + layer.width - inset[0]
                        && world_y >= layer.y + inset[1]
                        && world_y < layer.y + layer.height - inset[1]
                    {
                        super::blend(tile.get_pixel_mut(px as u32, py as u32), *color, coverage);
                    }
                });
            }
            cursor += font.h_advance(id);
            previous = Some(id);
        }
    }
    for (tx, ty, pixel) in tile.enumerate_pixels() {
        if pixel[3] > 0 {
            super::blend(
                output.get_pixel_mut(tx + x0, ty + y0),
                pixel.0,
                layer.opacity,
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AssetBytes, Chapter};

    fn dialogue() -> Layer {
        Layer {
            id: 1,
            name: "Dialogue".into(),
            visible: true,
            locked: false,
            opacity: 1.0,
            x: 100.0,
            y: 80.0,
            width: 280.0,
            height: 150.0,
            kind: LayerKind::Text {
                text: "Wait for me!".into(),
                color: [20, 20, 20, 255],
                size: 28.0,
                bubble: true,
                style: TextStyle::dialogue(),
            },
        }
    }

    fn style(layer: &mut Layer) -> &mut TextStyle {
        let LayerKind::Text { style, .. } = &mut layer.kind else {
            unreachable!()
        };
        style
    }

    #[test]
    fn old_text_without_style_keeps_legacy_defaults() {
        let json =
            r#"{"Text":{"text":"Old dialogue","color":[0,0,0,255],"size":32.0,"bubble":true}}"#;
        let layer: LayerKind = serde_json::from_str(json).unwrap();
        let LayerKind::Text { style, .. } = layer else {
            panic!("not text")
        };
        assert_eq!(style, TextStyle::default());
        assert!(!style.tail.enabled);
        assert_eq!(style.font, FontChoice::Karla);
    }

    #[test]
    fn bundled_fonts_parse_and_produce_distinct_lettering() {
        let mut fingerprints = Vec::new();
        for choice in FontChoice::ALL {
            let mut layer = dialogue();
            style(&mut layer).font = choice;
            assert_ne!(font(choice).glyph_id('A').0, 0);
            let image = super::super::text_preview(&layer).unwrap();
            assert!(!fingerprints.contains(image.as_raw()));
            fingerprints.push(image.into_raw());
        }
    }

    #[test]
    fn all_bubble_shapes_have_distinct_geometry() {
        let mut images = Vec::new();
        for shape in BubbleShape::ALL {
            let mut layer = dialogue();
            style(&mut layer).shape = shape;
            let image = super::super::text_preview(&layer).unwrap();
            assert!(image.pixels().any(|p| p[3] > 0));
            assert!(
                !images.contains(image.as_raw()),
                "duplicate shape {shape:?}"
            );
            images.push(image.into_raw());
        }
    }

    #[test]
    fn tails_are_in_bounds_and_hittable_in_every_direction() {
        for shape in [
            BubbleShape::Speech,
            BubbleShape::Thought,
            BubbleShape::Shout,
            BubbleShape::Electronic,
        ] {
            for tip in [[-0.5, 0.5], [1.5, 0.5], [0.5, -0.5], [0.5, 1.5]] {
                let mut layer = dialogue();
                style(&mut layer).shape = shape;
                style(&mut layer).tail.tip = tip;
                let target = [
                    layer.x + tip[0] * layer.width,
                    layer.y + tip[1] * layer.height,
                ];
                let [x, y, w, h] = text_bounds(&layer);
                assert!(
                    target[0] >= x && target[0] <= x + w && target[1] >= y && target[1] <= y + h
                );
                assert!(text_hit_test(&layer, target), "{shape:?} {tip:?}");
                assert!(!text_hit_test(&layer, [x - 10.0, y - 10.0]));
                let image = super::super::text_preview(&layer).unwrap();
                assert!(image.width() > layer.width as u32 || image.height() > layer.height as u32);
            }
        }
    }

    #[test]
    fn narration_and_inside_tips_hide_the_tail() {
        let mut layer = dialogue();
        style(&mut layer).shape = BubbleShape::Narration;
        let bounds = text_bounds(&layer);
        style(&mut layer).tail.tip = [-4.0, 5.0];
        assert_eq!(text_bounds(&layer), bounds);
        style(&mut layer).shape = BubbleShape::Speech;
        style(&mut layer).tail.tip = [0.5, 0.5];
        assert_eq!(paths(&layer, style(&mut layer.clone()))[0].len(), 120);
    }

    #[test]
    fn multiple_signed_pointers_are_hittable_and_disabled_pointers_do_not_expand_bounds() {
        for shape in BubbleShape::ALL {
            let mut layer = dialogue();
            let tips = [[-0.5, 0.5], [1.5, 0.5], [0.5, -0.5], [0.5, 1.5]];
            let setting = style(&mut layer);
            setting.shape = shape;
            setting.tail.tip = tips[0];
            setting.extra_tails = tips[1..]
                .iter()
                .map(|tip| BubbleTail {
                    tip: *tip,
                    ..setting.tail.clone()
                })
                .collect();
            let [x, y, w, h] = text_bounds(&layer);
            for tip in tips {
                let target = [
                    layer.x + tip[0] * layer.width,
                    layer.y + tip[1] * layer.height,
                ];
                assert_eq!(
                    text_hit_test(&layer, target),
                    shape != BubbleShape::Narration,
                    "{shape:?} {tip:?}"
                );
                if shape != BubbleShape::Narration {
                    assert!(
                        target[0] >= x
                            && target[0] <= x + w
                            && target[1] >= y
                            && target[1] <= y + h
                    );
                }
            }
            style(&mut layer).extra_tails.push(BubbleTail {
                enabled: false,
                tip: [-100.0, -100.0],
                ..BubbleTail::default()
            });
            assert_eq!(text_bounds(&layer), [x, y, w, h]);
        }
    }

    #[test]
    fn pointer_attachments_have_no_interior_outline_and_duplicates_do_not_darken_edges() {
        let mut layer = dialogue();
        let setting = style(&mut layer);
        setting.tail.tip = [-0.5, 0.5];
        setting.extra_tails.push(BubbleTail {
            tip: [1.5, 0.5],
            ..setting.tail.clone()
        });
        let setting = style(&mut layer.clone()).clone();
        let mut image = RgbaImage::new(640, 400);
        draw_bubble(&mut image, &layer, &setting, 1.0, [0.0, 0.0]);
        assert_eq!(image.get_pixel(100, 155).0, setting.fill);
        assert_eq!(image.get_pixel(380, 155).0, setting.fill);
        let mut duplicate = setting.clone();
        duplicate.extra_tails.push(duplicate.tail.clone());
        let mut duplicate_image = RgbaImage::new(640, 400);
        draw_bubble(&mut duplicate_image, &layer, &duplicate, 1.0, [0.0, 0.0]);
        assert_eq!(image, duplicate_image);
    }

    #[test]
    fn thought_pointers_each_have_a_separate_trail_and_can_replace_disabled_primary() {
        let mut layer = dialogue();
        let setting = style(&mut layer);
        setting.shape = BubbleShape::Thought;
        setting.tail.tip = [-0.5, 0.5];
        setting.extra_tails.push(BubbleTail {
            tip: [1.5, 0.5],
            ..setting.tail.clone()
        });
        let setting = setting.clone();
        assert_eq!(paths(&layer, &setting).len(), 7);
        style(&mut layer).tail.enabled = false;
        assert_eq!(paths(&layer, &style(&mut layer.clone()).clone()).len(), 4);
        assert!(text_hit_test(
            &layer,
            [layer.x + 1.5 * layer.width, layer.y + 0.5 * layer.height]
        ));
        assert!(!text_hit_test(
            &layer,
            [layer.x - 0.5 * layer.width, layer.y + 0.5 * layer.height]
        ));
    }

    #[test]
    fn linking_a_legacy_bubble_to_a_story_preset_does_not_change_its_rendering() {
        let mut layer = dialogue();
        *style(&mut layer) = TextStyle::default();
        let image = super::super::text_preview(&layer).unwrap();
        style(&mut layer).preset_id = Some(123);
        assert!(legacy(style(&mut layer)));
        assert_eq!(super::super::text_preview(&layer).unwrap(), image);
    }

    #[test]
    fn translucent_bubble_composites_text_and_fill_as_one_layer() {
        let mut layer = dialogue();
        layer.opacity = 0.5;
        let image = super::super::text_preview(&layer).unwrap();
        assert!(image.pixels().any(|p| p[3] == 128));
        assert!(image.pixels().all(|p| p[3] <= 128));
    }

    #[test]
    fn fit_height_wraps_text_without_silent_vertical_clipping() {
        let mut layer = dialogue();
        if let LayerKind::Text { text, .. } = &mut layer.kind {
            *text="A long sentence must wrap safely inside each kind of bubble without clipping its last line.".into();
        }
        for shape in BubbleShape::ALL {
            style(&mut layer).shape = shape;
            layer.height = 60.0;
            assert!(text_overflows(&layer));
            layer.height = fit_text_height(&layer).unwrap();
            assert!(!text_overflows(&layer), "{shape:?}");
        }
    }

    #[test]
    fn new_bubbles_and_tails_match_across_export_slices() {
        for shape in BubbleShape::ALL {
            for multiple in [false, true] {
                let mut layer = dialogue();
                style(&mut layer).shape = shape;
                style(&mut layer).tail.tip = [0.2, 2.5];
                if multiple {
                    let setting = style(&mut layer);
                    setting.extra_tails = [[1.2, 2.0], [-0.3, 1.2], [0.5, -0.4]]
                        .into_iter()
                        .map(|tip| BubbleTail {
                            tip,
                            ..setting.tail.clone()
                        })
                        .collect();
                }
                layer.opacity = 0.7;
                let mut chapter = Chapter::new(2, "Bubble", 640, 720);
                chapter.layers.push(layer);
                let all =
                    super::super::render_region(&chapter, &AssetBytes::new(), 0, 450, 400).unwrap();
                for (start, height) in [(0, 97), (97, 143), (240, 210)] {
                    let slice = super::super::render_region(
                        &chapter,
                        &AssetBytes::new(),
                        start,
                        height,
                        400,
                    )
                    .unwrap();
                    for (x, y, pixel) in slice.enumerate_pixels() {
                        assert_eq!(
                            *pixel,
                            *all.get_pixel(x, y + start),
                            "{shape:?}, {x}, {}",
                            y + start
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn text_previews_with_long_tails_stay_bounded() {
        let mut layer = dialogue();
        layer.height = 200_000.0;
        style(&mut layer).tail.tip = [5.0, 5.0];
        let image = super::super::text_preview(&layer).unwrap();
        assert!(image.width() <= 2048 && image.height() <= 2048);
    }

    #[test]
    fn overflow_warning_survives_the_maximum_body_height() {
        let mut layer = dialogue();
        layer.height = 200_000.0;
        if let LayerKind::Text { text, size, .. } = &mut layer.kind {
            *text = "A\n".repeat(3000);
            *size = 120.0;
        }
        assert_eq!(fit_text_height(&layer).unwrap(), 200_000.0);
        assert!(text_overflows(&layer));
    }

    #[test]
    fn long_unbroken_words_wrap_without_losing_characters() {
        let text = "ABCdef".repeat(4000);
        let layout = lines(&text, FontChoice::ComicNeue, 32.0, 160.0);
        assert!(layout.len() > 100);
        assert!(layout.iter().all(|line| line.width <= 160.0));
        assert_eq!(
            layout.into_iter().map(|line| line.text).collect::<String>(),
            text
        );
    }

    #[test]
    fn natural_fit_grows_and_shrinks_both_dimensions() {
        let mut layer = dialogue();
        if let LayerKind::Text { text, .. } = &mut layer.kind {
            *text = "Hi".into();
        }
        let short = fit_text_size(&layer, 640.0).unwrap();
        if let LayerKind::Text { text, .. } = &mut layer.kind {
            *text="I thought you had already left. Wait for me, and we can catch the last train together!".into();
        }
        let long = fit_text_size(&layer, 640.0).unwrap();
        assert!(
            long[0] > short[0] && long[1] > short[1],
            "short={short:?}, long={long:?}"
        );
        assert!(short[0] < 220.0 && short[1] < 180.0, "{short:?}");
        assert!(
            long[0] <= 640.0 && long[0] / long[1] > 1.2 && long[0] / long[1] < 2.6,
            "{long:?}"
        );
        layer.width = long[0];
        layer.height = long[1];
        if let LayerKind::Text { text, .. } = &mut layer.kind {
            *text = "Hi".into();
        }
        assert_eq!(fit_text_size(&layer, 640.0).unwrap(), short);
    }

    #[test]
    fn natural_fit_honors_manual_line_breaks_and_all_shapes() {
        let mut layer = dialogue();
        if let LayerKind::Text { text, .. } = &mut layer.kind {
            *text = "One\nTwo\n\nFour".into();
        }
        for shape in BubbleShape::ALL {
            style(&mut layer).shape = shape;
            let fitted = fit_text_size(&layer, 420.0).unwrap();
            layer.width = fitted[0];
            layer.height = fitted[1];
            assert!(!text_overflows(&layer), "{shape:?}: {fitted:?}");
            let LayerKind::Text {
                text,
                size,
                bubble,
                style,
                ..
            } = &layer.kind
            else {
                unreachable!()
            };
            let shown = display_text(text, style);
            let inset = text_insets(&layer, style, *bubble);
            assert_eq!(
                lines(&shown, style.font, *size, layer.width - inset[0] * 2.0).len(),
                4
            );
        }
    }

    #[test]
    fn natural_fit_respects_width_limit_and_font_measurements() {
        let mut layer = dialogue();
        if let LayerKind::Text { text, .. } = &mut layer.kind {
            *text =
                "This dialogue can grow taller, but it must stay within the chosen maximum width."
                    .into();
        }
        for choice in FontChoice::ALL {
            style(&mut layer).font = choice;
            let fitted = fit_text_size(&layer, 300.0).unwrap();
            assert!(fitted[0] <= 300.0);
            layer.width = fitted[0];
            layer.height = fitted[1];
            assert!(!text_overflows(&layer), "{choice:?}");
        }
        assert!(fit_text_size(&layer, f32::NAN).is_err());
    }

    #[test]
    fn premeasured_wrapping_matches_the_drawing_layout() {
        let samples = [
            "Several short words and one longerword to wrap",
            "One\nTwo and three\n\nFour",
            "Supercalifragilisticexpialidociousabcdefghijk",
        ];
        for choice in FontChoice::ALL {
            for text in samples {
                let measured = MeasuredText::new(text, choice, 28.0);
                for width in [40.0, 100.0, 220.0, 400.0] {
                    let summary = measured.wrap(width);
                    let layout = lines(text, choice, 28.0, width);
                    assert_eq!(summary.count, layout.len(), "{choice:?}, {width}, {text}");
                    let widest = layout.iter().map(|line| line.width).fold(0.0_f32, f32::max);
                    assert!((summary.widest - widest).abs() < 0.01);
                }
            }
        }
    }

    #[test]
    fn uppercase_and_missing_punctuation_are_nondestructive() {
        let mut layer = dialogue();
        style(&mut layer).font = FontChoice::KomikaHand;
        style(&mut layer).uppercase = true;
        let source = "“Wait… don’t leave!”";
        if let LayerKind::Text { text, .. } = &mut layer.kind {
            *text = source.into();
        }
        let LayerKind::Text { text, style, .. } = &layer.kind else {
            unreachable!()
        };
        let shown = display_text(text, style);
        assert!(shown.contains("WAIT"));
        assert_eq!(text, source);
        for ch in shown.chars().filter(|ch| !ch.is_whitespace()) {
            assert_ne!(
                font(style.font).glyph_id(ch).0,
                0,
                "Unsupported punctuation {ch}"
            );
        }
        let mut explicit = layer.clone();
        if let LayerKind::Text { text, style, .. } = &mut explicit.kind {
            *text = shown.into_owned();
            style.uppercase = false;
        }
        assert_eq!(
            fit_text_size(&layer, 640.0).unwrap(),
            fit_text_size(&explicit, 640.0).unwrap()
        );
        assert_eq!(
            super::super::text_preview(&layer).unwrap(),
            super::super::text_preview(&explicit).unwrap()
        );
    }

    #[test]
    fn fitting_metadata_does_not_change_legacy_appearance() {
        let mut style = TextStyle {
            auto_fit: true,
            max_width: 900.0,
            ..TextStyle::default()
        };
        assert!(legacy(&style));
        style.uppercase = true;
        assert!(!legacy(&style));
    }

    #[test]
    fn natural_fit_keeps_short_words_whole() {
        let mut layer = dialogue();
        if let LayerKind::Text {
            text, style, size, ..
        } = &mut layer.kind
        {
            *text = "Wait.".into();
            style.font = FontChoice::KomikaHand;
            style.uppercase = true;
            *size = 42.0;
        }
        let fitted = fit_text_size(&layer, 640.0).unwrap();
        layer.width = fitted[0];
        layer.height = fitted[1];
        let LayerKind::Text {
            text,
            size,
            bubble,
            style,
            ..
        } = &layer.kind
        else {
            unreachable!()
        };
        let shown = display_text(text, style);
        let inset = text_insets(&layer, style, *bubble);
        let layout = lines(&shown, style.font, *size, layer.width - inset[0] * 2.0);
        assert_eq!(layout.len(), 1, "{fitted:?}");
        assert_eq!(layout[0].text, "WAIT.");
    }

    #[test]
    fn modern_fit_does_not_restore_legacy_whitespace_layout_when_disabled() {
        let mut layer = dialogue();
        if let LayerKind::Text { text, style, .. } = &mut layer.kind {
            *text = "Hello      world".into();
            *style = TextStyle::default();
            style.modern_layout = true;
            style.auto_fit = true;
        }
        let fitted = fit_text_size(&layer, 260.0).unwrap();
        layer.width = fitted[0];
        layer.height = fitted[1];
        assert!(!legacy(style(&mut layer)));
        assert!(!text_overflows(&layer));
        let fitted_preview = super::super::text_preview(&layer).unwrap();
        style(&mut layer).auto_fit = false;
        assert!(!legacy(style(&mut layer)));
        assert_eq!(super::super::text_preview(&layer).unwrap(), fitted_preview);
    }
}
