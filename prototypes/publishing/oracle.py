from __future__ import annotations

import base64
import copy
import hashlib
import hmac
import json
import struct
import zlib
from datetime import datetime, timedelta, timezone
from pathlib import Path
from urllib.parse import parse_qsl, urlencode, urlsplit

from prototypes.publishing.schema_validate import validate

ROOT = Path(__file__).resolve().parents[2]
PROTOCOL_PATH = ROOT / "contracts" / "publishing" / "v1" / "protocol.json"
SCHEMA_PATH = ROOT / "contracts" / "publishing" / "v1" / "envelope.schema.json"
WORLD_PATH = ROOT / "prototypes" / "publishing" / "fixtures" / "world.json"

PROTOCOL = json.loads(PROTOCOL_PATH.read_text(encoding="utf-8"))
SCHEMA = json.loads(SCHEMA_PATH.read_text(encoding="utf-8"))
LIMITS = PROTOCOL["limits"]

BODY_SCHEMAS = {
    "auth.begin": "beginBody",
    "auth.cancel": "cancelAuthBody",
    "auth.exchange": "exchangeBody",
    "auth.revoke": "emptyBody",
    "account.discover": "emptyBody",
    "episode.upload": "uploadBody",
    "episode.preview": "previewBody",
    "publication.confirm": "confirmBody",
    "publication.withdraw": "withdrawBody",
    "publication.read": "readBody",
    "operation.cancel": "operationCancelBody",
}

_CACHED_WORLD = json.loads(WORLD_PATH.read_text(encoding="utf-8"))


def fresh_world():
    loaded = copy.deepcopy(_CACHED_WORLD)
    _check_world(loaded)
    accounts = {item["accountId"]: item for item in loaded["accounts"]}
    episodes = {}
    for series in loaded["series"]:
        ordinals = [episode["ordinal"] for episode in series["episodes"]]
        if ordinals != list(range(1, len(ordinals) + 1)):
            raise ValueError(f"episode ordinals for {series['seriesId']} are not 1..n")
        for episode in series["episodes"]:
            episodes[episode["episodeId"]] = {
                "seriesId": series["seriesId"],
                "ownerAccountId": series["ownerAccountId"],
                "seriesTitle": series["title"],
                "episodeId": episode["episodeId"],
                "ordinal": episode["ordinal"],
                "title": episode["title"],
            }
    return {
        "transportProfile": loaded["transportProfile"],
        "accounts": accounts,
        "series": {series["seriesId"]: series for series in loaded["series"]},
        "episodes": episodes,
        "runtime": {
            "records": {},
            "by_state": {},
            "by_code": {},
            "credentials": {},
            "operations": {},
            "counters": {"revision": 0, "publication": 0, "code": 0, "credential": 0},
        },
    }


def accept_client_storage(spec):
    if not isinstance(spec, dict):
        return "unknown_mode"
    mode = spec.get("mode")
    if mode in PROTOCOL["clientStorage"]["forbiddenModes"]:
        return "plaintext_forbidden"
    expected = PROTOCOL["clientStorage"]["modes"].get(mode)
    if expected is None:
        return "unknown_mode"
    allowed = {"mode", "plaintextFallback"}
    if expected["label"] is not None:
        allowed.add("label")
    if expected["persistence"] is not None:
        allowed.add("persistence")
    if spec.get("plaintextFallback") is not False:
        return "plaintext_fallback"
    if expected["label"] is not None and spec.get("label") != expected["label"]:
        return "missing_label"
    if expected["persistence"] is not None and spec.get("persistence") != expected["persistence"]:
        return "persistence"
    if set(spec) != allowed:
        return "unknown_mode"
    return None


def callback_query_allowed(keys):
    forbidden = set(PROTOCOL["forbiddenCallbackParams"])
    present = set(keys)
    if present & forbidden or ("code" in present and "error" in present):
        return False
    if present == {"code", "state"}:
        return True
    return "error" in present and "state" in present and present <= {"error", "error_description", "state"}


def loopback_bind_allowed(host, profile):
    return profile == PROTOCOL["transport"]["enabledProfile"] and host == "127.0.0.1"


def pkce_s256(verifier: str) -> str:
    digest = hashlib.sha256(verifier.encode("ascii")).digest()
    return base64.urlsafe_b64encode(digest).rstrip(b"=").decode("ascii")


def validate_redirect_uri(uri: str):
    parts = urlsplit(uri)
    redirect = PROTOCOL["redirect"]
    if parts.scheme != redirect["scheme"] or parts.hostname != redirect["host"]:
        return "redirect_rejected"
    if parts.username or parts.password or parts.query or parts.fragment:
        return "redirect_rejected"
    if parts.port is None or parts.port in redirect["forbidPorts"] or not 1 <= parts.port <= 65535:
        return "redirect_rejected"
    if parts.path != redirect["path"]:
        return "redirect_rejected"
    if not loopback_bind_allowed(parts.hostname, PROTOCOL["transport"]["enabledProfile"]):
        return "redirect_rejected"
    return None


def validate_issuer(issuer: str):
    if issuer != PROTOCOL["issuer"]["exact"]:
        return "issuer_rejected"
    parts = urlsplit(issuer)
    if not loopback_bind_allowed(parts.hostname, PROTOCOL["transport"]["enabledProfile"]):
        return "issuer_rejected"
    return None


def apply(world, envelope):
    command = envelope.get("command") if isinstance(envelope, dict) else None
    if not isinstance(command, str):
        command = None
    if not isinstance(envelope, dict) or validate(SCHEMA, envelope):
        return _fail(command, "invalid_request", "schema")
    if world["transportProfile"] != PROTOCOL["transport"]["enabledProfile"]:
        return _fail(command, "production_transport_not_enabled")
    if command not in PROTOCOL["commands"]:
        return _fail(command, "unknown_command")
    body = envelope["body"]
    if validate({"$ref": f"#/$defs/{BODY_SCHEMAS[command]}"}, body, SCHEMA):
        return _fail(command, "invalid_request", "schema")
    fixture = envelope.get("fixture") or {}
    pause = bool(fixture.get("pauseBeforeCommit", False))
    clock = _parse_clock(fixture.get("clock") or PROTOCOL["defaultClock"])
    if clock is None:
        return _fail(command, "invalid_request", "schema")
    if pause and command not in PROTOCOL["pausableCommands"]:
        return _fail(command, "invalid_request", "schema")
    if command == "episode.upload" and body["completion"] not in ("seal", "defer"):
        return _fail(command, "invalid_request", "schema")

    if command in PROTOCOL["credentialRequired"]:
        if "credential" not in envelope:
            return _fail(command, "unauthorized")
    elif "credential" in envelope:
        return _fail(command, "invalid_request", "schema")

    if command in PROTOCOL["mutatingCommands"]:
        return _mutating(world, envelope, command, body, clock, pause)
    live, problem = _live_credential(world, envelope, clock)
    if problem:
        return _fail(command, problem)
    return _dispatch(world, command, body, live, clock, envelope)


def _mutating(world, envelope, command, body, clock, pause):
    key = envelope["idempotencyKey"]
    identity = _identity_body(command, body)
    digest = _hash(command, identity, envelope.get("credential"))
    existing = world["runtime"]["operations"].get(key)
    if existing is not None:
        return _existing(world, envelope, existing, digest, clock, pause, command, body)
    if command in PROTOCOL["credentialRequired"]:
        live, problem = _live_credential(world, envelope, clock)
        if problem:
            return _fail(command, problem)
    else:
        live = None
    if _held(command, body, pause):
        prepared = _prepare(world, command, body, live, clock)
        if prepared[0] == "error":
            _store(world, key, digest, command, body, live, prepared[1])
            return prepared[1]
        _store(world, key, digest, command, body, live, None, status="in_progress")
        return _ok(command, {"operation": "in_progress", "idempotencyKey": key})
    response = _dispatch(world, command, body, live, clock, envelope)
    _store(world, key, digest, command, body, live, response)
    return response


def _existing(world, envelope, existing, digest, clock, pause, command, body):
    if not _replay_secret_ok(world, envelope, existing):
        return _fail(command, "unauthorized")
    if existing["hash"] != digest:
        return _fail(command, "idempotency_conflict")
    if existing["status"] == "cancelled":
        return _replay(existing["response"])
    if existing["status"] == "committed":
        return _replay(existing["response"])
    if _held(command, body, pause):
        return _replay(_ok(command, {"operation": "in_progress", "idempotencyKey": envelope["idempotencyKey"]}))
    live, problem = _live_credential(world, envelope, clock)
    if problem:
        return _fail(command, problem)
    response = _dispatch(world, command, body, live, clock, envelope)
    existing["status"] = "committed"
    existing["response"] = response
    return response


def _held(command, body, pause):
    if pause:
        return True
    return command == "episode.upload" and body.get("completion") == "defer"


def _dispatch(world, command, body, cred, clock, envelope):
    if command == "auth.begin":
        return _begin(world, body, clock)
    if command == "auth.cancel":
        return _cancel_auth(world, body)
    if command == "auth.exchange":
        return _exchange(world, body, clock)
    if command == "auth.revoke":
        return _revoke(cred)
    if command == "account.discover":
        return _ok(command, {"account": _account_view(world, cred["accountId"])})
    if command == "episode.upload":
        return _upload(world, cred, body)
    if command == "episode.preview":
        return _preview(world, cred, body)
    if command == "publication.confirm":
        return _confirm(world, cred, body)
    if command == "publication.withdraw":
        return _withdraw(world, cred, body)
    if command == "publication.read":
        return _read(world, cred, body)
    if command == "operation.cancel":
        return _cancel_operation(world, cred, body)
    return _fail(command, "unknown_command")


def _prepare(world, command, body, cred, clock):
    if command == "episode.upload":
        return _prepare_upload(world, cred, body)
    if command == "publication.confirm":
        return _prepare_confirm(world, cred, body)
    if command == "publication.withdraw":
        return _prepare_withdraw(world, cred, body)
    return ("ok", None)


def _begin(world, body, clock):
    if body["transportProfile"] != PROTOCOL["transport"]["enabledProfile"]:
        return _fail("auth.begin", "production_transport_not_enabled")
    issuer_error = validate_issuer(body["issuer"])
    if issuer_error:
        return _fail("auth.begin", issuer_error)
    redirect_error = validate_redirect_uri(body["redirectUri"])
    if redirect_error:
        return _fail("auth.begin", redirect_error)
    if body["codeChallengeMethod"] != "S256":
        return _fail("auth.begin", "pkce_rejected")
    if body["clientId"] != PROTOCOL["clientId"]:
        return _fail("auth.begin", "unauthorized")
    account = world["accounts"].get(body["loginHint"])
    if account is None or account.get("nonProductionLabel") != "TEST-ONLY" or account.get("testOnly") is not True:
        return _fail("auth.begin", "unauthorized")
    if body["scope"] != PROTOCOL["scope"]:
        return _fail("auth.begin", "invalid_request", "schema")
    if body["state"] in world["runtime"]["by_state"]:
        return _fail("auth.begin", "invalid_request", "schema")
    code = _next(world, "code", "code_test_")
    expires = clock + timedelta(seconds=LIMITS["authorizationTtlSeconds"])
    world["runtime"]["by_code"][code] = {
        "state": body["state"],
        "challenge": body["codeChallenge"],
        "redirectUri": body["redirectUri"],
        "issuer": body["issuer"],
        "accountId": account["accountId"],
        "expiresAt": expires,
        "status": "pending",
    }
    world["runtime"]["by_state"][body["state"]] = code
    redirect = body["redirectUri"] + "?" + urlencode([("code", code), ("state", body["state"])])
    authorize = body["issuer"] + "/authorize?" + urlencode(
        [
            ("response_type", "code"),
            ("client_id", body["clientId"]),
            ("redirect_uri", body["redirectUri"]),
            ("scope", "prototype:creator"),
            ("state", body["state"]),
            ("code_challenge", body["codeChallenge"]),
            ("code_challenge_method", "S256"),
        ]
    )
    return _ok(
        "auth.begin",
        {
            "authorizationUrl": authorize,
            "expiresAt": _fmt(expires),
            "issuer": body["issuer"],
            "issuerRole": PROTOCOL["issuer"]["role"],
            "distinctFromProductionProvider": True,
            "identityProof": PROTOCOL["identityProof"],
            "state": body["state"],
            "clientId": body["clientId"],
            "testOnlyRedirect": {"nonProductionLabel": "TEST-ONLY", "url": redirect},
        },
    )


def _cancel_auth(world, body):
    code = world["runtime"]["by_state"].get(body["state"])
    txn = world["runtime"]["by_code"].get(code) if code else None
    if txn is None or txn["issuer"] != body["issuer"] or txn["redirectUri"] != body["redirectUri"]:
        return _fail("auth.cancel", "not_found")
    if txn["status"] == "consumed":
        return _fail("auth.cancel", "already_terminal")
    txn["status"] = "cancelled"
    return _ok("auth.cancel", {"cancelled": True, "state": body["state"]})


def _exchange(world, body, clock):
    if body.get("grantType", "authorization_code") != "authorization_code":
        return _fail("auth.exchange", "unsupported_grant")
    txn = world["runtime"]["by_code"].get(body["code"])
    if txn is None:
        return _fail("auth.exchange", "unauthorized")
    if txn["status"] == "consumed":
        return _fail("auth.exchange", "code_replayed")
    if txn["status"] == "cancelled":
        return _fail("auth.exchange", "authorization_cancelled")
    if clock >= txn["expiresAt"]:
        txn["status"] = "consumed"
        return _fail("auth.exchange", "authorization_expired")
    if body["issuer"] != txn["issuer"] or validate_issuer(body["issuer"]):
        txn["status"] = "consumed"
        return _fail("auth.exchange", "issuer_rejected")
    if body["redirectUri"] != txn["redirectUri"] or validate_redirect_uri(body["redirectUri"]):
        txn["status"] = "consumed"
        return _fail("auth.exchange", "redirect_rejected")
    if body["state"] != txn["state"]:
        txn["status"] = "consumed"
        return _fail("auth.exchange", "state_mismatch")
    verifier = body["codeVerifier"]
    if not _verifier_ok(verifier) or not hmac.compare_digest(pkce_s256(verifier), txn["challenge"]):
        txn["status"] = "consumed"
        return _fail("auth.exchange", "pkce_rejected")
    txn["status"] = "consumed"
    credential = _issue_credential(world, txn["accountId"], clock)
    return _ok(
        "auth.exchange",
        {"credential": _credential_view(credential), "account": _account_view(world, txn["accountId"])},
    )


def _revoke(cred):
    cred["revoked"] = True
    return _ok("auth.revoke", {"revoked": True, "credentialId": cred["credentialId"]})


def _upload(world, cred, body):
    prepared = _prepare_upload(world, cred, body)
    if prepared[0] == "error":
        return prepared[1]
    job = prepared[1]
    record = _record(world, job["episode"])
    revision_id = _next(world, "revision", "rev_test_", width=4)
    record["revisions"].append(
        {
            "revisionId": revision_id,
            "contentSha256": job["contentSha256"],
            "publicMetadata": copy.deepcopy(job["metadata"]),
            "content": job["content"],
        }
    )
    if record["status"] is None:
        record["status"] = "draft"
    return _ok(
        "episode.upload",
        {
            "private": True,
            "localOriginals": "untouched",
            "seriesId": job["episode"]["seriesId"],
            "episodeId": job["episode"]["episodeId"],
            "revisionId": revision_id,
            "contentSha256": job["contentSha256"],
            "episodeStatus": record["status"],
            "publishedRevisionId": record["publishedRevisionId"],
        },
    )


def _prepare_upload(world, cred, body):
    episode, problem = _owned_episode(world, cred, body["seriesId"], body["episodeId"])
    if problem:
        return ("error", _fail("episode.upload", problem))
    metadata = body["publicMetadata"]
    if metadata["seriesTitle"] != episode["seriesTitle"] or metadata["episodeTitle"] != episode["title"]:
        return ("error", _fail("episode.upload", "metadata_rejected"))
    checked = _check_content(body["content"])
    if checked[0] == "error":
        return ("error", _fail("episode.upload", checked[1], checked[2]))
    content_sha = checked[1]
    if "expectedSha256" in body and not hmac.compare_digest(body["expectedSha256"], content_sha):
        return ("error", _fail("episode.upload", "media_rejected", "hash"))
    return (
        "ok",
        {
            "episode": episode,
            "metadata": copy.deepcopy(metadata),
            "content": copy.deepcopy(body["content"]),
            "contentSha256": content_sha,
        },
    )


def _preview(world, cred, body):
    episode, problem = _owned_episode(world, cred, body["seriesId"], body["episodeId"])
    if problem:
        return _fail("episode.preview", problem)
    revision = _revision(world, episode["episodeId"], body["revisionId"])
    if revision is None:
        return _fail("episode.preview", "not_found")
    return _ok(
        "episode.preview",
        {
            "private": True,
            "localOriginals": "untouched",
            "seriesId": episode["seriesId"],
            "episodeId": episode["episodeId"],
            "revisionId": revision["revisionId"],
            "contentSha256": revision["contentSha256"],
            "publicMetadata": copy.deepcopy(revision["publicMetadata"]),
            "content": copy.deepcopy(revision["content"]),
        },
    )


def _confirm(world, cred, body):
    prepared = _prepare_confirm(world, cred, body)
    if prepared[0] == "error":
        return prepared[1]
    episode = prepared[1]
    record = _record(world, episode)
    if record["publicationId"] is None:
        record["publicationId"] = _next(world, "publication", "pub_test_", width=4)
        record["canonicalUrl"] = (
            f"http://127.0.0.1:{PROTOCOL['reader']['port']}{PROTOCOL['reader']['pathPrefix']}{record['publicationId']}"
        )
    record["publishedRevisionId"] = body["revisionId"]
    record["status"] = "published"
    return _ok(
        "publication.confirm",
        {
            "status": "published",
            "seriesId": episode["seriesId"],
            "episodeId": episode["episodeId"],
            "revisionId": body["revisionId"],
            "publicationId": record["publicationId"],
            "canonicalUrl": record["canonicalUrl"],
            "publicInternet": False,
            "policyEligibility": PROTOCOL["policyEligibilityPrivate"],
        },
    )


def _prepare_confirm(world, cred, body):
    if body["policyEligibility"] != PROTOCOL["policyEligibilityPrivate"]:
        return ("error", _fail("publication.confirm", "public_launch_gate"))
    episode, problem = _owned_episode(world, cred, body["seriesId"], body["episodeId"])
    if problem:
        return ("error", _fail("publication.confirm", problem))
    record = world["runtime"]["records"].get(episode["episodeId"])
    head = record["revisions"][-1]["revisionId"] if record and record["revisions"] else None
    if head is None:
        return ("error", _fail("publication.confirm", "not_found"))
    if body["revisionId"] != head:
        if _revision(world, episode["episodeId"], body["revisionId"]) is None:
            return ("error", _fail("publication.confirm", "not_found"))
        return ("error", _fail("publication.confirm", "stale_revision"))
    return ("ok", episode)


def _withdraw(world, cred, body):
    prepared = _prepare_withdraw(world, cred, body)
    if prepared[0] == "error":
        return prepared[1]
    record = prepared[1]
    record["status"] = "withdrawn"
    return _ok(
        "publication.withdraw",
        {
            "status": "withdrawn",
            "seriesId": body["seriesId"],
            "episodeId": body["episodeId"],
            "revisionId": record["publishedRevisionId"],
            "publicationId": record["publicationId"],
            "canonicalUrl": record["canonicalUrl"],
            "publicInternet": False,
        },
    )


def _prepare_withdraw(world, cred, body):
    episode, problem = _owned_episode(world, cred, body["seriesId"], body["episodeId"])
    if problem:
        return ("error", _fail("publication.withdraw", problem))
    record = world["runtime"]["records"].get(episode["episodeId"])
    if record is None or record["publicationId"] is None:
        return ("error", _fail("publication.withdraw", "not_found"))
    if body["publicationId"] != record["publicationId"]:
        return ("error", _fail("publication.withdraw", "not_found"))
    if body["revisionId"] != record["publishedRevisionId"]:
        return ("error", _fail("publication.withdraw", "stale_revision"))
    if record["status"] not in ("published", "withdrawn"):
        return ("error", _fail("publication.withdraw", "not_found"))
    return ("ok", record)


def _read(world, cred, body):
    url = body["canonicalUrl"]
    if _asset_shaped(url):
        return _fail("publication.read", "asset_not_addressable")
    found = None
    for record in world["runtime"]["records"].values():
        if record.get("canonicalUrl") == url:
            found = record
            break
    if found is None:
        return _fail("publication.read", "not_found")
    episode = world["episodes"][found["episodeId"]]
    if episode["ownerAccountId"] != cred["accountId"]:
        return _fail("publication.read", "not_owner")
    if found["status"] == "withdrawn":
        return _fail("publication.read", "withdrawn")
    if found["status"] != "published" or found["publishedRevisionId"] is None:
        return _fail("publication.read", "not_found")
    revision = _revision(world, found["episodeId"], found["publishedRevisionId"])
    return _ok(
        "publication.read",
        {
            "status": "published",
            "publicationId": found["publicationId"],
            "canonicalUrl": found["canonicalUrl"],
            "revisionId": revision["revisionId"],
            "contentSha256": revision["contentSha256"],
            "publicMetadata": copy.deepcopy(revision["publicMetadata"]),
            "presentation": {
                "viewport": "phone",
                "widthPx": PROTOCOL["phoneViewportWidthPx"],
                "ordered": True,
            },
            "publicInternet": False,
            "content": copy.deepcopy(revision["content"]),
        },
    )


def _cancel_operation(world, cred, body):
    target_key = body["targetIdempotencyKey"]
    target = world["runtime"]["operations"].get(target_key)
    if target is None:
        return _fail("operation.cancel", "not_found")
    if target.get("accountId") != cred["accountId"]:
        return _fail("operation.cancel", "not_owner")
    if target["status"] == "committed":
        return _fail("operation.cancel", "already_terminal")
    if target["status"] == "in_progress":
        target["status"] = "cancelled"
        target["response"] = _fail(target["command"], "cancelled")
    return _ok(
        "operation.cancel",
        {"targetIdempotencyKey": target_key, "targetStatus": "cancelled"},
    )


def _check_content(content):
    if content["kind"] == "prose":
        return _check_prose(content)
    if content["kind"] != "comic":
        return ("error", "media_rejected", "type")
    slices = content["slices"]
    ordinals = [item["ordinal"] for item in slices]
    if ordinals != list(range(1, len(slices) + 1)):
        return ("error", "media_rejected", "order")
    decoded = []
    total = 0
    for item in slices:
        checked = _decode_slice(item)
        if checked[0] == "error":
            return checked
        total += len(checked[1])
        if total > LIMITS["maxEpisodeBytes"]:
            return ("error", "limit_exceeded", "limit")
        decoded.append((item, checked[1]))
    for item, raw in decoded:
        checked = _inspect_slice(item, raw)
        if checked[0] == "error":
            return checked
    return ("ok", hashlib.sha256(b"".join(raw for _item, raw in decoded)).hexdigest())


def _check_prose(content):
    if content["mediaType"] != "text/plain; charset=utf-8":
        return ("error", "media_rejected", "type")
    text = content["text"]
    if text.startswith("\ufeff"):
        return ("error", "media_rejected", "bom")
    for char in text:
        code = ord(char)
        if code in (9, 10):
            continue
        if code == 0x7F or code < 32 or 0xD800 <= code <= 0xDFFF:
            return ("error", "media_rejected", "controls")
    try:
        raw = text.encode("utf-8")
    except UnicodeEncodeError:
        return ("error", "media_rejected", "utf8")
    if len(raw) > LIMITS["maxProseBytes"]:
        return ("error", "limit_exceeded", "limit")
    return ("ok", hashlib.sha256(raw).hexdigest())


def _decode_slice(item):
    if item["mediaType"] not in ("image/png", "image/jpeg"):
        return ("error", "media_rejected", "type")
    if item["width"] * item["height"] > LIMITS["maxSlicePixels"]:
        return ("error", "limit_exceeded", "limit")
    try:
        raw = base64.b64decode(item["bytesBase64"], validate=True)
    except Exception:
        return ("error", "media_rejected", "magic")
    if len(raw) > LIMITS["maxSliceBytes"]:
        return ("error", "limit_exceeded", "limit")
    if len(raw) != item["byteLength"] or hashlib.sha256(raw).hexdigest() != item["sha256"]:
        return ("error", "media_rejected", "hash")
    return ("ok", raw)


def _inspect_slice(item, raw):
    parsed = _parse_png(raw) if item["mediaType"] == "image/png" else _parse_jpeg(raw)
    if parsed is None:
        return ("error", "media_rejected", "magic")
    if parsed == "limit":
        return ("error", "limit_exceeded", "limit")
    if parsed != (item["width"], item["height"]):
        return ("error", "media_rejected", "dimensions")
    return ("ok", raw)


def _parse_png(data):
    if not data.startswith(b"\x89PNG\r\n\x1a\n") or len(data) < 26 or data[12:16] != b"IHDR":
        return None
    length = struct.unpack(">I", data[8:12])[0]
    if length != 13:
        return None
    width, height, depth, color = struct.unpack(">IIBB", data[16:26])
    if depth != 8 or color not in (2, 6):
        return None
    if width * height > LIMITS["maxSlicePixels"]:
        return "limit"
    idat = bytearray()
    pos = 8
    while pos + 8 <= len(data):
        chunk_length = struct.unpack(">I", data[pos : pos + 4])[0]
        tag = data[pos + 4 : pos + 8]
        start = pos + 8
        end = start + chunk_length
        if end + 4 > len(data):
            return None
        if tag == b"IDAT":
            idat.extend(data[start:end])
        pos = end + 4
        if tag == b"IEND":
            break
    channels = 3 if color == 2 else 4
    expected = height * (1 + width * channels)
    if expected > LIMITS["pngDecompressCapBytes"]:
        return "limit"
    try:
        decoder = zlib.decompressobj()
        raw = decoder.decompress(bytes(idat), expected + 1)
    except zlib.error:
        return "limit"
    if len(raw) > expected or decoder.unconsumed_tail:
        return "limit"
    if len(raw) != expected:
        return None
    return (width, height)


def _parse_jpeg(data):
    if len(data) < 4 or data[:2] != b"\xff\xd8" or data[-2:] != b"\xff\xd9":
        return None
    index = 2
    while index + 4 < len(data):
        if data[index] != 0xFF:
            return None
        while index < len(data) and data[index] == 0xFF:
            index += 1
        if index >= len(data):
            return None
        marker = data[index]
        index += 1
        if marker in (0xD8, 0xD9):
            continue
        if marker == 0xDA or index + 2 > len(data):
            return None
        segment = struct.unpack(">H", data[index : index + 2])[0]
        if segment < 2 or index + segment > len(data):
            return None
        if marker in (0xC0, 0xC2):
            if segment < 7:
                return None
            precision = data[index + 2]
            height, width = struct.unpack(">HH", data[index + 3 : index + 7])
            if precision != 8 or width < 1 or height < 1:
                return None
            if width * height > LIMITS["maxSlicePixels"] or width > LIMITS["maxSliceWidth"] or height > LIMITS["maxSliceHeight"]:
                return "limit"
            return (width, height)
        index += segment
    return None


def _account_view(world, account_id):
    account = world["accounts"][account_id]
    series_views = []
    for series in world["series"].values():
        if series["ownerAccountId"] != account_id:
            continue
        episodes = []
        for episode in sorted(series["episodes"], key=lambda item: item["ordinal"]):
            record = world["runtime"]["records"].get(episode["episodeId"])
            publication = None
            if record is not None and record["revisions"]:
                publication = {
                    "status": record["status"],
                    "publicationId": record["publicationId"],
                    "publishedRevisionId": record["publishedRevisionId"],
                    "headRevisionId": record["revisions"][-1]["revisionId"],
                    "canonicalUrl": record["canonicalUrl"],
                }
            episodes.append(
                {
                    "episodeId": episode["episodeId"],
                    "ordinal": episode["ordinal"],
                    "title": episode["title"],
                    "publication": publication,
                }
            )
        series_views.append(
            {
                "seriesId": series["seriesId"],
                "title": series["title"],
                "ownerAccountId": series["ownerAccountId"],
                "episodes": episodes,
            }
        )
    series_views.sort(key=lambda item: item["seriesId"])
    return {
        "accountId": account["accountId"],
        "handle": account["handle"],
        "testOnly": True,
        "nonProductionLabel": "TEST-ONLY",
        "identityProof": PROTOCOL["identityProof"],
        "series": series_views,
    }


def _credential_view(credential):
    return {
        "credentialId": credential["credentialId"],
        "secret": credential["secret"],
        "tokenType": "prototype-credential",
        "scope": list(PROTOCOL["scope"]),
        "accountId": credential["accountId"],
        "expiresAt": _fmt(credential["expiresAt"]),
        "testOnly": True,
        "nonProductionLabel": "TEST-ONLY",
        "issuerRole": PROTOCOL["issuer"]["role"],
        "distinctFromProductionProvider": True,
        "delivery": "service-channel-only",
    }


def _issue_credential(world, account_id, clock):
    number = _next(world, "credential", "", width=16)
    credential = {
        "credentialId": f"cred_test_{number}",
        "secret": f"sek_test_{number}",
        "accountId": account_id,
        "expiresAt": clock + timedelta(seconds=LIMITS["credentialTtlSeconds"]),
        "revoked": False,
    }
    world["runtime"]["credentials"][credential["credentialId"]] = credential
    return credential


def _owned_episode(world, cred, series_id, episode_id):
    series = world["series"].get(series_id)
    episode = world["episodes"].get(episode_id)
    if series is None or episode is None or episode["seriesId"] != series_id:
        return None, "not_found"
    if series["ownerAccountId"] != cred["accountId"] or episode["ownerAccountId"] != cred["accountId"]:
        return None, "not_owner"
    return episode, None


def _record(world, episode):
    record = world["runtime"]["records"].get(episode["episodeId"])
    if record is None:
        record = {
            "episodeId": episode["episodeId"],
            "revisions": [],
            "status": None,
            "publicationId": None,
            "publishedRevisionId": None,
            "canonicalUrl": None,
        }
        world["runtime"]["records"][episode["episodeId"]] = record
    return record


def _revision(world, episode_id, revision_id):
    record = world["runtime"]["records"].get(episode_id)
    if record is None:
        return None
    for revision in record["revisions"]:
        if revision["revisionId"] == revision_id:
            return revision
    return None


def _asset_shaped(url):
    parts = urlsplit(url)
    if parts.hostname != "127.0.0.1" or parts.username or parts.password:
        return True
    path = parts.path
    if path.startswith("/ss-publish/assets") or path.startswith("/assets") or "/slices/" in path:
        return True
    return False


def _live_credential(world, envelope, clock):
    proof = envelope.get("credential")
    if not isinstance(proof, dict):
        return None, "unauthorized"
    cred = world["runtime"]["credentials"].get(proof.get("credentialId"))
    if cred is None or not hmac.compare_digest(cred["secret"], proof.get("secret", "")):
        return None, "unauthorized"
    if cred["revoked"]:
        return None, "credential_revoked"
    if clock >= cred["expiresAt"]:
        return None, "credential_expired"
    return cred, None


def _replay_secret_ok(world, envelope, existing):
    if existing.get("accountId") is None:
        return "credential" not in envelope
    proof = envelope.get("credential")
    if not isinstance(proof, dict):
        return False
    cred = world["runtime"]["credentials"].get(proof.get("credentialId"))
    if cred is None or cred["accountId"] != existing["accountId"]:
        return False
    return hmac.compare_digest(cred["secret"], proof.get("secret", ""))


def _store(world, key, digest, command, body, cred, response, status="committed"):
    world["runtime"]["operations"][key] = {
        "hash": digest,
        "status": status,
        "command": command,
        "body": body,
        "accountId": None if cred is None else cred["accountId"],
        "response": response,
    }


def _identity_body(command, body):
    if command != "episode.upload":
        return body
    reduced = dict(body)
    reduced.pop("completion", None)
    return reduced


def _hash(command, body, proof):
    credential_id = "" if proof is None else proof["credentialId"]
    payload = {"protocol": PROTOCOL["protocol"], "command": command, "body": body, "credentialId": credential_id}
    encoded = json.dumps(payload, sort_keys=True, separators=(",", ":"), ensure_ascii=False)
    return hashlib.sha256(encoded.encode("utf-8")).hexdigest()


def _next(world, name, prefix, width=16):
    world["runtime"]["counters"][name] += 1
    return prefix + f"{world['runtime']['counters'][name]:0{width}d}"


def _verifier_ok(verifier):
    if not isinstance(verifier, str) or not 43 <= len(verifier) <= 128:
        return False
    return all(char.isalnum() or char in "._~-" for char in verifier)


def _parse_clock(value):
    try:
        parsed = datetime.strptime(value, "%Y-%m-%dT%H:%M:%SZ").replace(tzinfo=timezone.utc)
    except (TypeError, ValueError):
        return None
    return parsed


def _fmt(moment):
    return moment.astimezone(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def _ok(command, result, replay=False):
    response = {
        "protocol": PROTOCOL["protocol"],
        "ok": True,
        "command": command,
        "idempotentReplay": replay,
        "result": result,
    }
    problems = validate({"$ref": "#/$defs/response"}, response, SCHEMA)
    if problems:
        raise RuntimeError(problems)
    return response


def _fail(command, code, detail=None):
    error = {"code": code, "retryable": False}
    if detail is not None:
        error["detail"] = detail
    response = {
        "protocol": PROTOCOL["protocol"],
        "ok": False,
        "command": command,
        "idempotentReplay": False,
        "error": error,
    }
    problems = validate({"$ref": "#/$defs/response"}, response, SCHEMA)
    if problems:
        raise RuntimeError(problems)
    return response


def _replay(response):
    copied = copy.deepcopy(response)
    copied["idempotentReplay"] = True
    return copied


def _check_world(loaded):
    if loaded.get("transportProfile") != PROTOCOL["transport"]["enabledProfile"]:
        raise ValueError("world transport profile")
    if loaded.get("nonProductionLabel") != "TEST-ONLY":
        raise ValueError("world label")
    owners = set()
    for account in loaded["accounts"]:
        if not account["accountId"].startswith("acct_test_") or account.get("testOnly") is not True:
            raise ValueError("account is not a test fixture")
        if account.get("nonProductionLabel") != "TEST-ONLY":
            raise ValueError("account label")
        owners.add(account["accountId"])
    for series in loaded["series"]:
        if series["ownerAccountId"] not in owners:
            raise ValueError("series owner")
