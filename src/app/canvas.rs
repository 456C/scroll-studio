#[cfg(test)]
use super::lettering_ui::pointer_target as tail_tip;
use super::lettering_ui::{pointer_targets, preserve_pointer_targets};
use super::*;
use egui::{pos2, vec2, Align2, FontId, Rect, Sense, Stroke, StrokeKind};
use scroll_studio::snapping::{self, Axis, Bounds, Guide, SnapTarget};

pub(super) struct SnapDrag {
    layer_id: u64,
    chapter_id: u64,
    pointer_origin: [f32; 2],
    layer_origin: [f32; 2],
    bounds: Bounds,
    viewport: Bounds,
    targets: Vec<SnapTarget>,
}

impl Studio {
    pub(super) fn add_linked_bubble(&mut self, parent: u64) {
        let mut updated = self.story.clone();
        match scroll_studio::bubbles::add_cluster_bubble(&mut updated, self.chapter, parent) {
            Ok(id) => {
                self.checkpoint();
                self.story = updated;
                self.selected = Some(id);
                self.selected_asset = None;
                self.tool = Tool::Move;
                self.changed();
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }
    fn set_canvas_zoom(&mut self, zoom: f32, auto_fit: bool) {
        if self.zoom > 0.0 && self.zoom != zoom && self.jump_y.is_none() {
            self.jump_y = Some(self.scroll_y);
        }
        self.zoom = zoom;
        self.auto_fit = auto_fit;
    }

    pub(super) fn asset_thumbnail(
        &mut self,
        ctx: &egui::Context,
        asset_id: u64,
    ) -> Option<egui::TextureHandle> {
        let bytes = self.assets.get(&asset_id)?;
        self.previews.image(ctx, asset_id, bytes, &[], 384)
    }

    pub(super) fn workspace(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG))
            .show(ctx, |ui| {
                let workspace_width = ui.available_width();
                // Fit follows the available workspace until the user chooses
                // a manual zoom. Keep the chapter position below the shelf.
                if self.auto_fit || self.zoom <= 0.0 {
                    let zoom = fit_zoom(
                        workspace_width,
                        self.story.chapters[self.chapter].width,
                        self.show_script,
                    );
                    self.set_canvas_zoom(zoom, true);
                }
                let viewport_rect = ui.available_rect_before_wrap();
                let toolbar_height = if matches!(
                    self.tool,
                    Tool::Brush
                        | Tool::Heal
                        | Tool::Blur
                        | Tool::Text
                        | Tool::Lasso
                        | Tool::SmartLasso
                        | Tool::Select
                        | Tool::Crop
                ) {
                    122.0
                } else {
                    86.0
                };
                let toolbar_width = (workspace_width - 16.0).clamp(300.0, 720.0);
                self.toolbar_rect = Rect::from_min_size(
                    pos2(
                        viewport_rect.center().x - toolbar_width * 0.5,
                        viewport_rect.top() + 8.0,
                    ),
                    vec2(toolbar_width, toolbar_height),
                );
                let zoom = self.zoom;
                let width = self.story.chapters[self.chapter].width as f32;
                let height = self.story.chapters[self.chapter].height as f32;
                let script_width = if self.show_script { 300.0 } else { 0.0 };
                let area = egui::ScrollArea::both()
                    .id_salt(("canvas", self.story.chapters[self.chapter].id))
                    .auto_shrink([false, false])
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible);
                let area = if let Some(y) = self.jump_y.take() {
                    area.vertical_scroll_offset(y * zoom)
                } else {
                    area
                };
                let area = if self.auto_fit {
                    area.horizontal_scroll_offset(0.0)
                } else {
                    area
                };
                area.show_viewport(ui, |ui, viewport| {
                    #[cfg(test)]
                    ctx.data_mut(|data| {
                        data.insert_temp(egui::Id::new("canvas_scroll_bounds"), ui.clip_rect())
                    });
                    self.scroll_y = viewport.top() / zoom;
                    let group_width = width * zoom + 72.0 + script_width;
                    let center_offset = ((viewport.width() - group_width) * 0.5).max(0.0);
                    let content_height = if self.show_script {
                        (height * zoom).max(self.script_lane_height())
                    } else {
                        height * zoom
                    };
                    let (stage, _) = ui.allocate_exact_size(
                        vec2(group_width.max(viewport.width()), content_height + 196.0),
                        Sense::hover(),
                    );
                    let canvas = Rect::from_min_size(
                        // Reserve the largest tool shelf so changing tools
                        // never shifts artwork under the pointer.
                        stage.min + vec2(42.0 + center_offset, 158.0),
                        vec2(width * zoom, height * zoom),
                    );
                    self.canvas_rect = canvas;
                    let painter = ui.painter().clone();
                    painter.rect_filled(
                        canvas.translate(vec2(5.0, 7.0)),
                        0,
                        Color32::from_black_alpha(80),
                    );
                    painter.rect_filled(canvas, 0, Color32::WHITE);
                    painter.text(
                        canvas.left_top() - vec2(0.0, 17.0),
                        Align2::LEFT_CENTER,
                        format!("{} px", width as u32),
                        FontId::proportional(10.0),
                        MUTED,
                    );
                    if self.show_script {
                        painter.text(
                            pos2(canvas.right() + 28.0, canvas.top() - 17.0),
                            Align2::LEFT_CENTER,
                            "SCRIPT",
                            FontId::proportional(11.0),
                            ACCENT,
                        );
                    }
                    let visible_top = ((ui.clip_rect().top() - canvas.top()) / zoom).max(0.0);
                    let visible_bottom =
                        ((ui.clip_rect().bottom() - canvas.top()) / zoom).min(height);
                    for y in
                        ((visible_top as u32 / 200) * 200..=(visible_bottom as u32)).step_by(200)
                    {
                        let sy = canvas.top() + y as f32 * zoom;
                        painter.text(
                            pos2(canvas.left() - 9.0, sy),
                            Align2::RIGHT_CENTER,
                            format!("{y}"),
                            FontId::monospace(9.0),
                            MUTED,
                        );
                        painter.line_segment(
                            [pos2(canvas.left() - 5.0, sy), pos2(canvas.left(), sy)],
                            Stroke::new(1.0_f32, MUTED),
                        );
                    }
                    let layers = &self.story.chapters[self.chapter].layers;
                    for layer in layers {
                        if !layer.visible {
                            continue;
                        }
                        let rendered = scroll_studio::bubbles::joined_layer(
                            layer,
                            &self.story.chapters[self.chapter].layers,
                        );
                        Self::paint_layer(
                            &mut self.previews,
                            &self.assets,
                            ctx,
                            &painter.with_clip_rect(ui.clip_rect().intersect(canvas)),
                            &rendered,
                            canvas,
                            zoom,
                        );
                    }
                    if layers.is_empty() {
                        let center = pos2(canvas.center().x, canvas.top() + 150.0);
                        let icon = Rect::from_center_size(center, vec2(48.0, 36.0));
                        let ink = Color32::from_gray(175);
                        painter.rect_stroke(icon, 5, Stroke::new(1.5_f32, ink), StrokeKind::Inside);
                        painter.circle_filled(icon.left_top() + vec2(35.0, 10.0), 3.0, ink);
                        painter.line_segment(
                            [
                                icon.left_bottom() + vec2(5.0, -6.0),
                                icon.left_top() + vec2(17.0, 15.0),
                            ],
                            Stroke::new(1.5_f32, ink),
                        );
                        painter.line_segment(
                            [
                                icon.left_top() + vec2(17.0, 15.0),
                                icon.right_bottom() - vec2(5.0, 6.0),
                            ],
                            Stroke::new(1.5_f32, ink),
                        );
                        painter.text(
                            center + vec2(0.0, 38.0),
                            Align2::CENTER_CENTER,
                            "Drop image",
                            FontId::proportional(14.0),
                            Color32::from_gray(145),
                        );
                    }
                    let response = ui.interact(
                        canvas,
                        egui::Id::new("canvas_input"),
                        Sense::click_and_drag(),
                    );
                    if response.hovered() {
                        ctx.set_cursor_icon(match self.tool {
                            Tool::Move => egui::CursorIcon::Default,
                            Tool::Text => egui::CursorIcon::Text,
                            _ => egui::CursorIcon::Crosshair,
                        });
                        if self.tool == Tool::Move
                            && self.story.chapters[self.chapter]
                                .layers
                                .iter()
                                .any(|layer| {
                                    Some(layer.id) == self.selected
                                        && layer.visible
                                        && !layer.locked
                                        && pointer_targets(layer).iter().any(|(_, tip)| {
                                            ctx.input(|i| i.pointer.hover_pos()).is_some_and(|p| {
                                                p.distance(canvas.min + vec2(tip[0], tip[1]) * zoom)
                                                    <= 12.0
                                            })
                                        })
                                })
                        {
                            ctx.set_cursor_icon(egui::CursorIcon::Grab);
                        }
                    }
                    if self.drag_tail.is_some() {
                        ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
                    }
                    if let Some(payload) = response.dnd_release_payload::<u64>() {
                        let point = ctx
                            .input(|i| i.pointer.hover_pos())
                            .map(|p| [(p.x - canvas.left()) / zoom, (p.y - canvas.top()) / zoom]);
                        if !ctx.input(|i| i.pointer.hover_pos()).is_some_and(|p| {
                            self.toolbar_rect.contains(p) || self.reference_contains(p)
                        }) {
                            self.place_asset(*payload, point);
                        }
                    }
                    // Areas have a previous-frame hit map. Guard new/expanded
                    // toolbar bounds too, without interrupting a drag that
                    // began on the artwork and releases over the shelf.
                    let began_on_toolbar = ctx
                        .input(|i| {
                            i.pointer.press_origin().or_else(|| {
                                i.events.iter().find_map(|event| match event {
                                    egui::Event::PointerButton {
                                        pos, pressed: true, ..
                                    } => Some(*pos),
                                    _ => None,
                                })
                            })
                        })
                        .is_some_and(|point| {
                            self.toolbar_rect.contains(point) || self.reference_contains(point)
                        });
                    let add_bubble = self.story.chapters[self.chapter]
                        .layers
                        .iter()
                        .find(|l| {
                            Some(l.id) == self.selected
                                && l.visible
                                && !l.locked
                                && matches!(l.kind, LayerKind::Text { bubble: true, .. })
                                && self.tool == Tool::Move
                        })
                        .map(|l| {
                            let bounds = layer_rect(l, canvas, zoom);
                            (
                                l.id,
                                Rect::from_center_size(
                                    bounds.right_center() + vec2(23.0, 0.0),
                                    vec2(28.0, 28.0),
                                ),
                            )
                        })
                        .filter(|(_, rect)| {
                            ui.clip_rect().contains_rect(*rect)
                                && !rect.intersects(self.toolbar_rect)
                        });
                    let mut add_clicked = None;
                    let began_on_add = add_bubble.is_some_and(|(_, rect)| {
                        ctx.input(|i| i.pointer.press_origin().or(i.pointer.interact_pos()))
                            .is_some_and(|p| rect.contains(p))
                    });
                    if let Some((id, rect)) = add_bubble {
                        painter.circle_filled(rect.center(), 13.0, ACCENT);
                        painter.text(
                            rect.center(),
                            Align2::CENTER_CENTER,
                            "+",
                            FontId::proportional(21.0),
                            BG,
                        );
                        let add = ui
                            .interact(
                                rect,
                                ui.id().with(("add_linked_bubble", id)),
                                Sense::click(),
                            )
                            .on_hover_text("Add linked bubble");
                        #[cfg(test)]
                        ctx.data_mut(|data| {
                            data.insert_temp(egui::Id::new("add_linked_bubble_bounds"), rect)
                        });
                        if add.clicked() {
                            add_clicked = Some(id);
                        }
                    }
                    if let Some(id) = add_clicked {
                        self.add_linked_bubble(id);
                    }
                    if !began_on_toolbar && !began_on_add {
                        self.canvas_input(ctx, &response, canvas);
                    }
                    if let Some(layer) = self.story.chapters[self.chapter]
                        .layers
                        .iter()
                        .find(|l| Some(l.id) == self.selected && l.visible)
                    {
                        let rect = layer_rect(layer, canvas, zoom);
                        painter.rect_stroke(
                            rect,
                            0,
                            Stroke::new(1.5_f32, ACCENT),
                            StrokeKind::Outside,
                        );
                        for corner in [
                            rect.left_top(),
                            rect.right_top(),
                            rect.left_bottom(),
                            rect.right_bottom(),
                        ] {
                            painter.rect_filled(
                                Rect::from_center_size(corner, vec2(6.0, 6.0)),
                                1,
                                ACCENT,
                            );
                        }
                        if !layer.locked {
                            for (index, tip) in pointer_targets(layer) {
                                let tip = canvas.min + vec2(tip[0], tip[1]) * zoom;
                                painter.circle_filled(tip, 6.0, ACCENT);
                                painter.circle_stroke(tip, 8.0, Stroke::new(1.0_f32, BG));
                                painter.text(
                                    tip,
                                    Align2::CENTER_CENTER,
                                    (index + 1).to_string(),
                                    FontId::proportional(9.0),
                                    BG,
                                );
                                if self.drag_tail == Some((layer.id, index))
                                    || ctx
                                        .input(|i| i.pointer.hover_pos())
                                        .is_some_and(|p| p.distance(tip) <= 12.0)
                                {
                                    painter.text(
                                        tip + vec2(12.0, -12.0),
                                        Align2::LEFT_CENTER,
                                        format!("Pointer {}", index + 1),
                                        FontId::proportional(11.0),
                                        ACCENT,
                                    );
                                }
                            }
                        }
                    }
                    self.paint_snap_guides(
                        &painter.with_clip_rect(ui.clip_rect().intersect(canvas)),
                        canvas,
                    );
                    self.paint_selection_outline(&painter, canvas, zoom);
                    if matches!(self.tool, Tool::Brush | Tool::Heal | Tool::Blur)
                        && response.hovered()
                    {
                        if let Some(p) = ctx
                            .pointer_hover_pos()
                            .filter(|p| canvas.contains(*p) && !self.toolbar_rect.contains(*p))
                        {
                            let radius = (self.active_brush_size() * zoom * 0.5).max(2.0);
                            painter.circle_stroke(
                                p,
                                radius,
                                Stroke::new(2.5_f32, Color32::from_black_alpha(160)),
                            );
                            painter.circle_stroke(
                                p,
                                radius,
                                Stroke::new(1.0_f32, Color32::from_rgb(175, 212, 250)),
                            );
                        }
                    }
                    if self.lasso.len() > 1 {
                        let points: Vec<_> = self
                            .lasso
                            .iter()
                            .map(|p| canvas.min + vec2(p[0], p[1]) * zoom)
                            .collect();
                        painter.add(egui::Shape::line(
                            points,
                            if matches!(self.tool, Tool::Heal | Tool::Blur) {
                                Stroke::new(
                                    self.active_brush_size() * zoom,
                                    Color32::from_rgba_unmultiplied(122, 184, 245, 100),
                                )
                            } else {
                                Stroke::new(2.0_f32, Color32::from_rgb(243, 183, 108))
                            },
                        ));
                    }
                    if self.show_script {
                        self.script_timeline(ui, canvas);
                    }
                    if !began_on_toolbar {
                        self.register_context(&response);
                        if response.secondary_clicked() {
                            if let Some(pointer) = response.interact_pointer_pos() {
                                let point = [
                                    (pointer.x - canvas.left()) / zoom,
                                    (pointer.y - canvas.top()) / zoom,
                                ];
                                let layer = context_layer_at(
                                    &self.story.chapters[self.chapter].layers,
                                    point,
                                );
                                self.selected = layer;
                                self.selected_asset = None;
                                self.context.canvas = Some(super::context_ui::CanvasContext {
                                    chapter: self.story.chapters[self.chapter].id,
                                    layer,
                                    point,
                                });
                            }
                        }
                        appearance::context_menu(&response, |ui| self.canvas_context_actions(ui));
                    }
                });
                self.floating_canvas_toolbar(ctx, viewport_rect, workspace_width);
            });
    }

    fn floating_canvas_toolbar(
        &mut self,
        ctx: &egui::Context,
        viewport_rect: Rect,
        workspace_width: f32,
    ) {
        let rect = self.toolbar_rect;
        egui::Area::new(egui::Id::new("canvas_tool_shelf"))
            .order(egui::Order::Middle)
            .fixed_pos(rect.min)
            .default_size(rect.size())
            .fade_in(false)
            .constrain_to(viewport_rect)
            .movable(false)
            .show(ctx, |ui| {
                ui.set_width(rect.width());
                if self.preferences.glass_enabled {
                    self.glass.paint(
                        ui.painter(),
                        rect,
                        14.0,
                        super::appearance::glass_tint(),
                        self.preferences.glass_strength,
                    );
                } else {
                    ui.painter().rect_filled(rect, 14.0, PANEL);
                    ui.painter().rect_stroke(
                        rect,
                        14.0,
                        Stroke::new(1.0_f32, Color32::from_gray(65)),
                        StrokeKind::Inside,
                    );
                }
                egui::Frame::new().inner_margin(10.0).show(ui, |ui| {
                    egui::Frame::new()
                        .fill(Color32::TRANSPARENT)
                        .inner_margin(0.0)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 4.0;
                                ui.spacing_mut().button_padding.x = 7.0;
                                for (tool, icon, name, tip) in [
                                    (
                                        Tool::Move,
                                        icons::Icon::Move,
                                        "Move",
                                        "V · Select and drag a layer",
                                    ),
                                    (Tool::Select, icons::Icon::Select,"Selection","M - Select an image area"),
                                    (Tool::Crop, icons::Icon::Crop,"Crop","C - Crop an image area; use Edge for contour styles"),
                                    (
                                        Tool::Lasso,
                                        icons::Icon::Lasso,
                                        "Lasso",
                                        "L · Draw around what to keep on the selected image",
                                    ),
                                    (
                                        Tool::SmartLasso,
                                        icons::Icon::SmartLasso,
                                        "Smart Lasso",
                                        "W · Click a connected area · Shift adds · Left Alt subtracts",
                                    ),
                                    (
                                        Tool::Brush,
                                        icons::Icon::Brush,
                                        "Brush",
                                        "B · Draw directly on your canvas",
                                    ),
                                    (Tool::Heal,icons::Icon::Heal,"Content-Aware Brush","J - Paint over an object to blend it into its surroundings"),
                                    (
                                        Tool::Text,
                                        icons::Icon::Bubble,
                                        "Bubble",
                                        "T · Click to add dialogue, then aim its pointers",
                                    ),
                                    (
                                        Tool::Panel,
                                        icons::Icon::Beat,
                                        "Beat",
                                        "P · Click to add a script beat at that position",
                                    ),
                                ] {
                                    if icons::button(ui, icon, self.tool == tool, name)
                                        .on_hover_text(tip)
                                        .clicked()
                                    {
                                        self.tool = tool;
                                    }
                                }
                                ui.separator();
                                if ui
                                    .add_enabled_ui(!self.undo.is_empty(), |ui| {
                                        icons::button(ui, icons::Icon::Undo, false, "Undo")
                                    })
                                    .inner
                                    .on_hover_text("Undo · Ctrl+Z")
                                    .clicked()
                                {
                                    self.undo();
                                }
                                if ui
                                    .add_enabled_ui(!self.redo.is_empty(), |ui| {
                                        icons::button(ui, icons::Icon::Redo, false, "Redo")
                                    })
                                    .inner
                                    .on_hover_text("Redo · Ctrl+Shift+Z")
                                    .clicked()
                                {
                                    self.redo();
                                }
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui
                                            .add(motion::Button::new("Fit").selected(self.auto_fit))
                                            .clicked()
                                        {
                                            self.set_canvas_zoom(
                                                fit_zoom(
                                                    workspace_width,
                                                    self.story.chapters[self.chapter].width,
                                                    self.show_script,
                                                ),
                                                true,
                                            );
                                        }
                                        if motion::small_button(ui, "+").clicked() {
                                            self.set_canvas_zoom(
                                                (self.zoom + 0.05).min(1.5),
                                                false,
                                            );
                                        }
                                        ui.label(
                                            RichText::new(format!("{:.0}%", self.zoom * 100.0))
                                                .size(12.0),
                                        );
                                        if motion::small_button(ui, "−").clicked() {
                                            self.set_canvas_zoom(
                                                (self.zoom - 0.05).max(0.02),
                                                false,
                                            );
                                        }
                                    },
                                );
                            });
                        });
                    ui.horizontal(|ui| {
                        ui.add_space(16.0);
                        ui.add_sized(
                            [(ui.available_width() - 390.0).max(40.0), 22.0],
                            egui::Label::new(
                                RichText::new(&self.story.chapters[self.chapter].name)
                                    .size(14.0)
                                    .strong(),
                            )
                            .truncate(),
                        )
                        .on_hover_text(&self.story.chapters[self.chapter].name);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            motion::menu_button(ui,match self.tool {Tool::Glint=>"Glint",Tool::Wind=>"Wind",Tool::Blur=>"Blur",_=>"Effects"},|ui|{
                                for (tool,label) in [(Tool::Glint,"Glint"),(Tool::Wind,"Wind"),(Tool::Blur,"Motion Blur")] {if ui.selectable_label(self.tool==tool,label).clicked(){self.tool=tool;ui.close_menu();}}
                            });
                            motion::menu_button(ui,"Snips",|ui|{
                                if motion::button(ui,"Background snippet").clicked(){self.begin_snippet(ctx,super::snippet_ui::SnippetKind::Background);ui.close_menu();}
                                if motion::button(ui,"Character snippet").clicked(){self.begin_snippet(ctx,super::snippet_ui::SnippetKind::Character);ui.close_menu();}
                            });
                            let read = motion::button(ui, "Read");
                            #[cfg(test)]
                            ctx.data_mut(|data| {
                                data.insert_temp(egui::Id::new("reader_toolbar_bounds"), read.rect)
                            });
                            if read.clicked() {
                                self.begin_reader();
                            }
                            ui.checkbox(&mut self.show_script, "Script");
                            let settings = motion::menu_button(ui, "  ", |ui| {
                                ui.checkbox(&mut self.snap_settings.canvas, "Canvas");
                                ui.checkbox(&mut self.snap_settings.objects, "Objects");
                                ui.checkbox(&mut self.snap_settings.spacing, "Equal spacing");
                                ui.separator();
                                ui.add(
                                    egui::Slider::new(
                                        &mut self.snap_settings.threshold_px,
                                        3.0..=16.0,
                                    )
                                    .text("Strength"),
                                );
                            })
                            .response;
                            icons::draw(
                                ui.painter(),
                                settings.rect.shrink(5.0),
                                icons::Icon::ChevronDown,
                                TEXT,
                            );
                            settings.widget_info(|| {
                                egui::WidgetInfo::labeled(
                                    egui::WidgetType::Button,
                                    ui.is_enabled(),
                                    "Snap settings",
                                )
                            });
                            settings.on_hover_text("Snap settings");
                            if snap_button(ui, &mut self.snap_settings.enabled).changed() {
                                self.snap_guides.clear();
                            }
                        });
                    });
                    if matches!(self.tool,Tool::Lasso|Tool::SmartLasso|Tool::Select|Tool::Crop) {
                        self.lasso_controls(ui);
                    } else if matches!(self.tool,Tool::Brush|Tool::Heal|Tool::Blur) {
                        ui.horizontal(|ui| {
                            ui.add_space(16.0);
                            if self.tool==Tool::Brush {ui.color_edit_button_srgba(&mut self.brush_color);}
                            ui.add(
                                egui::Slider::new(match self.tool {Tool::Heal=>&mut self.heal_brush_size,Tool::Blur=>&mut self.motion_brush.size,_=>&mut self.brush_size}, 1.0..=if self.tool == Tool::Brush {80.0} else {256.0})
                                    .text("Brush size"),
                            );
                            ui.spacing_mut().slider_width=85.0;
                            if self.tool==Tool::Blur {
                                ui.add(egui::Slider::new(&mut self.motion_brush.length,1.0..=128.0).text("Blur"));
                                motion::menu_button(ui,"Direction",|ui|{ui.checkbox(&mut self.motion_brush.automatic,"Follow stroke");if !self.motion_brush.automatic {ui.add(egui::Slider::new(&mut self.motion_brush.angle,-180.0..=180.0).suffix("°"));}});
                            }else{ui.add(egui::Slider::new(&mut self.lasso_state.stabilizer,0.0..=1.0).show_value(false).text("Stabilizer"));}
                        });
                    } else if self.tool == Tool::Text {
                        ui.horizontal_wrapped(|ui| {
                            ui.add_space(16.0);
                            ui.label("Preset");
                            let mut choice = self
                                .story
                                .bubble_presets
                                .iter()
                                .find(|preset| Some(preset.id) == self.selected_bubble_preset)
                                .or_else(|| self.story.bubble_presets.first())
                                .map(|preset| preset.id);
                            egui::ComboBox::from_id_salt("new_bubble_preset")
                                .width(180.0)
                                .selected_text(
                                    self.story
                                        .bubble_presets
                                        .iter()
                                        .find(|preset| Some(preset.id) == choice)
                                        .map_or("Choose a preset", |preset| preset.name.as_str()),
                                )
                                .show_ui(ui, |ui| {
                                    for preset in &self.story.bubble_presets {
                                        ui.selectable_value(
                                            &mut choice,
                                            Some(preset.id),
                                            &preset.name,
                                        );
                                    }
                                });
                            self.selected_bubble_preset = choice;
                            if motion::button(ui, "Bubbles")
                                .on_hover_text("Edit this story’s bubble presets")
                                .clicked()
                            {
                                self.tab = LibraryTab::Bubbles;
                            }
                        });
                    }
                });
            });
    }

    pub(super) fn paint_layer(
        previews: &mut super::previews::PreviewCache,
        assets: &AssetBytes,
        ctx: &egui::Context,
        painter: &egui::Painter,
        layer: &Layer,
        canvas: Rect,
        zoom: f32,
    ) {
        let rect = render_rect(layer, canvas, zoom);
        if layer.opacity <= 0.0 || !rect.intersects(painter.clip_rect()) {
            return;
        }
        let rgba = |c| super::artwork_color::color(c, layer.opacity);
        match &layer.kind {
            LayerKind::Image {
                asset_id,
                mask,
                clip,
                source_rect,
            } => {
                let texture = assets.get(asset_id).and_then(|source| {
                    let source_size = previews.source_size(*asset_id, source)?;
                    let view = previews::ImageViewport {
                        source_size,
                        image: rect,
                        clip: painter.clip_rect(),
                        mirrored: false,
                    };
                    if let Some(crop) = source_rect {
                        previews.source_crop_viewer(
                            ctx,
                            *asset_id,
                            source,
                            clip.as_ref(),
                            *crop,
                            view,
                        )
                    } else if let Some(clip) = clip {
                        previews.clipped_viewer(ctx, *asset_id, source, clip, view)
                    } else if mask.is_empty() {
                        previews.viewer_image(ctx, *asset_id, source, view)
                    } else {
                        // Legacy polygons remain readable with their original appearance.
                        let side = (rect.size().max_elem() * ctx.pixels_per_point())
                            .ceil()
                            .clamp(1.0, 2048.0) as u32;
                        previews
                            .image(ctx, *asset_id, source, mask, side)
                            .map(|texture| previews::viewer::ViewerImage {
                                texture,
                                rect,
                                mirrored: false,
                            })
                    }
                });
                if let Some(texture) = texture {
                    texture
                        .paint_tinted(painter, super::artwork_color::opacity_tint(layer.opacity));
                } else {
                    preview_placeholder(painter, rect, "Image preview…");
                }
            }

            LayerKind::Effect { effect } => {
                for line in effect.lines(layer.width, layer.height) {
                    let points = line
                        .points
                        .iter()
                        .map(|p| canvas.min + vec2(layer.x + p[0], layer.y + p[1]) * zoom)
                        .collect();
                    painter.add(egui::Shape::line(
                        points,
                        Stroke::new(line.width * zoom, rgba(line.color)),
                    ));
                }
            }
            LayerKind::Shape { color } => {
                painter.rect_filled(rect, 0, rgba(*color));
            }
            LayerKind::Stroke {
                points,
                color,
                size,
            } => {
                let pts: Vec<_> = points
                    .iter()
                    .map(|p| canvas.min + vec2(p[0] + layer.x, p[1] + layer.y) * zoom)
                    .collect();
                if pts.len() > 1 {
                    painter.add(egui::Shape::line(
                        pts,
                        Stroke::new(*size * zoom, rgba(*color)),
                    ));
                } else if let Some(point) = pts.first() {
                    painter.circle_filled(*point, *size * zoom * 0.5, rgba(*color));
                }
            }
            LayerKind::Text { .. } => {
                if let Some(texture) = previews.text(ctx, layer, zoom * ctx.pixels_per_point()) {
                    texture.paint(painter, rect);
                } else {
                    preview_placeholder(painter, rect, "Text preview…");
                }
            }
        }
    }

    fn canvas_input(&mut self, ctx: &egui::Context, response: &egui::Response, canvas: Rect) {
        if self.tool != Tool::Move
            || self.snap_drag.as_ref().is_some_and(|drag| {
                drag.chapter_id != self.story.chapters[self.chapter].id
                    || Some(drag.layer_id) != self.drag_layer
            })
        {
            self.snap_drag = None;
            self.snap_guides.clear();
        }
        let point = response.interact_pointer_pos().map(|p| {
            [
                (p.x - canvas.left()) / self.zoom,
                (p.y - canvas.top()) / self.zoom,
            ]
        });
        let hit = |layers: &[Layer], p: [f32; 2]| {
            layers
                .iter()
                .rev()
                .find(|l| {
                    if !l.visible || l.locked {
                        return false;
                    }
                    if matches!(l.kind, LayerKind::Text { .. }) {
                        return compositor::text_hit_test(l, p);
                    }
                    let r = layer_rect(l, Rect::from_min_size(pos2(0.0, 0.0), vec2(1.0, 1.0)), 1.0);
                    if !r.contains(pos2(p[0], p[1])) {
                        return false;
                    }
                    if let LayerKind::Image { mask, clip, .. } = &l.kind {
                        if let Some(clip) = clip {
                            let uv = super::lasso_ui::source_uv(
                                l,
                                [(p[0] - l.x) / l.width, (p[1] - l.y) / l.height],
                            );
                            return clip.selection.contains_uv(uv[0], uv[1]);
                        }
                        mask.is_empty()
                            || compositor::point_in_polygon(
                                (p[0] - l.x) / l.width,
                                (p[1] - l.y) / l.height,
                                mask,
                            )
                    } else {
                        true
                    }
                })
                .map(|l| l.id)
        };
        if response.clicked() {
            if let Some(p) = point {
                match self.tool {
                    Tool::Move | Tool::Lasso | Tool::Select | Tool::Crop => {
                        self.selected = hit(&self.story.chapters[self.chapter].layers, p);
                        self.selected_asset = None;
                    }
                    Tool::SmartLasso => self.smart_select(ctx, p),
                    Tool::Glint => self.add_glint(p),
                    Tool::Wind => self.add_wind(&[p]),
                    Tool::Blur => self.start_motion_blur(ctx, vec![p]),
                    Tool::Heal => self.start_healing(
                        ctx,
                        super::healing_ui::HealInput::Stroke(vec![p], self.heal_brush_size),
                    ),
                    Tool::Text => {
                        let Some(preset) = self
                            .story
                            .bubble_presets
                            .iter()
                            .find(|preset| Some(preset.id) == self.selected_bubble_preset)
                            .or_else(|| self.story.bubble_presets.first())
                            .cloned()
                        else {
                            self.status =
                                "Create a preset in Bubbles before adding dialogue.".into();
                            return;
                        };
                        self.checkpoint();
                        let id = self.story.alloc_id();
                        let mut layer = Layer {
                            id,
                            name: "Dialogue".into(),
                            visible: true,
                            locked: false,
                            opacity: 1.0,
                            x: p[0],
                            y: p[1],
                            width: 420.0,
                            height: 160.0,
                            kind: LayerKind::Text {
                                text: "Your dialogue…".into(),
                                color: [28, 31, 38, 255],
                                size: 38.0,
                                bubble: true,
                                style: TextStyle::dialogue(),
                            },
                        };
                        if let Err(error) =
                            scroll_studio::bubbles::initialize_layer(&mut layer, &preset)
                        {
                            self.error = Some(error.to_string());
                            return;
                        }
                        // A new bubble begins where the user clicked; fitting an
                        // existing bubble instead preserves its body center.
                        layer.x = p[0];
                        layer.y = p[1];
                        self.selected_bubble_preset = Some(preset.id);
                        self.story.chapters[self.chapter].layers.push(layer);
                        self.selected = Some(id);
                        self.selected_asset = None;
                        self.tool = Tool::Move;
                        self.changed();
                    }
                    Tool::Panel => self.add_beat(p[1]),
                    Tool::Brush => {
                        self.begin_stroke(p);
                        self.drag_layer = None;
                    }
                }
            }
        }
        if response.drag_started_by(egui::PointerButton::Primary) {
            let origin = ctx.input(|i| i.pointer.press_origin()).map(|p| {
                [
                    (p.x - canvas.left()) / self.zoom,
                    (p.y - canvas.top()) / self.zoom,
                ]
            });
            if let Some(p) = origin.or(point) {
                match self.tool {
                    Tool::Move => {
                        self.resize_anchor = None;
                        self.drag_tail = None;
                        self.drag_layer = None;
                        self.snap_drag = None;
                        self.snap_guides.clear();
                        if let Some(layer) =
                            self.story.chapters[self.chapter].layers.iter().find(|l| {
                                Some(l.id) == self.selected
                                    && l.visible
                                    && !l.locked
                                    && !matches!(l.kind, LayerKind::Stroke { .. })
                            })
                        {
                            if let Some((index, _)) = pointer_targets(layer)
                                .into_iter()
                                .map(|(index, tip)| {
                                    (index, (tip[0] - p[0]).hypot(tip[1] - p[1]) * self.zoom)
                                })
                                .filter(|(_, distance)| *distance <= 12.0)
                                .min_by(|a, b| a.1.total_cmp(&b.1))
                            {
                                self.drag_tail = Some((layer.id, index));
                            }
                            if self.drag_tail.is_none() {
                                for (corner, opposite) in [
                                    (
                                        [layer.x, layer.y],
                                        [layer.x + layer.width, layer.y + layer.height],
                                    ),
                                    (
                                        [layer.x + layer.width, layer.y],
                                        [layer.x, layer.y + layer.height],
                                    ),
                                    (
                                        [layer.x, layer.y + layer.height],
                                        [layer.x + layer.width, layer.y],
                                    ),
                                    (
                                        [layer.x + layer.width, layer.y + layer.height],
                                        [layer.x, layer.y],
                                    ),
                                ] {
                                    if (corner[0] - p[0]).hypot(corner[1] - p[1]) * self.zoom < 12.0
                                    {
                                        self.resize_anchor =
                                            Some((layer.id, opposite, layer.width / layer.height));
                                    }
                                }
                            }
                        }
                        if self.drag_tail.is_some() || self.resize_anchor.is_some() {
                            self.checkpoint();
                        } else {
                            self.selected = hit(&self.story.chapters[self.chapter].layers, p);
                            self.selected_asset = None;
                            self.drag_layer = self.selected;
                            if self.selected.is_some() {
                                self.checkpoint();
                                self.snap_drag = self.start_snap_drag(p, response, canvas);
                            }
                        }
                    }
                    Tool::Lasso | Tool::Select | Tool::Crop => {
                        let valid = self.story.chapters[self.chapter].layers.iter().any(|l| {
                            Some(l.id) == self.selected
                                && !l.locked
                                && matches!(l.kind, LayerKind::Image { .. })
                        });
                        if !valid {
                            self.selected = self.story.chapters[self.chapter]
                                .layers
                                .iter()
                                .rev()
                                .find(|l| {
                                    l.visible
                                        && !l.locked
                                        && matches!(l.kind, LayerKind::Image { .. })
                                        && p[0] >= l.x
                                        && p[0] < l.x + l.width
                                        && p[1] >= l.y
                                        && p[1] < l.y + l.height
                                })
                                .map(|l| l.id);
                        }
                        self.lasso_state.mode =
                            super::lasso_ui::selection_mode(ctx.input(|i| i.modifiers));
                        self.lasso = vec![p];
                    }
                    Tool::Brush => self.begin_stroke(p),
                    Tool::Heal | Tool::Blur | Tool::Wind => {
                        self.lasso = vec![p];
                    }
                    _ => (),
                }
            }
        }
        if response.dragged_by(egui::PointerButton::Primary) {
            if let Some(p) = point {
                match self.tool {
                    Tool::Move => {
                        if let Some((id, index)) = self.drag_tail {
                            if let Some(layer) = self.story.chapters[self.chapter]
                                .layers
                                .iter_mut()
                                .find(|l| l.id == id && l.visible && !l.locked)
                            {
                                if let LayerKind::Text { style, bubble, .. } = &mut layer.kind {
                                    if *bubble && style.shape != BubbleShape::Narration {
                                        if let Some(tail) =
                                            style.tail_at_mut(index).filter(|tail| tail.enabled)
                                        {
                                            tail.tip = [
                                                (p[0] - layer.x)
                                                    .clamp(-MAX_POINTER_OFFSET, MAX_POINTER_OFFSET)
                                                    / layer.width,
                                                (p[1] - layer.y)
                                                    .clamp(-MAX_POINTER_OFFSET, MAX_POINTER_OFFSET)
                                                    / layer.height,
                                            ];
                                            self.changed();
                                        }
                                    }
                                }
                            }
                            return;
                        }
                        if let Some((id, anchor, ratio)) = self.resize_anchor {
                            if let Some(layer) = self.story.chapters[self.chapter]
                                .layers
                                .iter_mut()
                                .find(|l| l.id == id && l.visible && !l.locked)
                            {
                                let pointers = pointer_targets(layer);
                                let mut size = [
                                    (p[0] - anchor[0]).abs().max(1.0),
                                    (p[1] - anchor[1]).abs().max(1.0),
                                ];
                                if ctx.input(|i| i.modifiers.shift) {
                                    size[1] = size[0] / ratio;
                                }
                                layer.width = size[0].min(16384.0);
                                layer.height = size[1].min(200000.0);
                                layer.x = if p[0] < anchor[0] {
                                    anchor[0] - layer.width
                                } else {
                                    anchor[0]
                                };
                                layer.y = if p[1] < anchor[1] {
                                    anchor[1] - layer.height
                                } else {
                                    anchor[1]
                                };
                                if let LayerKind::Text { style, .. } = &mut layer.kind {
                                    style.auto_fit = false;
                                }
                                preserve_pointer_targets(&pointers, layer);
                            }
                            self.changed();
                            return;
                        }
                        self.move_snapped_layer(ctx, p, response, canvas);
                    }
                    Tool::Select | Tool::Crop => {
                        if let Some(start) = self.lasso.first().copied() {
                            self.lasso = vec![start, [p[0], start[1]], p, [start[0], p[1]], start];
                        }
                    }
                    Tool::Lasso | Tool::Heal | Tool::Blur | Tool::Wind => {
                        let p = self.lasso.last().map_or(p, |last| {
                            scroll_studio::clipping::stabilize(
                                *last,
                                p,
                                self.lasso_state.stabilizer,
                                self.zoom,
                            )
                        });
                        if self.lasso.last().is_none_or(|last| {
                            (last[0] - p[0]).hypot(last[1] - p[1]) > 4.0 / self.zoom
                        }) && self.lasso.len() < 2000
                        {
                            self.lasso.push(p);
                        }
                    }
                    Tool::Brush => {
                        if let Some(l) = self.story.chapters[self.chapter]
                            .layers
                            .iter_mut()
                            .find(|l| Some(l.id) == self.drag_layer)
                        {
                            if let LayerKind::Stroke { points, .. } = &mut l.kind {
                                let p = points.last().map_or(p, |last| {
                                    scroll_studio::clipping::stabilize(
                                        *last,
                                        p,
                                        self.lasso_state.stabilizer,
                                        self.zoom,
                                    )
                                });
                                if points.last().is_none_or(|last| {
                                    (last[0] - p[0]).hypot(last[1] - p[1]) > 1.0 / self.zoom
                                }) {
                                    points.push(p);
                                }
                            }
                            self.changed();
                        }
                    }
                    _ => (),
                }
            }
        }
        if response.drag_stopped_by(egui::PointerButton::Primary) {
            // A final pointer move and its release can share a frame. egui then
            // reports drag_stopped without dragged, so finish from that position
            // before discarding the gesture's unsnapped origin.
            if self.tool == Tool::Move && self.snap_drag.is_some() {
                if let Some(p) = point {
                    self.move_snapped_layer(ctx, p, response, canvas);
                }
            }
            if matches!(self.tool, Tool::Lasso | Tool::Select | Tool::Crop) {
                if matches!(self.tool, Tool::Select | Tool::Crop) {
                    if let (Some(start), Some(p)) = (self.lasso.first().copied(), point) {
                        self.lasso = vec![start, [p[0], start[1]], p, [start[0], p[1]]];
                    }
                }
                if self.tool == Tool::Lasso {
                    if let Some(p) = point {
                        if self.lasso.last() != Some(&p) && self.lasso.len() < 2000 {
                            self.lasso.push(p);
                        }
                    }
                }
                if self.lasso.len() >= 3 {
                    self.finish_lasso(ctx);
                }
            }
            if self.tool == Tool::Brush {
                if let (Some(p), Some(layer)) = (
                    point,
                    self.story.chapters[self.chapter]
                        .layers
                        .iter_mut()
                        .find(|l| Some(l.id) == self.drag_layer),
                ) {
                    if let LayerKind::Stroke { points, .. } = &mut layer.kind {
                        if points.last() != Some(&p) {
                            points.push(p);
                        }
                    }
                }
            }
            if self.tool == Tool::Blur && !self.lasso.is_empty() {
                if let Some(p) = point {
                    self.lasso.push(p);
                }
                self.start_motion_blur(ctx, self.lasso.clone());
            }
            if self.tool == Tool::Wind && !self.lasso.is_empty() {
                if let Some(p) = point {
                    self.lasso.push(p);
                }
                self.add_wind(&self.lasso.clone());
            }
            if self.tool == Tool::Heal && !self.lasso.is_empty() {
                if let Some(p) = point {
                    self.lasso.push(p);
                }
                self.start_healing(
                    ctx,
                    super::healing_ui::HealInput::Stroke(self.lasso.clone(), self.heal_brush_size),
                );
            }
            self.lasso.clear();
            self.drag_layer = None;
            self.drag_tail = None;
            self.resize_anchor = None;
            self.snap_drag = None;
            self.snap_guides.clear();
        }
    }

    fn start_snap_drag(
        &self,
        pointer_origin: [f32; 2],
        response: &egui::Response,
        canvas: Rect,
    ) -> Option<SnapDrag> {
        let chapter = &self.story.chapters[self.chapter];
        let layer = chapter
            .layers
            .iter()
            .find(|layer| Some(layer.id) == self.drag_layer && layer.visible && !layer.locked)?;
        let bounds = snapping::layer_bounds(layer)?;
        let viewport = snap_viewport(response.interact_rect, canvas, self.zoom);
        Some(SnapDrag {
            layer_id: layer.id,
            chapter_id: chapter.id,
            pointer_origin,
            layer_origin: [layer.x, layer.y],
            bounds,
            viewport,
            targets: visible_snap_targets(chapter, layer.id, viewport),
        })
    }

    fn move_snapped_layer(
        &mut self,
        ctx: &egui::Context,
        pointer: [f32; 2],
        response: &egui::Response,
        canvas: Rect,
    ) {
        self.snap_guides.clear();
        let Some(drag) = self.snap_drag.as_mut() else {
            return;
        };
        let chapter = &self.story.chapters[self.chapter];
        if !chapter
            .layers
            .iter()
            .any(|layer| layer.id == drag.layer_id && layer.visible && !layer.locked)
        {
            self.snap_drag = None;
            return;
        }
        let viewport = snap_viewport(response.interact_rect, canvas, self.zoom);
        // Bounds are collected once per gesture, and again only when the viewport
        // scrolls. The render loop never clones artwork or decodes images here.
        if viewport != drag.viewport {
            drag.targets = visible_snap_targets(chapter, drag.layer_id, viewport);
            drag.viewport = viewport;
        }
        let delta = [
            pointer[0] - drag.pointer_origin[0],
            pointer[1] - drag.pointer_origin[1],
        ];
        let mut correction = [0.0, 0.0];
        if self.snap_settings.enabled && !ctx.input(|input| input.modifiers.alt) {
            let snapped = snapping::snap_translation(
                drag.bounds.translated(delta),
                Bounds {
                    x: 0.0,
                    y: 0.0,
                    width: chapter.width as f32,
                    height: chapter.height as f32,
                },
                &drag.targets,
                self.zoom,
                &self.snap_settings,
            );
            correction = snapped.offset;
            self.snap_guides = snapped.guides;
        }
        let position = [
            drag.layer_origin[0] + delta[0] + correction[0],
            drag.layer_origin[1] + delta[1] + correction[1],
        ];
        let id = drag.layer_id;
        if let Some(layer) = self.story.chapters[self.chapter]
            .layers
            .iter_mut()
            .find(|layer| layer.id == id)
        {
            if [layer.x, layer.y] != position {
                layer.x = position[0];
                layer.y = position[1];
                self.changed();
                ctx.request_repaint();
            }
        }
    }

    fn paint_snap_guides(&self, painter: &egui::Painter, canvas: Rect) {
        let ink = Color32::from_rgb(220, 132, 244);
        let stroke = Stroke::new(1.0_f32, ink);
        let screen = |x: f32, y: f32| canvas.min + vec2(x, y) * self.zoom;
        for guide in &self.snap_guides {
            match *guide {
                Guide::Alignment {
                    axis,
                    position,
                    from,
                    to,
                } => {
                    let points = match axis {
                        Axis::X => [screen(position, from), screen(position, to)],
                        Axis::Y => [screen(from, position), screen(to, position)],
                    };
                    painter.line_segment(points, stroke);
                }
                Guide::Spacing {
                    axis,
                    start,
                    end,
                    cross,
                } => {
                    let (points, tick) = match axis {
                        Axis::X => ([screen(start, cross), screen(end, cross)], vec2(0.0, 4.0)),
                        Axis::Y => ([screen(cross, start), screen(cross, end)], vec2(4.0, 0.0)),
                    };
                    painter.line_segment(points, stroke);
                    for point in points {
                        painter.line_segment([point - tick, point + tick], stroke);
                    }
                    let center = points[0].lerp(points[1], 0.5);
                    if painter.clip_rect().contains(center) {
                        let label = format!("{:.0}", (end - start).abs());
                        let galley = painter.layout_no_wrap(label, FontId::proportional(11.0), ink);
                        let rect = Rect::from_center_size(center, galley.size() + vec2(8.0, 4.0));
                        painter.rect_filled(rect, 3.0, BG);
                        painter.galley(rect.min + vec2(4.0, 2.0), galley, ink);
                    }
                }
            }
        }
    }

    fn begin_stroke(&mut self, p: [f32; 2]) {
        self.checkpoint();
        let id = self.story.alloc_id();
        let ch = &mut self.story.chapters[self.chapter];
        ch.layers.push(Layer {
            id,
            name: "Brush stroke".into(),
            visible: true,
            locked: false,
            opacity: 1.0,
            x: 0.0,
            y: 0.0,
            width: ch.width as f32,
            height: ch.height as f32,
            kind: LayerKind::Stroke {
                points: vec![p],
                color: self.brush_color.to_array(),
                size: self.brush_size,
            },
        });
        self.drag_layer = Some(id);
        self.selected = Some(id);
        self.selected_asset = None;
        self.changed();
    }
}

fn context_layer_at(layers: &[Layer], point: [f32; 2]) -> Option<u64> {
    layers
        .iter()
        .rev()
        .find(|layer| {
            if !layer.visible {
                return false;
            }
            if matches!(layer.kind, LayerKind::Text { .. }) {
                return compositor::text_hit_test(layer, point);
            }
            if !layer_rect(
                layer,
                Rect::from_min_size(pos2(0.0, 0.0), vec2(1.0, 1.0)),
                1.0,
            )
            .contains(pos2(point[0], point[1]))
            {
                return false;
            }
            if let LayerKind::Image { mask, clip, .. } = &layer.kind {
                if let Some(clip) = clip {
                    let uv = super::lasso_ui::source_uv(
                        layer,
                        [
                            (point[0] - layer.x) / layer.width,
                            (point[1] - layer.y) / layer.height,
                        ],
                    );
                    return clip.selection.contains_uv(uv[0], uv[1]);
                }
                mask.is_empty()
                    || compositor::point_in_polygon(
                        (point[0] - layer.x) / layer.width,
                        (point[1] - layer.y) / layer.height,
                        mask,
                    )
            } else {
                true
            }
        })
        .map(|layer| layer.id)
}

fn fit_zoom(workspace_width: f32, canvas_width: u32, show_script: bool) -> f32 {
    let script_width = if show_script { 300.0 } else { 0.0 };
    // Stage margins plus the vertical scrollbar, independent of toolbar contents.
    ((workspace_width - script_width - 72.0 - 16.0).max(1.0) / canvas_width.max(1) as f32)
        .clamp(0.02, 1.5)
}

fn preview_placeholder(painter: &egui::Painter, rect: Rect, label: &str) {
    painter.rect_filled(rect, 0, Color32::from_black_alpha(10));
    painter.rect_stroke(
        rect,
        0,
        Stroke::new(1.0_f32, Color32::from_black_alpha(35)),
        StrokeKind::Inside,
    );
    let visible = rect.intersect(painter.clip_rect());
    if visible.width() > 110.0 && visible.height() > 30.0 {
        painter.text(
            visible.left_top() + vec2(8.0, 8.0),
            Align2::LEFT_TOP,
            label,
            FontId::proportional(12.0),
            Color32::from_rgb(108, 120, 128),
        );
    }
}

fn layer_rect(layer: &Layer, canvas: Rect, zoom: f32) -> Rect {
    if let LayerKind::Stroke { points, size, .. } = &layer.kind {
        let mut bounds = Rect::NOTHING;
        for p in points {
            bounds.extend_with(pos2(p[0] + layer.x, p[1] + layer.y));
        }
        if bounds.is_finite() {
            return Rect::from_min_max(
                canvas.min + bounds.min.to_vec2() * zoom,
                canvas.min + bounds.max.to_vec2() * zoom,
            )
            .expand(*size * zoom / 2.0);
        }
    }
    Rect::from_min_size(
        canvas.min + vec2(layer.x, layer.y) * zoom,
        vec2(layer.width, layer.height) * zoom,
    )
}

fn snap_button(ui: &mut egui::Ui, enabled: &mut bool) -> egui::Response {
    let (rect, mut response) = ui.allocate_exact_size(vec2(72.0, 28.0), Sense::click());
    if response.clicked() {
        *enabled = !*enabled;
        response.mark_changed();
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), *enabled, "Snap")
    });
    let feedback = motion::feedback(ui, &response, *enabled);
    let fill = motion::mix(Color32::TRANSPARENT, CARD, feedback.hover);
    let fill = motion::mix(fill, ui.visuals().selection.bg_fill, feedback.selected);
    ui.painter()
        .rect_filled(feedback.paint_rect(rect), 7.0, fill);
    let ink = motion::mix(MUTED, ACCENT, feedback.selected);
    icons::draw(
        ui.painter(),
        Rect::from_min_size(rect.min + vec2(5.0, 4.0), vec2(20.0, 20.0)),
        icons::Icon::Magnet,
        ink,
    );
    ui.painter().text(
        rect.left_center() + vec2(30.0, 0.0),
        Align2::LEFT_CENTER,
        "Snap",
        FontId::proportional(13.0),
        ink,
    );
    response.on_hover_text("Snap · S\nHold Alt to move freely")
}

fn snap_viewport(clip: Rect, canvas: Rect, zoom: f32) -> Bounds {
    let clip = clip.expand(64.0);
    Bounds {
        x: (clip.left() - canvas.left()) / zoom,
        y: (clip.top() - canvas.top()) / zoom,
        width: clip.width() / zoom,
        height: clip.height() / zoom,
    }
}

fn visible_snap_targets(chapter: &Chapter, moving_id: u64, viewport: Bounds) -> Vec<SnapTarget> {
    chapter
        .layers
        .iter()
        .filter(|layer| layer.id != moving_id && layer.visible && layer.opacity > 0.0)
        .filter_map(|layer| {
            let bounds = snapping::layer_bounds(layer)?;
            (bounds.right() >= viewport.x
                && bounds.x <= viewport.right()
                && bounds.bottom() >= viewport.y
                && bounds.y <= viewport.bottom())
            .then_some(SnapTarget {
                id: layer.id,
                bounds,
            })
        })
        .collect()
}

fn render_rect(layer: &Layer, canvas: Rect, zoom: f32) -> Rect {
    if matches!(layer.kind, LayerKind::Text { .. }) {
        let [x, y, width, height] = compositor::text_bounds(layer);
        return Rect::from_min_size(canvas.min + vec2(x, y) * zoom, vec2(width, height) * zoom);
    }
    layer_rect(layer, canvas, zoom)
}

#[cfg(test)]
mod floating_toolbar_tests {
    use super::*;

    fn frame(
        ctx: &egui::Context,
        studio: &mut Studio,
        size: egui::Vec2,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        let mut native = eframe::Frame::_new_kittest();
        ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, size)),
                events,
                ..Default::default()
            },
            |ctx| eframe::App::update(studio, ctx, &mut native),
        )
    }

    fn fixture() -> (egui::Context, Studio) {
        let ctx = egui::Context::default();
        let mut studio = Studio::from_context(&ctx);
        studio.error = None;
        studio.welcome = false;
        studio.story = Story::new("Floating tools");
        studio.story.chapters[0].width = 800;
        studio.story.chapters[0].height = 3000;
        studio.zoom = 0.6;
        studio.auto_fit = false;
        studio.show_script = false;
        studio.snap_settings.enabled = false;
        let id = studio.story.alloc_id();
        let asset_id = studio.story.alloc_id();
        studio.story.chapters[0].layers.push(Layer {
            id,
            name: "Artwork below controls".into(),
            visible: true,
            locked: false,
            opacity: 1.0,
            x: 0.0,
            y: 0.0,
            width: 800.0,
            height: 1800.0,
            // Pointer handling is independent of preview readiness.
            kind: LayerKind::Image {
                source_rect: None,
                clip: None,
                asset_id,
                mask: Vec::new(),
            },
        });
        studio.selected = Some(id);
        studio.jump_y = Some(400.0);
        for _ in 0..3 {
            frame(&ctx, &mut studio, vec2(1100.0, 720.0), Vec::new());
        }
        (ctx, studio)
    }

    fn pointer(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    fn click(ctx: &egui::Context, studio: &mut Studio, pos: egui::Pos2) {
        frame(
            ctx,
            studio,
            vec2(1100.0, 720.0),
            vec![
                egui::Event::PointerMoved(pos),
                pointer(pos, true),
                pointer(pos, false),
            ],
        );
        frame(ctx, studio, vec2(1100.0, 720.0), Vec::new());
    }

    fn icon(ctx: &egui::Context, name: &str) -> Rect {
        ctx.data(|data| {
            data.get_temp::<Rect>(egui::Id::new(("icon_button_bounds", name)))
                .unwrap_or_else(|| panic!("Missing toolbar icon: {name}"))
        })
    }

    fn text_bounds(output: &egui::FullOutput, label: &str) -> Rect {
        fn find(shape: &egui::Shape, label: &str) -> Option<Rect> {
            match shape {
                egui::Shape::Text(text) if text.galley.job.text == label => {
                    Some(text.visual_bounding_rect())
                }
                egui::Shape::Vec(shapes) => shapes.iter().find_map(|shape| find(shape, label)),
                _ => None,
            }
        }
        output
            .shapes
            .iter()
            .find_map(|shape| find(&shape.shape, label))
            .unwrap_or_else(|| panic!("Missing control: {label}"))
    }

    #[test]
    fn fit_tracks_resizes_script_and_chapter_width_without_editing_story() {
        let (ctx, mut studio) = fixture();
        studio.auto_fit = true;
        studio.show_script = true;
        let second_id = studio.story.alloc_id();
        studio
            .story
            .chapters
            .push(Chapter::new(second_id, "Wide chapter", 1600, 3000));
        let original = studio.story.clone();
        let mut previous_width = 0.0;
        for size in [
            vec2(1100.0, 720.0),
            vec2(1366.0, 768.0),
            vec2(1707.0, 960.0),
        ] {
            for _ in 0..3 {
                frame(&ctx, &mut studio, size, Vec::new());
            }
            assert!(studio.canvas_rect.width() > previous_width);
            previous_width = studio.canvas_rect.width();
            assert!(
                (studio.scroll_y - 400.0).abs() < 0.1,
                "resizing lost chapter position: {}",
                studio.scroll_y
            );
        }
        let width_with_script = studio.canvas_rect.width();
        studio.show_script = false;
        for _ in 0..3 {
            frame(&ctx, &mut studio, vec2(1707.0, 960.0), Vec::new());
        }
        assert!(studio.canvas_rect.width() > width_with_script);
        studio.chapter = 1;
        studio.jump_y = Some(0.0);
        for _ in 0..3 {
            frame(&ctx, &mut studio, vec2(1100.0, 720.0), Vec::new());
        }
        let clip = ctx.data(|data| {
            data.get_temp::<Rect>(egui::Id::new("canvas_scroll_bounds"))
                .unwrap()
        });
        assert!(clip.contains_rect(Rect::from_min_max(
            studio.canvas_rect.left_top(),
            pos2(studio.canvas_rect.right(), clip.bottom())
        )));
        assert!(studio.zoom < 0.4, "wider chapter must fit at a lower scale");
        assert_eq!(studio.story, original);
        assert!(!studio.dirty && studio.undo.is_empty());
    }

    #[test]
    fn zoom_buttons_opt_out_of_fit_and_fit_button_restores_tracking() {
        let (ctx, mut studio) = fixture();
        studio.auto_fit = true;
        let size = vec2(1100.0, 720.0);
        let output = frame(&ctx, &mut studio, size, Vec::new());
        let automatic = studio.zoom;
        click(&ctx, &mut studio, text_bounds(&output, "+").center());
        assert!(!studio.auto_fit);
        assert!((studio.zoom - automatic - 0.05).abs() < 0.001);
        let manual = studio.zoom;
        studio.show_script = true;
        for _ in 0..3 {
            frame(&ctx, &mut studio, vec2(1707.0, 960.0), Vec::new());
        }
        assert_eq!(studio.zoom, manual);
        // Return to the test click helper's compact viewport before using its hit bounds.
        let output = frame(&ctx, &mut studio, size, Vec::new());
        click(&ctx, &mut studio, text_bounds(&output, "Fit").center());
        assert!(studio.auto_fit);
        assert!(studio.zoom < manual);
        let fitted = studio.zoom;
        for _ in 0..3 {
            frame(&ctx, &mut studio, vec2(1707.0, 960.0), Vec::new());
        }
        assert!(studio.zoom > fitted);
        assert!(!studio.dirty && studio.undo.is_empty());
    }

    #[test]
    fn small_canvas_and_script_group_center_under_a_bounded_shelf() {
        let (ctx, mut studio) = fixture();
        studio.jump_y = Some(0.0);
        for show_script in [false, true] {
            studio.show_script = show_script;
            for _ in 0..3 {
                frame(&ctx, &mut studio, vec2(2560.0, 1440.0), Vec::new());
            }
            let clip = ctx.data(|data| {
                data.get_temp::<Rect>(egui::Id::new("canvas_scroll_bounds"))
                    .unwrap()
            });
            let group_left = studio.canvas_rect.left() - 42.0;
            let group_right =
                studio.canvas_rect.right() + 30.0 + if show_script { 300.0 } else { 0.0 };
            assert!(((group_left + group_right) * 0.5 - clip.center().x).abs() <= 1.0);
            assert!(studio.toolbar_rect.width() <= 720.0);
            // The shelf follows the workspace center (including its reserved scrollbar).
            assert!((studio.toolbar_rect.center().x - clip.center().x).abs() <= 10.0);
        }
    }

    #[test]
    fn magnetic_drag_uses_document_coordinates_on_a_centered_canvas() {
        let (ctx, mut studio) = fixture();
        let id = studio.story.alloc_id();
        studio.story.chapters[0].layers = vec![Layer {
            id,
            name: "Centered artwork".into(),
            visible: true,
            locked: false,
            opacity: 1.0,
            x: 100.0,
            y: 300.0,
            width: 100.0,
            height: 100.0,
            kind: LayerKind::Shape {
                color: [90, 130, 170, 255],
            },
        }];
        studio.selected = Some(id);
        studio.jump_y = Some(0.0);
        studio.snap_settings.enabled = true;
        let size = vec2(1707.0, 960.0);
        for _ in 0..3 {
            frame(&ctx, &mut studio, size, Vec::new());
        }
        let original = studio.story.clone();
        let from = studio.canvas_rect.min + vec2(150.0, 350.0) * studio.zoom;
        let to = studio.canvas_rect.min + vec2(397.0, 650.0) * studio.zoom;
        frame(
            &ctx,
            &mut studio,
            size,
            vec![egui::Event::PointerMoved(from), pointer(from, true)],
        );
        frame(&ctx, &mut studio, size, vec![egui::Event::PointerMoved(to)]);
        frame(&ctx, &mut studio, size, vec![pointer(to, false)]);
        let layer = &studio.story.chapters[0].layers[0];
        assert!(
            (layer.x - 350.0).abs() < 0.01,
            "center snap must remain in document coordinates"
        );
        assert!((layer.y - 600.0).abs() < 0.01);
        assert_eq!(studio.undo.len(), 1);
        studio.undo();
        assert_eq!(studio.story, original);
    }

    #[test]
    fn toolbar_clicks_and_its_background_never_edit_artwork_underneath() {
        for tool in [
            Tool::Move,
            Tool::Lasso,
            Tool::Brush,
            Tool::Text,
            Tool::Panel,
        ] {
            let (ctx, mut studio) = fixture();
            studio.tool = tool;
            frame(&ctx, &mut studio, vec2(1100.0, 720.0), Vec::new());
            frame(&ctx, &mut studio, vec2(1100.0, 720.0), Vec::new());
            let original = studio.story.clone();
            let selected = studio.selected;
            let bg = pos2(
                studio.toolbar_rect.left() + 90.0,
                studio.toolbar_rect.bottom() - 5.0,
            );
            assert!(
                studio.canvas_rect.contains(bg),
                "fixture must put artwork under the shelf"
            );
            click(&ctx, &mut studio, bg);
            assert_eq!(studio.story, original);
            assert_eq!(studio.selected, selected);
            assert!(studio.undo.is_empty());

            let brush = icon(&ctx, "Brush").center();
            assert!(studio.canvas_rect.contains(brush));
            click(&ctx, &mut studio, brush);
            assert!(studio.tool == Tool::Brush);
            assert_eq!(studio.story, original);
            assert!(studio.undo.is_empty());
        }
    }

    #[test]
    fn a_drag_begun_on_the_shelf_cannot_move_or_draw_on_the_canvas() {
        for tool in [
            Tool::Move,
            Tool::Brush,
            Tool::Lasso,
            Tool::SmartLasso,
            Tool::Select,
            Tool::Crop,
        ] {
            let (ctx, mut studio) = fixture();
            studio.tool = tool;
            frame(&ctx, &mut studio, vec2(1100.0, 720.0), Vec::new());
            frame(&ctx, &mut studio, vec2(1100.0, 720.0), Vec::new());
            let original = studio.story.clone();
            let from = pos2(
                studio.toolbar_rect.left() + 95.0,
                studio.toolbar_rect.bottom() - 5.0,
            );
            let to = from + vec2(90.0, 180.0);
            frame(
                &ctx,
                &mut studio,
                vec2(1100.0, 720.0),
                vec![egui::Event::PointerMoved(from), pointer(from, true)],
            );
            frame(
                &ctx,
                &mut studio,
                vec2(1100.0, 720.0),
                vec![egui::Event::PointerMoved(to)],
            );
            frame(
                &ctx,
                &mut studio,
                vec2(1100.0, 720.0),
                vec![pointer(to, false)],
            );
            assert_eq!(studio.story, original);
            assert!(studio.undo.is_empty());
            assert!(studio.lasso.is_empty());
        }
    }

    #[test]
    fn newly_expanded_or_reappearing_shelf_blocks_the_first_pointer_frame() {
        let (ctx, mut studio) = fixture();
        let original = studio.story.clone();
        let expanded = pos2(
            studio.toolbar_rect.left() + 95.0,
            studio.toolbar_rect.bottom() + 22.0,
        );
        studio.tool = Tool::Brush;
        // No intervening idle frame: the area hit map still has Move's height.
        click(&ctx, &mut studio, expanded);
        assert!(studio.toolbar_rect.contains(expanded));
        assert_eq!(studio.story, original);
        assert!(studio.undo.is_empty());

        studio.settings_open = true;
        for _ in 0..2 {
            frame(&ctx, &mut studio, vec2(1100.0, 720.0), Vec::new());
        }
        studio.settings_open = false;
        studio.tool = Tool::Text;
        // The canvas and shelf return together after Settings.
        click(&ctx, &mut studio, expanded);
        assert!(studio.toolbar_rect.contains(expanded));
        assert_eq!(studio.story, original);
        assert!(studio.undo.is_empty());
    }

    #[test]
    fn a_canvas_drag_can_finish_beneath_the_shelf_and_undo_once() {
        let (ctx, mut studio) = fixture();
        let original = studio.story.clone();
        let from = pos2(
            studio.canvas_rect.left() + 180.0,
            studio.toolbar_rect.bottom() + 100.0,
        );
        let to = pos2(from.x + 35.0, studio.toolbar_rect.center().y);
        frame(
            &ctx,
            &mut studio,
            vec2(1100.0, 720.0),
            vec![egui::Event::PointerMoved(from), pointer(from, true)],
        );
        frame(
            &ctx,
            &mut studio,
            vec2(1100.0, 720.0),
            vec![egui::Event::PointerMoved(to)],
        );
        frame(
            &ctx,
            &mut studio,
            vec2(1100.0, 720.0),
            vec![pointer(to, false)],
        );
        let layer = &studio.story.chapters[0].layers[0];
        assert!((layer.x - (to.x - from.x) / studio.zoom).abs() < 0.01);
        assert!((layer.y - (to.y - from.y) / studio.zoom).abs() < 0.01);
        assert_eq!(studio.undo.len(), 1);
        assert!(studio.snap_drag.is_none() && studio.drag_layer.is_none());
        studio.undo();
        assert_eq!(studio.story, original);
    }

    #[test]
    fn rectangle_release_keeps_all_four_corners_for_selection_and_crop() {
        for tool in [Tool::Select, Tool::Crop] {
            let (ctx, mut studio) = fixture();
            studio.tool = tool;
            let LayerKind::Image { asset_id, .. } = studio.story.chapters[0].layers[0].kind else {
                panic!()
            };
            let mut encoded = std::io::Cursor::new(Vec::new());
            image::RgbaImage::from_pixel(800, 1800, image::Rgba([60, 90, 120, 255]))
                .write_to(&mut encoded, image::ImageFormat::Png)
                .unwrap();
            studio
                .assets
                .insert(asset_id, Arc::new(encoded.into_inner()));
            studio.story.assets.push(Asset {
                id: asset_id,
                name: "Rectangle source".into(),
                kind: AssetKind::Image,
                notes: String::new(),
                prompt: String::new(),
                width: 800,
                height: 1800,
                character: CharacterProfile::default(),
            });
            let from = pos2(
                studio.canvas_rect.left() + 100.0,
                studio.toolbar_rect.bottom() + 120.0,
            );
            let to = from + vec2(110.0, 140.0);
            frame(
                &ctx,
                &mut studio,
                vec2(1100.0, 720.0),
                vec![egui::Event::PointerMoved(from), pointer(from, true)],
            );
            frame(
                &ctx,
                &mut studio,
                vec2(1100.0, 720.0),
                vec![egui::Event::PointerMoved(to)],
            );
            frame(
                &ctx,
                &mut studio,
                vec2(1100.0, 720.0),
                vec![pointer(to, false)],
            );
            let deadline = Instant::now() + std::time::Duration::from_secs(3);
            while Instant::now() < deadline {
                studio.poll_selection(&ctx);
                studio.apply_selection_crop();
                if matches!(
                    &studio.story.chapters[0].layers[0].kind,
                    LayerKind::Image { clip: Some(_), .. }
                ) {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            let LayerKind::Image {
                clip: Some(clip), ..
            } = &studio.story.chapters[0].layers[0].kind
            else {
                panic!("Rectangle was not applied")
            };
            let [x, y, w, h] = clip.selection.bounds().unwrap();
            for (dx, dy) in [
                (w / 4, h / 4),
                (3 * w / 4, h / 4),
                (w / 4, 3 * h / 4),
                (3 * w / 4, 3 * h / 4),
            ] {
                assert!(
                    clip.selection.contains(x + dx, y + dy),
                    "Rectangle lost a corner"
                );
            }
        }
    }

    #[test]
    fn tool_variants_fit_small_windows_without_shifting_the_canvas() {
        for size in [
            vec2(1024.0, 700.0),
            vec2(1100.0, 720.0),
            vec2(1366.0, 768.0),
        ] {
            let (ctx, mut studio) = fixture();
            let mut origin = None;
            for tool in [
                Tool::Move,
                Tool::Brush,
                Tool::Text,
                Tool::Lasso,
                Tool::SmartLasso,
                Tool::Select,
                Tool::Crop,
                Tool::Heal,
                Tool::Panel,
            ] {
                studio.tool = tool;
                for _ in 0..3 {
                    frame(&ctx, &mut studio, size, Vec::new());
                }
                let previous = origin.get_or_insert(studio.canvas_rect.min);
                assert_eq!(
                    *previous, studio.canvas_rect.min,
                    "tool changes must not move artwork"
                );
                let bounds = Rect::from_min_size(egui::Pos2::ZERO, size);
                assert!(bounds.contains_rect(studio.toolbar_rect));
                for name in ["Move", "Lasso", "Brush", "Bubble", "Beat", "Undo", "Redo"] {
                    assert!(
                        studio.toolbar_rect.contains_rect(icon(&ctx, name)),
                        "{name} escaped shelf at {size:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_modal_keeps_toolbar_and_underlying_canvas_inactive() {
        let (ctx, mut studio) = fixture();
        let original = studio.story.clone();
        studio.dialog = Some(Dialog::Chapter);
        studio.dialog_name = "Another chapter".into();
        for _ in 0..3 {
            frame(&ctx, &mut studio, vec2(1100.0, 720.0), Vec::new());
        }
        let brush = icon(&ctx, "Brush").center();
        click(&ctx, &mut studio, brush);
        assert!(studio.tool == Tool::Move);
        assert!(studio.dialog.is_some());
        assert_eq!(studio.story, original);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_uses_full_workspace_width_and_reserves_script_and_scrollbar() {
        let with_script = fit_zoom(1000.0, 1600, true);
        assert!((with_script * 1600.0 + 300.0 + 88.0 - 1000.0).abs() < 0.001);
        assert!(fit_zoom(1000.0, 1600, false) > with_script);
        // Even the widest supported chapter can fit in a narrow workspace.
        assert!(fit_zoom(620.0, 8192, true) * 8192.0 + 388.0 <= 620.01);
    }

    fn fixture() -> (egui::Context, Studio, u64) {
        let ctx = egui::Context::default();
        let mut studio = Studio::from_context(&ctx);
        studio.story = Story::new("Pointer interaction regression");
        studio.zoom = 1.0;
        studio.chapter = 0;
        let id = studio.story.alloc_id();
        studio.story.chapters[0].layers = vec![Layer {
            id,
            name: "Small panel".into(),
            visible: true,
            locked: false,
            opacity: 1.0,
            x: 100.0,
            y: 100.0,
            width: 50.0,
            height: 50.0,
            kind: LayerKind::Shape {
                color: [20, 30, 40, 255],
            },
        }];
        frame(&ctx, &mut studio, vec![]);
        (ctx, studio, id)
    }

    fn frame(ctx: &egui::Context, studio: &mut Studio, events: Vec<egui::Event>) {
        frame_with_modifiers(ctx, studio, events, egui::Modifiers::NONE);
    }

    fn frame_with_modifiers(
        ctx: &egui::Context,
        studio: &mut Studio,
        events: Vec<egui::Event>,
        modifiers: egui::Modifiers,
    ) {
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1000.0, 1000.0))),
                events,
                modifiers,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let canvas = Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 900.0));
                    let response = ui.interact(
                        canvas,
                        egui::Id::new("test-canvas"),
                        Sense::click_and_drag(),
                    );
                    studio.canvas_input(ctx, &response, canvas);
                });
            },
        );
    }

    fn press(pos: egui::Pos2, button: egui::PointerButton) -> Vec<egui::Event> {
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ]
    }

    fn snap_fixture() -> (egui::Context, Studio, u64) {
        let (ctx, mut studio, id) = fixture();
        studio.story.chapters[0].width = 800;
        studio.story.chapters[0].height = 900;
        studio.welcome = false;
        (ctx, studio, id)
    }

    #[test]
    fn magnetic_center_releases_cleanly_and_undoes_as_one_drag() {
        let (ctx, mut studio, _) = snap_fixture();
        let original = studio.story.clone();
        frame(
            &ctx,
            &mut studio,
            press(pos2(110.0, 110.0), egui::PointerButton::Primary),
        );
        frame(
            &ctx,
            &mut studio,
            vec![egui::Event::PointerMoved(pos2(380.0, 130.0))],
        );
        assert_eq!(studio.story.chapters[0].layers[0].x, 375.0);
        assert!(!studio.snap_guides.is_empty());
        assert_eq!(studio.undo.len(), 1);
        frame(
            &ctx,
            &mut studio,
            vec![egui::Event::PointerButton {
                pos: pos2(380.0, 130.0),
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert!(studio.snap_guides.is_empty() && studio.snap_drag.is_none());
        assert_eq!(studio.story.chapters[0].layers[0].x, 375.0);
        studio.undo();
        assert_eq!(studio.story, original);
        studio.redo();
        assert_eq!(studio.story.chapters[0].layers[0].x, 375.0);
    }

    #[test]
    fn final_pointer_move_and_release_finish_the_magnetic_placement() {
        let (ctx, mut studio, _) = snap_fixture();
        frame(
            &ctx,
            &mut studio,
            press(pos2(110.0, 110.0), egui::PointerButton::Primary),
        );
        frame(
            &ctx,
            &mut studio,
            vec![egui::Event::PointerMoved(pos2(290.0, 130.0))],
        );
        assert_eq!(studio.story.chapters[0].layers[0].x, 280.0);
        frame(
            &ctx,
            &mut studio,
            vec![
                egui::Event::PointerMoved(pos2(380.0, 130.0)),
                egui::Event::PointerButton {
                    pos: pos2(380.0, 130.0),
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        assert_eq!(studio.story.chapters[0].layers[0].x, 375.0);
        assert!(studio.snap_drag.is_none() && studio.snap_guides.is_empty());
        assert_eq!(studio.undo.len(), 1);
    }

    #[test]
    fn magnets_use_unsnapped_origin_so_alt_and_escape_never_accumulate_drift() {
        let (ctx, mut studio, _) = snap_fixture();
        frame(
            &ctx,
            &mut studio,
            press(pos2(110.0, 110.0), egui::PointerButton::Primary),
        );
        frame(
            &ctx,
            &mut studio,
            vec![egui::Event::PointerMoved(pos2(380.0, 130.0))],
        );
        assert_eq!(studio.story.chapters[0].layers[0].x, 375.0);
        frame_with_modifiers(&ctx, &mut studio, vec![], egui::Modifiers::ALT);
        assert_eq!(studio.story.chapters[0].layers[0].x, 370.0);
        assert!(studio.snap_guides.is_empty());
        frame(&ctx, &mut studio, vec![]);
        assert_eq!(studio.story.chapters[0].layers[0].x, 375.0);
        frame(
            &ctx,
            &mut studio,
            vec![egui::Event::PointerMoved(pos2(400.0, 130.0))],
        );
        assert_eq!(studio.story.chapters[0].layers[0].x, 390.0);
        assert!(studio.snap_guides.is_empty());
        // Returning to the same raw pointer restores the same snap exactly.
        frame(
            &ctx,
            &mut studio,
            vec![egui::Event::PointerMoved(pos2(380.0, 130.0))],
        );
        assert_eq!(studio.story.chapters[0].layers[0].x, 375.0);
        let _ = ctx.run(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            },
            |ctx| studio.shortcuts(ctx),
        );
        assert!(studio.snap_drag.is_none() && studio.snap_guides.is_empty());
    }

    #[test]
    fn hidden_and_offscreen_objects_are_ignored_but_locked_objects_are_alignment_targets() {
        let (ctx, mut studio, _) = snap_fixture();
        studio.snap_settings.canvas = false;
        studio.snap_settings.spacing = false;
        let mut target = studio.story.chapters[0].layers[0].clone();
        target.id = studio.story.alloc_id();
        target.x = 300.0;
        target.y = 300.0;
        target.locked = true;
        let locked_id = target.id;
        studio.story.chapters[0].layers.push(target.clone());
        target.id = studio.story.alloc_id();
        target.x = 296.0;
        target.visible = false;
        studio.story.chapters[0].layers.push(target.clone());
        target.id = studio.story.alloc_id();
        target.visible = true;
        target.y = 2000.0;
        studio.story.chapters[0].layers.push(target);
        frame(
            &ctx,
            &mut studio,
            press(pos2(110.0, 110.0), egui::PointerButton::Primary),
        );
        frame(
            &ctx,
            &mut studio,
            vec![egui::Event::PointerMoved(pos2(307.0, 130.0))],
        );
        assert_eq!(studio.story.chapters[0].layers[0].x, 300.0);
        assert_eq!(studio.snap_drag.as_ref().unwrap().targets.len(), 1);
        assert_eq!(studio.snap_drag.as_ref().unwrap().targets[0].id, locked_id);
        assert_eq!(studio.story.chapters[0].layers[1].x, 300.0);
    }

    #[test]
    fn magnetic_placement_uses_the_visible_mask_and_brush_bounds() {
        let (ctx, mut studio, _) = snap_fixture();
        let layer = &mut studio.story.chapters[0].layers[0];
        layer.width = 100.0;
        layer.height = 100.0;
        layer.kind = LayerKind::Image {
            source_rect: None,
            clip: None,
            asset_id: 999,
            mask: vec![[0.5, 0.0], [1.0, 0.0], [1.0, 1.0], [0.5, 1.0]],
        };
        frame(
            &ctx,
            &mut studio,
            press(pos2(175.0, 125.0), egui::PointerButton::Primary),
        );
        frame(
            &ctx,
            &mut studio,
            vec![egui::Event::PointerMoved(pos2(397.0, 145.0))],
        );
        assert_eq!(studio.story.chapters[0].layers[0].x, 325.0);

        let (ctx, mut studio, _) = snap_fixture();
        let layer = &mut studio.story.chapters[0].layers[0];
        layer.x = 0.0;
        layer.y = 0.0;
        layer.width = 800.0;
        layer.height = 900.0;
        layer.kind = LayerKind::Stroke {
            points: vec![[100.0, 100.0], [200.0, 100.0]],
            color: [0, 0, 0, 255],
            size: 10.0,
        };
        frame(
            &ctx,
            &mut studio,
            press(pos2(150.0, 100.0), egui::PointerButton::Primary),
        );
        frame(
            &ctx,
            &mut studio,
            vec![egui::Event::PointerMoved(pos2(397.0, 120.0))],
        );
        assert_eq!(studio.story.chapters[0].layers[0].x, 250.0);
    }

    #[test]
    fn disabled_snapping_preserves_free_drag_at_a_near_center_position() {
        let (ctx, mut studio, _) = snap_fixture();
        studio.snap_settings.enabled = false;
        frame(
            &ctx,
            &mut studio,
            press(pos2(110.0, 110.0), egui::PointerButton::Primary),
        );
        frame(
            &ctx,
            &mut studio,
            vec![egui::Event::PointerMoved(pos2(380.0, 130.0))],
        );
        assert_eq!(studio.story.chapters[0].layers[0].x, 370.0);
        assert!(studio.snap_guides.is_empty());
    }

    #[test]
    fn fast_drag_selects_at_press_origin_even_after_leaving_small_layer() {
        let (ctx, mut studio, id) = fixture();
        frame(
            &ctx,
            &mut studio,
            press(pos2(110.0, 110.0), egui::PointerButton::Primary),
        );
        frame(
            &ctx,
            &mut studio,
            vec![egui::Event::PointerMoved(pos2(210.0, 210.0))],
        );
        assert_eq!(studio.selected, Some(id));
        assert_eq!(studio.story.chapters[0].layers[0].x, 200.0);
        assert_eq!(studio.undo.len(), 1);
    }

    #[test]
    fn quick_corner_drag_resizes_in_its_first_moving_frame() {
        let (ctx, mut studio, id) = fixture();
        studio.selected = Some(id);
        frame(
            &ctx,
            &mut studio,
            press(pos2(150.0, 150.0), egui::PointerButton::Primary),
        );
        frame(
            &ctx,
            &mut studio,
            vec![egui::Event::PointerMoved(pos2(230.0, 230.0))],
        );
        let layer = &studio.story.chapters[0].layers[0];
        assert_eq!(
            (layer.x, layer.y, layer.width, layer.height),
            (100.0, 100.0, 130.0, 130.0)
        );
    }

    #[test]
    fn brush_keeps_the_segment_from_the_initial_press() {
        let (ctx, mut studio, _) = fixture();
        studio.tool = Tool::Brush;
        frame(
            &ctx,
            &mut studio,
            press(pos2(20.0, 20.0), egui::PointerButton::Primary),
        );
        frame(
            &ctx,
            &mut studio,
            vec![egui::Event::PointerMoved(pos2(200.0, 200.0))],
        );
        let LayerKind::Stroke { points, .. } =
            &studio.story.chapters[0].layers.last().unwrap().kind
        else {
            panic!("Missing brush stroke")
        };
        assert_eq!(points.first(), Some(&[20.0, 20.0]));
        assert_eq!(points.last(), Some(&[200.0, 200.0]));
    }

    #[test]
    fn right_button_drag_does_not_draw() {
        let (ctx, mut studio, _) = fixture();
        studio.tool = Tool::Brush;
        frame(
            &ctx,
            &mut studio,
            press(pos2(20.0, 20.0), egui::PointerButton::Secondary),
        );
        frame(
            &ctx,
            &mut studio,
            vec![egui::Event::PointerMoved(pos2(200.0, 200.0))],
        );
        assert_eq!(studio.story.chapters[0].layers.len(), 1);
    }

    fn bubble_fixture() -> (egui::Context, Studio, u64) {
        let (ctx, mut studio, id) = fixture();
        let layer = &mut studio.story.chapters[0].layers[0];
        layer.width = 200.0;
        layer.height = 100.0;
        layer.kind = LayerKind::Text {
            text: "Hello!".into(),
            size: 28.0,
            color: [0, 0, 0, 255],
            bubble: true,
            style: TextStyle::dialogue(),
        };
        studio.selected = Some(id);
        (ctx, studio, id)
    }

    #[test]
    fn quick_pointer_drag_moves_only_the_tip_and_can_be_undone() {
        let (ctx, mut studio, id) = bubble_fixture();
        let original = studio.story.chapters[0].layers[0].clone();
        let tip = tail_tip(&original).unwrap();
        frame(
            &ctx,
            &mut studio,
            press(pos2(tip[0], tip[1]), egui::PointerButton::Primary),
        );
        frame(
            &ctx,
            &mut studio,
            vec![egui::Event::PointerMoved(pos2(500.0, 400.0))],
        );
        let layer = &studio.story.chapters[0].layers[0];
        assert_eq!(tail_tip(layer), Some([500.0, 400.0]));
        assert_eq!(
            (layer.x, layer.y, layer.width, layer.height),
            (100.0, 100.0, 200.0, 100.0)
        );
        assert_eq!(studio.drag_tail, Some((id, 0)));
        assert_eq!(studio.undo.len(), 1);
        studio.undo();
        assert_eq!(studio.story.chapters[0].layers[0], original);
    }

    #[test]
    fn locked_bubble_pointer_cannot_be_dragged() {
        let (ctx, mut studio, _) = bubble_fixture();
        studio.story.chapters[0].layers[0].locked = true;
        let original = studio.story.chapters[0].layers[0].clone();
        let tip = tail_tip(&original).unwrap();
        frame(
            &ctx,
            &mut studio,
            press(pos2(tip[0], tip[1]), egui::PointerButton::Primary),
        );
        frame(
            &ctx,
            &mut studio,
            vec![egui::Event::PointerMoved(pos2(500.0, 400.0))],
        );
        assert_eq!(studio.story.chapters[0].layers[0], original);
        assert!(studio.undo.is_empty());
    }

    #[test]
    fn secondary_drag_does_not_move_a_bubble_pointer() {
        let (ctx, mut studio, _) = bubble_fixture();
        let original = studio.story.chapters[0].layers[0].clone();
        let tip = tail_tip(&original).unwrap();
        frame(
            &ctx,
            &mut studio,
            press(pos2(tip[0], tip[1]), egui::PointerButton::Secondary),
        );
        frame(
            &ctx,
            &mut studio,
            vec![egui::Event::PointerMoved(pos2(500.0, 400.0))],
        );
        assert_eq!(studio.story.chapters[0].layers[0], original);
    }

    #[test]
    fn pointer_preview_bounds_expand_without_changing_resize_bounds() {
        let (_, mut studio, _) = bubble_fixture();
        let layer = &mut studio.story.chapters[0].layers[0];
        let LayerKind::Text { style, .. } = &mut layer.kind else {
            unreachable!()
        };
        style.tail.tip = [-0.5, 1.6];
        let canvas = Rect::from_min_size(pos2(20.0, 30.0), vec2(800.0, 900.0));
        let body = layer_rect(layer, canvas, 1.0);
        let preview = render_rect(layer, canvas, 1.0);
        assert_eq!(
            body,
            Rect::from_min_size(pos2(120.0, 130.0), vec2(200.0, 100.0))
        );
        let tip = tail_tip(layer).unwrap();
        assert!(preview.contains(canvas.min + vec2(tip[0], tip[1])));
        assert!(preview.left() < body.left());
        assert!(preview.bottom() > body.bottom());
    }

    #[test]
    fn resizing_bubble_keeps_pointer_aimed_at_same_chapter_position() {
        let (ctx, mut studio, _) = bubble_fixture();
        let target = tail_tip(&studio.story.chapters[0].layers[0]);
        frame(
            &ctx,
            &mut studio,
            press(pos2(300.0, 200.0), egui::PointerButton::Primary),
        );
        frame(
            &ctx,
            &mut studio,
            vec![egui::Event::PointerMoved(pos2(400.0, 300.0))],
        );
        let layer = &studio.story.chapters[0].layers[0];
        assert_eq!((layer.width, layer.height), (300.0, 200.0));
        assert_eq!(tail_tip(layer), target);
        let LayerKind::Text { style, .. } = &layer.kind else {
            unreachable!()
        };
        assert!(
            !style.auto_fit,
            "Explicitly resizing the bubble disables automatic fitting"
        );
    }

    #[test]
    fn second_pointer_drags_independently_to_negative_coordinates_and_undoes() {
        let (ctx, mut studio, id) = bubble_fixture();
        let layer = &mut studio.story.chapters[0].layers[0];
        let LayerKind::Text { style, .. } = &mut layer.kind else {
            unreachable!()
        };
        style.extra_tails.push(BubbleTail {
            enabled: true,
            tip: [1.4, -0.4],
            ..BubbleTail::default()
        });
        let original = layer.clone();
        let first_target = tail_tip(&original);
        let (_, tip) = pointer_targets(&original)[1];
        frame(
            &ctx,
            &mut studio,
            press(pos2(tip[0], tip[1]), egui::PointerButton::Primary),
        );
        frame(
            &ctx,
            &mut studio,
            vec![egui::Event::PointerMoved(pos2(-20.0, -30.0))],
        );
        let layer = &studio.story.chapters[0].layers[0];
        assert_eq!(studio.drag_tail, Some((id, 1)));
        let (index, target) = pointer_targets(layer)[1];
        assert_eq!(index, 1);
        assert!((target[0] + 20.0).abs() < 0.001);
        assert!((target[1] + 30.0).abs() < 0.001);
        assert_eq!(tail_tip(layer), first_target);
        assert_eq!(
            (layer.x, layer.y, layer.width, layer.height),
            (original.x, original.y, original.width, original.height)
        );
        let LayerKind::Text { style, .. } = &layer.kind else {
            unreachable!()
        };
        assert!(style.extra_tails[0].tip.iter().all(|value| *value < 0.0));
        assert_eq!(studio.undo.len(), 1);
        studio.undo();
        assert_eq!(studio.story.chapters[0].layers[0], original);
    }

    #[test]
    fn extra_pointer_remains_draggable_when_first_pointer_is_disabled() {
        let (ctx, mut studio, id) = bubble_fixture();
        let layer = &mut studio.story.chapters[0].layers[0];
        let LayerKind::Text { style, .. } = &mut layer.kind else {
            unreachable!()
        };
        style.tail.enabled = false;
        style.extra_tails.push(BubbleTail {
            enabled: true,
            tip: [-0.25, 0.5],
            ..BubbleTail::default()
        });
        frame(
            &ctx,
            &mut studio,
            press(pos2(50.0, 150.0), egui::PointerButton::Primary),
        );
        frame(
            &ctx,
            &mut studio,
            vec![egui::Event::PointerMoved(pos2(40.0, 50.0))],
        );
        assert_eq!(studio.drag_tail, Some((id, 1)));
        let targets = pointer_targets(&studio.story.chapters[0].layers[0]);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].0, 1);
        assert!((targets[0].1[0] - 40.0).abs() < 0.001);
        assert!((targets[0].1[1] - 50.0).abs() < 0.001);
    }

    #[test]
    fn corner_resize_preserves_all_pointer_targets() {
        let (ctx, mut studio, _) = bubble_fixture();
        let layer = &mut studio.story.chapters[0].layers[0];
        let LayerKind::Text { style, .. } = &mut layer.kind else {
            unreachable!()
        };
        style.extra_tails.push(BubbleTail {
            enabled: true,
            tip: [-1.0, -2.0],
            ..BubbleTail::default()
        });
        let targets = pointer_targets(layer);
        frame(
            &ctx,
            &mut studio,
            press(pos2(300.0, 200.0), egui::PointerButton::Primary),
        );
        frame(
            &ctx,
            &mut studio,
            vec![egui::Event::PointerMoved(pos2(400.0, 300.0))],
        );
        let layer = &studio.story.chapters[0].layers[0];
        assert_eq!((layer.width, layer.height), (300.0, 200.0));
        for ((actual_index, actual), (expected_index, expected)) in
            pointer_targets(layer).into_iter().zip(targets)
        {
            assert_eq!(actual_index, expected_index);
            assert!((actual[0] - expected[0]).abs() < 0.001);
            assert!((actual[1] - expected[1]).abs() < 0.001);
        }
    }

    #[test]
    fn new_bubble_uses_the_selected_story_preset() {
        let (ctx, mut studio, _) = fixture();
        let preset = studio.story.bubble_presets.last().unwrap().clone();
        studio.selected_bubble_preset = Some(preset.id);
        studio.tool = Tool::Text;
        let position = pos2(400.0, 300.0);
        frame(
            &ctx,
            &mut studio,
            press(position, egui::PointerButton::Primary),
        );
        frame(
            &ctx,
            &mut studio,
            vec![egui::Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        let layer = studio.story.chapters[0].layers.last().unwrap();
        let LayerKind::Text {
            style,
            color,
            size,
            bubble,
            ..
        } = &layer.kind
        else {
            panic!("The text tool must create a bubble")
        };
        assert_eq!(style.preset_id, Some(preset.id));
        assert_eq!(style.font, preset.style.font);
        assert_eq!((layer.x, layer.y), (400.0, 300.0));
        assert_eq!(
            (*color, *size, *bubble),
            (preset.color, preset.size, preset.bubble)
        );
        assert!(!compositor::text_overflows(layer));
    }
}
