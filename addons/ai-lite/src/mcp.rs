//! The MCP server: lets an AI app the user already has read the story and propose edits.
//!
//! Streamable HTTP transport on `127.0.0.1` only, answering each POST with
//! `application/json` (no server-sent events). Every request needs
//! `Authorization: Bearer <token>`, where the token is new each session and shown in AI
//! Lite's settings. Requests from web pages are refused by their `Origin`. Tools mirror
//! Scroll Studio's read and edit methods one to one; edits still wait for the user's yes.

use crate::{http, Host, RpcError, State, BUSY, CANCELLED};
use serde_json::{json, Value};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

/// Protocol versions this server speaks, newest first.
pub const PROTOCOL_VERSIONS: [&str; 3] = ["2025-11-25", "2025-06-18", "2025-03-26"];
const MAX_BODY: usize = 4 * 1024 * 1024;
const MAX_CONNECTIONS: usize = 8;
const MAX_SESSIONS: usize = 32;

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Off,
    Listening(u16),
    Failed(String),
}

/// The running server, so it can be stopped when the setting changes.
struct Server {
    port: u16,
    stop: Arc<AtomicBool>,
}

fn server() -> &'static Mutex<Option<Server>> {
    static SERVER: OnceLock<Mutex<Option<Server>>> = OnceLock::new();
    SERVER.get_or_init(|| Mutex::new(None))
}

/// Start, stop or move the server to match the settings.
pub fn apply_settings(state: &Arc<State>, host: &Arc<dyn Host>) {
    let settings = state
        .settings
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    let mut running = server().lock().unwrap_or_else(|e| e.into_inner());
    let wanted = if settings.mcp {
        Some(settings.port)
    } else {
        None
    };
    if running.as_ref().map(|s| s.port) == wanted {
        return;
    }
    if let Some(old) = running.take() {
        old.stop.store(true, Ordering::SeqCst);
        // Wake the accept loop so it sees the flag.
        let _ = TcpStream::connect(("127.0.0.1", old.port));
    }
    let status = if settings.mcp {
        match TcpListener::bind(("127.0.0.1", settings.port)) {
            Ok(listener) => {
                let stop = Arc::new(AtomicBool::new(false));
                let api = Api {
                    host: host.clone(),
                    state: state.clone(),
                    sessions: Mutex::new(Vec::new()),
                };
                spawn_accept(listener, Arc::new(api), stop.clone());
                *running = Some(Server {
                    port: settings.port,
                    stop,
                });
                Status::Listening(settings.port)
            }
            Err(error) => Status::Failed(format!(
                "Could not use port {} ({error}); choose another port",
                settings.port
            )),
        }
    } else {
        Status::Off
    };
    *state.mcp.lock().unwrap_or_else(|e| e.into_inner()) = status;
}

fn spawn_accept(listener: TcpListener, api: Arc<Api>, stop: Arc<AtomicBool>) {
    let active = Arc::new(AtomicUsize::new(0));
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            if stop.load(Ordering::SeqCst) {
                return;
            }
            let Ok(mut stream) = stream else { continue };
            // Loopback only, whatever the socket says.
            if !stream
                .peer_addr()
                .is_ok_and(|a: SocketAddr| a.ip().is_loopback())
            {
                continue;
            }
            if active.load(Ordering::SeqCst) >= MAX_CONNECTIONS {
                let _ = http::write_response(&mut stream, 503, &[], b"");
                continue;
            }
            active.fetch_add(1, Ordering::SeqCst);
            let (api, active) = (api.clone(), active.clone());
            std::thread::spawn(move || {
                stream.set_read_timeout(Some(Duration::from_secs(30))).ok();
                serve(&api, &mut stream);
                active.fetch_sub(1, Ordering::SeqCst);
            });
        }
    });
}

fn serve(api: &Api, stream: &mut TcpStream) {
    let (status, headers, body) = match http::read_request(&mut *stream, MAX_BODY) {
        Ok(request) => api.respond(&request),
        Err(problem) => (400, Vec::new(), problem.into_bytes()),
    };
    let headers: Vec<(&str, &str)> = headers.iter().map(|(n, v)| (*n, v.as_str())).collect();
    let _ = http::write_response(stream, status, &headers, &body);
}

/// Answers MCP requests by asking Scroll Studio.
pub struct Api {
    pub host: Arc<dyn Host>,
    pub state: Arc<State>,
    sessions: Mutex<Vec<String>>,
}

type Response = (u16, Vec<(&'static str, String)>, Vec<u8>);

impl Api {
    #[cfg(test)]
    pub fn new(host: Arc<dyn Host>, state: Arc<State>) -> Self {
        Self {
            host,
            state,
            sessions: Mutex::new(Vec::new()),
        }
    }

    pub fn respond(&self, request: &http::Request) -> Response {
        let plain = |status: u16, text: &str| (status, Vec::new(), text.as_bytes().to_vec());
        // A web page in a browser may reach 127.0.0.1 too; only local apps may talk here.
        if let Some(origin) = request.header("origin") {
            if !local_origin(origin) {
                return plain(403, "Origin not allowed");
            }
        }
        if request.path.split('?').next() != Some("/mcp") {
            return plain(404, "Not found");
        }
        let token = request
            .header("authorization")
            .and_then(|v| v.strip_prefix("Bearer "))
            .unwrap_or_default();
        if !same(token.trim(), &self.state.token) {
            return (
                401,
                vec![("WWW-Authenticate", "Bearer".into())],
                b"Use the token shown in AI Lite's settings".to_vec(),
            );
        }
        if request.method != "POST" {
            return (405, vec![("Allow", "POST".into())], Vec::new());
        }
        if let Some(version) = request.header("mcp-protocol-version") {
            if !PROTOCOL_VERSIONS.contains(&version) {
                return plain(400, "Unsupported MCP-Protocol-Version");
            }
        }
        let Ok(message) = serde_json::from_slice::<Value>(&request.body) else {
            return json_body(200, None, error(&Value::Null, -32700, "Not JSON"));
        };
        if !message.is_object() {
            return json_body(
                200,
                None,
                error(&Value::Null, -32600, "Send one message per request"),
            );
        }
        let method = message["method"].as_str();
        let id = message.get("id").filter(|id| !id.is_null()).cloned();
        let (Some(method), Some(id)) = (method, id) else {
            // Notifications and responses need no answer.
            return (202, Vec::new(), Vec::new());
        };
        let session = request.header("mcp-session-id");
        if method == "initialize" {
            let session = new_session();
            {
                let mut sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
                if sessions.len() == MAX_SESSIONS {
                    sessions.remove(0);
                }
                sessions.push(session.clone());
            }
            let asked = message["params"]["protocolVersion"].as_str().unwrap_or("");
            let version = PROTOCOL_VERSIONS
                .iter()
                .find(|v| **v == asked)
                .unwrap_or(&PROTOCOL_VERSIONS[0]);
            let result = json!({
                "protocolVersion": version,
                "capabilities": { "tools": { "listChanged": false } },
                "serverInfo": { "name": "scroll-studio-ai-lite", "title": "Scroll Studio (AI Lite)",
                    "version": env!("CARGO_PKG_VERSION") },
                "instructions": "Tools read the story open in Scroll Studio and propose edits. Each edit is shown to the user, who applies or cancels it; one edit can wait at a time. Use edit_batch to propose several changes as one step. The story itself is ID 0."
            });
            return json_body(200, Some(session), success(&id, result));
        }
        match session {
            None => return plain(400, "Missing Mcp-Session-Id"),
            Some(session)
                if !self
                    .sessions
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .iter()
                    .any(|s| s == session) =>
            {
                return plain(404, "Session not found; initialize again");
            }
            Some(_) => {}
        }
        let answer = match method {
            "ping" => success(&id, json!({})),
            "tools/list" => success(&id, json!({ "tools": self.tools() })),
            "tools/call" => success(&id, self.call_tool(&message["params"])),
            _ => error(&id, -32601, &format!("Unknown method `{method}`")),
        };
        json_body(200, None, answer)
    }

    fn tools(&self) -> Vec<Value> {
        self.state
            .methods
            .get()
            .map(|methods| {
                methods
                    .iter()
                    .filter_map(|m| {
                        let method = m["method"].as_str()?;
                        let edit = method.starts_with("edit.");
                        let mut about = m["about"].as_str().unwrap_or_default().to_owned();
                        if edit {
                            about.push_str(" The user reviews it in Scroll Studio; the result says whether it was applied.");
                        }
                        Some(json!({
                            "name": tool_name(method),
                            "description": about,
                            "inputSchema": m["params"],
                            "annotations": { "readOnlyHint": !edit, "destructiveHint": edit,
                                "openWorldHint": false },
                        }))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Run a tool. Problems are tool errors (`isError`), which the AI app can act on.
    fn call_tool(&self, params: &Value) -> Value {
        let name = params["name"].as_str().unwrap_or_default();
        let Some(method) = self.state.methods.get().and_then(|methods| {
            methods
                .iter()
                .filter_map(|m| m["method"].as_str())
                .find(|m| tool_name(m) == name)
        }) else {
            return tool_error(&format!("Unknown tool `{name}`"));
        };
        let mut arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
        if method == "edit.batch" {
            arguments = arguments.get("commands").cloned().unwrap_or(Value::Null);
        }
        if arguments.as_object().is_some_and(|a| a.is_empty()) && !method.starts_with("edit.") {
            arguments = Value::Null;
        }
        match self.host.call(method, arguments) {
            Ok(result) => json!({
                "content": [{ "type": "text", "text": serde_json::to_string_pretty(&result).unwrap_or_default() }],
                "isError": false,
            }),
            Err(RpcError { code, message }) => tool_error(&match code {
                CANCELLED => "The user cancelled this edit in Scroll Studio".to_owned(),
                BUSY => "Another edit is waiting for the user in Scroll Studio; try again after they decide".to_owned(),
                _ => message,
            }),
        }
    }
}

/// `story.summary` → `story_summary`: names that every MCP client accepts.
pub fn tool_name(method: &str) -> String {
    method.replace('.', "_")
}

fn tool_error(message: &str) -> Value {
    json!({ "content": [{ "type": "text", "text": message }], "isError": true })
}

fn success(id: &Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error(id: &Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn json_body(status: u16, session: Option<String>, body: Value) -> Response {
    let mut headers = vec![("Content-Type", "application/json".to_owned())];
    if let Some(session) = session {
        headers.push(("Mcp-Session-Id", session));
    }
    (status, headers, body.to_string().into_bytes())
}

fn local_origin(origin: &str) -> bool {
    let rest = origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"));
    let Some(rest) = rest else {
        return false;
    };
    let host = match rest.strip_prefix('[') {
        Some(v6) => v6.split(']').next().unwrap_or_default(),
        None => rest.split(':').next().unwrap_or_default(),
    };
    matches!(host, "localhost" | "127.0.0.1" | "::1")
}

/// Compare without stopping at the first difference.
fn same(given: &str, expected: &str) -> bool {
    given.len() == expected.len()
        && given
            .bytes()
            .zip(expected.bytes())
            .fold(0u8, |diff, (a, b)| diff | (a ^ b))
            == 0
}

fn new_session() -> String {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).expect("the system has no random numbers");
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;
    use std::collections::HashSet;

    /// Scroll Studio stand-in: reads answer, edits wait for a pretend user.
    struct FakeHost {
        calls: Mutex<Vec<(String, Value)>>,
        decision: Result<Value, RpcError>,
    }

    impl Host for FakeHost {
        fn call(&self, method: &str, params: Value) -> Result<Value, RpcError> {
            self.calls.lock().unwrap().push((method.into(), params));
            if method.starts_with("edit.") {
                self.decision.clone()
            } else {
                Ok(json!({ "title": "After the rain" }))
            }
        }
    }

    fn api(decision: Result<Value, RpcError>) -> (Api, Arc<FakeHost>) {
        let state = Arc::new(State {
            settings: Mutex::new(Settings::default()),
            token: "secret".into(),
            methods: OnceLock::new(),
            cancelled: Mutex::new(HashSet::new()),
            mcp: Mutex::new(Status::Off),
        });
        let _ = state.methods.set(vec![
            json!({ "method": "story.summary", "about": "Summary.", "params": { "type": "object", "properties": {} } }),
            json!({ "method": "edit.set_field", "about": "Set a field.", "params": { "type": "object" } }),
            json!({ "method": "edit.batch", "about": "Batch.", "params": { "type": "object" } }),
        ]);
        let host = Arc::new(FakeHost {
            calls: Mutex::new(Vec::new()),
            decision,
        });
        (Api::new(host.clone(), state), host)
    }

    fn post(api: &Api, session: Option<&str>, body: Value) -> (u16, Value, Option<String>) {
        let mut headers = vec![
            ("authorization".to_owned(), "Bearer secret".to_owned()),
            ("content-type".to_owned(), "application/json".to_owned()),
        ];
        if let Some(session) = session {
            headers.push(("mcp-session-id".to_owned(), session.to_owned()));
        }
        let (status, headers, body) = api.respond(&http::Request {
            method: "POST".into(),
            path: "/mcp".into(),
            headers,
            body: body.to_string().into_bytes(),
        });
        let session = headers
            .iter()
            .find(|(n, _)| *n == "Mcp-Session-Id")
            .map(|(_, v)| v.clone());
        (
            status,
            serde_json::from_slice(&body).unwrap_or(Value::Null),
            session,
        )
    }

    fn start(api: &Api) -> String {
        let (status, answer, session) = post(
            api,
            None,
            json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize",
                "params": { "protocolVersion": "2025-06-18", "capabilities": {},
                    "clientInfo": { "name": "test", "version": "1" } } }),
        );
        assert_eq!(status, 200);
        assert_eq!(answer["result"]["protocolVersion"], "2025-06-18");
        assert!(answer["result"]["capabilities"]["tools"].is_object());
        let session = session.expect("a session id");
        let (status, _, _) = post(
            api,
            Some(&session),
            json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
        );
        assert_eq!(status, 202);
        session
    }

    #[test]
    fn tools_mirror_the_api_and_reads_answer() {
        let (api, host) = api(Ok(json!({ "applied": true })));
        let session = start(&api);
        let (_, listed, _) = post(
            &api,
            Some(&session),
            json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
        );
        let tools = listed["result"]["tools"].as_array().unwrap();
        let names: Vec<_> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["story_summary", "edit_set_field", "edit_batch"]);
        assert_eq!(tools[0]["annotations"]["readOnlyHint"], true);
        let (_, called, _) = post(
            &api,
            Some(&session),
            json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call",
                "params": { "name": "story_summary", "arguments": {} } }),
        );
        assert_eq!(called["result"]["isError"], false);
        assert!(called["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("After the rain"));
        // Batches are wrapped for tools and unwrapped for Scroll Studio.
        post(
            &api,
            Some(&session),
            json!({ "jsonrpc": "2.0", "id": 4, "method": "tools/call",
                "params": { "name": "edit_batch", "arguments": { "commands": [] } } }),
        );
        let calls = host.calls.lock().unwrap();
        assert_eq!(calls[0], ("story.summary".into(), Value::Null));
        assert_eq!(calls[1], ("edit.batch".into(), json!([])));
    }

    #[test]
    fn a_cancelled_edit_is_a_tool_error() {
        let (api, _) = api(Err(RpcError::new(CANCELLED, "Cancelled")));
        let session = start(&api);
        let (_, called, _) = post(
            &api,
            Some(&session),
            json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call",
                "params": { "name": "edit_set_field", "arguments": { "target_id": 0, "field": "title", "value": "x" } } }),
        );
        assert_eq!(called["result"]["isError"], true);
        assert!(called["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("cancelled"));
    }

    #[test]
    fn strangers_web_pages_and_unknown_sessions_are_refused() {
        let (api, _) = api(Ok(json!({})));
        let request = |headers: Vec<(&str, &str)>, method: &str, path: &str| http::Request {
            method: method.into(),
            path: path.into(),
            headers: headers
                .into_iter()
                .map(|(n, v)| (n.to_owned(), v.to_owned()))
                .collect(),
            body: br#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#.to_vec(),
        };
        let status = |r: http::Request| api.respond(&r).0;
        assert_eq!(status(request(vec![], "POST", "/mcp")), 401);
        assert_eq!(
            status(request(
                vec![("authorization", "Bearer wrong!")],
                "POST",
                "/mcp"
            )),
            401
        );
        let auth = ("authorization", "Bearer secret");
        assert_eq!(
            status(request(
                vec![auth, ("origin", "https://evil.example")],
                "POST",
                "/mcp"
            )),
            403
        );
        assert_eq!(status(request(vec![auth], "GET", "/mcp")), 405);
        assert_eq!(status(request(vec![auth], "POST", "/other")), 404);
        assert_eq!(
            status(request(vec![auth], "POST", "/mcp")),
            400,
            "no session"
        );
        assert_eq!(
            status(request(
                vec![auth, ("mcp-session-id", "made-up")],
                "POST",
                "/mcp"
            )),
            404
        );
        assert_eq!(
            status(request(
                vec![auth, ("mcp-protocol-version", "1999-01-01")],
                "POST",
                "/mcp"
            )),
            400
        );
        assert!(local_origin("http://localhost:6274"));
        assert!(local_origin("http://[::1]:80"));
        assert!(!local_origin("http://127.0.0.1.evil.example"));
        assert!(!local_origin("null"));
    }

    #[test]
    fn unknown_protocol_versions_get_the_newest() {
        let (api, _) = api(Ok(json!({})));
        let (_, answer, _) = post(
            &api,
            None,
            json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize",
                "params": { "protocolVersion": "2030-01-01" } }),
        );
        assert_eq!(answer["result"]["protocolVersion"], PROTOCOL_VERSIONS[0]);
    }
}
