"""Packaged-demo verification entrypoint.

One invocation validates an owner manifest and writes four evidence files.
This unit does not download archives, extract them, or claim browser proof.
"""

from __future__ import annotations

import json
import sys
from typing import Callable

import contract
import evidence
import lifecycle
import links
from contract import Manifest, ManifestError

EXIT_MALFORMED = 2
EXIT_PROVENANCE = 3
EXIT_HASH = 4
EXIT_OCCUPIED = 5
EXIT_BLOCKED = 6
EXIT_EXISTING_FILES = 7
EXIT_CLEANUP_UNPROVEN = 8

_STATUS_FOR = {
    "malformed": EXIT_MALFORMED,
    "missing-provenance": EXIT_PROVENANCE,
    "missing-artifact": EXIT_PROVENANCE,
    "unsafe-path": EXIT_PROVENANCE,
    "hash-mismatch": EXIT_HASH,
    "size-mismatch": EXIT_HASH,
}


def acceptance_block_reason(manifest: Manifest, platform: str) -> str | None:
    if manifest.synthetic:
        return "synthetic-manifest"
    if manifest.environment_kind != contract.ENVIRONMENT_KIND:
        return "environment-kind"
    if not manifest.sanctioned:
        return "not-sanctioned"
    if platform != "win32":
        return "platform-not-hosted-windows"
    return None


def execute_accepted(
    manifest: Manifest,
    *,
    spawn: Callable[..., lifecycle.OwnedProcess] = lifecycle.spawn_owned,
    fetch: Callable[[str], dict[str, object]] = links.fetch_landing,
    cleanup: Callable[[lifecycle.OwnedProcess], lifecycle.CleanupResult] = lifecycle.cleanup_owned,
    port_check: Callable[[str, int], bool] = lifecycle.port_occupied,
) -> dict[str, object]:
    """Lifecycle composer. `run` calls this only after the acceptance gate opens."""
    records = _blank("accepted-lifecycle")
    records["process-listener"]["ownerRoutes"] = list(manifest.route_paths)
    records["http"]["ownerRoutesFetched"] = False
    records["process-listener"]["portProbed"] = True
    if port_check(manifest.host, manifest.port):
        records["process-listener"].update(
            {
                "status": "refused",
                "code": "occupied-port",
                "spawnCalled": False,
                "occupantSignaled": False,
            }
        )
        records["http"]["reason"] = "occupied-port"
        records["browser-render"]["reason"] = "occupied-port"
        records["owned-process-cleanup"]["reason"] = "occupied-port-no-owned-process"
        return {"exitCode": EXIT_OCCUPIED, "label": "blocked-not-packaged-demo-proof", "records": records}
    records["process-listener"]["spawnCalled"] = True
    try:
        owned = spawn(manifest.argv, manifest.working_directory)
    except lifecycle.LifecycleError as exc:
        records["process-listener"].update({"status": "rejected", "code": exc.code, "detail": exc.detail})
        records["owned-process-cleanup"].update({"reason": exc.code, "signaled": False})
        return {"exitCode": EXIT_CLEANUP_UNPROVEN, "label": "blocked-not-packaged-demo-proof", "records": records}
    try:
        records["process-listener"].update(
            {
                "status": "spawned",
                "pid": owned.pid,
                "startToken": owned.identity.start_token,
            }
        )
        try:
            fetched = fetch(manifest.landing_url)
            records["http"] = {
                "status": fetched.get("status", "fetched"),
                "httpStatus": fetched.get("httpStatus"),
                "headers": fetched.get("headers", {}),
                "bodySha256": fetched.get("bodySha256"),
                "bodyLength": fetched.get("bodyLength"),
                "bodyTruncated": fetched.get("bodyTruncated", False),
                "followedRedirect": False,
                "discoveredLinks": fetched.get("discoveredLinks", []),
                "notBrowserRender": True,
                "ownerRoutes": list(manifest.route_paths),
                "ownerRoutesFetched": False,
                "reason": None,
            }
        except (OSError, links.LandingError) as exc:
            records["http"] = {
                "status": "error",
                "reason": exc.__class__.__name__,
                "discoveredLinks": [],
                "notBrowserRender": True,
            }
        records["browser-render"] = _browser("browser render is not collected by this unit")
    finally:
        try:
            result = cleanup(owned)
            records["owned-process-cleanup"] = result.as_evidence()
        finally:
            owned.close()
    exit_code = EXIT_BLOCKED
    if records["owned-process-cleanup"].get("signaled") and not records["owned-process-cleanup"].get("terminated"):
        exit_code = EXIT_CLEANUP_UNPROVEN
    return {"exitCode": exit_code, "label": "blocked-not-packaged-demo-proof", "records": records}


def run(manifest_path: str, evidence_dir: str) -> int:
    try:
        evidence.prepare_evidence_dir(evidence_dir)
    except evidence.ExistingTree as exc:
        _emit(EXIT_EXISTING_FILES, "existing-files", evidence_dir, exc.detail)
        return EXIT_EXISTING_FILES
    records = _blank("manifest")
    label = "rejected-input"
    exit_code = EXIT_MALFORMED
    try:
        document = contract.load_manifest_document(manifest_path)
        manifest = contract.validate_manifest(document)
        records["process-listener"]["manifestSynthetic"] = manifest.synthetic
        records["process-listener"]["ownerRoutes"] = list(manifest.route_paths)
        records["http"]["ownerRoutesFetched"] = False
        checked = contract.verify_archives(manifest)
        records["process-listener"]["archivesHashed"] = True
        records["process-listener"]["validatedBytes"] = [item.as_evidence() for item in checked]
        contract.inspect_working_directory(
            manifest.working_directory,
            (manifest.app.validated.path, manifest.service.validated.path),
        )
        reason = acceptance_block_reason(manifest, sys.platform)
        if reason is not None:
            label = "synthetic" if manifest.synthetic else "blocked-not-packaged-demo-proof"
            exit_code = EXIT_BLOCKED
            records["process-listener"].update(
                {
                    "status": "blocked",
                    "code": reason,
                    "spawnCalled": False,
                    "portProbed": False,
                }
            )
            records["http"]["reason"] = reason
            records["browser-render"]["reason"] = reason
            records["owned-process-cleanup"]["reason"] = reason
        else:
            finished = execute_accepted(manifest)
            label = str(finished["label"])
            exit_code = int(finished["exitCode"])
            records = finished["records"]  # type: ignore[assignment]
    except ManifestError as exc:
        exit_code = _STATUS_FOR.get(exc.code, EXIT_MALFORMED)
        label = "synthetic" if records["process-listener"].get("manifestSynthetic") else "rejected-input"
        records["process-listener"].update({"status": "rejected", "code": exc.code, "detail": exc.detail[:300]})
        if exc.checked:
            records["process-listener"]["archivesHashed"] = True
            records["process-listener"]["validatedBytes"] = [item.as_evidence() for item in exc.checked]
        records["http"]["reason"] = exc.code
        records["browser-render"]["reason"] = exc.code
        records["owned-process-cleanup"]["reason"] = exc.code
    evidence.write_evidence(evidence_dir, label=label, records=records)
    _emit(
        exit_code,
        records["process-listener"].get("code") or records["process-listener"].get("status"),
        evidence_dir,
        label,
    )
    return exit_code


def _blank(stage: str) -> dict[str, dict[str, object]]:
    return {
        "http": {
            "status": "not-attempted",
            "reason": None,
            "discoveredLinks": [],
            "notBrowserRender": True,
            "stage": stage,
        },
        "process-listener": {
            "status": "not-attempted",
            "code": None,
            "detail": None,
            "archivesHashed": False,
            "portProbed": False,
            "spawnCalled": False,
            "occupantSignaled": False,
            "stage": stage,
        },
        "browser-render": _browser(None),
        "owned-process-cleanup": {
            "status": "not-attempted",
            "reason": None,
            "signaled": False,
            "terminated": False,
            "pidsSignaled": [],
            "killedByPort": False,
            "killedByImageName": False,
            "stage": stage,
        },
    }


def _browser(reason: str | None) -> dict[str, object]:
    return {
        "status": "blocked",
        "outcome": "blocked",
        "captured": False,
        "engine": None,
        "capturePath": None,
        "reason": reason,
        "note": "HTTP responses are not browser-render evidence",
    }


def _emit(exit_code: int, status: object, evidence_dir: str, label: str) -> None:
    print(
        json.dumps(
            {
                "status": status,
                "exitCode": exit_code,
                "label": label,
                "packagedDemoProved": False,
                "evidenceDir": evidence_dir,
            }
        ),
        flush=True,
    )


def main(argv: list[str] | None = None) -> int:
    import argparse

    parser = argparse.ArgumentParser(prog="packaged-demo-harness")
    parser.add_argument("--manifest", required=True)
    parser.add_argument("--evidence-dir", required=True)
    args = parser.parse_args(argv)
    code = run(args.manifest, args.evidence_dir)
    if code == 0:
        return EXIT_BLOCKED
    return code


if __name__ == "__main__":
    sys.exit(main())
