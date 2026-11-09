//! Native client against Vesta's loopback listener. This is not the egui window
//! and not a final safety acceptance.
//!
//! Chrome 156 sends `Origin: null` on the fixture form POST when the authorize
//! response includes `Referrer-Policy: no-referrer`. The listener then answers
//! 403 `unauthorized` and the PKCE callback never arrives. Do not spoof Origin.

use scroll_studio::export::ExportOptions;
use scroll_studio::model::{AssetBytes, Story};
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
    assert!(preview_html.contains("A public paragraph"));
    assert!(!preview_html.contains("SECRET_SYNOPSIS"));

    let story = Story::with_canvas("Private story", 64, 64);
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
    let south = owner_a
        .1
        .episodes
        .iter()
        .find(|episode| episode.episode_title == "South stair")
        .unwrap();
    let comic_ack = publish_wire::upload(
        origin,
        &owner_a.0,
        &comic,
        south,
        "story-comic",
        "episode-comic",
    )
    .unwrap();
    assert!(comic_ack
        .browser_preview_url
        .as_deref()
        .unwrap()
        .ends_with(&comic_ack.revision_id));

    let stale = publish_wire::confirm(origin, &owner_a.0, &owned, "rev_test_0099").unwrap_err();
    assert!(
        matches!(stale, WireError::Rejected { ref code, status: 409 } if code == "stale_revision")
    );
    let published = publish_wire::confirm(origin, &owner_a.0, &owned, &first.revision_id).unwrap();
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
    let page = http_status(&reader, None);
    assert_eq!(page, 410);
    let still_preview = http_status(&preview, Some("/tmp/chrome-owner-a"));
    assert_eq!(
        still_preview, 200,
        "owner preview remains after ordinary withdraw"
    );

    let cancel = publish_wire::cancel_operation(origin, &owner_a.0, key).unwrap_err();
    assert!(
        matches!(cancel, WireError::Rejected { ref code, status: 409 } if code == "already_terminal")
    );
    publish_wire::revoke(origin, &owner_a.0).unwrap();
    let expired = publish_wire::discover(origin, &owner_a.0).unwrap_err();
    assert!(matches!(
        expired,
        WireError::Rejected { ref code, status: 401 } if code == "credential_revoked"
    ));
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
                    std::thread::sleep(Duration::from_millis(400));
                    stop_chrome(&mut chrome);
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

fn http_status(url: &str, profile: Option<&str>) -> u16 {
    if let Some(profile) = profile {
        let html = chrome_text(
            url,
            profile,
            "/opt/cursor/artifacts/publish-preview-after-withdraw-360.png",
        );
        return if html.contains("TEST ONLY") && !html.contains("410") {
            200
        } else {
            410
        };
    }
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
