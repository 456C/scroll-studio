//! Small procedural effects stored as editable parameters, not raster assets.
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Effect {
    Glint(Glint),
    Motion(Motion),
    Wind(Wind),
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Glint {
    pub angle: f32,
    pub color: [u8; 4],
    pub glow: f32,
    pub rays: u8,
}
impl Default for Glint {
    fn default() -> Self {
        Self {
            angle: 0.0,
            color: [220, 245, 255, 255],
            glow: 0.5,
            rays: 4,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Motion {
    pub source_layer: u64,
    pub angle: f32,
    pub length: f32,
    pub density: u16,
    pub color: [u8; 4],
    pub thickness: f32,
    pub smear_threshold: u8,
}
impl Default for Motion {
    fn default() -> Self {
        Self {
            source_layer: 0,
            angle: 0.0,
            length: 0.7,
            density: 32,
            color: [20, 25, 35, 220],
            thickness: 1.5,
            smear_threshold: 70,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Wind {
    pub angle: f32,
    pub curve: f32,
    pub density: u16,
    pub length: f32,
    pub color: [u8; 4],
    pub thickness: f32,
}
impl Default for Wind {
    fn default() -> Self {
        Self {
            angle: -25.0,
            curve: 0.2,
            density: 48,
            length: 0.8,
            color: [235, 248, 255, 210],
            thickness: 1.1,
        }
    }
}
pub struct EffectLine {
    pub points: Vec<[f32; 2]>,
    pub color: [u8; 4],
    pub width: f32,
}
fn noise(index: u32) -> f32 {
    let mut n = index.wrapping_mul(0x45d9f3b);
    n ^= n >> 16;
    n = n.wrapping_mul(0x45d9f3b);
    ((n >> 8) & 0xffff) as f32 / 65535.0
}
fn rotate(point: [f32; 2], angle: f32) -> [f32; 2] {
    let (s, c) = angle.to_radians().sin_cos();
    [point[0] * c - point[1] * s, point[0] * s + point[1] * c]
}
impl Effect {
    pub fn validate(&self) -> bool {
        let angle = match self {
            Self::Glint(g) => g.angle,
            Self::Motion(m) => m.angle,
            Self::Wind(w) => w.angle,
        };
        if !angle.is_finite() || angle.abs() > 360.0 {
            return false;
        }
        match self {
            Self::Glint(g) => {
                g.glow.is_finite() && (0.0..=1.0).contains(&g.glow) && [4, 6, 8].contains(&g.rays)
            }
            Self::Motion(m) => {
                m.length.is_finite()
                    && (0.0..=1.0).contains(&m.length)
                    && (4..=128).contains(&m.density)
                    && m.thickness.is_finite()
                    && (0.1..=12.0).contains(&m.thickness)
            }
            Self::Wind(w) => {
                w.length.is_finite()
                    && (0.0..=1.0).contains(&w.length)
                    && w.curve.is_finite()
                    && (-1.0..=1.0).contains(&w.curve)
                    && (4..=128).contains(&w.density)
                    && w.thickness.is_finite()
                    && (0.1..=12.0).contains(&w.thickness)
            }
        }
    }
    pub fn lines(&self, width: f32, height: f32) -> Vec<EffectLine> {
        let mut lines = Vec::new();
        if !self.validate() || width <= 0.0 || height <= 0.0 {
            return lines;
        }
        let center = [width * 0.5, height * 0.5];
        let unit = width.min(height);
        let map = |point: [f32; 2], angle: f32| {
            let p = rotate(point, angle);
            [center[0] + p[0], center[1] + p[1]]
        };
        match self {
            Self::Glint(g) => {
                for ray in 0..g.rays {
                    let angle = g.angle + ray as f32 * 360.0 / g.rays as f32;
                    let reach = unit * if ray % 2 == 0 { 0.44 } else { 0.33 };
                    for pass in (0..=3).rev() {
                        if pass > 0 && g.glow == 0.0 {
                            continue;
                        }
                        let mut color = g.color;
                        if pass > 0 {
                            color[3] = (color[3] as f32 * g.glow * 0.1) as u8;
                        }
                        // Taper the ray toward its end; four short segments keep
                        // the center bright without a bulky star-shaped polygon.
                        for segment in 0..6 {
                            let t0 = segment as f32 / 6.0;
                            let t1 = (segment + 1) as f32 / 6.0;
                            let size = (unit * 0.045 * (1.0 - t0).powi(2) + 0.35)
                                * (1.0 + pass as f32 * 1.6);
                            lines.push(EffectLine {
                                points: vec![
                                    map([reach * t0, 0.0], angle),
                                    map([reach * t1, 0.0], angle),
                                ],
                                color,
                                width: size,
                            });
                        }
                    }
                }
            }
            Self::Motion(m) => {
                for i in 0..m.density {
                    let t = i as f32 / m.density as f32;
                    let angle = std::f32::consts::PI * (0.55 + t * 0.9);
                    let tip = [angle.cos() * width * 0.27, angle.sin() * height * 0.32];
                    let length = unit * 0.24 * m.length * (0.25 + noise(i as u32 + 11) * 0.75);
                    let a = map([tip[0] - length, tip[1]], m.angle);
                    let b = map(tip, m.angle);
                    let mut color = m.color;
                    color[3] = (color[3] as f32 * (0.45 + noise(i as u32 + 231) * 0.55)) as u8;
                    lines.push(EffectLine {
                        points: vec![a, b],
                        color,
                        width: m.thickness * (0.5 + noise(i as u32 + 61)),
                    });
                }
            }
            Self::Wind(w) => {
                for i in 0..w.density {
                    let lane = (i as f32 + noise(i as u32 + 52)) / w.density as f32 - 0.5;
                    let start = -0.4 + noise(i as u32 + 7) * 0.3;
                    let length = (0.2 + noise(i as u32 + 13) * 0.6) * w.length;
                    let points = (0..=20)
                        .map(|step| {
                            let t = step as f32 / 20.0;
                            map(
                                [
                                    (start + length * t) * unit,
                                    (lane * 0.72 + w.curve * (t - 0.5).powi(2) * 0.5) * unit,
                                ],
                                w.angle,
                            )
                        })
                        .collect();
                    let mut color = w.color;
                    color[3] = (color[3] as f32 * (0.3 + noise(i as u32 + 72) * 0.7)) as u8;
                    lines.push(EffectLine {
                        points,
                        color,
                        width: w.thickness * (0.35 + noise(i as u32 + 91) * 1.3),
                    });
                }
            }
        }
        lines
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn effects_are_deterministic_bounded_and_reject_invalid_settings() {
        for e in [
            Effect::Glint(Glint::default()),
            Effect::Motion(Motion::default()),
            Effect::Wind(Wind::default()),
        ] {
            let lines = e.lines(400.0, 400.0);
            assert!(!lines.is_empty() && lines.len() <= 256);
            assert!(lines
                .iter()
                .all(|l| l.points.iter().flatten().all(|p| p.is_finite())));
            assert_eq!(
                serde_json::from_str::<Effect>(&serde_json::to_string(&e).unwrap()).unwrap(),
                e
            );
        }
        assert!(!Effect::Wind(Wind {
            curve: f32::NAN,
            ..Default::default()
        })
        .validate());
    }
}
