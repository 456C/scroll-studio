//! AI Lite, a Scroll Studio addon.
//!
//! - **Star button:** the user types a request; AI Lite asks a model server on their
//!   computer (Ollama, LM Studio, llama.cpp: anything with an OpenAI-compatible
//!   `/v1/chat/completions`) and proposes the edit, which the user reviews in Scroll Studio.
//! - **MCP server:** off by default. When on, AI apps the user already has can read the
//!   story and propose edits through `http://127.0.0.1:<port>/mcp` with a token that
//!   changes every session. Edits still need the user's yes in Scroll Studio.
//!
//! It talks to Scroll Studio over stdin/stdout (JSON-RPC 2.0, one message per line) and
//! exits when its input closes.

mod assist;
mod http;
mod mcp;
mod settings;

use serde_json::{json, Value};
use settings::Settings;
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, Write};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

/// Error codes Scroll Studio uses (see its addon host).
pub const INVALID_PARAMS: i64 = -32602;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const REFUSED: i64 = -32002;
pub const CANCELLED: i64 = -32003;
pub const BUSY: i64 = -32004;

#[derive(Clone, Debug, PartialEq)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
}

impl RpcError {
    pub fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

/// Scroll Studio, as AI Lite sees it: something to send requests to.
pub trait Host: Send + Sync {
    fn call(&self, method: &str, params: Value) -> Result<Value, RpcError>;
}

/// The stdin/stdout connection to Scroll Studio.
pub struct Link {
    out: Mutex<std::io::Stdout>,
    next: Mutex<u64>,
    waiting: Mutex<HashMap<u64, Sender<Result<Value, RpcError>>>>,
}

impl Link {
    fn new() -> Self {
        Self {
            out: Mutex::new(std::io::stdout()),
            next: Mutex::new(1),
            waiting: Mutex::new(HashMap::new()),
        }
    }

    fn write(&self, message: &Value) {
        let mut out = self.out.lock().unwrap_or_else(|e| e.into_inner());
        // If Scroll Studio is gone, stdin closes too and the addon exits.
        let _ = writeln!(out, "{message}").and_then(|_| out.flush());
    }

    fn reply(&self, id: &Value, result: Result<Value, RpcError>) {
        self.write(&match result {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err(error) => json!({ "jsonrpc": "2.0", "id": id,
                "error": { "code": error.code, "message": error.message } }),
        });
    }

    fn deliver(&self, message: &Value) {
        let Some(id) = message["id"].as_u64() else {
            return;
        };
        let Some(sender) = self
            .waiting
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&id)
        else {
            return;
        };
        let _ = sender.send(match message.get("error") {
            Some(error) => Err(RpcError::new(
                error["code"].as_i64().unwrap_or(0),
                error["message"]
                    .as_str()
                    .unwrap_or("Scroll Studio reported an error"),
            )),
            None => Ok(message["result"].clone()),
        });
    }
}

impl Host for Link {
    fn call(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        let id = {
            let mut next = self.next.lock().unwrap_or_else(|e| e.into_inner());
            *next += 1;
            *next
        };
        let (sender, receiver) = mpsc::channel();
        self.waiting
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id, sender);
        self.write(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        // Edits wait for the user; reads come back at once.
        let wait = if method.starts_with("edit.") {
            Duration::from_secs(24 * 60 * 60)
        } else {
            Duration::from_secs(30)
        };
        let answer = receiver
            .recv_timeout(wait)
            .unwrap_or_else(|_| Err(RpcError::new(0, "Scroll Studio did not answer")));
        self.waiting
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&id);
        answer
    }
}

/// Everything the addon's threads share.
pub struct State {
    pub settings: Mutex<Settings>,
    /// The MCP token for this session.
    pub token: String,
    /// The API as Scroll Studio describes it, once known.
    pub methods: OnceLock<Vec<Value>>,
    /// Star-button requests the user gave up on.
    pub cancelled: Mutex<HashSet<u64>>,
    pub mcp: Mutex<mcp::Status>,
}

impl State {
    pub fn is_cancelled(&self, id: u64) -> bool {
        self.cancelled
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(&id)
    }
}

fn main() {
    let state = Arc::new(State {
        settings: Mutex::new(Settings::load()),
        token: random_token(),
        methods: OnceLock::new(),
        cancelled: Mutex::new(HashSet::new()),
        mcp: Mutex::new(mcp::Status::Off),
    });
    let link = Arc::new(Link::new());
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let method = message["method"].as_str().map(str::to_owned);
        match (method, message.get("id").filter(|id| !id.is_null())) {
            // A request from Scroll Studio: answered on its own thread, so a slow model
            // never holds up the answers Scroll Studio sends back.
            (Some(method), Some(id)) => {
                let (state, link, id) = (state.clone(), link.clone(), id.clone());
                let params = message["params"].clone();
                std::thread::spawn(move || {
                    let host: Arc<dyn Host> = link.clone();
                    // Whatever happens, Scroll Studio gets an answer and is not left waiting.
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        handle(&state, &host, &method, &id, params)
                    }))
                    .unwrap_or_else(|_| Err(RpcError::new(REFUSED, "AI Lite ran into a problem")));
                    if let Some(id) = id.as_u64() {
                        let mut cancelled =
                            state.cancelled.lock().unwrap_or_else(|e| e.into_inner());
                        cancelled.remove(&id);
                    }
                    link.reply(&id, result);
                });
            }
            (Some(method), None) => match method.as_str() {
                "host.ready" => {
                    let (state, link) = (state.clone(), link.clone());
                    std::thread::spawn(move || {
                        let host: Arc<dyn Host> = link;
                        if let Ok(described) = host.call("host.describe", Value::Null) {
                            let methods = described["methods"].as_array().cloned();
                            let _ = state.methods.set(methods.unwrap_or_default());
                        }
                        mcp::apply_settings(&state, &host);
                    });
                }
                "addon.cancel" => {
                    if let Some(id) = message["params"]["id"].as_u64() {
                        let mut cancelled =
                            state.cancelled.lock().unwrap_or_else(|e| e.into_inner());
                        cancelled.insert(id);
                    }
                }
                _ => {}
            },
            (None, _) => link.deliver(&message),
        }
    }
    // Scroll Studio closed the connection: stop.
}

fn handle(
    state: &Arc<State>,
    host: &Arc<dyn Host>,
    method: &str,
    id: &Value,
    params: Value,
) -> Result<Value, RpcError> {
    match method {
        "addon.settings" => Ok(settings::fields(state)),
        "addon.set_setting" => {
            let key = params["key"].as_str().unwrap_or_default();
            let problem = {
                let mut settings = state.settings.lock().unwrap_or_else(|e| e.into_inner());
                settings.set(key, &params["value"]).err()
            };
            if problem.is_none() {
                mcp::apply_settings(state, host);
            }
            let mut fields = settings::fields(state);
            if let Some(problem) = problem {
                settings::push_status(&mut fields, &problem);
            }
            Ok(fields)
        }
        "addon.assist" => {
            let request = id.as_u64().unwrap_or(0);
            assist::run(state, host.as_ref(), request, &params)
        }
        _ => Err(RpcError::new(
            METHOD_NOT_FOUND,
            format!("AI Lite does not know `{method}`"),
        )),
    }
}

/// 32 random bytes as hex.
fn random_token() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("the system has no random numbers");
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
