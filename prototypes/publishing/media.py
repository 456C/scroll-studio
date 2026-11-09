from __future__ import annotations

import base64
import hashlib
import struct
import zlib

JPEG_1X1 = bytes.fromhex(
    "ffd8ffe000104a46494600010100000100010000ffdb00430008060607060508070707090908"
    "0a0c140d0c0b0b0c1912130f141d1a1f1e1d1a1c1c20242e2720222c231c1c2837292c303134"
    "34341f27393d38323c2e333432ffc0000b080001000101011100ffc4001f0000010501010101"
    "010100000000000000000102030405060708090a0bffc400b510000201030302040305050404"
    "0000017d01020300041105122131410613516107227114328191a1082342b1c11552d1f02433"
    "627282090a161718191a25262728292a3435363738393a434445464748494a53545556575859"
    "5a636465666768696a737475767778797a838485868788898a92939495969798999aa2a3a4a5"
    "a6a7a8a9aab2b3b4b5b6b7b8b9bac2c3c4c5c6c7c8c9cad2d3d4d5d6d7d8d9dae1e2e3e4e5e6"
    "e7e8e9eaf1f2f3f4f5f6f7f8f9faffda000c03010002110311003f00f7fa28a2803fffd9"
)


def png_1x1() -> bytes:
    ihdr = struct.pack(">IIBBBBB", 1, 1, 8, 2, 0, 0, 0)
    raw = zlib.compress(b"\x00\x00\x00\x00")
    return b"\x89PNG\r\n\x1a\n" + _chunk(b"IHDR", ihdr) + _chunk(b"IDAT", raw) + _chunk(b"IEND", b"")


def jpeg_1x1() -> bytes:
    return JPEG_1X1


def facts(kind: str) -> dict:
    data = png_1x1() if kind == "png" else jpeg_1x1()
    media_type = "image/png" if kind == "png" else "image/jpeg"
    return {
        "mediaType": media_type,
        "width": 1,
        "height": 1,
        "byteLength": len(data),
        "sha256": hashlib.sha256(data).hexdigest(),
        "bytesBase64": base64.b64encode(data).decode("ascii"),
        "bytes": data,
    }


def _chunk(tag: bytes, data: bytes) -> bytes:
    crc = zlib.crc32(tag + data) & 0xFFFFFFFF
    return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", crc)
