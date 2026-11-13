//! Addons in the app: starting enabled addons, answering them each frame, the proposal
//! review, and the Addons tab in Settings. With no addons installed only the Settings tab
//! exists, holding "Install from file…".

use super::*;
use scroll_studio::addons::{
    host::{self, Decision, Running, Status},
    process::AddonProcess,
    Installed, Manifest, Permission, Registry,
};
use scroll_studio::wire;
use std::sync::mpsc::Receiver;
use std::time::Duration;

/// A background install: its result, the version it replaces, and whether that was enabled.
type Installing = (Receiver<Result<Manifest, String>>, Option<Manifest>, bool);

/// A worked-out proposal review and what it was worked out for.
struct Review {
    addon: String,
    request: serde_json::Value,
    /// Story revision and document the changes were described against.
    revision: u64,
    epoch: u64,
    changes: Vec<Change>,
}

/// Addons known to this window.
pub(super) struct AddonState {
    /// `None` when this window has no addons folder (tests, or no per-user folder).
    registry: Option<Registry>,
    installed: Vec<Installed>,
    running: Vec<Running>,
    message: Option<String>,
    /// An install running in the background, and the manifest it replaces.
    installing: Option<Installing>,
    /// The review for the open proposal, worked out once per story state.
    review: Option<Review>,
    /// The star button's request box, when open.
    pub(super) assist: Option<super::assist_ui::Assist>,
    /// Each running addon's own settings, as it last described them.
    settings: std::collections::HashMap<String, Vec<SettingField>>,
    /// Requests sent to addons that the app is waiting on.
    waiting: Vec<(String, u64, Purpose)>,
    /// The addon whose Remove button is asking to confirm.
    confirm_remove: Option<String>,
}

/// Why the app asked an addon something.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Purpose {
    Settings,
    Assist,
}

/// One of an addon's own settings, as the addon describes it.
#[derive(Clone, Debug, PartialEq)]
struct SettingField {
    key: String,
    label: String,
    kind: SettingKind,
    value: serde_json::Value,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SettingKind {
    Text,
    Toggle,
    /// Read-only and copyable, like an address or a token.
    Info,
}

fn setting_fields(answer: &serde_json::Value) -> Vec<SettingField> {
    answer["fields"]
        .as_array()
        .map(|fields| {
            fields
                .iter()
                .take(32)
                .filter_map(|field| {
                    Some(SettingField {
                        key: field["key"].as_str()?.chars().take(64).collect(),
                        label: field["label"].as_str()?.chars().take(64).collect(),
                        kind: match field["kind"].as_str()? {
                            "text" => SettingKind::Text,
                            "toggle" => SettingKind::Toggle,
                            "info" => SettingKind::Info,
                            _ => return None,
                        },
                        value: field["value"].clone(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

impl AddonState {
    pub(super) fn new(registry: Option<Registry>) -> Self {
        let installed = registry.as_ref().map(Registry::list).unwrap_or_default();
        Self {
            registry,
            installed,
            running: Vec::new(),
            message: None,
            installing: None,
            review: None,
            assist: None,
            settings: Default::default(),
            waiting: Vec::new(),
            confirm_remove: None,
        }
    }

    /// An addon may start only when every addon it `requires` is installed and usable.
    fn check_requirements(&self, id: &str) -> anyhow::Result<()> {
        let Some(Ok(manifest)) = self
            .installed
            .iter()
            .find(|i| i.id == id)
            .map(|i| i.manifest.as_ref())
        else {
            return Ok(()); // reported where the manifest is read
        };
        let missing: Vec<_> = manifest
            .requires
            .iter()
            .filter(|needed| {
                !self
                    .installed
                    .iter()
                    .any(|i| &i.id == *needed && i.manifest.is_ok())
            })
            .map(String::as_str)
            .collect();
        anyhow::ensure!(
            missing.is_empty(),
            "{} needs {} installed first",
            manifest.name,
            missing.join(", ")
        );
        Ok(())
    }

    /// The running addon that offers the star button, and its label.
    pub(super) fn assistant(&self) -> Option<(String, String)> {
        self.running.iter().find_map(|r| {
            let label = r.manifest.assist.clone()?;
            (r.status == Status::Running).then(|| (r.manifest.id.clone(), label))
        })
    }

    /// Ask a running addon something; `None` if it is not running or too busy.
    pub(super) fn ask(
        &mut self,
        addon: &str,
        method: &str,
        params: serde_json::Value,
        purpose: Purpose,
    ) -> Option<u64> {
        let running = self.running.iter_mut().find(|r| r.manifest.id == addon)?;
        let id = running.request(method, params)?;
        self.waiting.push((addon.to_owned(), id, purpose));
        Some(id)
    }

    pub(super) fn cancel(&mut self, addon: &str, id: u64) {
        self.waiting.retain(|(a, i, _)| !(a == addon && *i == id));
        if let Some(running) = self.running.iter_mut().find(|r| r.manifest.id == addon) {
            running.cancel_request(id);
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

/// One change in a proposal as the user sees it.
#[derive(Debug, PartialEq)]
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
                let message = format!("{id}: {error:#}");
                self.status = message.clone();
                self.addons.message = Some(message);
            }
        }
    }

    fn start_addon(&mut self, ctx: &egui::Context, id: &str) -> anyhow::Result<()> {
        self.stop_addon(id, "Restarted");
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
        self.addons.check_requirements(id)?;
        let program = manifest.entry_in(&installed.folder)?;
        let wake = ctx.clone();
        let process = AddonProcess::start(
            &program,
            &installed.folder,
            Arc::new(move || wake.request_repaint()),
        )?;
        self.addons
            .running
            .push(Running::new(manifest, process, self.revision));
        self.addons.settings.remove(id);
        self.addons.ask(
            id,
            "addon.settings",
            serde_json::json!({}),
            Purpose::Settings,
        );
        Ok(())
    }

    /// Answer running addons, pass on story changes and finish background installs.
    /// Cheap when no addons are installed.
    pub(super) fn poll_addons(&mut self, ctx: &egui::Context) {
        let changes = self.take_story_changes();
        if let Some((receiver, _, _)) = &self.addons.installing {
            let result = match receiver.try_recv() {
                Ok(result) => Some(result),
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
                // The install thread ended without an answer.
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    Some(Err("The install stopped unexpectedly".into()))
                }
            };
            if let Some(result) = result {
                let (_, previous, was_enabled) = self.addons.installing.take().expect("checked");
                self.finish_install(ctx, result, previous, was_enabled);
            } else {
                ctx.request_repaint_after(Duration::from_millis(100));
            }
        }
        if self.addons.running.is_empty() {
            return;
        }
        let mut more = false;
        let mut answers = Vec::new();
        let mut stopped = None;
        for running in &mut self.addons.running {
            let was_running = running.status == Status::Running;
            more |= running.poll(&self.story).more;
            for change in &changes {
                running.story_changed(change);
            }
            // Said once, when an addon stops by itself; Settings → Addons can restart it.
            if let (true, Status::Stopped(reason)) = (was_running, &running.status) {
                stopped = Some(format!("{}: {reason}", running.manifest.name));
            }
            let id = running.manifest.id.clone();
            answers.extend(running.take_answers().into_iter().map(|a| (id.clone(), a)));
        }
        if let Some(message) = stopped {
            self.status = message.chars().take(200).collect();
        }
        for (addon, (id, outcome)) in answers {
            let Some(i) = self
                .addons
                .waiting
                .iter()
                .position(|(a, i, _)| *a == addon && *i == id)
            else {
                continue;
            };
            let (_, _, purpose) = self.addons.waiting.remove(i);
            match purpose {
                // An addon without settings answers with an error: it just has none.
                Purpose::Settings => {
                    let fields = outcome.map(|a| setting_fields(&a)).unwrap_or_default();
                    self.addons.settings.insert(addon, fields);
                }
                Purpose::Assist => self.finish_assist(id, outcome),
            }
        }
        if more {
            // Anything left over is answered next frame, keeping each frame short.
            ctx.request_repaint();
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
            self.addons.review = None;
            return;
        };
        let running = &self.addons.running[index];
        let name = running.manifest.name.clone();
        let addon = running.manifest.id.clone();
        let proposal = running.proposal.clone().expect("found above");
        // Worked out again whenever the story changes, so Apply does what is shown.
        let current = self.addons.review.as_ref().is_some_and(|r| {
            r.addon == addon
                && r.request == proposal.request_id
                && r.revision == self.revision
                && r.epoch == self.document_epoch
        });
        if !current {
            self.addons.review = Some(Review {
                addon: addon.clone(),
                request: proposal.request_id.clone(),
                revision: self.revision,
                epoch: self.document_epoch,
                changes: describe(&self.story, &proposal.command),
            });
        }
        let changes = &self.addons.review.as_ref().expect("set above").changes;
        let mut decision = None;
        let modal = egui::Modal::new(egui::Id::new("addon_proposal")).show(ctx, |ui| {
            ui.set_width((ctx.screen_rect().width() - 64.0).clamp(300.0, 680.0));
            ui.heading(format!("{name} suggests a change"));
            ui.label(
                RichText::new(if changes.len() == 1 {
                    "1 change".to_owned()
                } else {
                    format!("{} changes", changes.len())
                })
                .color(MUTED),
            );
            ui.add_space(8.0);
            egui::ScrollArea::vertical()
                .id_salt("addon_proposal_changes")
                .max_height((ctx.screen_rect().height() - 260.0).clamp(120.0, 460.0))
                .show(ui, |ui| {
                    for (i, change) in changes.iter().enumerate() {
                        ui.label(RichText::new(&change.title).strong());
                        // Full text, so nothing applied is hidden; long text scrolls.
                        for (label, text) in [("Before", &change.before), ("After", &change.after)]
                        {
                            if let Some(text) = text {
                                ui.label(RichText::new(label).size(12.0).color(MUTED));
                                egui::ScrollArea::vertical()
                                    .id_salt(("addon_change", i, label))
                                    .max_height(140.0)
                                    .show(ui, |ui| {
                                        ui.add(
                                            egui::Label::new(if text.is_empty() {
                                                "(empty)"
                                            } else {
                                                text.as_str()
                                            })
                                            .selectable(true)
                                            .wrap(),
                                        );
                                    });
                            }
                        }
                        ui.add_space(8.0);
                    }
                });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if motion::button(ui, "Cancel").clicked() {
                    decision = Some(Choice::Cancel);
                }
                if motion::button(ui, "Stop addon")
                    .on_hover_text("Cancel and stop this addon until you restart it")
                    .clicked()
                {
                    decision = Some(Choice::Stop);
                }
                if ui
                    .add(motion::Button::new(RichText::new("Apply").color(BG)).fill(ACCENT))
                    .clicked()
                {
                    decision = Some(Choice::Apply);
                }
            });
        });
        if decision.is_none() && modal.should_close() {
            decision = Some(Choice::Cancel);
        }
        let Some(choice) = decision else {
            return;
        };
        self.addons.review = None;
        match choice {
            Choice::Apply => {
                // The story may have changed since the addon asked: check again, including
                // locks, before applying.
                let result = host::check_edit(&self.story, &proposal.command).and_then(|()| {
                    self.apply_command(proposal.command)
                        .map_err(|e| format!("{e:#}"))
                });
                let decision = match result {
                    Ok(applied) => {
                        self.settle_after_outside_change();
                        Decision::Applied(Box::new(applied))
                    }
                    Err(error) => {
                        self.status = format!("{name}'s change could not be applied: {error}");
                        Decision::Failed(error)
                    }
                };
                self.addons.running[index].resolve(decision);
            }
            Choice::Cancel => {
                let running = &mut self.addons.running[index];
                let was_paused = running.suggestions_paused();
                running.resolve(Decision::Cancelled);
                if !was_paused && running.suggestions_paused() {
                    self.status =
                        format!("{name} suggestions paused; allow them again in Settings → Addons");
                }
            }
            Choice::Stop => {
                self.addons.running[index].resolve(Decision::Cancelled);
                self.addons.running[index].stop("Stopped from its suggestion".into());
            }
        }
    }

    /// Settings → Addons.
    pub(super) fn addons_settings(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        ui.horizontal(|ui| {
            let installing = self.addons.installing.is_some();
            let can_install = self.addons.registry.is_some() && !installing;
            if ui
                .add_enabled(
                    can_install,
                    motion::Button::new(if installing {
                        "Installing…"
                    } else {
                        "Install from file…"
                    }),
                )
                .clicked()
            {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Scroll Studio addon", &["zip"])
                    .pick_file()
                {
                    self.install_addon(&path);
                }
            }
        });
        if let Some(message) = &self.addons.message {
            ui.add_space(6.0);
            ui.label(RichText::new(message).color(MUTED));
        }
        ui.add_space(12.0);
        let mut action = None;
        // Nothing may touch an addon's files while an install could be swapping them.
        let busy = self.addons.installing.is_some();
        for installed in &self.addons.installed {
            let running = self.addons.running(&installed.id);
            let confirming = self.addons.confirm_remove.as_deref() == Some(installed.id.as_str());
            ui.add_enabled_ui(!busy, |ui| {
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
                                            remove_buttons(
                                                ui,
                                                &installed.id,
                                                confirming,
                                                &mut action,
                                            );
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
                                        let paused =
                                            running.is_some_and(|r| r.suggestions_paused());
                                        if paused {
                                            ui.horizontal_wrapped(|ui| {
                                                ui.label(
                                                    RichText::new("Suggestions paused")
                                                        .color(MUTED),
                                                )
                                                .on_hover_text("You turned down several in a row");
                                                if motion::button(ui, "Allow again").clicked() {
                                                    action = Some(AddonAction::Allow(
                                                        installed.id.clone(),
                                                    ));
                                                }
                                            });
                                        } else {
                                            ui.label(RichText::new("Running").color(ACCENT));
                                        }
                                        if let Some(fields) =
                                            self.addons.settings.get(&installed.id)
                                        {
                                            if !fields.is_empty() {
                                                addon_settings_ui(
                                                    ui,
                                                    &installed.id,
                                                    fields,
                                                    &mut action,
                                                );
                                            }
                                        }
                                    }
                                    Some(Status::Stopped(reason)) => {
                                        ui.horizontal_wrapped(|ui| {
                                            ui.label(RichText::new(reason).color(MUTED));
                                            if motion::button(ui, "Restart").clicked() {
                                                action = Some(AddonAction::Restart(
                                                    installed.id.clone(),
                                                ));
                                            }
                                        });
                                    }
                                    // Enabled but not running: it failed to start.
                                    None if installed.enabled => {
                                        ui.horizontal_wrapped(|ui| {
                                            ui.label(RichText::new("Not running").color(MUTED));
                                            if motion::button(ui, "Start").clicked() {
                                                action = Some(AddonAction::Restart(
                                                    installed.id.clone(),
                                                ));
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
                                            remove_buttons(
                                                ui,
                                                &installed.id,
                                                confirming,
                                                &mut action,
                                            );
                                        },
                                    );
                                });
                                ui.label(
                                    RichText::new(format!("Cannot be used: {reason}")).color(MUTED),
                                );
                            }
                        }
                    })
            });
            ui.add_space(8.0);
        }
        if busy {
            action = None;
        }
        if let Some(action) = action {
            self.addon_action(&ctx, action);
        }
    }

    /// Install in the background. A running copy is stopped first so its files can be
    /// replaced; it restarts afterwards unless the new version asks for new permissions.
    fn install_addon(&mut self, path: &std::path::Path) {
        let Some(registry) = self.addons.registry.clone() else {
            return;
        };
        // Only the manifest decides the ID, so peek at it to stop the right addon.
        let peek = peek_manifest(path);
        let (previous, was_enabled) = match &peek {
            Some(id) => {
                let installed = self.addons.installed.iter().find(|i| &i.id == id);
                let previous = installed.and_then(|i| i.manifest.clone().ok());
                let was_enabled = installed.is_some_and(|i| i.enabled);
                self.stop_addon(id, "Updating");
                (previous, was_enabled)
            }
            None => (None, false),
        };
        let (sender, receiver) = std::sync::mpsc::channel();
        let path = path.to_path_buf();
        std::thread::spawn(move || {
            let _ = sender.send(registry.install_zip(&path).map_err(|e| format!("{e:#}")));
        });
        self.addons.installing = Some((receiver, previous, was_enabled));
        self.addons.message = Some("Installing…".into());
    }

    fn finish_install(
        &mut self,
        ctx: &egui::Context,
        result: Result<Manifest, String>,
        previous: Option<Manifest>,
        was_enabled: bool,
    ) {
        self.addons.refresh();
        let manifest = match result {
            Ok(manifest) => manifest,
            Err(error) => {
                self.addons.message = Some(format!("Could not install: {error}"));
                // The old version is still installed: bring it back as it was.
                if let Some(previous) = previous.filter(|_| was_enabled) {
                    if let Err(error) = self.start_addon(ctx, &previous.id) {
                        self.addons.message = Some(format!(
                            "Could not install: {error}. {}: {error:#}",
                            previous.name
                        ));
                    }
                }
                return;
            }
        };
        let note = "Addons run as programs on your computer; enable only ones you trust.";
        let same_permissions = previous
            .as_ref()
            .is_some_and(|p| p.permissions == manifest.permissions);
        if was_enabled && same_permissions {
            self.addons.message = Some(format!(
                "Updated {} to {}.",
                manifest.name, manifest.version
            ));
            if let Err(error) = self.start_addon(ctx, &manifest.id) {
                self.addons.message = Some(format!("{}: {error:#}", manifest.name));
            }
            return;
        }
        if was_enabled {
            // New permissions need the user's yes again.
            if let Some(registry) = &self.addons.registry {
                let _ = registry.set_enabled(&manifest.id, false);
            }
            self.addons.refresh();
            self.addons.message = Some(format!(
                "Updated {} to {}. It asks for different permissions, so it is off until you enable it. {note}",
                manifest.name, manifest.version
            ));
            return;
        }
        self.addons.message = Some(format!(
            "Installed {} {}. {note}",
            manifest.name, manifest.version
        ));
    }

    fn addon_action(&mut self, ctx: &egui::Context, action: AddonAction) {
        let Some(registry) = self.addons.registry.clone() else {
            return;
        };
        let result = match &action {
            AddonAction::Enable(id, true) => self
                .addons
                .check_requirements(id)
                .and_then(|()| registry.set_enabled(id, true))
                .and_then(|_| {
                    self.addons.refresh();
                    self.start_addon(ctx, id)
                }),
            AddonAction::Enable(id, false) => {
                self.stop_addon(id, "Disabled");
                registry.set_enabled(id, false)
            }
            AddonAction::Restart(id) => self.start_addon(ctx, id),
            AddonAction::AskRemove(id) => {
                self.addons.confirm_remove = Some(id.clone());
                return;
            }
            AddonAction::KeepInstalled => {
                self.addons.confirm_remove = None;
                return;
            }
            AddonAction::Allow(id) => {
                if let Some(running) = self
                    .addons
                    .running
                    .iter_mut()
                    .find(|r| &r.manifest.id == id)
                {
                    running.allow_suggestions();
                }
                return;
            }
            AddonAction::Set(id, key, value) => {
                self.addons.ask(
                    id,
                    "addon.set_setting",
                    serde_json::json!({ "key": key, "value": value }),
                    Purpose::Settings,
                );
                return;
            }
            AddonAction::Remove(id) => {
                self.addons.confirm_remove = None;
                self.stop_addon(id, "Removed");
                // Addons that need this one cannot keep running without it.
                let dependents: Vec<_> = self
                    .addons
                    .running
                    .iter()
                    .filter(|r| r.manifest.requires.contains(id))
                    .map(|r| r.manifest.id.clone())
                    .collect();
                for dependent in dependents {
                    self.stop_addon(&dependent, &format!("Needs {id}, which was removed"));
                }
                registry.remove(id)
            }
        };
        self.addons.refresh();
        if let Err(error) = result {
            self.addons.message = Some(format!("{error:#}"));
        }
    }

    /// Whether a suggestion is waiting for the user's decision.
    pub(super) fn addon_proposal_open(&self) -> bool {
        self.addons.running.iter().any(|r| r.proposal.is_some())
    }

    /// Turn down every waiting suggestion, for example when another story opens.
    pub(super) fn fail_addon_proposals(&mut self, reason: &str) {
        for running in &mut self.addons.running {
            running.resolve(Decision::Failed(reason.into()));
        }
        self.addons.review = None;
    }

    /// Stop an addon, telling it about anything it had waiting.
    fn stop_addon(&mut self, id: &str, reason: &str) {
        if let Some(running) = self.addons.running.iter_mut().find(|r| r.manifest.id == id) {
            running.resolve(Decision::Failed("The addon was stopped".into()));
            running.stop(reason.into());
        }
        self.addons.running.retain(|r| r.manifest.id != id);
        // A restarted addon numbers its requests from 1 again: nothing sent to the old one
        // may be matched with the new one's answers.
        self.addons.waiting.retain(|(addon, _, _)| addon != id);
        if let Some(assist) = self.addons.assist.as_mut().filter(|a| a.addon == id) {
            if assist.request.take().is_some() {
                assist.error = Some(format!("{reason}; send again when it is running"));
            }
        }
    }
}

enum Choice {
    Apply,
    Cancel,
    Stop,
}

enum AddonAction {
    Enable(String, bool),
    Restart(String),
    Remove(String),
    Set(String, String, serde_json::Value),
    Allow(String),
    AskRemove(String),
    KeepInstalled,
}

/// Remove asks once more before deleting, in place of the button.
fn remove_buttons(ui: &mut egui::Ui, id: &str, confirming: bool, action: &mut Option<AddonAction>) {
    if !confirming {
        if motion::button(ui, "Remove").clicked() {
            *action = Some(AddonAction::AskRemove(id.to_owned()));
        }
        return;
    }
    // Right to left: the confirming button sits where Remove was.
    if motion::button(ui, "Confirm remove").clicked() {
        *action = Some(AddonAction::Remove(id.to_owned()));
    }
    if motion::button(ui, "Keep").clicked() {
        *action = Some(AddonAction::KeepInstalled);
    }
}

/// An addon's own settings, behind a disclosure so the card stays short.
fn addon_settings_ui(
    ui: &mut egui::Ui,
    id: &str,
    fields: &[SettingField],
    action: &mut Option<AddonAction>,
) {
    egui::CollapsingHeader::new("Settings")
        .id_salt(("addon_settings", id))
        .show(ui, |ui| {
            egui::Grid::new(("addon_settings_grid", id))
                .num_columns(2)
                .spacing([12.0, 8.0])
                .show(ui, |ui| {
                    for field in fields {
                        ui.label(RichText::new(&field.label).color(MUTED));
                        match field.kind {
                            SettingKind::Toggle => {
                                let mut on = field.value.as_bool().unwrap_or(false);
                                if ui.checkbox(&mut on, "").changed() {
                                    *action = Some(AddonAction::Set(
                                        id.to_owned(),
                                        field.key.clone(),
                                        on.into(),
                                    ));
                                }
                            }
                            SettingKind::Text => {
                                let draft_id = egui::Id::new(("addon_setting", id, &field.key));
                                let current = field.value.as_str().unwrap_or_default().to_owned();
                                let mut draft = ui
                                    .data_mut(|d| d.get_temp::<String>(draft_id))
                                    .unwrap_or_else(|| current.clone());
                                let edit = ui.add(
                                    egui::TextEdit::singleline(&mut draft)
                                        .char_limit(512)
                                        .desired_width(240.0),
                                );
                                if edit.has_focus() {
                                    ui.data_mut(|d| d.insert_temp(draft_id, draft));
                                } else {
                                    if edit.lost_focus() && draft != current {
                                        *action = Some(AddonAction::Set(
                                            id.to_owned(),
                                            field.key.clone(),
                                            draft.into(),
                                        ));
                                    }
                                    ui.data_mut(|d| d.remove::<String>(draft_id));
                                }
                            }
                            SettingKind::Info => {
                                let text = match &field.value {
                                    serde_json::Value::String(text) => text.clone(),
                                    serde_json::Value::Null => String::new(),
                                    other => other.to_string(),
                                };
                                ui.horizontal(|ui| {
                                    ui.add(egui::Label::new(&text).selectable(true).truncate());
                                    if !text.is_empty()
                                        && motion::copy_button(
                                            ui,
                                            ("addon_setting_copy", id, &field.key),
                                            "Copy",
                                            false,
                                        )
                                        .clicked()
                                    {
                                        ui.ctx().copy_text(text.clone());
                                    }
                                });
                            }
                        }
                        ui.end_row();
                    }
                });
        });
}

/// The addon ID a package declares, without installing it.
fn peek_manifest(path: &std::path::Path) -> Option<String> {
    use std::io::Read;
    let file = std::fs::File::open(path).ok()?;
    let mut zip = zip::ZipArchive::new(file).ok()?;
    let mut entry = zip
        .by_name(scroll_studio::addons::manifest::FILE_NAME)
        .ok()?;
    let mut bytes = Vec::new();
    (&mut entry)
        .take(64 * 1024 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    Manifest::parse(&bytes).ok().map(|m| m.id)
}

/// Describe a proposal from a rehearsal on a copy: every text field it would change, with
/// the full text before and after, then every change that is not plain text.
fn describe(story: &Story, command: &Command) -> Vec<Change> {
    let mut rehearsal = story.clone();
    if let Err(error) = rehearsal.apply(command.clone()) {
        return vec![Change {
            title: "This suggestion no longer applies".into(),
            before: None,
            after: Some(format!("{error:#}")),
        }];
    }
    let mut changes = Vec::new();
    for (target, field) in story.text_fields() {
        let before = story.field(target, field).ok();
        let after = rehearsal.field(target, field).ok();
        if before != after {
            changes.push(Change {
                title: format!(
                    "{} · {}",
                    object_label(story, target),
                    humanize(field.as_str())
                ),
                before: Some(before.unwrap_or("(removed)").to_owned()),
                after: Some(after.unwrap_or("(removed)").to_owned()),
            });
        }
    }
    let mut parts = Vec::new();
    flatten(command, &mut parts);
    for part in parts {
        if matches!(part, Command::SetField { .. } | Command::ReplaceText { .. }) {
            continue; // shown above as the text it changes
        }
        let (title, details) = match wire::command_request(part) {
            Ok((method, params)) => (
                humanize(method.trim_start_matches(wire::EDIT_PREFIX)),
                (!params.is_null()).then(|| readable(story, &params)),
            ),
            Err(_) => (humanize(part.name()), None),
        };
        changes.push(Change {
            title,
            before: None,
            after: details,
        });
    }
    changes
}

fn flatten<'a>(command: &'a Command, out: &mut Vec<&'a Command>) {
    match command {
        Command::Batch(parts) => parts.iter().for_each(|part| flatten(part, out)),
        other => out.push(other),
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

/// Parameters as lines a person can read: `Title: The flooded station`, objects by name.
fn readable(story: &Story, params: &serde_json::Value) -> String {
    use serde_json::Value;
    fn text(story: &Story, key: &str, value: &Value) -> String {
        match value {
            Value::String(text) => text.clone(),
            Value::Bool(on) => if *on { "Yes" } else { "No" }.into(),
            Value::Null => "None".into(),
            Value::Number(n) if key.ends_with("_id") => n
                .as_u64()
                .map(|id| object_label(story, Target::Object(id)))
                .unwrap_or_else(|| n.to_string()),
            Value::Number(n) => n.to_string(),
            Value::Array(items) => items
                .iter()
                .map(|item| text(story, key.trim_end_matches('s'), item))
                .collect::<Vec<_>>()
                .join(", "),
            Value::Object(map) => map
                .iter()
                .map(|(k, v)| format!("{}: {}", humanize(k), text(story, k, v)))
                .collect::<Vec<_>>()
                .join(" · "),
        }
    }
    match params {
        Value::Object(map) => map
            .iter()
            .map(|(key, v)| {
                let label = humanize(key.trim_end_matches("_id").trim_end_matches("_ids"));
                format!("{label}: {}", text(story, key, v))
            })
            .collect::<Vec<_>>()
            .join("\n"),
        other => text(story, "", other),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reviews_show_every_text_change_in_full_and_each_structural_change() {
        let story = Story::demo();
        let long = "word ".repeat(2000);
        let changes = describe(
            &story,
            &Command::Batch(vec![
                Command::SetField {
                    target: Target::Story,
                    field: Field::Synopsis,
                    value: long.clone(),
                },
                Command::AddGroup,
            ]),
        );
        assert_eq!(changes.len(), 2);
        assert_eq!(changes[0].title, "Story “After the rain” · Synopsis");
        assert_eq!(changes[0].before.as_deref(), Some(story.synopsis.as_str()));
        assert_eq!(
            changes[0].after.as_deref(),
            Some(long.as_str()),
            "no truncation"
        );
        assert_eq!(changes[1].title, "Add group");
        // A find-and-replace lists every field it would change, not just the pattern.
        let replaced = describe(
            &story,
            &Command::ReplaceText {
                find: "Mira".into(),
                replace: "Mina".into(),
                scope: commands::Scope::Story,
            },
        );
        let expected = story
            .text_fields()
            .into_iter()
            .filter(|(t, f)| story.field(*t, *f).unwrap().contains("Mira"))
            .count();
        assert!(expected > 0);
        assert_eq!(replaced.len(), expected);
        assert!(replaced
            .iter()
            .all(|c| c.after.as_deref().unwrap().contains("Mina")));
    }

    #[test]
    fn structural_changes_read_as_plain_lines_with_object_names() {
        let story = Story::demo();
        let character = story
            .assets
            .iter()
            .find(|a| a.kind == AssetKind::Character)
            .unwrap()
            .clone();
        let changes = describe(
            &story,
            &Command::Batch(vec![
                Command::AddLoreNote {
                    title: "The flooded station".into(),
                    position: [40.0, 40.0],
                },
                Command::AddPaletteColor {
                    character: character.id,
                    name: "Coat".into(),
                    color: [40, 40, 48, 255],
                },
            ]),
        );
        let note = changes[0].after.as_deref().unwrap();
        assert!(note.contains("Title: The flooded station"), "{note}");
        assert!(note.contains("Position: 40.0, 40.0"), "{note}");
        let color = changes[1].after.as_deref().unwrap();
        assert!(
            color.contains(&format!("Character: Character “{}”", character.name)),
            "{color}"
        );
        assert!(!color.contains('{'), "no raw JSON: {color}");
    }

    #[test]
    fn a_stopped_addon_leaves_nothing_to_match_its_successors_answers() {
        let ctx = egui::Context::default();
        let mut studio = Studio::from_context(&ctx);
        studio
            .addons
            .waiting
            .push(("ai-lite".into(), 2, Purpose::Assist));
        studio
            .addons
            .waiting
            .push(("other".into(), 2, Purpose::Settings));
        studio.stop_addon("ai-lite", "Restarted");
        assert_eq!(
            studio.addons.waiting,
            vec![("other".to_owned(), 2, Purpose::Settings)]
        );
    }

    #[test]
    fn an_addon_needing_another_only_starts_once_that_one_is_installed() {
        let manifest = |id: &str, requires: &[&str]| Manifest {
            id: id.into(),
            name: id.to_uppercase(),
            version: "1.0.0".into(),
            min_app: "0.1.0".into(),
            requires: requires.iter().map(|r| r.to_string()).collect(),
            entry: "run.exe".into(),
            permissions: Vec::new(),
            download_mb: 0.0,
            hardware: String::new(),
            assist: None,
        };
        let installed = |m: Manifest| Installed {
            id: m.id.clone(),
            folder: std::path::PathBuf::from(&m.id),
            manifest: Ok(m),
            size_bytes: 0,
            enabled: false,
        };
        let mut state = AddonState::new(None);
        state.installed = vec![installed(manifest("artist", &["models"]))];
        let error = state.check_requirements("artist").unwrap_err().to_string();
        assert!(error.contains("ARTIST needs models"), "{error}");
        state.installed.push(installed(manifest("models", &[])));
        assert!(state.check_requirements("artist").is_ok());
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
            studio.poll_addons(&ctx);
        }
        assert!(studio.story_changes.is_empty());
        // No proposal, no review.
        let _ = ctx.run(Default::default(), |ctx| studio.addon_proposal_ui(ctx));
        assert!(studio.addons.review.is_none());
    }
}
