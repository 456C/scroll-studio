//! Running an addon program: start it, exchange lines without ever blocking the UI, stop it.
//!
//! Everything that touches the addon's pipes runs on its own thread with bounded queues:
//! a writer that the UI hands lines to, a reader for messages, and a reader for error
//! output. An addon that stops reading, floods output or sends endless lines cannot freeze
//! the app or grow its memory without limit; it is stopped instead.
//!
//! Addons must exit when their input closes: that is how the app stops them, along with
//! killing the program it started.

use anyhow::{bail, Context, Result};
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// The longest message line an addon may send.
pub const MAX_LINE_BYTES: usize = 16 * 1024 * 1024;
/// Messages received but not yet handled; the addon waits when this is full.
const INCOMING_QUEUE: usize = 256;
/// Messages waiting to be written to the addon; an addon this far behind is stopped.
const OUTGOING_QUEUE: usize = 256;
const ERROR_LINES: usize = 20;
const ERROR_LINE_CHARS: usize = 500;
/// How long the UI waits for an addon to exit, or for its last error lines, before moving on.
const GRACE: Duration = Duration::from_millis(200);

/// Called from a reader thread when something arrives, so the UI can repaint.
pub type Wake = Arc<dyn Fn() + Send + Sync>;

#[derive(Debug, PartialEq)]
pub enum Event {
    Line(String),
    /// A line longer than [`MAX_LINE_BYTES`] was discarded.
    TooLong,
    /// The addon closed its output (it stopped or crashed).
    Closed,
}

/// A started addon program. Dropping it stops the program.
pub struct AddonProcess {
    child: Child,
    outgoing: Option<SyncSender<String>>,
    /// `None` once stopped, which also ends the reader thread and frees its queue.
    events: Option<Receiver<Event>>,
    errors: Arc<Mutex<VecDeque<String>>>,
    error_reader: Option<JoinHandle<()>>,
}

impl AddonProcess {
    /// Start an addon's program from its folder.
    pub fn start(program: &Path, folder: &Path, wake: Wake) -> Result<Self> {
        let mut command = Command::new(program);
        command.current_dir(folder);
        Self::spawn(command, wake)
    }

    /// Start any command as an addon (also used by tests).
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
        let mut stdin = child.stdin.take().context("The addon takes no input")?;
        let stdout = child.stdout.take().context("The addon has no output")?;
        let stderr = child
            .stderr
            .take()
            .context("The addon has no error output")?;

        let (outgoing, to_write) = mpsc::sync_channel::<String>(OUTGOING_QUEUE);
        std::thread::Builder::new()
            .name("addon-input".into())
            .spawn(move || {
                for line in to_write {
                    if stdin
                        .write_all(line.as_bytes())
                        .and_then(|_| stdin.write_all(b"\n"))
                        .and_then(|_| stdin.flush())
                        .is_err()
                    {
                        return;
                    }
                }
                // Dropping stdin here tells the addon to exit.
            })?;

        let (sender, events) = mpsc::sync_channel(INCOMING_QUEUE);
        std::thread::Builder::new()
            .name("addon-output".into())
            .spawn(move || {
                let mut reader = BufReader::new(stdout);
                loop {
                    let event = match read_line(&mut reader, MAX_LINE_BYTES) {
                        Ok(Line::Text(line)) => Event::Line(line),
                        Ok(Line::TooLong) => Event::TooLong,
                        Ok(Line::End) | Err(_) => Event::Closed,
                    };
                    let closed = event == Event::Closed;
                    // Blocks while the app catches up, which also slows a flooding addon.
                    if sender.send(event).is_err() {
                        return;
                    }
                    wake();
                    if closed {
                        return;
                    }
                }
            })?;

        let errors = Arc::new(Mutex::new(VecDeque::new()));
        let recent = errors.clone();
        let error_reader = std::thread::Builder::new()
            .name("addon-errors".into())
            .spawn(move || {
                let mut reader = BufReader::new(stderr);
                while let Ok(line) = read_line(&mut reader, ERROR_LINE_CHARS * 4) {
                    let text = match line {
                        Line::Text(text) => text,
                        Line::TooLong => "(a very long line)".into(),
                        Line::End => return,
                    };
                    let mut recent = recent.lock().unwrap_or_else(|e| e.into_inner());
                    if recent.len() == ERROR_LINES {
                        recent.pop_front();
                    }
                    recent.push_back(text.chars().take(ERROR_LINE_CHARS).collect());
                }
            })?;
        Ok(Self {
            child,
            outgoing: Some(outgoing),
            events: Some(events),
            errors,
            error_reader: Some(error_reader),
        })
    }

    /// Queue one message line. Never blocks: an addon that has stopped reading fails here.
    pub fn send(&mut self, line: String) -> Result<()> {
        let outgoing = self
            .outgoing
            .as_ref()
            .context("The addon is not accepting messages")?;
        match outgoing.try_send(line) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => bail!("The addon stopped reading its messages"),
            Err(TrySendError::Disconnected(_)) => bail!("The addon closed its input"),
        }
    }

    pub fn try_recv(&self) -> Option<Event> {
        self.events.as_ref()?.try_recv().ok()
    }

    /// The addon's last error output, newest last, on one line. Waits briefly for the
    /// program to exit and its last lines to arrive, never longer than [`GRACE`]: a program
    /// it started may keep the error output open.
    pub fn recent_errors(&mut self) -> String {
        let deadline = Instant::now() + GRACE;
        while Instant::now() < deadline {
            let exited = matches!(self.child.try_wait(), Ok(Some(_)));
            let read = self.error_reader.as_ref().is_none_or(|r| r.is_finished());
            if exited && read {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let recent = self.errors.lock().unwrap_or_else(|e| e.into_inner());
        recent.iter().cloned().collect::<Vec<_>>().join(" / ")
    }

    /// Close the addon's input so it can finish its last reply and exit, then end it.
    pub fn stop(&mut self) {
        self.outgoing = None;
        self.events = None;
        let deadline = Instant::now() + GRACE;
        while Instant::now() < deadline {
            if matches!(self.child.try_wait(), Ok(Some(_)) | Err(_)) {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for AddonProcess {
    fn drop(&mut self) {
        self.stop();
    }
}

enum Line {
    Text(String),
    TooLong,
    End,
}

/// Read one line of at most `limit` bytes. Longer lines are discarded in chunks without
/// being stored; text that is not valid UTF-8 is kept with replacement characters.
fn read_line(reader: &mut impl BufRead, limit: usize) -> std::io::Result<Line> {
    let mut line = Vec::new();
    let read = reader.take(limit as u64 + 1).read_until(b'\n', &mut line)?;
    if read == 0 {
        return Ok(Line::End);
    }
    if line.len() > limit && line.last() != Some(&b'\n') {
        loop {
            let buffer = reader.fill_buf()?;
            if buffer.is_empty() {
                break;
            }
            if let Some(end) = buffer.iter().position(|b| *b == b'\n') {
                reader.consume(end + 1);
                break;
            }
            let len = buffer.len();
            reader.consume(len);
        }
        return Ok(Line::TooLong);
    }
    Ok(Line::Text(
        String::from_utf8_lossy(&line)
            .trim_end_matches(['\r', '\n'])
            .to_owned(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_lines_are_discarded_without_being_stored_and_text_is_lossy() {
        let mut input: Vec<u8> = vec![b'x'; 100];
        input.extend_from_slice(b"\nok\n\xff\xfe bad\nend");
        let mut reader = BufReader::with_capacity(8, input.as_slice());
        assert!(matches!(read_line(&mut reader, 10).unwrap(), Line::TooLong));
        assert!(matches!(read_line(&mut reader, 10).unwrap(), Line::Text(t) if t == "ok"));
        assert!(
            matches!(read_line(&mut reader, 10).unwrap(), Line::Text(t) if t.ends_with(" bad"))
        );
        assert!(matches!(read_line(&mut reader, 10).unwrap(), Line::Text(t) if t == "end"));
        assert!(matches!(read_line(&mut reader, 10).unwrap(), Line::End));
    }
}
