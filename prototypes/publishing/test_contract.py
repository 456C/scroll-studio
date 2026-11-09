from __future__ import annotations

import base64
import copy
import hashlib
import json
import struct
import unittest
import zlib
from pathlib import Path

from prototypes.publishing.consumer_example import FIXTURES, TEMPLATES, _interp, run_sequence
from prototypes.publishing.fixture_harness import FixtureHarness
from prototypes.publishing.media import facts
from prototypes.publishing.oracle import (
    LIMITS,
    PROTOCOL,
    SCHEMA,
    accept_client_storage,
    apply,
    fresh_world,
    loopback_bind_allowed,
    parse_clock,
)
from prototypes.publishing.runtime_rules import new_authorization_code, new_credential, server_now

ROOT = Path(__file__).resolve().parents[2]
SEQUENCES = FIXTURES / "sequences"
VERIFIERS = {
    "ownerA": "fixture-verifier-owner-a-scroll-studio-test-only-001",
    "ownerB": "fixture-verifier-owner-b-scroll-studio-test-only-001",
}


class ContractTests(unittest.TestCase):
    def test_sequences(self):
        paths = sorted(SEQUENCES.glob("*.json"))
        self.assertGreaterEqual(len(paths), 20)
        for path in paths:
            with self.subTest(path.name):
                run_sequence(path)

    def test_exchange_replay_does_not_return_the_secret(self):
        world = fresh_world()
        body = _begin_body()
        first, harness = _send(world, "auth.begin", "replay-secret-begin-01", body, login="acct_test_owner_a")
        again, _again_harness = _send(world, "auth.begin", "replay-secret-begin-01", body, login="acct_test_owner_a")
        self.assertEqual(first["result"]["authorizationUrl"], again["result"]["authorizationUrl"])
        self.assertNotIn("testOnlyRedirect", first["result"])
        exchange_body = _exchange_body(harness)
        opened, _opened_harness = _send(world, "auth.exchange", "replay-secret-exchange1", exchange_body)
        replayed, _replay_harness = _send(world, "auth.exchange", "replay-secret-exchange1", exchange_body)
        self.assertFalse(replayed["ok"])
        self.assertEqual(replayed["error"]["code"], "exchange_consumed")
        self.assertNotIn("result", replayed)
        self.assertTrue(opened["result"]["credential"]["secret"].startswith("sek_test_"))

    def test_begin_rejects(self):
        document = json.loads((FIXTURES / "begin-rejects.json").read_text(encoding="utf-8"))
        self.assertEqual(document["stateEntropy"], "synthetic-fixture-not-csprng")
        base = _begin_body()
        for index, case in enumerate(document["cases"]):
            with self.subTest(case["name"]):
                body = copy.deepcopy(base)
                body.update(case["overlay"])
                response = apply(fresh_world(), _envelope("auth.begin", f"begin-reject-{index:04d}", body))
                self.assertFalse(response["ok"])
                self.assertEqual(response["error"]["code"], case["errorCode"])
                if "detail" in case:
                    self.assertEqual(response["error"].get("detail"), case["detail"])

    def test_schema_matches_protocol_limits(self):
        slice_props = SCHEMA["$defs"]["slice"]["properties"]
        self.assertEqual(slice_props["byteLength"]["maximum"], LIMITS["maxSliceBytes"])
        self.assertEqual(slice_props["width"]["maximum"], LIMITS["maxSliceWidth"])
        self.assertEqual(slice_props["height"]["maximum"], LIMITS["maxSliceHeight"])
        self.assertEqual(SCHEMA["$defs"]["comic"]["properties"]["slices"]["maxItems"], LIMITS["maxSlices"])
        self.assertEqual(SCHEMA["$defs"]["prose"]["properties"]["text"]["maxLength"], LIMITS["maxProseBytes"])
        metadata = SCHEMA["$defs"]["metadata"]["properties"]
        self.assertEqual(set(metadata), set(PROTOCOL["publicMetadataAllowlist"]))
        self.assertEqual(metadata["seriesTitle"]["maxLength"], LIMITS["maxTitleChars"])
        self.assertEqual(metadata["synopsis"]["maxLength"], LIMITS["maxSynopsisChars"])
        self.assertEqual(SCHEMA["$defs"]["errorObject"]["properties"]["code"]["enum"], PROTOCOL["errorCodes"])
        self.assertNotIn("termsAccepted", SCHEMA["$defs"]["confirmBody"]["properties"])
        self.assertNotIn("loginHint", SCHEMA["$defs"]["beginBody"]["properties"])
        self.assertNotIn("fixture", SCHEMA["properties"])
        self.assertIn("exchange_consumed", PROTOCOL["errorCodes"])
        commands = set(PROTOCOL["commands"])
        self.assertEqual(commands, set(PROTOCOL["mutatingCommands"]) | set(PROTOCOL["readCommands"]))
        self.assertFalse(set(PROTOCOL["mutatingCommands"]) & set(PROTOCOL["readCommands"]))

    def test_desktop_numbers_still_match_the_contract(self):
        reader = (ROOT / "src" / "app" / "reader_ui.rs").read_text(encoding="utf-8")
        export = (ROOT / "src" / "export.rs").read_text(encoding="utf-8")
        self.assertIn("360.0_f32", reader)
        self.assertEqual(PROTOCOL["phoneViewportWidthPx"], 360)
        self.assertIn("WEBTOON_IMAGE_LIMIT: usize = 100", export)
        self.assertIn("WEBTOON_FILE_LIMIT: u64 = 2_000_000", export)
        self.assertIn("WEBTOON_EPISODE_LIMIT: u64 = 20_000_000", export)
        self.assertEqual(LIMITS["maxSlices"], 100)
        self.assertEqual(LIMITS["maxSliceBytes"], 2_000_000)
        self.assertEqual(LIMITS["maxEpisodeBytes"], 20_000_000)

    def test_storage_and_bind_rules(self):
        self.assertIsNone(accept_client_storage({"mode": "os-credential-store", "plaintextFallback": False}))
        self.assertEqual(
            accept_client_storage(
                {
                    "mode": "session-memory-only",
                    "plaintextFallback": False,
                    "label": "session-memory-only",
                    "persistence": "process-memory",
                }
            ),
            None,
        )
        self.assertEqual(accept_client_storage({"mode": "plaintext-file"}), "plaintext_forbidden")
        self.assertEqual(
            accept_client_storage({"mode": "os-credential-store", "plaintextFallback": True}),
            "plaintext_fallback",
        )
        self.assertEqual(
            accept_client_storage(
                {"mode": "session-memory-only", "plaintextFallback": False, "persistence": "process-memory"}
            ),
            "missing_label",
        )
        enabled = PROTOCOL["transport"]["enabledProfile"]
        self.assertTrue(loopback_bind_allowed("127.0.0.1", enabled))
        self.assertFalse(loopback_bind_allowed("0.0.0.0", enabled))
        self.assertFalse(loopback_bind_allowed("::", enabled))
        self.assertFalse(loopback_bind_allowed("127.0.0.1", "production-secure-transport"))

    def test_network_command_rejects_oracle_hooks(self):
        body = _begin_body()
        body["loginHint"] = "acct_test_owner_a"
        response = apply(fresh_world(), _envelope("auth.begin", "hook-login-00000001", body))
        self.assertEqual(response["error"]["code"], "invalid_request")
        hooked = _envelope("auth.begin", "hook-fixture-0000001", _begin_body())
        hooked["fixture"] = {"nonProductionLabel": "TEST-ONLY", "clock": PROTOCOL["defaultClock"]}
        response = apply(fresh_world(), hooked)
        self.assertEqual(response["error"]["code"], "invalid_request")

    def test_live_secrets_are_not_fixture_counters(self):
        first_id, first_secret = new_credential()
        second_id, second_secret = new_credential()
        self.assertNotEqual(first_secret, second_secret)
        self.assertFalse(first_secret.startswith("sek_test_"))
        self.assertFalse(first_id.startswith("cred_test_"))
        self.assertGreaterEqual(len(new_authorization_code()), 32)
        self.assertIsNotNone(server_now().tzinfo)
        oracle_text = (ROOT / "prototypes" / "publishing" / "oracle.py").read_text(encoding="utf-8")
        self.assertNotIn("sek_test_", oracle_text)
        harness_text = (ROOT / "prototypes" / "publishing" / "fixture_harness.py").read_text(encoding="utf-8")
        self.assertIn("sek_test_", harness_text)

    def test_fixtures_decode_to_pixels(self):
        for kind in ("png", "jpeg"):
            media = facts(kind)
            self.assertEqual((media["width"], media["height"]), (120, 160))
            self.assertGreater(media["byteLength"], 100)

    def test_oracle_has_no_listener(self):
        for relative in (
            "prototypes/publishing/oracle.py",
            "prototypes/publishing/consumer_example.py",
            "prototypes/publishing/schema_validate.py",
            "prototypes/publishing/media.py",
        ):
            text = (ROOT / relative).read_text(encoding="utf-8")
            for banned in ("socket", "urlopen", "HTTPServer", "datetime.now", ".bind("):
                self.assertNotIn(banned, text, relative)

    def test_revision_bytes_ignore_later_mutation(self):
        world, credential = _owner_session()
        body = _comic_body()
        uploaded, _upload_harness = _send(world, "episode.upload", "mutate-upload-000001", body, credential)
        body["content"]["slices"][0]["bytesBase64"] = base64.b64encode(b"changed").decode("ascii")
        preview, _preview_harness = _send(
            world,
            "episode.preview",
            "mutate-preview-00001",
            {
                "seriesId": "ser_test_lane",
                "episodeId": "ep_test_lane_01",
                "revisionId": uploaded["result"]["acknowledgement"]["revisionId"],
            },
            credential,
        )
        self.assertEqual(preview["result"]["content"]["slices"][0]["bytesBase64"], facts("png")["bytesBase64"])

    def test_discover_orders_episodes_by_ordinal(self):
        world, credential = _owner_session()
        world["series"]["ser_test_lane"]["episodes"].reverse()
        discovered, _discover_harness = _send(world, "account.discover", "order-discover-00001", {}, credential)
        titles = [episode["title"] for episode in discovered["result"]["account"]["series"][0]["episodes"]]
        self.assertEqual(titles, ["North door", "South stair"])

    def test_pixel_cap_and_png_bomb_and_prose_bytes(self):
        world, credential = _owner_session()
        wide = _comic_body()
        wide["content"]["slices"][0]["width"] = 4096
        wide["content"]["slices"][0]["height"] = 1000
        wide.pop("expectedSha256")
        rejected, _pixel_harness = _send(world, "episode.upload", "pixel-cap-000000001", wide, credential)
        self.assertEqual(rejected["error"]["code"], "limit_exceeded")
        bomb = _comic_body()
        bomb["content"]["slices"][0] = _bomb_slice()
        bomb.pop("expectedSha256")
        bombed, _bomb_harness = _send(world, "episode.upload", "png-bomb-0000000001", bomb, credential)
        self.assertEqual(bombed["error"]["code"], "limit_exceeded")
        prose = _prose_body()
        prose["content"]["text"] = "雪" * 21846
        prose.pop("expectedSha256")
        oversized, _prose_harness = _send(world, "episode.upload", "prose-cap-00000001", prose, credential)
        self.assertEqual(oversized["error"]["code"], "limit_exceeded")

    def test_episode_byte_budget(self):
        world, credential = _owner_session()
        chunk = b"\x00" * LIMITS["maxSliceBytes"]
        encoded = base64.b64encode(chunk).decode("ascii")
        digest = hashlib.sha256(chunk).hexdigest()
        slices = []
        count = (LIMITS["maxEpisodeBytes"] // LIMITS["maxSliceBytes"]) + 1
        for ordinal in range(1, count + 1):
            slices.append(
                {
                    "ordinal": ordinal,
                    "mediaType": "image/png",
                    "width": 1,
                    "height": 1,
                    "byteLength": len(chunk),
                    "sha256": digest,
                    "bytesBase64": encoded,
                }
            )
        body = _comic_body()
        body["content"] = {"kind": "comic", "slices": slices}
        body.pop("expectedSha256")
        response, _budget_harness = _send(world, "episode.upload", "episode-budget-00001", body, credential)
        self.assertEqual(response["error"]["code"], "limit_exceeded")

    def test_bad_fixture_world_is_refused(self):
        loaded = json.loads((FIXTURES / "world.json").read_text(encoding="utf-8"))
        loaded["accounts"][0]["testOnly"] = False
        with self.assertRaises(ValueError):
            from prototypes.publishing.oracle import _check_world

            _check_world(loaded)

    def test_templates_use_the_protocol_constants(self):
        begin = json.loads((TEMPLATES / "begin-owner-a.json").read_text(encoding="utf-8"))
        self.assertEqual(begin["issuer"], PROTOCOL["issuer"]["exact"])
        self.assertEqual(begin["redirectUri"], PROTOCOL["exampleRedirect"])
        self.assertEqual(begin["clientId"], PROTOCOL["clientId"])


def _begin_body():
    template = json.loads((TEMPLATES / "begin-owner-a.json").read_text(encoding="utf-8"))
    return _interp(template, {"verifiers": VERIFIERS, "steps": {}, "media": {}})


def _exchange_body(harness):
    template = json.loads((TEMPLATES / "exchange-owner-a.json").read_text(encoding="utf-8"))
    from urllib.parse import parse_qsl, urlsplit

    query = dict(parse_qsl(urlsplit(harness.redirect_url).query))
    ctx = {"verifiers": VERIFIERS, "steps": {}, "redirects": {"begin": harness.redirect_url}, "media": {}}
    body = _interp(template, ctx)
    body["code"] = query["code"]
    body["state"] = query["state"]
    return body


def _comic_body():
    template = json.loads((TEMPLATES / "upload-comic.json").read_text(encoding="utf-8"))
    media = {"png": facts("png"), "jpeg": facts("jpeg")}
    body = _interp(template, {"verifiers": VERIFIERS, "steps": {}, "media": media})
    body["expectedSha256"] = media["png"]["sha256"]
    return body


def _prose_body():
    template = json.loads((TEMPLATES / "upload-prose.json").read_text(encoding="utf-8"))
    body = _interp(template, {"verifiers": VERIFIERS, "steps": {}, "media": {}})
    body["expectedSha256"] = hashlib.sha256(body["content"]["text"].encode("utf-8")).hexdigest()
    return body


def _owner_session():
    world = fresh_world()
    _begin, harness = _send(world, "auth.begin", "helper-begin-0000001", _begin_body(), login="acct_test_owner_a")
    opened, _opened = _send(world, "auth.exchange", "helper-exchange-0001", _exchange_body(harness))
    credential = {
        "credentialId": opened["result"]["credential"]["credentialId"],
        "secret": opened["result"]["credential"]["secret"],
    }
    return world, credential


def _bomb_slice():
    ihdr = struct.pack(">IIBBBBB", 1, 1, 8, 2, 0, 0, 0)
    inflated = b"\x00" * 100000
    data = b"\x89PNG\r\n\x1a\n" + _chunk(b"IHDR", ihdr) + _chunk(b"IDAT", zlib.compress(inflated)) + _chunk(b"IEND", b"")
    return {
        "ordinal": 1,
        "mediaType": "image/png",
        "width": 1,
        "height": 1,
        "byteLength": len(data),
        "sha256": hashlib.sha256(data).hexdigest(),
        "bytesBase64": base64.b64encode(data).decode("ascii"),
    }


def _chunk(tag, data):
    crc = zlib.crc32(tag + data) & 0xFFFFFFFF
    return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", crc)


def _envelope(command, key, body, credential=None):
    envelope = {
        "protocol": PROTOCOL["protocol"],
        "command": command,
        "idempotencyKey": key,
        "body": body,
    }
    if credential is not None:
        envelope["credential"] = credential
    return envelope


def _send(world, command, key, body, credential=None, login=None, clock=None, pause=False):
    harness = FixtureHarness(
        parse_clock(clock or PROTOCOL["defaultClock"]),
        pause=pause,
        login_account_id=login,
    )
    return apply(world, _envelope(command, key, body, credential), harness), harness


if __name__ == "__main__":
    unittest.main()
