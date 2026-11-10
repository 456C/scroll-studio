"""Strict manifest gate for packaged-demo verification.

The gate reads local JSON and local files. It does not download, extract,
or launch a process.
"""

from __future__ import annotations

import hashlib
import hmac
import json
import os
import re
import stat
from dataclasses import dataclass
from urllib.parse import urlsplit

CONTRACT = "scroll-studio.packaged-demo.manifest.v1"
ENVIRONMENT_KIND = "isolated-hosted-windows"
DISCOVERY = "natural-links-from-landing"
LOOPBACK_HOST = "127.0.0.1"

_COMMIT = re.compile(r"^[0-9a-f]{40}\Z")
_SHA256 = re.compile(r"^[0-9a-f]{64}\Z")
_SHELLS = frozenset(
    {
        "sh",
        "bash",
        "dash",
        "zsh",
        "ksh",
        "fish",
        "cmd",
        "cmd.exe",
        "powershell",
        "powershell.exe",
        "pwsh",
        "pwsh.exe",
        "wscript",
        "wscript.exe",
        "cscript",
        "cscript.exe",
    }
)
_MAX_MANIFEST_BYTES = 1_000_000


class ManifestError(Exception):
    def __init__(self, code: str, detail: str) -> None:
        super().__init__(detail)
        self.code = code
        self.detail = detail


@dataclass(frozen=True)
class ArchivePin:
    path: str
    sha256: str


@dataclass(frozen=True)
class ComponentPin:
    commit: str
    archive: ArchivePin


@dataclass(frozen=True)
class Manifest:
    synthetic: bool
    app: ComponentPin
    service: ComponentPin
    working_directory: str
    argv: tuple[str, ...]
    host: str
    port: int
    landing_url: str
    discovery: str
    route_paths: tuple[str, ...]
    environment_kind: str
    sanctioned: bool


def load_manifest_document(path: str) -> object:
    try:
        fd = os.open(path, os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0))
    except OSError as exc:
        raise ManifestError("malformed", "manifest path could not be opened") from exc
    try:
        info = os.fstat(fd)
        if not stat.S_ISREG(info.st_mode):
            raise ManifestError("malformed", "manifest path is not a regular file")
        if info.st_size > _MAX_MANIFEST_BYTES:
            raise ManifestError("malformed", "manifest exceeds 1000000 bytes")
        chunks: list[bytes] = []
        remaining = _MAX_MANIFEST_BYTES + 1
        while remaining > 0:
            block = os.read(fd, min(1024 * 1024, remaining))
            if not block:
                break
            chunks.append(block)
            remaining -= len(block)
        raw = b"".join(chunks)
    finally:
        os.close(fd)
    if len(raw) > _MAX_MANIFEST_BYTES:
        raise ManifestError("malformed", "manifest exceeds 1000000 bytes")
    try:
        text = raw.decode("utf-8")
    except UnicodeError as exc:
        raise ManifestError("malformed", "manifest is not utf-8 json") from exc
    try:
        return json.loads(text, object_pairs_hook=_reject_duplicate_keys)
    except ValueError as exc:
        raise ManifestError("malformed", "manifest is not utf-8 json") from exc


def validate_manifest(document: object) -> Manifest:
    root = _object(
        document,
        {"contract", "synthetic", "provenance", "lifecycle", "routes", "environment"},
        "manifest",
    )
    if root["contract"] != CONTRACT:
        raise ManifestError("malformed", "manifest contract id is not recognized")
    synthetic = _bool(root["synthetic"], "synthetic")
    provenance = _object(root["provenance"], {"app", "service"}, "provenance")
    app = _component(provenance["app"], "app")
    service = _component(provenance["service"], "service")
    lifecycle = _object(
        root["lifecycle"],
        {"workingDirectory", "argv", "loopback"},
        "lifecycle",
    )
    loopback = _object(lifecycle["loopback"], {"host", "port"}, "loopback")
    if loopback["host"] != LOOPBACK_HOST:
        raise ManifestError("malformed", "loopback host must be 127.0.0.1")
    port = _port(loopback["port"])
    argv = _argv(lifecycle["argv"])
    working_directory = _absolute(lifecycle["workingDirectory"], "working directory")
    routes = _object(root["routes"], {"landingUrl", "discovery", "paths"}, "routes")
    if routes["discovery"] != DISCOVERY:
        raise ManifestError("malformed", "routes.discovery is not natural-links-from-landing")
    landing = _landing(routes["landingUrl"], port)
    route_paths = _route_paths(routes["paths"])
    environment = _object(root["environment"], {"kind", "sanctioned"}, "environment")
    if environment["kind"] != ENVIRONMENT_KIND:
        raise ManifestError("malformed", "environment.kind is not isolated-hosted-windows")
    sanctioned = _bool(environment["sanctioned"], "environment.sanctioned")
    if synthetic and sanctioned:
        raise ManifestError("malformed", "a synthetic manifest cannot be sanctioned")
    return Manifest(
        synthetic=synthetic,
        app=app,
        service=service,
        working_directory=working_directory,
        argv=argv,
        host=LOOPBACK_HOST,
        port=port,
        landing_url=landing,
        discovery=DISCOVERY,
        route_paths=route_paths,
        environment_kind=ENVIRONMENT_KIND,
        sanctioned=sanctioned,
    )


def verify_archives(manifest: Manifest) -> None:
    _verify_one("app", manifest.app.archive)
    _verify_one("service", manifest.service.archive)


def inspect_working_directory(path: str, archive_paths: tuple[str, ...]) -> None:
    if os.path.islink(path):
        raise ManifestError("unsafe-path", "working directory is a symlink")
    if not os.path.isdir(path):
        raise ManifestError("missing-provenance", "working directory is not an existing directory")
    real = os.path.realpath(path)
    for archive in archive_paths:
        if os.path.realpath(archive) == real:
            raise ManifestError("unsafe-path", "working directory is an archive path")


def hash_file(path: str) -> str:
    _assert_regular_file(path)
    flags = os.O_RDONLY
    if hasattr(os, "O_NOFOLLOW"):
        flags |= os.O_NOFOLLOW
    try:
        fd = os.open(path, flags)
    except OSError as exc:
        raise ManifestError("unsafe-path", "archive path could not be opened") from exc
    digest = hashlib.sha256()
    try:
        if not stat.S_ISREG(os.fstat(fd).st_mode):
            raise ManifestError("unsafe-path", "archive path is not a regular file")
        while True:
            block = os.read(fd, 1024 * 1024)
            if not block:
                break
            digest.update(block)
    finally:
        os.close(fd)
    return digest.hexdigest()


def _verify_one(label: str, archive: ArchivePin) -> None:
    try:
        actual = hash_file(archive.path)
    except ManifestError:
        raise
    if not hmac.compare_digest(actual, archive.sha256):
        raise ManifestError(
            "hash-mismatch",
            f"{label} archive sha256 {actual} != {archive.sha256}",
        )


def _assert_regular_file(path: str) -> None:
    if os.path.islink(path):
        raise ManifestError("unsafe-path", "archive path is a symlink")
    if not os.path.isfile(path):
        raise ManifestError("missing-artifact", "archive path is not a regular file")


def _reject_duplicate_keys(pairs: list[tuple[str, object]]) -> dict[str, object]:
    document: dict[str, object] = {}
    for key, value in pairs:
        if key in document:
            raise ValueError("duplicate key")
        document[key] = value
    return document


def _object(value: object, keys: set[str], label: str) -> dict[str, object]:
    if not isinstance(value, dict):
        raise ManifestError("malformed", f"{label} must be an object")
    present = set(value)
    missing = keys - present
    if missing & {"app", "service", "commit", "archive", "path", "sha256", "workingDirectory", "paths"}:
        raise ManifestError("missing-provenance", f"{label} is missing exact provenance")
    if present != keys:
        raise ManifestError("malformed", f"{label} keys do not match the contract")
    return value


def _component(value: object, label: str) -> ComponentPin:
    body = _object(value, {"commit", "archive"}, label)
    commit = body["commit"]
    if not isinstance(commit, str) or not _COMMIT.fullmatch(commit):
        raise ManifestError("missing-provenance", f"{label} commit is not 40 lowercase hex characters")
    archive = _object(body["archive"], {"path", "sha256"}, f"{label} archive")
    digest = archive["sha256"]
    if not isinstance(digest, str) or not _SHA256.fullmatch(digest):
        raise ManifestError("missing-provenance", f"{label} archive sha256 is not 64 lowercase hex characters")
    return ComponentPin(commit=commit, archive=ArchivePin(path=_absolute(archive["path"], f"{label} archive path"), sha256=digest))


def _argv(value: object) -> tuple[str, ...]:
    if isinstance(value, str) or not isinstance(value, list):
        raise ManifestError("malformed", "start command must be an argv list")
    argv: list[str] = []
    for item in value:
        if not isinstance(item, str) or not item or any(ord(ch) < 32 for ch in item):
            raise ManifestError("malformed", "argv entries must be non-empty strings")
        argv.append(item)
    if not argv:
        raise ManifestError("malformed", "start command must be an argv list")
    if _program_name(argv[0]) in _SHELLS:
        raise ManifestError("malformed", "start command must not be a shell")
    return tuple(argv)


def _program_name(argv0: str) -> str:
    return os.path.basename(argv0.replace("\\", "/")).lower()


def _route_paths(value: object) -> tuple[str, ...]:
    if not isinstance(value, list) or not value:
        raise ManifestError("missing-provenance", "owner routes are missing")
    found: list[str] = []
    seen: set[str] = set()
    for item in value:
        if not isinstance(item, str) or any(ord(ch) < 32 or ch in "\\?#%" for ch in item):
            raise ManifestError("malformed", "owner route must be a plain absolute path")
        if not item.startswith("/") or "://" in item or item.startswith("//"):
            raise ManifestError("malformed", "owner route must be a plain absolute path")
        parts = item.split("/")
        if any(part in ("", ".", "..") for part in parts[1:]):
            raise ManifestError("malformed", "owner route is not a normalized path")
        if item in seen:
            raise ManifestError("malformed", "owner routes must be unique")
        seen.add(item)
        found.append(item)
    return tuple(found)


def _landing(value: object, port: int) -> str:
    if not isinstance(value, str):
        raise ManifestError("malformed", "landing url must be a string")
    parts = urlsplit(value)
    if parts.scheme != "http" or parts.hostname != LOOPBACK_HOST:
        raise ManifestError("malformed", "landing url must use http://127.0.0.1")
    if parts.port != port:
        raise ManifestError("malformed", "landing url port must match the loopback port")
    if parts.username or parts.password or parts.query or parts.fragment:
        raise ManifestError("malformed", "landing url must not carry userinfo, query, or fragment")
    if parts.path not in ("", "/"):
        raise ManifestError("malformed", "landing url path must be /")
    return f"http://{LOOPBACK_HOST}:{port}/"


def _port(value: object) -> int:
    if isinstance(value, bool) or not isinstance(value, int) or not 1 <= value <= 65535:
        raise ManifestError("malformed", "loopback port must be an integer from 1 to 65535")
    return value


def _bool(value: object, label: str) -> bool:
    if not isinstance(value, bool):
        raise ManifestError("malformed", f"{label} must be a boolean")
    return value


def _absolute(value: object, label: str) -> str:
    if not isinstance(value, str) or not value or any(ord(ch) < 32 for ch in value):
        raise ManifestError("malformed", f"{label} must be a non-empty string")
    if not os.path.isabs(value):
        raise ManifestError("malformed", f"{label} must be absolute")
    return value
