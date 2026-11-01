#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;

use eframe::egui;

fn main() -> eframe::Result<()> {
    let icon = image::load_from_memory_with_format(
        include_bytes!("../assets/scroll-studio-icon.png"),
        image::ImageFormat::Png,
    )
    .expect("The bundled Scroll Studio icon is valid PNG")
    .into_rgba8();
    let (icon_width, icon_height) = icon.dimensions();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Scroll Studio")
            .with_icon(egui::IconData {
                rgba: icon.into_raw(),
                width: icon_width,
                height: icon_height,
            })
            .with_inner_size([1280.0, 760.0])
            .with_min_inner_size([1100.0, 720.0])
            .with_drag_and_drop(true),
        renderer: eframe::Renderer::Glow,
        multisampling: 0,
        ..Default::default()
    };
    eframe::run_native(
        "Scroll Studio",
        options,
        Box::new(|cc| Ok(Box::new(app::Studio::new(cc)))),
    )
}
