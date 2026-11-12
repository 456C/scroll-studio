//! Native client against the loopback listener. This is not the egui window.
//! The browser is headless Chrome. It does not spoof Origin or send fixture clocks.

use scroll_studio::export::ExportOptions;
use scroll_studio::model::{AssetBytes, Chapter, Story};
use scroll_studio::publish::{self, PublicMetadata};
use scroll_studio::publish_wire::{self, Origin, WireError};
use std::{
    io::{Read, Write},
    net::TcpStream,
    process::{Command, Stdio},
    time::Duration,
};

#[test]
#[ignore = "requires python3 -m prototypes.publishing.listener on 127.0.0.1:19427"]
fn comic_and_prose_against_the_listener() {
    let origin = Origin::pinned();
    let owner_a = sign_in(origin, "acct_test_owner_a", "/tmp/chrome-owner-a");
    let prose = publish::package_prose_bytes(
        b"A public paragraph.\nSecond line.",
        PublicMetadata::new(
            "Fixture Lane",
            "North door",
            "Reviewed public synopsis.",
            "en",
        )
        .unwrap(),
    )
    .unwrap();
    assert!(!prose.contains_bytes(b"SECRET_SYNOPSIS"));
    let owned = owner_a
        .1
        .episodes
        .iter()
        .find(|episode| episode.episode_title == "North door")
        .unwrap()
        .clone();
    let key = "listener-prose-key-01";
    let first = publish_wire::upload_with_key(
        origin,
        &owner_a.0,
        &prose,
        &owned,
        "story-prose",
        "episode-prose",
        key,
    )
    .unwrap();
    let second = publish_wire::upload_with_key(
        origin,
        &owner_a.0,
        &prose,
        &owned,
        "story-prose",
        "episode-prose",
        key,
    )
    .unwrap();
    assert_eq!(first.revision_id, second.revision_id);
    let preview = first.browser_preview_url.clone().unwrap();
    assert!(preview.contains(&first.revision_id));
    assert!(preview.starts_with("http://127.0.0.1:19427/ss-publish/browser/preview/"));
    let preview_html = chrome_text(
        &preview,
        "/tmp/chrome-owner-a",
        "/opt/cursor/artifacts/publish-preview-360.png",
    );
    assert!(
        preview_html.contains("A public paragraph"),
        "preview was {}",
        preview_html.chars().take(500).collect::<String>()
    );
    assert!(!preview_html.contains("SECRET_SYNOPSIS"));

    let mut story = Story::with_canvas("Private story", 64, 64);
    story.chapters[0].panels[0].script = "PRIVATE_PANEL_SCRIPT".into();
    let other = story.alloc_id();
    story
        .chapters
        .push(Chapter::new(other, "UNSELECTED_CHAPTER", 64, 64));
    let before = story.clone();
    let comic = publish::package_chapter(
        &story.chapters[0],
        &AssetBytes::new(),
        &ExportOptions {
            profile: scroll_studio::export::ExportProfile::Original,
            format: scroll_studio::export::ExportFormat::Png,
            jpeg_quality: 90,
        },
        PublicMetadata::new(
            "Fixture Lane",
            "South stair",
            "Reviewed comic synopsis.",
            "en",
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(story, before);
    assert!(!comic.contains_bytes(b".scro"));
    assert!(!comic.contains_bytes(b"PRIVATE_PANEL_SCRIPT"));
    assert!(!comic.contains_bytes(b"UNSELECTED_CHAPTER"));
    let south = owner_a
        .1
        .episodes
        .iter()
        .find(|episode| episode.episode_title == "South stair")
        .unwrap()
        .clone();
    let comic_ack = publish_wire::upload(
        origin,
        &owner_a.0,
        &comic,
        &south,
        "story-comic",
        "episode-comic",
    )
    .unwrap();
    assert!(comic_ack
        .browser_preview_url
        .as_deref()
        .unwrap()
        .ends_with(&comic_ack.revision_id));

    let revised = publish::package_prose_bytes(
        b"A public paragraph.\nSecond line.",
        PublicMetadata::new(
            "Fixture Lane",
            "North door",
            "A later public synopsis.",
            "en",
        )
        .unwrap(),
    )
    .unwrap();
    let newer = publish_wire::upload_with_key(
        origin,
        &owner_a.0,
        &revised,
        &owned,
        "story-prose",
        "episode-prose",
        "listener-prose-key-02",
    )
    .unwrap();
    assert_ne!(first.revision_id, newer.revision_id);
    let guessed_revision =
        publish_wire::confirm(origin, &owner_a.0, &owned, "rev_test_0099").unwrap_err();
    assert!(
        matches!(guessed_revision, WireError::Rejected { ref code, status: 404 } if code == "not_found"),
        "unknown revision was {guessed_revision}"
    );
    let stale = publish_wire::confirm(origin, &owner_a.0, &owned, &first.revision_id).unwrap_err();
    assert!(
        matches!(stale, WireError::Rejected { ref code, status: 409 } if code == "stale_revision"),
        "stale confirm was {stale}"
    );
    let published = publish_wire::confirm(origin, &owner_a.0, &owned, &newer.revision_id).unwrap();
    let reader = published.canonical_url.clone().unwrap();
    let status = publish_wire::read_publication(origin, &owner_a.0, &reader).unwrap();
    assert!(
        matches!(status, publish_wire::LiveStatus::Read { ref status, .. } if status == "published")
    );
    let reader_html = chrome_text(
        &reader,
        "/tmp/chrome-owner-a",
        "/opt/cursor/artifacts/publish-reader-360.png",
    );
    assert!(reader_html.contains("TEST ONLY"));
    assert!(reader_html.contains("A public paragraph"));

    let owner_b = sign_in(origin, "acct_test_owner_b", "/tmp/chrome-owner-b");
    let foreign = publish_wire::read_publication(origin, &owner_b.0, &reader).unwrap_err();
    assert!(
        matches!(foreign, WireError::Rejected { ref code, status: 403 } if code == "not_owner")
    );
    let guessed = publish_wire::read_publication(
        origin,
        &owner_a.0,
        "http://127.0.0.1:19427/ss-publish/browser/reader/pub_test_9999",
    )
    .unwrap_err();
    assert!(
        matches!(guessed, WireError::Rejected { ref code, status: 404 } if code == "not_found")
    );

    publish_wire::withdraw(
        origin,
        &owner_a.0,
        &owned,
        published.publication_id.as_deref().unwrap(),
        &published.revision_id,
    )
    .unwrap();
    let after = publish_wire::read_publication(origin, &owner_a.0, &reader).unwrap_err();
    assert!(matches!(after, WireError::Rejected { ref code, status: 410 } if code == "withdrawn"));
    let anonymous = raw_status(&reader);
    assert_eq!(
        anonymous, 401,
        "a reader without a session is the sign-in page"
    );
    let (still_status, still_body) = session_page(
        &preview,
        "/tmp/chrome-owner-a",
        "/opt/cursor/artifacts/publish-preview-after-withdraw-360.png",
    );
    assert_eq!(
        still_status, 200,
        "owner preview remains after ordinary withdraw: {still_body}"
    );
    assert!(still_body.contains("A public paragraph"));

    let cancel = publish_wire::cancel_operation(origin, &owner_a.0, key).unwrap_err();
    assert!(
        matches!(cancel, WireError::Rejected { ref code, status: 409 } if code == "already_terminal")
    );
    let comic_published =
        publish_wire::confirm(origin, &owner_a.0, &south, &comic_ack.revision_id).unwrap();
    let comic_reader = comic_published.canonical_url.clone().unwrap();
    let comic_publication = comic_published.publication_id.clone().unwrap();
    let operator = sign_in(origin, "acct_test_operator", "/tmp/chrome-operator");
    assert_eq!(operator.1.account_id, "acct_test_operator");
    let held =
        operator_submit("/tmp/chrome-operator", &comic_publication, "hold").expect("operator hold");
    assert!(held.contains("Publication held"), "hold page was {held}");
    let held_read = publish_wire::read_publication(origin, &owner_a.0, &comic_reader).unwrap_err();
    assert!(
        matches!(held_read, WireError::Rejected { ref code, status: 404 } if code == "not_found"),
        "held read was {held_read}"
    );
    let (held_browser, held_browser_body) = session_page(
        &comic_reader,
        "/tmp/chrome-owner-a",
        "/opt/cursor/artifacts/publish-reader-held-360.png",
    );
    assert_eq!(held_browser, 410, "held reader body {held_browser_body}");
    let comic_preview = comic_ack.browser_preview_url.clone().unwrap();
    let (held_preview, held_preview_body) = session_page(
        &comic_preview,
        "/tmp/chrome-owner-a",
        "/opt/cursor/artifacts/publish-preview-while-held-360.png",
    );
    assert_eq!(
        held_preview, 200,
        "hold leaves the exact preview: {held_preview_body}"
    );
    let held_confirm =
        publish_wire::confirm(origin, &owner_a.0, &south, &comic_ack.revision_id).unwrap_err();
    assert!(
        matches!(held_confirm, WireError::Rejected { ref code, status: 404 } if code == "not_found")
    );

    let removed = operator_submit("/tmp/chrome-operator", &comic_publication, "remove")
        .expect("operator remove");
    assert!(
        removed.contains("Publication removed"),
        "remove page was {removed}"
    );
    let (removed_preview, removed_preview_body) = session_page(
        &comic_preview,
        "/tmp/chrome-owner-a",
        "/opt/cursor/artifacts/publish-preview-after-remove-360.png",
    );
    assert_eq!(
        removed_preview, 404,
        "remove blocks preview: {removed_preview_body}"
    );
    let removed_read =
        publish_wire::read_publication(origin, &owner_a.0, &comic_reader).unwrap_err();
    assert!(
        matches!(removed_read, WireError::Rejected { ref code, status: 410 } if code == "withdrawn"),
        "removed read was {removed_read}"
    );
    let removed_upload = publish_wire::upload(
        origin,
        &owner_a.0,
        &comic,
        &south,
        "story-comic",
        "episode-comic",
    )
    .unwrap_err();
    assert!(
        matches!(removed_upload, WireError::Rejected { ref code, status: 410 } if code == "withdrawn")
    );
    assert_eq!(story, before);

    publish_wire::revoke(origin, &owner_a.0).unwrap();
    let expired = publish_wire::discover(origin, &owner_a.0).unwrap_err();
    assert!(matches!(
        expired,
        WireError::Rejected { ref code, status: 401 } if code == "credential_revoked"
    ));

    let late = publish_wire::sign_in(origin, |url| {
        std::thread::sleep(Duration::from_secs(125));
        click_account(url, "acct_test_owner_b", "/tmp/chrome-auth-expired").map_err(|error| {
            WireError::Protocol(format!(
                "The browser did not finish fixture sign-in: {error}"
            ))
        })
    })
    .unwrap_err();
    assert!(
        matches!(late, WireError::Rejected { ref code, status: 401 } if code == "authorization_expired"),
        "late exchange was {late}"
    );
}

fn sign_in(
    origin: Origin,
    account_id: &str,
    profile: &str,
) -> (publish_wire::Credential, publish_wire::AccountSnapshot) {
    let account = account_id.to_string();
    let profile = profile.to_string();
    publish_wire::sign_in(origin, move |url| {
        click_account(url, &account, &profile).map_err(|error| {
            WireError::Protocol(format!(
                "The browser did not finish fixture sign-in: {error}"
            ))
        })
    })
    .expect("listener sign-in")
}

fn click_account(url: &str, account_id: &str, profile: &str) -> Result<(), String> {
    let _ = std::fs::create_dir_all("/opt/cursor/artifacts");
    let _ = Command::new("/opt/google/chrome/chrome")
        .args([
            "--headless=new",
            "--disable-gpu",
            "--no-sandbox",
            "--window-size=360,800",
            &format!("--user-data-dir={profile}-picker"),
            &format!("--screenshot=/opt/cursor/artifacts/publish-picker-{account_id}.png"),
            url,
        ])
        .output();
    let _ = Command::new("pkill")
        .args(["-f", "remote-debugging-port=9333"])
        .output();
    std::thread::sleep(Duration::from_millis(200));
    let _ = std::fs::remove_dir_all(profile);
    let mut chrome = Command::new("/opt/google/chrome/chrome")
        .args([
            "--headless=new",
            "--disable-gpu",
            "--no-first-run",
            "--no-sandbox",
            &format!("--user-data-dir={profile}"),
            "--remote-debugging-port=9333",
            url,
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| error.to_string())?;
    let port = 9333_u16;
    let mut list = String::new();
    let mut last = String::new();
    for _ in 0..20 {
        match http_get(&format!("http://127.0.0.1:{port}/json/list")) {
            Ok(body)
                if body.contains(account_id) || body.contains("/ss-publish/browser/authorize") =>
            {
                list = body;
                break;
            }
            Ok(body) => last = body,
            Err(error) => last = error,
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    if list.is_empty() {
        stop_chrome(&mut chrome);
        return Err(format!("json/list on {port}: {last}"));
    }
    let page = list
        .split("\"type\": \"page\"")
        .nth(1)
        .ok_or_else(|| format!("no page target in {list}"))?;
    let socket = page
        .split("webSocketDebuggerUrl\": \"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .ok_or_else(|| format!("no page debugger socket in {list}"))?
        .to_string();
    let mut ws = WebSocket::connect(&socket)?;
    let mut last = String::new();
    for _ in 0..40 {
        let expression = format!(
            "(() => {{ const button = document.querySelector('button[value=\"{account_id}\"]'); if (!button) return 'waiting'; button.click(); return 'clicked'; }})()"
        );
        let value = ws.evaluate(&expression)?;
        last = value.clone();
        if value.contains("\"value\":\"clicked\"") {
            for _ in 0..20 {
                let location = ws.evaluate("location.href").unwrap_or_default();
                if location.contains("/ss-publish/callback") {
                    let _ = ws.send_text("{\"id\":4,\"method\":\"Browser.close\",\"params\":{}}");
                    let _ = chrome.wait();
                    return Ok(());
                }
                std::thread::sleep(Duration::from_millis(150));
            }
            let location = ws.evaluate("location.href").unwrap_or_default();
            let body = ws
                .evaluate("document.body ? document.body.innerText.slice(0, 400) : ''")
                .unwrap_or_default();
            stop_chrome(&mut chrome);
            return Err(format!(
                "affirm did not reach the callback; location={location}; body={body}"
            ));
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    stop_chrome(&mut chrome);
    Err(format!(
        "fixture account button did not appear; last={last}"
    ))
}

fn stop_chrome(chrome: &mut std::process::Child) {
    let _ = Command::new("kill")
        .args(["-TERM", &chrome.id().to_string()])
        .status();
    let _ = chrome.wait();
}

fn chrome_text(url: &str, profile: &str, shot: &str) -> String {
    let _ = std::fs::create_dir_all("/opt/cursor/artifacts");
    let _ = Command::new("/opt/google/chrome/chrome")
        .args([
            "--headless=new",
            "--disable-gpu",
            "--no-sandbox",
            "--window-size=360,800",
            &format!("--user-data-dir={profile}"),
            &format!("--screenshot={shot}"),
            url,
        ])
        .output();
    let dump = Command::new("/opt/google/chrome/chrome")
        .args([
            "--headless=new",
            "--disable-gpu",
            "--no-sandbox",
            "--virtual-time-budget=3000",
            "--window-size=360,800",
            &format!("--user-data-dir={profile}"),
            "--dump-dom",
            url,
        ])
        .output()
        .expect("chrome dump");
    String::from_utf8_lossy(&dump.stdout).into_owned()
}

fn raw_status(url: &str) -> u16 {
    let host = url.trim_start_matches("http://");
    let mut stream = TcpStream::connect("127.0.0.1:19427").unwrap();
    let request = format!(
        "GET {host_path} HTTP/1.1\r\nHost: 127.0.0.1:19427\r\nConnection: close\r\n\r\n",
        host_path = host
            .split_once('/')
            .map(|(_, path)| format!("/{path}"))
            .unwrap_or_else(|| "/".to_string())
    );
    stream.write_all(request.as_bytes()).unwrap();
    let mut buffer = String::new();
    stream.read_to_string(&mut buffer).unwrap();
    buffer
        .split_whitespace()
        .nth(1)
        .unwrap_or("0")
        .parse()
        .unwrap_or(0)
}

fn session_page(url: &str, profile: &str, shot: &str) -> (u16, String) {
    let _ = std::fs::create_dir_all("/opt/cursor/artifacts");
    let _ = Command::new("/opt/google/chrome/chrome")
        .args([
            "--headless=new",
            "--disable-gpu",
            "--no-sandbox",
            "--window-size=360,800",
            &format!("--user-data-dir={profile}"),
            &format!("--screenshot={shot}"),
            url,
        ])
        .output();
    match chrome_status(url, profile) {
        Ok(result) => result,
        Err(error) => (0, error),
    }
}

fn operator_submit(profile: &str, publication_id: &str, action: &str) -> Result<String, String> {
    let url = "http://127.0.0.1:19427/ss-publish/browser/operator";
    let expression = format!(
        "(() => {{ const item = [...document.querySelectorAll('li')].find((entry) => entry.innerText.includes('{publication_id}')); if (!item) return 'waiting'; const form = [...item.querySelectorAll('form')].find((entry) => entry.action.endsWith('/{action}')); if (!form) return 'missing'; form.querySelector('button').click(); return 'clicked'; }})()"
    );
    let mut ws = launch_page(profile, url)?;
    let mut last = String::new();
    for _ in 0..30 {
        let value = ws.evaluate(&expression)?;
        last = value.clone();
        if value.contains("\"value\":\"clicked\"") {
            for _ in 0..20 {
                let body = ws.evaluate("document.body ? document.body.innerText : ''")?;
                if body.contains("Publication held") || body.contains("Publication removed") {
                    let text = json_string_value(&body);
                    drop(ws);
                    return Ok(text);
                }
                std::thread::sleep(Duration::from_millis(150));
            }
            let body = ws.evaluate("document.body ? document.body.innerText : ''")?;
            drop(ws);
            return Err(format!("operator {action} stayed on {body}"));
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    drop(ws);
    Err(format!("operator control did not appear; last={last}"))
}

fn chrome_status(url: &str, profile: &str) -> Result<(u16, String), String> {
    let mut ws = launch_page(profile, "about:blank")?;
    ws.send_text("{\"id\":2,\"method\":\"Network.enable\",\"params\":{}}")?;
    let _ = ws.read_text();
    let navigate = format!(
        "{{\"id\":3,\"method\":\"Page.navigate\",\"params\":{{\"url\":{}}}}}",
        serde_json::to_string(url).map_err(|error| error.to_string())?
    );
    ws.send_text(&navigate)?;
    ws.set_read_timeout(Duration::from_millis(400))?;
    let mut status = 0_u16;
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        let Ok(frame) = ws.read_text() else {
            continue;
        };
        if frame.contains("responseReceived") && frame.contains(url) {
            if let Some(code) = frame.split("\"status\":").nth(1).and_then(|rest| {
                rest.chars()
                    .take_while(|character| character.is_ascii_digit())
                    .collect::<String>()
                    .parse()
                    .ok()
            }) {
                status = code;
            }
        }
    }
    ws.set_read_timeout(Duration::from_secs(5))?;
    let body = ws
        .evaluate("document.body ? document.body.innerText.slice(0, 800) : ''")
        .unwrap_or_default();
    drop(ws);
    if status == 0 {
        return Err(format!("no status for {url}; body={body}"));
    }
    Ok((status, json_string_value(&body)))
}

fn launch_page(profile: &str, url: &str) -> Result<Page, String> {
    let _ = Command::new("pkill")
        .args(["-f", "remote-debugging-port=9333"])
        .output();
    std::thread::sleep(Duration::from_millis(200));
    let mut chrome = Command::new("/opt/google/chrome/chrome")
        .args([
            "--headless=new",
            "--disable-gpu",
            "--no-first-run",
            "--no-sandbox",
            &format!("--user-data-dir={profile}"),
            "--remote-debugging-port=9333",
            url,
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| error.to_string())?;
    let mut list = String::new();
    let mut last = String::new();
    for _ in 0..20 {
        match http_get("http://127.0.0.1:9333/json/list") {
            Ok(body) if body.contains("\"type\": \"page\"") => {
                list = body;
                break;
            }
            Ok(body) => last = body,
            Err(error) => last = error,
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    if list.is_empty() {
        stop_child(&mut chrome);
        return Err(format!("json/list on 9333: {last}"));
    }
    let page = list
        .split("\"type\": \"page\"")
        .nth(1)
        .ok_or_else(|| format!("no page target in {list}"))?;
    let socket = page
        .split("webSocketDebuggerUrl\": \"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .ok_or_else(|| format!("no page debugger socket in {list}"))?
        .to_string();
    let ws = match WebSocket::connect(&socket) {
        Ok(ws) => ws,
        Err(error) => {
            stop_child(&mut chrome);
            return Err(error);
        }
    };
    Ok(Page { chrome, ws })
}

struct Page {
    chrome: std::process::Child,
    ws: WebSocket,
}

impl Page {
    fn evaluate(&mut self, expression: &str) -> Result<String, String> {
        self.ws.evaluate(expression)
    }

    fn send_text(&mut self, text: &str) -> Result<(), String> {
        self.ws.send_text(text)
    }

    fn read_text(&mut self) -> Result<String, String> {
        self.ws.read_text()
    }

    fn set_read_timeout(&self, duration: Duration) -> Result<(), String> {
        self.ws.set_read_timeout(duration)
    }
}

impl Drop for Page {
    fn drop(&mut self) {
        stop_child(&mut self.chrome);
    }
}

fn stop_child(chrome: &mut std::process::Child) {
    let _ = Command::new("kill")
        .args(["-TERM", &chrome.id().to_string()])
        .status();
    let _ = chrome.wait();
}

fn json_string_value(payload: &str) -> String {
    let Some(rest) = payload.split("\"value\":\"").nth(1) else {
        return payload.to_string();
    };
    let mut out = String::new();
    let mut escaped = false;
    for character in rest.chars() {
        if escaped {
            out.push(match character {
                'n' => '\n',
                't' => '\t',
                other => other,
            });
            escaped = false;
            continue;
        }
        if character == '\\' {
            escaped = true;
            continue;
        }
        if character == '"' {
            break;
        }
        out.push(character);
    }
    out
}

fn http_get(url: &str) -> Result<String, String> {
    let host_path = url.trim_start_matches("http://");
    let (host, path) = host_path.split_once('/').ok_or("bad url")?;
    let mut stream = TcpStream::connect(host).map_err(|error| error.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|error| error.to_string())?;
    let request = format!("GET /{path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n");
    stream
        .write_all(request.as_bytes())
        .map_err(|error| error.to_string())?;
    let mut raw = Vec::new();
    let mut chunk = [0_u8; 4096];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => {
                raw.extend_from_slice(&chunk[..read]);
                if let Some(split) = raw.windows(4).position(|window| window == b"\r\n\r\n") {
                    let header = String::from_utf8_lossy(&raw[..split]);
                    let length = header.lines().find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().ok())
                            .flatten()
                    });
                    if length.is_some_and(|length| raw.len() >= split + 4 + length) {
                        break;
                    }
                }
            }
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock
                    || error.kind() == std::io::ErrorKind::TimedOut =>
            {
                break;
            }
            Err(error) => return Err(error.to_string()),
        }
    }
    let text = String::from_utf8_lossy(&raw);
    Ok(text
        .split_once("\r\n\r\n")
        .map(|(_, body)| body.to_string())
        .unwrap_or_default())
}

struct WebSocket {
    stream: TcpStream,
}

impl WebSocket {
    fn connect(url: &str) -> Result<Self, String> {
        let rest = url.trim_start_matches("ws://");
        let (host, path) = rest.split_once('/').ok_or("bad socket")?;
        let mut stream = TcpStream::connect(host).map_err(|error| error.to_string())?;
        stream
            .set_read_timeout(Some(Duration::from_secs(15)))
            .map_err(|error| error.to_string())?;
        let key = "dGhlIHNhbXBsZSBub25jZQ==";
        let request = format!(
            "GET /{path} HTTP/1.1\r\nHost: {host}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n"
        );
        stream
            .write_all(request.as_bytes())
            .map_err(|error| error.to_string())?;
        let mut buffer = [0_u8; 1024];
        let read = stream
            .read(&mut buffer)
            .map_err(|error| error.to_string())?;
        let header = String::from_utf8_lossy(&buffer[..read]);
        if !header.contains("101") {
            return Err(format!("websocket upgrade failed: {header}"));
        }
        Ok(Self { stream })
    }

    fn set_read_timeout(&self, duration: Duration) -> Result<(), String> {
        self.stream
            .set_read_timeout(Some(duration))
            .map_err(|error| error.to_string())
    }

    fn evaluate(&mut self, expression: &str) -> Result<String, String> {
        let payload = format!(
            "{{\"id\":1,\"method\":\"Runtime.evaluate\",\"params\":{{\"expression\":{},\"returnByValue\":true}}}}",
            serde_json::to_string(expression).map_err(|error| error.to_string())?
        );
        self.send_text(&payload)?;
        let text = self.read_text()?;
        Ok(text)
    }

    fn send_text(&mut self, text: &str) -> Result<(), String> {
        let mut frame = vec![0x81_u8];
        let len = text.len();
        if len < 126 {
            frame.push(0x80 | len as u8);
        } else {
            frame.push(0x80 | 126);
            frame.extend_from_slice(&(len as u16).to_be_bytes());
        }
        let mask = [1_u8, 2, 3, 4];
        frame.extend_from_slice(&mask);
        for (index, byte) in text.bytes().enumerate() {
            frame.push(byte ^ mask[index % 4]);
        }
        self.stream
            .write_all(&frame)
            .map_err(|error| error.to_string())
    }

    fn read_text(&mut self) -> Result<String, String> {
        let mut header = [0_u8; 2];
        self.stream
            .read_exact(&mut header)
            .or_else(|error| {
                if error.kind() == std::io::ErrorKind::WouldBlock
                    || error.kind() == std::io::ErrorKind::TimedOut
                {
                    self.stream.read_exact(&mut header)
                } else {
                    Err(error)
                }
            })
            .map_err(|error| error.to_string())?;
        let mut len = (header[1] & 0x7f) as usize;
        if len == 126 {
            let mut extended = [0_u8; 2];
            self.stream
                .read_exact(&mut extended)
                .map_err(|error| error.to_string())?;
            len = u16::from_be_bytes(extended) as usize;
        }
        let mut body = vec![0_u8; len];
        self.stream
            .read_exact(&mut body)
            .map_err(|error| error.to_string())?;
        String::from_utf8(body).map_err(|error| error.to_string())
    }
}
