//! Run with `cargo run --release --example brush_profile` on the target machine.
//! Reports CPU rasterization, not end-to-end tablet/display latency.
use scroll_studio::painting::{Brush, Painting, Tip};
use std::{hint::black_box, time::Instant};

fn main() {
    for pressure in [false, true] {
        for tip in [Tip::Ink, Tip::Airbrush] {
            for size in [16.0, 128.0, 256.0] {
                let mut paint = Painting::new(image::RgbaImage::new(4096, 4096));
                let brush = Brush {
                    tip,
                    size,
                    opacity: 1.0,
                    flow: 0.65,
                    softness_percent: 20.0,
                    spacing_percent: 12.0,
                    color: [35, 90, 160],
                };
                let mut times = Vec::new();
                for batch in 0..120 {
                    let start = Instant::now();
                    for step in 0..8 {
                        let x = (batch * 8 + step) as f32;
                        let mut dab = brush;
                        if pressure {
                            dab.size *= 0.65 + 0.35 * (x * 0.01).sin();
                        }
                        paint.dab([256.0 + x * 3.5, 2048.0 + (x * 0.03).sin() * 800.0], dab);
                    }
                    times.push(start.elapsed().as_secs_f64() * 1000.0);
                    black_box(&paint.image);
                    paint.invalidated.clear();
                }
                paint.finish();
                times.sort_by(f64::total_cmp);
                println!(
                "{tip:?} {size:.0}px pressure={pressure} / eight dabs, CPU ms: median={:.3} p95={:.3} max={:.3}",
                times[60], times[114], times[119]
            );
            }
        }
    }
}
