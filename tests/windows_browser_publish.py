"""Hosted Windows browser check for the loopback publishing listener.

This file is app-owned. It does not modify the service tree. It drives the
browser that is already installed on the runner through authorize, the fixture
picker, affirm, the desktop callback, preview, and the reader.
"""

from __future__ import annotations

import base64
import hashlib
import json
import os
import secrets
import socket
import subprocess
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qsl, urlsplit

PROTOCOL = "scroll-studio.publishing.private-prototype.v1"
ORIGIN = "http://127.0.0.1:19427"
HOST = "127.0.0.1:19427"
ISSUER = "http://127.0.0.1:19427/ss-publish/test-issuer"
CLIENT_ID = "scroll-studio-desktop-test"
PROSE = "Windows runner public paragraph."
ALPHABET = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_"


def token(length: int) -> str:
    return "".join(secrets.choice(ALPHABET) for _ in range(length))


def pkce_challenge(verifier: str) -> str:
    digest = hashlib.sha256(verifier.encode("ascii")).digest()
    return base64.urlsafe_b64encode(digest).decode("ascii").rstrip("=")


def redact(text: str) -> str:
    lowered = text.lower()
    if "code=" in lowered or "state=" in lowered or "sek_live_" in lowered:
        return "[redacted callback or credential]"
    return text


def command(name: str, body: dict, credential: dict | None = None) -> dict:
    payload = {
        "protocol": PROTOCOL,
        "command": name,
        "idempotencyKey": token(32),
        "body": body,
    }
    if credential is not None:
        payload["credential"] = credential
    raw = json.dumps(payload).encode("utf-8")
    connection = socket.create_connection(("127.0.0.1", 19427), 10)
    header = (
        "POST /ss-publish/commands HTTP/1.1\r\n"
        f"Host: {HOST}\r\n"
        "Content-Type: application/json\r\n"
        f"Content-Length: {len(raw)}\r\n"
        "Connection: close\r\n\r\n"
    ).encode("ascii")
    connection.sendall(header + raw)
    data = b""
    connection.settimeout(20)
    while True:
        chunk = connection.recv(65536)
        if not chunk:
            break
        data += chunk
    head, _, response_body = data.partition(b"\r\n\r\n")
    status = int(head.split()[1])
    parsed = json.loads(response_body.decode("utf-8"))
    if not parsed.get("ok"):
        code = parsed.get("error", {}).get("code")
        raise SystemExit(f"{name} failed: HTTP {status} {code}")
    return parsed


class Callback(BaseHTTPRequestHandler):
    result: tuple[str, str] | None = None
    error: str | None = None

    def do_GET(self) -> None:
        path, _, query = self.path.partition("?")
        keys = [key for key, _value in parse_qsl(query, keep_blank_values=True)]
        if path != "/ss-publish/callback" or keys != ["code", "state"]:
            Callback.error = f"unexpected callback path keys {keys}"
            self.send_response(400)
        else:
            values = dict(parse_qsl(query))
            Callback.result = (values["code"], values["state"])
            self.send_response(200)
        body = b"Signed in. You can close this window."
        self.send_header("Content-Type", "text/plain; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, fmt: str, *args: object) -> None:
        return


def find_browser() -> str:
    candidates = [
        r"C:\Program Files\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
        r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
    ]
    for path in candidates:
        if os.path.isfile(path):
            print("browser", path)
            return path
    raise SystemExit("Neither Chrome nor Edge is installed at the usual Windows paths.")


def ws_connect(url: str) -> socket.socket:
    parts = urlsplit(url)
    host, _, port = parts.netloc.partition(":")
    connection = socket.create_connection((host, int(port or "80")), 10)
    key = base64.b64encode(os.urandom(16)).decode("ascii")
    request = (
        f"GET {parts.path} HTTP/1.1\r\n"
        f"Host: {parts.netloc}\r\n"
        "Upgrade: websocket\r\n"
        "Connection: Upgrade\r\n"
        f"Sec-WebSocket-Key: {key}\r\n"
        "Sec-WebSocket-Version: 13\r\n\r\n"
    )
    connection.sendall(request.encode("ascii"))
    data = b""
    while b"\r\n\r\n" not in data:
        data += connection.recv(4096)
    if b" 101 " not in data.split(b"\r\n", 1)[0]:
        raise SystemExit("browser debugger upgrade failed")
    return connection


def ws_send(connection: socket.socket, text: str) -> None:
    payload = text.encode("utf-8")
    mask = os.urandom(4)
    frame = bytearray([0x81])
    length = len(payload)
    if length < 126:
        frame.append(0x80 | length)
    elif length < 65536:
        frame.append(0x80 | 126)
        frame.extend(length.to_bytes(2, "big"))
    else:
        frame.append(0x80 | 127)
        frame.extend(length.to_bytes(8, "big"))
    frame.extend(mask)
    frame.extend(byte ^ mask[index % 4] for index, byte in enumerate(payload))
    connection.sendall(frame)


def ws_read(connection: socket.socket) -> str:
    def exact(count: int) -> bytes:
        data = b""
        while len(data) < count:
            chunk = connection.recv(count - len(data))
            if not chunk:
                raise SystemExit("browser debugger closed")
            data += chunk
        return data

    header = exact(2)
    length = header[1] & 0x7F
    if length == 126:
        length = int.from_bytes(exact(2), "big")
    elif length == 127:
        length = int.from_bytes(exact(8), "big")
    return exact(length).decode("utf-8", "replace")


def http_get(url: str) -> str:
    parts = urlsplit(url)
    connection = socket.create_connection((parts.hostname, parts.port or 80), 5)
    connection.settimeout(3)
    connection.sendall(
        f"GET {parts.path or '/'} HTTP/1.1\r\nHost: {parts.netloc}\r\nConnection: close\r\n\r\n".encode(
            "ascii"
        )
    )
    data = b""
    while True:
        try:
            chunk = connection.recv(4096)
        except TimeoutError:
            break
        if not chunk:
            break
        data += chunk
        if b"\r\n\r\n" in data and b"Content-Length:" in data.split(b"\r\n\r\n", 1)[0]:
            header, body = data.split(b"\r\n\r\n", 1)
            for line in header.split(b"\r\n"):
                if line.lower().startswith(b"content-length:"):
                    expected = int(line.split(b":", 1)[1].strip())
                    if len(body) >= expected:
                        return body[:expected].decode("utf-8", "replace")
    return data.split(b"\r\n\r\n", 1)[-1].decode("utf-8", "replace")


class Browser:
    def __init__(self, executable: str) -> None:
        profile = os.path.join(os.environ.get("RUNNER_TEMP", os.getcwd()), "windows-browser-profile")
        os.makedirs(profile, exist_ok=True)
        self.proc = subprocess.Popen(
            [
                executable,
                "--headless=new",
                "--disable-gpu",
                "--no-first-run",
                "--no-sandbox",
                f"--user-data-dir={profile}",
                "--remote-debugging-port=9344",
                "about:blank",
            ],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        listing = ""
        for _ in range(40):
            try:
                listing = http_get("http://127.0.0.1:9344/json/list")
                if '"type": "page"' in listing:
                    break
            except OSError:
                time.sleep(0.25)
        page = listing.split('"type": "page"', 1)[1]
        socket_url = page.split('"webSocketDebuggerUrl": "', 1)[1].split('"', 1)[0]
        self.connection = ws_connect(socket_url)
        self.connection.settimeout(10)
        self.call_id = 0

    def evaluate(self, expression: str) -> str:
        self.call_id += 1
        ws_send(
            self.connection,
            json.dumps(
                {
                    "id": self.call_id,
                    "method": "Runtime.evaluate",
                    "params": {"expression": expression, "returnByValue": True},
                }
            ),
        )
        deadline = time.time() + 10
        while time.time() < deadline:
            message = json.loads(ws_read(self.connection))
            if message.get("id") == self.call_id:
                return json.dumps(message)
        raise SystemExit("browser evaluation timed out")

    def navigate(self, url: str) -> None:
        self.call_id += 1
        ws_send(
            self.connection,
            json.dumps({"id": self.call_id, "method": "Page.navigate", "params": {"url": url}}),
        )
        deadline = time.time() + 10
        while time.time() < deadline:
            message = json.loads(ws_read(self.connection))
            if message.get("id") == self.call_id:
                return
        raise SystemExit("browser navigation timed out")

    def click_account(self, account_id: str) -> None:
        expression = (
            "(() => { const button = document.querySelector("
            f"'button[value=\"{account_id}\"]');"
            " if (!button) return 'waiting'; button.click(); return 'clicked'; })()"
        )
        for _ in range(40):
            value = self.evaluate(expression)
            if '"value":"clicked"' in value.replace(" ", ""):
                return
            time.sleep(0.25)
        raise SystemExit("fixture account button did not appear")

    def text(self, url: str) -> str:
        self.navigate(url)
        time.sleep(0.8)
        raw = self.evaluate("document.body ? document.body.innerText : ''")
        marker = '"value":"'
        if marker not in raw:
            return raw
        value = raw.split(marker, 1)[1]
        out = []
        escaped = False
        for character in value:
            if escaped:
                out.append({"n": "\n", "t": "\t", '"': '"', "\\": "\\"}.get(character, character))
                escaped = False
                continue
            if character == "\\":
                escaped = True
                continue
            if character == '"':
                break
            out.append(character)
        return "".join(out)

    def close(self) -> None:
        self.proc.terminate()
        try:
            self.proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            self.proc.kill()


def main() -> None:
    browser_path = find_browser()
    callback_server = ThreadingHTTPServer(("127.0.0.1", 0), Callback)
    callback_port = callback_server.server_address[1]
    if callback_port == 19427:
        raise SystemExit("callback landed on the service port")
    thread = threading.Thread(target=callback_server.serve_forever, daemon=True)
    thread.start()
    redirect = f"http://127.0.0.1:{callback_port}/ss-publish/callback"
    verifier = token(64)
    state = token(48)
    started = command(
        "auth.begin",
        {
            "transportProfile": "local-http-isolated-fixture",
            "issuer": ISSUER,
            "redirectUri": redirect,
            "codeChallengeMethod": "S256",
            "codeChallenge": pkce_challenge(verifier),
            "state": state,
            "clientId": CLIENT_ID,
            "scope": ["prototype:creator"],
        },
    )
    authorization = started["result"]["authorizationUrl"]
    if not authorization.startswith(f"{ORIGIN}/ss-publish/browser/authorize?"):
        raise SystemExit("authorization URL is not the fixture page")
    print("opened fixture authorize page")
    browser = Browser(browser_path)
    try:
        browser.navigate(authorization)
        browser.click_account("acct_test_owner_a")
        deadline = time.time() + 30
        while Callback.result is None and Callback.error is None and time.time() < deadline:
            time.sleep(0.1)
        if Callback.error:
            raise SystemExit(Callback.error)
        if Callback.result is None:
            raise SystemExit("fixture picker did not reach the callback")
        code, returned = Callback.result
        if returned != state:
            raise SystemExit("callback state did not match")
        print("affirm reached the callback with code and state only")
        exchanged = command(
            "auth.exchange",
            {
                "code": code,
                "codeVerifier": verifier,
                "redirectUri": redirect,
                "state": state,
                "issuer": ISSUER,
                "grantType": "authorization_code",
            },
        )
        credential = exchanged["result"]["credential"]
        account = exchanged["result"]["account"]
        if account.get("accountId") != "acct_test_owner_a":
            raise SystemExit("signed in as the wrong fixture account")
        kept = {
            "credentialId": credential["credentialId"],
            "secret": credential["secret"],
        }
        uploaded = command(
            "episode.upload",
            {
                "seriesId": "ser_test_lane",
                "episodeId": "ep_test_lane_01",
                "selection": {
                    "mode": "selected-episode-only",
                    "localStoryRef": "windows-runner",
                    "localEpisodeRef": "north-door",
                },
                "publicMetadata": {
                    "seriesTitle": "Fixture Lane",
                    "episodeTitle": "North door",
                    "synopsis": "Reviewed on the Windows runner.",
                    "language": "en",
                },
                "content": {
                    "kind": "prose",
                    "mediaType": "text/plain; charset=utf-8",
                    "text": PROSE,
                },
                "completion": "seal",
            },
            kept,
        )
        acknowledgement = uploaded["result"]["acknowledgement"]
        if acknowledgement.get("authoritative") is not False:
            raise SystemExit("upload acknowledgement was treated as status")
        preview = acknowledgement["browserPreviewUrl"]
        preview_text = browser.text(preview)
        if PROSE not in preview_text:
            raise SystemExit("preview did not show the uploaded paragraph")
        print("preview showed the uploaded paragraph")
        confirmed = command(
            "publication.confirm",
            {
                "seriesId": "ser_test_lane",
                "episodeId": "ep_test_lane_01",
                "revisionId": acknowledgement["revisionId"],
                "policyEligibility": "private-prototype-only",
            },
            kept,
        )
        reader = confirmed["result"]["acknowledgement"]["canonicalUrl"]
        publication = confirmed["result"]["acknowledgement"]["publicationId"]
        reader_text = browser.text(reader)
        if "TEST ONLY" not in reader_text or PROSE not in reader_text:
            raise SystemExit("reader did not show the published paragraph")
        print("reader showed the published paragraph")
        command(
            "publication.withdraw",
            {
                "seriesId": "ser_test_lane",
                "episodeId": "ep_test_lane_01",
                "publicationId": publication,
                "revisionId": acknowledgement["revisionId"],
            },
            kept,
        )
        withdrawn = browser.text(reader)
        if "Unavailable" not in withdrawn:
            raise SystemExit("withdrawn reader was still showing the paragraph")
        print("reader showed unavailable after withdraw")
    finally:
        browser.close()
        callback_server.shutdown()


if __name__ == "__main__":
    main()
