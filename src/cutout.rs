//! A bounded soft backing derived from the actual cutout alpha. Foreground
//! pixels retain their original resolution; only the backing distance grid
//! uses a smaller working size for very large source images.
use crate::clipping::{distances, SelectionMask};
use anyhow::{ensure, Result};
use image::RgbaImage;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BackingSettings {
    pub enabled: bool,
    pub color: [u8; 4],
    pub reach: u16,
    pub softness: u8,
    pub gaps: u16,
}
impl Default for BackingSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            color: [225, 230, 235, 160],
            reach: 24,
            softness: 100,
            gaps: 12,
        }
    }
}
impl BackingSettings {
    pub fn validate(self) -> Result<()> {
        ensure!(
            self.reach <= 512 && self.gaps <= 128 && self.softness <= 100,
            "Invalid cutout backing settings"
        );
        Ok(())
    }
}

pub fn apply_backing(image: &mut RgbaImage, settings: BackingSettings) -> Result<()> {
    settings.validate()?;
    if !settings.enabled || settings.color[3] == 0 {
        return Ok(());
    }
    let (w, h) = image.dimensions();
    ensure!(
        w > 0 && h > 0 && u64::from(w) * u64::from(h) <= 32_000_000,
        "Invalid cutout dimensions"
    );
    let scale = (2048.0 / w.max(h) as f32).min(1.0);
    let gw = (w as f32 * scale).ceil() as u32;
    let gh = (h as f32 * scale).ceil() as u32;
    let mut silhouette = SelectionMask::empty(gw, gh)?;
    for (x, y, p) in image.enumerate_pixels() {
        if p[3] > 8 {
            let gx = (u64::from(x) * u64::from(gw) / u64::from(w)) as u32;
            let gy = (u64::from(y) * u64::from(gh) / u64::from(h)) as u32;
            silhouette.set_index((gy * gw + gx) as usize);
        }
    }
    if !silhouette.any() {
        return Ok(());
    }
    // Morphological closing joins small hair gaps without covering wide holes.
    let gap = (settings.gaps as f32 * scale * 0.5 * 8.0).round() as u16;
    if gap > 0 {
        let expanded = {
            let field = distances(&silhouette, gap + 16);
            let mut mask = silhouette.clone();
            for (i, &distance) in field.iter().enumerate() {
                if distance <= gap {
                    mask.set_index(i);
                }
            }
            mask
        };
        let field = distances(&expanded, gap + 16);
        for (i, &distance) in field.iter().enumerate() {
            if expanded.get_index(i) && distance >= gap {
                silhouette.set_index(i);
            }
        }
    }
    let reach = settings.reach as f32 * scale;
    let field = distances(&silhouette, ((reach + 2.0) * 8.0).ceil() as u16);
    let mut coverage = vec![0.0_f32; (gw * gh) as usize];
    let inner = reach * (1.0 - settings.softness as f32 / 100.0);
    for (i, a) in coverage.iter_mut().enumerate() {
        *a = if silhouette.get_index(i) {
            1.0
        } else if reach <= 0.0 {
            0.0
        } else {
            let distance = field[i] as f32 / 8.0;
            let t = ((reach - distance) / (reach - inner).max(0.125)).clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        };
    }
    drop(field);
    // Bilinear sampling softens the backing grid only. Never resize the source.
    let sample = |x: f32, y: f32| {
        let x = x.clamp(0.0, (gw - 1) as f32);
        let y = y.clamp(0.0, (gh - 1) as f32);
        let x0 = x.floor() as u32;
        let y0 = y.floor() as u32;
        let x1 = (x0 + 1).min(gw - 1);
        let y1 = (y0 + 1).min(gh - 1);
        let tx = x - x0 as f32;
        let ty = y - y0 as f32;
        let a =
            coverage[(y0 * gw + x0) as usize] * (1.0 - tx) + coverage[(y0 * gw + x1) as usize] * tx;
        let b =
            coverage[(y1 * gw + x0) as usize] * (1.0 - tx) + coverage[(y1 * gw + x1) as usize] * tx;
        a * (1.0 - ty) + b * ty
    };
    for (x, y, p) in image.enumerate_pixels_mut() {
        if p[3] == 255 {
            continue;
        }
        let foreground = p[3] as f32 / 255.0;
        let backing = sample(
            (x as f32 + 0.5) * gw as f32 / w as f32 - 0.5,
            (y as f32 + 0.5) * gh as f32 / h as f32 - 0.5,
        ) * settings.color[3] as f32
            / 255.0
            * (1.0 - foreground);
        let alpha = foreground + backing;
        if alpha <= 0.0 {
            continue;
        }
        for c in 0..3 {
            p[c] = ((p[c] as f32 * foreground + settings.color[c] as f32 * backing) / alpha).round()
                as u8;
        }
        p[3] = (alpha * 255.0).round() as u8;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;
    #[test]
    fn backing_follows_alpha_preserves_opaque_pixels_and_stays_local() {
        let mut image = RgbaImage::new(96, 96);
        for y in 25..71 {
            for x in 30..66 {
                image.put_pixel(x, y, Rgba([20, 40, 60, 255]));
            }
        }
        let original = image.clone();
        apply_backing(
            &mut image,
            BackingSettings {
                enabled: true,
                color: [100, 140, 170, 128],
                reach: 12,
                gaps: 0,
                ..Default::default()
            },
        )
        .unwrap();
        for (x, y, p) in original.enumerate_pixels() {
            if p[3] == 255 {
                assert_eq!(image.get_pixel(x, y), p);
            }
        }
        assert!(image.get_pixel(25, 45)[3] > 0);
        assert!(image.get_pixel(25, 45)[3] < 128);
        assert_eq!(image.get_pixel(5, 45)[3], 0);
        assert_eq!(image.get_pixel(25, 45).0[..3], [100, 140, 170]);
        let mut disabled = original.clone();
        apply_backing(&mut disabled, BackingSettings::default()).unwrap();
        assert_eq!(disabled, original);
    }
    #[test]
    fn small_hair_gaps_close_but_wide_holes_and_empty_images_do_not_fill() {
        let mut image = RgbaImage::new(80, 80);
        for y in 10..70 {
            for x in 10..70 {
                image.put_pixel(x, y, Rgba([30, 40, 50, 255]));
            }
        }
        for y in 20..60 {
            for x in 20..24 {
                image.put_pixel(x, y, Rgba([0; 4]));
            }
        }
        for y in 25..55 {
            for x in 40..60 {
                image.put_pixel(x, y, Rgba([0; 4]));
            }
        }
        let settings = BackingSettings {
            enabled: true,
            color: [110, 150, 190, 180],
            reach: 0,
            gaps: 8,
            ..Default::default()
        };
        apply_backing(&mut image, settings).unwrap();
        assert_eq!(image.get_pixel(22, 40)[3], 180);
        assert_eq!(image.get_pixel(50, 40)[3], 0);
        assert_eq!(image.get_pixel(0, 40)[3], 0);
        let mut empty = RgbaImage::new(80, 80);
        apply_backing(&mut empty, settings).unwrap();
        assert!(empty.pixels().all(|p| p[3] == 0));
    }
    #[test]
    fn old_edge_settings_load_without_backing_and_invalid_settings_are_rejected() {
        let original = crate::clipping::EdgeSettings::default();
        let mut value = serde_json::to_value(original).unwrap();
        value.as_object_mut().unwrap().remove("backing");
        let decoded: crate::clipping::EdgeSettings = serde_json::from_value(value).unwrap();
        assert_eq!(decoded, original);
        assert!(BackingSettings {
            reach: 513,
            ..Default::default()
        }
        .validate()
        .is_err());
        assert!(BackingSettings {
            softness: 101,
            ..Default::default()
        }
        .validate()
        .is_err());
    }
}
