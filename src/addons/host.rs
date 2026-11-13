//! The addon protocol: JSON-RPC 2.0, one message per line, over the addon's stdin/stdout.
//!
//! - **Addon → app requests** use [`crate::wire`] methods: read methods need
//!   `story.read`; `edit.*` methods need `story.propose` and become a proposal that the
//!   user applies or cancels. The reply arrives when they decide: `{"applied": true,
//!   "created": [...]}` or error [`CANCELLED`]. Each addon has at most one proposal open.
//!   `host.methods` lists what this addon may call.
//! - **App → addon notifications:** `host.ready {protocol, app_version, permissions}` once
//!   at start, then `story.changed {revision, scope}` for addons that may read.
//!
//! Edits that would change a locked layer are refused before the user sees them.

use super::manifest::{Manifest, Permission};
use super::process::{AddonProcess, Event};
use crate::commands::{Applied, ChangeScope, Command, StoryChanged};
use crate::model::Story;
use crate::wire::{self, Policy, Request};
use anyhow::Result;
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
    /// A line that is not a JSON-RPC request: answered with an error when it carries an
    /// ID, otherwise ignored (addons may print stray output).
    Invalid { id: Option<Value>, reason: String },
}

pub fn parse_message(line: &str) -> FromAddon {
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        return FromAddon::Invalid {
            id: None,
            reason: "not JSON".into(),
        };
    };
    let id = value
        .get("id")
        .cloned()
        .filter(|id| id.is_string() || id.is_number());
    let invalid = |reason: &str| FromAddon::Invalid {
        id: id.clone(),
        reason: reason.into(),
    };
    if value.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return invalid("expected \"jsonrpc\": \"2.0\"");
    }
    let Some(method) = value.get("method").and_then(Value::as_str) else {
        return invalid("expected a method");
    };
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
        Request::Edit(command) => {
            // Rehearse on a copy so the user only ever sees edits that will apply.
            let mut rehearsal = story.clone();
            if let Err(error) = rehearsal.apply((*command).clone()) {
                return fail(REFUSED, format!("{error:#}"));
            }
            if let Some(layer) = changed_locked_layer(story, &rehearsal) {
                return fail(
                    REFUSED,
                    format!("Layer {layer} is locked; unlock it in Scroll Studio first"),
                );
            }
            Action::Propose(command)
        }
    }
}

/// A locked layer the edit would change, move or remove.
fn changed_locked_layer(before: &Story, after: &Story) -> Option<u64> {
    before.chapters.iter().find_map(|chapter| {
        let later = after.chapters.iter().find(|c| c.id == chapter.id);
        chapter
            .layers
            .iter()
            .enumerate()
            .find_map(|(index, layer)| {
                if !layer.locked {
                    return None;
                }
                let same = later.is_some_and(|c| c.layers.get(index) == Some(layer));
                (!same).then_some(layer.id)
            })
    })
}

/// The reply for a proposal the user decided.
pub fn decision_line(id: &Value, decision: &Result<Applied, String>) -> String {
    match decision {
        Ok(applied) => result_line(id, json!({ "applied": true, "created": applied.created })),
        Err(reason) => error_line(id, CANCELLED, reason),
    }
}

/// An edit waiting for the user.
#[derive(Clone, Debug, PartialEq)]
pub struct Proposal {
    pub request_id: Value,
    pub command: Command,
}

/// How a running addon is doing.
#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Running,
    /// Stopped by itself; the message includes its last error output.
    Stopped(String),
}

/// A started addon and its open proposal.
pub struct Running {
    pub manifest: Manifest,
    process: AddonProcess,
    pub proposal: Option<Proposal>,
    pub status: Status,
}

impl Running {
    pub fn new(manifest: Manifest, mut process: AddonProcess) -> Self {
        let ready = notification_line(
            "host.ready",
            json!({
                "protocol": PROTOCOL_VERSION,
                "app_version": env!("CARGO_PKG_VERSION"),
                "permissions": manifest.permissions,
            }),
        );
        let status = match process.send(&ready) {
            Ok(()) => Status::Running,
            Err(error) => Status::Stopped(format!("{error:#}")),
        };
        Self {
            manifest,
            process,
            proposal: None,
            status,
        }
    }

    /// Answer everything the addon has sent. Returns true when a new proposal is waiting.
    pub fn poll(&mut self, story: &Story) -> bool {
        let mut proposed = false;
        while let Some(event) = self.process.try_recv() {
            match event {
                Event::Line(line) => match parse_message(&line) {
                    FromAddon::Request { id, method, params } => {
                        match handle(story, &self.manifest, &method, &params) {
                            Action::Reply(result) => self.send(&result_line(&id, result)),
                            Action::Fail { code, message } => {
                                self.send(&error_line(&id, code, &message))
                            }
                            Action::Propose(_) if self.proposal.is_some() => {
                                self.send(&error_line(
                                    &id,
                                    BUSY,
                                    "Another suggestion is waiting for the user",
                                ))
                            }
                            Action::Propose(command) => {
                                self.proposal = Some(Proposal {
                                    request_id: id,
                                    command: *command,
                                });
                                proposed = true;
                            }
                        }
                    }
                    FromAddon::Notification { .. } => {}
                    FromAddon::Invalid {
                        id: Some(id),
                        reason,
                    } => self.send(&error_line(&id, INVALID_REQUEST, &reason)),
                    FromAddon::Invalid { id: None, .. } => {}
                },
                Event::TooLong => {}
                Event::Closed => {
                    let log = self.process.recent_errors();
                    self.status = Status::Stopped(if log.is_empty() {
                        "The addon stopped".into()
                    } else {
                        format!("The addon stopped: {log}")
                    });
                    self.proposal = None;
                }
            }
        }
        proposed
    }

    /// Tell the addon what the user decided about its proposal.
    pub fn resolve(&mut self, decision: Result<Applied, String>) {
        if let Some(proposal) = self.proposal.take() {
            self.send(&decision_line(&proposal.request_id, &decision));
        }
    }

    pub fn story_changed(&mut self, change: &StoryChanged) {
        if self.manifest.has(Permission::StoryRead) {
            self.send(&notification_line(
                "story.changed",
                story_changed_params(change),
            ));
        }
    }

    fn send(&mut self, line: &str) {
        if self.status == Status::Running {
            if let Err(error) = self.process.send(line) {
                self.status = Status::Stopped(format!("{error:#}"));
            }
        }
    }
}
