use super::*;
use scroll_studio::publish::{self, EpisodeKind, LinkError, LocalDestination, PublicMetadata};
use scroll_studio::publish_wire::{self, Origin};

pub(super) enum ServicePayload {
    SignedIn(publish_wire::Credential, publish_wire::AccountSnapshot),
    Noted {
        ack: publish_wire::Acknowledgement,
        message: String,
        status: Option<publish_wire::LiveStatus>,
    },
    Status(publish_wire::LiveStatus, String),
}

pub(super) type SignInResult = Result<ServicePayload, String>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PublishKind {
    Chapter,
    Prose,
}

#[derive(Clone, Debug)]
pub(super) struct PublishDraft {
    pub kind: PublishKind,
    pub account_label: String,
    pub series_title: String,
    pub episode_title: String,
    pub synopsis: String,
    pub language: String,
    pub prose: Option<Vec<u8>>,
    pub prose_name: String,
    pub message: Option<String>,
}

impl Default for PublishDraft {
    fn default() -> Self {
        Self {
            kind: PublishKind::Chapter,
            account_label: String::new(),
            series_title: String::new(),
            episode_title: String::new(),
            synopsis: String::new(),
            language: "en".into(),
            prose: None,
            prose_name: String::new(),
            message: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum BuiltFrom {
    Chapter(u64),
    Prose,
}

impl Studio {
    pub(super) fn publish_ui(&mut self, ctx: &egui::Context) {
        if !self.publish_open || self.story.chapters.is_empty() {
            return;
        }
        self.poll_sign_in();
        self.sync_publish_draft();
        let mut open = true;
        let mut review = false;
        let mut confirm = false;
        let mut upload = false;
        let mut choose_text = false;
        let mut sign_in = false;
        let mut check_status = false;
        let mut confirm_publication = false;
        let mut withdraw_publication = false;
        let mut open_preview: Option<String> = None;
        let mut chosen: Option<scroll_studio::publish_wire::OwnedEpisode> = None;
        egui::Window::new("Publish episode")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_width(460.0);
                egui::ScrollArea::vertical()
                    .id_salt("publish_fields")
                    .max_height(280.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.selectable_value(
                                &mut self.publish_draft.kind,
                                PublishKind::Chapter,
                                "Chapter",
                            );
                            ui.selectable_value(
                                &mut self.publish_draft.kind,
                                PublishKind::Prose,
                                "Prose",
                            );
                        });
                        ui.add_space(6.0);
                        match self.publish_draft.kind {
                            PublishKind::Chapter => {
                                ui.label(
                                    RichText::new(&self.story.chapters[self.chapter].name)
                                        .strong()
                                        .size(16.0),
                                );
                            }
                            PublishKind::Prose => {
                                ui.horizontal(|ui| {
                                    let name = if self.publish_draft.prose_name.is_empty() {
                                        "No text file"
                                    } else {
                                        self.publish_draft.prose_name.as_str()
                                    };
                                    ui.label(name);
                                    if ui.button("Choose text…").clicked() {
                                        choose_text = true;
                                    }
                                });
                                if let Some(bytes) = &self.publish_draft.prose {
                                    if let Ok(text) = std::str::from_utf8(bytes) {
                                        egui::ScrollArea::vertical()
                                            .id_salt("publish_prose")
                                            .max_height(72.0)
                                            .show(ui, |ui| {
                                                ui.label(text);
                                            });
                                    }
                                }
                            }
                        }
                        ui.add_space(8.0);
                        field(ui, "Account", &mut self.publish_draft.account_label);
                        field(ui, "Series", &mut self.publish_draft.series_title);
                        field(ui, "Episode", &mut self.publish_draft.episode_title);
                        ui.label("Synopsis");
                        egui::ScrollArea::vertical()
                            .id_salt("publish_synopsis")
                            .max_height(64.0)
                            .show(ui, |ui| {
                                ui.add(
                                    egui::TextEdit::multiline(&mut self.publish_draft.synopsis)
                                        .desired_rows(3)
                                        .desired_width(f32::INFINITY),
                                );
                            });
                        field(ui, "Language", &mut self.publish_draft.language);
                    });
                if let Some(package) = self.publish.package() {
                    let digest = &package.content_sha256_hex()[..12];
                    ui.label(match package.kind() {
                        EpisodeKind::Comic => format!(
                            "{} · {} · {digest}",
                            package.profile_label().unwrap_or("Comic"),
                            package.format_label().unwrap_or("image")
                        ),
                        EpisodeKind::Prose => format!("Prose · {digest}"),
                    });
                    for member in package.members() {
                        let shape = match (member.width(), member.height()) {
                            (Some(width), Some(height)) => format!("{width}×{height} · "),
                            _ => String::new(),
                        };
                        ui.label(format!(
                            "{} · {}{} bytes",
                            member.label(),
                            shape,
                            member.byte_len()
                        ));
                    }
                    ui.add_space(6.0);
                }
                if let Some(message) = &self.publish_draft.message {
                    ui.label(RichText::new(message).color(Color32::from_rgb(242, 193, 121)));
                    ui.add_space(6.0);
                }
                ui.horizontal(|ui| {
                    if primary(ui, "Review").clicked() {
                        review = true;
                    }
                    if ui
                        .add_enabled(
                            self.publish.package().is_some(),
                            egui::Button::new("Confirm review"),
                        )
                        .clicked()
                    {
                        confirm = true;
                    }
                    let upload_button = ui.add_enabled(
                        self.publish.is_reviewed(),
                        egui::Button::new("Upload draft"),
                    );
                    #[cfg(test)]
                    if self.publish.is_reviewed() {
                        ui.ctx().data_mut(|data| {
                            data.insert_temp(
                                egui::Id::new("publish_upload_bounds"),
                                upload_button.rect,
                            );
                        });
                    }
                    if upload_button.clicked() {
                        upload = true;
                    }
                    if ui.button("Sign in").clicked() {
                        sign_in = true;
                    }
                });
                if let Some(account) = self.publish.account() {
                    ui.label(format!("{} · test-only", account.handle));
                    for episode in &account.episodes {
                        let label = format!("{} · {}", episode.series_title, episode.episode_title);
                        if ui
                            .selectable_label(self.publish.owned() == Some(episode), &label)
                            .clicked()
                        {
                            chosen = Some(episode.clone());
                        }
                    }
                }
                if let Some(ack) = self.publish.acknowledgement() {
                    ui.label(format!("Recorded {} · not current status", ack.revision_id));
                    if let Some(url) = &ack.browser_preview_url {
                        if ui.button("Open preview").clicked() {
                            open_preview = Some(url.clone());
                        }
                    }
                    if ui.button("Check status").clicked() {
                        check_status = true;
                    }
                    if ui.button("Publish").clicked() {
                        confirm_publication = true;
                    }
                    if ui.button("Withdraw").clicked() {
                        withdraw_publication = true;
                    }
                }
            });
        self.publish_open = open;
        if choose_text {
            self.choose_prose_file();
        }
        if review {
            self.review_publish();
        }
        if confirm {
            self.confirm_publish_review();
        }
        if upload {
            self.upload_publish_draft();
        }
        if sign_in {
            self.start_sign_in();
        }
        if let Some(episode) = chosen {
            self.publish.select_owned(episode);
        }
        if let Some(url) = open_preview {
            self.publish_draft.message = Some(match publish_wire::open_system_browser(&url) {
                Ok(()) => "Opened the preview in the system browser.".into(),
                Err(error) => error.to_string(),
            });
        }
        if check_status {
            self.check_publish_status();
        }
        if confirm_publication {
            self.confirm_publication();
        }
        if withdraw_publication {
            self.withdraw_publication();
        }
    }

    fn confirm_publication(&mut self) {
        self.finish_publication(true);
    }

    fn withdraw_publication(&mut self) {
        self.finish_publication(false);
    }

    fn finish_publication(&mut self, confirm: bool) {
        let Some(credential) = self.publish.credential().cloned() else {
            self.publish_draft.message =
                Some("Private prototype service is not connected. Nothing was uploaded.".into());
            return;
        };
        let Some(owned) = self.publish.owned().cloned() else {
            self.publish_draft.message = Some("Choose an owned episode.".into());
            return;
        };
        let Some(ack) = self.publish.acknowledgement().cloned() else {
            self.publish_draft.message = Some("Record a revision before publishing it.".into());
            return;
        };
        self.spawn_service(move || {
            let next = if confirm {
                publish_wire::confirm(Origin::pinned(), &credential, &owned, &ack.revision_id)
            } else {
                let publication_id = ack
                    .publication_id
                    .clone()
                    .ok_or_else(|| "Publish the revision before withdrawing it.".to_string())?;
                publish_wire::withdraw(
                    Origin::pinned(),
                    &credential,
                    &owned,
                    &publication_id,
                    &ack.revision_id,
                )
            }
            .map_err(|error| error.to_string())?;
            let url = next
                .canonical_url
                .clone()
                .ok_or_else(|| "The service did not return a reader URL.".to_string())?;
            let revision = next.revision_id.clone();
            let status = authoritative_status(
                publish_wire::read_publication(Origin::pinned(), &credential, &url),
                &url,
                &revision,
            )?;
            let label = match &status {
                publish_wire::LiveStatus::Read { status, .. } => status.clone(),
                publish_wire::LiveStatus::Discover { status, .. } => status.clone(),
                publish_wire::LiveStatus::NotRead => "unread".into(),
            };
            Ok(ServicePayload::Noted {
                ack: next,
                message: format!("Current status: {label}. The acknowledgement is not the status."),
                status: Some(status),
            })
        });
    }

    fn poll_sign_in(&mut self) {
        let finished = self
            .publish_sign_in
            .as_ref()
            .and_then(|receiver| receiver.try_recv().ok());
        if finished.is_some() {
            self.publish_sign_in = None;
        }
        if let Some(result) = finished {
            match result {
                Ok(ServicePayload::SignedIn(credential, account)) => {
                    let handle = account.handle.clone();
                    self.publish.store_sign_in(credential, account);
                    let where_kept = match self.publish.keeping() {
                        Some(publish_wire::CredentialKeeping::WindowsCredentialManager) => {
                            "Windows Credential Manager"
                        }
                        _ => "memory for this session",
                    };
                    self.publish_draft.message = Some(format!(
                        "Signed in as {handle}. Test-only credential kept in {where_kept}."
                    ));
                }
                Ok(ServicePayload::Noted {
                    ack,
                    message,
                    status,
                }) => {
                    self.publish.note_acknowledgement(ack);
                    if let Some(status) = status {
                        self.publish.note_live(status);
                    }
                    self.publish_draft.message = Some(message);
                }
                Ok(ServicePayload::Status(status, message)) => {
                    self.publish.note_live(status);
                    self.publish_draft.message = Some(message);
                }
                Err(error) => self.publish_draft.message = Some(error),
            }
        }
    }

    fn start_sign_in(&mut self) {
        if self.publish_sign_in.is_some() {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        self.publish_sign_in = Some(rx);
        std::thread::spawn(move || {
            let result = publish_wire::sign_in(Origin::pinned(), publish_wire::open_system_browser)
                .map(|(credential, account)| ServicePayload::SignedIn(credential, account))
                .map_err(|error| error.to_string());
            let _ = tx.send(result);
        });
        self.publish_draft.message = Some("Waiting for the system browser.".into());
    }

    fn check_publish_status(&mut self) {
        let Some(credential) = self.publish.credential().cloned() else {
            self.publish_draft.message =
                Some("Private prototype service is not connected. Nothing was uploaded.".into());
            return;
        };
        let Some(ack) = self.publish.acknowledgement() else {
            self.publish_draft.message =
                Some("There is no reader URL to check. Nothing was uploaded.".into());
            return;
        };
        let Some(url) = ack.canonical_url.clone() else {
            self.publish_draft.message =
                Some("There is no reader URL to check. Nothing was uploaded.".into());
            return;
        };
        let revision = ack.revision_id.clone();
        self.spawn_service(move || {
            let status = authoritative_status(
                publish_wire::read_publication(Origin::pinned(), &credential, &url),
                &url,
                &revision,
            )?;
            let label = match &status {
                publish_wire::LiveStatus::Read { status, .. } => status.clone(),
                publish_wire::LiveStatus::Discover { status, .. } => status.clone(),
                publish_wire::LiveStatus::NotRead => "unread".into(),
            };
            Ok(ServicePayload::Status(
                status,
                format!("Current status: {label}."),
            ))
        });
    }

    fn sync_publish_draft(&mut self) {
        let chapter_id = self.story.chapters[self.chapter].id;
        let stale = match (&self.publish_built, self.publish_draft.kind) {
            (Some(BuiltFrom::Chapter(id)), PublishKind::Chapter) => *id != chapter_id,
            (Some(BuiltFrom::Prose), PublishKind::Prose) => {
                let packaged = self
                    .publish
                    .package()
                    .and_then(|package| package.members().first())
                    .map(|member| member.bytes());
                packaged != self.publish_draft.prose.as_deref()
            }
            (Some(_), _) => true,
            (None, _) => false,
        };
        let metadata_changed = self.publish.package().is_some_and(|package| {
            let metadata = package.metadata();
            metadata.series_title() != self.publish_draft.series_title
                || metadata.episode_title() != self.publish_draft.episode_title
                || metadata.synopsis() != self.publish_draft.synopsis
                || metadata.language() != self.publish_draft.language
        });
        let destination_changed = self.publish.destination().is_some_and(|destination| {
            destination.account_label() != self.publish_draft.account_label
                || destination.series_title() != self.publish_draft.series_title
                || destination.episode_title() != self.publish_draft.episode_title
        });
        if stale || metadata_changed || destination_changed {
            self.publish.invalidate();
            self.publish_built = None;
        }
    }

    pub(super) fn review_publish(&mut self) {
        self.publish.invalidate();
        self.publish_built = None;
        let metadata = match reviewed_metadata(&self.publish_draft) {
            Ok(metadata) => metadata,
            Err(error) => {
                self.publish_draft.message = Some(error.to_string());
                return;
            }
        };
        let destination = match LocalDestination::new(
            self.publish_draft.account_label.clone(),
            self.publish_draft.series_title.clone(),
            self.publish_draft.episode_title.clone(),
        ) {
            Ok(destination) => destination,
            Err(error) => {
                self.publish_draft.message = Some(error.to_string());
                return;
            }
        };
        let built = match self.publish_draft.kind {
            PublishKind::Chapter => {
                let chapter = &self.story.chapters[self.chapter];
                let id = chapter.id;
                match publish::package_chapter(
                    chapter,
                    &self.assets,
                    &self.export_options,
                    metadata,
                ) {
                    Ok(package) => (package, BuiltFrom::Chapter(id)),
                    Err(error) => {
                        self.publish_draft.message = Some(error.to_string());
                        return;
                    }
                }
            }
            PublishKind::Prose => {
                let Some(bytes) = self.publish_draft.prose.clone() else {
                    self.publish_draft.message = Some("Choose a UTF-8 text file.".into());
                    return;
                };
                match publish::package_prose_bytes(&bytes, metadata) {
                    Ok(package) => (package, BuiltFrom::Prose),
                    Err(error) => {
                        self.publish_draft.message = Some(error.to_string());
                        return;
                    }
                }
            }
        };
        self.publish.set_destination(destination);
        self.publish.set_package(built.0);
        self.publish_built = Some(built.1);
        self.publish_draft.message = None;
    }

    fn confirm_publish_review(&mut self) {
        self.sync_publish_draft();
        match self.publish.confirm_review() {
            Ok(()) => self.publish_draft.message = Some("Review confirmed.".into()),
            Err(error) => self.publish_draft.message = Some(error.to_string()),
        }
    }

    fn upload_publish_draft(&mut self) {
        self.sync_publish_draft();
        let ready = self.publish.is_reviewed()
            && self.publish.credential().is_some()
            && self.publish.owned().is_some()
            && self.publish.package().is_some();
        if !ready {
            let Err(error) = self.publish.upload_draft();
            self.publish_draft.message = Some(match error {
                LinkError::ServiceNotConnected => {
                    "Private prototype service is not connected. Nothing was uploaded.".into()
                }
                other => other.to_string(),
            });
            return;
        }
        let credential = self.publish.credential().cloned().unwrap();
        let owned = self.publish.owned().cloned().unwrap();
        let package = self.publish.package().cloned().unwrap();
        let story_ref = match package.chapter_id() {
            Some(id) => format!("chapter-{id}"),
            None => "prose".into(),
        };
        self.spawn_service(move || {
            publish_wire::upload(
                Origin::pinned(),
                &credential,
                &package,
                &owned,
                &story_ref,
                "episode",
            )
            .map(|ack| {
                let revision = ack.revision_id.clone();
                ServicePayload::Noted {
                    ack,
                    message: format!("Recorded {revision}. This is not the current status."),
                    status: None,
                }
            })
            .map_err(|error| error.to_string())
        });
    }

    fn spawn_service(
        &mut self,
        work: impl FnOnce() -> Result<ServicePayload, String> + Send + 'static,
    ) {
        if self.publish_sign_in.is_some() {
            self.publish_draft.message = Some("A publishing request is already running.".into());
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        self.publish_sign_in = Some(rx);
        std::thread::spawn(move || {
            let _ = tx.send(work());
        });
        self.publish_draft.message = Some("Working away from the editor.".into());
    }

    fn choose_prose_file(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Choose prose text")
            .add_filter("Text", &["txt"])
            .pick_file()
        else {
            return;
        };
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "text".into());
        match fs_read_limited(&path, 65_537) {
            Ok(bytes) => {
                self.publish_draft.prose = Some(bytes);
                self.publish_draft.prose_name = name;
                self.publish_draft.message = None;
                self.publish.invalidate();
                self.publish_built = None;
            }
            Err(error) => self.publish_draft.message = Some(error),
        }
    }
}

fn authoritative_status(
    result: Result<publish_wire::LiveStatus, publish_wire::WireError>,
    canonical_url: &str,
    revision_id: &str,
) -> Result<publish_wire::LiveStatus, String> {
    match result {
        Ok(status) => Ok(status),
        Err(publish_wire::WireError::Rejected { code, status: 410 }) if code == "withdrawn" => {
            Ok(publish_wire::LiveStatus::Read {
                status: "withdrawn".into(),
                revision_id: revision_id.to_string(),
                canonical_url: canonical_url.to_string(),
            })
        }
        Err(error) => Err(error.to_string()),
    }
}

fn reviewed_metadata(draft: &PublishDraft) -> Result<PublicMetadata, publish::PackageError> {
    PublicMetadata::new(
        draft.series_title.clone(),
        draft.episode_title.clone(),
        draft.synopsis.clone(),
        draft.language.clone(),
    )
}

fn field(ui: &mut egui::Ui, label: &str, text: &mut String) {
    ui.label(label);
    ui.add(egui::TextEdit::singleline(text).desired_width(f32::INFINITY));
}

fn fs_read_limited(path: &std::path::Path, max_len: u64) -> Result<Vec<u8>, String> {
    let len = std::fs::metadata(path)
        .map_err(|error| format!("Could not read the text file: {error}"))?
        .len();
    if len > max_len {
        return Err(format!("Prose is over the {max_len} byte read limit."));
    }
    std::fs::read(path).map_err(|error| format!("Could not read the text file: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect_text(shape: &egui::epaint::Shape, out: &mut String) {
        match shape {
            egui::epaint::Shape::Text(text) => {
                out.push_str(text.galley.text());
                out.push('\n');
            }
            egui::epaint::Shape::Vec(shapes) => {
                for shape in shapes {
                    collect_text(shape, out);
                }
            }
            _ => {}
        }
    }

    fn fixture() -> (egui::Context, Studio) {
        let ctx = egui::Context::default();
        let mut studio = Studio::from_context(&ctx);
        studio.story = Story::with_canvas("Publish fixture", 64, 64);
        studio.welcome = false;
        studio.preferences.motion = settings::Motion::Off;
        studio.publish_open = true;
        studio.publish_draft.account_label = "Ada".into();
        studio.publish_draft.series_title = "North line".into();
        studio.publish_draft.episode_title = "Platform".into();
        studio.publish_draft.synopsis = "A reviewed public synopsis.".into();
        studio.publish_draft.language = "en".into();
        (ctx, studio)
    }

    fn frame(ctx: &egui::Context, studio: &mut Studio, size: egui::Vec2, events: Vec<egui::Event>) {
        let mut host = eframe::Frame::_new_kittest();
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                events,
                ..Default::default()
            },
            |ctx| eframe::App::update(studio, ctx, &mut host),
        );
    }

    #[test]
    fn prose_review_fits_and_upload_stays_local() {
        for size in [egui::vec2(1100.0, 720.0), egui::vec2(1366.0, 768.0)] {
            let (ctx, mut studio) = fixture();
            let before = studio.story.clone();
            studio.publish_draft.kind = PublishKind::Prose;
            studio.publish_draft.prose = Some(b"A public paragraph.\nSecond line.".to_vec());
            studio.publish_draft.prose_name = "letter.txt".into();
            studio.review_publish();
            assert!(studio.publish.package().is_some());
            assert_eq!(studio.story, before);
            frame(&ctx, &mut studio, size, Vec::new());
            frame(&ctx, &mut studio, size, Vec::new());
            let mut host = eframe::Frame::_new_kittest();
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                },
                |ctx| eframe::App::update(&mut studio, ctx, &mut host),
            );
            let mut text = String::new();
            for clipped in &output.shapes {
                collect_text(&clipped.shape, &mut text);
            }
            assert!(
                text.contains("episode.txt"),
                "member list missing from {text}"
            );
            assert!(text.contains("letter.txt"), "chosen file name missing");
            assert!(text.contains("Confirm review"));
            let upload = ctx
                .data(|data| data.get_temp::<egui::Rect>(egui::Id::new("publish_upload_bounds")));
            assert!(
                upload.is_none(),
                "upload is disabled until review is confirmed"
            );
            studio.confirm_publish_review();
            assert!(studio.publish.is_reviewed());
            frame(&ctx, &mut studio, size, Vec::new());
            let upload = ctx
                .data(|data| data.get_temp::<egui::Rect>(egui::Id::new("publish_upload_bounds")))
                .expect("upload bounds");
            assert!(
                upload.width() >= 24.0 && upload.height() >= 18.0,
                "upload rect {upload:?} outside a usable target"
            );
            assert!(
                egui::Rect::from_min_size(egui::Pos2::ZERO, size).contains_rect(upload),
                "upload rect {upload:?} does not fit in {size:?}"
            );
            let before_upload = studio.story.clone();
            let revision = studio.revision;
            let dirty = studio.dirty;
            let click = upload.center();
            let press = |pressed| {
                vec![
                    egui::Event::PointerMoved(click),
                    egui::Event::PointerButton {
                        pos: click,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]
            };
            frame(&ctx, &mut studio, size, press(true));
            frame(&ctx, &mut studio, size, press(false));
            let message = studio.publish_draft.message.clone().unwrap_or_default();
            assert!(
                message.contains("not connected"),
                "upload message was {message}; reviewed {} package {}",
                studio.publish.is_reviewed(),
                studio.publish.package().is_some()
            );
            assert!(studio.publish.is_reviewed());
            assert_eq!(
                studio.publish.publication_status(),
                scroll_studio::publish::PublicationStatus::NotRead
            );
            assert_eq!(studio.story, before_upload);
            assert_eq!(studio.revision, revision);
            assert_eq!(studio.dirty, dirty);
            studio.publish_draft.synopsis = "Changed after review.".into();
            frame(&ctx, &mut studio, size, Vec::new());
            assert!(!studio.publish.is_reviewed());
            assert!(studio.publish.package().is_none());
        }
    }

    #[test]
    fn withdrawn_read_is_authoritative_status() {
        let status = authoritative_status(
            Err(publish_wire::WireError::Rejected {
                code: "withdrawn".into(),
                status: 410,
            }),
            "http://127.0.0.1:19427/ss-publish/browser/reader/pub_test_0001",
            "rev_test_0001",
        )
        .unwrap();
        match status {
            publish_wire::LiveStatus::Read {
                status,
                revision_id,
                canonical_url,
            } => {
                assert_eq!(status, "withdrawn");
                assert_eq!(revision_id, "rev_test_0001");
                assert!(canonical_url.ends_with("pub_test_0001"));
            }
            other => panic!("expected a read, got {other:?}"),
        }
        let refused = authoritative_status(
            Err(publish_wire::WireError::Rejected {
                code: "not_owner".into(),
                status: 403,
            }),
            "http://127.0.0.1:19427/ss-publish/browser/reader/pub_test_0001",
            "rev_test_0001",
        );
        assert!(refused.is_err());
    }

    #[test]
    fn publishing_request_leaves_the_editor_responsive() {
        let (ctx, mut studio) = fixture();
        let (sender, receiver) = std::sync::mpsc::channel();
        studio.publish_sign_in = Some(receiver);
        studio.publish_draft.message = Some("Working away from the editor.".into());
        let started = std::time::Instant::now();
        frame(&ctx, &mut studio, egui::vec2(1100.0, 720.0), Vec::new());
        assert!(
            started.elapsed() < std::time::Duration::from_millis(500),
            "a frame waited on the publishing thread"
        );
        let mut text = String::new();
        let mut host = eframe::Frame::_new_kittest();
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1100.0, 720.0),
                )),
                ..Default::default()
            },
            |ctx| eframe::App::update(&mut studio, ctx, &mut host),
        );
        for clipped in &output.shapes {
            collect_text(&clipped.shape, &mut text);
        }
        assert!(
            text.contains("Working away from the editor."),
            "publish window did not show the in-flight message: {text}"
        );
        studio.publish_draft.synopsis = "Edited while the service works.".into();
        frame(&ctx, &mut studio, egui::vec2(1100.0, 720.0), Vec::new());
        assert_eq!(
            studio.publish_draft.synopsis,
            "Edited while the service works."
        );
        assert!(studio.publish_sign_in.is_some());
        sender
            .send(Err("The service finished after the editor moved on.".into()))
            .unwrap();
        frame(&ctx, &mut studio, egui::vec2(1100.0, 720.0), Vec::new());
        let message = studio.publish_draft.message.clone().unwrap_or_default();
        assert!(message.contains("finished after the editor moved on"));
        assert!(studio.publish_sign_in.is_none());
    }

    #[test]
    fn switching_chapter_drops_the_comic_review() {
        let (_ctx, mut studio) = fixture();
        studio.export_options.profile = scroll_studio::export::ExportProfile::Original;
        studio.review_publish();
        assert_eq!(
            studio.publish.package().unwrap().chapter_id(),
            Some(studio.story.chapters[0].id)
        );
        studio.confirm_publish_review();
        assert!(studio.publish.is_reviewed());
        let id = studio.story.alloc_id();
        studio
            .story
            .chapters
            .push(Chapter::new(id, "SECRET_OTHER_CHAPTER", 64, 64));
        studio.chapter = 1;
        studio.sync_publish_draft();
        assert!(studio.publish.package().is_none());
        assert!(!studio.publish.is_reviewed());
    }
}
