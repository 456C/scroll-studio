use super::*;
use egui::{pos2, vec2, Pos2, Rect, Sense, Vec2};
use image::RgbaImage;
use scroll_studio::painting::{
    Bounds, Brush, Dynamics, Painting, Sample, StrokePath, Tile, Tip, TILE,
};
use std::path::Path;

#[derive(Clone)]
enum Action {
    New([u32; 2]),
    Open(PathBuf),
    Home,
    Quit,
}
enum ResultImage {
    Open(PathBuf, RgbaImage),
    Saved(PathBuf),
}

#[derive(Clone, Copy)]
struct PaintViewport {
    view: Rect,
    image: Rect,
    scale: f32,
}

pub(super) struct PaintStudio {
    doc: Painting,
    brushes: [Brush; 5],
    tip: usize,
    color: [u8; 3],
    picker: bool,
    stroke: Option<StrokePath>,
    dynamics: [Dynamics; 5],
    pressure: f32,
    pen: Option<egui::TouchId>,
    stroke_ms: f32,
    upload_bytes: usize,
    textures: HashMap<Tile, egui::TextureHandle>,
    checker: Option<egui::TextureHandle>,
    swatches: Option<egui::TextureHandle>,
    zoom: f32,
    pan: Vec2,
    path: Option<PathBuf>,
    save_path: Option<PathBuf>,
    saved: u64,
    pending: Option<Action>,
    job: Option<mpsc::Receiver<anyhow::Result<ResultImage>>>,
    new_size: Option<[u32; 2]>,
    error: Option<String>,
    home: bool,
    allow_close: bool,
}
impl Default for PaintStudio {
    fn default() -> Self {
        let brushes = std::array::from_fn(|i| Brush {
            tip: Tip::ALL[i],
            size: [9.0, 3.0, 32.0, 64.0, 26.0][i],
            flow: [1.0, 0.8, 0.55, 0.16, 1.0][i],
            opacity: [1.0, 1.0, 0.55, 1.0, 1.0][i],
            softness_percent: [0.0, 12.0, 10.0, 75.0, 0.0][i],
            spacing_percent: 12.0,
            color: [28, 30, 34],
        });
        Self {
            doc: Painting::new(RgbaImage::new(1200, 1200)),
            brushes,
            tip: 0,
            color: [28, 30, 34],
            picker: false,
            stroke: None,
            dynamics: [Dynamics::default(); 5],
            pressure: 1.0,
            pen: None,
            stroke_ms: 0.0,
            upload_bytes: 0,
            textures: HashMap::new(),
            checker: None,
            swatches: None,
            zoom: 1.0,
            pan: Vec2::ZERO,
            path: None,
            save_path: None,
            saved: 0,
            pending: None,
            job: None,
            new_size: None,
            error: None,
            home: false,
            allow_close: false,
        }
    }
}

impl PaintStudio {
    fn finish(&mut self) {
        if let Some(stroke) = self.stroke.take() {
            stroke.finish(&mut self.doc);
        } else {
            self.doc.finish();
        }
    }
    fn reset(&mut self, image: RgbaImage, path: Option<PathBuf>) {
        self.doc = Painting::new(image);
        self.path = path;
        self.save_path = None;
        self.saved = 0;
        self.textures.clear();
        self.stroke = None;
        self.pressure = 1.0;
        self.pen = None;
        self.zoom = 1.0;
        self.pan = Vec2::ZERO;
    }
    fn worker(
        &mut self,
        ctx: &egui::Context,
        run: impl FnOnce() -> anyhow::Result<ResultImage> + Send + 'static,
    ) {
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(run());
            ctx.request_repaint();
        });
        self.job = Some(rx);
    }
    fn execute(&mut self, ctx: &egui::Context, action: Action) {
        match action {
            Action::New([w, h]) => self.reset(RgbaImage::new(w, h), None),
            Action::Home => self.home = true,
            Action::Quit => {
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Action::Open(path) => self.worker(ctx, move || {
                anyhow::ensure!(
                    std::fs::metadata(&path)?.len() <= 64 * 1024 * 1024,
                    "Image exceeds 64 MiB"
                );
                let image = compositor::decode_image(&std::fs::read(&path)?)?;
                Ok(ResultImage::Open(path, image))
            }),
        }
    }
    fn request(&mut self, ctx: &egui::Context, action: Action) {
        self.finish();
        if self.doc.revision != self.saved {
            self.pending = Some(action);
        } else {
            self.execute(ctx, action);
        }
    }
    fn open(&mut self, ctx: &egui::Context) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Images", &["png", "jpg", "jpeg", "webp"])
            .pick_file()
        {
            self.request(ctx, Action::Open(path));
        }
    }
    fn save(&mut self, ctx: &egui::Context, choose_path: bool) {
        self.finish();
        let path = if !choose_path {
            self.save_path.clone()
        } else {
            None
        };
        let path = path.or_else(|| {
            let stem = self
                .path
                .as_deref()
                .and_then(Path::file_stem)
                .and_then(|s| s.to_str());
            let name = stem.map_or_else(|| "Painting.png".into(), |s| format!("{s} - edit.png"));
            rfd::FileDialog::new()
                .add_filter("PNG", &["png"])
                .set_file_name(name)
                .save_file()
        });
        if let Some(mut path) = path {
            path.set_extension("png");
            let image = self.doc.image.clone();
            self.worker(ctx, move || {
                storage::save_painting_png(&path, &image)?;
                Ok(ResultImage::Saved(path))
            });
        }
    }
    fn poll(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.job else { return };
        match rx.try_recv() {
            Err(mpsc::TryRecvError::Empty) => return,
            Err(_) => self.error = Some("Image operation stopped".into()),
            Ok(Err(e)) => self.error = Some(format!("{e:#}")),
            Ok(Ok(ResultImage::Open(path, image))) => self.reset(image, Some(path)),
            Ok(Ok(ResultImage::Saved(path))) => {
                self.saved = self.doc.revision;
                self.save_path = Some(path);
                self.job = None;
                if let Some(action) = self.pending.take() {
                    self.execute(ctx, action);
                }
                return;
            }
        }
        self.job = None;
    }
    pub(super) fn update(&mut self, ctx: &egui::Context) -> bool {
        self.poll(ctx);
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if self.job.is_none() {
                self.new_size = None;
                self.request(ctx, Action::Quit);
            }
        }
        if self.home {
            return true;
        }
        let blocked = self.job.is_some()
            || self.pending.is_some()
            || self.new_size.is_some()
            || self.error.is_some();
        if !blocked && !ctx.wants_keyboard_input() {
            let key = |mods, k| {
                ctx.input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(mods, k)))
            };
            let cmd = egui::Modifiers::COMMAND;
            let shift = cmd | egui::Modifiers::SHIFT;
            if key(shift, egui::Key::S) {
                self.save(ctx, true);
            } else if key(cmd, egui::Key::S) {
                self.save(ctx, false);
            } else if key(cmd, egui::Key::O) {
                self.open(ctx);
            } else if key(cmd, egui::Key::N) {
                self.new_size = Some([1200, 1200]);
            } else if key(shift, egui::Key::Z) || key(cmd, egui::Key::Y) {
                self.finish();
                self.doc.redo();
            } else if key(cmd, egui::Key::Z) {
                self.finish();
                self.doc.undo();
            } else if key(egui::Modifiers::NONE, egui::Key::E) {
                self.tip = 4;
                self.picker = false;
            } else if key(egui::Modifiers::NONE, egui::Key::B) {
                self.tip = 0;
                self.picker = false;
            } else if key(egui::Modifiers::NONE, egui::Key::OpenBracket) {
                self.brushes[self.tip].size = (self.brushes[self.tip].size / 1.15).max(1.0);
            } else if key(egui::Modifiers::NONE, egui::Key::CloseBracket) {
                self.brushes[self.tip].size = (self.brushes[self.tip].size * 1.15).min(512.0);
            } else if key(egui::Modifiers::NONE, egui::Key::I) {
                self.picker = true;
            }
        }
        egui::TopBottomPanel::top("paint_bar")
            .exact_height(48.0)
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.add_enabled_ui(!blocked, |ui| {
                        if motion::button(ui, "‹ Start").clicked() {
                            self.request(ctx, Action::Home);
                        }
                        ui.separator();
                        if motion::button(ui, "New canvas").clicked() {
                            self.new_size = Some([1200, 1200]);
                        }
                        if motion::button(ui, "Open image…").clicked() {
                            self.open(ctx);
                        }
                        let name = self
                            .save_path
                            .as_ref()
                            .or(self.path.as_ref())
                            .and_then(|p| p.file_name())
                            .and_then(|s| s.to_str())
                            .unwrap_or("Untitled");
                        ui.add(
                            egui::Label::new(format!(
                                "{}{}",
                                name,
                                if self.doc.revision != self.saved {
                                    " •"
                                } else {
                                    ""
                                }
                            ))
                            .truncate(),
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_enabled_ui(!blocked, |ui| {
                            if primary(ui, "Save PNG").clicked() {
                                self.save(ctx, false);
                            }
                            motion::menu_button(ui, "•••", |ui| {
                                if motion::button(ui, "Save as…").clicked() {
                                    ui.close_menu();
                                    self.save(ctx, true);
                                }
                                ui.separator();
                                ui.collapsing("Performance",|ui| {
                                    ui.label(format!("{:.2} ms brush update",self.stroke_ms))
                                        .on_hover_text("CPU painting and texture preparation. Device and display latency are additional.");
                                    ui.label(format!("{:.0} KB texture update",self.upload_bytes as f32/1024.0));
                                });
                            });
                        });
                        if self.job.is_some() {
                            ui.spinner();
                        }
                    });
                });
            });
        egui::SidePanel::left("paint_tools")
            .exact_width(182.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(PANEL).inner_margin(12.0))
            .show(ctx, |ui| {
                if blocked {
                    ui.disable();
                }
                ui.heading("Brushes");
                ui.add_space(8.0);
                let swatches = self.swatches.get_or_insert_with(|| {
                    let mut sample = Painting::new(RgbaImage::new(60, 80));
                    for (i, mut brush) in self.brushes.iter().copied().enumerate() {
                        brush.color = [205, 214, 225];
                        brush.size = [3.5, 2.5, 9.0, 14.0, 4.0][i];
                        if brush.tip == Tip::Eraser {
                            for y in 66..78 {
                                for x in 3..57 {
                                    sample.image[(x, y)] = image::Rgba([205, 214, 225, 130]);
                                }
                            }
                        }
                        for x in 5..55 {
                            let t = (x - 5) as f32 / 49.0;
                            let y = i as f32 * 16.0 + 8.0 - (t * std::f32::consts::TAU).sin() * 2.4;
                            sample.dab([x as f32, y], brush);
                        }
                    }
                    ctx.load_texture(
                        "brush-swatches",
                        egui::ColorImage::from_rgba_unmultiplied([60, 80], sample.image.as_raw()),
                        egui::TextureOptions::LINEAR,
                    )
                });
                for (i, tip) in Tip::ALL.iter().enumerate() {
                    let row = panels::list_row(
                        ui,
                        tip.name(),
                        !self.picker && i == self.tip,
                        32.0,
                        Some(if *tip == Tip::Eraser {
                            icons::Icon::Eraser
                        } else {
                            icons::Icon::Brush
                        }),
                    );
                    let mark = Rect::from_center_size(
                        row.rect.right_center() - vec2(30.0, 0.0),
                        vec2(50.0, 14.0),
                    );
                    ui.painter().image(
                        swatches.id(),
                        mark,
                        Rect::from_min_max(
                            pos2(0.0, i as f32 / 5.0),
                            pos2(1.0, (i + 1) as f32 / 5.0),
                        ),
                        Color32::WHITE,
                    );
                    if row.clicked() {
                        self.tip = i;
                        self.picker = false;
                    }
                }
                ui.separator();
                ui.horizontal(|ui| {
                    ui.color_edit_button_srgb(&mut self.color);
                    if ui.selectable_label(self.picker, "Pick color").clicked() {
                        self.picker = !self.picker;
                    }
                });
                ui.add_space(6.0);
                let brush = &mut self.brushes[self.tip];
                ui.label("Size");
                ui.add(
                    egui::Slider::new(&mut brush.size, 1.0..=512.0)
                        .logarithmic(true)
                        .suffix(" px"),
                );
                ui.label("Opacity");
                ui.add(
                    egui::Slider::new(&mut brush.opacity, 0.01..=1.0)
                        .custom_formatter(|v, _| format!("{:.0}%", v * 100.0))
                        .custom_parser(|s| {
                            s.trim_end_matches('%')
                                .parse::<f64>()
                                .ok()
                                .map(|v| v / 100.0)
                        }),
                );
                let dynamics = &mut self.dynamics[self.tip];
                ui.label("Smoothing");
                ui.add(egui::Slider::new(&mut dynamics.smoothing, 0.0..=1.0).show_value(false))
                    .on_hover_text(
                        "Zero follows the pen directly. Increase to smooth hand movement.",
                    );
                motion::menu_button(ui, "Brush settings", |ui| {
                    ui.label("Flow");
                    ui.add(
                        egui::Slider::new(&mut brush.flow, 0.01..=1.0)
                            .custom_formatter(|v, _| format!("{:.0}%", v * 100.0))
                            .custom_parser(|s| {
                                s.trim_end_matches('%')
                                    .parse::<f64>()
                                    .ok()
                                    .map(|v| v / 100.0)
                            }),
                    );
                    ui.label("Edge softness");
                    ui.add(
                        egui::Slider::new(&mut brush.softness_percent, 0.0..=100.0)
                            .suffix("%")
                            .max_decimals(0),
                    );
                    ui.label("Tip spacing");
                    ui.add(
                        egui::Slider::new(&mut brush.spacing_percent, 3.0..=150.0)
                            .suffix("%")
                            .max_decimals(0),
                    );
                    ui.separator();
                    ui.label("Pen pressure");
                    ui.checkbox(&mut dynamics.pressure_size, "Size");
                    ui.checkbox(&mut dynamics.pressure_flow, "Flow");
                    ui.add(
                        egui::Slider::new(&mut dynamics.minimum_size, 0.01..=1.0)
                            .text("Minimum size")
                            .show_value(false),
                    );
                    ui.add(
                        egui::Slider::new(&mut dynamics.pressure_curve, 0.4..=2.5)
                            .text("Response")
                            .show_value(false),
                    );
                });
                ui.separator();
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(self.doc.can_undo(), motion::Button::new("Undo"))
                        .clicked()
                    {
                        self.doc.undo();
                    }
                    if ui
                        .add_enabled(self.doc.can_redo(), motion::Button::new("Redo"))
                        .clicked()
                    {
                        self.doc.redo();
                    }
                });
                ui.add_space(10.0);
                ui.label(
                    RichText::new(format!(
                        "{} × {} px",
                        self.doc.image.width(),
                        self.doc.image.height()
                    ))
                    .color(MUTED),
                );
                if motion::button(ui, "Fit canvas")
                    .on_hover_text("Ctrl + wheel to zoom · Space + drag to pan")
                    .clicked()
                {
                    self.zoom = 1.0;
                    self.pan = Vec2::ZERO;
                }
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG))
            .show(ctx, |ui| {
                self.canvas(
                    ui,
                    !blocked
                        && self.job.is_none()
                        && self.pending.is_none()
                        && self.new_size.is_none(),
                )
            });
        if !blocked && self.job.is_none() {
            if let Some(path) =
                ctx.input(|i| i.raw.dropped_files.iter().find_map(|f| f.path.clone()))
            {
                self.request(ctx, Action::Open(path));
            }
        }
        self.dialogs(ctx);
        self.home
    }

    fn canvas(&mut self, ui: &mut egui::Ui, enabled: bool) {
        let (view, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        let size = vec2(
            self.doc.image.width() as f32,
            self.doc.image.height() as f32,
        );
        let fit = ((view.width() - 36.0) / size.x)
            .min((view.height() - 36.0) / size.y)
            .max(0.001);
        let pointer = ui.input(|i| i.pointer.interact_pos());
        let pan_tool = ui.input(|i| i.key_down(egui::Key::Space) || i.pointer.middle_down());
        if enabled && response.hovered() && self.stroke.is_none() {
            let factor = ui.input(|i| i.zoom_delta());
            if factor != 1.0 {
                let old = self.zoom;
                self.zoom = (old * factor).clamp(0.2, 64.0);
                let offset = pointer.unwrap_or(view.center()) - view.center();
                self.pan = offset - (offset - self.pan) * (self.zoom / old);
            }
        }
        if enabled && pan_tool && response.dragged() {
            self.pan += response.drag_delta();
        }
        let scale = fit * self.zoom;
        let rect = Rect::from_center_size(view.center() + self.pan, size * scale);
        let position = |pt: Pos2| ((pt - rect.min) / scale).to_pos2();
        let pressed_here = response.is_pointer_button_down_on()
            || response.clicked_by(egui::PointerButton::Primary);
        let mut brush = self.brushes[self.tip];
        brush.color = self.color;
        let started = Instant::now();
        let drawing = self.stroke.is_some();
        if !enabled || pan_tool {
            self.finish();
        }
        if enabled && !pan_tool {
            let events = ui.input(|i| i.events.clone());
            let pick = self.picker || ui.input(|i| i.modifiers.alt);
            self.stroke_events(
                &events,
                PaintViewport {
                    view,
                    image: rect,
                    scale,
                },
                pressed_here,
                pick,
                brush,
            );
        }
        if !ui.input(|i| i.focused) {
            self.finish();
        }
        if drawing || self.stroke.is_some() {
            self.stroke_ms = started.elapsed().as_secs_f32() * 1000.0;
        }
        let painter = ui.painter_at(view);
        let checker = self.checker.get_or_insert_with(|| {
            let colors = [Color32::from_gray(154), Color32::from_gray(171)];
            ui.ctx().load_texture(
                "paint-checker",
                egui::ColorImage {
                    size: [2, 2],
                    pixels: vec![colors[0], colors[1], colors[1], colors[0]],
                },
                egui::TextureOptions::NEAREST_REPEAT,
            )
        });
        painter.image(
            checker.id(),
            rect,
            Rect::from_min_max(Pos2::ZERO, (rect.size() / 24.0).to_pos2()),
            Color32::WHITE,
        );
        let painted = !self.doc.invalidated.is_empty();
        let updates: Vec<_> = self.doc.take_updates().collect();
        if !updates.is_empty() {
            self.upload_bytes = 0;
        }
        for (key, bounds) in updates {
            if self.textures.contains_key(&key) {
                let pixels = self.texture_region(key, bounds);
                self.upload_bytes += pixels.pixels.len() * 4;
                self.textures.get_mut(&key).unwrap().set_partial(
                    [bounds[0] as usize, bounds[1] as usize],
                    pixels,
                    egui::TextureOptions::LINEAR,
                );
            }
        }
        let visible = rect.intersect(view);
        if visible.is_positive() {
            let from = position(visible.min);
            let to = position(visible.max);
            let xs = from.x.max(0.0) as u32 / TILE
                ..(to.x.ceil() as u32)
                    .min(self.doc.image.width())
                    .div_ceil(TILE);
            let ys = from.y.max(0.0) as u32 / TILE
                ..(to.y.ceil() as u32)
                    .min(self.doc.image.height())
                    .div_ceil(TILE);
            if self.textures.len() > 2048 {
                self.textures
                    .retain(|(x, y), _| xs.contains(x) && ys.contains(y));
            }
            for y in ys {
                for x in xs.clone() {
                    let tex = self.textures.entry((x, y)).or_insert_with(|| {
                        let tile = self.doc.tile((x, y));
                        let img = egui::ColorImage::from_rgba_unmultiplied(
                            [tile.width() as usize, tile.height() as usize],
                            tile.as_raw(),
                        );
                        ui.ctx().load_texture(
                            format!("paint-{x}-{y}"),
                            img,
                            egui::TextureOptions::LINEAR,
                        )
                    });
                    let start = rect.min + vec2((x * TILE) as f32, (y * TILE) as f32) * scale;
                    painter.image(
                        tex.id(),
                        Rect::from_min_size(start, tex.size_vec2() * scale),
                        Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                        Color32::WHITE,
                    );
                }
            }
        }
        if drawing || self.stroke.is_some() || painted {
            self.stroke_ms = started.elapsed().as_secs_f32() * 1000.0;
        }
        if enabled {
            appearance::context_menu(&response, |ui| {
                if ui
                    .add_enabled(self.doc.can_undo(), motion::Button::new("Undo"))
                    .clicked()
                {
                    self.finish();
                    self.doc.undo();
                    ui.close_menu();
                }
                if ui
                    .add_enabled(self.doc.can_redo(), motion::Button::new("Redo"))
                    .clicked()
                {
                    self.finish();
                    self.doc.redo();
                    ui.close_menu();
                }
                if motion::button(ui, "Pick color").clicked() {
                    self.picker = true;
                    ui.close_menu();
                }
                if motion::button(ui, "Fit canvas").clicked() {
                    self.zoom = 1.0;
                    self.pan = Vec2::ZERO;
                    ui.close_menu();
                }
            });
        }
        if enabled && response.hovered() {
            if let Some(p) = pointer.filter(|p| rect.contains(*p)) {
                if self.picker || pan_tool {
                    response.on_hover_cursor(if pan_tool {
                        egui::CursorIcon::Grab
                    } else {
                        egui::CursorIcon::Crosshair
                    });
                } else {
                    let r = (brush.size * scale * 0.5).max(2.0);
                    painter.circle_stroke(
                        p,
                        r,
                        egui::Stroke::new(2.0_f32, Color32::from_black_alpha(160)),
                    );
                    painter.circle_stroke(p, r, egui::Stroke::new(1.0_f32, Color32::WHITE));
                }
            }
        }
    }
    fn texture_region(&self, (tx, ty): Tile, [x0, y0, x1, y1]: Bounds) -> egui::ColorImage {
        let width = self.doc.image.width() as usize;
        let mut pixels = Vec::with_capacity(((x1 - x0) * (y1 - y0)) as usize);
        for y in y0 + ty * TILE..y1 + ty * TILE {
            let start = (y as usize * width + (x0 + tx * TILE) as usize) * 4;
            for p in self.doc.image.as_raw()[start..start + ((x1 - x0) * 4) as usize]
                .as_chunks::<4>()
                .0
            {
                pixels.push(Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3]));
            }
        }
        egui::ColorImage {
            size: [(x1 - x0) as usize, (y1 - y0) as usize],
            pixels,
        }
    }

    fn stroke_events(
        &mut self,
        events: &[egui::Event],
        viewport: PaintViewport,
        can_start: bool,
        picker: bool,
        brush: Brush,
    ) {
        let PaintViewport {
            view,
            image: rect,
            scale,
        } = viewport;
        let sample = |point: Pos2, force: f32| Sample {
            point: [
                (point.x - rect.left()) / scale,
                (point.y - rect.top()) / scale,
            ],
            force,
        };
        for event in events {
            match *event {
                egui::Event::Touch {
                    id, phase, force, ..
                } => {
                    if phase == egui::TouchPhase::Start && self.pen.is_none() {
                        self.pen = Some(id);
                    }
                    if self.pen == Some(id) {
                        if let Some(force) = force.filter(|f| f.is_finite() && *f > 0.0) {
                            self.pressure = force.clamp(0.0, 1.0);
                        }
                        if phase == egui::TouchPhase::Cancel {
                            self.finish();
                            self.pen = None;
                            self.pressure = 1.0;
                        }
                        // End is followed by a PointerButton release. Preserve its final pressure.
                    }
                }
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    ..
                } if can_start && view.contains(pos) && rect.contains(pos) => {
                    if picker {
                        let p = sample(pos, 1.0).point;
                        let x = (p[0] as u32).min(self.doc.image.width() - 1);
                        let y = (p[1] as u32).min(self.doc.image.height() - 1);
                        self.color = self.doc.image[(x, y)].0[..3].try_into().unwrap();
                        self.picker = false;
                    } else {
                        let mut path = StrokePath::new(brush, self.dynamics[self.tip]);
                        path.add(&mut self.doc, sample(pos, self.pressure));
                        self.stroke = Some(path);
                    }
                }
                egui::Event::PointerMoved(pos) => {
                    if let Some(path) = self.stroke.as_mut() {
                        path.add(&mut self.doc, sample(pos, self.pressure));
                    }
                }
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    ..
                } => {
                    if let Some(path) = self.stroke.as_mut() {
                        path.add(&mut self.doc, sample(pos, self.pressure));
                    }
                    self.finish();
                    self.pen = None;
                    self.pressure = 1.0;
                }
                egui::Event::PointerGone => {
                    self.finish();
                    self.pen = None;
                    self.pressure = 1.0;
                }
                _ => {}
            }
        }
    }
    fn dialogs(&mut self, ctx: &egui::Context) {
        if let Some(message) = self.error.clone() {
            egui::Modal::new("paint_error".into()).show(ctx, |ui| {
                ui.label(message);
                if motion::button(ui, "Close").clicked() {
                    self.error = None;
                }
            });
        } else if self.job.is_none() {
            if let Some(action) = self.pending.clone() {
                egui::Modal::new("paint_unsaved".into()).show(ctx, |ui| {
                    ui.heading("Save your image?");
                    ui.horizontal(|ui| {
                        if primary(ui, "Save PNG").clicked() {
                            self.save(ctx, false);
                        }
                        if motion::button(ui, "Discard").clicked() {
                            self.pending = None;
                            self.execute(ctx, action);
                        }
                        if motion::button(ui, "Cancel").clicked() {
                            self.pending = None;
                        }
                    });
                });
            } else if let Some(mut size) = self.new_size {
                egui::Modal::new("paint_new".into()).show(ctx, |ui| {
                    ui.heading("New canvas");
                    ui.horizontal(|ui| {
                        ui.label("Width");
                        ui.add(
                            egui::DragValue::new(&mut size[0])
                                .range(1..=16384)
                                .suffix(" px"),
                        );
                        ui.label("Height");
                        ui.add(
                            egui::DragValue::new(&mut size[1])
                                .range(1..=16384)
                                .suffix(" px"),
                        );
                    });
                    self.new_size = Some(size);
                    let valid = u64::from(size[0]) * u64::from(size[1]) <= 32_000_000;
                    if !valid {
                        ui.label("Choose a canvas under 32 megapixels");
                    }
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(valid, motion::Button::new("Create"))
                            .clicked()
                        {
                            self.new_size = None;
                            self.request(ctx, Action::New(size));
                        }
                        if motion::button(ui, "Cancel").clicked() {
                            self.new_size = None;
                        }
                    });
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn button(p: Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos: p,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }
    #[test]
    fn multiple_events_in_one_frame_preserve_corners_and_separate_strokes() {
        let mut batched = PaintStudio::default();
        batched.reset(RgbaImage::new(128, 128), None);
        let brush = Brush {
            size: 3.0,
            ..Brush::default()
        };
        let view = Rect::from_min_size(Pos2::ZERO, vec2(128.0, 128.0));
        let points = [
            pos2(10.0, 10.0),
            pos2(10.0, 80.0),
            pos2(60.0, 80.0),
            pos2(60.0, 10.0),
        ];
        let events = vec![
            button(points[0], true),
            egui::Event::PointerMoved(points[1]),
            egui::Event::PointerMoved(points[2]),
            button(points[3], false),
            button(pos2(100.0, 80.0), true),
            button(pos2(100.0, 100.0), false),
        ];
        let viewport = PaintViewport {
            view,
            image: view,
            scale: 1.0,
        };
        batched.stroke_events(&events, viewport, true, false, brush);
        let mut separate = PaintStudio::default();
        separate.reset(RgbaImage::new(128, 128), None);
        for event in events {
            separate.stroke_events(&[event], viewport, true, false, brush);
        }
        assert_eq!(batched.doc.image, separate.doc.image);
        assert!(batched.doc.image[(10, 80)][3] > 0);
        assert_eq!(batched.doc.image[(35, 10)][3], 0);
        assert_eq!(batched.doc.image[(80, 50)][3], 0);
        batched.doc.undo();
        assert_eq!(batched.doc.image[(100, 90)][3], 0);
        assert!(batched.doc.image[(10, 80)][3] > 0);
        batched.doc.undo();
        assert!(batched.doc.image.pixels().all(|p| p[3] == 0));
    }
    #[test]
    fn texture_updates_preserve_handles_and_use_partial_pixel_regions() {
        let ctx = egui::Context::default();
        let mut paint = PaintStudio::default();
        paint.reset(RgbaImage::new(128, 128), None);
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| paint.canvas(ui, true));
        });
        let texture = paint.textures[&(0, 0)].id();
        paint.doc.dab(
            [40.0, 40.0],
            Brush {
                size: 4.0,
                ..Brush::default()
            },
        );
        paint.doc.finish();
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| paint.canvas(ui, true));
        });
        assert_eq!(paint.textures[&(0, 0)].id(), texture);
        let delta = output
            .textures_delta
            .set
            .iter()
            .find(|(id, _)| *id == texture)
            .unwrap();
        assert!(delta.1.pos.is_some());
        assert!(delta.1.image.size()[0] < 16 && delta.1.image.size()[1] < 16);
    }
}
