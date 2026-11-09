//! Loopback client for pin `498d752` of the private publishing protocol.
//!
//! A stand-in in the unit tests speaks the same HTTP shapes. It is not the
//! platform service, and a passing test is not a published episode.

use crate::publish::{EpisodeKind, EpisodePackage, PackageMember};
use getrandom::fill as random_fill;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    time::Duration,
};

pub const ORIGIN_HOST: &str = "127.0.0.1";
pub const PINNED_PORT: u16 = 19427;
pub const COMMAND_PATH: &str = "/ss-publish/commands";
pub const AUTHORIZE_PATH: &str = "/ss-publish/browser/authorize";
pub const CALLBACK_PATH: &str = "/ss-publish/callback";
pub const MAX_COMMAND_BYTES: usize = 28_000_000;
const PROTOCOL: &str = "scroll-studio.publishing.private-prototype.v1";
const CLIENT_ID: &str = "scroll-studio-desktop-test";
const ISSUER: &str = "http://127.0.0.1:19427/ss-publish/test-issuer";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Origin {
    pub port: u16,
}

impl Origin {
    pub fn pinned() -> Self {
        Self { port: PINNED_PORT }
    }

    pub fn base(&self) -> String {
        format!("http://{ORIGIN_HOST}:{}", self.port)
    }

    fn host_header(&self) -> String {
        format!("{ORIGIN_HOST}:{}", self.port)
    }

    fn socket(&self) -> SocketAddr {
        SocketAddr::from((std::net::Ipv4Addr::LOCALHOST, self.port))
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct Credential {
    id: String,
    secret: String,
}

impl Credential {
    pub fn id(&self) -> &str {
        &self.id
    }
}

impl std::fmt::Debug for Credential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Credential")
            .field("id", &self.id)
            .field("secret", &"[redacted]")
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnedEpisode {
    pub series_id: String,
    pub episode_id: String,
    pub series_title: String,
    pub episode_title: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountSnapshot {
    pub account_id: String,
    pub handle: String,
    pub episodes: Vec<OwnedEpisode>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Acknowledgement {
    pub effect: String,
    pub revision_id: String,
    pub browser_preview_url: Option<String>,
    pub canonical_url: Option<String>,
    pub publication_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LiveStatus {
    NotRead,
    Discover {
        status: String,
        canonical_url: Option<String>,
    },
    Read {
        status: String,
        revision_id: String,
        canonical_url: String,
    },
}

#[derive(Debug)]
pub enum WireError {
    NotConnected,
    Cancelled,
    Rejected { code: String, status: u16 },
    Protocol(String),
}

impl std::fmt::Display for WireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotConnected => write!(
                f,
                "Private prototype service is not connected. Nothing was uploaded."
            ),
            Self::Cancelled => write!(f, "Sign-in was cancelled. Nothing was uploaded."),
            Self::Rejected { code, status } => {
                write!(
                    f,
                    "The service refused the command ({code}, HTTP {status})."
                )
            }
            Self::Protocol(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for WireError {}

pub fn sign_in(
    origin: Origin,
    open_browser: impl FnOnce(&str) -> Result<(), WireError>,
) -> Result<(Credential, AccountSnapshot), WireError> {
    let listener = TcpListener::bind((ORIGIN_HOST, 0)).map_err(|error| {
        WireError::Protocol(format!(
            "Could not listen for the sign-in callback: {error}"
        ))
    })?;
    listener
        .set_nonblocking(false)
        .map_err(|error| WireError::Protocol(error.to_string()))?;
    let port = listener
        .local_addr()
        .map_err(|error| WireError::Protocol(error.to_string()))?
        .port();
    if port == PINNED_PORT {
        return Err(WireError::Protocol(
            "The sign-in callback landed on the service port.".into(),
        ));
    }
    let verifier = random_token(64)?;
    let challenge = pkce_s256(&verifier);
    let state = random_token(48)?;
    let callback_state = state.clone();
    let redirect = format!("http://{ORIGIN_HOST}:{port}{CALLBACK_PATH}");
    let begin = post(
        origin,
        &envelope(
            "auth.begin",
            &random_token(32)?,
            None,
            json!({
                "transportProfile": "local-http-isolated-fixture",
                "issuer": ISSUER,
                "redirectUri": redirect,
                "codeChallengeMethod": "S256",
                "codeChallenge": challenge,
                "state": state,
                "clientId": CLIENT_ID,
                "scope": ["prototype:creator"]
            }),
        ),
    )?;
    let url = begin["result"]["authorizationUrl"]
        .as_str()
        .ok_or_else(|| WireError::Protocol("Sign-in did not return an authorization URL.".into()))?
        .to_string();
    validate_browser_url(origin, &url)?;
    if url.contains("secret=") || url.contains("sek_live_") || url.contains("loginHint") {
        return Err(WireError::Protocol(
            "The authorization URL is not safe to open.".into(),
        ));
    }
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(accept_callback(listener, &callback_state));
    });
    open_browser(&url)?;
    let (code, returned_state) = rx
        .recv_timeout(Duration::from_secs(120))
        .map_err(|_| WireError::Cancelled)??;
    if returned_state != state {
        return Err(WireError::Protocol(
            "The sign-in callback state did not match.".into(),
        ));
    }
    let exchanged = post(
        origin,
        &envelope(
            "auth.exchange",
            &random_token(32)?,
            None,
            json!({
                "code": code,
                "codeVerifier": verifier,
                "redirectUri": redirect,
                "state": state,
                "issuer": ISSUER,
                "grantType": "authorization_code"
            }),
        ),
    )?;
    let credential = credential_from(&exchanged["result"]["credential"])?;
    let account = account_from(&exchanged["result"]["account"])?;
    Ok((credential, account))
}

pub fn discover(origin: Origin, credential: &Credential) -> Result<AccountSnapshot, WireError> {
    let response = post(
        origin,
        &envelope(
            "account.discover",
            &random_token(32)?,
            Some(credential),
            json!({}),
        ),
    )?;
    account_from(&response["result"]["account"])
}

pub fn upload(
    origin: Origin,
    credential: &Credential,
    package: &EpisodePackage,
    owned: &OwnedEpisode,
    story_ref: &str,
    episode_ref: &str,
) -> Result<Acknowledgement, WireError> {
    if package.metadata().series_title() != owned.series_title
        || package.metadata().episode_title() != owned.episode_title
    {
        return Err(WireError::Protocol(
            "The reviewed titles do not match the owned episode. Nothing was uploaded.".into(),
        ));
    }
    let content = match package.kind() {
        EpisodeKind::Comic => json!({
            "kind": "comic",
            "slices": package.members().iter().enumerate().map(|(index, member)| slice_json(index, member)).collect::<Vec<_>>()
        }),
        EpisodeKind::Prose => json!({
            "kind": "prose",
            "mediaType": "text/plain; charset=utf-8",
            "text": prose_text(package)?
        }),
    };
    let response = post(
        origin,
        &envelope(
            "episode.upload",
            &random_token(32)?,
            Some(credential),
            json!({
                "seriesId": owned.series_id,
                "episodeId": owned.episode_id,
                "selection": {
                    "mode": "selected-episode-only",
                    "localStoryRef": story_ref,
                    "localEpisodeRef": episode_ref
                },
                "publicMetadata": {
                    "seriesTitle": package.metadata().series_title(),
                    "episodeTitle": package.metadata().episode_title(),
                    "synopsis": package.metadata().synopsis(),
                    "language": package.metadata().language()
                },
                "content": content,
                "completion": "seal",
                "expectedSha256": package.content_sha256_hex()
            }),
        ),
    )?;
    acknowledgement(&response["result"])
}

pub fn confirm(
    origin: Origin,
    credential: &Credential,
    owned: &OwnedEpisode,
    revision_id: &str,
) -> Result<Acknowledgement, WireError> {
    let response = post(
        origin,
        &envelope(
            "publication.confirm",
            &random_token(32)?,
            Some(credential),
            json!({
                "seriesId": owned.series_id,
                "episodeId": owned.episode_id,
                "revisionId": revision_id,
                "policyEligibility": "private-prototype-only"
            }),
        ),
    )?;
    acknowledgement(&response["result"])
}

pub fn withdraw(
    origin: Origin,
    credential: &Credential,
    owned: &OwnedEpisode,
    publication_id: &str,
    revision_id: &str,
) -> Result<Acknowledgement, WireError> {
    let response = post(
        origin,
        &envelope(
            "publication.withdraw",
            &random_token(32)?,
            Some(credential),
            json!({
                "seriesId": owned.series_id,
                "episodeId": owned.episode_id,
                "publicationId": publication_id,
                "revisionId": revision_id
            }),
        ),
    )?;
    acknowledgement(&response["result"])
}

pub fn read_publication(
    origin: Origin,
    credential: &Credential,
    canonical_url: &str,
) -> Result<LiveStatus, WireError> {
    validate_browser_url(origin, canonical_url)?;
    let response = post(
        origin,
        &envelope(
            "publication.read",
            &random_token(32)?,
            Some(credential),
            json!({ "canonicalUrl": canonical_url }),
        ),
    )?;
    let result = &response["result"];
    if result["authoritative"] != true {
        return Err(WireError::Protocol(
            "publication.read did not return an authoritative status.".into(),
        ));
    }
    Ok(LiveStatus::Read {
        status: result["status"].as_str().unwrap_or("published").to_string(),
        revision_id: result["revisionId"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        canonical_url: result["canonicalUrl"]
            .as_str()
            .unwrap_or(canonical_url)
            .to_string(),
    })
}

pub fn open_system_browser(url: &str) -> Result<(), WireError> {
    validate_browser_url(Origin::pinned(), url)?;
    if url.contains("sek_live_") || url.contains("secret=") || url.contains("code_verifier=") {
        return Err(WireError::Protocol(
            "Refusing to open a URL that carries a credential.".into(),
        ));
    }
    let mut command = browser_command(url);
    command.spawn().map_err(|error| {
        WireError::Protocol(format!("Could not open the system browser: {error}"))
    })?;
    Ok(())
}

fn browser_command(url: &str) -> std::process::Command {
    #[cfg(target_os = "windows")]
    {
        let mut command = std::process::Command::new("cmd");
        command.args(["/C", "start", "", url]);
        return command;
    }
    #[cfg(target_os = "macos")]
    {
        let mut command = std::process::Command::new("open");
        command.arg(url);
        return command;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(url);
        command
    }
}

fn envelope(command: &str, key: &str, credential: Option<&Credential>, body: Value) -> Value {
    let mut value = json!({
        "protocol": PROTOCOL,
        "command": command,
        "idempotencyKey": key,
        "body": body
    });
    if let Some(credential) = credential {
        value["credential"] = json!({
            "credentialId": credential.id,
            "secret": credential.secret
        });
    }
    value
}

fn post(origin: Origin, payload: &Value) -> Result<Value, WireError> {
    let bytes = serde_json::to_vec(payload)
        .map_err(|error| WireError::Protocol(format!("Could not encode the command: {error}")))?;
    if bytes.len() > MAX_COMMAND_BYTES {
        return Err(WireError::Protocol(
            "The command is over the 28000000 byte limit. Nothing was uploaded.".into(),
        ));
    }
    if payload.get("fixture").is_some() || payload["body"].get("loginHint").is_some() {
        return Err(WireError::Protocol(
            "Fixture clock, pause, and test redirect hooks are refused.".into(),
        ));
    }
    let mut stream = TcpStream::connect_timeout(&origin.socket(), Duration::from_secs(3))
        .map_err(|_| WireError::NotConnected)?;
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .map_err(|error| WireError::Protocol(error.to_string()))?;
    let request = format!(
        "POST {COMMAND_PATH} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        origin.host_header(),
        bytes.len()
    );
    stream
        .write_all(request.as_bytes())
        .and_then(|_| stream.write_all(&bytes))
        .map_err(|_| WireError::NotConnected)?;
    let HttpResponse {
        status,
        headers,
        body,
    } = read_response(&mut stream)?;
    if !headers.iter().any(|(name, value)| {
        name.eq_ignore_ascii_case("content-type") && value.starts_with("application/json")
    }) || !headers.iter().any(|(name, value)| {
        name.eq_ignore_ascii_case("cache-control")
            && value.to_ascii_lowercase().contains("no-store")
    }) {
        return Err(WireError::Protocol(
            "The service response was not a no-store JSON command.".into(),
        ));
    }
    let value: Value = serde_json::from_slice(&body)
        .map_err(|_| WireError::Protocol("The service response was not JSON.".into()))?;
    if value["ok"] == true {
        if status != 200 {
            return Err(WireError::Protocol(
                "A successful command did not use HTTP 200.".into(),
            ));
        }
        return Ok(value);
    }
    let code = value["error"]["code"].as_str().unwrap_or("invalid_request");
    let expected = error_status(code);
    if status != expected {
        return Err(WireError::Protocol(format!(
            "HTTP {status} does not match the status for {code}."
        )));
    }
    Err(WireError::Rejected {
        code: code.to_string(),
        status,
    })
}

fn error_status(code: &str) -> u16 {
    match code {
        "invalid_request" | "state_mismatch" | "redirect_rejected" | "issuer_rejected"
        | "pkce_rejected" | "unsupported_grant" | "media_rejected" | "metadata_rejected" => 400,
        "unauthorized"
        | "credential_expired"
        | "credential_revoked"
        | "code_replayed"
        | "authorization_expired"
        | "authorization_cancelled"
        | "exchange_consumed" => 401,
        "not_owner" | "public_launch_gate" | "production_transport_not_enabled" => 403,
        "not_found" | "unknown_command" | "asset_not_addressable" => 404,
        "stale_revision" | "idempotency_conflict" | "cancelled" | "already_terminal" => 409,
        "withdrawn" => 410,
        "limit_exceeded" => 413,
        _ => 400,
    }
}

fn acknowledgement(result: &Value) -> Result<Acknowledgement, WireError> {
    let ack = &result["acknowledgement"];
    if ack["authoritative"] != false {
        return Err(WireError::Protocol(
            "An upload, confirm, or withdraw result is not the current publication status.".into(),
        ));
    }
    if result.get("status").is_some() {
        return Err(WireError::Protocol(
            "The acknowledgement included a publication status.".into(),
        ));
    }
    Ok(Acknowledgement {
        effect: ack["effect"].as_str().unwrap_or_default().to_string(),
        revision_id: ack["revisionId"].as_str().unwrap_or_default().to_string(),
        browser_preview_url: ack["browserPreviewUrl"].as_str().map(str::to_string),
        canonical_url: ack["canonicalUrl"].as_str().map(str::to_string),
        publication_id: ack["publicationId"].as_str().map(str::to_string),
    })
}

fn credential_from(value: &Value) -> Result<Credential, WireError> {
    let id = value["credentialId"].as_str().unwrap_or("");
    let secret = value["secret"].as_str().unwrap_or("");
    if !is_match(id, "cred_live_", 32) || !is_match(secret, "sek_live_", 64) {
        return Err(WireError::Protocol(
            "The credential does not match the live credential shape.".into(),
        ));
    }
    if value["delivery"] != "service-channel-only" || value["testOnly"] != true {
        return Err(WireError::Protocol(
            "The credential is not a test-only service-channel credential.".into(),
        ));
    }
    Ok(Credential {
        id: id.to_string(),
        secret: secret.to_string(),
    })
}

fn account_from(value: &Value) -> Result<AccountSnapshot, WireError> {
    if value["testOnly"] != true || value["identityProof"] != "browser-fixture-affirmation" {
        return Err(WireError::Protocol(
            "The account is not a browser fixture affirmation.".into(),
        ));
    }
    let mut episodes = Vec::new();
    for series in value["series"].as_array().unwrap_or(&Vec::new()) {
        let series_id = series["seriesId"].as_str().unwrap_or("").to_string();
        let series_title = series["title"].as_str().unwrap_or("").to_string();
        for episode in series["episodes"].as_array().unwrap_or(&Vec::new()) {
            let publication = &episode["publication"];
            if publication["authoritative"] == true {
                // Discover is allowed to carry current status. Callers read it
                // from this snapshot only when they ask for status.
            }
            episodes.push(OwnedEpisode {
                series_id: series_id.clone(),
                episode_id: episode["episodeId"].as_str().unwrap_or("").to_string(),
                series_title: series_title.clone(),
                episode_title: episode["title"].as_str().unwrap_or("").to_string(),
            });
        }
    }
    Ok(AccountSnapshot {
        account_id: value["accountId"].as_str().unwrap_or("").to_string(),
        handle: value["handle"].as_str().unwrap_or("").to_string(),
        episodes,
    })
}

fn slice_json(index: usize, member: &PackageMember) -> Value {
    json!({
        "ordinal": index + 1,
        "mediaType": member.media_type(),
        "width": member.width().unwrap_or(0),
        "height": member.height().unwrap_or(0),
        "byteLength": member.byte_len(),
        "sha256": member.sha256_hex(),
        "bytesBase64": base64_encode(member.bytes())
    })
}

fn prose_text(package: &EpisodePackage) -> Result<String, WireError> {
    let bytes = package
        .members()
        .first()
        .map(|member| member.bytes())
        .unwrap_or_default();
    if bytes.len() > 65_536 {
        return Err(WireError::Protocol(
            "Prose is over the 65536 byte limit.".into(),
        ));
    }
    String::from_utf8(bytes.to_vec()).map_err(|_| WireError::Protocol("Prose is not UTF-8.".into()))
}

fn validate_browser_url(origin: Origin, url: &str) -> Result<(), WireError> {
    let prefix = format!("{}/ss-publish/browser/", origin.base());
    if !url.starts_with(&prefix) {
        return Err(WireError::Protocol(
            "The browser URL is not on the prototype origin.".into(),
        ));
    }
    let query = url.split_once('?').map(|(_, query)| query).unwrap_or("");
    for key in [
        "access_token",
        "refresh_token",
        "id_token",
        "token",
        "bearer",
        "secret",
        "code_verifier",
    ] {
        if query
            .split('&')
            .any(|pair| pair.starts_with(&format!("{key}=")) || pair == key)
        {
            return Err(WireError::Protocol(
                "The browser URL carries a forbidden parameter.".into(),
            ));
        }
    }
    Ok(())
}

fn accept_callback(
    listener: TcpListener,
    expected_state: &str,
) -> Result<(String, String), WireError> {
    listener
        .set_nonblocking(false)
        .map_err(|error| WireError::Protocol(error.to_string()))?;
    let (mut stream, _) = listener.accept().map_err(|_| WireError::Cancelled)?;
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut buffer = vec![0_u8; 4096];
    let read = stream.read(&mut buffer).unwrap_or(0);
    let text = String::from_utf8_lossy(&buffer[..read]);
    let request = text.lines().next().unwrap_or("");
    let path = request.split_whitespace().nth(1).unwrap_or("");
    let (route, query) = path.split_once('?').unwrap_or((path, ""));
    if route != CALLBACK_PATH {
        return Err(WireError::Protocol(
            "The sign-in callback used an unexpected path.".into(),
        ));
    }
    let mut code = None;
    let mut state = None;
    for pair in query.split('&').filter(|pair| !pair.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        if matches!(
            key,
            "access_token"
                | "refresh_token"
                | "id_token"
                | "token"
                | "bearer"
                | "secret"
                | "code_verifier"
        ) {
            return Err(WireError::Protocol(
                "The sign-in callback carried a forbidden parameter.".into(),
            ));
        }
        match key {
            "code" => code = Some(value.to_string()),
            "state" => state = Some(value.to_string()),
            _ => {
                return Err(WireError::Protocol(
                    "The sign-in callback carried an unexpected parameter.".into(),
                ))
            }
        }
    }
    let body = b"<html><body>Signed in. You can close this window.</body></html>";
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.write_all(body);
    let state = state.unwrap_or_default();
    if state != expected_state {
        return Err(WireError::Protocol(
            "The sign-in callback state did not match.".into(),
        ));
    }
    Ok((code.unwrap_or_default(), state))
}

struct HttpResponse {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

fn read_response(stream: &mut TcpStream) -> Result<HttpResponse, WireError> {
    let mut raw = Vec::new();
    let mut chunk = [0_u8; 8192];
    loop {
        let read = stream.read(&mut chunk).unwrap_or(0);
        if read == 0 {
            break;
        }
        raw.extend_from_slice(&chunk[..read]);
        if raw.len() > MAX_COMMAND_BYTES {
            return Err(WireError::Protocol(
                "The service response is over the body limit.".into(),
            ));
        }
    }
    let split = raw
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| WireError::Protocol("The service response had no header.".into()))?;
    let header = String::from_utf8_lossy(&raw[..split]).into_owned();
    let body = raw[split + 4..].to_vec();
    let mut lines = header.lines();
    let status_line = lines.next().unwrap_or("");
    let status: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| WireError::Protocol("The service status line was unreadable.".into()))?;
    let headers = lines
        .filter_map(|line| {
            let (name, value) = line.split_once(':')?;
            Some((name.trim().to_string(), value.trim().to_string()))
        })
        .collect();
    Ok(HttpResponse {
        status,
        headers,
        body,
    })
}

fn pkce_s256(verifier: &str) -> String {
    base64url(&Sha256::digest(verifier.as_bytes()))
}

fn random_token(len: usize) -> Result<String, WireError> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut bytes = vec![0_u8; len];
    random_fill(&mut bytes).map_err(|error| WireError::Protocol(error.to_string()))?;
    Ok(bytes
        .into_iter()
        .map(|byte| ALPHABET[(byte as usize) % ALPHABET.len()] as char)
        .collect())
}

fn is_match(value: &str, prefix: &str, hex_len: usize) -> bool {
    let Some(rest) = value.strip_prefix(prefix) else {
        return false;
    };
    rest.len() == hex_len
        && rest
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let mut buf = [0_u8; 3];
        buf[..chunk.len()].copy_from_slice(chunk);
        let n = (u32::from(buf[0]) << 16) | (u32::from(buf[1]) << 8) | u32::from(buf[2]);
        out.push(TABLE[((n >> 18) & 63) as usize] as char);
        out.push(TABLE[((n >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[((n >> 6) & 63) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(n & 63) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

fn base64url(bytes: &[u8]) -> String {
    base64_encode(bytes)
        .trim_end_matches('=')
        .replace('+', "-")
        .replace('/', "_")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::publish::PublicMetadata;
    use std::io::Write;
    use std::sync::{Arc, Mutex};

    #[test]
    fn pinned_origin_is_the_single_loopback_port() {
        assert_eq!(Origin::pinned().port, 19427);
        assert_eq!(Origin::pinned().base(), "http://127.0.0.1:19427");
        assert_eq!(COMMAND_PATH, "/ss-publish/commands");
        assert!(AUTHORIZE_PATH.starts_with("/ss-publish/browser/"));
    }

    #[test]
    fn prose_over_the_byte_cap_is_rejected_before_a_character_cap() {
        let text = "\u{1F600}".repeat(20_000);
        assert!(text.chars().count() < 65_536);
        assert!(text.len() > 65_536);
        let metadata = PublicMetadata::new(
            "North line",
            "Platform",
            "A reviewed public synopsis.",
            "en",
        )
        .unwrap();
        assert!(crate::publish::package_prose_bytes(text.as_bytes(), metadata).is_err());
    }

    #[test]
    fn client_signs_in_uploads_and_does_not_trust_the_acknowledgement() {
        let stand = StandIn::start();
        let origin = stand.origin();
        let (credential, account) = sign_in(origin, |url| {
            assert!(url.starts_with(&format!("{}/ss-publish/browser/authorize?", origin.base())));
            assert!(!url.contains("sek_live_"));
            assert!(!url.contains("loginHint"));
            stand.complete_browser(url);
            Ok(())
        })
        .unwrap();
        assert!(format!("{credential:?}").contains("[redacted]"));
        assert!(!format!("{credential:?}").contains("sek_live_"));
        let owned = account.episodes[0].clone();
        let metadata = PublicMetadata::new(
            &owned.series_title,
            &owned.episode_title,
            "A reviewed public synopsis.",
            "en",
        )
        .unwrap();
        let package = crate::publish::package_prose_bytes(b"Public line.", metadata).unwrap();
        let ack = upload(
            origin,
            &credential,
            &package,
            &owned,
            "story-1",
            "episode-1",
        )
        .unwrap();
        assert_eq!(ack.effect, "revision-sealed");
        assert!(ack
            .browser_preview_url
            .unwrap()
            .starts_with(&format!("{}/ss-publish/browser/preview/", origin.base())));
        let confirmed = confirm(origin, &credential, &owned, &ack.revision_id).unwrap();
        assert!(confirmed
            .canonical_url
            .as_ref()
            .unwrap()
            .contains("/ss-publish/browser/reader/"));
        let status = read_publication(
            origin,
            &credential,
            confirmed.canonical_url.as_deref().unwrap(),
        )
        .unwrap();
        assert!(matches!(status, LiveStatus::Read { ref status, .. } if status == "published"));
        withdraw(
            origin,
            &credential,
            &owned,
            confirmed.publication_id.as_deref().unwrap(),
            &confirmed.revision_id,
        )
        .unwrap();
        let error = read_publication(
            origin,
            &credential,
            confirmed.canonical_url.as_deref().unwrap(),
        )
        .unwrap_err();
        assert!(
            matches!(error, WireError::Rejected { ref code, status: 410 } if code == "withdrawn")
        );
        assert!(!stand.saw_fixture());
    }

    struct StandIn {
        origin: Origin,
        state: Arc<Mutex<StandState>>,
    }

    struct StandState {
        verifier_challenge: Option<String>,
        redirect: Option<String>,
        state: Option<String>,
        code: Option<String>,
        exchanged: bool,
        revision: String,
        publication: String,
        withdrawn: bool,
        saw_fixture: bool,
    }

    impl StandIn {
        fn start() -> Self {
            let listener = TcpListener::bind((ORIGIN_HOST, 0)).unwrap();
            let port = listener.local_addr().unwrap().port();
            let state = Arc::new(Mutex::new(StandState {
                verifier_challenge: None,
                redirect: None,
                state: None,
                code: None,
                exchanged: false,
                revision: "rev_test_0001".into(),
                publication: "pub_test_0001".into(),
                withdrawn: false,
                saw_fixture: false,
            }));
            let shared = Arc::clone(&state);
            std::thread::spawn(move || serve(listener, shared));
            Self {
                origin: Origin { port },
                state,
            }
        }

        fn origin(&self) -> Origin {
            self.origin
        }

        fn saw_fixture(&self) -> bool {
            self.state.lock().unwrap().saw_fixture
        }

        fn complete_browser(&self, authorization_url: &str) {
            let query = authorization_url.split_once('?').unwrap().1;
            let mut redirect = None;
            let mut state = None;
            for _ in 0..50 {
                let guard = self.state.lock().unwrap();
                if guard.redirect.is_some() {
                    redirect = guard.redirect.clone();
                    state = guard.state.clone();
                    break;
                }
                drop(guard);
                std::thread::sleep(Duration::from_millis(10));
            }
            let redirect = redirect.expect("begin");
            let state = state.unwrap();
            assert!(query.contains(&format!("state={state}")));
            let code = "code-live-1";
            self.state.lock().unwrap().code = Some(code.into());
            let host = redirect
                .trim_start_matches("http://")
                .split('/')
                .next()
                .unwrap();
            let mut stream = TcpStream::connect(host).unwrap();
            let path = format!("{CALLBACK_PATH}?code={code}&state={state}");
            let request =
                format!("GET {path} HTTP/1.1\r\nHost: {ORIGIN_HOST}\r\nConnection: close\r\n\r\n");
            stream.write_all(request.as_bytes()).unwrap();
        }
    }

    fn serve(listener: TcpListener, state: Arc<Mutex<StandState>>) {
        for _ in 0..8 {
            let Ok((mut stream, _)) = listener.accept() else {
                break;
            };
            let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
            let mut raw = Vec::new();
            let mut chunk = [0_u8; 8192];
            while let Ok(read) = stream.read(&mut chunk) {
                if read == 0 {
                    break;
                }
                raw.extend_from_slice(&chunk[..read]);
                if raw.windows(4).any(|window| window == b"\r\n\r\n") && content_ready(&raw) {
                    break;
                }
            }
            let text = String::from_utf8_lossy(&raw);
            if text.contains("\"fixture\"")
                || text.contains("loginHint")
                || text.contains("testOnlyRedirect")
            {
                state.lock().unwrap().saw_fixture = true;
            }
            let body = text.split("\r\n\r\n").nth(1).unwrap_or("");
            let value: Value = serde_json::from_str(body).unwrap_or(json!({}));
            let command = value["command"].as_str().unwrap_or("");
            let response = match command {
                "auth.begin" => {
                    let mut guard = state.lock().unwrap();
                    guard.verifier_challenge =
                        value["body"]["codeChallenge"].as_str().map(str::to_string);
                    guard.redirect = value["body"]["redirectUri"].as_str().map(str::to_string);
                    guard.state = value["body"]["state"].as_str().map(str::to_string);
                    let url = format!(
                        "http://{ORIGIN_HOST}:{}/ss-publish/browser/authorize?response_type=code&client_id={CLIENT_ID}&redirect_uri={}&scope=prototype:creator&state={}&code_challenge={}&code_challenge_method=S256",
                        listener_port(&listener),
                        urlencoding(&guard.redirect.clone().unwrap_or_default()),
                        guard.state.clone().unwrap_or_default(),
                        guard.verifier_challenge.clone().unwrap_or_default()
                    );
                    ok(
                        "auth.begin",
                        json!({"authorizationUrl": url, "expiresAt": "2026-11-09T18:02:00Z", "issuer": ISSUER, "issuerRole": "prototype-test-issuer", "distinctFromProductionProvider": true, "identityProof": "browser-fixture-affirmation", "state": guard.state.clone().unwrap_or_default(), "clientId": CLIENT_ID}),
                    )
                }
                "auth.exchange" => {
                    let mut guard = state.lock().unwrap();
                    if guard.exchanged {
                        err("exchange_consumed")
                    } else {
                        guard.exchanged = true;
                        let secret = "sek_live_".to_string() + &"ab".repeat(32);
                        ok(
                            "auth.exchange",
                            json!({"credential": {"credentialId": format!("cred_live_{}", "cd".repeat(16)), "secret": secret, "tokenType": "prototype-credential", "scope": ["prototype:creator"], "accountId": "acct_test_owner_a", "expiresAt": "2026-11-09T19:00:00Z", "testOnly": true, "nonProductionLabel": "TEST-ONLY", "issuerRole": "prototype-test-issuer", "distinctFromProductionProvider": true, "delivery": "service-channel-only"}, "account": sample_account()}),
                        )
                    }
                }
                "account.discover" => ok("account.discover", json!({"account": sample_account()})),
                "episode.upload" => {
                    let guard = state.lock().unwrap();
                    ok(
                        "episode.upload",
                        json!({"acknowledgement": {"accepted": true, "effect": "revision-sealed", "authoritative": false, "private": true, "localOriginals": "untouched", "seriesId": "ser_test_north", "episodeId": "ep_test_platform", "revisionId": guard.revision, "contentSha256": value["body"]["expectedSha256"], "browserPreviewUrl": format!("http://{ORIGIN_HOST}:{}/ss-publish/browser/preview/ser_test_north/ep_test_platform/{}", listener_port(&listener), guard.revision)}}),
                    )
                }
                "publication.confirm" => {
                    let guard = state.lock().unwrap();
                    ok(
                        "publication.confirm",
                        json!({"acknowledgement": {"accepted": true, "effect": "confirm-recorded", "authoritative": false, "seriesId": "ser_test_north", "episodeId": "ep_test_platform", "revisionId": guard.revision, "publicationId": guard.publication, "canonicalUrl": format!("http://{ORIGIN_HOST}:{}/ss-publish/browser/reader/{}", listener_port(&listener), guard.publication), "publicInternet": false, "policyEligibility": "private-prototype-only"}}),
                    )
                }
                "publication.withdraw" => {
                    state.lock().unwrap().withdrawn = true;
                    let guard = state.lock().unwrap();
                    ok(
                        "publication.withdraw",
                        json!({"acknowledgement": {"accepted": true, "effect": "withdraw-recorded", "authoritative": false, "seriesId": "ser_test_north", "episodeId": "ep_test_platform", "revisionId": guard.revision, "publicationId": guard.publication, "canonicalUrl": format!("http://{ORIGIN_HOST}:{}/ss-publish/browser/reader/{}", listener_port(&listener), guard.publication), "publicInternet": false}}),
                    )
                }
                "publication.read" => {
                    if state.lock().unwrap().withdrawn {
                        err("withdrawn")
                    } else {
                        let guard = state.lock().unwrap();
                        ok(
                            "publication.read",
                            json!({"authoritative": true, "status": "published", "publicationId": guard.publication, "canonicalUrl": value["body"]["canonicalUrl"], "revisionId": guard.revision, "contentSha256": "ab".repeat(32), "publicMetadata": {"seriesTitle": "North line", "episodeTitle": "Platform", "synopsis": "A reviewed public synopsis.", "language": "en"}, "presentation": {"viewport": "phone", "widthPx": 360, "ordered": true}, "publicInternet": false, "content": {"kind": "prose", "mediaType": "text/plain; charset=utf-8", "text": "Public line."}}),
                        )
                    }
                }
                _ => err("unknown_command"),
            };
            let (status, payload) = response;
            let bytes = serde_json::to_vec(&payload).unwrap();
            let http = format!(
                "HTTP/1.1 {status} X\r\nContent-Type: application/json; charset=utf-8\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                bytes.len()
            );
            let _ = stream.write_all(http.as_bytes());
            let _ = stream.write_all(&bytes);
        }
    }

    fn content_ready(raw: &[u8]) -> bool {
        let Some(split) = raw.windows(4).position(|window| window == b"\r\n\r\n") else {
            return false;
        };
        let header = String::from_utf8_lossy(&raw[..split]);
        let length = header.lines().find_map(|line| {
            let (name, value) = line.split_once(':')?;
            if name.eq_ignore_ascii_case("content-length") {
                value.trim().parse::<usize>().ok()
            } else {
                None
            }
        });
        match length {
            Some(length) => raw.len() >= split + 4 + length,
            None => true,
        }
    }

    fn listener_port(listener: &TcpListener) -> u16 {
        listener.local_addr().unwrap().port()
    }

    fn sample_account() -> Value {
        json!({
            "accountId": "acct_test_owner_a",
            "handle": "ada",
            "testOnly": true,
            "nonProductionLabel": "TEST-ONLY",
            "identityProof": "browser-fixture-affirmation",
            "series": [{
                "seriesId": "ser_test_north",
                "title": "North line",
                "ownerAccountId": "acct_test_owner_a",
                "episodes": [{
                    "episodeId": "ep_test_platform",
                    "ordinal": 1,
                    "title": "Platform",
                    "publication": null
                }]
            }]
        })
    }

    fn ok(command: &str, result: Value) -> (u16, Value) {
        (
            200,
            json!({"protocol": PROTOCOL, "ok": true, "command": command, "idempotentReplay": false, "result": result}),
        )
    }

    fn err(code: &str) -> (u16, Value) {
        (
            error_status(code),
            json!({"protocol": PROTOCOL, "ok": false, "command": null, "idempotentReplay": false, "error": {"code": code, "retryable": false}}),
        )
    }

    fn urlencoding(value: &str) -> String {
        let mut out = String::new();
        for byte in value.bytes() {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
                out.push(byte as char);
            } else {
                out.push_str(&format!("%{byte:02X}"));
            }
        }
        out
    }
}
