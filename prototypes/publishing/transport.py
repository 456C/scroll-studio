from __future__ import annotations

import json
from html import escape
from pathlib import Path
from urllib.parse import parse_qsl, urlencode, urlsplit

from prototypes.publishing.fixture_harness import FixtureHarness
from prototypes.publishing.oracle import PROTOCOL, apply, issue_browser_code
from prototypes.publishing.runtime_rules import new_csrf, new_session_id, server_now

SPEC = json.loads(
    (Path(__file__).resolve().parents[2] / "contracts" / "publishing" / "v1" / "transport.json").read_text(
        encoding="utf-8"
    )
)
ORIGIN = SPEC["origin"]
REFERRER_POLICY = "same-origin"
HOST = SPEC["hostHeader"]
COOKIE = SPEC["cookie"]["name"]
COOKIE_PATH = SPEC["cookie"]["path"]
SESSION_SECONDS = SPEC["cookie"]["maxAgeSeconds"]


class HttpResult:
    def __init__(self, status, headers, body):
        self.status = status
        self.headers = headers
        self.body = body


def dispatch(world, request, harness=None):
    headers = {key.lower(): value for key, value in request.get("headers", {}).items()}
    if headers.get("host") != HOST:
        return _json_error(400, "invalid_request")
    method = request.get("method", "")
    path = request.get("path", "")
    query = request.get("query") or {}
    body = request.get("body") or b""
    clock = harness.now() if harness is not None else server_now()
    if path == SPEC["command"]["path"]:
        return _command(world, method, headers, body, harness)
    if path == "/ss-publish/browser/series" and method == "GET":
        return _series(world, headers, clock, harness)
    if path == "/ss-publish/browser/operator" and method == "GET":
        return _operator_page(world, headers, clock, harness)
    if path == "/ss-publish/browser/report" and method == "POST":
        return _report(world, headers, body, clock)
    if path == "/ss-publish/browser/operator/hold" and method == "POST":
        return _operator_hold(world, headers, body, clock)
    if path == "/ss-publish/browser/operator/remove" and method == "POST":
        return _operator_remove(world, headers, body, clock)
    if path == SPEC["routes"]["authorize"] and method == "GET":
        return _authorize(world, query, headers, clock, harness)
    if path == SPEC["routes"]["affirm"] and method == "POST":
        return _affirm(world, headers, body, clock, harness)
    if path == SPEC["routes"]["revokeSession"] and method == "POST":
        return _revoke(world, headers, body, clock)
    if path.startswith(SPEC["routes"]["reader"]) and method == "GET":
        return _reader(world, path[len(SPEC["routes"]["reader"]) :], headers, clock, harness)
    if path.startswith(SPEC["routes"]["readerAsset"]) and method == "GET":
        return _reader_asset(world, path[len(SPEC["routes"]["readerAsset"]) :], headers, clock)
    if path.startswith(SPEC["routes"]["previewAsset"]) and method == "GET":
        return _preview_asset(world, path[len(SPEC["routes"]["previewAsset"]) :], headers, clock)
    if path.startswith(SPEC["routes"]["preview"]) and method == "GET":
        return _preview(world, path[len(SPEC["routes"]["preview"]) :], headers, clock, harness)
    return _json_error(404, "not_found")


def browser_affirm(world, authorization_url, account_id, harness):
    parts = urlsplit(authorization_url)
    if f"{parts.scheme}://{parts.netloc}" != ORIGIN or parts.path != SPEC["routes"]["authorize"]:
        raise RuntimeError("authorization URL is not the browser route")
    if any(token in authorization_url.lower() for token in ("access_token", "bearer", "secret=")):
        raise RuntimeError("authorization URL carries a credential")
    query = dict(parse_qsl(parts.query))
    page = dispatch(
        world,
        {"method": "GET", "path": parts.path, "query": query, "headers": {"host": HOST}, "body": b""},
        harness,
    )
    html = page.body.decode("utf-8")
    if page.status != 200 or "TEST ONLY" not in html or account_id not in html or "secret" in html:
        raise RuntimeError("authorize page is not the fixture sign-in")
    csrf = _hidden(html, "csrf")
    form = urlencode({"csrf": csrf, "state": query["state"], "accountId": account_id}).encode()
    posted = dispatch(
        world,
        {
            "method": "POST",
            "path": SPEC["routes"]["affirm"],
            "query": {},
            "headers": {
                "host": HOST,
                "origin": ORIGIN,
                "content-type": "application/x-www-form-urlencoded",
            },
            "body": form,
        },
        harness,
    )
    if posted.status != 302:
        raise RuntimeError(f"affirm status {posted.status}")
    location = posted.headers["location"]
    cookie = posted.headers.get("set-cookie", "")
    if not cookie.startswith(COOKIE + "=") or "HttpOnly" not in cookie or "SameSite=Strict" not in cookie:
        raise RuntimeError("session cookie is not HttpOnly SameSite=Strict")
    if f"Path={COOKIE_PATH}" not in cookie or "Domain=" in cookie:
        raise RuntimeError("cookie path or domain")
    if cookie_sent(COOKIE_PATH, "/ss-publish/callback"):
        raise RuntimeError("callback path would receive the session cookie")
    callback = urlsplit(location)
    keys = [key for key, _value in parse_qsl(callback.query)]
    if keys != ["code", "state"] or callback.path != "/ss-publish/callback":
        raise RuntimeError("callback is not code and state")
    return location, cookie.split(";", 1)[0].split("=", 1)[1]


def cookie_sent(cookie_path, request_path):
    if request_path == cookie_path:
        return True
    prefix = cookie_path if cookie_path.endswith("/") else cookie_path + "/"
    return request_path.startswith(prefix)


def _command(world, method, headers, body, harness):
    if method != "POST":
        return _json_error(405, "invalid_request", {"allow": "POST"})
    origin = headers.get("origin")
    if origin is not None and origin != ORIGIN:
        return _json_error(403, "unauthorized")
    if len(body) > SPEC["command"]["maxBodyBytes"]:
        return _json_error(413, "limit_exceeded")
    if headers.get("content-type", "").split(";")[0].strip().lower() != "application/json":
        return _json_error(415, "invalid_request")
    try:
        envelope = json.loads(body.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError):
        return _json_error(400, "invalid_request")
    result = apply(world, envelope, harness)
    status = SPEC["command"]["successStatus"] if result.get("ok") else SPEC["errorStatus"][result["error"]["code"]]
    return _json_result(status, result)


def _authorize(world, query, headers, clock, harness):
    txn = _txn_for_query(world, query)
    if txn is None:
        return _json_error(400, "invalid_request")
    session = _session(world, headers, clock)
    if session is None:
        csrf = _new_csrf(world, harness)
        txn["csrf"] = csrf
        return _html(200, _picker(world, csrf, state=query.get("state"), nxt=None))
    if txn["status"] == "consumed" or txn.get("accountId") not in (None, session["accountId"]):
        return _json_error(400, "invalid_request")
    code = issue_browser_code(world, query["state"], session["accountId"], harness)
    if not code:
        return _json_error(400, "invalid_request")
    location = txn["redirectUri"] + "?" + urlencode([("code", code), ("state", txn["state"])])
    return HttpResult(302, {"location": location, "cache-control": "no-store", "referrer-policy": REFERRER_POLICY}, b"")


def _affirm(world, headers, body, clock, harness):
    if headers.get("origin") != ORIGIN:
        return _json_error(403, "unauthorized")
    form = dict(parse_qsl(body.decode("utf-8"), keep_blank_values=True))
    account_id = form.get("accountId", "")
    account = world["accounts"].get(account_id)
    if account is None or account.get("testOnly") is not True or not account_id.startswith("acct_test_"):
        return _json_error(403, "unauthorized")
    state = form.get("state")
    nxt = form.get("next")
    if state:
        txn = world["runtime"]["by_state"].get(state)
        if txn is None or not txn.get("csrf") or form.get("csrf") != txn["csrf"]:
            return _json_error(403, "unauthorized")
        if not issue_browser_code(world, state, account_id, harness):
            return _json_error(400, "invalid_request")
        txn["csrf"] = None
        location = txn["redirectUri"] + "?" + urlencode([("code", txn["code"]), ("state", state)])
    elif nxt and _safe_next(nxt):
        if not _consume_form_csrf(world, form.get("csrf"), nxt, clock):
            return _json_error(403, "unauthorized")
        location = ORIGIN + nxt
    else:
        return _json_error(400, "invalid_request")
    session_id = _new_session(world, harness)
    world["runtime"]["sessions"][session_id] = {
        "sessionId": session_id,
        "accountId": account_id,
        "csrf": _new_csrf(world, harness),
        "expiresAt": clock + _seconds(SESSION_SECONDS),
        "revoked": False,
    }
    return HttpResult(
        302,
        {
            "location": location,
            "cache-control": "no-store",
            "referrer-policy": REFERRER_POLICY,
            "set-cookie": _set_cookie(session_id, SESSION_SECONDS),
        },
        b"",
    )


def _revoke(world, headers, body, clock):
    if headers.get("origin") != ORIGIN:
        return _json_error(403, "unauthorized")
    session = _session(world, headers, clock)
    form = dict(parse_qsl(body.decode("utf-8"), keep_blank_values=True))
    if session is None or not session.get("csrf") or form.get("csrf") != session["csrf"]:
        return _json_error(403, "unauthorized")
    session["revoked"] = True
    return HttpResult(
        200,
        {
            "cache-control": "no-store",
            "referrer-policy": REFERRER_POLICY,
            "set-cookie": _set_cookie("", 0),
            "content-type": "text/html; charset=utf-8",
        },
        b"<!DOCTYPE html><title>TEST ONLY</title><p>Session revoked.</p>",
    )


def _reader(world, publication_id, headers, clock, harness):
    session = _session(world, headers, clock)
    path = SPEC["routes"]["reader"] + publication_id
    if session is None:
        return _sign_in(world, path, clock, harness)
    record = _record_by_publication(world, publication_id)
    if record is None or _owner(world, record) != session["accountId"]:
        return _html(404, _bare("Not found."))
    if record.get("operatorRemoved") or record.get("operatorHold") or record["status"] in SPEC["deniedReaderStatuses"]:
        return _html(410, _bare("Unavailable."))
    if record["status"] != "published":
        return _html(404, _bare("Not found."))
    revision = _revision(world, record)
    return _html(200, _episode_html(world, record, revision, preview=False, csrf=session["csrf"]))


def _preview(world, rest, headers, clock, harness):
    session = _session(world, headers, clock)
    parts = rest.split("/")
    if len(parts) != 3:
        return _html(404, _bare("Not found."))
    series_id, episode_id, revision_id = parts
    path = f"{SPEC['routes']['preview']}{series_id}/{episode_id}/{revision_id}"
    if session is None:
        return _sign_in(world, path, clock, harness)
    episode = world["episodes"].get(episode_id)
    revision = None
    record = world["runtime"]["records"].get(episode_id)
    if record is not None:
        revision = next((item for item in record["revisions"] if item["revisionId"] == revision_id), None)
    if (
        episode is None
        or episode["seriesId"] != series_id
        or episode["ownerAccountId"] != session["accountId"]
        or revision is None
        or record.get("operatorRemoved")
    ):
        return _html(404, _bare("Not found."))
    return _html(200, _episode_html(world, record, revision, preview=True, csrf=session["csrf"]))


def _reader_asset(world, rest, headers, clock):
    session = _session(world, headers, clock)
    if session is None:
        return HttpResult(401, {"cache-control": "no-store"}, b"")
    parts = rest.split("/")
    if len(parts) != 2:
        return HttpResult(404, {"cache-control": "no-store"}, b"")
    publication_id, ordinal = parts
    record = _record_by_publication(world, publication_id)
    if record is None or _owner(world, record) != session["accountId"]:
        return HttpResult(404, {"cache-control": "no-store"}, b"")
    if record.get("operatorRemoved") or record.get("operatorHold") or record["status"] in SPEC["deniedReaderStatuses"] or record["status"] != "published":
        return HttpResult(410, {"cache-control": "no-store"}, b"")
    return _slice_response(world, record, _revision(world, record), ordinal)


def _preview_asset(world, rest, headers, clock):
    session = _session(world, headers, clock)
    if session is None:
        return HttpResult(401, {"cache-control": "no-store"}, b"")
    parts = rest.split("/")
    if len(parts) != 4:
        return HttpResult(404, {"cache-control": "no-store"}, b"")
    series_id, episode_id, revision_id, ordinal = parts
    episode = world["episodes"].get(episode_id)
    record = world["runtime"]["records"].get(episode_id)
    revision = None
    if record is not None:
        revision = next((item for item in record["revisions"] if item["revisionId"] == revision_id), None)
    if (
        episode is None
        or episode["seriesId"] != series_id
        or episode["ownerAccountId"] != session["accountId"]
        or revision is None
        or record.get("operatorRemoved")
    ):
        return HttpResult(404, {"cache-control": "no-store"}, b"")
    return _slice_response(world, record, revision, ordinal)


def _picker(world, csrf, state, nxt):
    buttons = []
    for account in world["accounts"].values():
        if account.get("testOnly") is True and account["accountId"].startswith("acct_test_"):
            buttons.append(
                f'<button type="submit" name="accountId" value="{escape(account["accountId"])}">'
                f'{escape(account["handle"])}</button>'
            )
    hidden = f'<input type="hidden" name="csrf" value="{escape(csrf)}">'
    if state:
        hidden += f'<input type="hidden" name="state" value="{escape(state)}">'
    if nxt:
        hidden += f'<input type="hidden" name="next" value="{escape(nxt)}">'
    return _page(
        "TEST ONLY fixture sign-in",
        "<p>TEST ONLY. These accounts are synthetic publishing fixtures. "
        "They are not production identities.</p>"
        f'<form method="post" action="{SPEC["routes"]["affirm"]}">{hidden}{"".join(buttons)}</form>',
    )


def _episode_html(world, record, revision, preview, csrf):
    content = revision["content"]
    episode = world["episodes"][record["episodeId"]]
    series_id = episode["seriesId"]
    if content["kind"] == "prose":
        body = f"<pre>{escape(content['text'])}</pre>"
    else:
        images = []
        for item in content["slices"]:
            if preview:
                src = (
                    f"{SPEC['routes']['previewAsset']}{series_id}/{record['episodeId']}/"
                    f"{revision['revisionId']}/{item['ordinal']}"
                )
            else:
                src = f"{SPEC['routes']['readerAsset']}{record['publicationId']}/{item['ordinal']}"
            images.append(f'<img src="{escape(src)}" alt="">')
        body = "".join(images)
    kind = "preview" if preview else "reader"
    revoke = (
        f'<form method="post" action="{SPEC["routes"]["revokeSession"]}">'
        f'<input type="hidden" name="csrf" value="{escape(csrf)}">'
        '<button type="submit">Revoke session</button></form>'
    )
    report = ""
    if not preview and record.get("publicationId"):
        report = (
            f'<form method="post" action="/ss-publish/browser/report">'
            f'<input type="hidden" name="csrf" value="{escape(csrf)}">'
            f'<input type="hidden" name="publicationId" value="{escape(record["publicationId"])}">'
            '<label>Report <input name="reason" maxlength="400" required></label>'
            '<button type="submit">Send report</button></form>'
        )
    return _page(
        f"TEST ONLY {kind}",
        "<p>TEST ONLY private fixture reader.</p>"
        f"<p>{escape(episode['seriesTitle'])}</p>"
        f"{_episode_nav(world, episode, preview)}"
        f"<h1>{escape(episode['title'])}</h1>"
        f"{body}{report}{revoke}",
    )


def _episode_nav(world, episode, preview):
    series = world["series"][episode["seriesId"]]
    ordered = sorted(series["episodes"], key=lambda item: item["ordinal"])
    links = []
    previous = ""
    nxt = ""
    for item in ordered:
        href = _episode_href(world, item["episodeId"], preview)
        label = f"{item['ordinal']}. {item['title']}"
        if href is None:
            links.append(f"<li>{escape(label)} <span>not published</span></li>")
            continue
        links.append(f'<li><a href="{escape(href)}">{escape(label)}</a></li>')
    index = [item["episodeId"] for item in ordered].index(episode["episodeId"])
    if index > 0:
        href = _episode_href(world, ordered[index - 1]["episodeId"], preview)
        if href:
            previous = f'<a href="{escape(href)}">Previous</a>'
    if index + 1 < len(ordered):
        href = _episode_href(world, ordered[index + 1]["episodeId"], preview)
        if href:
            nxt = f'<a href="{escape(href)}">Next</a>'
    return (
        '<p><a href="/ss-publish/browser/series">Series</a></p>'
        f"<ol>{''.join(links)}</ol><p>{previous} {nxt}</p>"
    )


def _episode_href(world, episode_id, preview):
    record = world["runtime"]["records"].get(episode_id)
    if record is None or record.get("operatorRemoved") or not record["revisions"]:
        return None
    if record.get("operatorHold") and not preview:
        return None
    if preview:
        revision = record["revisions"][-1]
        episode = world["episodes"][episode_id]
        return f"{SPEC['routes']['preview']}{episode['seriesId']}/{episode_id}/{revision['revisionId']}"
    if record["status"] == "published" and record.get("publicationId"):
        return SPEC["routes"]["reader"] + record["publicationId"]
    return None


def _series(world, headers, clock, harness):
    session = _session(world, headers, clock)
    path = "/ss-publish/browser/series"
    if session is None:
        return _sign_in(world, path, clock, harness)
    blocks = []
    for series in sorted(world["series"].values(), key=lambda item: item["seriesId"]):
        if series["ownerAccountId"] != session["accountId"]:
            continue
        items = []
        for episode in sorted(series["episodes"], key=lambda item: item["ordinal"]):
            href = _episode_href(world, episode["episodeId"], preview=False)
            label = f"{episode['ordinal']}. {episode['title']}"
            if href is None:
                items.append(f"<li>{escape(label)} <span>not published</span></li>")
            else:
                items.append(f'<li><a href="{escape(href)}">{escape(label)}</a></li>')
        blocks.append(f"<section><h1>{escape(series['title'])}</h1><ol>{''.join(items)}</ol></section>")
    if not blocks:
        blocks.append("<p>No series.</p>")
    revoke = (
        f'<form method="post" action="{SPEC["routes"]["revokeSession"]}">'
        f'<input type="hidden" name="csrf" value="{escape(session["csrf"])}">'
        '<button type="submit">Revoke session</button></form>'
    )
    return _html(200, _page("TEST ONLY series", "<p>TEST ONLY private fixture reader.</p>" + "".join(blocks) + revoke))


def _operator_page(world, headers, clock, harness):
    session = _session(world, headers, clock)
    if session is None:
        return _sign_in(world, "/ss-publish/browser/operator", clock, harness)
    if not _operator(world, session):
        return _html(404, _bare("Not found."))
    reports = []
    for report in world["runtime"].get("reports", []):
        reports.append(
            "<li>"
            f"{escape(report['publicationId'])} {escape(report['accountId'])} {escape(report['reason'])}"
            "</li>"
        )
    actions = []
    for record in world["runtime"]["records"].values():
        if record.get("publicationId") is None:
            continue
        actions.append(
            "<li>"
            f"{escape(record['publicationId'])} {escape(str(record['status']))}"
            f'{_operator_form(session["csrf"], record["publicationId"], "hold", "Hold")}'
            f'{_operator_form(session["csrf"], record["publicationId"], "remove", "Remove")}'
            "</li>"
        )
    body = (
        "<p>TEST ONLY local operator. These controls stay on this loopback service.</p>"
        f"<ul>{''.join(reports) or '<li>No reports.</li>'}</ul>"
        f"<ul>{''.join(actions) or '<li>No publications.</li>'}</ul>"
    )
    return _html(200, _page("TEST ONLY operator", body))


def _operator_form(csrf, publication_id, action, label):
    return (
        f'<form method="post" action="/ss-publish/browser/operator/{action}">'
        f'<input type="hidden" name="csrf" value="{escape(csrf)}">'
        f'<input type="hidden" name="publicationId" value="{escape(publication_id)}">'
        f"<button type=\"submit\">{label}</button></form>"
    )


def _report(world, headers, body, clock):
    session, form, problem = _cookie_form(world, headers, body, clock)
    if problem:
        return problem
    publication_id = form.get("publicationId", "")
    reason = form.get("reason", "").strip()
    record = _record_by_publication(world, publication_id)
    if record is None or _owner(world, record) != session["accountId"] or len(reason) < 1 or len(reason) > 400:
        return _html(400, _bare("Report refused."))
    world["runtime"].setdefault("reports", []).append(
        {"publicationId": publication_id, "accountId": session["accountId"], "reason": reason}
    )
    return _html(200, _page("TEST ONLY report", "<p>TEST ONLY. Report recorded.</p>"))


def _operator_hold(world, headers, body, clock):
    session, form, problem = _cookie_form(world, headers, body, clock)
    if problem:
        return problem
    if not _operator(world, session):
        return _html(404, _bare("Not found."))
    record = _record_by_publication(world, form.get("publicationId", ""))
    if record is None or record.get("operatorRemoved") or record["status"] not in ("published", "held"):
        return _html(404, _bare("Not found."))
    record["operatorHold"] = True
    record["status"] = "held"
    return _html(200, _page("TEST ONLY operator", "<p>TEST ONLY. Publication held.</p>"))


def _operator_remove(world, headers, body, clock):
    session, form, problem = _cookie_form(world, headers, body, clock)
    if problem:
        return problem
    if not _operator(world, session):
        return _html(404, _bare("Not found."))
    record = _record_by_publication(world, form.get("publicationId", ""))
    if record is None or record.get("publicationId") is None:
        return _html(404, _bare("Not found."))
    record["operatorRemoved"] = True
    record["operatorHold"] = False
    record["status"] = "withdrawn"
    return _html(200, _page("TEST ONLY operator", "<p>TEST ONLY. Publication removed.</p>"))


def _cookie_form(world, headers, body, clock):
    if headers.get("origin") != ORIGIN:
        return None, None, _json_error(403, "unauthorized")
    session = _session(world, headers, clock)
    form = dict(parse_qsl(body.decode("utf-8", errors="replace"), keep_blank_values=True))
    if session is None or not session.get("csrf") or form.get("csrf") != session["csrf"]:
        return None, None, _json_error(403, "unauthorized")
    return session, form, None


def _operator(world, session):
    account = world["accounts"].get(session["accountId"])
    return bool(account and account.get("operator") is True)


def _page(title, body):
    return (
        "<!DOCTYPE html><html lang=\"en\"><head><meta charset=\"utf-8\">"
        '<meta name="viewport" content="width=360">'
        "<style>body{margin:0;font:16px/1.4 sans-serif}main{max-width:360px;margin:0 auto;padding:12px}"
        "img{width:100%;height:auto;display:block}pre{white-space:pre-wrap}</style>"
        f"<title>{escape(title)}</title></head><body><main>{body}</main></body></html>"
    )


def _slice_response(world, record, revision, ordinal):
    if revision is None or revision["content"]["kind"] != "comic":
        return HttpResult(404, {"cache-control": "no-store"}, b"")
    for item in revision["content"]["slices"]:
        if str(item["ordinal"]) == str(ordinal):
            import base64

            raw = base64.b64decode(item["bytesBase64"])
            return HttpResult(
                200,
                {"cache-control": "no-store", "content-type": item["mediaType"]},
                raw,
            )
    return HttpResult(404, {"cache-control": "no-store"}, b"")


def _sign_in(world, nxt, clock, harness):
    csrf = _new_csrf(world, harness)
    world["runtime"].setdefault("csrf_forms", {})[csrf] = {"next": nxt, "expiresAt": clock + _seconds(SESSION_SECONDS)}
    return _html(401, _picker(world, csrf, state=None, nxt=nxt))


def _session(world, headers, clock):
    raw = headers.get("cookie", "")
    session_id = None
    for part in raw.split(";"):
        piece = part.strip()
        if piece.startswith(COOKIE + "="):
            session_id = piece.split("=", 1)[1]
    session = world["runtime"]["sessions"].get(session_id)
    if session is None or session["revoked"] or clock >= session["expiresAt"]:
        return None
    return session


def _txn_for_query(world, query):
    required = ("response_type", "client_id", "redirect_uri", "state", "code_challenge", "code_challenge_method")
    if any(key not in query for key in required):
        return None
    txn = world["runtime"]["by_state"].get(query["state"])
    if txn is None or txn["status"] == "cancelled":
        return None
    if query["client_id"] != txn["clientId"] or query["redirect_uri"] != txn["redirectUri"]:
        return None
    if query["code_challenge"] != txn["challenge"] or query["code_challenge_method"] != "S256":
        return None
    if query["response_type"] != "code":
        return None
    return txn


def _consume_form_csrf(world, csrf, nxt, clock):
    forms = world["runtime"].get("csrf_forms", {})
    record = forms.get(csrf)
    if not isinstance(record, dict):
        return False
    if clock >= record.get("expiresAt", clock):
        forms.pop(csrf, None)
        return False
    if record.get("next") != nxt:
        return False
    forms.pop(csrf, None)
    return True


def _safe_next(nxt):
    if not nxt.startswith("/ss-publish/browser/"):
        return False
    if nxt.startswith("//") or "\\" in nxt or "?" in nxt or "#" in nxt:
        return False
    if nxt in ("/ss-publish/browser/series", "/ss-publish/browser/operator"):
        return True
    return nxt.startswith(SPEC["routes"]["reader"]) or nxt.startswith(SPEC["routes"]["preview"])


def _record_by_publication(world, publication_id):
    for record in world["runtime"]["records"].values():
        if record.get("publicationId") == publication_id:
            return record
    return None


def _owner(world, record):
    return world["episodes"][record["episodeId"]]["ownerAccountId"]


def _revision(world, record):
    if record.get("publishedRevisionId") is None:
        return None
    return next(item for item in record["revisions"] if item["revisionId"] == record["publishedRevisionId"])


def _picker_needs_series():
    return None


def _html(status, text):
    return HttpResult(
        status,
        {"cache-control": "no-store", "content-type": "text/html; charset=utf-8", "referrer-policy": REFERRER_POLICY},
        text.encode("utf-8"),
    )


def _bare(message):
    return f"<!DOCTYPE html><title>TEST ONLY</title><p>{escape(message)}</p>"


def _json_error(status, code, extra=None):
    payload = {
        "protocol": PROTOCOL["protocol"],
        "ok": False,
        "command": None,
        "idempotentReplay": False,
        "error": {"code": code, "retryable": False},
    }
    headers = {"content-type": SPEC["command"]["responseContentType"], "cache-control": "no-store"}
    if extra:
        headers.update(extra)
    return HttpResult(status, headers, json.dumps(payload).encode("utf-8"))


def _json_result(status, result):
    return HttpResult(
        status,
        {"content-type": SPEC["command"]["responseContentType"], "cache-control": "no-store"},
        json.dumps(result).encode("utf-8"),
    )


def _set_cookie(value, max_age):
    return (
        f"{COOKIE}={value}; HttpOnly; SameSite=Strict; Path={COOKIE_PATH}; Max-Age={max_age}"
    )


def _new_csrf(world, harness):
    return harness.next_csrf(world) if isinstance(harness, FixtureHarness) else new_csrf()


def _new_session(world, harness):
    return harness.next_session(world) if isinstance(harness, FixtureHarness) else new_session_id()


def _hidden(html, name):
    marker = f'name="{name}" value="'
    start = html.find(marker)
    if start < 0:
        raise RuntimeError(f"missing {name}")
    rest = html[start + len(marker) :]
    return rest.split('"', 1)[0]


def _seconds(value):
    from datetime import timedelta

    return timedelta(seconds=value)
