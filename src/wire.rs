//! The JSON boundary for addons and the MCP server.
//!
//! A request is a method name and JSON parameters, as in JSON-RPC:
//! - Read methods are the read API's names (`story.summary`, `object.get`, ...).
//! - Edit methods are `edit.<command>`, using [`Command::name`]: `edit.set_field`,
//!   `edit.add_layer`, `edit.batch`, ...
//!
//! Edit parameters follow the read API's conventions: references end in `_id` or `_ids`,
//! the story is object 0, kinds use lowercase names. Layers use the `.scro` layer format.
//! Unknown parameters are refused rather than ignored, so a misspelt optional parameter
//! cannot silently change what an edit does.
//!
//! The boundary also enforces what may arrive from outside the app:
//! - Internal commands (the exact inverses used for undo) have no wire form at all.
//! - Commands that need image bytes (see [`Command::needs_image_bytes`]) are refused
//!   unless the [`Policy`] says the host supplies the bytes alongside.

use crate::{commands::Command, read_api::Query};
use anyhow::{bail, ensure, Context, Result};
use serde_json::{json, Map, Value};

/// Prefix of every edit method.
pub const EDIT_PREFIX: &str = "edit.";

/// Every edit method, in a stable order. Internal commands are not listed.
pub const EDIT_METHODS: [&str; 43] = [
    "edit.set_field",
    "edit.replace_text",
    "edit.add_panel",
    "edit.set_gender",
    "edit.add_outfit",
    "edit.set_outfit",
    "edit.remove_outfit",
    "edit.set_active_outfit",
    "edit.add_palette_color",
    "edit.set_palette_color",
    "edit.remove_palette_color",
    "edit.set_reference",
    "edit.remove_reference",
    "edit.add_lore_note",
    "edit.add_lore_card",
    "edit.remove_lore_card",
    "edit.move_lore_card",
    "edit.set_lore_card_color",
    "edit.connect_lore",
    "edit.set_lore_connection",
    "edit.disconnect_lore",
    "edit.set_chapter_height",
    "edit.add_chapter",
    "edit.set_panel_geometry",
    "edit.delete_panel",
    "edit.sort_panels",
    "edit.add_asset",
    "edit.add_image_asset",
    "edit.add_image_version",
    "edit.add_layer",
    "edit.set_layer",
    "edit.update_layer",
    "edit.delete_layer",
    "edit.move_layer",
    "edit.add_linked_bubble",
    "edit.add_group",
    "edit.set_group_color",
    "edit.set_group_members",
    "edit.remove_group",
    "edit.move_chronology",
    "edit.set_chronology_entry",
    "edit.order_chronology_by_reading",
    "edit.batch",
];

/// Every read method, in a stable order.
pub const READ_METHODS: [&str; 13] = [
    "story.summary",
    "chapters.list",
    "panels.list",
    "layers.list",
    "layer.export",
    "characters.list",
    "assets.list",
    "locations.list",
    "groups.list",
    "timeline.list",
    "object.get",
    "lore.board",
    "panels.at",
];

/// What the host allows through the boundary.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Policy {
    /// Accept commands that create image sheets. Only a host that stores the image bytes
    /// for the created IDs may set this.
    pub allow_image_commands: bool,
}

/// A parsed request.
#[derive(Clone, Debug, PartialEq)]
pub enum Request {
    Read(Query),
    Edit(Box<Command>),
}

/// Parse a request from its method and parameters, applying `policy`.
pub fn parse_request(method: &str, params: &Value, policy: Policy) -> Result<Request> {
    match method.strip_prefix(EDIT_PREFIX) {
        Some(name) => parse_command(name, params, policy).map(|c| Request::Edit(Box::new(c))),
        None => Query::parse(method, params).map(Request::Read),
    }
}

/// Parse an edit command by its name (without the `edit.` prefix).
pub fn parse_command(name: &str, params: &Value, policy: Policy) -> Result<Command> {
    ensure!(
        EDIT_METHODS.contains(&format!("{EDIT_PREFIX}{name}").as_str()),
        "Unknown edit `{EDIT_PREFIX}{name}`"
    );
    if name == "batch" {
        check_batch(params)?;
    }
    let request = normalize(json!({ "method": name, "params": params }));
    let command: Command = serde_json::from_value(request.clone())
        .with_context(|| format!("Invalid parameters for `{EDIT_PREFIX}{name}`"))?;
    // Re-encoding gives every parameter the command understood; anything else was a typo
    // or a parameter this version does not know.
    let (_, understood) = command_request(&command)?;
    reject_unknown(
        &request["params"],
        &understood,
        &format!("{EDIT_PREFIX}{name}"),
    )?;
    ensure!(
        !command.is_internal(),
        "`{EDIT_PREFIX}{name}` is not available here"
    );
    if command.needs_image_bytes() && !policy.allow_image_commands {
        bail!("`{EDIT_PREFIX}{name}` needs image data, which this connection cannot send");
    }
    Ok(command)
}

/// The method and parameters for a command, for proposals, logs and round trips.
/// Internal commands have no wire form.
pub fn command_request(command: &Command) -> Result<(String, Value)> {
    ensure!(!command.is_internal(), "This command has no wire form");
    let value = serde_json::to_value(command).context("Could not encode the command")?;
    let method = value["method"]
        .as_str()
        .context("Encoded command has no method")?
        .to_owned();
    let params = value.get("params").cloned().unwrap_or(Value::Null);
    Ok((format!("{EDIT_PREFIX}{method}"), params))
}

/// The most entries one `edit.batch` may hold.
pub const MAX_BATCH: usize = 1000;

/// A batch is a flat list of commands: no batches inside batches, which would allow
/// unbounded nesting (and recursion) from outside.
fn check_batch(params: &Value) -> Result<()> {
    let entries = params
        .as_array()
        .context("`edit.batch` takes a list of {method, params} entries")?;
    ensure!(
        entries.len() <= MAX_BATCH,
        "`edit.batch` holds at most {MAX_BATCH} commands"
    );
    for (i, entry) in entries.iter().enumerate() {
        let method = entry
            .get("method")
            .and_then(Value::as_str)
            .with_context(|| format!("`edit.batch[{i}]` needs a `method`"))?;
        ensure!(
            method != "batch",
            "`edit.batch[{i}]` is a batch; batches cannot be nested"
        );
    }
    Ok(())
}

/// Commands that take no parameters; clients often send `{}` for those.
const NO_PARAMS: [&str; 2] = ["add_group", "order_chronology_by_reading"];

/// Accept `{}` as "no parameters", here and in batch entries.
fn normalize(mut request: Value) -> Value {
    let method = request["method"].as_str().unwrap_or_default().to_owned();
    if NO_PARAMS.contains(&method.as_str())
        && request["params"].as_object().is_some_and(Map::is_empty)
    {
        request["params"] = Value::Null;
    }
    if method == "batch" {
        if let Some(entries) = request["params"].as_array_mut() {
            for entry in entries {
                *entry = normalize(std::mem::take(entry));
            }
        }
    }
    request
}

/// Every key in `given` must also appear in `understood` (recursively through objects and
/// through batches, whose parameters are lists of `{method, params}`).
/// A `null` the command did not re-encode stands for "absent" and is accepted.
fn reject_unknown(given: &Value, understood: &Value, path: &str) -> Result<()> {
    match (given, understood) {
        (Value::Object(given), Value::Object(understood)) => {
            for (key, value) in given {
                match understood.get(key) {
                    Some(known) => reject_unknown(value, known, &format!("{path}.{key}"))?,
                    None if value.is_null() => {}
                    None => bail!("Unknown parameter `{key}` in `{path}`"),
                }
            }
            Ok(())
        }
        // An object where the command has a plain value (e.g. `{"story": null}` for
        // `"story"`) is a different shape, not an alias.
        (Value::Object(_), known) if !known.is_null() => {
            bail!("Unexpected object for `{path}`")
        }
        (Value::Array(given), Value::Array(understood)) => {
            for (i, (value, known)) in given.iter().zip(understood).enumerate() {
                reject_unknown(value, known, &format!("{path}[{i}]"))?;
            }
            Ok(())
        }
        // Batches: each entry is `{method, params}` with the method still unprefixed.
        _ => Ok(()),
    }
}

/// Encode a batch entry the way `edit.batch` expects it.
pub fn batch_entry(command: &Command) -> Result<Value> {
    let (method, params) = command_request(command)?;
    let mut entry = Map::new();
    entry.insert(
        "method".into(),
        method.trim_start_matches(EDIT_PREFIX).to_owned().into(),
    );
    // Commands without parameters encode no `params`.
    if !params.is_null() {
        entry.insert("params".into(), params);
    }
    Ok(Value::Object(entry))
}

pub mod docs;

#[cfg(test)]
mod tests;
