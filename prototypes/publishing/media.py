from __future__ import annotations

import base64
import hashlib
from pathlib import Path

from prototypes.publishing.image_decode import decode_size

ROOT = Path(__file__).resolve().parent
FILES = {"png": "north.png", "jpeg": "north.jpg"}
MAX_PIXELS = 4_000_000


def facts(kind: str) -> dict:
    data = (ROOT / "fixtures" / "media" / FILES[kind]).read_bytes()
    decoded = decode_size(data, MAX_PIXELS)
    if not isinstance(decoded, tuple):
        raise RuntimeError(f"{kind} fixture did not decode")
    width, height = decoded
    media_type = "image/png" if kind == "png" else "image/jpeg"
    return {
        "mediaType": media_type,
        "width": width,
        "height": height,
        "byteLength": len(data),
        "sha256": hashlib.sha256(data).hexdigest(),
        "bytesBase64": base64.b64encode(data).decode("ascii"),
        "bytes": data,
    }
