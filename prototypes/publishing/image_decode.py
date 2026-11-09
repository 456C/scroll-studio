from __future__ import annotations

import shutil
import subprocess


def decode_size(data: bytes, max_pixels: int):
    if shutil.which("ffmpeg") is None or shutil.which("ffprobe") is None:
        raise RuntimeError("image acceptance requires ffmpeg and ffprobe")
    probe = subprocess.run(
        [
            "ffprobe",
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height",
            "-of",
            "csv=p=0",
            "-i",
            "pipe:0",
        ],
        input=data,
        capture_output=True,
        timeout=5,
        check=False,
    )
    if probe.returncode != 0:
        return None
    text = probe.stdout.decode("ascii", errors="replace").strip()
    parts = text.split(",")
    if len(parts) != 2:
        return None
    width, height = int(parts[0]), int(parts[1])
    if width < 1 or height < 1:
        return None
    if width * height > max_pixels:
        return "limit"
    expected = width * height * 3
    proc = subprocess.Popen(
        [
            "ffmpeg",
            "-hide_banner",
            "-loglevel",
            "error",
            "-i",
            "pipe:0",
            "-frames:v",
            "1",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "pipe:1",
        ],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    try:
        raw, _err = proc.communicate(data, timeout=5)
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.communicate()
        return "limit"
    if proc.returncode != 0 or len(raw) != expected:
        return "limit" if len(raw) > expected else None
    return (width, height)
