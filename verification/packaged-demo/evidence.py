"""Four separate evidence files. This writer cannot mark packaged-demo proof."""

from __future__ import annotations

import json
import os
from datetime import datetime, timezone

FILES = {
    "http": "http.json",
    "process-listener": "process-listener.json",
    "browser-render": "browser-render.json",
    "owned-process-cleanup": "owned-process-cleanup.json",
}


class ExistingTree(Exception):
    def __init__(self, detail: str) -> None:
        super().__init__(detail)
        self.detail = detail


def prepare_evidence_dir(path: str) -> None:
    if os.path.islink(path):
        raise ExistingTree("evidence path is a symlink")
    if os.path.isdir(path):
        with os.scandir(path) as entries:
            if any(entries):
                raise ExistingTree("evidence directory is not empty")
        return
    if os.path.exists(path):
        raise ExistingTree("evidence path exists and is not a directory")
    os.makedirs(path)


def write_evidence(path: str, *, label: str, records: dict[str, dict[str, object]]) -> None:
    if set(records) != set(FILES):
        raise RuntimeError("evidence records must be exactly the four kinds")
    stamp = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    for kind, filename in FILES.items():
        payload = {
            "evidenceKind": kind,
            "label": label,
            "packagedDemoProved": False,
            "recordedAt": stamp,
            "body": records[kind],
        }
        target = os.path.join(path, filename)
        data = (json.dumps(payload, indent=2) + "\n").encode("utf-8")
        fd = os.open(target, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o644)
        try:
            os.write(fd, data)
        finally:
            os.close(fd)
