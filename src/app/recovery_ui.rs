use super::*;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const RECOVERY_INTERVAL: Duration = Duration::from_secs(60);

pub(super) struct RecoveryState {
    session: Option<recovery::RecoverySession>,
    worker: Option<mpsc::Receiver<anyhow::Result<recovery::RecoveryEntry>>>,
    scan: Option<mpsc::Receiver<anyhow::Result<Vec<recovery::RecoveryEntry>>>>,
    entries: Vec<recovery::RecoveryEntry>,
    source: Option<recovery::RecoveryEntry>,
    last_revision: Option<u64>,
    attempted_at: Instant,
    pub browser_open: bool,
    pub notice: String,
}

impl RecoveryState {
    pub fn new() -> Self {
        Self {
            session: None,
            worker: None,
            scan: None,
            entries: Vec::new(),
            source: None,
            last_revision: None,
            attempted_at: Instant::now(),
            browser_open: false,
            notice: "Recovery copies every minute while editing".into(),
        }
    }
}

impl Studio {
    pub(super) fn scan_recovery(&mut self, ctx: &egui::Context, show: bool) {
        if self.recovery.scan.is_some() {
            return;
        }
        self.recovery.browser_open |= show;
        let root = recovery::default_directory();
        let ctx = ctx.clone();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(recovery::list(&root));
            ctx.request_repaint();
        });
        self.recovery.scan = Some(rx);
    }

    pub(super) fn poll_recovery(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.recovery.scan {
            match rx.try_recv() {
                Ok(result) => {
                    self.recovery.scan = None;
                    match result {
                        Ok(entries) => {
                            self.recovery.browser_open |= !entries.is_empty() && self.welcome;
                            self.recovery.entries = entries;
                        }
                        Err(e) => {
                            self.recovery.notice = format!("Recovery discovery unavailable: {e}")
                        }
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.recovery.scan = None;
                    self.recovery.notice =
                        "Recovery discovery stopped. Try opening recovery again.".into();
                }
                Err(mpsc::TryRecvError::Empty) => (),
            }
        }
        if let Some(rx) = &self.recovery.worker {
            match rx.try_recv() {
                Ok(result) => {
                    self.recovery.worker = None;
                    match result {
                        Ok(entry) => {
                            self.recovery.last_revision = Some(entry.revision);
                            self.recovery.notice = if entry.revision == self.revision {
                                "Recovery copy is up to date"
                            } else {
                                "Recovery copy saved; recent edits pending"
                            }
                            .into();
                        }
                        Err(e) => self.recovery.notice = format!("Recovery copy failed: {e}"),
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.recovery.worker = None;
                    self.recovery.notice =
                        "Recovery worker stopped. Save your story manually.".into();
                }
                Err(mpsc::TryRecvError::Empty) => (),
            }
        }
        if self.welcome || !self.dirty || self.recovery.last_revision == Some(self.revision) {
            return;
        }
        if self.recovery.notice == "Recovery copy is up to date" {
            self.recovery.notice = "Recent edits will be included in the next recovery copy".into();
        }
        let elapsed = self.recovery.attempted_at.elapsed();
        if elapsed < RECOVERY_INTERVAL {
            ctx.request_repaint_after(RECOVERY_INTERVAL - elapsed);
            return;
        }
        if self.recovery.worker.is_some()
            || self.job.is_some()
            || ctx.input(|i| i.pointer.any_down())
        {
            ctx.request_repaint_after(Duration::from_secs(1));
            return;
        }
        self.recovery.attempted_at = Instant::now();
        if self.recovery.session.is_none() {
            match recovery::RecoverySession::new(&recovery::default_directory()) {
                Ok(session) => self.recovery.session = Some(session),
                Err(e) => {
                    self.recovery.notice = format!("Recovery unavailable: {e}");
                    ctx.request_repaint_after(RECOVERY_INTERVAL);
                    return;
                }
            }
        }
        let session = self.recovery.session.as_ref().unwrap().clone();
        let story = self.story.clone();
        let assets = assets_for_story(&story, &self.assets);
        let path = self.path.clone();
        let revision = self.revision;
        let ctx = ctx.clone();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(session.save(&story, &assets, path.as_deref(), revision));
            ctx.request_repaint();
        });
        self.recovery.worker = Some(rx);
        self.recovery.notice = "Writing a recovery copy…".into();
    }

    /// Retire a document's session separately so an old cleanup cannot erase a
    /// later document's recovery data, even if its save worker is still running.
    pub(super) fn retire_recovery(&mut self) {
        let worker = self.recovery.worker.take();
        if let Some(session) = self.recovery.session.take() {
            std::thread::spawn(move || {
                if let Some(rx) = worker {
                    let _ = rx.recv();
                }
                let _ = session.clear();
            });
        }
        self.recovery.source = None;
        self.recovery.last_revision = None;
        self.recovery.attempted_at = Instant::now();
        self.recovery.notice = "Recovery copies every minute while editing".into();
    }

    pub(super) fn clear_saved_recovery(&mut self) {
        let source = self.recovery.source.take();
        self.retire_recovery();
        if let Some(source) = source {
            std::thread::spawn(move || {
                let _ = source.discard();
            });
        }
        self.recovery.notice = "Story saved; recovery copy cleared".into();
    }

    pub(super) fn restore_recovery(&mut self, entry: recovery::RecoveryEntry) {
        match entry.load() {
            Ok((story, assets)) => {
                self.story = story;
                self.assets = assets;
                // Save As is intentional: recovery never silently overwrites an original.
                self.path = None;
                self.reset_view();
                self.welcome = false;
                self.changed();
                self.recovery.source = Some(entry);
                self.recovery.browser_open = false;
                self.status = "Recovered · unsaved copy".into();
            }
            Err(e) => self.error = Some(format!("Could not restore this recovery copy: {e:#}")),
        }
    }

    pub(super) fn recovery_dialog(&mut self, ctx: &egui::Context) {
        if !self.recovery.browser_open || self.dialog.is_some() {
            return;
        }
        let mut open = true;
        let entries = self.recovery.entries.clone();
        let mut restore = None;
        egui::Window::new("Recovery")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_width(490.0);
                if self.recovery.scan.is_some() {
                    ui.spinner();
                } else if entries.is_empty() {
                    ui.label("No recovery copies");
                }
                egui::ScrollArea::vertical()
                    .max_height(360.0)
                    .show(ui, |ui| {
                        for entry in &entries {
                            egui::Frame::new()
                                .fill(CARD)
                                .corner_radius(8.0)
                                .inner_margin(12.0)
                                .show(ui, |ui| {
                                    ui.set_width(460.0);
                                    ui.label(RichText::new(&entry.title).size(18.0).strong());
                                    let now = SystemTime::now()
                                        .duration_since(UNIX_EPOCH)
                                        .unwrap_or_default()
                                        .as_secs();
                                    ui.label(
                                        RichText::new(format!(
                                            "{} min ago",
                                            now.saturating_sub(entry.saved_unix_seconds) / 60
                                        ))
                                        .size(12.0)
                                        .color(MUTED),
                                    );
                                    if let Some(path) = &entry.original_path {
                                        ui.add(
                                            egui::Label::new(path.display().to_string()).truncate(),
                                        )
                                        .on_hover_text(path.display().to_string());
                                    }
                                    if primary(ui, "Restore copy")
                                        .on_hover_text(
                                            "Opens an unsaved copy; keeps the original file.",
                                        )
                                        .clicked()
                                    {
                                        restore = Some(entry.clone());
                                    }
                                });
                        }
                    });
                ui.add_space(8.0);
                if motion::button(ui, "Close").clicked() {
                    self.recovery.browser_open = false;
                }
            });
        self.recovery.browser_open &= open;
        if let Some(entry) = restore {
            self.recovery.browser_open = false;
            self.request(Pending::Recover(entry));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestRoot(PathBuf);
    impl TestRoot {
        fn new() -> Self {
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "scroll-recovery-ui-test-{}-{stamp}",
                std::process::id()
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path.canonicalize().unwrap())
        }
    }
    impl Drop for TestRoot {
        fn drop(&mut self) {
            let temp = std::env::temp_dir().canonicalize().unwrap();
            if self.0.parent() == Some(temp.as_path())
                && self
                    .0
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("scroll-recovery-ui-test-")
            {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }

    fn studio() -> (egui::Context, Studio) {
        let ctx = egui::Context::default();
        let mut studio = Studio::from_context(&ctx);
        studio.welcome = false;
        studio.dirty = true;
        studio.revision = 10;
        (ctx, studio)
    }

    #[test]
    fn unchanged_revision_and_interval_gate_avoid_redundant_recovery_work() {
        let (ctx, mut studio) = studio();
        studio.recovery.last_revision = Some(studio.revision);
        studio.recovery.attempted_at = Instant::now() - Duration::from_secs(120);
        studio.poll_recovery(&ctx);
        assert!(studio.recovery.worker.is_none());
        assert!(studio.recovery.session.is_none());
        studio.revision += 1;
        studio.recovery.attempted_at = Instant::now();
        studio.poll_recovery(&ctx);
        assert!(studio.recovery.worker.is_none());
        assert!(studio.recovery.session.is_none());
    }

    #[test]
    fn edits_during_snapshot_do_not_report_a_current_recovery() {
        let root = TestRoot::new();
        let (ctx, mut studio) = studio();
        let session = recovery::RecoverySession::new(&root.0).unwrap();
        let entry = session
            .save(&studio.story, &AssetBytes::new(), None, 9)
            .unwrap();
        let (tx, rx) = mpsc::channel();
        tx.send(Ok(entry)).unwrap();
        studio.recovery.worker = Some(rx);
        studio.recovery.session = Some(session);
        studio.poll_recovery(&ctx);
        assert_eq!(studio.recovery.last_revision, Some(9));
        assert!(studio.recovery.notice.contains("recent edits pending"));
    }

    #[test]
    fn disconnected_scan_can_be_retried() {
        let (ctx, mut studio) = studio();
        let (tx, rx) = mpsc::channel();
        drop(tx);
        studio.recovery.scan = Some(rx);
        studio.poll_recovery(&ctx);
        assert!(studio.recovery.scan.is_none());
        assert!(studio.recovery.notice.contains("discovery stopped"));
    }

    #[test]
    fn retirement_waits_for_queued_old_worker_without_clearing_new_session() {
        let root = TestRoot::new();
        let (_, mut studio) = studio();
        let session = recovery::RecoverySession::new(&root.0).unwrap();
        let first = session
            .save(&studio.story, &AssetBytes::new(), None, 1)
            .unwrap();
        let old_metadata = first.snapshot_path().parent().unwrap().join("current.json");
        let worker_session = session.clone();
        let story = studio.story.clone();
        let (gate_tx, gate_rx) = mpsc::channel();
        let (tx, rx) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            gate_rx.recv().unwrap();
            tx.send(worker_session.save(&story, &AssetBytes::new(), None, 2))
                .unwrap();
        });
        studio.recovery.session = Some(session);
        studio.recovery.worker = Some(rx);
        studio.retire_recovery();
        let next_session = recovery::RecoverySession::new(&root.0).unwrap();
        let next = next_session
            .save(&Story::new("New document"), &AssetBytes::new(), None, 3)
            .unwrap();
        assert!(old_metadata.exists());
        gate_tx.send(()).unwrap();
        worker.join().unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while old_metadata.exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(!old_metadata.exists());
        assert!(next.snapshot_path().exists());
        drop(next_session);
        assert_eq!(recovery::list(&root.0).unwrap()[0].title, "New document");
    }
}
