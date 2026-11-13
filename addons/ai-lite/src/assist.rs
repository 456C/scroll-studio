//! The star button: turn the user's request into one edit, using a model server on their
//! computer, and propose it to Scroll Studio.

use crate::{http, Host, RpcError, State, CANCELLED, INVALID_PARAMS, REFUSED};
use serde_json::{json, Value};
use std::time::Duration;

/// How long a model may take to answer.
const MODEL_TIMEOUT: Duration = Duration::from_secs(180);
/// Text sent to the model about what was asked, and about the story, is cut to these many
/// characters each: the subject always gets its share.
const MAX_SUBJECT_CHARS: usize = 16_000;
const MAX_STORY_CHARS: usize = 8_000;
const MAX_EDITS: usize = 50;

/// What the edits may do, by what the user asked about.
fn allowed(kind: &str) -> &'static [&'static str] {
    match kind {
        "character" => &[
            "set_field",
            "replace_text",
            "set_gender",
            "add_outfit",
            "set_outfit",
            "add_palette_color",
            "set_palette_color",
        ],
        "lore" => &[
            "set_field",
            "replace_text",
            "add_lore_note",
            "add_lore_card",
            "move_lore_card",
            "connect_lore",
            "set_lore_connection",
        ],
        _ => &["set_field", "replace_text"],
    }
}

/// Answer an `addon.assist` request: `{"applied": bool, "summary": text}` once the user
/// has decided, or an error saying what went wrong.
pub fn run(
    state: &State,
    host: &dyn Host,
    request: u64,
    params: &Value,
) -> Result<Value, RpcError> {
    let wanted = params["request"].as_str().unwrap_or_default().trim();
    if wanted.is_empty() {
        return Err(RpcError::new(INVALID_PARAMS, "Say what should change"));
    }
    let target = &params["target"];
    let kind = target["kind"].as_str().unwrap_or("story");
    // What edits may touch: the object asked about (the story is 0), or anything for lore.
    let subject = match kind {
        "lore" => None,
        "character" | "panel" | "location" => Some(
            target["id"]
                .as_u64()
                .ok_or_else(|| RpcError::new(INVALID_PARAMS, "The request names no object"))?,
        ),
        _ => Some(0),
    };
    let context = gather(host, target)?;
    let settings = state
        .settings
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    let server = http::parse_url(&settings.server).map_err(fail)?;
    let model = if settings.model.trim().is_empty() {
        first_model(&server)?
    } else {
        settings.model.trim().to_owned()
    };
    let methods = state.methods.get().cloned().unwrap_or_default();
    let mut messages = vec![
        json!({ "role": "system", "content": instructions(kind, subject, &methods) }),
        json!({ "role": "user", "content": format!(
            "Story context (JSON):\n{context}\n\nRequest: {wanted}"
        ) }),
    ];
    // One retry: if Scroll Studio cannot use the edits, the model sees why.
    for attempt in 0..2 {
        if state.is_cancelled(request) {
            return Err(RpcError::new(CANCELLED, "Cancelled"));
        }
        let reply = chat(&server, &model, &messages)?;
        if state.is_cancelled(request) {
            return Err(RpcError::new(CANCELLED, "Cancelled"));
        }
        let (summary, edits) = match parse_reply(&reply, kind, subject) {
            Ok(parsed) => parsed,
            Err(problem) if attempt == 0 => {
                retry(&mut messages, &reply, &problem);
                continue;
            }
            Err(problem) => return Err(fail(problem)),
        };
        if edits.is_empty() {
            return Ok(json!({ "applied": false, "summary": summary }));
        }
        match host.call("edit.batch", Value::Array(edits)) {
            Ok(applied) => {
                return Ok(
                    json!({ "applied": applied["applied"].as_bool().unwrap_or(true),
                    "summary": summary }),
                )
            }
            Err(error) if error.code == CANCELLED => {
                return Ok(json!({ "applied": false }));
            }
            Err(error)
                if attempt == 0 && (error.code == INVALID_PARAMS || error.code == REFUSED) =>
            {
                retry(&mut messages, &reply, &error.message);
            }
            Err(error) => return Err(error),
        }
    }
    Err(fail(
        "The model could not produce a change Scroll Studio accepts",
    ))
}

fn fail(message: impl Into<String>) -> RpcError {
    RpcError::new(REFUSED, message)
}

fn retry(messages: &mut Vec<Value>, reply: &str, problem: &str) {
    messages.push(json!({ "role": "assistant", "content": reply }));
    messages.push(json!({ "role": "user", "content": format!(
        "Scroll Studio could not use that: {problem}\nReply again with corrected JSON only."
    ) }));
}

/// What the request is about, then the story's summary, each within its own budget.
fn gather(host: &dyn Host, target: &Value) -> Result<String, RpcError> {
    let summary = host.call("story.summary", Value::Null)?;
    let about = match target["kind"].as_str() {
        Some("lore") => host.call("lore.board", Value::Null)?,
        Some("character") | Some("panel") | Some("location") => {
            let id = target["id"]
                .as_u64()
                .ok_or_else(|| RpcError::new(INVALID_PARAMS, "The request names no object"))?;
            host.call("object.get", json!({ "id": id }))?
        }
        _ => host.call("object.get", json!({ "id": 0 }))?,
    };
    Ok(format!(
        "Subject:\n{}\n\nStory:\n{}",
        cut(about.to_string(), MAX_SUBJECT_CHARS),
        cut(summary.to_string(), MAX_STORY_CHARS)
    ))
}

fn cut(text: String, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text;
    }
    let mut cut: String = text.chars().take(limit).collect();
    cut.push_str(" …(cut)");
    cut
}

/// What the model is told: its job, the reply format, and the methods it may use, with
/// their parameters as Scroll Studio describes them.
fn instructions(kind: &str, subject: Option<u64>, described: &[Value]) -> String {
    let mut methods = String::new();
    for name in allowed(kind) {
        let full = format!("edit.{name}");
        let schema = described
            .iter()
            .find(|m| m["method"] == full.as_str())
            .map(|m| {
                format!(
                    "{} Parameters: {}",
                    m["about"].as_str().unwrap_or(""),
                    m["params"]
                )
            })
            .unwrap_or_default();
        methods.push_str(&format!("- {name}: {schema}\n"));
    }
    let scope = match subject {
        Some(id) => format!("Change only the subject: every ID you use must be {id}. "),
        None => "Use the IDs in the story context. ".to_owned(),
    };
    format!(
        "You help write a vertical comic in Scroll Studio by proposing edits to its story data. \
The user reviews every edit before it is applied.\n\
Reply with JSON only, no prose and no code fences:\n\
{{\"summary\": \"one short sentence\", \"edits\": [{{\"method\": \"set_field\", \"params\": {{...}}}}]}}\n\
Use only these methods:\n{methods}\
{scope}Keep existing text the user did not ask \
to change. Write in the story's language and voice. If the request cannot be done with these methods, \
reply with an empty edits list and say why in the summary."
    )
}

/// The summary and the edits, checked against the allowed methods.
fn parse_reply(
    reply: &str,
    kind: &str,
    subject: Option<u64>,
) -> Result<(String, Vec<Value>), String> {
    let value = extract_json(reply).ok_or("The reply was not JSON")?;
    let summary = value["summary"]
        .as_str()
        .unwrap_or("Suggested change")
        .chars()
        .take(200)
        .collect();
    let edits = value["edits"]
        .as_array()
        .ok_or("The reply has no \"edits\" list")?;
    if edits.len() > MAX_EDITS {
        return Err(format!("Use at most {MAX_EDITS} edits"));
    }
    let mut checked = Vec::new();
    for edit in edits {
        let method = edit["method"]
            .as_str()
            .map(|m| m.trim_start_matches("edit."))
            .ok_or("Every edit needs a \"method\"")?;
        if !allowed(kind).contains(&method) {
            return Err(format!("`{method}` is not one of the methods you may use"));
        }
        let mut entry = json!({ "method": method });
        if let (Some(subject), Some(params)) = (subject, edit.get("params")) {
            // Story text can say anything; the edit may still only touch what was asked about.
            only_touches(params, subject)?;
        }
        if let Some(params) = edit.get("params").filter(|p| !p.is_null()) {
            entry["params"] = params.clone();
        }
        checked.push(entry);
    }
    Ok((summary, checked))
}

/// Every ID in `params` is `subject`; a whole-story find-and-replace only for the story.
fn only_touches(params: &Value, subject: u64) -> Result<(), String> {
    let Some(map) = params.as_object() else {
        return Ok(());
    };
    for (key, value) in map {
        if key.ends_with("_id") && value.as_u64() != Some(subject) {
            return Err(format!(
                "`{key}` must be {subject}, the object the user asked about"
            ));
        }
        if key == "scope" && value.is_string() && subject != 0 {
            return Err(format!("Use {{\"target_id\": {subject}}} as the scope"));
        }
        if value.is_object() {
            only_touches(value, subject)?;
        }
    }
    Ok(())
}

/// The first JSON object in a reply, allowing for code fences and text around it.
fn extract_json(reply: &str) -> Option<Value> {
    if let Ok(value) = serde_json::from_str::<Value>(reply.trim()) {
        return value.is_object().then_some(value);
    }
    let start = reply.find('{')?;
    let mut stream = serde_json::Deserializer::from_str(&reply[start..]).into_iter::<Value>();
    stream.next()?.ok().filter(Value::is_object)
}

/// Ask the model server for one reply (OpenAI-compatible chat completions).
fn chat(server: &http::Url, model: &str, messages: &[Value]) -> Result<String, RpcError> {
    let body = json!({
        "model": model,
        "messages": messages,
        "temperature": 0.2,
        "stream": false,
    });
    let answer = http::call_json(
        "POST",
        server,
        "/v1/chat/completions",
        Some(&body),
        MODEL_TIMEOUT,
    )
    .map_err(fail)?;
    answer["choices"][0]["message"]["content"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| fail("The model server sent no reply text"))
}

/// The first model the server lists, for when the user has not chosen one.
fn first_model(server: &http::Url) -> Result<String, RpcError> {
    let models = http::call_json("GET", server, "/v1/models", None, Duration::from_secs(10))
        .map_err(fail)?;
    models["data"][0]["id"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| {
            fail("The model server has no models. Download one (for example `ollama pull llama3.2`) or enter a model name in AI Lite's settings")
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replies_are_found_in_fences_and_checked() {
        let reply = "Sure!\n```json\n{\"summary\": \"Darker\", \"edits\": [{\"method\": \"edit.set_field\", \"params\": {\"target_id\": 4, \"field\": \"hair\", \"value\": \"Black\"}}]}\n```";
        let (summary, edits) = parse_reply(reply, "character", Some(4)).unwrap();
        assert_eq!(summary, "Darker");
        assert_eq!(edits[0]["method"], "set_field");
        assert_eq!(edits[0]["params"]["value"], "Black");
        // Methods outside what the request is about are refused.
        let reply = r#"{"edits": [{"method": "delete_panel", "params": {"panel_id": 3}}]}"#;
        assert!(parse_reply(reply, "panel", Some(3)).is_err());
        assert!(parse_reply("no json here", "story", Some(0)).is_err());
        assert!(parse_reply(r#"{"summary": "x"}"#, "story", Some(0)).is_err());
        let (_, none) =
            parse_reply(r#"{"summary": "Can't", "edits": []}"#, "story", Some(0)).unwrap();
        assert!(none.is_empty());
    }

    #[test]
    fn edits_may_only_touch_what_was_asked_about() {
        let other = r#"{"edits": [{"method": "set_field", "params": {"target_id": 9, "field": "name", "value": "x"}}]}"#;
        assert!(parse_reply(other, "character", Some(4)).is_err());
        let everywhere = r#"{"edits": [{"method": "replace_text", "params": {"find": "a", "replace": "b", "scope": "story"}}]}"#;
        assert!(parse_reply(everywhere, "character", Some(4)).is_err());
        assert!(parse_reply(everywhere, "story", Some(0)).is_ok());
        let nested = r#"{"edits": [{"method": "replace_text", "params": {"find": "a", "replace": "b", "scope": {"target_id": 9}}}]}"#;
        assert!(parse_reply(nested, "panel", Some(3)).is_err());
        // The lore board may link anything.
        let lore = r#"{"edits": [{"method": "connect_lore", "params": {"from_id": 1, "to_id": 2, "label": "x"}}]}"#;
        assert!(parse_reply(lore, "lore", None).is_ok());
    }

    #[test]
    fn instructions_list_only_allowed_methods_with_their_parameters() {
        let described = vec![
            json!({ "method": "edit.set_field", "about": "Replace one text field.",
            "params": { "type": "object" } }),
        ];
        let text = instructions("story", Some(0), &described);
        assert!(text.contains("- set_field: Replace one text field."));
        assert!(text.contains("- replace_text"));
        assert!(!text.contains("add_lore_note"));
    }
}
