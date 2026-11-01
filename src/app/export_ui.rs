use super::*;

impl Studio {
    pub(super) fn export_dialog_ui(&mut self, ctx: &egui::Context) {
        if self.export_dialog {
            let mut open = true;
            let mut start = false;
            egui::Window::new("Export chapter")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.set_width(480.0);
                    ui.add(
                        egui::Label::new(
                            RichText::new(&self.story.chapters[self.chapter].name)
                                .size(20.0)
                                .strong(),
                        )
                        .truncate(),
                    )
                    .on_hover_text(&self.story.chapters[self.chapter].name);
                    ui.add_space(6.0);
                    ui.label("Platform");
                    ui.horizontal(|ui| {
                        for profile in [
                            export::ExportProfile::Webtoon,
                            export::ExportProfile::Tapas,
                            export::ExportProfile::Original,
                        ] {
                            ui.selectable_value(
                                &mut self.export_options.profile,
                                profile,
                                profile.label(),
                            );
                        }
                    });
                    ui.add_space(6.0);
                    ui.label("Format");
                    ui.horizontal(|ui| {
                        ui.selectable_value(
                            &mut self.export_options.format,
                            export::ExportFormat::Png,
                            "PNG",
                        )
                        .on_hover_text("Lossless");
                        ui.selectable_value(
                            &mut self.export_options.format,
                            export::ExportFormat::Jpeg,
                            "JPEG",
                        )
                        .on_hover_text("Smaller files");
                    });
                    if self.export_options.format == export::ExportFormat::Jpeg {
                        ui.add(
                            egui::Slider::new(&mut self.export_options.jpeg_quality, 60..=100)
                                .text("JPEG quality"),
                        );
                    }
                    ui.add_space(12.0);
                    match self.export_options.plan(&self.story.chapters[self.chapter]) {
                        Ok(plan) => {
                            egui::Frame::new()
                                .fill(CARD)
                                .inner_margin(14.0)
                                .corner_radius(8.0)
                                .show(ui, |ui| {
                                    ui.label(format!(
                                        "{} px wide · {}+ images",
                                        plan.width, plan.minimum_slices
                                    ));
                                    ui.collapsing("Details", |ui| {
                                        ui.label(format!("{} px total height", plan.height));
                                        ui.label(format!(
                                            "Up to {} px per image",
                                            plan.slice_height
                                        ));
                                        if let Some(limit) = plan.per_file_limit {
                                            ui.label(format!(
                                                "Under {:.0} MB per image",
                                                limit as f64 / 1_000_000.0
                                            ));
                                        }
                                    });
                                });
                            for warning in &plan.warnings {
                                ui.label(
                                    RichText::new(warning).color(Color32::from_rgb(242, 193, 121)),
                                );
                            }
                            ui.add_space(12.0);
                            if ui
                                .add_enabled(
                                    self.job.is_none(),
                                    motion::Button::new(RichText::new("Export…").color(BG))
                                        .fill(ACCENT),
                                )
                                .clicked()
                            {
                                start = true;
                            }
                        }
                        Err(error) => {
                            ui.label(RichText::new(error.to_string()).color(Color32::LIGHT_RED));
                        }
                    }
                });
            self.export_dialog &= open;
            if start {
                self.export_dialog = false;
                self.export();
            }
        }
        if let Some(report) = self.export_report.clone() {
            let mut open = true;
            egui::Window::new("Export complete")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.set_width(500.0);
                    ui.label(
                        RichText::new(format!(
                            "{} images · {:.2} MB",
                            report.files.len(),
                            report.total_bytes as f64 / 1_000_000.0
                        ))
                        .size(22.0)
                        .strong(),
                    );
                    ui.label(format!(
                        "{} / {}",
                        report.profile.label(),
                        report.format.label()
                    ));
                    ui.add_space(10.0);
                    if report.compliant {
                        ui.label(
                            RichText::new(if report.profile == export::ExportProfile::Original {
                                "Original resolution"
                            } else {
                                "Size checks passed"
                            })
                            .color(ACCENT),
                        );
                    }
                    for warning in &report.warnings {
                        ui.label(RichText::new(warning).color(Color32::from_rgb(242, 193, 121)));
                    }
                    ui.add_space(12.0);
                    ui.add(egui::Label::new(report.directory.display().to_string()).wrap());
                    ui.horizontal(|ui| {
                        if motion::copy_button(
                            ui,
                            ("copy_export", &report.directory),
                            "Copy path",
                            false,
                        )
                        .clicked()
                        {
                            ctx.copy_text(report.directory.display().to_string());
                        }
                        if primary(ui, "Done").clicked() {
                            self.export_report = None;
                        }
                    });
                });
            if !open {
                self.export_report = None;
            }
        }
    }
}
