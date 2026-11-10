"""Synthetic negative and lifecycle checks.

A passing run is not packaged-demo acceptance. The owner archive is not opened.
"""

from __future__ import annotations

import hashlib
import json
import os
import socket
import subprocess
import sys
import tempfile
import threading
import unittest
from dataclasses import replace
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from unittest import mock

import contract
import harness
import lifecycle
from contract import ManifestError

HERE = os.path.dirname(os.path.abspath(__file__))
FRAGMENT = os.path.join(HERE, "fixtures", "owner-expected-fragment.json")
OWNER_COMMIT = "91ba4894750d4e0d22a784c05b50ba89837451cf"
APP_COMMIT = "69d1f9b2a7e5839880d948e2e289125c76ae89cb"
TAR_SHA = "a3677623d821a294644006e1a377474e92b067a65a78b0501f4cbd7d271edec6"
ZIP_SHA = "b61d17fb307320a229475fdaeb970e9b0d9a72282f77200ace2d25d591882873"
OWNER_PATH = "/workspace/service-handoff-91ba489/scroll-studio-service-91ba489.tar.gz"
OWNER_ROUTES = (
    "/ss-publish/demo",
    "/ss-publish/demo/paper-harbor",
    "/ss-publish/demo/paper-harbor/the-fold",
    "/ss-publish/demo/paper-harbor/margin-note",
)
RUNTIME = ("harness.py", "contract.py", "lifecycle.py", "links.py", "evidence.py")


def _listen_ephemeral() -> socket.socket:
    while True:
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
        if port != 19427:
            sock.listen(1)
            sock.settimeout(0.3)
            return sock
        sock.close()


def _document(root: str, **overrides: object) -> dict[str, object]:
    app_bytes = overrides.pop("app_bytes", b"synthetic-app")
    service_bytes = overrides.pop("service_bytes", b"synthetic-service")
    app_path = os.path.join(root, "app.bin")
    service_path = os.path.join(root, "service.bin")
    work = overrides.pop("work", os.path.join(root, "work"))
    os.makedirs(work, exist_ok=True)
    with open(app_path, "wb") as handle:
        handle.write(app_bytes)  # type: ignore[arg-type]
    with open(service_path, "wb") as handle:
        handle.write(service_bytes)  # type: ignore[arg-type]
    port = int(overrides.pop("port", 19427))  # type: ignore[arg-type]
    argv = overrides.pop("argv", [sys.executable, "-c", "import time; time.sleep(30)"])
    document: dict[str, object] = {
        "contract": contract.CONTRACT,
        "synthetic": True,
        "provenance": {
            "app": {
                "commit": "a" * 40,
                "archive": {
                    "path": app_path,
                    "sha256": hashlib.sha256(app_bytes).hexdigest(),  # type: ignore[arg-type]
                },
            },
            "service": {
                "commit": "b" * 40,
                "archive": {
                    "path": service_path,
                    "sha256": hashlib.sha256(service_bytes).hexdigest(),  # type: ignore[arg-type]
                },
            },
        },
        "lifecycle": {
            "workingDirectory": work,
            "argv": argv,
            "loopback": {"host": "127.0.0.1", "port": port},
        },
        "routes": {
            "landingUrl": f"http://127.0.0.1:{port}/",
            "discovery": contract.DISCOVERY,
            "paths": ["/synthetic/kept"],
        },
        "environment": {"kind": contract.ENVIRONMENT_KIND, "sanctioned": False},
    }
    for key, value in overrides.items():
        document[key] = value
    return document


def _write(root: str, document: dict[str, object]) -> str:
    path = os.path.join(root, "manifest.json")
    with open(path, "w", encoding="utf-8") as handle:
        json.dump(document, handle)
    return path


class FragmentTests(unittest.TestCase):
    def test_fragment_matches_owner_supplied_expected_values(self) -> None:
        with open(FRAGMENT, encoding="utf-8") as handle:
            body = json.load(handle)
        self.assertTrue(body["notAManifest"])
        self.assertTrue(body["notMeasuredProof"])
        self.assertEqual(body["app"]["commit"], APP_COMMIT)
        self.assertEqual(len(APP_COMMIT), 40)
        self.assertIsNone(body["app"]["artifact"]["sha256"])
        self.assertEqual(body["app"]["artifact"]["sha256Status"], "UNKNOWN")
        self.assertIsNone(body["app"]["artifact"]["localPath"])
        self.assertEqual(body["service"]["commit"], OWNER_COMMIT)
        self.assertEqual(len(OWNER_COMMIT), 40)
        artifacts = {item["kind"]: item for item in body["service"]["artifacts"]}
        self.assertEqual(artifacts["coordinator-tar"]["path"], OWNER_PATH)
        self.assertEqual(artifacts["coordinator-tar"]["sha256"], TAR_SHA)
        self.assertEqual(artifacts["hosted-zip"]["claimedSha256"], ZIP_SHA)
        self.assertFalse(artifacts["hosted-zip"]["claimedSha256MeasuredByThisUnit"])
        self.assertIsNone(artifacts["hosted-zip"]["localPath"])
        self.assertNotEqual(TAR_SHA, ZIP_SHA)
        self.assertEqual(len(TAR_SHA), 64)
        self.assertEqual(len(ZIP_SHA), 64)
        self.assertEqual(body["service"]["argv"], ["python", "-m", "prototypes.publishing.listener"])
        self.assertEqual(body["service"]["landingUrl"], "http://127.0.0.1:19427/")
        self.assertEqual(tuple(body["service"]["ownerRoutes"]), OWNER_ROUTES)
        self.assertFalse(body["ownerReportNotValidation"]["claimedByThisUnit"])
        self.assertIn("EXE launch", body["ownerReportNotValidation"]["notReported"])
        self.assertIn("app.archive.sha256", body["pending"])
        self.assertFalse(os.path.exists(OWNER_PATH))

    def test_fragment_is_rejected_without_hashing_or_spawning(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            evidence = os.path.join(root, "evidence")
            with mock.patch("contract.hash_file", side_effect=AssertionError("hashed")):
                code = harness.run(FRAGMENT, evidence)
            self.assertEqual(code, harness.EXIT_MALFORMED)
            process = json.loads((open(os.path.join(evidence, "process-listener.json"), encoding="utf-8").read()))
            http = json.loads(open(os.path.join(evidence, "http.json"), encoding="utf-8").read())
            browser = json.loads(open(os.path.join(evidence, "browser-render.json"), encoding="utf-8").read())
            cleanup = json.loads(open(os.path.join(evidence, "owned-process-cleanup.json"), encoding="utf-8").read())
            self.assertEqual(process["evidenceKind"], "process-listener")
            self.assertEqual(http["evidenceKind"], "http")
            self.assertEqual(browser["evidenceKind"], "browser-render")
            self.assertEqual(cleanup["evidenceKind"], "owned-process-cleanup")
            for payload in (process, http, browser, cleanup):
                self.assertFalse(payload["packagedDemoProved"])
            self.assertEqual(process["body"]["code"], "malformed")
            self.assertFalse(process["body"]["archivesHashed"])
            self.assertFalse(process["body"]["spawnCalled"])
            self.assertFalse(process["body"]["portProbed"])
            self.assertEqual(http["body"]["status"], "not-attempted")
            self.assertFalse(browser["body"]["captured"])
            self.assertFalse(cleanup["body"]["signaled"])
            self.assertNotIn("discoveredLinks", cleanup["body"])
            self.assertNotIn("pidsSignaled", http["body"])

    def test_runtime_modules_do_not_embed_owner_pins_or_shell_helpers(self) -> None:
        for name in RUNTIME:
            text = open(os.path.join(HERE, name), encoding="utf-8").read()
            self.assertNotIn(OWNER_COMMIT, text)
            self.assertNotIn(APP_COMMIT, text)
            self.assertNotIn(TAR_SHA, text)
            self.assertNotIn(ZIP_SHA, text)
            self.assertNotIn(OWNER_PATH, text)
            self.assertNotIn("42746459364", text)
            self.assertNotIn("shell=True", text)
            self.assertNotIn("os.system", text)
            self.assertNotIn("urlopen", text)
            self.assertNotIn("urlretrieve", text)
            self.assertNotIn("tarfile", text)
            self.assertNotIn("taskkill", text)
            self.assertNotIn("Stop-Process", text)
            self.assertNotIn("windows_browser_publish", text)
            self.assertNotIn("test_listener", text)


class ManifestTests(unittest.TestCase):
    def test_malformed_shapes(self) -> None:
        samples: list[object] = [{}, [], "nope", None, {"contract": "other"}]
        for sample in samples:
            with self.assertRaises(ManifestError) as caught:
                contract.validate_manifest(sample)
            self.assertEqual(caught.exception.code, "malformed")

    def test_duplicate_key_is_malformed(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            path = os.path.join(root, "dup.json")
            with open(path, "w", encoding="utf-8") as handle:
                handle.write('{"contract":"a","contract":"b"}')
            with self.assertRaises(ManifestError) as caught:
                contract.load_manifest_document(path)
            self.assertEqual(caught.exception.code, "malformed")

    def test_abbreviated_and_uppercase_commits_are_not_exact(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            document = _document(root)
            document["provenance"]["service"]["commit"] = "91ba489"  # type: ignore[index]
            with self.assertRaises(ManifestError) as caught:
                contract.validate_manifest(document)
            self.assertEqual(caught.exception.code, "missing-provenance")
            document = _document(root)
            document["provenance"]["app"]["commit"] = "A" * 40  # type: ignore[index]
            with self.assertRaises(ManifestError) as caught:
                contract.validate_manifest(document)
            self.assertEqual(caught.exception.code, "missing-provenance")

    def test_incomplete_app_digest_stays_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            document = _document(root)
            document["provenance"]["app"]["commit"] = APP_COMMIT  # type: ignore[index]
            document["provenance"]["service"]["commit"] = OWNER_COMMIT  # type: ignore[index]
            document["provenance"]["service"]["archive"]["sha256"] = ZIP_SHA  # type: ignore[index]
            document["routes"]["paths"] = list(OWNER_ROUTES)  # type: ignore[index]
            document["provenance"]["app"]["archive"]["sha256"] = "UNKNOWN"  # type: ignore[index]
            with mock.patch("contract.hash_file", side_effect=AssertionError("hashed")):
                with self.assertRaises(ManifestError) as caught:
                    contract.validate_manifest(document)
            self.assertEqual(caught.exception.code, "missing-provenance")
            del document["provenance"]["app"]["archive"]["sha256"]  # type: ignore[attr-defined]
            with self.assertRaises(ManifestError) as caught:
                contract.validate_manifest(document)
            self.assertEqual(caught.exception.code, "missing-provenance")

    def test_tar_and_zip_digests_are_not_substituted(self) -> None:
        self.assertNotEqual(TAR_SHA, ZIP_SHA)
        with tempfile.TemporaryDirectory() as root:
            document = _document(root, service_bytes=b"synthetic-bytes-not-either-owner-archive")
            for declared in (ZIP_SHA, TAR_SHA):
                document["provenance"]["service"]["archive"]["sha256"] = declared  # type: ignore[index]
                manifest = contract.validate_manifest(document)
                with self.assertRaises(ManifestError) as caught:
                    contract.verify_archives(manifest)
                self.assertEqual(caught.exception.code, "hash-mismatch")
                self.assertIn(declared, caught.exception.detail)
                other = TAR_SHA if declared == ZIP_SHA else ZIP_SHA
                self.assertNotIn(other, caught.exception.detail)

    def test_missing_owner_routes(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            document = _document(root)
            del document["routes"]["paths"]  # type: ignore[attr-defined]
            with self.assertRaises(ManifestError) as caught:
                contract.validate_manifest(document)
            self.assertEqual(caught.exception.code, "missing-provenance")
            document = _document(root)
            document["routes"]["paths"] = ["../ss-publish/demo", "http://127.0.0.1:19427/ss-publish/demo"]  # type: ignore[index]
            with self.assertRaises(ManifestError) as caught:
                contract.validate_manifest(document)
            self.assertEqual(caught.exception.code, "malformed")

    def test_missing_app_provenance(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            document = _document(root)
            del document["provenance"]["app"]  # type: ignore[attr-defined]
            with self.assertRaises(ManifestError) as caught:
                contract.validate_manifest(document)
            self.assertEqual(caught.exception.code, "missing-provenance")

    def test_shell_and_non_loopback_are_malformed(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            cases = []
            string_argv = _document(root)
            string_argv["lifecycle"]["argv"] = "python -m prototypes.publishing.listener"  # type: ignore[index]
            cases.append(string_argv)
            shell_argv = _document(root)
            shell_argv["lifecycle"]["argv"] = ["bash", "-c", "python -m prototypes.publishing.listener"]  # type: ignore[index]
            cases.append(shell_argv)
            cmd_argv = _document(root)
            cmd_argv["lifecycle"]["argv"] = ["cmd.exe", "/c", "python"]  # type: ignore[index]
            cases.append(cmd_argv)
            public_host = _document(root)
            public_host["lifecycle"]["loopback"]["host"] = "0.0.0.0"  # type: ignore[index]
            cases.append(public_host)
            localhost = _document(root)
            localhost["lifecycle"]["loopback"]["host"] = "localhost"  # type: ignore[index]
            cases.append(localhost)
            external = _document(root)
            external["routes"]["landingUrl"] = "http://example.com/"  # type: ignore[index]
            cases.append(external)
            deep = _document(root)
            deep["routes"]["landingUrl"] = "http://127.0.0.1:19427/ss-publish/browser/operator"  # type: ignore[index]
            cases.append(deep)
            query = _document(root)
            query["routes"]["landingUrl"] = "http://127.0.0.1:19427/?secret=sek_live_x"  # type: ignore[index]
            cases.append(query)
            port_text = _document(root)
            port_text["lifecycle"]["loopback"]["port"] = "19427"  # type: ignore[index]
            cases.append(port_text)
            relative = _document(root)
            relative["provenance"]["service"]["archive"]["path"] = "service.bin"  # type: ignore[index]
            cases.append(relative)
            for document in cases:
                with self.assertRaises(ManifestError) as caught:
                    contract.validate_manifest(document)
                self.assertEqual(caught.exception.code, "malformed", document)

    def test_synthetic_cannot_be_sanctioned(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            document = _document(root)
            document["environment"]["sanctioned"] = True  # type: ignore[index]
            with self.assertRaises(ManifestError) as caught:
                contract.validate_manifest(document)
            self.assertEqual(caught.exception.code, "malformed")

    def test_hash_mismatch_preserves_bytes(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            payload = b"synthetic-archive-bytes"
            document = _document(root, service_bytes=payload)
            archive = document["provenance"]["service"]["archive"]["path"]  # type: ignore[index]
            before = open(archive, "rb").read()
            stamp = os.stat(archive).st_mtime_ns
            document["provenance"]["service"]["archive"]["sha256"] = "f" * 64  # type: ignore[index]
            manifest = contract.validate_manifest(document)
            with self.assertRaises(ManifestError) as caught:
                contract.verify_archives(manifest)
            self.assertEqual(caught.exception.code, "hash-mismatch")
            self.assertEqual(open(archive, "rb").read(), before)
            self.assertEqual(os.stat(archive).st_mtime_ns, stamp)

    def test_symlink_archive_is_not_followed(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            document = _document(root, service_bytes=b"target-bytes")
            target = document["provenance"]["service"]["archive"]["path"]  # type: ignore[index]
            link = os.path.join(root, "linked.bin")
            os.symlink(target, link)
            document["provenance"]["service"]["archive"]["path"] = link  # type: ignore[index]
            manifest = contract.validate_manifest(document)
            with self.assertRaises(ManifestError) as caught:
                contract.verify_archives(manifest)
            self.assertEqual(caught.exception.code, "unsafe-path")

    def test_missing_archive(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            document = _document(root)
            missing = os.path.join(root, "missing.bin")
            document["provenance"]["app"]["archive"]["path"] = missing  # type: ignore[index]
            manifest = contract.validate_manifest(document)
            with self.assertRaises(ManifestError) as caught:
                contract.verify_archives(manifest)
            self.assertEqual(caught.exception.code, "missing-artifact")


class GateTests(unittest.TestCase):
    def test_linux_blocks_before_spawn_or_port_probe(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            sentinel = os.path.join(root, "launched")
            work = os.path.join(root, "work")
            os.makedirs(work)
            marker = os.path.join(work, "keep.txt")
            with open(marker, "wb") as handle:
                handle.write(b"preserve")
            stamp = os.stat(marker).st_mtime_ns
            listener = _listen_ephemeral()
            try:
                port = listener.getsockname()[1]
                code = (
                    "open(r'''"
                    + sentinel
                    + "''','w',encoding='utf-8').write('launched')"
                )
                document = _document(
                    root,
                    port=port,
                    argv=[sys.executable, "-c", code],
                    work=work,
                )
                document["synthetic"] = False
                document["environment"]["sanctioned"] = True  # type: ignore[index]
                evidence = os.path.join(root, "evidence")
                with mock.patch("harness.execute_accepted", side_effect=AssertionError("launched")):
                    status = harness.run(_write(root, document), evidence)
                self.assertEqual(status, harness.EXIT_BLOCKED)
                self.assertFalse(os.path.exists(sentinel))
                self.assertEqual(open(marker, "rb").read(), b"preserve")
                self.assertEqual(os.stat(marker).st_mtime_ns, stamp)
                with self.assertRaises(TimeoutError):
                    listener.accept()
                process = json.loads(open(os.path.join(evidence, "process-listener.json"), encoding="utf-8").read())
                self.assertEqual(process["body"]["code"], "platform-not-hosted-windows")
                self.assertFalse(process["body"]["spawnCalled"])
                self.assertFalse(process["body"]["portProbed"])
                self.assertTrue(process["body"]["archivesHashed"])
                self.assertEqual(process["label"], "blocked-not-packaged-demo-proof")
                self.assertFalse(process["packagedDemoProved"])
            finally:
                listener.close()

    def test_reasons(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            synthetic = contract.validate_manifest(_document(root))
            self.assertEqual(harness.acceptance_block_reason(synthetic, "win32"), "synthetic-manifest")
            document = _document(root)
            document["synthetic"] = False
            unsanctioned = contract.validate_manifest(document)
            self.assertEqual(harness.acceptance_block_reason(unsanctioned, "win32"), "not-sanctioned")
            document["environment"]["sanctioned"] = True  # type: ignore[index]
            sanctioned = contract.validate_manifest(document)
            self.assertEqual(
                harness.acceptance_block_reason(sanctioned, "linux"),
                "platform-not-hosted-windows",
            )
            self.assertIsNone(harness.acceptance_block_reason(sanctioned, "win32"))

    def test_existing_evidence_is_preserved(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            evidence = os.path.join(root, "evidence")
            os.makedirs(evidence)
            keep = os.path.join(evidence, "keep.txt")
            with open(keep, "w", encoding="utf-8") as handle:
                handle.write("keep")
            code = harness.run(FRAGMENT, evidence)
            self.assertEqual(code, harness.EXIT_EXISTING_FILES)
            self.assertEqual(open(keep, encoding="utf-8").read(), "keep")
            self.assertFalse(os.path.exists(os.path.join(evidence, "http.json")))


class LifecycleTests(unittest.TestCase):
    def test_occupied_port_preserves_occupant(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            listener = _listen_ephemeral()
            port = listener.getsockname()[1]
            ready = os.path.join(root, "ready")
            code = (
                "import socket,sys,time\n"
                "s=socket.socket()\n"
                "s.bind(('127.0.0.1', int(sys.argv[1])))\n"
                "s.listen(1)\n"
                "open(sys.argv[2],'w',encoding='utf-8').write('up')\n"
                "s.settimeout(20)\n"
                "try:\n"
                "    c,_=s.accept()\n"
                "    c.close()\n"
                "except OSError:\n"
                "    pass\n"
                "time.sleep(30)\n"
            )
            listener.close()
            owned = lifecycle.spawn_owned([sys.executable, "-c", code, str(port), ready], root)
            try:
                for _ in range(50):
                    if os.path.exists(ready):
                        break
                    if owned.process.poll() is not None:
                        self.fail("occupant exited before listen")
                    lifecycle._sleep_briefly()
                self.assertTrue(os.path.exists(ready))
                self.assertTrue(lifecycle.port_occupied("127.0.0.1", port))
                self.assertIsNone(owned.process.poll())
                self.assertNotEqual(port, 19427)
            finally:
                result = lifecycle.cleanup_owned(owned)
                owned.close()
            self.assertTrue(result.terminated)
            self.assertTrue(result.signaled)
            self.assertFalse(os.path.exists(f"/proc/{owned.pid}"))

    def test_free_port_is_not_occupied(self) -> None:
        sock = _listen_ephemeral()
        port = sock.getsockname()[1]
        sock.close()
        self.assertFalse(lifecycle.port_occupied("127.0.0.1", port))

    def test_non_loopback_probe_is_refused(self) -> None:
        with self.assertRaises(lifecycle.LifecycleError) as caught:
            lifecycle.port_occupied("0.0.0.0", 9)
        self.assertEqual(caught.exception.code, "non-loopback")

    def test_owned_pid_cleanup_and_foreign_pid_preserved(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            marker = os.path.join(root, "keep.txt")
            with open(marker, "wb") as handle:
                handle.write(b"keep")
            stamp = os.stat(marker).st_mtime_ns
            owned = lifecycle.spawn_owned([sys.executable, "-c", "import time; time.sleep(30)"], root)
            try:
                tampered = replace(owned, identity=replace(owned.identity, start_token="1"))
                refused = lifecycle.cleanup_owned(tampered)
                self.assertFalse(refused.signaled)
                self.assertIsNone(owned.process.poll())
                detached = replace(owned, pidfd=None)
                no_handle = lifecycle.cleanup_owned(detached)
                self.assertFalse(no_handle.signaled)
                self.assertEqual(no_handle.reason, "no-durable-handle")
                self.assertIsNone(owned.process.poll())
                done = lifecycle.cleanup_owned(owned)
                self.assertTrue(done.signaled)
                self.assertTrue(done.terminated)
                self.assertFalse(done.as_evidence()["killedByPort"])
            finally:
                if owned.process.poll() is None:
                    lifecycle.cleanup_owned(owned)
                owned.close()
            self.assertEqual(open(marker, "rb").read(), b"keep")
            self.assertEqual(os.stat(marker).st_mtime_ns, stamp)
        foreign = lifecycle.OwnedProcess(
            pid=os.getpid(),
            launcher_pid=-1,
            identity=lifecycle.ProcessIdentity(os.getpid(), "not-the-real-token", -1, ("not-the-harness",)),
            argv=("not-the-harness",),
            pidfd=None,
            process=None,  # type: ignore[arg-type]
        )
        preserved = lifecycle.cleanup_owned(foreign)
        self.assertFalse(preserved.signaled)
        self.assertTrue(os.path.exists(f"/proc/{os.getpid()}"))

    def test_already_exited_child_is_not_signaled(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            owned = lifecycle.spawn_owned([sys.executable, "-c", "import sys; sys.exit(0)"], root)
            try:
                owned.process.wait(timeout=2)
                result = lifecycle.cleanup_owned(owned)
                self.assertFalse(result.signaled)
                self.assertEqual(result.reason, "already-exited-or-unreadable")
            finally:
                owned.close()

    def test_shell_argv_is_not_spawned(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            with self.assertRaises(lifecycle.LifecycleError):
                lifecycle.spawn_owned(["bash", "-c", "sleep 30"], root)


class HttpAndOrchestrationTests(unittest.TestCase):
    def test_link_filter_is_same_origin_and_redacts_sensitive_query(self) -> None:
        landing = "http://127.0.0.1:19427/"
        body = """
        <a href="/synthetic/kept">k</a>
        <a href="http://127.0.0.1:19427/synthetic/also">a</a>
        <a href="http://example.com/no">n</a>
        <a href="http://127.0.0.1:9/nope">p</a>
        <a href="javascript:alert(1)">j</a>
        <a href="http://127.0.0.1:19427/synthetic/secret?secret=sek_live_marker">s</a>
        """.encode()
        links = __import__("links").discover_links(body, "text/html", landing)
        self.assertEqual(
            links,
            [
                "http://127.0.0.1:19427/synthetic/kept",
                "http://127.0.0.1:19427/synthetic/also",
                "http://127.0.0.1:19427/synthetic/secret",
            ],
        )
        self.assertNotIn("sek_live_marker", "\n".join(links))

    def test_local_http_is_labeled_and_does_not_follow_links(self) -> None:
        hits = {"count": 0}

        class Handler(BaseHTTPRequestHandler):
            def do_GET(self) -> None:  # noqa: N802
                hits["count"] += 1
                body = b'<html><a href="/synthetic/kept">k</a><a href="http://example.com/no">n</a></html>'
                self.send_response(200)
                self.send_header("Content-Type", "text/html; charset=utf-8")
                self.send_header("Content-Length", str(len(body)))
                self.send_header("Referrer-Policy", "same-origin")
                self.send_header("Set-Cookie", "secret=sek_live_should_not_be_stored")
                self.end_headers()
                self.wfile.write(body)

            def log_message(self, fmt: str, *args: object) -> None:
                return

        server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.assertNotEqual(server.server_address[1], 19427)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            import links as linkmod

            observed = linkmod.fetch_landing(f"http://127.0.0.1:{server.server_address[1]}/")
        finally:
            server.shutdown()
            server.server_close()
            thread.join(timeout=2)
        self.assertEqual(hits["count"], 1)
        self.assertEqual(observed["httpStatus"], 200)
        self.assertEqual(
            observed["discoveredLinks"],
            [f"http://127.0.0.1:{server.server_address[1]}/synthetic/kept"],
        )
        self.assertNotIn("sek_live_should_not_be_stored", json.dumps(observed))
        self.assertTrue(observed["notBrowserRender"])
        self.assertEqual(observed["headers"]["referrer-policy"], "same-origin")

    def test_occupied_orchestration_does_not_spawn(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            manifest = contract.validate_manifest(_document(root, port=9))
            calls: list[str] = []

            def spawn(*_args: object, **_kwargs: object) -> lifecycle.OwnedProcess:
                calls.append("spawn")
                raise AssertionError("spawned")

            result = harness.execute_accepted(
                manifest,
                spawn=spawn,
                port_check=lambda _host, _port: True,
                fetch=lambda _url: {"status": "fetched"},
            )
            self.assertEqual(calls, [])
            self.assertEqual(result["exitCode"], harness.EXIT_OCCUPIED)
            records = result["records"]
            self.assertFalse(records["process-listener"]["spawnCalled"])  # type: ignore[index]
            self.assertFalse(records["process-listener"]["occupantSignaled"])  # type: ignore[index]
            self.assertFalse(records["browser-render"]["captured"])  # type: ignore[index]

    def test_composer_cleans_synthetic_child_and_blocks_browser(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            manifest = contract.validate_manifest(
                _document(root, argv=[sys.executable, "-c", "import time; time.sleep(30)"], port=9)
            )
            result = harness.execute_accepted(
                manifest,
                port_check=lambda _host, _port: False,
                fetch=lambda _url: {
                    "status": "fetched",
                    "httpStatus": 200,
                    "headers": {},
                    "bodySha256": "ab" * 32,
                    "bodyLength": 0,
                    "bodyTruncated": False,
                    "discoveredLinks": ["http://127.0.0.1:9/synthetic/kept"],
                },
            )
            records = result["records"]
            self.assertEqual(result["exitCode"], harness.EXIT_BLOCKED)
            self.assertTrue(records["owned-process-cleanup"]["terminated"])  # type: ignore[index]
            self.assertFalse(records["browser-render"]["captured"])  # type: ignore[index]
            self.assertTrue(records["browser-render"]["note"])  # type: ignore[index]
            pid = records["process-listener"]["pid"]  # type: ignore[index]
            self.assertFalse(os.path.exists(f"/proc/{pid}"))
            self.assertNotIn("discoveredLinks", records["owned-process-cleanup"])  # type: ignore[arg-type]


class CommandTests(unittest.TestCase):
    def test_cli_rejects_fragment(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            evidence = os.path.join(root, "evidence")
            completed = subprocess.run(
                [
                    sys.executable,
                    os.path.join(HERE, "harness.py"),
                    "--manifest",
                    FRAGMENT,
                    "--evidence-dir",
                    evidence,
                ],
                shell=False,
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(completed.returncode, harness.EXIT_MALFORMED)
            payload = json.loads(completed.stdout)
            self.assertFalse(payload["packagedDemoProved"])
            self.assertEqual(payload["exitCode"], harness.EXIT_MALFORMED)
            self.assertNotIn(TAR_SHA, completed.stdout)
            self.assertNotIn(ZIP_SHA, completed.stdout)


def _git_head() -> str | None:
    path = HERE
    while True:
        git = os.path.join(path, ".git")
        if os.path.isdir(git):
            head = open(os.path.join(git, "HEAD"), encoding="utf-8").read().strip()
            if head.startswith("ref: "):
                ref = head.split(" ", 1)[1]
                return open(os.path.join(git, ref), encoding="utf-8").read().strip()
            return head
        parent = os.path.dirname(path)
        if parent == path:
            return None
        path = parent


def record_evidence(dest: str, tests_run: int, failed: int, events: list[dict[str, str]]) -> None:
    import evidence as evidence_mod

    evidence_mod.prepare_evidence_dir(dest)
    fragment_dir = os.path.join(dest, "fragment-invocation")
    os.makedirs(fragment_dir)
    completed = subprocess.run(
        [
            sys.executable,
            os.path.join(HERE, "harness.py"),
            "--manifest",
            FRAGMENT,
            "--evidence-dir",
            fragment_dir,
        ],
        shell=False,
        capture_output=True,
        text=True,
        check=False,
    )
    with tempfile.TemporaryDirectory() as root:
        payload = b"recorded-synthetic-service"
        document = _document(root, service_bytes=payload)
        archive = document["provenance"]["service"]["archive"]["path"]  # type: ignore[index]
        before = open(archive, "rb").read()
        document["provenance"]["service"]["archive"]["sha256"] = "f" * 64  # type: ignore[index]
        manifest = contract.validate_manifest(document)
        mismatch = None
        try:
            contract.verify_archives(manifest)
        except ManifestError as exc:
            mismatch = exc
        preserved = open(archive, "rb").read() == before
        listener = _listen_ephemeral()
        occupied_port = listener.getsockname()[1]
        occupied = lifecycle.port_occupied("127.0.0.1", occupied_port)
        try:
            client = listener.accept()
        except TimeoutError:
            client = None
        else:
            client[0].close()
        listener.close()
        owned = lifecycle.spawn_owned([sys.executable, "-c", "import time; time.sleep(30)"], root)
        tampered = lifecycle.cleanup_owned(replace(owned, identity=replace(owned.identity, start_token="1")))
        alive_after_tamper = owned.process.poll() is None
        cleaned = lifecycle.cleanup_owned(owned)
        owned.close()
        import links as linkmod

        parsed = linkmod.discover_links(
            b'<a href="/synthetic/kept"></a><a href="http://example.com/no"></a>',
            "text/html",
            "http://127.0.0.1:19427/",
        )
    records = {
        "http": {
            "status": "synthetic",
            "source": "local-fixture-not-the-packaged-listener",
            "notBrowserRender": True,
            "discoveredLinks": parsed,
            "droppedExternal": "http://example.com/no",
            "liveServiceLandingFetched": False,
        },
        "process-listener": {
            "status": "synthetic",
            "fragmentExit": completed.returncode,
            "fragmentHashedArchive": False,
            "hashMismatchCode": None if mismatch is None else mismatch.code,
            "hashMismatchFilePreserved": preserved,
            "occupiedPort": occupied_port,
            "occupiedRefused": occupied,
            "occupantAcceptedProbe": client is not None,
            "port19427Used": occupied_port == 19427,
            "spawnOfOwnerArgv": False,
        },
        "browser-render": {
            "status": "blocked",
            "outcome": "blocked",
            "captured": False,
            "engine": None,
            "reason": "actual browser render was not run; synthetic HTTP is not browser evidence",
        },
        "owned-process-cleanup": {
            "status": "synthetic",
            "tamperedIdentitySignaled": tampered.signaled,
            "ownedPid": cleaned.pid,
            "ownedSignaled": cleaned.signaled,
            "ownedTerminated": cleaned.terminated,
            "aliveAfterTamper": alive_after_tamper,
            "killedByPort": False,
            "killedByImageName": False,
        },
    }
    evidence_mod.write_evidence(dest, label="synthetic", records=records)
    index = {
        "label": "synthetic",
        "packagedDemoProved": False,
        "packagedDemoAcceptance": "BLOCKED",
        "unitPassIsNotPackagedDemoProof": True,
        "unitTestsFailed": failed,
        "unitTestsRun": tests_run,
        "unitResults": events,
        "requiredBase": "f5c2bfee403e6194eb2c7e3ab6f1021cd5bd2df3",
        "headAtRecordTime": _git_head(),
        "platform": sys.platform,
        "python": sys.version,
        "ownerArchiveVisibleOnThisVm": os.path.exists(OWNER_PATH),
        "fragmentInvocationExit": completed.returncode,
        "fragmentInvocationStdout": completed.stdout.strip(),
        "proofCommand": "python verification/packaged-demo/test_harness.py --evidence-dir verification/packaged-demo/evidence/synthetic",
        "harnessInvocation": "python verification/packaged-demo/harness.py --manifest MANIFEST.json --evidence-dir EVIDENCE_DIR",
        "blockedChecks": [
            "hosted-windows launch of the owner service archive",
            "loopback listener evidence for the owner process on 127.0.0.1:19427",
            "natural link discovery against the owner landing page",
            "browser-render capture",
            "owned-process cleanup of the owner listener",
            "verification of the owner-supplied app commit against a local archive",
            "measurement of either service digest; tar and hosted ZIP hashes are not interchangeable",
            "app archive sha256, which the owner marked UNKNOWN",
            "sanctioned local paths for the app ZIP and the service hosted ZIP",
            "the owner Windows run, including its reported browser navigation; this unit did not run it and saw no EXE launch or screenshots",
        ],
    }
    with open(os.path.join(dest, "index.json"), "w", encoding="utf-8") as handle:
        json.dump(index, handle, indent=2)
        handle.write("\n")


class Collect(unittest.TextTestResult):
    def __init__(self, stream: object, descriptions: bool, verbosity: int) -> None:
        super().__init__(stream, descriptions, verbosity)
        self.events: list[dict[str, str]] = []

    def addSuccess(self, test: unittest.TestCase) -> None:
        super().addSuccess(test)
        self.events.append({"name": test.id(), "status": "pass"})

    def addFailure(self, test: unittest.TestCase, err: object) -> None:
        super().addFailure(test, err)  # type: ignore[arg-type]
        self.events.append({"name": test.id(), "status": "fail"})

    def addError(self, test: unittest.TestCase, err: object) -> None:
        super().addError(test, err)  # type: ignore[arg-type]
        self.events.append({"name": test.id(), "status": "error"})


def main(argv: list[str] | None = None) -> int:
    import argparse

    parser = argparse.ArgumentParser(prog="packaged-demo-tests")
    parser.add_argument("--evidence-dir")
    args = parser.parse_args(argv)
    suite = unittest.defaultTestLoader.loadTestsFromModule(sys.modules[__name__])
    result = unittest.TextTestRunner(verbosity=2, resultclass=Collect).run(suite)
    if args.evidence_dir and result.wasSuccessful():
        record_evidence(args.evidence_dir, result.testsRun, len(result.failures) + len(result.errors), result.events)  # type: ignore[attr-defined]
    return 0 if result.wasSuccessful() else 1


if __name__ == "__main__":
    sys.exit(main())
