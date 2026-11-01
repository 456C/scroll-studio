//! Native, same-size comparison of export and texture filtering. QA only.
use eframe::egui;
use scroll_studio::{compositor, model::*, storage};

struct Comparison {
    density: f32,
    chapter: Chapter,
    proof: egui::TextureHandle,
    previews: Vec<(Layer, egui::TextureHandle, egui::TextureHandle)>,
}

impl Comparison {
    fn new(ctx: &egui::Context, chapter: Chapter) -> Self {
        let upload = |name: &str, image: image::RgbaImage, options| {
            ctx.load_texture(
                name,
                egui::ColorImage::from_rgba_unmultiplied(
                    [image.width() as usize, image.height() as usize],
                    image.as_raw(),
                ),
                options,
            )
        };
        let density = ctx.pixels_per_point();
        let output_width = (360.0 * density).round() as u32;
        let height =
            (chapter.height as f32 * output_width as f32 / chapter.width as f32).ceil() as u32;
        let proof = upload(
            "export",
            compositor::render_region(&chapter, &AssetBytes::new(), 0, height, output_width)
                .unwrap(),
            egui::TextureOptions::NEAREST,
        );
        let previews = chapter
            .layers
            .iter()
            .filter(|l| matches!(l.kind, LayerKind::Text { .. }))
            .map(|layer| {
                let image = compositor::text_preview(layer).unwrap();
                let display = compositor::text_preview_at_scale(
                    layer,
                    output_width as f32 / chapter.width as f32,
                )
                .unwrap();
                let color = egui::ColorImage {
                    size: [display.width() as usize, display.height() as usize],
                    pixels: display
                        .pixels()
                        .map(|p| {
                            let [r, g, b, a] = p.0;
                            let premul = |v: u8| ((u16::from(v) * u16::from(a) + 127) / 255) as u8;
                            egui::Color32::from_rgba_premultiplied(
                                premul(r),
                                premul(g),
                                premul(b),
                                a,
                            )
                        })
                        .collect(),
                };
                let linear = ctx.load_texture("display-size", color, egui::TextureOptions::LINEAR);
                let mip = upload(
                    "mip",
                    image,
                    egui::TextureOptions::LINEAR
                        .with_mipmap_mode(Some(egui::TextureFilter::Linear)),
                );
                (layer.clone(), linear, mip)
            })
            .collect();
        Self {
            density,
            chapter,
            proof,
            previews,
        }
    }
}

impl eframe::App for Comparison {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        if self.density != ctx.pixels_per_point() {
            *self = Self::new(ctx, self.chapter.clone());
        }
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal_top(|ui| {
                for (index, label) in [
                    "Direct export · 360 px",
                    "Display-size texture · Linear",
                    "Full-size texture · Mipmapped",
                ]
                .iter()
                .enumerate()
                {
                    ui.vertical(|ui| {
                        ui.set_width(360.0);
                        ui.label(format!("{label} · density {}", ctx.pixels_per_point()));
                        let scale = 360.0 / self.chapter.width as f32;
                        let (canvas, _) = ui.allocate_exact_size(
                            egui::vec2(360.0, self.chapter.height as f32 * scale),
                            egui::Sense::hover(),
                        );
                        if index == 0 {
                            ui.painter().image(
                                self.proof.id(),
                                canvas,
                                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                                egui::Color32::WHITE,
                            );
                        } else {
                            ui.painter().rect_filled(
                                canvas,
                                0.0,
                                egui::Color32::from_rgb(235, 239, 235),
                            );
                            for (layer, linear, mip) in &self.previews {
                                let [x, y, w, h] = compositor::text_bounds(layer);
                                let rect = egui::Rect::from_min_size(
                                    canvas.min + egui::vec2(x, y) * scale,
                                    egui::vec2(w, h) * scale,
                                );
                                let rect = if index == 1 {
                                    egui::Rect::from_min_size(
                                        egui::pos2(
                                            (rect.min.x * ctx.pixels_per_point()).round()
                                                / ctx.pixels_per_point(),
                                            (rect.min.y * ctx.pixels_per_point()).round()
                                                / ctx.pixels_per_point(),
                                        ),
                                        linear.size_vec2() / ctx.pixels_per_point(),
                                    )
                                } else {
                                    rect
                                };
                                ui.painter().image(
                                    if index == 1 { linear.id() } else { mip.id() },
                                    rect,
                                    egui::Rect::from_min_max(
                                        egui::Pos2::ZERO,
                                        egui::pos2(1.0, 1.0),
                                    ),
                                    egui::Color32::WHITE,
                                );
                            }
                        }
                    });
                }
            });
        });
    }
}

fn main() -> eframe::Result<()> {
    let path = std::env::args_os()
        .nth(1)
        .expect("Pass a sample story path");
    let (story, _) = storage::load_story(std::path::Path::new(&path)).expect("Valid sample");
    if std::env::args().any(|arg| arg == "--metrics") {
        let layers: Vec<_> = story
            .chapters
            .iter()
            .flat_map(|ch| {
                ch.layers
                    .iter()
                    .filter(|l| matches!(l.kind, LayerKind::Text { .. }))
                    .map(move |l| (ch.width, l))
            })
            .collect();
        let mut base_bytes = 0;
        let mut display_bytes = 0;
        let mut base_time = std::time::Duration::ZERO;
        let mut display_time = std::time::Duration::ZERO;
        for (width, layer) in &layers {
            let _ = compositor::text_preview(layer).unwrap();
            let started = std::time::Instant::now();
            let full = compositor::text_preview(layer).unwrap();
            base_time += started.elapsed();
            base_bytes += full.as_raw().len();
            let started = std::time::Instant::now();
            let phone = compositor::text_preview_at_scale(layer, 540.0 / *width as f32).unwrap();
            display_time += started.elapsed();
            display_bytes += phone.as_raw().len();
        }
        println!(
            "{} text layers, phone at 150%: {} -> {} raster bytes; {:?} -> {:?} preparation",
            layers.len(),
            base_bytes,
            display_bytes,
            base_time,
            display_time
        );
        return Ok(());
    }
    eframe::run_native(
        "Scroll Studio · Preview comparison",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default().with_inner_size([1120.0, 770.0]),
            renderer: eframe::Renderer::Glow,
            multisampling: 0,
            ..Default::default()
        },
        Box::new(move |cc| {
            Ok(Box::new(Comparison::new(
                &cc.egui_ctx,
                story.chapters[0].clone(),
            )))
        }),
    )
}
