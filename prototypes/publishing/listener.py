from __future__ import annotations

import json
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from threading import BoundedSemaphore, Lock
from urllib.parse import parse_qsl

from prototypes.publishing.oracle import fresh_world
from prototypes.publishing.transport import SPEC, dispatch

HOST = "127.0.0.1"
PORT = 19427
MAX_IN_FLIGHT = 4
TIMEOUT_SECONDS = 15
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


class Listener(ThreadingHTTPServer):
    allow_reuse_address = True
    daemon_threads = True

    def __init__(self, world=None):
        assert_loopback(HOST)
        self.world = live_world() if world is None else world
        self.state_lock = Lock()
        self.slots = BoundedSemaphore(MAX_IN_FLIGHT)
        super().__init__((HOST, PORT), Handler)
        bound_host, bound_port = self.server_address[:2]
        if bound_host != HOST or bound_port != PORT:
            self.server_close()
            raise RuntimeError("bind was not 127.0.0.1:19427")
        self.timeout = TIMEOUT_SECONDS

    def serve(self):
        self.serve_forever(poll_interval=0.2)


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def do_GET(self):
        self._handle()

    def do_POST(self):
        self._handle()

    def do_PUT(self):
        self._handle()

    def log_message(self, fmt, *args):
        return

    def _handle(self):
        self.connection.settimeout(TIMEOUT_SECONDS)
        if not self.server.slots.acquire(blocking=False):
            self._write(503, {"content-type": "application/json; charset=utf-8", "cache-control": "no-store"}, _busy())
            return
        try:
            if self.headers.get("Transfer-Encoding"):
                self._write(400, {"content-type": "application/json; charset=utf-8", "cache-control": "no-store"}, _busy("invalid_request"))
                return
            length = int(self.headers.get("Content-Length", "0") or "0")
            if length < 0 or length > MAX_BODY:
                self.close_connection = True
                self._write(413, {"content-type": "application/json; charset=utf-8", "cache-control": "no-store"}, _busy("limit_exceeded"))
                return
            body = self.rfile.read(length) if length else b""
            path, _, raw_query = self.path.partition("?")
            query = dict(parse_qsl(raw_query, keep_blank_values=True))
            headers = {key.lower(): value for key, value in self.headers.items()}
            request = {"method": self.command, "path": path, "query": query, "headers": headers, "body": body}
            with self.server.state_lock:
                result = dispatch(self.server.world, request, None)
            self._write(result.status, result.headers, result.body)
        except Exception:
            self._write(500, {"content-type": "text/plain; charset=utf-8", "cache-control": "no-store"}, b"unavailable")
        finally:
            self.server.slots.release()

    def _write(self, status, headers, body):
        self.send_response(status)
        for key, value in headers.items():
            self.send_header(key, value)
        self.send_header("content-length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


def _busy(code="limit_exceeded"):
    return json.dumps(
        {
            "protocol": "scroll-studio.publishing.private-prototype.v1",
            "ok": False,
            "command": None,
            "idempotentReplay": False,
            "error": {"code": code, "retryable": False},
        }
    ).encode("utf-8")


def main():
    listener = Listener()
    print("listening http://127.0.0.1:19427", flush=True)
    listener.serve()


if __name__ == "__main__":
    main()
