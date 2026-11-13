//! Running an addon program: start it, read its lines without blocking the UI, stop it.

use anyhow::{Context, Result};
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};

/// The longest message line an addon may send.
pub const MAX_LINE_BYTES: u64 = 16 * 1024 * 1024;
const ERROR_LINES: usize = 20;

/// Called from a reader thread when something arrives, so the UI can repaint.
pub type Wake = Arc<dyn Fn() + Send + Sync>;

#[derive(Debug, PartialEq)]
pub enum Event {
    Line(String),
    /// A line longer than [`MAX_LINE_BYTES`] was dropped.
    TooLong,
    /// The addon closed its output (it stopped or crashed).
    Closed,
}

/// A started addon program. Dropping it stops the program.
pub struct AddonProcess {
    child: Child,
    stdin: Option<ChildStdin>,
    events: Receiver<Event>,
    errors: Arc<Mutex<VecDeque<String>>>,
}

impl AddonProcess {
    /// Start an addon's program from its folder.
    pub fn start(program: &Path, folder: &Path, wake: Wake) -> Result<Self> {
        let mut command = Command::new(program);
        command.current_dir(folder);
        Self::spawn(command, wake)
    }

    /// Start any command as an addon (used by tests and the stage 8 addon).
    pub fn spawn(mut command: Command, wake: Wake) -> Result<Self> {
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env(
                "SCROLL_STUDIO_ADDON_PROTOCOL",
                super::host::PROTOCOL_VERSION.to_string(),
            );
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // CREATE_NO_WINDOW: an addon never flashes a console window.
            command.creation_flags(0x0800_0000);
        }
        let mut child = command.spawn().context("Could not start the addon")?;
        let stdout = child.stdout.take().context("The addon has no output")?;
        let stderr = child
            .stderr
            .take()
            .context("The addon has no error output")?;
        let (sender, events) = mpsc::channel();
        let reader_wake = wake.clone();
        std::thread::Builder::new()
            .name("addon-output".into())
            .spawn(move || {
                let mut reader = BufReader::new(stdout);
                loop {
                    let mut line = Vec::new();
                    let read = (&mut reader)
                        .take(MAX_LINE_BYTES + 1)
                        .read_until(b'\n', &mut line);
                    let event = match read {
                        Ok(0) | Err(_) => Event::Closed,
                        Ok(_) if line.len() as u64 > MAX_LINE_BYTES => {
                            // Skip the rest of the oversized line.
                            let mut rest = Vec::new();
                            let _ = reader.read_until(b'\n', &mut rest);
                            Event::TooLong
                        }
                        Ok(_) => Event::Line(
                            String::from_utf8_lossy(&line)
                                .trim_end_matches(['\r', '\n'])
                                .to_owned(),
                        ),
                    };
                    let closed = event == Event::Closed;
                    if sender.send(event).is_err() {
                        return;
                    }
                    reader_wake();
                    if closed {
                        return;
                    }
                }
            })?;
        let errors = Arc::new(Mutex::new(VecDeque::new()));
        let recent = errors.clone();
        std::thread::Builder::new()
            .name("addon-errors".into())
            .spawn(move || {
                for line in BufReader::new(stderr).lines() {
                    let Ok(line) = line else { return };
                    let mut recent = recent.lock().unwrap_or_else(|e| e.into_inner());
                    if recent.len() == ERROR_LINES {
                        recent.pop_front();
                    }
                    recent.push_back(line.chars().take(500).collect());
                }
            })?;
        Ok(Self {
            stdin: child.stdin.take(),
            child,
            events,
            errors,
        })
    }

    /// Send one message line.
    pub fn send(&mut self, line: &str) -> Result<()> {
        let stdin = self
            .stdin
            .as_mut()
            .context("The addon is not accepting messages")?;
        stdin.write_all(line.as_bytes())?;
        stdin.write_all(b"\n")?;
        stdin.flush()?;
        Ok(())
    }

    pub fn try_recv(&self) -> Option<Event> {
        self.events.try_recv().ok()
    }

    /// The addon's last error output, newest last, on one line.
    pub fn recent_errors(&self) -> String {
        let recent = self.errors.lock().unwrap_or_else(|e| e.into_inner());
        recent.iter().cloned().collect::<Vec<_>>().join(" / ")
    }

    pub fn stop(&mut self) {
        self.stdin = None;
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for AddonProcess {
    fn drop(&mut self) {
        self.stop();
    }
}
