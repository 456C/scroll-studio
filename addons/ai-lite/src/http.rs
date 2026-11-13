//! Just enough HTTP/1.1 for AI Lite: a client for plain-`http://` model servers on this
//! computer or network, and a server for MCP on the loopback address. Bodies are limited
//! in size and every connection has timeouts.

use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

const MAX_HEADER_LINE: usize = 8 * 1024;
const MAX_HEADERS: usize = 64;
/// The largest body read from a model server.
const MAX_RESPONSE_BODY: usize = 8 * 1024 * 1024;

#[derive(Debug, PartialEq)]
pub struct Request {
    pub method: String,
    pub path: String,
    /// Header names in lowercase.
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Request {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }
}

/// Read one request. `max_body` limits the body.
pub fn read_request(stream: impl Read, max_body: usize) -> Result<Request, String> {
    let mut reader = BufReader::new(stream);
    let start = read_line(&mut reader)?;
    let mut parts = start.split_whitespace();
    let (Some(method), Some(path), Some(version)) = (parts.next(), parts.next(), parts.next())
    else {
        return Err("Malformed request line".into());
    };
    if !version.starts_with("HTTP/1.") {
        return Err("Unsupported HTTP version".into());
    }
    let headers = read_headers(&mut reader)?;
    let request = Request {
        method: method.to_owned(),
        path: path.to_owned(),
        headers,
        body: Vec::new(),
    };
    if request
        .header("transfer-encoding")
        .is_some_and(|v| !v.eq_ignore_ascii_case("identity"))
    {
        return Err("Chunked request bodies are not supported".into());
    }
    let length = match request.header("content-length") {
        Some(length) => length
            .trim()
            .parse::<usize>()
            .map_err(|_| "Bad Content-Length")?,
        None => 0,
    };
    if length > max_body {
        return Err("Request body too large".into());
    }
    let mut body = vec![0; length];
    reader
        .read_exact(&mut body)
        .map_err(|e| format!("Could not read the body: {e}"))?;
    Ok(Request { body, ..request })
}

pub fn write_response(
    mut stream: impl Write,
    status: u16,
    headers: &[(&str, &str)],
    body: &[u8],
) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        202 => "Accepted",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        413 => "Payload Too Large",
        _ => "Error",
    };
    let mut head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    for (name, value) in headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()
}

/// A plain-HTTP address split into host, port and path.
#[derive(Debug, PartialEq)]
pub struct Url {
    pub host: String,
    pub port: u16,
    pub path: String,
}

pub fn parse_url(url: &str) -> Result<Url, String> {
    let url = url.trim();
    if url.starts_with("https://") {
        return Err(
            "This version of AI Lite talks to model servers over http:// only, such as Ollama or LM Studio on this computer".into(),
        );
    }
    let rest = url
        .strip_prefix("http://")
        .ok_or("The model server address starts with http://")?;
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, ""),
    };
    if authority.is_empty() || authority.contains('@') {
        return Err("The model server address needs a host, like http://127.0.0.1:11434".into());
    }
    // `[::1]:8080` for IPv6, `host:8080` otherwise; the port defaults to 80.
    let (host, port) = match authority.strip_prefix('[') {
        Some(bracketed) => {
            let (host, after) = bracketed
                .split_once(']')
                .ok_or("The model server address is malformed")?;
            (host, after.strip_prefix(':'))
        }
        None => match authority.split_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (authority, None),
        },
    };
    let port = match port {
        Some(port) => port
            .parse::<u16>()
            .map_err(|_| "The model server port is not a number")?,
        None => 80,
    };
    if host.is_empty() {
        return Err("The model server address needs a host, like http://127.0.0.1:11434".into());
    }
    Ok(Url {
        host: host.to_owned(),
        port,
        path: path.trim_end_matches('/').to_owned(),
    })
}

/// Send a request and read a JSON answer. `body` is sent as JSON when given.
pub fn call_json(
    method: &str,
    url: &Url,
    path: &str,
    body: Option<&Value>,
    timeout: Duration,
) -> Result<Value, String> {
    // `localhost` may resolve to IPv6 first while the server listens on IPv4: try each.
    let mut last = None;
    let mut connected = None;
    for address in (url.host.as_str(), url.port)
        .to_socket_addrs()
        .map_err(|e| format!("Could not find {}: {e}", url.host))?
    {
        match TcpStream::connect_timeout(&address, Duration::from_secs(5)) {
            Ok(stream) => {
                connected = Some(stream);
                break;
            }
            Err(error) => last = Some(error),
        }
    }
    let mut stream = connected.ok_or_else(|| {
        format!(
            "Could not reach the model server at {}:{} ({}). Is it running?",
            url.host,
            url.port,
            last.map_or("not found".into(), |e| e.to_string())
        )
    })?;
    stream.set_read_timeout(Some(timeout)).ok();
    stream.set_write_timeout(Some(Duration::from_secs(30))).ok();
    let payload = body.map(|b| b.to_string()).unwrap_or_default();
    let host = if url.host.contains(':') {
        format!("[{}]:{}", url.host, url.port)
    } else {
        format!("{}:{}", url.host, url.port)
    };
    let mut head = format!(
        "{method} {}{path} HTTP/1.1\r\nHost: {host}\r\nAccept: application/json\r\nConnection: close\r\nContent-Length: {}\r\n",
        url.path,
        payload.len()
    );
    if body.is_some() {
        head.push_str("Content-Type: application/json\r\n");
    }
    head.push_str("\r\n");
    stream
        .write_all(head.as_bytes())
        .and_then(|_| stream.write_all(payload.as_bytes()))
        .map_err(|e| format!("Could not send to the model server: {e}"))?;
    let (status, body) = read_response(stream)?;
    let text = String::from_utf8_lossy(&body);
    if !(200..300).contains(&status) {
        let detail = serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|v| {
                v["error"]["message"]
                    .as_str()
                    .or(v["error"].as_str())
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| text.chars().take(200).collect());
        return Err(format!("The model server answered {status}: {detail}"));
    }
    serde_json::from_str(&text).map_err(|_| "The model server did not answer with JSON".into())
}

fn read_response(stream: impl Read) -> Result<(u16, Vec<u8>), String> {
    let mut reader = BufReader::new(stream);
    let status_line = read_line(&mut reader)?;
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .ok_or("The model server sent a malformed answer")?;
    let headers = read_headers(&mut reader)?;
    let header = |name: &str| {
        headers
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    };
    let mut body = Vec::new();
    if header("transfer-encoding").is_some_and(|v| v.to_ascii_lowercase().contains("chunked")) {
        loop {
            let size_line = read_line(&mut reader)?;
            let size = usize::from_str_radix(size_line.split(';').next().unwrap_or("").trim(), 16)
                .map_err(|_| "Malformed chunk")?;
            if size == 0 {
                break;
            }
            if body
                .len()
                .checked_add(size)
                .is_none_or(|end| end > MAX_RESPONSE_BODY)
            {
                return Err("The model server's answer is too large".into());
            }
            let start = body.len();
            body.resize(start + size, 0);
            reader
                .read_exact(&mut body[start..])
                .map_err(|e| format!("The model server stopped answering: {e}"))?;
            read_line(&mut reader)?;
        }
    } else if let Some(length) = header("content-length") {
        let length: usize = length.trim().parse().map_err(|_| "Bad Content-Length")?;
        if length > MAX_RESPONSE_BODY {
            return Err("The model server's answer is too large".into());
        }
        body.resize(length, 0);
        reader
            .read_exact(&mut body)
            .map_err(|e| format!("The model server stopped answering: {e}"))?;
    } else {
        reader
            .take(MAX_RESPONSE_BODY as u64 + 1)
            .read_to_end(&mut body)
            .map_err(|e| format!("The model server stopped answering: {e}"))?;
        if body.len() > MAX_RESPONSE_BODY {
            return Err("The model server's answer is too large".into());
        }
    }
    Ok((status, body))
}

fn read_line(reader: &mut impl BufRead) -> Result<String, String> {
    let mut line = Vec::new();
    reader
        .take(MAX_HEADER_LINE as u64 + 1)
        .read_until(b'\n', &mut line)
        .map_err(|e| format!("Connection error: {e}"))?;
    if line.len() > MAX_HEADER_LINE {
        return Err("Header line too long".into());
    }
    if line.is_empty() {
        return Err("Connection closed".into());
    }
    Ok(String::from_utf8_lossy(&line)
        .trim_end_matches(['\r', '\n'])
        .to_owned())
}

fn read_headers(reader: &mut impl BufRead) -> Result<Vec<(String, String)>, String> {
    let mut headers = Vec::new();
    loop {
        let line = read_line(reader)?;
        if line.is_empty() {
            return Ok(headers);
        }
        if headers.len() == MAX_HEADERS {
            return Err("Too many headers".into());
        }
        let (name, value) = line.split_once(':').ok_or("Malformed header")?;
        headers.push((name.trim().to_ascii_lowercase(), value.trim().to_owned()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_are_read_with_limits() {
        let raw = b"POST /mcp HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 2\r\nAuthorization: Bearer x\r\n\r\n{}";
        let request = read_request(&raw[..], 10).unwrap();
        assert_eq!(request.method, "POST");
        assert_eq!(request.path, "/mcp");
        assert_eq!(request.header("authorization"), Some("Bearer x"));
        assert_eq!(request.body, b"{}");
        assert!(read_request(&raw[..], 1).is_err(), "body over the limit");
        assert!(read_request(&b"garbage\r\n\r\n"[..], 10).is_err());
        let chunked = b"POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n";
        assert!(read_request(&chunked[..], 10).is_err());
    }

    #[test]
    fn responses_are_read_whole_or_chunked() {
        let plain = b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\n{}  ";
        assert_eq!(read_response(&plain[..]).unwrap(), (200, b"{}  ".to_vec()));
        let chunked =
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\n{\"a\r\n4\r\n\":1}\r\n0\r\n\r\n";
        assert_eq!(
            read_response(&chunked[..]).unwrap(),
            (200, b"{\"a\":1}".to_vec())
        );
        let unframed = b"HTTP/1.0 500 Oops\r\n\r\nbad";
        assert_eq!(
            read_response(&unframed[..]).unwrap(),
            (500, b"bad".to_vec())
        );
    }

    #[test]
    fn only_plain_http_addresses_are_accepted() {
        assert_eq!(
            parse_url("http://127.0.0.1:11434/").unwrap(),
            Url {
                host: "127.0.0.1".into(),
                port: 11434,
                path: String::new()
            }
        );
        assert_eq!(parse_url("http://localhost/api").unwrap().path, "/api");
        assert_eq!(parse_url("http://[::1]:8080").unwrap().host, "::1");
        assert!(parse_url("https://api.example.com").is_err());
        assert!(parse_url("ftp://x").is_err());
        assert!(parse_url("http://user@host").is_err());
        assert!(parse_url("http://:80").is_err());
    }

    #[test]
    fn responses_are_written_with_a_length() {
        let mut out = Vec::new();
        write_response(&mut out, 202, &[("X-Test", "1")], b"").unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.starts_with("HTTP/1.1 202 Accepted\r\n"));
        assert!(text.contains("Content-Length: 0\r\n"));
        assert!(text.contains("X-Test: 1\r\n"));
    }
}
