//! The addon protocol: JSON-RPC 2.0, one message per line, over the addon's stdin/stdout.
//!
//! - **Addon → app requests** use [`crate::wire`] methods: read methods need
//!   `story.read`; `edit.*` methods need `story.propose` and become a proposal that the
//!   user applies or cancels. The reply arrives when they decide: `{"applied": true,
//!   "created": [...]}`, or error [`CANCELLED`] (the user said no) or [`REFUSED`] (it could
//!   not be applied). Each addon has at most one proposal open; another gets [`BUSY`].
//!   `host.methods` lists what this addon may call.
//! - **App → addon notifications:** `host.ready {protocol, app_version, permissions,
//!   revision}` once at start, then `story.changed {revision, scope}` for addons that may
//!   read.
//!
//! Edits that would change a locked layer are refused before the user sees them, and
//! checked again when the user applies. A line the host cannot use is answered with a
//! JSON-RPC error whose `id` is `null`; an addon's own response lines are ignored. An
//! addon whose suggestions are cancelled [`MAX_CANCELLED`] times in a row is stopped.
//!
//! Permissions limit what an addon may ask the app for. They are not a sandbox: an addon
//! is a program on the user's computer, running with the user's rights.

use super::manifest::{Manifest, Permission};
use super::process::{AddonProcess, Event};
use crate::commands::{Applied, ChangeScope, Command, StoryChanged};
use crate::model::Story;
use crate::wire::{self, Policy, Request};
use serde_json::{json, Value};

pub const PROTOCOL_VERSION: u32 = 1;

// Error codes: the JSON-RPC standard ones, then ours.
pub const PARSE_ERROR: i64 = -32700;
pub const INVALID_REQUEST: i64 = -32600;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;
pub const PERMISSION_DENIED: i64 = -32001;
pub const REFUSED: i64 = -32002;
pub const CANCELLED: i64 = -32003;
pub const BUSY: i64 = -32004;

/// Cancelled suggestions in a row before the addon is stopped.
pub const MAX_CANCELLED: u32 = 3;
/// Messages handled per frame; the rest wait for the next frame.
pub const MAX_EVENTS_PER_POLL: usize = 64;

/// A message from an addon.
#[derive(Clone, Debug, PartialEq)]
pub enum FromAddon {
    Request {
        id: Value,
        method: String,
        params: Value,
    },
    /// Notifications from addons are accepted and ignored in protocol 1.
    Notification { method: String },
    /// A response line (the host sends no requests in protocol 1): ignored.
    Response,
    /// A line the host cannot use, answered with an error.
    Invalid {
        id: Option<Value>,
        code: i64,
        reason: String,
    },
}

pub fn parse_message(line: &str) -> FromAddon {
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        return FromAddon::Invalid {
            id: None,
            code: PARSE_ERROR,
            reason: "Not JSON".into(),
        };
    };
    if value.get("method").is_none()
        && (value.get("result").is_some() || value.get("error").is_some())
    {
        return FromAddon::Response;
    }
    let id = value
        .get("id")
        .cloned()
        .filter(|id| id.is_string() || id.is_number());
    let invalid = |reason: &str| FromAddon::Invalid {
        id: id.clone(),
        code: INVALID_REQUEST,
        reason: reason.into(),
    };
    if value.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return invalid("Expected \"jsonrpc\": \"2.0\"");
    }
    let Some(method) = value.get("method").and_then(Value::as_str) else {
        return invalid("Expected a method");
    };
    if value.get("id").is_some_and(|raw| !raw.is_null()) && id.is_none() {
        return invalid("An id must be a string or a number");
    }
    let params = value.get("params").cloned().unwrap_or(Value::Null);
    match id {
        Some(id) => FromAddon::Request {
            id,
            method: method.to_owned(),
            params,
        },
        None => FromAddon::Notification {
            method: method.to_owned(),
        },
    }
}

pub fn result_line(id: &Value, result: Value) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "result": result }).to_string()
}

pub fn error_line(id: &Value, code: i64, message: &str) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }).to_string()
}

pub fn notification_line(method: &str, params: Value) -> String {
    json!({ "jsonrpc": "2.0", "method": method, "params": params }).to_string()
}

pub fn story_changed_params(change: &StoryChanged) -> Value {
    match &change.scope {
        ChangeScope::Everything => json!({ "revision": change.revision, "scope": "everything" }),
        ChangeScope::Objects(ids) => {
            json!({ "revision": change.revision, "scope": { "ids": ids } })
        }
    }
}

/// What the app should do with a request.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Reply(Value),
    Fail {
        code: i64,
        message: String,
    },
    /// Show the edit to the user; answer once they decide.
    Propose(Box<Command>),
}

fn fail(code: i64, message: impl Into<String>) -> Action {
    Action::Fail {
        code,
        message: message.into(),
    }
}

/// Decide a request against the current story. Never changes the story.
pub fn handle(story: &Story, manifest: &Manifest, method: &str, params: &Value) -> Action {
    if method == "host.methods" {
        let mut methods = Vec::new();
        if manifest.has(Permission::StoryRead) {
            methods.extend(wire::READ_METHODS);
        }
        if manifest.has(Permission::StoryPropose) {
            methods.extend(wire::EDIT_METHODS);
        }
        return Action::Reply(json!({ "methods": methods }));
    }
    let is_edit = method.starts_with(wire::EDIT_PREFIX);
    let known = if is_edit {
        wire::EDIT_METHODS.contains(&method)
    } else {
        wire::READ_METHODS.contains(&method)
    };
    if !known {
        return fail(METHOD_NOT_FOUND, format!("Unknown method `{method}`"));
    }
    let needed = if is_edit {
        Permission::StoryPropose
    } else {
        Permission::StoryRead
    };
    if !manifest.has(needed) {
        return fail(
            PERMISSION_DENIED,
            format!(
                "This addon was not given permission to {}",
                needed.label().to_lowercase()
            ),
        );
    }
    // Image bytes cannot travel over this connection in protocol 1.
    let request = match wire::parse_request(method, params, Policy::default()) {
        Ok(request) => request,
        Err(error) => return fail(INVALID_PARAMS, format!("{error:#}")),
    };
    match request {
        Request::Read(query) => match story.query(&query) {
            Ok(answer) => Action::Reply(answer),
            Err(error) => fail(INVALID_PARAMS, format!("{error:#}")),
        },
        Request::Edit(command) => match check_edit(story, &command) {
            Ok(()) => Action::Propose(command),
            Err(message) => fail(REFUSED, message),
        },
    }
}

/// Rehearse an addon's edit on a copy: it must apply, change something, and leave every
/// locked layer as it is. Run when the edit arrives and again when the user applies it.
pub fn check_edit(story: &Story, command: &Command) -> Result<(), String> {
    if matches!(command, Command::Batch(parts) if parts.is_empty()) {
        return Err("There is nothing to change".into());
    }
    let mut rehearsal = story.clone();
    rehearsal
        .apply(command.clone())
        .map_err(|error| format!("{error:#}"))?;
    if let Some(layer) = changed_locked_layer(story, &rehearsal) {
        return Err(format!(
            "Layer {layer} is locked; unlock it in Scroll Studio first"
        ));
    }
    Ok(())
}

/// A locked layer the edit would change, move to another chapter or remove. Layers are
/// matched by ID, so edits to unlocked layers that only shift a locked layer's position in
/// the stack are allowed.
fn changed_locked_layer(before: &Story, after: &Story) -> Option<u64> {
    let after_layers: std::collections::HashMap<_, _> = after
        .chapters
        .iter()
        .flat_map(|c| c.layers.iter().map(move |l| (l.id, (c.id, l))))
        .collect();
    before.chapters.iter().find_map(|chapter| {
        chapter.layers.iter().find_map(|layer| {
            if !layer.locked {
                return None;
            }
            let same = after_layers
                .get(&layer.id)
                .is_some_and(|(c, l)| *c == chapter.id && *l == layer);
            (!same).then_some(layer.id)
        })
    })
}

/// What happened to a proposal.
#[derive(Clone, Debug, PartialEq)]
pub enum Decision {
    Applied(Box<Applied>),
    /// The user said no.
    Cancelled,
    /// It could not be applied (the story changed, or the addon was stopped).
    Failed(String),
}

/// The reply for a decided proposal.
pub fn decision_line(id: &Value, decision: &Decision) -> String {
    match decision {
        Decision::Applied(applied) => {
            result_line(id, json!({ "applied": true, "created": applied.created }))
        }
        Decision::Cancelled => error_line(id, CANCELLED, "Cancelled by the user"),
        Decision::Failed(reason) => error_line(id, REFUSED, reason),
    }
}

/// An edit waiting for the user.
#[derive(Clone, Debug, PartialEq)]
pub struct Proposal {
    pub request_id: Value,
    pub command: Command,
}

/// How a started addon is doing.
#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Running,
    /// Stopped, by itself or by the app; the message says why.
    Stopped(String),
}

/// What a poll found.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Polled {
    /// A new proposal is waiting for the user.
    pub proposed: bool,
    /// Messages may still be waiting; poll again next frame.
    pub more: bool,
}

/// A started addon and its open proposal.
pub struct Running {
    pub manifest: Manifest,
    process: AddonProcess,
    pub proposal: Option<Proposal>,
    pub status: Status,
    /// Suggestions cancelled in a row.
    cancelled: u32,
}

impl Running {
    /// Start talking to a started addon. `revision` is the story's current revision.
    pub fn new(manifest: Manifest, process: AddonProcess, revision: u64) -> Self {
        let mut running = Self {
            manifest,
            process,
            proposal: None,
            status: Status::Running,
            cancelled: 0,
        };
        let ready = notification_line(
            "host.ready",
            json!({
                "protocol": PROTOCOL_VERSION,
                "app_version": env!("CARGO_PKG_VERSION"),
                "permissions": running.manifest.permissions,
                "revision": revision,
            }),
        );
        running.send(ready);
        running
    }

    /// Answer what the addon has sent, at most [`MAX_EVENTS_PER_POLL`] messages.
    pub fn poll(&mut self, story: &Story) -> Polled {
        let mut polled = Polled::default();
        for _ in 0..MAX_EVENTS_PER_POLL {
            if self.status != Status::Running {
                return polled;
            }
            let Some(event) = self.process.try_recv() else {
                return polled;
            };
            match event {
                Event::Line(line) => match parse_message(&line) {
                    // Checked before rehearsing, so a waiting suggestion makes spam cheap.
                    FromAddon::Request { id, method, .. }
                        if self.proposal.is_some() && method.starts_with("edit.") =>
                    {
                        self.send(error_line(
                            &id,
                            BUSY,
                            "Another suggestion is waiting for the user",
                        ))
                    }
                    FromAddon::Request { id, method, params } => {
                        match handle(story, &self.manifest, &method, &params) {
                            Action::Reply(result) => self.send(result_line(&id, result)),
                            Action::Fail { code, message } => {
                                self.send(error_line(&id, code, &message))
                            }
                            Action::Propose(_) if self.proposal.is_some() => self.send(error_line(
                                &id,
                                BUSY,
                                "Another suggestion is waiting for the user",
                            )),
                            Action::Propose(command) => {
                                self.proposal = Some(Proposal {
                                    request_id: id,
                                    command: *command,
                                });
                                polled.proposed = true;
                            }
                        }
                    }
                    FromAddon::Notification { .. } | FromAddon::Response => {}
                    FromAddon::Invalid { id, code, reason } => {
                        self.send(error_line(&id.unwrap_or(Value::Null), code, &reason))
                    }
                },
                Event::TooLong => self.send(error_line(
                    &Value::Null,
                    INVALID_REQUEST,
                    "Message too long",
                )),
                Event::Closed => self.stopped("The addon stopped".into()),
            }
        }
        polled.more = self.status == Status::Running;
        polled
    }

    /// Tell the addon what happened to its proposal.
    pub fn resolve(&mut self, decision: Decision) {
        let Some(proposal) = self.proposal.take() else {
            return;
        };
        match &decision {
            Decision::Cancelled => self.cancelled += 1,
            Decision::Applied(_) => self.cancelled = 0,
            Decision::Failed(_) => {}
        }
        self.send(decision_line(&proposal.request_id, &decision));
        if self.cancelled >= MAX_CANCELLED {
            self.stop(format!(
                "Stopped after {MAX_CANCELLED} cancelled suggestions in a row"
            ));
        }
    }

    pub fn story_changed(&mut self, change: &StoryChanged) {
        if self.manifest.has(Permission::StoryRead) {
            self.send(notification_line(
                "story.changed",
                story_changed_params(change),
            ));
        }
    }

    /// Stop the program; anything it had waiting is dropped.
    pub fn stop(&mut self, reason: String) {
        self.process.stop();
        self.proposal = None;
        self.status = Status::Stopped(reason);
    }

    fn send(&mut self, line: String) {
        if self.status == Status::Running {
            if let Err(error) = self.process.send(line) {
                self.stopped(format!("{error:#}"));
            }
        }
    }

    /// Stop after the addon stopped or failed, keeping its last error output in the reason.
    fn stopped(&mut self, reason: String) {
        let log = self.process.recent_errors();
        self.stop(if log.is_empty() {
            reason
        } else {
            format!("{reason}: {log}")
        });
    }
}
