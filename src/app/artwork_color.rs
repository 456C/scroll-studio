//! Match the CPU compositor's sRGBA blending in the Glow artwork path.
//! UI theme colors deliberately keep egui's normal color conversion.
use eframe::egui::{Color32, ColorImage};

pub(super) fn transparency_grid(painter: &eframe::egui::Painter, rect: eframe::egui::Rect) {
    use eframe::egui::{pos2, vec2, Rect};
    let visible = rect.intersect(painter.clip_rect());
    if !visible.is_positive() {
        return;
    }
    painter.rect_filled(visible, 0.0, Color32::from_gray(49));
    let step = 16.0_f32.max((visible.area() / 2500.0).sqrt());
    let x0 = (visible.left() / step).floor() as i32;
    let y0 = (visible.top() / step).floor() as i32;
    let x1 = (visible.right() / step).ceil() as i32;
    let y1 = (visible.bottom() / step).ceil() as i32;
    for y in y0..y1 {
        for x in x0..x1 {
            if (x + y) % 2 == 0 {
                let cell =
                    Rect::from_min_size(pos2(x as f32 * step, y as f32 * step), vec2(step, step))
                        .intersect(visible);
                painter.rect_filled(cell, 0.0, Color32::from_gray(61));
            }
        }
    }
}

pub(super) fn color(rgba: [u8; 4], opacity: f32) -> Color32 {
    let alpha = (rgba[3] as f32 / 255.0 * opacity).clamp(0.0, 1.0);
    Color32::from_rgba_premultiplied(
        (rgba[0] as f32 * alpha).round() as u8,
        (rgba[1] as f32 * alpha).round() as u8,
        (rgba[2] as f32 * alpha).round() as u8,
        (alpha * 255.0).round() as u8,
    )
}

pub(super) fn opacity_tint(opacity: f32) -> Color32 {
    let alpha = (opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgba_premultiplied(alpha, alpha, alpha, alpha)
}

/// Convert in place before filtering, so hidden RGB cannot bleed into cutouts.
/// All preview worker results use this encoding; original assets stay intact.
pub(super) fn premultiply(image: &mut image::RgbaImage) {
    for pixel in image.pixels_mut() {
        if pixel[3] == 255 {
            continue;
        }
        if pixel[3] == 0 {
            pixel.0 = [0; 4];
            continue;
        }
        let alpha = u16::from(pixel[3]);
        for channel in &mut pixel.0[..3] {
            *channel = ((u16::from(*channel) * alpha + 127) / 255) as u8;
        }
    }
}

pub(super) fn upload(image: &image::RgbaImage) -> ColorImage {
    ColorImage::from_rgba_premultiplied(
        [image.width() as usize, image.height() as usize],
        image.as_raw(),
    )
}
