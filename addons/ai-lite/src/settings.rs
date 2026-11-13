//! AI Lite's settings, kept in `config.json` in the addon's folder (never in a story).

use crate::{http, State};
use serde_json::{json, Value};
use std::path::PathBuf;

pub const DEFAULT_SERVER: &str = "http://127.0.0.1:11434";
pub const DEFAULT_PORT: u16 = 39390;

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    /// The model server's address (plain http).
    pub server: String,
    /// The model to use; empty uses the first one the server lists.
    pub model: String,
    pub mcp: bool,
    pub port: u16,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            server: DEFAULT_SERVER.into(),
            model: String::new(),
            mcp: false,
            port: DEFAULT_PORT,
        }
    }
}

fn path() -> PathBuf {
    // Scroll Studio starts addons in their own folder.
    PathBuf::from("config.json")
}

impl Settings {
    pub fn load() -> Self {
        std::fs::read(path())
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .map(|value| Self::from_json(&value))
            .unwrap_or_default()
    }

    fn from_json(value: &Value) -> Self {
        let defaults = Self::default();
        Self {
            server: value["server"]
                .as_str()
                .filter(|s| http::parse_url(s).is_ok())
                .map_or(defaults.server, str::to_owned),
            model: value["model"]
                .as_str()
                .map_or(defaults.model, |s| s.chars().take(200).collect()),
            mcp: value["mcp"].as_bool().unwrap_or(defaults.mcp),
            port: value["port"]
                .as_u64()
                .and_then(|p| u16::try_from(p).ok())
                .filter(|p| *p >= 1024)
                .unwrap_or(defaults.port),
        }
    }

    fn to_json(&self) -> Value {
        json!({ "server": self.server, "model": self.model, "mcp": self.mcp, "port": self.port })
    }

    /// Change one setting and save. On a bad value nothing changes and the error says why.
    pub fn set(&mut self, key: &str, value: &Value) -> Result<(), String> {
        let text = || value.as_str().map(str::trim).ok_or("Expected text");
        let mut next = self.clone();
        match key {
            "server" => {
                let server = text()?;
                let server = if server.is_empty() {
                    DEFAULT_SERVER
                } else {
                    server
                };
                http::parse_url(server)?;
                next.server = server.to_owned();
            }
            "model" => next.model = text()?.chars().take(200).collect(),
            "mcp" => next.mcp = value.as_bool().ok_or("Expected on or off")?,
            "port" => {
                next.port = text()?
                    .parse::<u16>()
                    .ok()
                    .filter(|p| *p >= 1024)
                    .ok_or("Use a port from 1024 to 65535")?;
            }
            _ => return Err(format!("Unknown setting `{key}`")),
        }
        // Saved first, so a setting that could not be kept does not take effect either.
        next.save()?;
        *self = next;
        Ok(())
    }

    fn save(&self) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(&self.to_json()).map_err(|e| e.to_string())?;
        let temp = path().with_extension("json.tmp");
        std::fs::write(&temp, bytes)
            .and_then(|_| std::fs::rename(&temp, path()))
            .map_err(|e| format!("Could not save the settings: {e}"))
    }
}

/// The settings as Scroll Studio shows them.
pub fn fields(state: &State) -> Value {
    let settings = state
        .settings
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    let mut fields = vec![
        json!({ "key": "server", "label": "Model server", "kind": "text", "value": settings.server }),
        json!({ "key": "model", "label": "Model", "kind": "text", "value": settings.model }),
        json!({ "key": "mcp", "label": "MCP server", "kind": "toggle", "value": settings.mcp }),
    ];
    if settings.mcp {
        fields.push(
            json!({ "key": "port", "label": "MCP port", "kind": "text", "value": settings.port.to_string() }),
        );
        let status = state.mcp.lock().unwrap_or_else(|e| e.into_inner()).clone();
        match status {
            crate::mcp::Status::Listening(port) => {
                fields.push(
                    json!({ "key": "mcp_url", "label": "MCP address", "kind": "info",
                    "value": format!("http://127.0.0.1:{port}/mcp") }),
                );
                fields.push(
                    json!({ "key": "mcp_token", "label": "MCP token", "kind": "info",
                    "value": state.token }),
                );
            }
            crate::mcp::Status::Failed(reason) => {
                fields.push(
                    json!({ "key": "mcp_status", "label": "MCP", "kind": "info", "value": reason }),
                );
            }
            crate::mcp::Status::Off => {}
        }
    }
    json!({ "fields": fields })
}

pub fn push_status(fields: &mut Value, message: &str) {
    if let Some(list) = fields["fields"].as_array_mut() {
        list.push(json!({ "key": "status", "label": "Problem", "kind": "info", "value": message }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bad_values_change_nothing() {
        let mut settings = Settings::default();
        let before = settings.clone();
        assert!(settings
            .set("server", &json!("https://example.com"))
            .is_err());
        assert!(settings.set("port", &json!("80")).is_err());
        assert!(settings.set("mcp", &json!("yes")).is_err());
        assert!(settings.set("nope", &json!("x")).is_err());
        assert_eq!(settings, before);
    }

    #[test]
    fn saved_settings_are_checked_when_loaded() {
        let loaded =
            Settings::from_json(&json!({ "server": "https://x", "port": 22, "mcp": true }));
        assert_eq!(loaded.server, DEFAULT_SERVER);
        assert_eq!(loaded.port, DEFAULT_PORT);
        assert!(loaded.mcp);
    }
}
