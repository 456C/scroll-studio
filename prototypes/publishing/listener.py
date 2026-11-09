from __future__ import annotations

import json
import socket
import socketserver
import threading
from urllib.parse import parse_qsl

from prototypes.publishing.oracle import fresh_world
from prototypes.publishing.transport import SPEC, dispatch

HOST = "127.0.0.1"
PORT = 19427
MAX_IN_FLIGHT = 4
LISTEN_BACKLOG = 8
TIMEOUT_SECONDS = 15
HEADER_LIMIT = 8192
MAX_BODY = SPEC["command"]["maxBodyBytes"]
OPERATOR_ID = "acct_test_operator"


def live_world():
    world = fresh_world()
    world["accounts"][OPERATOR_ID] = {
        "accountId": OPERATOR_ID,
        "handle": "test-operator",
        "testOnly": True,
        "nonProductionLabel": "TEST-ONLY",
        "operator": True,
    }
    world["runtime"]["reports"] = []
    return world


def assert_loopback(host):
    if host != HOST:
        raise RuntimeError("listener binds 127.0.0.1 only")


class Listener(socketserver.TCPServer):
    allow_reuse_address = True
    request_queue_size = LISTEN_BACKLOG
    daemon_threads = True

    def __init__(self, world=None):
        assert_loopback(HOST)
        self.world = live_world() if world is None else world
        self.state_lock = threading.Lock()
        self.slots = threading.BoundedSemaphore(MAX_IN_FLIGHT)
        self.count_lock = threading.Lock()
        self.entered = 0
        self.timeout_seconds = TIMEOUT_SECONDS
        super().__init__((HOST, PORT), None)
        bound_host, bound_port = self.server_address[:2]
        if bound_host != HOST or bound_port != PORT:
            self.server_close()
            raise RuntimeError("bind was not 127.0.0.1:19427")

    def process_request(self, request, client_address):
        try:
            request.settimeout(self.timeout_seconds)
        except OSError:
            _close(request)
            return
        if not self.slots.acquire(blocking=False):
            _send(request, 503, _json_headers(), _busy("limit_exceeded", retryable=True))
            _close(request)
            return
        threading.Thread(target=self._serve_one, args=(request,), daemon=True).start()

    def _serve_one(self, conn):
        with self.count_lock:
            self.entered += 1
        try:
            _handle(self, conn)
        finally:
            with self.count_lock:
                self.entered -= 1
            self.slots.release()
            _close(conn)

    def serve(self):
        self.serve_forever(poll_interval=0.2)


def _handle(server, conn):
    try:
        head, rest = _read_until(conn, b"\r\n\r\n", HEADER_LIMIT)
        method, path, query, headers = _parse_request(head)
        if headers.get("transfer-encoding"):
            _send(conn, 400, _json_headers(), _busy("invalid_request"))
            return
        length = _content_length(headers)
        if length > MAX_BODY:
            _send(conn, 413, _json_headers(), _busy("limit_exceeded"))
            return
        if length == 0 and rest:
            _send(conn, 400, _json_headers(), _busy("invalid_request"))
            return
        body = rest
        if len(body) < length:
            body += _read_exact(conn, length - len(body))
        if len(body) != length:
            _send(conn, 400, _json_headers(), _busy("invalid_request"))
            return
        request = {"method": method, "path": path, "query": query, "headers": headers, "body": body}
        with server.state_lock:
            result = dispatch(server.world, request, None)
        _send(conn, result.status, result.headers, result.body)
    except (OSError, ValueError):
        try:
            _send(conn, 400, _json_headers(), _busy("invalid_request"))
        except OSError:
            return


def _parse_request(head):
    text = head.decode("iso-8859-1")
    lines = text.split("\r\n")
    parts = lines[0].split(" ")
    if len(parts) != 3 or not parts[2].startswith("HTTP/1."):
        raise ValueError("request line")
    method, target = parts[0], parts[1]
    if target.startswith("/") is False or target.startswith("//"):
        raise ValueError("target")
    path, _, raw_query = target.partition("?")
    seen = {}
    for line in lines[1:]:
        if not line:
            continue
        if line[0] in " \t" or ":" not in line:
            raise ValueError("header")
        name, value = line.split(":", 1)
        name = name.strip().lower()
        value = value.strip()
        if name == "" or name in seen and name in ("host", "content-length", "transfer-encoding"):
            raise ValueError("duplicate")
        seen[name] = value
    if "host" not in seen:
        raise ValueError("host")
    return method, path, dict(parse_qsl(raw_query, keep_blank_values=True)), seen


def _content_length(headers):
    raw = headers.get("content-length")
    if raw is None:
        return 0
    if raw != "0" and (raw.startswith("0") or not raw.isdigit()):
        raise ValueError("length")
    return int(raw)


def _read_until(conn, marker, limit):
    data = b""
    while marker not in data:
        if len(data) > limit:
            raise ValueError("headers")
        chunk = conn.recv(min(1024, limit + 1 - len(data)))
        if not chunk:
            raise ValueError("closed")
        data += chunk
    head, rest = data.split(marker, 1)
    if len(rest) > MAX_BODY:
        raise ValueError("body")
    return head, rest


def _read_exact(conn, length):
    data = b""
    while len(data) < length:
        chunk = conn.recv(min(65536, length - len(data)))
        if not chunk:
            raise ValueError("partial")
        data += chunk
    return data


def _send(conn, status, headers, body):
    reasons = {
        200: "OK",
        302: "Found",
        400: "Bad Request",
        401: "Unauthorized",
        403: "Forbidden",
        404: "Not Found",
        405: "Method Not Allowed",
        409: "Conflict",
        410: "Gone",
        413: "Content Too Large",
        415: "Unsupported Media Type",
        500: "Error",
        503: "Service Unavailable",
    }
    fields = {key.lower(): value for key, value in headers.items()}
    fields["connection"] = "close"
    fields["content-length"] = str(len(body))
    lines = [f"HTTP/1.1 {status} {reasons.get(status, 'Error')}"]
    lines.extend(f"{key}: {value}" for key, value in fields.items())
    conn.sendall(("\r\n".join(lines) + "\r\n\r\n").encode("iso-8859-1") + body)


def _close(conn):
    try:
        conn.shutdown(socket.SHUT_RDWR)
    except OSError:
        pass
    try:
        conn.close()
    except OSError:
        pass


def _json_headers():
    return {"content-type": "application/json; charset=utf-8", "cache-control": "no-store"}


def _busy(code="limit_exceeded", retryable=False):
    return json.dumps(
        {
            "protocol": "scroll-studio.publishing.private-prototype.v1",
            "ok": False,
            "command": None,
            "idempotentReplay": False,
            "error": {"code": code, "retryable": retryable},
        }
    ).encode("utf-8")


def main():
    listener = Listener()
    print("listening http://127.0.0.1:19427", flush=True)
    listener.serve()


if __name__ == "__main__":
    main()
