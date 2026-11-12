from __future__ import annotations

import base64
import copy
import hashlib
import json
from pathlib import Path
from urllib.parse import parse_qsl, urlsplit

from prototypes.publishing.fixture_harness import FixtureHarness
from prototypes.publishing.media import facts
from prototypes.publishing.transport import browser_affirm
from prototypes.publishing.oracle import (
    PROTOCOL,
    SCHEMA,
    accept_client_storage,
    apply,
    callback_query_allowed,
    fresh_world,
    parse_clock,
    pkce_s256,
)
from prototypes.publishing.schema_validate import validate

FIXTURES = Path(__file__).resolve().parent / "fixtures"
TEMPLATES = FIXTURES / "templates"
FORBIDDEN_KEYS = {
    "access_token",
    "refresh_token",
    "id_token",
    "code_verifier",
    "password",
    "bearer",
}


class SequenceError(Exception):
    pass


def run_sequence(path):
    sequence = json.loads(Path(path).read_text(encoding="utf-8"))
    if sequence.get("stateEntropy") != "synthetic-fixture-not-csprng":
        raise SequenceError("fixture state is not labeled")
    if sequence.get("verifierLabel") != "synthetic-fixture-not-a-secret":
        raise SequenceError("fixture verifier is not labeled")
    storage_error = accept_client_storage(sequence["clientStorage"])
    if sequence.get("expectStorageError"):
        if storage_error != sequence["expectStorageError"]:
            raise SequenceError(f"storage {storage_error}")
        return []
    if storage_error:
        raise SequenceError(storage_error)
    world = fresh_world()
    ctx = {
        "verifiers": sequence["verifiers"],
        "media": {"png": facts("png"), "jpeg": facts("jpeg")},
        "steps": {},
        "credentials": {},
        "redirects": {},
        "clock": sequence.get("clock", PROTOCOL["defaultClock"]),
    }
    for step in sequence["steps"]:
        response = _step(world, ctx, step)
        ctx["steps"][step["id"]] = response
        _check_expect(step, response, ctx)
    return [ctx["steps"][step["id"]] for step in sequence["steps"]]


def main():
    run_sequence(FIXTURES / "sequences" / "comic.json")
    print("comic ok")
    run_sequence(FIXTURES / "sequences" / "prose.json")
    print("prose ok")


def _step(world, ctx, step):
    if step.get("clientCheck") == "callback":
        allowed = callback_query_allowed(step["queryKeys"])
        if allowed:
            return {"ok": True, "clientError": None}
        return {"ok": False, "clientError": "callback_forbidden"}
    body = {}
    if step.get("use"):
        body = json.loads((TEMPLATES / f"{step['use']}.json").read_text(encoding="utf-8"))
    body.update(step.get("body") or {})
    body = _interp(body, ctx)
    _fill_content_sha(body)
    envelope = {
        "protocol": PROTOCOL["protocol"],
        "command": step["command"],
        "idempotencyKey": step["idempotencyKey"],
        "body": body,
    }
    credential_from = step.get("credentialFrom")
    if credential_from:
        envelope["credential"] = dict(ctx["credentials"][credential_from])
    harness = FixtureHarness(
        parse_clock(step.get("clock", ctx["clock"])),
        pause=bool(step.get("pauseBeforeCommit", False)),
    )
    response = apply(world, envelope, harness)
    problems = validate({"$ref": "#/$defs/response"}, response, SCHEMA)
    if problems:
        raise SequenceError(f"{step['id']} response schema {problems}")
    _scan(response, step["command"])
    if response.get("ok") and step["command"] == "auth.begin":
        _check_begin(response)
        if not response.get("idempotentReplay"):
            account = step.get("browserAffirm")
            if account is None and step.get("use") == "begin-owner-a":
                account = "acct_test_owner_a"
            if account is None and step.get("use") == "begin-owner-b":
                account = "acct_test_owner_b"
            location, _cookie = browser_affirm(world, response["result"]["authorizationUrl"], account, harness)
            ctx["redirects"][step["id"]] = location
    if response.get("ok") and step["command"] == "auth.exchange":
        _check_credential(response)
        cred = response["result"]["credential"]
        ctx["credentials"][step["id"]] = {
            "credentialId": cred["credentialId"],
            "secret": cred["secret"],
        }
    return response


def _check_begin(response):
    result = response["result"]
    if "testOnlyRedirect" in result or "loginHint" in result:
        raise SequenceError("oracle hook leaked into the network result")
    authorize = urlsplit(result["authorizationUrl"])
    authorize_pairs = parse_qsl(authorize.query)
    authorize_keys = [key for key, _value in authorize_pairs]
    authorize_query = dict(authorize_pairs)
    if authorize.path != "/ss-publish/browser/authorize":
        raise SequenceError("authorize path")
    if authorize_query.get("redirect_uri") != PROTOCOL["exampleRedirect"]:
        raise SequenceError("redirect did not round-trip")
    if authorize_query.get("code_challenge_method") != "S256" or authorize_query.get("response_type") != "code":
        raise SequenceError("authorize query")
    if authorize_keys != PROTOCOL["authorizationQueryKeys"]:
        raise SequenceError(f"authorization keys {authorize_keys}")
    if result["issuerRole"] != "prototype-test-issuer" or result["distinctFromProductionProvider"] is not True:
        raise SequenceError("issuer role")
    if "code" in authorize_keys or result["identityProof"] != "browser-fixture-affirmation":
        raise SequenceError("begin leaked a code or identity proof")


def _check_credential(response):
    cred = response["result"]["credential"]
    expected = {
        "tokenType": "prototype-credential",
        "scope": ["prototype:creator"],
        "testOnly": True,
        "nonProductionLabel": "TEST-ONLY",
        "issuerRole": "prototype-test-issuer",
        "distinctFromProductionProvider": True,
        "delivery": "service-channel-only",
    }
    for key, value in expected.items():
        if cred[key] != value:
            raise SequenceError(f"credential {key}")
    if not cred["credentialId"].startswith("cred_live_") or not cred["secret"].startswith("sek_live_"):
        raise SequenceError("credential prefix")


def _check_expect(step, response, ctx):
    expect = _interp(step.get("expect") or {}, ctx)
    if "clientError" in expect and response.get("clientError") != expect["clientError"]:
        raise SequenceError(f"{step['id']} client {response.get('clientError')}")
    if "clientError" in expect:
        return
    if "ok" in expect and response["ok"] is not expect["ok"]:
        raise SequenceError(f"{step['id']} ok { _redact(response) }")
    if "errorCode" in expect:
        actual = None if response["ok"] else response["error"]["code"]
        if actual != expect["errorCode"]:
            raise SequenceError(f"{step['id']} code {actual} {_redact(response)}")
    if "detail" in expect:
        actual = None if response["ok"] else response["error"].get("detail")
        if actual != expect["detail"]:
            raise SequenceError(f"{step['id']} detail {actual}")
    if "idempotentReplay" in expect and response["idempotentReplay"] is not expect["idempotentReplay"]:
        raise SequenceError(f"{step['id']} replay {response['idempotentReplay']}")
    for path in expect.get("absent") or []:
        try:
            _dig(response, path)
        except (KeyError, IndexError, TypeError):
            continue
        raise SequenceError(f"{step['id']} still has {path}")
    for path, value in (expect.get("equals") or {}).items():
        actual = _dig(response, path)
        if actual != value:
            raise SequenceError(f"{step['id']} {path} {actual!r} != {value!r}")
    if "seriesIds" in expect:
        actual = [series["seriesId"] for series in response["result"]["account"]["series"]]
        if actual != expect["seriesIds"]:
            raise SequenceError(f"{step['id']} series {actual}")
    if expect.get("matchesMedia"):
        kind = expect["matchesMedia"]
        media = ctx["media"][kind]
        slice0 = _dig(response, "result.content.slices.0")
        if slice0["bytesBase64"] != media["bytesBase64"] or slice0["sha256"] != media["sha256"]:
            raise SequenceError(f"{step['id']} media bytes")
        if slice0["ordinal"] != 1:
            raise SequenceError(f"{step['id']} ordinal")


def _scan(value, command, key_name=None):
    if isinstance(value, dict):
        for key, item in value.items():
            if key in FORBIDDEN_KEYS or "bearer" in key.lower():
                raise SequenceError(f"forbidden key {key}")
            if key == "secret" and command != "auth.exchange":
                raise SequenceError("secret left the exchange command")
            _scan(item, command, key)
    elif isinstance(value, list):
        for item in value:
            _scan(item, command, key_name)
    elif isinstance(value, str) and value.startswith("http"):
        query = parse_qsl(urlsplit(value).query)
        for key, item in query:
            if key in FORBIDDEN_KEYS or "bearer" in key.lower() or "bearer" in item.lower():
                raise SequenceError(f"forbidden callback param {key}")


def _fill_content_sha(body):
    content = body.get("content")
    if not isinstance(content, dict):
        return
    if content.get("kind") == "comic":
        raw = b"".join(base64.b64decode(item["bytesBase64"]) for item in content["slices"])
    elif content.get("kind") == "prose":
        raw = content["text"].encode("utf-8")
    else:
        return
    digest = hashlib.sha256(raw).hexdigest()
    if body.get("expectedSha256") == "{{contentSha}}":
        body["expectedSha256"] = digest


def _interp(value, ctx):
    if isinstance(value, str) and value == "{{contentSha}}":
        return value
    if isinstance(value, str) and value.startswith("{{") and value.endswith("}}") and value.count("{{") == 1:
        return _token(value[2:-2], ctx)
    if isinstance(value, list):
        return [_interp(item, ctx) for item in value]
    if isinstance(value, dict):
        return {key: _interp(item, ctx) for key, item in value.items()}
    return value


def _token(token, ctx):
    if token.startswith("pkce:"):
        return pkce_s256(ctx["verifiers"][token.split(":", 1)[1]])
    if token.startswith("verifier:"):
        return ctx["verifiers"][token.split(":", 1)[1]]
    if token.startswith("media:"):
        kind, field = token.split(":", 1)[1].split(".")
        return ctx["media"][kind][field]
    if token.startswith("callback:"):
        step_id, field = token.split(":", 1)[1].split(".")
        url = ctx["redirects"][step_id]
        return dict(parse_qsl(urlsplit(url).query))[field]
    if token.startswith("step:"):
        step_id, path = token.split(":", 1)[1].split(".", 1)
        return _dig(ctx["steps"][step_id], path)
    raise SequenceError(f"unknown token {token}")


def _dig(value, path):
    current = value
    for part in path.split("."):
        if isinstance(current, list):
            current = current[int(part)]
        else:
            current = current[part]
    return current


def _redact(response):
    cloned = copy.deepcopy(response)
    credential = cloned.get("result", {}).get("credential") if isinstance(cloned.get("result"), dict) else None
    if isinstance(credential, dict) and "secret" in credential:
        credential["secret"] = "[redacted]"
    return cloned


if __name__ == "__main__":
    main()
