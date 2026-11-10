"""Landing-page link discovery and a single loopback GET.

Discovered links are strings parsed from the landing response. They are not
fetched, and the GET is not browser-render evidence.
"""

from __future__ import annotations

import hashlib
import json
from html.parser import HTMLParser
from urllib.parse import parse_qsl, urljoin, urlsplit

import http.client

LOOPBACK_HOST = "127.0.0.1"
_READ_LIMIT = 1_000_000
_SAFE_HEADERS = (
    "content-type",
    "content-length",
    "cache-control",
    "referrer-policy",
    "x-content-type-options",
)
_SENSITIVE_QUERY = frozenset(
    {"code", "state", "secret", "token", "credential", "password", "auth", "access_token"}
)


class LandingError(Exception):
    def __init__(self, code: str, detail: str) -> None:
        super().__init__(detail)
        self.code = code
        self.detail = detail


class _Anchors(HTMLParser):
    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self.hrefs: list[str] = []

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        if tag != "a":
            return
        for key, value in attrs:
            if key.lower() == "href" and value:
                self.hrefs.append(value)


def discover_links(body: bytes, content_type: str, landing: str) -> list[str]:
    base = urlsplit(landing)
    if base.scheme != "http" or base.hostname != LOOPBACK_HOST or base.port is None:
        return []
    ctype = content_type.split(";", 1)[0].strip().lower()
    raw_links: list[str] = []
    if ctype in {"text/html", "application/xhtml+xml", ""}:
        parser = _Anchors()
        parser.feed(body.decode("utf-8", "replace"))
        raw_links.extend(parser.hrefs)
    if "json" in ctype or body.lstrip().startswith((b"{", b"[")):
        raw_links.extend(_json_strings(body))
    found: list[str] = []
    seen: set[str] = set()
    for raw in raw_links:
        if raw.lower().startswith(("javascript:", "data:", "vbscript:")):
            continue
        parts = urlsplit(urljoin(landing, raw))
        if parts.scheme != "http" or parts.hostname != LOOPBACK_HOST or parts.port != base.port:
            continue
        if parts.username or parts.password:
            continue
        query_keys = [key for key, _value in parse_qsl(parts.query, keep_blank_values=True)]
        if any(key.lower() in _SENSITIVE_QUERY for key in query_keys):
            cleaned = parts._replace(query="", fragment="").geturl()
        else:
            cleaned = parts._replace(fragment="").geturl()
        if cleaned not in seen:
            seen.add(cleaned)
            found.append(cleaned)
    return found


def fetch_landing(url: str, timeout: float = 5.0) -> dict[str, object]:
    parts = urlsplit(url)
    if (
        parts.scheme != "http"
        or parts.hostname != LOOPBACK_HOST
        or parts.port is None
        or parts.username
        or parts.password
        or parts.query
        or parts.fragment
    ):
        raise LandingError("non-loopback", "landing fetch refused")
    path = parts.path or "/"
    connection = http.client.HTTPConnection(LOOPBACK_HOST, parts.port, timeout=timeout)
    try:
        connection.request(
            "GET",
            path,
            headers={
                "Host": f"{LOOPBACK_HOST}:{parts.port}",
                "Connection": "close",
                "Accept": "text/html,application/json;q=0.9,*/*;q=0.1",
                "User-Agent": "scroll-studio-packaged-demo-harness",
            },
        )
        response = connection.getresponse()
        body = response.read(_READ_LIMIT + 1)
        truncated = len(body) > _READ_LIMIT
        if truncated:
            body = body[:_READ_LIMIT]
        content_type = response.headers.get("Content-Type", "")
        headers = {
            name: response.headers[name]
            for name in _SAFE_HEADERS
            if response.headers.get(name) is not None
        }
        landing = f"http://{LOOPBACK_HOST}:{parts.port}/"
        return {
            "status": "fetched",
            "httpStatus": response.status,
            "headers": headers,
            "bodySha256": hashlib.sha256(body).hexdigest(),
            "bodyLength": len(body),
            "bodyTruncated": truncated,
            "followedRedirect": False,
            "discoveredLinks": discover_links(body, content_type, landing),
            "notBrowserRender": True,
        }
    finally:
        connection.close()


def _json_strings(body: bytes) -> list[str]:
    try:
        parsed = json.loads(body.decode("utf-8"))
    except (UnicodeError, json.JSONDecodeError):
        return []
    found: list[str] = []

    def walk(value: object) -> None:
        if isinstance(value, str):
            found.append(value)
        elif isinstance(value, dict):
            for item in value.values():
                walk(item)
        elif isinstance(value, list):
            for item in value:
                walk(item)

    walk(parsed)
    return found
