from __future__ import annotations

import base64
import copy
import hashlib
import http.client
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
import secrets
import socket
import struct
import subprocess
import threading
import time
import unittest
from datetime import datetime, timezone
from pathlib import Path
from urllib.parse import parse_qsl, urlencode, urlsplit

from prototypes.publishing.image_decode import decode_size
from prototypes.publishing.listener import HOST, MAX_IN_FLIGHT, PORT, TIMEOUT_SECONDS, Listener, assert_loopback, live_world
from prototypes.publishing.media import facts
from prototypes.publishing.oracle import PROTOCOL, SCHEMA, fresh_world, pkce_s256
from prototypes.publishing.schema_validate import validate
from prototypes.publishing.transport import SPEC, dispatch

ORIGIN = "http://127.0.0.1:19427"
REDIRECT = "http://127.0.0.1:43121/ss-publish/callback"
EPISODES = {
    "ep_test_lane_01": ("ser_test_lane", "Fixture Lane", "North door"),
    "ep_test_lane_02": ("ser_test_lane", "Fixture Lane", "South stair"),
    "ep_test_other_01": ("ser_test_other", "Other Lane", "Their door"),
}


class ListenerTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.server = Listener()
        cls.thread = threading.Thread(target=cls.server.serve, daemon=True)
        cls.thread.start()
        for _ in range(50):
            try:
                status, _headers, body = request("GET", "/ss-publish/browser/series")
                if status == 401 and b"TEST ONLY" in body:
                    return
            except OSError:
                pass
        raise RuntimeError("listener did not accept a loopback connection")

    @classmethod
    def tearDownClass(cls):
        cls.server.shutdown()
        cls.server.server_close()

    def setUp(self):
        with self.server.state_lock:
            self.server.world = live_world()

    def tearDown(self):
        self.server.timeout_seconds = TIMEOUT_SECONDS

    def test_bind_refuses_non_loopback(self):
        with self.assertRaises(RuntimeError):
            assert_loopback("0.0.0.0")
        self.assertEqual(self.server.server_address[:2], (HOST, PORT))
        source = (Path(__file__).resolve().parents[2] / "prototypes" / "publishing" / "listener.py").read_text()
        self.assertNotIn("FixtureHarness", source)
        self.assertNotIn("loginHint", source)
        self.assertNotIn("testOnlyRedirect", source)

    def test_publish_preview_reader_and_pixels(self):
        cookie, credential, _location = sign_in("acct_test_owner_a")
        png = facts("png")
        jpeg = facts("jpeg")
        uploaded = upload_comic(credential, "ep_test_lane_01", png)
        preview = uploaded["result"]["acknowledgement"]["browserPreviewUrl"]
        revision = uploaded["result"]["acknowledgement"]["revisionId"]
        self.assertNotIn(credential["secret"], preview)
        status, headers, body = request("GET", urlsplit(preview).path, cookie=cookie)
        self.assertEqual(status, 200)
        self.assertEqual(headers["cache-control"], "no-store")
        self.assertIn(b"width=360", body)
        self.assertIn(b"North door", body)
        self.assertIn(b"South stair", body)
        self.assertNotIn(credential["secret"].encode(), body)
        asset = f"/ss-publish/browser/preview-assets/ser_test_lane/ep_test_lane_01/{revision}/1"
        status, headers, raw = request("GET", asset, cookie=cookie)
        self.assertEqual(status, 200)
        self.assertEqual(headers["content-type"], "image/png")
        self.assertEqual(decode_size(raw, 4_000_000), (120, 160))
        second = upload_comic(credential, "ep_test_lane_01", jpeg)
        second_revision = second["result"]["acknowledgement"]["revisionId"]
        self.assertNotEqual(revision, second_revision)
        status, _headers, first_bytes = request("GET", asset, cookie=cookie)
        self.assertEqual(status, 200)
        self.assertTrue(first_bytes.startswith(b"\x89PNG"))
        jpeg_asset = f"/ss-publish/browser/preview-assets/ser_test_lane/ep_test_lane_01/{second_revision}/1"
        status, headers, jpeg_bytes = request("GET", jpeg_asset, cookie=cookie)
        self.assertEqual(headers["content-type"], "image/jpeg")
        self.assertEqual(decode_size(jpeg_bytes, 4_000_000), (120, 160))
        confirmed = confirm(credential, "ep_test_lane_01", second_revision)
        canonical = confirmed["result"]["acknowledgement"]["canonicalUrl"]
        self.assertTrue(canonical.startswith(ORIGIN + "/ss-publish/browser/reader/"))
        publication_id = confirmed["result"]["acknowledgement"]["publicationId"]
        status, headers, page = request("GET", "/ss-publish/browser/reader/" + publication_id, cookie=cookie)
        self.assertEqual(status, 200)
        self.assertEqual(headers["cache-control"], "no-store")
        self.assertIn(b"TEST ONLY", page)
        reader_asset = f"/ss-publish/browser/assets/{publication_id}/1"
        status, _headers, served = request("GET", reader_asset, cookie=cookie)
        self.assertEqual(decode_size(served, 4_000_000), (120, 160))
        status, stale = command(
            "publication.confirm",
            {
                "seriesId": "ser_test_lane",
                "episodeId": "ep_test_lane_01",
                "revisionId": revision,
                "policyEligibility": "private-prototype-only",
            },
            credential,
        )
        self.assertEqual(status, 409)
        self.assertEqual(stale["error"]["code"], "stale_revision")
        status, page_headers, series = request("GET", "/ss-publish/browser/series", cookie=cookie)
        self.assertEqual(status, 200)
        text = series.decode("utf-8")
        self.assertLess(text.index("North door"), text.index("South stair"))
        self.assertNotIn("Other Lane", text)
        self.assertIn("not published", text)
        status, _headers, guessed = request("GET", f"/ss-publish/browser/assets/{publication_id}/9", cookie=cookie)
        self.assertEqual(status, 404)
        self.assertNotIn(b"\xff\xd8", guessed)
        status, denied = command(
            "publication.read",
            {"canonicalUrl": f"{ORIGIN}/ss-publish/browser/assets/{publication_id}/1"},
            credential,
        )
        self.assertEqual(denied["error"]["code"], "asset_not_addressable")
        _other_cookie, other, _other_location = sign_in("acct_test_owner_b")
        status, hidden = command("publication.read", {"canonicalUrl": canonical}, other)
        self.assertEqual(status, 403)
        self.assertEqual(hidden["error"]["code"], "not_owner")
        status, _headers, foreign = request("GET", "/ss-publish/browser/reader/" + publication_id, cookie=_other_cookie)
        self.assertEqual(status, 404)
        self.assertFalse(foreign.startswith(b"\xff\xd8"))
        self.assertNotIn(b"\x89PNG", foreign)

    def test_prose_unicode_preview_and_byte_cap(self):
        cookie, credential, _location = sign_in("acct_test_owner_a")
        first = "first snow 雪"
        second = "second snow 雪"
        opened = upload_prose(credential, "ep_test_lane_02", first)
        revision = opened["result"]["acknowledgement"]["revisionId"]
        again = upload_prose(credential, "ep_test_lane_02", second)
        preview = opened["result"]["acknowledgement"]["browserPreviewUrl"]
        status, _headers, page = request("GET", urlsplit(preview).path, cookie=cookie)
        self.assertEqual(status, 200)
        self.assertIn(first.encode(), page)
        self.assertNotIn(second.encode(), page)
        self.assertIn(revision, preview)
        self.assertNotIn(again["result"]["acknowledgement"]["revisionId"], preview)
        status, rejected = upload_prose(credential, "ep_test_lane_02", "雪" * 21846, expect_ok=False)
        self.assertEqual(status, 400)
        self.assertEqual(rejected["error"]["code"], "invalid_request")
        self.assertEqual(rejected["error"]["detail"], "schema")

    def test_replay_revoke_expiry_cancel_and_recovery(self):
        state = "state-" + secrets.token_hex(24)
        verifier = secrets.token_urlsafe(48)
        status, opened = command(
            "auth.begin",
            {
                "transportProfile": "local-http-isolated-fixture",
                "issuer": PROTOCOL["issuer"]["exact"],
                "redirectUri": REDIRECT,
                "codeChallengeMethod": "S256",
                "codeChallenge": pkce_s256(verifier),
                "state": state,
                "clientId": PROTOCOL["clientId"],
                "scope": ["prototype:creator"],
            },
        )
        self.assertEqual(status, 200)
        parts = urlsplit(opened["result"]["authorizationUrl"])
        _status, _headers, page = request("GET", parts.path + "?" + parts.query)
        _status, headers, _body = request(
            "POST",
            "/ss-publish/browser/affirm",
            urlencode({"csrf": hidden(page, "csrf"), "state": state, "accountId": "acct_test_owner_a"}).encode(),
            {"content-type": "application/x-www-form-urlencoded", "origin": ORIGIN},
        )
        query = dict(parse_qsl(urlsplit(headers["location"]).query))
        exchange_body = {
            "code": query["code"],
            "codeVerifier": verifier,
            "redirectUri": REDIRECT,
            "state": query["state"],
            "issuer": PROTOCOL["issuer"]["exact"],
        }
        status, exchanged = command("auth.exchange", exchange_body, key="exchange-replay-key01")
        self.assertEqual(status, 200)
        credential = {
            "credentialId": exchanged["result"]["credential"]["credentialId"],
            "secret": exchanged["result"]["credential"]["secret"],
        }
        status, replayed = command("auth.exchange", exchange_body, key="exchange-replay-key01")
        self.assertEqual(status, 401)
        self.assertEqual(replayed["error"]["code"], "exchange_consumed")
        self.assertNotIn("result", replayed)
        self.assertNotIn(credential["secret"], json.dumps(replayed))
        status, discovered = command("account.discover", {}, credential)
        self.assertEqual(status, 200)
        self.assertTrue(discovered["result"]["account"]["series"])
        status, same = command("account.discover", {}, credential, key="discover-live-key-0001")
        self.assertEqual(status, 200)
        with self.server.state_lock:
            stored = self.server.world["runtime"]["credentials"][credential["credentialId"]]
            stored["expiresAt"] = datetime(2020, 1, 1, tzinfo=timezone.utc)
        status, expired = command("account.discover", {}, credential, key="discover-live-key-0001")
        self.assertEqual(status, 401)
        self.assertEqual(expired["error"]["code"], "credential_expired")
        self.assertNotIn("result", expired)
        cookie, credential, _location = sign_in("acct_test_owner_a")
        status, revoked = command("auth.revoke", {}, credential)
        self.assertTrue(revoked["result"]["revoked"])
        status, after = command("account.discover", {}, credential)
        self.assertEqual(after["error"]["code"], "credential_revoked")
        status, page_headers, page = request("GET", "/ss-publish/browser/series", cookie=cookie)
        self.assertEqual(status, 200)
        self.assertIn(b"Fixture Lane", page)
        csrf = hidden(page, "csrf")
        status, headers, body = request(
            "POST",
            "/ss-publish/browser/session/revoke",
            urlencode({"csrf": csrf}).encode(),
            {"content-type": "application/x-www-form-urlencoded", "origin": ORIGIN},
            cookie=cookie,
        )
        self.assertEqual(status, 200)
        self.assertIn(b"Session revoked", body)
        status, _headers, signed_out = request("GET", "/ss-publish/browser/series", cookie=cookie)
        self.assertEqual(status, 401)
        self.assertIn(b"TEST ONLY", signed_out)
        cookie, credential, _location = sign_in("acct_test_owner_a")
        uploaded = upload_comic(credential, "ep_test_lane_01", facts("png"))
        status, cancelled = command(
            "operation.cancel",
            {"targetIdempotencyKey": "upload-finished-key01"},
            credential,
        )
        self.assertEqual(cancelled["error"]["code"], "not_found")
        status, terminal = command(
            "operation.cancel",
            {"targetIdempotencyKey": uploaded["idempotencyKey"]},
            credential,
        )
        self.assertEqual(terminal["error"]["code"], "already_terminal")
        status, missing = command("auth.recover", {}, credential)
        self.assertEqual(missing["error"]["code"], "unknown_command")
        status, refresh = command(
            "auth.exchange",
            {
                "code": "not-a-live-code",
                "codeVerifier": "b" * 43,
                "redirectUri": REDIRECT,
                "state": "synthetic-state-does-not-exist-000000000000",
                "issuer": PROTOCOL["issuer"]["exact"],
                "grantType": "refresh_token",
            },
        )
        self.assertEqual(refresh["error"]["code"], "unsupported_grant")
        self.assertNotIn(credential["secret"], json.dumps(refresh))

    def test_browser_csrf_host_origin_and_withdrawal(self):
        status, rejected = command(
            "auth.begin",
            begin_body("owner-a-state-00000000000000000000000000000001"),
            origin="http://evil.example",
        )
        self.assertEqual(status, 403)
        self.assertEqual(rejected["error"]["code"], "unauthorized")
        status, _headers, bad_host = request(
            "POST",
            "/ss-publish/commands",
            b"{}",
            {"content-type": "application/json", "host": "127.0.0.1:9"},
        )
        self.assertEqual(status, 400)
        status, hinted = command(
            "auth.begin",
            {**begin_body("owner-a-state-00000000000000000000000000000002"), "loginHint": "acct_test_owner_a"},
        )
        self.assertEqual(hinted["error"]["detail"], "schema")
        cookie, credential, _location = sign_in("acct_test_owner_a")
        status, _headers, page = request("GET", "/ss-publish/browser/authorize?state=nope")
        self.assertEqual(status, 400)
        uploaded = upload_comic(credential, "ep_test_lane_01", facts("png"))
        revision = uploaded["result"]["acknowledgement"]["revisionId"]
        confirmed = confirm(credential, "ep_test_lane_01", revision)
        publication_id = confirmed["result"]["acknowledgement"]["publicationId"]
        canonical = confirmed["result"]["acknowledgement"]["canonicalUrl"]
        asset = f"/ss-publish/browser/assets/{publication_id}/1"
        status, headers, raw = request("GET", asset, cookie=cookie)
        self.assertEqual(status, 200)
        self.assertEqual(headers["cache-control"], "no-store")
        self.assertTrue(raw.startswith(b"\x89PNG"))
        command(
            "publication.withdraw",
            {
                "seriesId": "ser_test_lane",
                "episodeId": "ep_test_lane_01",
                "revisionId": revision,
                "publicationId": publication_id,
            },
            credential,
        )
        status, replayed = command(
            "publication.confirm",
            {
                "seriesId": "ser_test_lane",
                "episodeId": "ep_test_lane_01",
                "revisionId": revision,
                "policyEligibility": "private-prototype-only",
            },
            credential,
            key=confirmed["idempotencyKey"],
        )
        self.assertTrue(replayed["idempotentReplay"])
        self.assertFalse(replayed["result"]["acknowledgement"]["authoritative"])
        self.assertNotIn("status", replayed["result"]["acknowledgement"])
        status, current = command("publication.read", {"canonicalUrl": canonical}, credential)
        self.assertEqual(status, 410)
        self.assertEqual(current["error"]["code"], "withdrawn")
        status, _headers, gone = request("GET", asset, cookie=cookie)
        self.assertEqual(status, 410)
        self.assertEqual(gone, b"")
        status, _headers, html = request("GET", "/ss-publish/browser/reader/" + publication_id, cookie=cookie)
        self.assertEqual(status, 410)
        self.assertNotIn(b"\x89PNG", html)
        preview = uploaded["result"]["acknowledgement"]["browserPreviewUrl"]
        status, _headers, still = request("GET", f"/ss-publish/browser/preview-assets/ser_test_lane/ep_test_lane_01/{revision}/1", cookie=cookie)
        self.assertEqual(status, 200)
        self.assertTrue(still.startswith(b"\x89PNG"))
        self.assertIn(revision, preview)
        status, _headers, form = request("GET", "/ss-publish/browser/series", cookie=cookie)
        csrf = hidden(form, "csrf")
        status, _headers, _body = request(
            "POST",
            "/ss-publish/browser/affirm",
            urlencode({"csrf": "not-the-token", "state": "state-0000000000000000000000000000000000000000", "accountId": "acct_test_owner_a"}).encode(),
            {"content-type": "application/x-www-form-urlencoded", "origin": "https://evil.example"},
            cookie=cookie,
        )
        self.assertEqual(status, 403)
        status, _headers, _body = request(
            "POST",
            "/ss-publish/browser/session/revoke",
            urlencode({"csrf": csrf}).encode(),
            {"content-type": "application/x-www-form-urlencoded", "origin": "http://127.0.0.1:1"},
            cookie=cookie,
        )
        self.assertEqual(status, 403)

    def test_operator_report_hold_remove(self):
        cookie, credential, _location = sign_in("acct_test_owner_a")
        uploaded = upload_comic(credential, "ep_test_lane_01", facts("png"))
        revision = uploaded["result"]["acknowledgement"]["revisionId"]
        confirmed = confirm(credential, "ep_test_lane_01", revision)
        publication_id = confirmed["result"]["acknowledgement"]["publicationId"]
        canonical = confirmed["result"]["acknowledgement"]["canonicalUrl"]
        status, _headers, page = request("GET", "/ss-publish/browser/reader/" + publication_id, cookie=cookie)
        csrf = hidden(page, "csrf")
        status, _headers, reported = request(
            "POST",
            "/ss-publish/browser/report",
            urlencode({"csrf": csrf, "publicationId": publication_id, "reason": "synthetic report"}).encode(),
            {"content-type": "application/x-www-form-urlencoded", "origin": ORIGIN},
            cookie=cookie,
        )
        self.assertEqual(status, 200)
        self.assertIn(b"Report recorded", reported)
        status, _headers, hidden_page = request("GET", "/ss-publish/browser/operator", cookie=cookie)
        self.assertEqual(status, 404)
        operator_cookie, _operator, _location = sign_in("acct_test_operator")
        status, _headers, board = request("GET", "/ss-publish/browser/operator", cookie=operator_cookie)
        self.assertEqual(status, 200)
        self.assertIn(b"synthetic report", board)
        self.assertIn(publication_id.encode(), board)
        operator_csrf = hidden(board, "csrf")
        status, _headers, held = request(
            "POST",
            "/ss-publish/browser/operator/hold",
            urlencode({"csrf": operator_csrf, "publicationId": publication_id}).encode(),
            {"content-type": "application/x-www-form-urlencoded", "origin": ORIGIN},
            cookie=operator_cookie,
        )
        self.assertEqual(status, 200)
        status, discovered = command("account.discover", {}, credential)
        episode = discovered["result"]["account"]["series"][0]["episodes"][0]
        self.assertEqual(episode["publication"]["status"], "held")
        status, unreadable = command("publication.read", {"canonicalUrl": canonical}, credential)
        self.assertEqual(unreadable["error"]["code"], "not_found")
        status, _headers, blocked = request("GET", f"/ss-publish/browser/assets/{publication_id}/1", cookie=cookie)
        self.assertEqual(status, 410)
        self.assertEqual(blocked, b"")
        status, _headers, held_page = request("GET", "/ss-publish/browser/reader/" + publication_id, cookie=cookie)
        self.assertEqual(status, 410)
        self.assertNotIn(b"\x89PNG", held_page)
        status, _headers, board = request("GET", "/ss-publish/browser/operator", cookie=operator_cookie)
        operator_csrf = hidden(board, "csrf")
        status, _headers, removed = request(
            "POST",
            "/ss-publish/browser/operator/remove",
            urlencode({"csrf": operator_csrf, "publicationId": publication_id}).encode(),
            {"content-type": "application/x-www-form-urlencoded", "origin": ORIGIN},
            cookie=operator_cookie,
        )
        self.assertEqual(status, 200)
        status, gone = command("publication.read", {"canonicalUrl": canonical}, credential)
        self.assertEqual(gone["error"]["code"], "withdrawn")
        status, preview = command(
            "episode.preview",
            {"seriesId": "ser_test_lane", "episodeId": "ep_test_lane_01", "revisionId": revision},
            credential,
        )
        self.assertEqual(preview["error"]["code"], "not_found")
        status, _headers, asset = request(
            "GET",
            f"/ss-publish/browser/preview-assets/ser_test_lane/ep_test_lane_01/{revision}/1",
            cookie=cookie,
        )
        self.assertEqual(status, 404)
        self.assertEqual(asset, b"")
        status, withdrawn = command(
            "publication.withdraw",
            {
                "seriesId": "ser_test_lane",
                "episodeId": "ep_test_lane_01",
                "revisionId": revision,
                "publicationId": publication_id,
            },
            credential,
        )
        self.assertEqual(status, 200)
        self.assertFalse(withdrawn["result"]["acknowledgement"]["authoritative"])

    def test_body_cap_closes_before_the_payload(self):
        conn = http.client.HTTPConnection(HOST, PORT, timeout=5)
        conn.putrequest("POST", "/ss-publish/commands")
        conn.putheader("Content-Type", "application/json")
        conn.putheader("Content-Length", "28000001")
        conn.endheaders()
        status = conn.getresponse().status
        conn.close()
        self.assertEqual(status, 413)

    def test_hold_and_remove_do_not_yield_to_the_owner(self):
        cookie, credential, _location = sign_in("acct_test_owner_a")
        png = facts("png")
        uploaded = upload_comic(credential, "ep_test_lane_01", png)
        revision = uploaded["result"]["acknowledgement"]["revisionId"]
        confirmed = confirm(credential, "ep_test_lane_01", revision)
        publication_id = confirmed["result"]["acknowledgement"]["publicationId"]
        canonical = confirmed["result"]["acknowledgement"]["canonicalUrl"]
        original = _stored_revision(self, "ep_test_lane_01", revision)
        _operator_cookie, _operator_credential, _operator_location = sign_in("acct_test_operator")
        board = _post_operator(self, _operator_cookie, "hold", publication_id)
        self.assertIn(b"Publication held", board)
        status, replayed = command(
            "publication.confirm",
            {
                "seriesId": "ser_test_lane",
                "episodeId": "ep_test_lane_01",
                "revisionId": revision,
                "policyEligibility": "private-prototype-only",
            },
            credential,
            key=confirmed["idempotencyKey"],
        )
        self.assertTrue(replayed["idempotentReplay"])
        self.assertFalse(replayed["result"]["acknowledgement"]["authoritative"])
        status, blocked = command(
            "publication.confirm",
            {
                "seriesId": "ser_test_lane",
                "episodeId": "ep_test_lane_01",
                "revisionId": revision,
                "policyEligibility": "private-prototype-only",
            },
            credential,
        )
        self.assertEqual(status, 404)
        self.assertEqual(blocked["error"]["code"], "not_found")
        self.assertNotIn("content", blocked)
        self._assert_held(cookie, credential, publication_id, canonical)
        jpeg = facts("jpeg")
        uploaded_again = upload_comic(credential, "ep_test_lane_01", jpeg)
        new_revision = uploaded_again["result"]["acknowledgement"]["revisionId"]
        status, still = command(
            "publication.confirm",
            {
                "seriesId": "ser_test_lane",
                "episodeId": "ep_test_lane_01",
                "revisionId": new_revision,
                "policyEligibility": "private-prototype-only",
            },
            credential,
        )
        self.assertEqual(still["error"]["code"], "not_found")
        self._assert_held(cookie, credential, publication_id, canonical)
        status, headers, preview = request(
            "GET",
            f"/ss-publish/browser/preview-assets/ser_test_lane/ep_test_lane_01/{revision}/1",
            cookie=cookie,
        )
        self.assertEqual(status, 200)
        self.assertEqual(headers["content-type"], "image/png")
        self.assertEqual(preview, base64.b64decode(original["content"]["slices"][0]["bytesBase64"]))
        status, shown = command(
            "episode.preview",
            {"seriesId": "ser_test_lane", "episodeId": "ep_test_lane_01", "revisionId": revision},
            credential,
        )
        self.assertEqual(shown["result"]["content"]["slices"][0]["bytesBase64"], original["content"]["slices"][0]["bytesBase64"])
        self.assertEqual(shown["result"]["contentSha256"], original["contentSha256"])
        removed = _post_operator(self, _operator_cookie, "remove", publication_id)
        self.assertIn(b"Publication removed", removed)
        self.assertEqual(_stored_revision(self, "ep_test_lane_01", revision), original)
        self.assertEqual(_stored_revision(self, "ep_test_lane_01", new_revision)["content"]["slices"][0]["sha256"], jpeg["sha256"])
        before = len(self.server.world["runtime"]["records"]["ep_test_lane_01"]["revisions"])
        status, uploaded_after = upload_prose(credential, "ep_test_lane_01", "should not seal", expect_ok=False)
        self.assertEqual(uploaded_after["error"]["code"], "withdrawn")
        self.assertEqual(len(self.server.world["runtime"]["records"]["ep_test_lane_01"]["revisions"]), before)
        status, confirm_after = command(
            "publication.confirm",
            {
                "seriesId": "ser_test_lane",
                "episodeId": "ep_test_lane_01",
                "revisionId": new_revision,
                "policyEligibility": "private-prototype-only",
            },
            credential,
        )
        self.assertEqual(confirm_after["error"]["code"], "withdrawn")
        status, withheld = command(
            "episode.preview",
            {"seriesId": "ser_test_lane", "episodeId": "ep_test_lane_01", "revisionId": revision},
            credential,
        )
        self.assertEqual(withheld["error"]["code"], "not_found")
        self.assertNotIn("content", withheld)
        status, read = command("publication.read", {"canonicalUrl": canonical}, credential)
        self.assertEqual(read["error"]["code"], "withdrawn")
        self.assertNotIn("content", read)
        status, _headers, asset = request("GET", f"/ss-publish/browser/assets/{publication_id}/1", cookie=cookie)
        self.assertEqual(status, 410)
        self.assertEqual(asset, b"")
        status, _headers, exact = request(
            "GET",
            f"/ss-publish/browser/preview-assets/ser_test_lane/ep_test_lane_01/{revision}/1",
            cookie=cookie,
        )
        self.assertEqual(status, 404)
        self.assertEqual(exact, b"")
        status, _headers, page = request("GET", "/ss-publish/browser/series", cookie=cookie)
        status, _headers, refused = request(
            "POST",
            "/ss-publish/browser/operator/hold",
            urlencode({"csrf": hidden(page, "csrf"), "publicationId": publication_id}).encode(),
            {"content-type": "application/x-www-form-urlencoded", "origin": ORIGIN},
            cookie=cookie,
        )
        self.assertEqual(status, 404)
        status, _headers, anonymous = request(
            "POST",
            "/ss-publish/browser/operator/remove",
            urlencode({"csrf": "missing", "publicationId": publication_id}).encode(),
            {"content-type": "application/x-www-form-urlencoded", "origin": ORIGIN},
        )
        self.assertEqual(status, 403)
        self.assertIn(b"unauthorized", anonymous)
        status, outsider = command(
            "episode.upload",
            {
                "seriesId": "ser_test_lane",
                "episodeId": "ep_test_lane_01",
                "selection": {"mode": "selected-episode-only"},
                "publicMetadata": {
                    "seriesTitle": "Fixture Lane",
                    "episodeTitle": "North door",
                    "synopsis": "Synthetic episode.",
                    "language": "en",
                },
                "completion": "seal",
                "content": {"kind": "prose", "mediaType": "text/plain; charset=utf-8", "text": "nope"},
            },
            _operator_credential,
        )
        self.assertEqual(outsider["error"]["code"], "not_owner")

    def _assert_held(self, cookie, credential, publication_id, canonical):
        status, discovered = command("account.discover", {}, credential)
        episode = next(
            item
            for series in discovered["result"]["account"]["series"]
            for item in series["episodes"]
            if item["episodeId"] == "ep_test_lane_01"
        )
        self.assertEqual(episode["publication"]["status"], "held")
        status, current = command("publication.read", {"canonicalUrl": canonical}, credential)
        self.assertEqual(current["error"]["code"], "not_found")
        self.assertNotIn("content", current)
        status, _headers, asset = request("GET", f"/ss-publish/browser/assets/{publication_id}/1", cookie=cookie)
        self.assertEqual(status, 410)
        self.assertEqual(asset, b"")
        record = self.server.world["runtime"]["records"]["ep_test_lane_01"]
        self.assertTrue(record["operatorHold"])
        self.assertEqual(record["status"], "held")

    def test_sign_in_csrf_expires_once_and_matches_next(self):
        status, _headers, page = request("GET", "/ss-publish/browser/series")
        self.assertEqual(status, 401)
        csrf = hidden(page, "csrf")
        self.assertIn(b'name="next" value="/ss-publish/browser/series"', page)
        mismatch = _affirm_next(csrf, "/ss-publish/browser/operator", "acct_test_owner_a")
        self.assertEqual(mismatch, 403)
        self.assertIn(csrf, self.server.world["runtime"]["csrf_forms"])
        opened = _affirm_next(csrf, "/ss-publish/browser/series", "acct_test_owner_a")
        self.assertEqual(opened, 302)
        self.assertNotIn(csrf, self.server.world["runtime"]["csrf_forms"])
        replayed = _affirm_next(csrf, "/ss-publish/browser/series", "acct_test_owner_a")
        self.assertEqual(replayed, 403)
        status, _headers, page = request("GET", "/ss-publish/browser/series")
        csrf = hidden(page, "csrf")
        with self.server.state_lock:
            self.server.world["runtime"]["csrf_forms"][csrf]["expiresAt"] = datetime(2020, 1, 1, tzinfo=timezone.utc)
        expired = _affirm_next(csrf, "/ss-publish/browser/series", "acct_test_owner_a")
        self.assertEqual(expired, 403)
        self.assertNotIn(csrf, self.server.world["runtime"]["csrf_forms"])

    def test_rejected_connections_close(self):
        self.server.timeout_seconds = 1
        oversized = raw_response(
            b"POST /ss-publish/commands HTTP/1.1\r\nHost: 127.0.0.1:19427\r\n"
            b"Content-Type: application/json\r\nContent-Length: 28000001\r\n\r\n"
        )
        self.assertTrue(oversized.startswith(b"HTTP/1.1 413 "))
        self.assertIn(b"connection: close", oversized.lower())
        limited = _json_body(oversized)
        self.assertEqual(limited["error"], {"code": "limit_exceeded", "retryable": False})
        self.assertEqual(SPEC["errorStatus"]["limit_exceeded"], 413)
        self.assertEqual(validate({"$ref": "#/$defs/response"}, limited, SCHEMA), [])
        self.assertTrue(validate({"$ref": "#/$defs/busyResponse"}, limited, SCHEMA))
        chunked = raw_response(
            b"POST /ss-publish/commands HTTP/1.1\r\nHost: 127.0.0.1:19427\r\n"
            b"Transfer-Encoding: chunked\r\nContent-Length: 3\r\n\r\n1\r\nZ\r\n"
        )
        self.assertTrue(chunked.startswith(b"HTTP/1.1 400 "))
        self.assertIn(b"connection: close", chunked.lower())
        duplicate_length = raw_response(
            b"POST /ss-publish/commands HTTP/1.1\r\nHost: 127.0.0.1:19427\r\n"
            b"Content-Length: 2\r\nContent-Length: 2\r\n\r\n{}"
        )
        self.assertTrue(duplicate_length.startswith(b"HTTP/1.1 400 "))
        duplicate_host = raw_response(
            b"GET /ss-publish/browser/series HTTP/1.1\r\nHost: 127.0.0.1:19427\r\nHost: 127.0.0.1:19427\r\n\r\n"
        )
        self.assertTrue(duplicate_host.startswith(b"HTTP/1.1 400 "))
        malformed = raw_response(
            b"POST /ss-publish/commands HTTP/1.1\r\nHost: 127.0.0.1:19427\r\nContent-Length: 12x\r\n\r\n"
        )
        self.assertTrue(malformed.startswith(b"HTTP/1.1 400 "))
        partial = raw_response(
            b"POST /ss-publish/commands HTTP/1.1\r\nHost: 127.0.0.1:19427\r\n"
            b"Content-Type: application/json\r\nContent-Length: 400\r\n\r\n"
            b'{"idempotencyKey":"partial-body-key-0001"'
        )
        self.assertTrue(partial.startswith(b"HTTP/1.1 400 "))
        self.assertNotIn("partial-body-key-0001", self.server.world["runtime"]["operations"])
        silent = socket.create_connection((HOST, PORT), timeout=3)
        silent.settimeout(3)
        try:
            data = silent.recv(200)
        except socket.timeout:
            data = b""
        silent.close()
        self.assertTrue(data == b"" or data.startswith(b"HTTP/1.1 400 "))
        self.server.timeout_seconds = 5
        holders = [socket.create_connection((HOST, PORT), timeout=3) for _ in range(MAX_IN_FLIGHT)]
        try:
            deadline = time.time() + 2
            while time.time() < deadline and self.server.entered < MAX_IN_FLIGHT:
                time.sleep(0.05)
            self.assertEqual(self.server.entered, MAX_IN_FLIGHT)
            busy = raw_response(b"GET /ss-publish/browser/series HTTP/1.1\r\nHost: 127.0.0.1:19427\r\n\r\n")
            self.assertTrue(busy.startswith(b"HTTP/1.1 503 "))
            self.assertIn(b"connection: close", busy.lower())
            admitted = _json_body(busy)
            self.assertEqual(admitted["error"], {"code": "limit_exceeded", "retryable": True})
            self.assertEqual(SPEC["busy"]["status"], 503)
            self.assertEqual(SPEC["busy"]["retryable"], True)
            self.assertEqual(validate({"$ref": "#/$defs/busyResponse"}, admitted, SCHEMA), [])
            self.assertEqual(validate({"$ref": "#/$defs/response"}, admitted, SCHEMA), [])
            self.assertTrue(validate({"$ref": "#/$defs/errorObject"}, admitted["error"], SCHEMA))
        finally:
            for holder in holders:
                holder.close()
            deadline = time.time() + 2
            while time.time() < deadline and self.server.entered:
                time.sleep(0.05)

    def test_busy_admission_does_not_follow_a_trickle(self):
        self.server.timeout_seconds = 8
        holders = [socket.create_connection((HOST, PORT), timeout=3) for _ in range(MAX_IN_FLIGHT)]
        trick = None
        silent = None
        stop = threading.Event()
        try:
            deadline = time.monotonic() + 2
            while time.monotonic() < deadline and self.server.entered < MAX_IN_FLIGHT:
                time.sleep(0.02)
            self.assertEqual(self.server.entered, MAX_IN_FLIGHT)
            trick = socket.create_connection((HOST, PORT), timeout=3)

            def trickle():
                for _ in range(12):
                    if stop.is_set():
                        return
                    try:
                        trick.send(b"G")
                    except OSError:
                        return
                    time.sleep(0.15)

            threading.Thread(target=trickle, daemon=True).start()
            time.sleep(0.25)
            started = time.monotonic()
            followed = raw_response(b"GET /ss-publish/browser/series HTTP/1.1\r\nHost: 127.0.0.1:19427\r\n\r\n")
            elapsed = time.monotonic() - started
            self.assertLess(elapsed, 0.9)
            self._assert_busy(followed)
            silent = socket.create_connection((HOST, PORT), timeout=3)
            time.sleep(0.05)
            started = time.monotonic()
            after_silent = raw_response(b"GET /ss-publish/browser/series HTTP/1.1\r\nHost: 127.0.0.1:19427\r\n\r\n")
            self.assertLess(time.monotonic() - started, 0.9)
            self._assert_busy(after_silent)
            self.assertEqual(self.server.world["runtime"]["operations"], {})
        finally:
            stop.set()
            for sock in holders + [trick, silent]:
                if sock is not None:
                    try:
                        sock.close()
                    except OSError:
                        pass

    def _assert_busy(self, raw):
        self.assertTrue(raw.startswith(b"HTTP/1.1 503 "))
        self.assertIn(b"connection: close", raw.lower())
        admitted = _json_body(raw)
        self.assertEqual(admitted["error"], {"code": "limit_exceeded", "retryable": True})
        self.assertEqual(validate({"$ref": "#/$defs/busyResponse"}, admitted, SCHEMA), [])
        self.assertEqual(validate({"$ref": "#/$defs/response"}, admitted, SCHEMA), [])
        self.assertTrue(validate({"$ref": "#/$defs/errorObject"}, admitted["error"], SCHEMA))

    def test_demo_catalogue_is_separate_from_creator_state(self):
        world = fresh_world()
        before = copy.deepcopy(world)
        pages = {
            "/": b"Enter the synthetic demo",
            "/ss-publish/demo": b"Read the comic",
            "/ss-publish/demo/paper-harbor": b"Read the prose",
            "/ss-publish/demo/paper-harbor/the-fold": b"A hull does not need a coastline.",
            "/ss-publish/demo/paper-harbor/margin-note": b"She would not say that the harbor",
        }
        for path, needle in pages.items():
            result = dispatch(world, _get(path))
            self.assertEqual(result.status, 200, path)
            self.assertIn(b"Local prototype. Synthetic demo. Not a real publication.", result.body)
            self.assertIn(b"Scroll Studio", result.body)
            self.assertIn(needle, result.body)
            self.assertNotIn(b"Fixture Lane", result.body)
            self.assertNotIn(b"North door", result.body)
        home = dispatch(world, _get("/"))
        self.assertIn(b"Read the sample comic and prose", home.body)
        self.assertIn(b"Creator test sign-in", home.body)
        self.assertNotIn(b"panel-fold.jpg", home.body)
        self.assertNotIn(b"not a catalogue of visitors", home.body)
        self.assertNotIn(b"A hull does not need a coastline.", home.body)
        browse = dispatch(world, _get("/ss-publish/demo"))
        self.assertIn(b"The Fold", browse.body)
        self.assertIn(b"What the Margin Kept", browse.body)
        self.assertIn(b"Read the prose", browse.body)
        self.assertIn(b'width="1152"', browse.body)
        picture = dispatch(world, _get("/ss-publish/demo/assets/panel-fold.jpg"))
        self.assertEqual(picture.status, 200)
        self.assertTrue(picture.body.startswith(b"\xff\xd8"))
        self.assertEqual(picture.headers["content-type"], "image/jpeg")
        missing = dispatch(world, _get("/ss-publish/demo/assets/../listener.py"))
        self.assertEqual(missing.status, 404)
        rejected = dispatch(world, {"method": "POST", "path": "/", "headers": {"host": "127.0.0.1:19427"}, "body": b""})
        self.assertEqual(rejected.status, 405)
        self.assertEqual(world, before)
        door = dispatch(world, _get("/ss-publish/browser/series"))
        self.assertEqual(door.status, 401)
        self.assertIn(b"TEST ONLY", door.body)
        for name in ("demo_site.py", "transport.py"):
            text = (Path(__file__).resolve().parent / name).read_text(encoding="utf-8")
            self.assertNotIn("reader_content_kit", text)

    def test_demo_reading_in_chrome(self):
        chrome = _Chrome()
        shots = Path("/opt/cursor/artifacts")
        shots.mkdir(parents=True, exist_ok=True)
        try:
            chrome.call(
                "Emulation.setDeviceMetricsOverride",
                {"width": 1280, "height": 800, "deviceScaleFactor": 1, "mobile": False},
            )
            chrome.navigate(ORIGIN + "/")
            self.assertIn("Read the sample comic and prose", chrome.text("document.body.innerText"))
            self.assertIn("Enter the synthetic demo", chrome.text("document.body.innerText"))
            self.assertNotIn("A hull does not need a coastline.", chrome.text("document.body.innerText"))
            _shot(chrome, shots / "followup-home-desktop.png")
            chrome.click('a.enter')
            self.assertIn("Paper Harbor", chrome.text("document.body.innerText"))
            self.assertIn("Read the comic", chrome.text("document.body.innerText"))
            self.assertIn("Read the prose", chrome.text("document.body.innerText"))
            _shot(chrome, shots / "followup-browse-desktop.png")
            chrome.click('a[href="/ss-publish/demo/paper-harbor"]')
            chrome.click('a[href="/ss-publish/demo/paper-harbor/the-fold"]')
            self.assertIn("The Fold", chrome.text("document.body.innerText"))
            self.assertIn("A hull does not need a coastline.", chrome.text("document.body.innerText"))
            _shot(chrome, shots / "followup-comic-desktop.png")
            chrome.call(
                "Emulation.setDeviceMetricsOverride",
                {"width": 390, "height": 844, "deviceScaleFactor": 1, "mobile": False},
            )
            chrome.navigate(ORIGIN + "/ss-publish/demo/paper-harbor/margin-note")
            self.assertIn("What the Margin Kept", chrome.text("document.body.innerText"))
            self.assertIn("cold pear", chrome.text("document.body.innerText"))
            self.assertTrue(chrome.eval(_NO_OVERFLOW))
            _shot(chrome, shots / "followup-prose-390.png")
            _fit(chrome, 320, 700)
            chrome.navigate(ORIGIN + "/ss-publish/demo/paper-harbor/the-fold")
            self.assertTrue(chrome.eval(_NO_OVERFLOW))
            _shot(chrome, shots / "followup-comic-320.png")
            self.assertIn("The Fold", chrome.text("document.body.innerText"))
        finally:
            chrome.close()

    def test_demo_keyboard_and_narrow_layout(self):
        chrome = _Chrome()
        try:
            chrome.navigate(ORIGIN + "/")
            _press(chrome, "Tab", 9)
            self.assertIn("Skip to content", chrome.eval(_ACTIVE))
            _press(chrome, "Enter", 13)
            self.assertEqual(chrome.eval("document.activeElement && document.activeElement.id"), "content")
            focused = _tab_until(chrome, "Enter the synthetic demo")
            self.assertIn("Enter the synthetic demo", focused)
            _press(chrome, "Enter", 13)
            chrome.drain(1.0)
            self.assertIn("Read the comic", chrome.text("document.body.innerText"))
            chrome.navigate(ORIGIN + "/ss-publish/demo/paper-harbor/the-fold")
            self.assertEqual(chrome.eval("document.querySelectorAll('h1').length"), 1)
            self.assertTrue(chrome.eval("!!document.querySelector('main#content') && !!document.querySelector('header')"))
            self.assertEqual(chrome.eval("Array.from(document.images).filter(img => !img.alt).length"), 0)
            self.assertEqual(chrome.eval("document.images[0].getAttribute('width')"), "1152")
            self.assertEqual(chrome.eval("document.images[0].getAttribute('height')"), "864")
            _tab_until(chrome, "Next: What the Margin Kept")
            _press(chrome, "Enter", 13)
            chrome.drain(1.0)
            self.assertIn("What the Margin Kept", chrome.text("document.body.innerText"))
            _tab_until(chrome, "Previous: The Fold")
            _press(chrome, "Enter", 13)
            chrome.drain(1.0)
            self.assertIn("The Fold", chrome.text("document.body.innerText"))
            for width in (320, 390):
                _fit(chrome, width, 800)
                chrome.navigate(ORIGIN + "/ss-publish/demo/paper-harbor/the-fold")
                self.assertTrue(chrome.eval(_NO_OVERFLOW), width)
                chrome.navigate(ORIGIN + "/ss-publish/demo/paper-harbor/margin-note")
                self.assertTrue(chrome.eval(_NO_OVERFLOW), width)
        finally:
            chrome.close()

    def test_chrome_sign_in_sends_service_origin(self):
        callback = _CallbackServer(43121)
        foreign = _ForeignFormServer(43122)
        chrome = None
        try:
            verifier = secrets.token_urlsafe(48)
            state = "state-" + secrets.token_hex(24)
            status, opened = command(
                "auth.begin",
                {
                    "transportProfile": "local-http-isolated-fixture",
                    "issuer": PROTOCOL["issuer"]["exact"],
                    "redirectUri": REDIRECT,
                    "codeChallengeMethod": "S256",
                    "codeChallenge": pkce_s256(verifier),
                    "state": state,
                    "clientId": PROTOCOL["clientId"],
                    "scope": ["prototype:creator"],
                },
            )
            self.assertEqual(status, 200)
            chrome = _Chrome()
            chrome.navigate(opened["result"]["authorizationUrl"])
            self.assertEqual(chrome.header("referrer-policy"), "same-origin")
            chrome.click('button[value="acct_test_owner_a"]')
            affirm = chrome.request_matching("POST", "/ss-publish/browser/affirm")
            self.assertEqual(affirm["headers"].get("Origin"), ORIGIN)
            self.assertTrue(affirm["headers"].get("Referer", "").startswith(ORIGIN + "/ss-publish/browser/authorize?"))
            deadline = time.time() + 3
            while time.time() < deadline and not callback.hits:
                time.sleep(0.05)
            self.assertEqual(len(callback.hits), 1)
            hit = callback.hits[0]
            self.assertIsNone(hit["referer"])
            query = dict(parse_qsl(urlsplit("http://127.0.0.1:43121" + hit["path"]).query))
            self.assertEqual(list(query), ["code", "state"])
            self.assertEqual(query["state"], state)
            status, exchanged = command(
                "auth.exchange",
                {
                    "code": query["code"],
                    "codeVerifier": verifier,
                    "redirectUri": REDIRECT,
                    "state": query["state"],
                    "issuer": PROTOCOL["issuer"]["exact"],
                },
            )
            self.assertEqual(status, 200, exchanged)
            credential = {
                "credentialId": exchanged["result"]["credential"]["credentialId"],
                "secret": exchanged["result"]["credential"]["secret"],
            }
            uploaded = upload_comic(credential, "ep_test_lane_01", facts("png"))
            preview = uploaded["result"]["acknowledgement"]["browserPreviewUrl"]
            chrome.navigate(preview)
            preview_text = chrome.text("document.body.innerText")
            self.assertIn("North door", preview_text)
            self.assertNotIn("synthetic publishing fixtures", preview_text)
            revision = uploaded["result"]["acknowledgement"]["revisionId"]
            confirmed = confirm(credential, "ep_test_lane_01", revision)
            chrome.navigate(confirmed["result"]["acknowledgement"]["canonicalUrl"])
            reader_text = chrome.text("document.body.innerText")
            self.assertIn("North door", reader_text)
            self.assertIn("TEST ONLY", reader_text)
            self.assertNotIn(credential["secret"], reader_text)
            chrome.navigate(foreign.url("/cross"))
            chrome.click("#go")
            cross = chrome.request_matching("POST", "/ss-publish/browser/affirm")
            self.assertEqual(cross["headers"].get("Origin"), "http://127.0.0.1:43122")
            self.assertEqual(chrome.response_matching("/ss-publish/browser/affirm"), 403)
            chrome.navigate(foreign.url("/null"))
            chrome.click("#go")
            nulled = chrome.request_matching("POST", "/ss-publish/browser/affirm")
            self.assertEqual(nulled["headers"].get("Origin"), "null")
            self.assertEqual(chrome.response_matching("/ss-publish/browser/affirm"), 403)
        finally:
            if chrome is not None:
                chrome.close()
            callback.close()
            foreign.close()

    def test_phone_viewport_screenshot(self):
        cookie, credential, _location = sign_in("acct_test_owner_a")
        uploaded = upload_comic(credential, "ep_test_lane_01", facts("png"))
        revision = uploaded["result"]["acknowledgement"]["revisionId"]
        confirmed = confirm(credential, "ep_test_lane_01", revision)
        publication_id = confirmed["result"]["acknowledgement"]["publicationId"]
        url = f"{ORIGIN}/ss-publish/browser/reader/{publication_id}"
        out = Path("/tmp/scroll-reader-360.png")
        text = capture(url, cookie, out)
        self.assertIn("North door", text)
        self.assertIn("TEST ONLY", text)
        self.assertGreater(out.stat().st_size, 1000)
        artifact = Path("/opt/cursor/artifacts/reader-360.png")
        artifact.parent.mkdir(parents=True, exist_ok=True)
        artifact.write_bytes(out.read_bytes())


def _stored_revision(test, episode_id, revision_id):
    with test.server.state_lock:
        for revision in test.server.world["runtime"]["records"][episode_id]["revisions"]:
            if revision["revisionId"] == revision_id:
                return copy.deepcopy(revision)
    raise AssertionError(revision_id)


def _post_operator(test, cookie, action, publication_id):
    status, _headers, page = request("GET", "/ss-publish/browser/operator", cookie=cookie)
    if status != 200:
        raise AssertionError(page)
    status, _headers, body = request(
        "POST",
        f"/ss-publish/browser/operator/{action}",
        urlencode({"csrf": hidden(page, "csrf"), "publicationId": publication_id}).encode(),
        {"content-type": "application/x-www-form-urlencoded", "origin": ORIGIN},
        cookie=cookie,
    )
    if status != 200:
        raise AssertionError(body)
    return body


def _affirm_next(csrf, nxt, account_id):
    status, _headers, _body = request(
        "POST",
        "/ss-publish/browser/affirm",
        urlencode({"csrf": csrf, "next": nxt, "accountId": account_id}).encode(),
        {"content-type": "application/x-www-form-urlencoded", "origin": ORIGIN},
    )
    return status


class _HitServer(ThreadingHTTPServer):
    allow_reuse_address = True


class _CallbackServer:
    def __init__(self, port):
        self.hits = []

        class Handler(BaseHTTPRequestHandler):
            def do_GET(handler):
                if handler.path.startswith("/ss-publish/callback"):
                    self.hits.append({"path": handler.path, "referer": handler.headers.get("Referer")})
                handler.send_response(200)
                handler.send_header("content-type", "text/plain")
                handler.end_headers()
                handler.wfile.write(b"ok")

            def log_message(handler, format, *args):
                return

        self.httpd = _HitServer(("127.0.0.1", port), Handler)
        self.thread = threading.Thread(target=self.httpd.serve_forever, daemon=True)
        self.thread.start()

    def close(self):
        self.httpd.shutdown()
        self.httpd.server_close()


class _ForeignFormServer:
    def __init__(self, port):
        self.port = port
        form = (
            '<form method="post" action="http://127.0.0.1:19427/ss-publish/browser/affirm">'
            '<input type="hidden" name="csrf" value="not-the-token">'
            '<input type="hidden" name="accountId" value="acct_test_owner_a">'
            '<input type="hidden" name="state" value="state-foreign-00000000000000000000000000000000">'
            '<button id="go" type="submit">go</button></form>'
        )
        pages = {
            "/cross": "<!DOCTYPE html><title>cross</title><body>" + form + "</body>",
            "/null": '<!DOCTYPE html><head><meta name="referrer" content="no-referrer"></head><title>null</title><body>'
            + form
            + "</body>",
        }

        class Handler(BaseHTTPRequestHandler):
            def do_GET(handler):
                page = pages.get(handler.path)
                if page is None:
                    handler.send_response(404)
                    handler.end_headers()
                    return
                body = page.encode()
                handler.send_response(200)
                handler.send_header("content-type", "text/html; charset=utf-8")
                handler.send_header("content-length", str(len(body)))
                handler.end_headers()
                handler.wfile.write(body)

            def log_message(handler, format, *args):
                return

        self.httpd = _HitServer(("127.0.0.1", port), Handler)
        self.thread = threading.Thread(target=self.httpd.serve_forever, daemon=True)
        self.thread.start()

    def url(self, path):
        return f"http://127.0.0.1:{self.port}{path}"

    def close(self):
        self.httpd.shutdown()
        self.httpd.server_close()


class _Chrome:
    def __init__(self):
        self.profile = f"/tmp/ss-chrome-signin-{secrets.token_hex(4)}"
        os.makedirs(self.profile, exist_ok=True)
        self.proc = subprocess.Popen(
            [
                "google-chrome",
                "--headless=new",
                "--disable-gpu",
                "--no-sandbox",
                "--disable-dev-shm-usage",
                "--no-first-run",
                f"--user-data-dir={self.profile}",
                "--remote-debugging-port=9335",
                "about:blank",
            ],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            start_new_session=True,
        )
        version = None
        for _ in range(80):
            try:
                with urllib_get("http://127.0.0.1:9335/json/version") as raw:
                    version = json.loads(raw.read().decode())
                    break
            except OSError:
                time.sleep(0.1)
        if version is None:
            self.close()
            raise AssertionError("chrome did not open a debugging port")
        self.sock = ws_connect(version["webSocketDebuggerUrl"])
        self.sock.settimeout(5)
        self.events = []
        self.session = None
        target = self.call("Target.createTarget", {"url": "about:blank"})
        attached = self.call(
            "Target.attachToTarget",
            {"targetId": target["result"]["targetId"], "flatten": True},
        )
        self.session = attached["result"]["sessionId"]
        self.call("Network.enable")
        self.call("Page.enable")

    def call(self, method, params=None):
        cdp.next_id += 1
        message = {"id": cdp.next_id, "method": method, "params": params or {}}
        if self.session is not None:
            message["sessionId"] = self.session
        ws_send(self.sock, json.dumps(message))
        while True:
            opcode, payload = ws_read(self.sock)
            if opcode != 1:
                continue
            parsed = json.loads(payload)
            if parsed.get("method"):
                self.events.append(parsed)
            if parsed.get("id") == cdp.next_id:
                if "error" in parsed:
                    raise AssertionError(parsed["error"])
                return parsed

    def drain(self, seconds):
        self.sock.settimeout(0.2)
        end = time.time() + seconds
        while time.time() < end:
            try:
                opcode, payload = ws_read(self.sock)
            except OSError:
                continue
            if opcode == 1:
                parsed = json.loads(payload)
                if parsed.get("method"):
                    self.events.append(parsed)
        self.sock.settimeout(5)

    def navigate(self, url):
        self.mark = len(self.events)
        self.call("Page.navigate", {"url": url})
        self.drain(1.2)

    def click(self, selector):
        self.mark = len(self.events)
        found = False
        for _ in range(30):
            if self.eval(f"document.querySelector('{selector}') ? 'yes' : 'no'") == "yes":
                found = True
                break
            time.sleep(0.1)
        if not found:
            raise AssertionError(self.eval("document.body ? document.body.innerText : 'empty'"))
        self.eval(f"document.querySelector('{selector}').click()")
        self.drain(1.5)

    def eval(self, expression):
        result = self.call("Runtime.evaluate", {"expression": expression, "returnByValue": True})
        return result["result"]["result"].get("value")

    def text(self, expression):
        value = ""
        for _ in range(20):
            value = self.eval(expression) or ""
            if value:
                return value
            time.sleep(0.1)
        return value

    def header(self, name):
        for event in reversed(self.events):
            if event.get("method") != "Network.responseReceived":
                continue
            for key, value in event["params"]["response"]["headers"].items():
                if key.lower() == name.lower():
                    return value
        raise AssertionError(name)

    def request_matching(self, method, path):
        for event in self.events[self.mark :]:
            if event.get("method") != "Network.requestWillBeSent":
                continue
            request = event["params"]["request"]
            if request["method"] == method and path in request["url"]:
                return request
        raise AssertionError(path)

    def response_matching(self, path):
        for event in reversed(self.events[self.mark :]):
            if event.get("method") != "Network.responseReceived":
                continue
            response = event["params"]["response"]
            if path in response["url"]:
                return response["status"]
        raise AssertionError(path)

    def close(self):
        try:
            self.sock.close()
        except OSError:
            pass
        try:
            os.killpg(self.proc.pid, 15)
            self.proc.wait(timeout=3)
        except ProcessLookupError:
            pass
        except subprocess.TimeoutExpired:
            os.killpg(self.proc.pid, 9)
            self.proc.wait(timeout=3)


def _get(path):
    return {"method": "GET", "path": path, "headers": {"host": "127.0.0.1:19427"}, "body": b""}


_ACTIVE = "document.activeElement ? (document.activeElement.innerText || document.activeElement.id || '') : ''"
_NO_OVERFLOW = "document.documentElement.scrollWidth <= document.documentElement.clientWidth + 1"


def _fit(chrome, width, height):
    chrome.call(
        "Emulation.setDeviceMetricsOverride",
        {"width": width, "height": height, "deviceScaleFactor": 1, "mobile": False},
    )


def _press(chrome, key, code):
    for phase in ("keyDown", "keyUp"):
        chrome.call(
            "Input.dispatchKeyEvent",
            {
                "type": phase,
                "key": key,
                "code": key,
                "windowsVirtualKeyCode": code,
                "nativeVirtualKeyCode": code,
            },
        )


def _tab_until(chrome, label):
    seen = ""
    for _ in range(12):
        _press(chrome, "Tab", 9)
        seen = chrome.eval(_ACTIVE) or ""
        if label in seen:
            return seen
    raise AssertionError(seen)


def _shot(chrome, path):
    png = chrome.call("Page.captureScreenshot", {"format": "png"})
    path.write_bytes(base64.b64decode(png["result"]["data"]))


def _json_body(raw):
    return json.loads(raw.split(b"\r\n\r\n", 1)[1].decode("utf-8"))


def raw_response(payload):
    sock = socket.create_connection((HOST, PORT), timeout=5)
    sock.settimeout(5)
    sock.sendall(payload)
    data = b""
    while True:
        try:
            chunk = sock.recv(4096)
        except socket.timeout:
            break
        if not chunk:
            break
        data += chunk
    sock.close()
    return data


def begin_body(state):
    verifier = "verifier-scroll-studio-live-listener-0000000001"
    return {
        "transportProfile": "local-http-isolated-fixture",
        "issuer": PROTOCOL["issuer"]["exact"],
        "redirectUri": REDIRECT,
        "codeChallengeMethod": "S256",
        "codeChallenge": pkce_s256(verifier),
        "state": state,
        "clientId": PROTOCOL["clientId"],
        "scope": ["prototype:creator"],
    }


def sign_in(account_id):
    state = "state-" + secrets.token_hex(24)
    verifier = secrets.token_urlsafe(48)
    status, opened = command(
        "auth.begin",
        {
            "transportProfile": "local-http-isolated-fixture",
            "issuer": PROTOCOL["issuer"]["exact"],
            "redirectUri": REDIRECT,
            "codeChallengeMethod": "S256",
            "codeChallenge": pkce_s256(verifier),
            "state": state,
            "clientId": PROTOCOL["clientId"],
            "scope": ["prototype:creator"],
        },
    )
    if status != 200:
        raise AssertionError(opened)
    parts = urlsplit(opened["result"]["authorizationUrl"])
    status, _headers, page = request("GET", parts.path + "?" + parts.query)
    if status != 200 or b"TEST ONLY" not in page or account_id.encode() not in page:
        raise AssertionError(page[:200])
    status, headers, _body = request(
        "POST",
        "/ss-publish/browser/affirm",
        urlencode({"csrf": hidden(page, "csrf"), "state": state, "accountId": account_id}).encode(),
        {"content-type": "application/x-www-form-urlencoded", "origin": ORIGIN},
    )
    if status != 302:
        raise AssertionError(status)
    location = headers["location"]
    query = dict(parse_qsl(urlsplit(location).query))
    cookie = headers["set-cookie"].split(";", 1)[0].split("=", 1)[1]
    status, exchanged = command(
        "auth.exchange",
        {
            "code": query["code"],
            "codeVerifier": verifier,
            "redirectUri": REDIRECT,
            "state": query["state"],
            "issuer": PROTOCOL["issuer"]["exact"],
        },
    )
    if status != 200:
        raise AssertionError(exchanged)
    credential = exchanged["result"]["credential"]
    proof = {"credentialId": credential["credentialId"], "secret": credential["secret"]}
    if not proof["credentialId"].startswith("cred_live_") or not proof["secret"].startswith("sek_live_"):
        raise AssertionError("credential was not minted by the live helper")
    return cookie, proof, location


def upload_comic(credential, episode_id, media):
    series_id, series_title, episode_title = EPISODES[episode_id]
    status, response = command(
        "episode.upload",
        {
            "seriesId": series_id,
            "episodeId": episode_id,
            "selection": {"mode": "selected-episode-only", "localStoryRef": "local-story", "localEpisodeRef": episode_id},
            "publicMetadata": {
                "seriesTitle": series_title,
                "episodeTitle": episode_title,
                "synopsis": "Synthetic episode.",
                "language": "en",
            },
            "completion": "seal",
            "expectedSha256": media["sha256"],
            "content": {
                "kind": "comic",
                "slices": [
                    {
                        "ordinal": 1,
                        "mediaType": media["mediaType"],
                        "width": media["width"],
                        "height": media["height"],
                        "byteLength": media["byteLength"],
                        "sha256": media["sha256"],
                        "bytesBase64": media["bytesBase64"],
                    }
                ],
            },
        },
        credential,
    )
    if status != 200:
        raise AssertionError(response)
    return response


def upload_prose(credential, episode_id, text, expect_ok=True):
    series_id, series_title, episode_title = EPISODES[episode_id]
    raw = text.encode("utf-8")
    status, response = command(
        "episode.upload",
        {
            "seriesId": series_id,
            "episodeId": episode_id,
            "selection": {"mode": "selected-episode-only"},
            "publicMetadata": {
                "seriesTitle": series_title,
                "episodeTitle": episode_title,
                "synopsis": "Synthetic episode.",
                "language": "en",
            },
            "completion": "seal",
            "expectedSha256": hashlib.sha256(raw).hexdigest(),
            "content": {"kind": "prose", "mediaType": "text/plain; charset=utf-8", "text": text},
        },
        credential,
    )
    if expect_ok and status != 200:
        raise AssertionError(response)
    if expect_ok:
        return response
    return status, response


def confirm(credential, episode_id, revision_id):
    series_id = EPISODES[episode_id][0]
    status, response = command(
        "publication.confirm",
        {
            "seriesId": series_id,
            "episodeId": episode_id,
            "revisionId": revision_id,
            "policyEligibility": "private-prototype-only",
        },
        credential,
    )
    if status != 200:
        raise AssertionError(response)
    return response


def command(name, body, credential=None, key=None, origin=None):
    envelope = {
        "protocol": PROTOCOL["protocol"],
        "command": name,
        "idempotencyKey": key or secrets.token_hex(16),
        "body": body,
    }
    if credential is not None:
        envelope["credential"] = credential
    data = json.dumps(envelope).encode("utf-8")
    headers = {"content-type": "application/json"}
    if origin is not None:
        headers["origin"] = origin
    status, _headers, raw = request("POST", "/ss-publish/commands", data, headers)
    parsed = json.loads(raw.decode("utf-8"))
    parsed["idempotencyKey"] = envelope["idempotencyKey"]
    return status, parsed


def request(method, path, body=None, headers=None, cookie=None):
    conn = http.client.HTTPConnection(HOST, PORT, timeout=20)
    sent = dict(headers or {})
    sent.setdefault("host", "127.0.0.1:19427")
    if cookie is not None:
        sent["cookie"] = "ss_publish_fixture=" + cookie
    conn.request(method, path, body=body, headers=sent)
    response = conn.getresponse()
    payload = response.read()
    header_map = {key.lower(): value for key, value in response.getheaders()}
    conn.close()
    return response.status, header_map, payload


def hidden(page, name):
    marker = f'name="{name}" value="'.encode()
    start = page.find(marker)
    if start < 0:
        raise AssertionError(name)
    rest = page[start + len(marker) :]
    return rest.split(b'"', 1)[0].decode("ascii")


def capture(url, cookie, out):
    profile = Path("/tmp/ss-chrome-profile")
    profile.mkdir(parents=True, exist_ok=True)
    proc = subprocess.Popen(
        [
            "google-chrome",
            "--headless=new",
            "--disable-gpu",
            "--no-sandbox",
            "--disable-dev-shm-usage",
            "--no-first-run",
            f"--user-data-dir={profile}",
            "--window-size=360,780",
            "--remote-debugging-port=9333",
            "about:blank",
        ],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        start_new_session=True,
    )
    try:
        version = None
        for _ in range(80):
            try:
                with urllib_get("http://127.0.0.1:9333/json/version") as raw:
                    version = json.loads(raw.read().decode())
                    break
            except OSError:
                time.sleep(0.1)
        if version is None:
            raise AssertionError("chrome did not open a debugging port")
        browser = ws_connect(version["webSocketDebuggerUrl"])
        target = cdp(browser, "Target.createTarget", {"url": "about:blank"})
        session = cdp(browser, "Target.attachToTarget", {"targetId": target["result"]["targetId"], "flatten": True})
        session_id = session["result"]["sessionId"]
        cdp(browser, "Network.enable", {}, session_id)
        cdp(browser, "Page.enable", {}, session_id)
        cdp(
            browser,
            "Emulation.setDeviceMetricsOverride",
            {"width": 360, "height": 780, "deviceScaleFactor": 1, "mobile": True},
            session_id,
        )
        cdp(
            browser,
            "Network.setCookie",
            {
                "name": "ss_publish_fixture",
                "value": cookie,
                "url": ORIGIN,
                "path": "/ss-publish/browser",
                "httpOnly": True,
                "sameSite": "Strict",
            },
            session_id,
        )
        cdp(browser, "Page.navigate", {"url": url}, session_id)
        time.sleep(0.8)
        text = cdp(
            browser,
            "Runtime.evaluate",
            {"expression": "document.body.innerText", "returnByValue": True},
            session_id,
        )
        shot = cdp(browser, "Page.captureScreenshot", {"format": "png"}, session_id)
        out.write_bytes(base64.b64decode(shot["result"]["data"]))
        browser.close()
        return text["result"]["result"]["value"]
    finally:
        try:
            os.killpg(proc.pid, 15)
            proc.wait(timeout=3)
        except ProcessLookupError:
            pass
        except subprocess.TimeoutExpired:
            os.killpg(proc.pid, 9)
            proc.wait(timeout=3)


def urllib_get(url):
    import urllib.request

    return urllib.request.urlopen(url, timeout=0.2)


def cdp(sock, method, params, session_id=None):
    cdp.next_id += 1
    message = {"id": cdp.next_id, "method": method, "params": params}
    if session_id is not None:
        message["sessionId"] = session_id
    ws_send(sock, json.dumps(message))
    while True:
        opcode, payload = ws_read(sock)
        if opcode != 1:
            continue
        parsed = json.loads(payload)
        if parsed.get("id") == cdp.next_id:
            if "error" in parsed:
                raise AssertionError(parsed["error"])
            return parsed


cdp.next_id = 0


def ws_connect(url):
    parts = urlsplit(url)
    sock = socket.create_connection((parts.hostname, parts.port), 5)
    key = base64.b64encode(os.urandom(16)).decode()
    request = (
        f"GET {parts.path} HTTP/1.1\r\nHost: {parts.hostname}:{parts.port}\r\n"
        "Upgrade: websocket\r\nConnection: Upgrade\r\n"
        f"Sec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n"
    )
    sock.sendall(request.encode())
    data = b""
    while b"\r\n\r\n" not in data:
        data += sock.recv(4096)
    return sock


def ws_send(sock, text):
    payload = text.encode()
    mask = os.urandom(4)
    header = bytearray([0x81])
    length = len(payload)
    if length < 126:
        header.append(0x80 | length)
    else:
        header.append(0x80 | 126)
        header.extend(struct.pack(">H", length))
    header.extend(mask)
    masked = bytes(byte ^ mask[index % 4] for index, byte in enumerate(payload))
    sock.sendall(header + masked)


def ws_read(sock):
    header = _read(sock, 2)
    opcode = header[0] & 0x0F
    length = header[1] & 0x7F
    if length == 126:
        length = struct.unpack(">H", _read(sock, 2))[0]
    elif length == 127:
        length = struct.unpack(">Q", _read(sock, 8))[0]
    if header[1] & 0x80:
        mask = _read(sock, 4)
        data = bytes(byte ^ mask[index % 4] for index, byte in enumerate(_read(sock, length)))
    else:
        data = _read(sock, length)
    return opcode, data


def _read(sock, length):
    data = b""
    while len(data) < length:
        chunk = sock.recv(length - len(data))
        if not chunk:
            raise OSError("socket closed")
        data += chunk
    return data


class StaticToonsTests(unittest.TestCase):
    def test_export_links_stay_in_the_bundle_or_on_the_live_site(self):
        import tempfile
        from prototypes.publishing.export_toons import APPROVED_SITE, export_toons

        out = Path(tempfile.mkdtemp(prefix="toons-export-"))
        root = export_toons(out)
        pages = list(root.rglob("*.html"))
        self.assertGreaterEqual(len(pages), 4)
        bundled = {path.relative_to(root).as_posix() for path in root.rglob("*") if path.is_file()}
        self.assertIn("assets/panel-fold.jpg", bundled)
        self.assertIn("assets/Karla.ttf", bundled)
        self.assertIn("assets/toons.css", bundled)
        home = (root / "index.html").read_text(encoding="utf-8")
        self.assertIn("Read the sample comic and prose", home)
        self.assertIn("Enter the synthetic demo", home)
        self.assertNotIn("panel-fold.jpg", home)
        self.assertNotIn("ss-publish", home)
        self.assertNotIn("localhost", home)
        self.assertNotIn("19427", home)
        for page in pages:
            text = page.read_text(encoding="utf-8")
            self.assertNotIn("<script", text.lower())
            self.assertNotIn("Fixture sign-in", text)
            self.assertNotIn("localhost", text)
            self.assertNotIn("127.0.0.1", text)
            self.assertNotIn("ss-publish", text)
            self.assertIn("Prototype. Synthetic sample. Not a real publication.", text)
            self.assertNotIn("Local prototype", text)
            for match in re_attr(text):
                ref = match
                if ref.startswith("#"):
                    continue
                if ref in APPROVED_SITE:
                    continue
                target = (page.parent / ref).resolve()
                self.assertTrue(str(target).startswith(str(root.resolve())), ref)
                self.assertTrue(target.is_file(), ref)

    def test_toons_static_in_chrome(self):
        import tempfile
        from http.server import ThreadingHTTPServer
        from functools import partial
        from prototypes.publishing.export_toons import export_toons

        out = Path(tempfile.mkdtemp(prefix="toons-serve-"))
        export_toons(out)
        handler = partial(__import__("http.server", fromlist=["SimpleHTTPRequestHandler"]).SimpleHTTPRequestHandler, directory=str(out))
        httpd = ThreadingHTTPServer(("127.0.0.1", 8877), handler)
        thread = threading.Thread(target=httpd.serve_forever, daemon=True)
        thread.start()
        chrome = _Chrome()
        shots = Path("/opt/cursor/artifacts")
        try:
            chrome.call(
                "Emulation.setDeviceMetricsOverride",
                {"width": 1280, "height": 800, "deviceScaleFactor": 1, "mobile": False},
            )
            chrome.navigate("http://127.0.0.1:8877/toons/")
            self.assertIn("Read the sample comic and prose", chrome.text("document.body.innerText"))
            self.assertNotIn("panel-fold", chrome.eval("document.documentElement.outerHTML"))
            self.assertEqual(chrome.eval("document.images.length"), 1)
            _shot(chrome, shots / "toons-home-desktop.png")
            chrome.click('a.button')
            self.assertIn("Read the comic", chrome.text("document.body.innerText"))
            self.assertIn("The Fold", chrome.text("document.body.innerText"))
            _shot(chrome, shots / "toons-series-desktop.png")
            chrome.click('a[href="the-fold.html"]')
            self.assertIn("A hull does not need a coastline.", chrome.text("document.body.innerText"))
            _shot(chrome, shots / "toons-comic-desktop.png")
            _fit(chrome, 390, 844)
            chrome.navigate("http://127.0.0.1:8877/toons/paper-harbor/margin-note.html")
            self.assertIn("cold pear", chrome.text("document.body.innerText"))
            self.assertTrue(chrome.eval(_NO_OVERFLOW))
            _shot(chrome, shots / "toons-prose-390.png")
            _fit(chrome, 320, 700)
            chrome.navigate("http://127.0.0.1:8877/toons/paper-harbor/the-fold.html")
            self.assertTrue(chrome.eval(_NO_OVERFLOW))
            _shot(chrome, shots / "toons-comic-320.png")
        finally:
            chrome.close()
            httpd.shutdown()
            httpd.server_close()

    def test_late_resources_do_not_move_reading_controls(self):
        import tempfile
        from http.server import SimpleHTTPRequestHandler
        from prototypes.publishing.export_toons import export_toons

        out = Path(tempfile.mkdtemp(prefix="toons-stable-"))
        root = export_toons(out)
        css = (root / "assets" / "toons.css").read_text(encoding="utf-8")
        self.assertIn("font-display:optional", css)
        self.assertNotIn("font-display:swap", css)
        self.assertIn("width:min(100%,1152px)", css)
        self.assertIn("aspect-ratio:1152/864", css)
        self.assertIn("background:#202326", css)
        home = (root / "index.html").read_text(encoding="utf-8")
        self.assertIn('rel="preload"', home)
        self.assertIn("as=\"font\"", home)
        self.assertNotIn("panel-", home)
        self.assertNotIn("<script", home.lower())

        gate = threading.Event()

        class Handler(SimpleHTTPRequestHandler):
            def __init__(self, *args, **kwargs):
                super().__init__(*args, directory=str(out), **kwargs)

            def do_GET(self):
                if self.path.split("?", 1)[0].endswith((".jpg", ".ttf")):
                    gate.wait(8)
                super().do_GET()

            def log_message(self, fmt, *args):
                return

        httpd = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        port = httpd.server_address[1]
        thread = threading.Thread(target=httpd.serve_forever, daemon=True)
        thread.start()
        chrome = _Chrome()
        shots = Path("/opt/cursor/artifacts")
        try:
            chrome.call("Network.setCacheDisabled", {"cacheDisabled": True})
            chrome.call(
                "Page.addScriptToEvaluateOnNewDocument",
                {"source": "window.__shifts=[];new PerformanceObserver(l=>{for (const e of l.getEntries()) if(!e.hadRecentInput) window.__shifts.push(e.value)}).observe({type:'layout-shift',buffered:true});"},
            )
            checks = (
                ("series-desktop", 1280, 800, "/toons/paper-harbor/index.html", "a.button", "Read the comic"),
                ("comic-desktop", 1280, 800, "/toons/paper-harbor/the-fold.html", 'nav.ep a[href="margin-note.html"]', "Next: What the Margin Kept"),
                ("series-390", 390, 844, "/toons/paper-harbor/index.html", "a.button", "Read the comic"),
                ("comic-390", 390, 844, "/toons/paper-harbor/the-fold.html", 'nav.ep a[href="margin-note.html"]', "Next: What the Margin Kept"),
            )
            for name, width, height, path, selector, label in checks:
                gate.clear()
                _fit(chrome, width, height)
                chrome.navigate(f"http://127.0.0.1:{port}{path}")
                self.assertIn(label, chrome.text("document.body.innerText"))
                chrome.eval(f"document.querySelector('{selector}').scrollIntoView({{block:'center'}})")
                time.sleep(0.5)
                before = _place(chrome, selector)
                self.assertIn(label, before["hit"])
                self.assertGreater(min(before["imgs"]), 200)
                self.assertLessEqual(before["overflow"], 1)
                if name.endswith("390") or name.startswith("comic"):
                    _shot(chrome, shots / f"stable-{name}-held.png")
                gate.set()
                deadline = time.time() + 5
                while time.time() < deadline:
                    if chrome.eval("[...document.querySelectorAll('figure img, .card img')].every(img => img.complete && img.naturalWidth > 0)"):
                        break
                    time.sleep(0.1)
                else:
                    self.fail(name)
                time.sleep(0.4)
                after = _place(chrome, selector)
                self.assertIn(label, after["hit"], name)
                self.assertLess(abs(after["y"] - before["y"]), 1, name)
                self.assertLess(abs(after["view"] - before["view"]), 1, name)
                self.assertEqual(after["imgs"], before["imgs"], name)
                self.assertLess(after["shifts"], 0.001, name)
                self.assertLessEqual(after["overflow"], 1, name)
                if name.endswith("390") or name.startswith("comic"):
                    _shot(chrome, shots / f"stable-{name}-settled.png")
        finally:
            gate.set()
            chrome.close()
            httpd.shutdown()
            httpd.server_close()


def _place(chrome, selector):
    return chrome.eval(
        "(() => {"
        f"const el = document.querySelector('{selector}');"
        "const r = el.getBoundingClientRect();"
        "const hit = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);"
        "const link = hit && hit.closest ? hit.closest('a') : null;"
        "return {"
        "y: Math.round((r.top + window.scrollY) * 100) / 100,"
        "view: Math.round(r.top * 100) / 100,"
        "hit: link ? link.textContent : '',"
        "imgs: [...document.querySelectorAll('figure img, .card img')].map(img => Math.round(img.getBoundingClientRect().height * 100) / 100),"
        "shifts: (window.__shifts || []).reduce((sum, value) => sum + value, 0),"
        "overflow: document.documentElement.scrollWidth - document.documentElement.clientWidth"
        "};})()"
    )


def re_attr(text):
    import re
    return re.findall(r'''(?:href|src)="([^"]+)"''', text)


if __name__ == "__main__":
    unittest.main()
