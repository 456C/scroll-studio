use super::*;
use egui::{pos2, vec2, Align2, FontId, Pos2, Rect, Sense, Stroke, Vec2};
use scroll_studio::lore::{self, Kind};

const CARD_WIDTH: f32 = 208.0;

pub(super) struct BoardView {
    pan: Vec2,
    zoom: f32,
    center: Pos2,
    selected: Option<u64>,
    connection: Option<u64>,
    dragging: Option<(u64, Pos2, [f32; 2])>,
    link_from: Option<u64>,
    connecting: bool,
    fit: bool,
    menu_position: Pos2,
}
impl Default for BoardView {
    fn default() -> Self {
        Self {
            pan: Vec2::ZERO,
            zoom: 1.0,
            center: Pos2::ZERO,
            selected: None,
            connection: None,
            dragging: None,
            link_from: None,
            connecting: false,
            fit: true,
            menu_position: Pos2::ZERO,
        }
    }
}
impl BoardView {
    pub(super) fn cancel_gesture(&mut self) {
        self.dragging = None;
        self.link_from = None;
        self.connecting = false;
    }
}

struct CardVisual {
    id: u64,
    target: u64,
    rect: Rect,
    kind: Kind,
    title: String,
    text: String,
    portrait: Option<u64>,
    color: Color32,
}

fn text_in(painter: &egui::Painter, rect: Rect, text: &str, size: f32, color: Color32) {
    let galley = painter.layout(text.into(), FontId::proportional(size), color, rect.width());
    painter
        .with_clip_rect(painter.clip_rect().intersect(rect))
        .galley(rect.min, galley, color);
}

fn string_points(a: Rect, b: Rect) -> [Pos2; 4] {
    let delta = b.center() - a.center();
    let edge = |rect: Rect, direction: Vec2| {
        let factor = (rect.width() * 0.5 / direction.x.abs().max(0.001))
            .min(rect.height() * 0.5 / direction.y.abs().max(0.001));
        rect.center() + direction * factor
    };
    let start = edge(a, delta);
    let end = edge(b, -delta);
    let sag = (start.distance(end) * 0.10).min(48.0);
    [
        start,
        start.lerp(end, 0.33) + vec2(0.0, sag),
        start.lerp(end, 0.67) + vec2(0.0, sag),
        end,
    ]
}
fn curve_point(points: [Pos2; 4], t: f32) -> Pos2 {
    let a = points[0].lerp(points[1], t);
    let b = points[1].lerp(points[2], t);
    let c = points[2].lerp(points[3], t);
    a.lerp(b, t).lerp(b.lerp(c, t), t)
}

impl Studio {
    pub(super) fn new_lore(&mut self) {
        let at = self.lore_view.center - vec2(CARD_WIDTH * 0.5, 82.0);
        self.add_lore_note(at);
    }
    fn add_lore_note(&mut self, at: Pos2) {
        if self.story.lore.len() >= 4096 || self.story.lore_board.cards.len() >= lore::MAX_CARDS {
            return;
        }
        let Ok(applied) = self.apply_command(Command::AddLoreNote {
            title: "Untitled note".into(),
            position: [at.x, at.y],
        }) else {
            return;
        };
        let (note, card) = (applied.created[0], applied.created[1]);
        self.lore_view.selected = Some(card);
        self.lore_view.connection = None;
        self.lore_view.fit = false;
        self.selected_lore = Some(note);
        self.tab = LibraryTab::Lore;
    }
    pub(super) fn unlink_lore_target(&mut self, id: u64) {
        lore::unlink_target(&mut self.story, id);
    }

    fn pin_lore_target(&mut self, target: u64, at: Pos2) {
        if let Some(card) = self
            .story
            .lore_board
            .cards
            .iter()
            .find(|c| c.target == target)
        {
            self.lore_view.selected = Some(card.id);
            self.lore_view.connection = None;
            self.lore_view.pan = -(vec2(
                card.position[0] + CARD_WIDTH * 0.5,
                card.position[1] + 118.0,
            )) * self.lore_view.zoom;
            return;
        }
        self.lore_view.selected = self
            .apply_command(Command::AddLoreCard {
                target,
                position: [at.x, at.y],
            })
            .ok()
            .and_then(|applied| applied.created.first().copied());
        self.lore_view.connection = None;
        self.lore_view.fit = false;
    }
    fn lore_card_menu(&mut self, ui: &mut egui::Ui, at: Pos2) {
        for kind in [Kind::Character, Kind::Location, Kind::Group] {
            motion::menu_button(ui, kind.name(), |ui| {
                let mut choices: Vec<_> = lore::targets(&self.story)
                    .into_iter()
                    .filter(|(_, t)| t.kind == kind)
                    .map(|(id, t)| (id, t.title.to_owned()))
                    .collect();
                choices.sort_by(|a, b| a.1.cmp(&b.1));
                if choices.is_empty() {
                    ui.label("No entries yet");
                }
                egui::ScrollArea::vertical().max_height(280.0).show_rows(
                    ui,
                    28.0,
                    choices.len(),
                    |ui, rows| {
                        for (id, title) in &choices[rows] {
                            if motion::button(ui, title).clicked() {
                                self.pin_lore_target(*id, at);
                                ui.close_menu();
                            }
                        }
                    },
                );
            });
        }
    }
    fn open_lore_card(&mut self, target: u64) {
        let kind = lore::targets(&self.story).get(&target).map(|t| t.kind);
        match kind {
            Some(Kind::Character) => {
                self.select_character(target);
                self.tab = LibraryTab::Characters;
            }
            Some(Kind::Location) => {
                self.selected_location = Some(target);
                self.tab = LibraryTab::Backgrounds;
            }
            Some(Kind::Group) => {
                self.selected_group = Some(target);
                self.tab = LibraryTab::Groups;
            }
            Some(Kind::Note) => {
                self.selected_lore = Some(target);
            }
            None => {}
        }
    }
    fn connect_lore_card(&mut self, to: u64) {
        if let Some(from) = self.lore_view.link_from.take() {
            if from != to {
                // An existing connection between the two cards is selected instead.
                self.lore_view.connection = self
                    .apply_command(Command::ConnectLore {
                        from,
                        to,
                        label: String::new(),
                    })
                    .ok()
                    .and_then(|a| a.created.first().or(a.touched.first()).copied());
                self.lore_view.selected = None;
            }
            self.lore_view.connecting = false;
        } else {
            self.lore_view.link_from = Some(to);
            self.lore_view.connecting = true;
        }
    }
    fn remove_lore_card(&mut self, id: u64, delete_note: bool) {
        let _ = self.apply_command(Command::RemoveLoreCard {
            card: id,
            delete_note,
        });
        self.lore_view.selected = None;
        self.lore_view.cancel_gesture();
    }

    // The existing navigation panel becomes a compact inspector when an object is selected.
    pub(super) fn lore_list(&mut self, ui: &mut egui::Ui) {
        let mut remove_edge = None;
        if let Some(index) = self
            .story
            .lore_board
            .connections
            .iter()
            .position(|e| Some(e.id) == self.lore_view.connection)
        {
            let mut edge = self.story.lore_board.connections[index].clone();
            ui.label(RichText::new("Connection").color(MUTED));
            ui.add_space(6.0);
            ui.add(
                egui::TextEdit::singleline(&mut edge.label)
                    .hint_text("Relationship")
                    .desired_width(f32::INFINITY)
                    .char_limit(1024),
            );
            ui.color_edit_button_srgb(&mut edge.color);
            if motion::button(ui, "Remove connection").clicked() {
                remove_edge = Some(edge.id);
            }
            if edge != self.story.lore_board.connections[index] {
                let _ = self.apply_command(Command::SetLoreConnection {
                    connection: edge.id,
                    label: edge.label,
                    color: edge.color,
                });
            }
        } else if let Some(index) = self
            .story
            .lore_board
            .cards
            .iter()
            .position(|c| Some(c.id) == self.lore_view.selected)
        {
            let mut card = self.story.lore_board.cards[index].clone();
            let targets = lore::targets(&self.story);
            let Some(target) = targets.get(&card.target) else {
                return;
            };
            let kind = target.kind;
            let title = target.title.to_owned();
            ui.label(RichText::new(kind.name()).color(MUTED));
            ui.add_space(6.0);
            if kind == Kind::Note {
                let Some(note_index) = self.story.lore.iter().position(|n| n.id == card.target)
                else {
                    return;
                };
                let mut note = self.story.lore[note_index].clone();
                ui.add(
                    egui::TextEdit::singleline(&mut note.title)
                        .hint_text("Note title")
                        .desired_width(f32::INFINITY)
                        .char_limit(1024),
                );
                ui.color_edit_button_srgb(&mut card.color);
                editing::text_panel(
                    ui,
                    ("note-body", self.document_epoch, card.id),
                    &mut note.text,
                    "Write a note…",
                    (ui.available_height() - 52.0).max(100.0),
                    25_000,
                );
                let previous = &self.story.lore[note_index];
                let mut edits = Vec::new();
                for (field, old, new) in [
                    (Field::Title, &previous.title, &note.title),
                    (Field::Text, &previous.text, &note.text),
                ] {
                    if old != new {
                        edits.push(Command::SetField {
                            target: Target::Object(note.id),
                            field,
                            value: new.clone(),
                        });
                    }
                }
                if !edits.is_empty() {
                    let _ = self.apply_command(Command::Batch(edits));
                }
            } else {
                ui.add(egui::Label::new(RichText::new(title).strong()).wrap());
                if motion::button(ui, "Open sheet").clicked() {
                    self.open_lore_card(card.target);
                }
                ui.color_edit_button_srgb(&mut card.color);
                ui.add_space(6.0);
                ui.label(RichText::new("Board note").color(MUTED));
                editing::text_panel(
                    ui,
                    ("card-body", self.document_epoch, card.id),
                    &mut card.note,
                    "Add a detail…",
                    (ui.available_height() - 52.0).max(100.0),
                    25_000,
                );
            }
            let previous = &self.story.lore_board.cards[index];
            let mut edits = Vec::new();
            if card.note != previous.note {
                edits.push(Command::SetField {
                    target: Target::Object(card.id),
                    field: Field::BoardNote,
                    value: card.note.clone(),
                });
            }
            if card.color != previous.color {
                edits.push(Command::SetLoreCardColor {
                    card: card.id,
                    color: card.color,
                });
            }
            if !edits.is_empty() {
                let _ = self.apply_command(Command::Batch(edits));
            }
        } else if !self.story.lore_board.cards.is_empty() {
            ui.label(
                RichText::new(format!(
                    "{} cards · {} connections",
                    self.story.lore_board.cards.len(),
                    self.story.lore_board.connections.len()
                ))
                .size(12.0)
                .color(MUTED),
            );
        }
        if let Some(id) = remove_edge {
            let _ = self.apply_command(Command::DisconnectLore { connection: id });
            self.lore_view.connection = None;
        }
    }

    pub(super) fn lore_workspace(&mut self, ctx: &egui::Context) {
        self.canvas_rect = Rect::NOTHING;
        if !ctx.wants_keyboard_input() && !context_ui::context_menu_open(ctx) {
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.lore_view.cancel_gesture();
            }
            if ctx.input(|i| i.key_pressed(egui::Key::Delete)) {
                if let Some(id) = self.lore_view.connection.take() {
                    let _ = self.apply_command(Command::DisconnectLore { connection: id });
                } else if let Some(id) = self.lore_view.selected {
                    let is_note = self
                        .story
                        .lore_board
                        .cards
                        .iter()
                        .find(|c| c.id == id)
                        .is_some_and(|c| self.story.lore.iter().any(|n| n.id == c.target));
                    self.remove_lore_card(id, is_note);
                }
            }
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG).inner_margin(0.0))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(PANEL)
                    .inner_margin(egui::Margin::symmetric(16, 10))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Lore").size(20.0));
                            ui.add_space(10.0);
                            if !self.story.lore_board.cards.is_empty() {
                                if motion::button(ui, "+ Note").clicked() {
                                    self.new_lore();
                                }
                                let at = self.lore_view.center - vec2(CARD_WIDTH * 0.5, 118.0);
                                motion::menu_button(ui, "+ Card", |ui| self.lore_card_menu(ui, at));
                            }
                            let connect = icons::button(
                                ui,
                                icons::Icon::Lore,
                                self.lore_view.connecting,
                                "Connect cards",
                            );
                            if connect.clicked() {
                                self.lore_view.connecting = !self.lore_view.connecting;
                                self.lore_view.link_from = self
                                    .lore_view
                                    .selected
                                    .filter(|_| self.lore_view.connecting);
                                self.lore_view.connection = None;
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if motion::button(ui, "Fit").clicked() {
                                        self.lore_view.fit = true;
                                    }
                                    ui.label(
                                        RichText::new(format!(
                                            "{:.0}%",
                                            self.lore_view.zoom * 100.0
                                        ))
                                        .color(MUTED),
                                    );
                                },
                            );
                        });
                    });
                let (view, canvas) =
                    ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
                self.register_context(&canvas);
                #[cfg(test)]
                ctx.data_mut(|d| d.insert_temp(egui::Id::new("lore_board_view"), view));
                let sources = lore::targets(&self.story);
                if self.lore_view.fit && !self.story.lore_board.cards.is_empty() {
                    let bounds =
                        self.story
                            .lore_board
                            .cards
                            .iter()
                            .fold(Rect::NOTHING, |bounds, c| {
                                let height =
                                    sources.get(&c.target).map_or(164.0, |t| t.kind.height());
                                bounds.union(Rect::from_min_size(
                                    pos2(c.position[0], c.position[1]),
                                    vec2(CARD_WIDTH, height),
                                ))
                            });
                    self.lore_view.zoom = ((view.width() - 96.0) / bounds.width())
                        .min((view.height() - 96.0) / bounds.height())
                        .clamp(0.25, 1.25);
                    self.lore_view.pan = -bounds.center().to_vec2() * self.lore_view.zoom;
                    self.lore_view.fit = false;
                }
                if ui.rect_contains_pointer(view)
                    && !context_ui::context_menu_open(ctx)
                    && self.lore_view.dragging.is_none()
                {
                    let factor = ui.input(|i| i.zoom_delta());
                    let old = self.lore_view.zoom;
                    self.lore_view.zoom = (old * factor).clamp(0.25, 2.25);
                    let offset = ui.input(|i| i.pointer.hover_pos()).unwrap_or(view.center())
                        - view.center();
                    self.lore_view.pan =
                        offset - (offset - self.lore_view.pan) * (self.lore_view.zoom / old);
                    if factor == 1.0 {
                        self.lore_view.pan += ui.input(|i| i.smooth_scroll_delta);
                    }
                }
                let panning = ui.input(|i| i.key_down(egui::Key::Space) || i.pointer.middle_down());
                if (canvas.dragged() && !self.lore_view.connecting)
                    || (panning && ui.rect_contains_pointer(view))
                {
                    self.lore_view.pan += ui.input(|i| i.pointer.delta());
                }
                let zoom = self.lore_view.zoom;
                let origin = view.center() + self.lore_view.pan;
                self.lore_view.center = ((view.center() - origin) / zoom).to_pos2();
                let painter = ui.painter_at(view);
                let grid = 32.0 * zoom;
                let grid = if grid < 20.0 { grid * 2.0 } else { grid };
                for y in 0..(view.height() / grid).ceil() as i32 + 1 {
                    for x in 0..(view.width() / grid).ceil() as i32 + 1 {
                        let point = pos2(
                            view.left()
                                + (origin.x - view.left()).rem_euclid(grid)
                                + x as f32 * grid,
                            view.top() + (origin.y - view.top()).rem_euclid(grid) + y as f32 * grid,
                        );
                        painter.circle_filled(point, 0.8, Color32::from_gray(54));
                    }
                }
                let rects: HashMap<_, _> = self
                    .story
                    .lore_board
                    .cards
                    .iter()
                    .map(|c| {
                        let height = sources.get(&c.target).map_or(164.0, |t| t.kind.height());
                        (
                            c.id,
                            Rect::from_min_size(
                                origin + vec2(c.position[0], c.position[1]) * zoom,
                                vec2(CARD_WIDTH, height) * zoom,
                            ),
                        )
                    })
                    .collect();
                let visuals: Vec<_> = self
                    .story
                    .lore_board
                    .cards
                    .iter()
                    .filter(|c| rects[&c.id].intersects(view))
                    .filter_map(|c| {
                        let target = sources.get(&c.target)?;
                        Some(CardVisual {
                            id: c.id,
                            target: c.target,
                            rect: rects[&c.id],
                            kind: target.kind,
                            title: target.title.chars().take(140).collect(),
                            text: if target.kind == Kind::Note {
                                target.text
                            } else {
                                &c.note
                            }
                            .chars()
                            .take(260)
                            .collect(),
                            portrait: target.portrait,
                            color: Color32::from_rgb(c.color[0], c.color[1], c.color[2]),
                        })
                    })
                    .collect();
                let edges: Vec<_> = self
                    .story
                    .lore_board
                    .connections
                    .iter()
                    .filter_map(|e| {
                        Some((
                            e.clone(),
                            string_points(*rects.get(&e.from)?, *rects.get(&e.to)?),
                        ))
                    })
                    .collect();
                for (edge, points) in edges {
                    if !Rect::from_points(&points).expand(30.0).intersects(view) {
                        continue;
                    }
                    let color = Color32::from_rgb(edge.color[0], edge.color[1], edge.color[2]);
                    let selected = Some(edge.id) == self.lore_view.connection;
                    painter.add(egui::epaint::CubicBezierShape::from_points_stroke(
                        points,
                        false,
                        Color32::TRANSPARENT,
                        Stroke::new(
                            if selected { 3.0_f32 } else { 1.7_f32 },
                            if selected { ACCENT } else { color },
                        ),
                    ));
                    let mid = curve_point(points, 0.5);
                    let text = if edge.label.trim().is_empty() {
                        "•"
                    } else {
                        edge.label.as_str()
                    };
                    let galley = painter.layout(
                        text.chars().take(64).collect(),
                        FontId::proportional((12.0 * zoom).max(10.0)),
                        TEXT,
                        160.0 * zoom,
                    );
                    let label = Rect::from_center_size(mid, galley.size() + vec2(16.0, 10.0));
                    let r =
                        ui.interact(label, ui.id().with(("connection", edge.id)), Sense::click());
                    painter.rect_filled(label, 6.0, if selected { CARD } else { PANEL });
                    painter.galley(label.min + vec2(8.0, 5.0), galley, TEXT);
                    if r.clicked() || r.secondary_clicked() {
                        self.lore_view.connection = Some(edge.id);
                        self.lore_view.selected = None;
                    }
                    self.register_context(&r);
                    appearance::context_menu(&r, |ui| {
                        if motion::button(ui, "Remove connection").clicked() {
                            let _ = self.apply_command(Command::DisconnectLore {
                                connection: edge.id,
                            });
                            self.lore_view.connection = None;
                            ui.close_menu();
                        }
                    });
                }
                if let Some(from) = self.lore_view.link_from.and_then(|id| rects.get(&id)) {
                    if let Some(pointer) = ui.input(|i| i.pointer.hover_pos()) {
                        let points =
                            string_points(*from, Rect::from_center_size(pointer, vec2(1.0, 1.0)));
                        painter.add(egui::epaint::CubicBezierShape::from_points_stroke(
                            points,
                            false,
                            Color32::TRANSPARENT,
                            Stroke::new(2.0_f32, ACCENT),
                        ));
                    }
                }
                for card in visuals {
                    self.paint_lore_card(ui, &painter, &card, zoom);
                    let r = ui.interact(
                        card.rect,
                        ui.id().with(("lore_card", card.id)),
                        Sense::click_and_drag(),
                    );
                    #[cfg(test)]
                    ctx.data_mut(|d| {
                        d.insert_temp(egui::Id::new(("lore_card", card.id)), card.rect)
                    });
                    if r.clicked() || r.secondary_clicked() {
                        self.lore_view.selected = Some(card.id);
                        self.lore_view.connection = None;
                        self.selected_lore = (card.kind == Kind::Note).then_some(card.target);
                        if r.clicked() && self.lore_view.connecting {
                            self.connect_lore_card(card.id);
                        }
                    }
                    if r.double_clicked() && !self.lore_view.connecting {
                        self.open_lore_card(card.target);
                    }
                    if r.drag_started_by(egui::PointerButton::Primary)
                        && !panning
                        && !self.lore_view.connecting
                    {
                        if let Some(position) = ui.input(|i| i.pointer.press_origin()) {
                            let initial = self
                                .story
                                .lore_board
                                .cards
                                .iter()
                                .find(|c| c.id == card.id)
                                .unwrap()
                                .position;
                            self.checkpoint();
                            self.lore_view.dragging = Some((card.id, position, initial));
                            self.lore_view.selected = Some(card.id);
                            self.lore_view.connection = None;
                        }
                    }
                    let port = Rect::from_center_size(card.rect.right_center(), vec2(24.0, 24.0));
                    let handle = ui
                        .interact(
                            port,
                            ui.id().with(("lore_port", card.id)),
                            Sense::click_and_drag(),
                        )
                        .on_hover_text("Connect");
                    painter.circle_filled(
                        port.center(),
                        5.0,
                        if handle.hovered() || self.lore_view.link_from == Some(card.id) {
                            ACCENT
                        } else {
                            card.color
                        },
                    );
                    painter.circle_stroke(port.center(), 5.0, Stroke::new(1.5_f32, BG));
                    if handle.drag_started() {
                        self.lore_view.link_from = Some(card.id);
                        self.lore_view.connecting = true;
                    }
                    if handle.clicked() {
                        self.connect_lore_card(card.id);
                    }
                    self.register_context(&r);
                    appearance::context_menu(&r, |ui| {
                        if motion::button(
                            ui,
                            if card.kind == Kind::Note {
                                "Edit note"
                            } else {
                                "Open sheet"
                            },
                        )
                        .clicked()
                        {
                            self.open_lore_card(card.target);
                            ui.close_menu();
                        }
                        if motion::button(ui, "Connect to…").clicked() {
                            self.lore_view.link_from = Some(card.id);
                            self.lore_view.connecting = true;
                            ui.close_menu();
                        }
                        if motion::button(
                            ui,
                            if card.kind == Kind::Note {
                                "Delete note"
                            } else {
                                "Remove from board"
                            },
                        )
                        .clicked()
                        {
                            self.remove_lore_card(card.id, card.kind == Kind::Note);
                            ui.close_menu();
                        }
                    });
                }
                if let Some((id, start, initial)) = self.lore_view.dragging {
                    if let Some(pointer) = ui.input(|i| i.pointer.interact_pos()) {
                        let delta = (pointer - start) / zoom;
                        let position = [
                            (initial[0] + delta.x).clamp(-1_000_000.0, 1_000_000.0),
                            (initial[1] + delta.y).clamp(-1_000_000.0, 1_000_000.0),
                        ];
                        let moved = self
                            .story
                            .lore_board
                            .cards
                            .iter()
                            .any(|c| c.id == id && c.position != position);
                        // The drag took its single undo step when it started.
                        if moved {
                            let _ =
                                self.apply_in_gesture(Command::MoveLoreCard { card: id, position });
                        }
                    }
                    if !ui.input(|i| i.pointer.primary_down()) {
                        self.lore_view.dragging = None;
                    }
                    ctx.request_repaint();
                }
                // A port drag can end anywhere on its destination card.
                if ui.input(|i| i.pointer.primary_released() && i.pointer.is_decidedly_dragging())
                    && self.lore_view.link_from.is_some()
                {
                    if let Some(pointer) = ui.input(|i| i.pointer.interact_pos()) {
                        if let Some((&id, _)) = rects.iter().find(|(id, rect)| {
                            Some(**id) != self.lore_view.link_from && rect.contains(pointer)
                        }) {
                            self.connect_lore_card(id);
                        }
                    }
                }
                if canvas.clicked() {
                    self.lore_view.selected = None;
                    self.lore_view.connection = None;
                    self.lore_view.cancel_gesture();
                }
                if canvas.secondary_clicked() {
                    if let Some(pointer) = ui.input(|i| i.pointer.interact_pos()) {
                        self.lore_view.menu_position = ((pointer - origin) / zoom).to_pos2();
                    }
                }
                let at = self.lore_view.menu_position;
                appearance::context_menu(&canvas, |ui| {
                    if motion::button(ui, "Add note").clicked() {
                        self.add_lore_note(at);
                        ui.close_menu();
                    }
                    motion::menu_button(ui, "Add card", |ui| self.lore_card_menu(ui, at));
                    if motion::button(ui, "Fit board").clicked() {
                        self.lore_view.fit = true;
                        ui.close_menu();
                    }
                });
                if self.story.lore_board.cards.is_empty() {
                    let rect = Rect::from_center_size(view.center(), vec2(260.0, 150.0));
                    ui.scope_builder(
                        egui::UiBuilder::new()
                            .max_rect(rect)
                            .layout(egui::Layout::top_down(egui::Align::Center)),
                        |ui| {
                            let (mark, _) =
                                ui.allocate_exact_size(vec2(48.0, 48.0), Sense::hover());
                            icons::draw(ui.painter(), mark, icons::Icon::Lore, MUTED);
                            ui.add_space(12.0);
                            ui.allocate_ui_with_layout(
                                vec2(132.0, 32.0),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    if primary(ui, "+ Note").clicked() {
                                        self.new_lore();
                                    }
                                    motion::menu_button(ui, "+ Card", |ui| {
                                        self.lore_card_menu(
                                            ui,
                                            self.lore_view.center - vec2(104.0, 118.0),
                                        )
                                    });
                                },
                            );
                        },
                    );
                }
            });
    }

    fn paint_lore_card(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        card: &CardVisual,
        zoom: f32,
    ) {
        let rect = card.rect;
        let selected = self.lore_view.selected == Some(card.id);
        painter.rect_filled(
            rect.translate(vec2(0.0, 5.0)),
            10.0,
            Color32::from_black_alpha(50),
        );
        let fill = if card.kind == Kind::Note {
            motion::mix(PANEL, card.color, 0.10)
        } else {
            PANEL
        };
        painter.rect_filled(rect, 8.0, fill);
        painter.rect_stroke(
            rect,
            8.0,
            Stroke::new(
                if selected { 1.8_f32 } else { 1.0_f32 },
                if selected {
                    ACCENT
                } else {
                    Color32::from_gray(64)
                },
            ),
            egui::StrokeKind::Inside,
        );
        let padding = 14.0 * zoom;
        painter.line_segment(
            [
                rect.min + vec2(padding, 2.0),
                pos2(rect.right() - padding, rect.top() + 2.0),
            ],
            Stroke::new(2.0_f32, card.color),
        );
        let inner = rect.shrink(padding);
        painter.text(
            inner.min,
            Align2::LEFT_TOP,
            card.kind.name().to_uppercase(),
            FontId::proportional(9.0 * zoom),
            card.color,
        );
        let title = Rect::from_min_size(
            inner.min + vec2(0.0, 17.0 * zoom),
            vec2(inner.width(), 40.0 * zoom),
        );
        text_in(
            painter,
            title,
            if card.title.trim().is_empty() {
                "Untitled"
            } else {
                &card.title
            },
            16.0 * zoom,
            TEXT,
        );
        let text_top = if card.kind == Kind::Note { 66.0 } else { 176.0 };
        if card.kind != Kind::Note {
            let art = Rect::from_min_size(
                rect.min + vec2(padding, 68.0 * zoom),
                vec2(inner.width(), 96.0 * zoom),
            );
            if let Some(portrait) = card.portrait {
                self.paint_character_reference(ui, ui.ctx(), portrait, art, true);
            } else {
                painter.rect_filled(art, 4.0, BG);
                icons::draw(
                    painter,
                    Rect::from_center_size(art.center(), vec2(34.0, 34.0) * zoom),
                    match card.kind {
                        Kind::Character => icons::Icon::Characters,
                        Kind::Location => icons::Icon::Background,
                        _ => icons::Icon::Groups,
                    },
                    card.color,
                );
            }
        }
        let body = Rect::from_min_max(
            rect.min + vec2(padding, text_top * zoom),
            rect.max - vec2(padding, padding),
        );
        text_in(
            painter,
            body,
            &card.text,
            12.0 * zoom,
            if card.kind == Kind::Note { TEXT } else { MUTED },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame(ctx: &egui::Context, studio: &mut Studio, events: Vec<egui::Event>) {
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1100.0, 720.0))),
                events,
                ..Default::default()
            },
            |ctx| {
                studio.library(ctx);
                studio.lore_workspace(ctx);
            },
        );
    }
    fn button(pos: Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            pressed,
            button: egui::PointerButton::Primary,
            modifiers: egui::Modifiers::NONE,
        }
    }
    fn card(ctx: &egui::Context, id: u64) -> Rect {
        ctx.data(|d| d.get_temp(egui::Id::new(("lore_card", id))).unwrap())
    }
    #[test]
    fn connecting_already_connected_cards_selects_the_existing_connection() {
        let ctx = egui::Context::default();
        let mut studio = Studio::from_context(&ctx);
        studio.new_lore();
        let first = studio.lore_view.selected.unwrap();
        studio.new_lore();
        let second = studio.lore_view.selected.unwrap();
        studio.lore_view.link_from = Some(first);
        studio.connect_lore_card(second);
        let edge = studio
            .lore_view
            .connection
            .expect("a new connection is selected");
        let (undo, story) = (studio.undo.len(), studio.story.clone());
        // Reverse direction: the same pair, so nothing changes and no undo step is added.
        studio.lore_view.link_from = Some(second);
        studio.connect_lore_card(first);
        assert_eq!(studio.lore_view.connection, Some(edge));
        assert_eq!(studio.undo.len(), undo);
        assert_eq!(studio.story, story);
    }
    #[test]
    fn deleting_a_note_card_is_one_undo_step_that_restores_note_card_and_links() {
        let ctx = egui::Context::default();
        let mut studio = Studio::from_context(&ctx);
        studio.new_lore();
        let first = studio.lore_view.selected.unwrap();
        studio.new_lore();
        let second = studio.lore_view.selected.unwrap();
        studio.lore_view.link_from = Some(first);
        studio.connect_lore_card(second);
        let before = studio.story.clone();
        let undo = studio.undo.len();
        studio.remove_lore_card(first, true);
        assert_eq!(studio.undo.len(), undo + 1);
        assert_eq!(studio.story.lore.len(), before.lore.len() - 1);
        assert!(studio.story.lore_board.connections.is_empty());
        studio.undo();
        assert_eq!(studio.story, before);
    }
    #[test]
    fn cards_move_with_one_undo_and_ports_connect_without_moving_destinations() {
        let ctx = egui::Context::default();
        let mut studio = Studio::from_context(&ctx);
        studio.welcome = false;
        studio.tab = LibraryTab::Lore;
        studio.add_lore_note(pos2(-260.0, -50.0));
        studio.add_lore_note(pos2(80.0, -50.0));
        studio.undo.clear();
        let ids: Vec<_> = studio.story.lore_board.cards.iter().map(|c| c.id).collect();
        for _ in 0..3 {
            frame(&ctx, &mut studio, vec![]);
        }
        let original = studio.story.clone();
        let from = card(&ctx, ids[0]).center();
        let to = from + vec2(-25.0, 60.0);
        frame(&ctx, &mut studio, vec![egui::Event::PointerMoved(from)]);
        frame(&ctx, &mut studio, vec![button(from, true)]);
        frame(&ctx, &mut studio, vec![egui::Event::PointerMoved(to)]);
        frame(&ctx, &mut studio, vec![button(to, false)]);
        assert_ne!(
            studio.story.lore_board.cards[0].position,
            original.lore_board.cards[0].position
        );
        assert_eq!(studio.undo.len(), 1);
        studio.undo();
        assert_eq!(studio.story, original);
        studio.redo();
        frame(&ctx, &mut studio, vec![]);
        let positions: Vec<_> = studio
            .story
            .lore_board
            .cards
            .iter()
            .map(|c| c.position)
            .collect();
        let from = card(&ctx, ids[0]).right_center();
        let to = card(&ctx, ids[1]).center();
        frame(&ctx, &mut studio, vec![egui::Event::PointerMoved(from)]);
        frame(&ctx, &mut studio, vec![button(from, true)]);
        frame(
            &ctx,
            &mut studio,
            vec![egui::Event::PointerMoved(from + vec2(35.0, 0.0))],
        );
        frame(&ctx, &mut studio, vec![egui::Event::PointerMoved(to)]);
        frame(&ctx, &mut studio, vec![button(to, false)]);
        assert_eq!(studio.story.lore_board.connections.len(), 1);
        assert_eq!(
            studio
                .story
                .lore_board
                .cards
                .iter()
                .map(|c| c.position)
                .collect::<Vec<_>>(),
            positions
        );
        studio.undo();
        assert!(studio.story.lore_board.connections.is_empty());
        assert_eq!(studio.story.chapters, original.chapters);
        frame(&ctx, &mut studio, vec![]);
        let pointer = card(&ctx, ids[0]).center();
        frame(&ctx, &mut studio, vec![egui::Event::PointerMoved(pointer)]);
        let view: Rect = ctx.data(|d| d.get_temp(egui::Id::new("lore_board_view")).unwrap());
        let before = (pointer - view.center() - studio.lore_view.pan) / studio.lore_view.zoom;
        let zoom = studio.lore_view.zoom;
        frame(&ctx, &mut studio, vec![egui::Event::Zoom(1.25)]);
        let after = (pointer - view.center() - studio.lore_view.pan) / studio.lore_view.zoom;
        assert!(studio.lore_view.zoom > zoom);
        assert!((before - after).length() < 0.01);
    }
}
