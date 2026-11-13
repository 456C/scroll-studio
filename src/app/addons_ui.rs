//! Addons in the app: starting enabled addons, answering them each frame, the proposal
//! review, and the Addons tab in Settings. With no addons installed only the Settings tab
//! exists, holding "Install from file…".

use super::*;
use scroll_studio::addons::{
    host::{Running, Status},
    process::AddonProcess,
    Installed, Permission, Registry,
};
use scroll_studio::wire;

/// Addons known to this window.
pub(super) struct AddonState {
    /// `None` when this window has no addons folder (tests, or no per-user folder).
    registry: Option<Registry>,
    installed: Vec<Installed>,
    running: Vec<Running>,
    message: Option<String>,
}

impl AddonState {
    pub(super) fn new(registry: Option<Registry>) -> Self {
        let installed = registry.as_ref().map(Registry::list).unwrap_or_default();
        Self {
            registry,
            installed,
            running: Vec::new(),
            message: None,
        }
    }

    fn refresh(&mut self) {
        if let Some(registry) = &self.registry {
            self.installed = registry.list();
        }
    }

    fn running(&self, id: &str) -> Option<&Running> {
        self.running.iter().find(|r| r.manifest.id == id)
    }
}

/// One line of a proposal as the user sees it.
struct Change {
    title: String,
    before: Option<String>,
    after: Option<String>,
}

impl Studio {
    /// Start every enabled addon. Called once when the window opens.
    pub(super) fn start_enabled_addons(&mut self, ctx: &egui::Context) {
        let ids: Vec<_> = self
            .addons
            .installed
            .iter()
            .filter(|i| i.enabled)
            .map(|i| i.id.clone())
            .collect();
        for id in ids {
            if let Err(error) = self.start_addon(ctx, &id) {
                self.addons.message = Some(format!("{id}: {error:#}"));
            }
        }
    }

    fn start_addon(&mut self, ctx: &egui::Context, id: &str) -> anyhow::Result<()> {
        self.addons.running.retain(|r| r.manifest.id != id);
        let installed = self
            .addons
            .installed
            .iter()
            .find(|i| i.id == id)
            .ok_or_else(|| anyhow::anyhow!("That addon is no longer installed"))?;
        let manifest = installed
            .manifest
            .clone()
            .map_err(|error| anyhow::anyhow!(error))?;
        let program = manifest.entry_in(&installed.folder)?;
        let ctx = ctx.clone();
        let process = AddonProcess::start(
            &program,
            &installed.folder,
            Arc::new(move || ctx.request_repaint()),
        )?;
        self.addons.running.push(Running::new(manifest, process));
        Ok(())
    }

    /// Answer running addons and pass on story changes. Cheap when none are running.
    pub(super) fn poll_addons(&mut self) {
        let changes = self.take_story_changes();
        if self.addons.running.is_empty() {
            return;
        }
        for running in &mut self.addons.running {
            running.poll(&self.story);
            for change in &changes {
                running.story_changed(change);
            }
        }
    }

    /// The review for the first waiting proposal: what will change, then Apply or Cancel.
    pub(super) fn addon_proposal_ui(&mut self, ctx: &egui::Context) {
        let Some(index) = self
            .addons
            .running
            .iter()
            .position(|r| r.proposal.is_some())
        else {
            return;
        };
        let name = self.addons.running[index].manifest.name.clone();
        let command = self.addons.running[index]
            .proposal
            .as_ref()
            .expect("found above")
            .command
            .clone();
        let changes = describe(&self.story, &command);
        let mut decision = None;
        let modal = egui::Modal::new(egui::Id::new("addon_proposal")).show(ctx, |ui| {
            ui.set_width((ctx.screen_rect().width() - 64.0).clamp(300.0, 620.0));
            ui.heading(format!("{name} suggests a change"));
            ui.label(
                RichText::new(if changes.len() == 1 {
                    "1 change. Apply it as one step you can undo.".to_owned()
                } else {
                    format!(
                        "{} changes. Apply them as one step you can undo.",
                        changes.len()
                    )
                })
                .color(MUTED),
            );
            ui.add_space(8.0);
            egui::ScrollArea::vertical()
                .max_height((ctx.screen_rect().height() - 260.0).clamp(120.0, 420.0))
                .show(ui, |ui| {
                    for change in &changes {
                        ui.label(RichText::new(&change.title).strong());
                        if let Some(before) = &change.before {
                            ui.label(RichText::new(format!("Before: {before}")).color(MUTED));
                        }
                        if let Some(after) = &change.after {
                            ui.label(format!("After: {after}"));
                        }
                        ui.add_space(6.0);
                    }
                });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if motion::button(ui, "Cancel").clicked() {
                    decision = Some(false);
                }
                if ui
                    .add(motion::Button::new(RichText::new("Apply").color(BG)).fill(ACCENT))
                    .clicked()
                {
                    decision = Some(true);
                }
            });
        });
        if decision.is_none() && modal.should_close() {
            decision = Some(false);
        }
        match decision {
            Some(true) => {
                // The story may have changed since the addon asked; apply re-checks it.
                let result = self.apply_command(command).map_err(|e| format!("{e:#}"));
                if let Err(error) = &result {
                    self.status = format!("{name}'s change could not be applied: {error}");
                }
                self.addons.running[index].resolve(result);
            }
            Some(false) => {
                self.addons.running[index].resolve(Err("Cancelled by the user".into()));
            }
            None => {}
        }
    }

    /// Settings → Addons.
    pub(super) fn addons_settings(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        ui.horizontal(|ui| {
            let can_install = self.addons.registry.is_some();
            if ui
                .add_enabled(can_install, motion::Button::new("Install from file…"))
                .clicked()
            {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Scroll Studio addon", &["zip"])
                    .pick_file()
                {
                    self.install_addon(&ctx, &path);
                }
            }
        });
        if let Some(message) = &self.addons.message {
            ui.add_space(6.0);
            ui.label(RichText::new(message).color(MUTED));
        }
        ui.add_space(12.0);
        let mut action = None;
        for installed in &self.addons.installed {
            let running = self.addons.running(&installed.id);
            egui::Frame::new()
                .fill(CARD)
                .corner_radius(10.0)
                .inner_margin(12.0)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    match &installed.manifest {
                        Ok(manifest) => {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(&manifest.name).strong());
                                ui.label(
                                    RichText::new(format!(
                                        "{} · {:.1} MB",
                                        manifest.version,
                                        installed.size_bytes as f64 / 1e6
                                    ))
                                    .color(MUTED),
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if motion::button(ui, "Remove").clicked() {
                                            action =
                                                Some(AddonAction::Remove(installed.id.clone()));
                                        }
                                        let mut enabled = installed.enabled;
                                        if ui.checkbox(&mut enabled, "Enabled").changed() {
                                            action = Some(AddonAction::Enable(
                                                installed.id.clone(),
                                                enabled,
                                            ));
                                        }
                                    },
                                );
                            });
                            if !manifest.hardware.is_empty() {
                                ui.label(RichText::new(&manifest.hardware).color(MUTED));
                            }
                            let permissions: Vec<_> = manifest
                                .permissions
                                .iter()
                                .map(|p: &Permission| p.label())
                                .collect();
                            if !permissions.is_empty() {
                                ui.label(
                                    RichText::new(format!("Can: {}", permissions.join(" · ")))
                                        .color(MUTED),
                                );
                            }
                            match running.map(|r| &r.status) {
                                Some(Status::Running) => {
                                    ui.label(RichText::new("Running").color(ACCENT));
                                }
                                Some(Status::Stopped(reason)) => {
                                    ui.horizontal_wrapped(|ui| {
                                        ui.label(RichText::new(reason).color(MUTED));
                                        if motion::button(ui, "Restart").clicked() {
                                            action =
                                                Some(AddonAction::Restart(installed.id.clone()));
                                        }
                                    });
                                }
                                None => {}
                            }
                        }
                        Err(reason) => {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(&installed.id).strong());
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if motion::button(ui, "Remove").clicked() {
                                            action =
                                                Some(AddonAction::Remove(installed.id.clone()));
                                        }
                                    },
                                );
                            });
                            ui.label(
                                RichText::new(format!("Cannot be used: {reason}")).color(MUTED),
                            );
                        }
                    }
                });
            ui.add_space(8.0);
        }
        if let Some(action) = action {
            self.addon_action(&ctx, action);
        }
    }

    fn install_addon(&mut self, ctx: &egui::Context, path: &std::path::Path) {
        let Some(registry) = self.addons.registry.clone() else {
            return;
        };
        // Stop a running copy so its files can be replaced.
        let id = registry.install_zip(path);
        match id {
            Ok(manifest) => {
                let was_running = self.addons.running(&manifest.id).is_some();
                self.addons.running.retain(|r| r.manifest.id != manifest.id);
                self.addons.refresh();
                self.addons.message =
                    Some(format!("Installed {} {}", manifest.name, manifest.version));
                if was_running {
                    if let Err(error) = self.start_addon(ctx, &manifest.id) {
                        self.addons.message = Some(format!("{}: {error:#}", manifest.name));
                    }
                }
            }
            Err(error) => {
                self.addons.message = Some(format!("Could not install: {error:#}"));
            }
        }
    }

    fn addon_action(&mut self, ctx: &egui::Context, action: AddonAction) {
        let Some(registry) = self.addons.registry.clone() else {
            return;
        };
        let result = match &action {
            AddonAction::Enable(id, true) => registry.set_enabled(id, true).and_then(|_| {
                self.addons.refresh();
                self.start_addon(ctx, id)
            }),
            AddonAction::Enable(id, false) => {
                self.stop_addon(id);
                registry.set_enabled(id, false)
            }
            AddonAction::Restart(id) => self.start_addon(ctx, id),
            AddonAction::Remove(id) => {
                self.stop_addon(id);
                registry.remove(id)
            }
        };
        self.addons.refresh();
        self.addons.message = result.err().map(|error| format!("{error:#}"));
    }

    /// Stop an addon, cancelling anything it had waiting.
    fn stop_addon(&mut self, id: &str) {
        if let Some(running) = self.addons.running.iter_mut().find(|r| r.manifest.id == id) {
            running.resolve(Err("The addon was stopped".into()));
        }
        self.addons.running.retain(|r| r.manifest.id != id);
    }
}

enum AddonAction {
    Enable(String, bool),
    Restart(String),
    Remove(String),
}

/// Describe a proposal: one entry per change, with before and after for text.
fn describe(story: &Story, command: &Command) -> Vec<Change> {
    match command {
        Command::Batch(parts) => parts.iter().flat_map(|c| describe(story, c)).collect(),
        Command::SetField {
            target,
            field,
            value,
        } => vec![Change {
            title: format!(
                "{} · {}",
                object_label(story, *target),
                humanize(field.as_str())
            ),
            before: story.field(*target, *field).ok().map(preview),
            after: Some(preview(value)),
        }],
        other => {
            let (method, params) = wire::command_request(other).unwrap_or_default();
            let title = humanize(method.trim_start_matches(wire::EDIT_PREFIX));
            let details = params.to_string();
            vec![Change {
                title,
                before: None,
                after: (details != "null").then(|| preview(&details)),
            }]
        }
    }
}

fn object_label(story: &Story, target: Target) -> String {
    let Ok(object) = story.query(&scroll_studio::read_api::Query::ObjectGet {
        id: match target {
            Target::Story => commands::STORY_ID,
            Target::Object(id) => id,
        },
    }) else {
        return "Unknown object".into();
    };
    let kind = humanize(object["kind"].as_str().unwrap_or("object"));
    let name = object["fields"]["name"]
        .as_str()
        .or_else(|| object["fields"]["title"].as_str())
        .unwrap_or_default();
    if name.is_empty() {
        kind
    } else {
        format!("{kind} “{name}”")
    }
}

fn humanize(name: &str) -> String {
    let words = name.replace('_', " ");
    let mut chars = words.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// Long text, shortened for the review (the full text is still what applies).
fn preview(text: &str) -> String {
    const LIMIT: usize = 600;
    if text.is_empty() {
        return "(empty)".into();
    }
    if text.chars().count() <= LIMIT {
        return text.to_owned();
    }
    let short: String = text.chars().take(LIMIT).collect();
    format!("{short}…")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proposals_describe_text_changes_with_before_and_after() {
        let story = Story::demo();
        let changes = describe(
            &story,
            &Command::Batch(vec![
                Command::SetField {
                    target: Target::Story,
                    field: Field::Title,
                    value: "Rain".into(),
                },
                Command::AddGroup,
            ]),
        );
        assert_eq!(changes.len(), 2);
        assert_eq!(changes[0].title, "Story “After the rain” · Title");
        assert_eq!(changes[0].before.as_deref(), Some("After the rain"));
        assert_eq!(changes[0].after.as_deref(), Some("Rain"));
        assert_eq!(changes[1].title, "Add group");
        assert!(preview(&"x".repeat(5000)).chars().count() <= 601);
    }

    #[test]
    fn with_no_addons_nothing_runs_and_story_changes_do_not_pile_up() {
        let ctx = egui::Context::default();
        let mut studio = Studio::from_context(&ctx);
        assert!(studio.addons.installed.is_empty() && studio.addons.running.is_empty());
        for _ in 0..3 {
            studio
                .apply_command(Command::SetField {
                    target: Target::Story,
                    field: Field::Synopsis,
                    value: "x".into(),
                })
                .ok();
            studio.poll_addons();
        }
        assert!(studio.story_changes.is_empty());
        // No proposal, no review.
        let _ = ctx.run(Default::default(), |ctx| studio.addon_proposal_ui(ctx));
    }
}
