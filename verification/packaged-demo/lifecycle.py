"""Loopback occupancy check and owned-process cleanup.

A process is signaled only through a durable handle captured at spawn, and
only when its identity still matches that snapshot. Occupied ports are not
cleared.
"""

from __future__ import annotations

import os
import signal
import subprocess
import sys
from dataclasses import dataclass
from typing import Sequence

LOOPBACK_HOST = "127.0.0.1"


class LifecycleError(Exception):
    def __init__(self, code: str, detail: str) -> None:
        super().__init__(detail)
        self.code = code
        self.detail = detail


@dataclass(frozen=True)
class ProcessIdentity:
    pid: int
    start_token: str
    ppid: int
    argv: tuple[str, ...]


@dataclass
class OwnedProcess:
    pid: int
    launcher_pid: int
    identity: ProcessIdentity
    argv: tuple[str, ...]
    pidfd: int | None
    process: subprocess.Popen[bytes]

    def close(self) -> None:
        if self.pidfd is None:
            return
        os.close(self.pidfd)
        self.pidfd = None


@dataclass(frozen=True)
class CleanupResult:
    signaled: bool
    terminated: bool
    reason: str
    pid: int | None
    start_token: str | None

    def as_evidence(self) -> dict[str, object]:
        return {
            "status": "recorded",
            "signaled": self.signaled,
            "terminated": self.terminated,
            "reason": self.reason,
            "pid": self.pid,
            "startToken": self.start_token,
            "pidsSignaled": [self.pid] if self.signaled and self.pid is not None else [],
            "killedByPort": False,
            "killedByImageName": False,
        }


def port_occupied(host: str, port: int, timeout: float = 0.5) -> bool:
    if host != LOOPBACK_HOST:
        raise LifecycleError("non-loopback", "refusing to probe a non-loopback host")
    if isinstance(port, bool) or not isinstance(port, int) or not 1 <= port <= 65535:
        raise LifecycleError("malformed", "port")
    sock = _socket().socket(_socket().AF_INET, _socket().SOCK_STREAM)
    sock.settimeout(timeout)
    try:
        try:
            sock.connect((host, port))
        except ConnectionRefusedError:
            return False
        except OSError:
            return True
        return True
    finally:
        sock.close()


def spawn_owned(argv: Sequence[str], cwd: str) -> OwnedProcess:
    if isinstance(argv, str) or not isinstance(argv, (list, tuple)):
        raise LifecycleError("malformed", "argv must be a sequence")
    argv_t = tuple(argv)
    if not argv_t or not all(isinstance(item, str) and item and "\x00" not in item for item in argv_t):
        raise LifecycleError("malformed", "argv entries must be non-empty strings")
    if _is_shell(argv_t[0]):
        raise LifecycleError("malformed", "start command must not be a shell")
    if os.path.islink(cwd) or not os.path.isdir(cwd):
        raise LifecycleError("unsafe-path", "working directory is not a real directory")
    if sys.platform != "win32" and not hasattr(os, "pidfd_open"):
        raise LifecycleError("no-durable-handle", "linux pidfd is unavailable")
    process = subprocess.Popen(
        list(argv_t),
        cwd=cwd,
        shell=False,
        stdin=subprocess.DEVNULL,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    pidfd: int | None = None
    try:
        if sys.platform == "win32":
            identity = _wait_windows_identity(process, argv_t)
        else:
            pidfd = os.pidfd_open(process.pid)
            identity = _wait_linux_identity(process.pid, argv_t)
        if identity is None or identity.ppid != os.getpid() or identity.argv != argv_t:
            raise LifecycleError("identity-not-established", "spawned process identity did not match")
        return OwnedProcess(
            pid=process.pid,
            launcher_pid=os.getpid(),
            identity=identity,
            argv=argv_t,
            pidfd=pidfd,
            process=process,
        )
    except Exception:
        _emergency_stop(process, pidfd)
        raise


def cleanup_owned(owned: OwnedProcess) -> CleanupResult:
    if sys.platform == "win32":
        return _cleanup_windows(owned)
    return _cleanup_linux(owned)


def ownership_matches(recorded: ProcessIdentity, current: ProcessIdentity | None, launcher_pid: int) -> bool:
    if current is None:
        return False
    if recorded.pid != current.pid or recorded.pid <= 0:
        return False
    if not recorded.start_token or recorded.start_token != current.start_token:
        return False
    if recorded.ppid != launcher_pid or current.ppid != launcher_pid:
        return False
    if not recorded.argv or recorded.argv != current.argv:
        return False
    return True


def _socket():
    import socket

    return socket


def _cleanup_linux(owned: OwnedProcess) -> CleanupResult:
    current = _linux_identity(owned.pid)
    if current is None:
        return CleanupResult(False, False, "already-exited-or-unreadable", owned.pid, owned.identity.start_token)
    if not ownership_matches(owned.identity, current, owned.launcher_pid):
        return CleanupResult(False, False, "identity-not-established", owned.pid, owned.identity.start_token)
    if owned.pidfd is None:
        return CleanupResult(False, False, "no-durable-handle", owned.pid, owned.identity.start_token)
    try:
        signal.pidfd_send_signal(owned.pidfd, signal.SIGTERM)
    except OSError:
        return CleanupResult(False, False, "signal-failed", owned.pid, owned.identity.start_token)
    try:
        owned.process.wait(timeout=2)
    except subprocess.TimeoutExpired:
        again = _linux_identity(owned.pid)
        if not ownership_matches(owned.identity, again, owned.launcher_pid):
            return CleanupResult(True, False, "identity-changed-after-term", owned.pid, owned.identity.start_token)
        try:
            signal.pidfd_send_signal(owned.pidfd, signal.SIGKILL)
        except OSError:
            return CleanupResult(True, False, "kill-failed", owned.pid, owned.identity.start_token)
        try:
            owned.process.wait(timeout=2)
        except subprocess.TimeoutExpired:
            return CleanupResult(True, False, "still-alive", owned.pid, owned.identity.start_token)
    alive = owned.process.poll() is None
    return CleanupResult(
        True,
        not alive,
        "terminated" if not alive else "still-alive",
        owned.pid,
        owned.identity.start_token,
    )


def _cleanup_windows(owned: OwnedProcess) -> CleanupResult:
    handle = int(owned.process._handle)  # noqa: SLF001 - CreateProcess handle, not a pid lookup
    token = _windows_creation_token(handle)
    if not token or token != owned.identity.start_token:
        return CleanupResult(False, False, "identity-not-established", owned.pid, owned.identity.start_token)
    current = _windows_identity(handle, owned.pid)
    if current is not None and current.argv != owned.identity.argv:
        return CleanupResult(False, False, "identity-not-established", owned.pid, owned.identity.start_token)
    if not _terminate_windows(handle):
        return CleanupResult(False, False, "signal-failed", owned.pid, owned.identity.start_token)
    try:
        owned.process.wait(timeout=2)
    except subprocess.TimeoutExpired:
        return CleanupResult(True, False, "still-alive", owned.pid, owned.identity.start_token)
    return CleanupResult(True, True, "terminated", owned.pid, owned.identity.start_token)


def _emergency_stop(process: subprocess.Popen[bytes], pidfd: int | None) -> None:
    try:
        if sys.platform == "win32":
            _terminate_windows(int(process._handle))  # noqa: SLF001
        elif pidfd is not None:
            try:
                signal.pidfd_send_signal(pidfd, signal.SIGKILL)
            except OSError:
                pass
    finally:
        if pidfd is not None:
            os.close(pidfd)
    try:
        process.wait(timeout=2)
    except subprocess.TimeoutExpired:
        return


def _wait_linux_identity(pid: int, argv: tuple[str, ...]) -> ProcessIdentity | None:
    identity = None
    for _ in range(50):
        identity = _linux_identity(pid)
        if identity is not None and identity.argv == argv and identity.ppid == os.getpid():
            return identity
        _sleep_briefly()
    return None


def _linux_identity(pid: int) -> ProcessIdentity | None:
    try:
        with open(f"/proc/{pid}/stat", "rb") as handle:
            raw = handle.read()
        with open(f"/proc/{pid}/cmdline", "rb") as handle:
            command = handle.read()
    except OSError:
        return None
    marker = raw.rfind(b")")
    if marker < 0:
        return None
    fields = raw[marker + 2 :].split()
    if len(fields) < 20:
        return None
    try:
        ppid = int(fields[1])
        start_token = fields[19].decode("ascii")
    except (ValueError, UnicodeError):
        return None
    argv = tuple(part.decode("utf-8", "surrogateescape") for part in command.split(b"\0") if part)
    return ProcessIdentity(pid=pid, start_token=start_token, ppid=ppid, argv=argv)


def _wait_windows_identity(process: subprocess.Popen[bytes], argv: tuple[str, ...]) -> ProcessIdentity | None:
    handle = int(process._handle)  # noqa: SLF001
    token = _windows_creation_token(handle)
    parent = _windows_parent_pid(handle)
    if not token or parent != os.getpid():
        return None
    identity = _windows_identity(handle, process.pid)
    if identity is None or identity.argv != argv or identity.start_token != token or identity.ppid != parent:
        return None
    return identity


def _windows_identity(handle: int, pid: int) -> ProcessIdentity | None:
    command = _windows_command_line(handle)
    if command is None:
        return None
    parsed = _windows_argv(command)
    if not parsed:
        return None
    token = _windows_creation_token(handle)
    parent = _windows_parent_pid(handle)
    if not token or parent is None:
        return None
    return ProcessIdentity(pid=pid, start_token=token, ppid=parent, argv=parsed)


def _windows_creation_token(handle: int) -> str | None:
    import ctypes
    from ctypes import wintypes

    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
    get_times = kernel32.GetProcessTimes
    get_times.argtypes = [
        wintypes.HANDLE,
        ctypes.POINTER(wintypes.FILETIME),
        ctypes.POINTER(wintypes.FILETIME),
        ctypes.POINTER(wintypes.FILETIME),
        ctypes.POINTER(wintypes.FILETIME),
    ]
    get_times.restype = wintypes.BOOL
    created = wintypes.FILETIME()
    exited = wintypes.FILETIME()
    kernel = wintypes.FILETIME()
    user = wintypes.FILETIME()
    if not get_times(handle, ctypes.byref(created), ctypes.byref(exited), ctypes.byref(kernel), ctypes.byref(user)):
        return None
    return f"{created.dwHighDateTime:08x}{created.dwLowDateTime:08x}"


def _windows_parent_pid(handle: int) -> int | None:
    import ctypes
    from ctypes import wintypes

    if ctypes.sizeof(ctypes.c_void_p) != 8:
        return None

    class ProcessBasicInformation(ctypes.Structure):
        _fields_ = [
            ("ExitStatus", ctypes.c_ulong),
            ("PebBaseAddress", ctypes.c_void_p),
            ("AffinityMask", ctypes.c_ulonglong),
            ("BasePriority", ctypes.c_ulong),
            ("UniqueProcessId", ctypes.c_ulonglong),
            ("InheritedFromUniqueProcessId", ctypes.c_ulonglong),
        ]

    ntdll = ctypes.WinDLL("ntdll")
    query = ntdll.NtQueryInformationProcess
    query.argtypes = [
        wintypes.HANDLE,
        ctypes.c_ulong,
        ctypes.c_void_p,
        ctypes.c_ulong,
        ctypes.POINTER(ctypes.c_ulong),
    ]
    query.restype = ctypes.c_long
    info = ProcessBasicInformation()
    returned = ctypes.c_ulong(0)
    status = query(handle, 0, ctypes.byref(info), ctypes.sizeof(info), ctypes.byref(returned))
    if status != 0:
        return None
    return int(info.InheritedFromUniqueProcessId)


def _windows_command_line(handle: int) -> str | None:
    import ctypes
    from ctypes import wintypes

    if ctypes.sizeof(ctypes.c_void_p) != 8:
        return None

    class ProcessBasicInformation(ctypes.Structure):
        _fields_ = [
            ("ExitStatus", ctypes.c_ulong),
            ("PebBaseAddress", ctypes.c_void_p),
            ("AffinityMask", ctypes.c_ulonglong),
            ("BasePriority", ctypes.c_ulong),
            ("UniqueProcessId", ctypes.c_ulonglong),
            ("InheritedFromUniqueProcessId", ctypes.c_ulonglong),
        ]

    ntdll = ctypes.WinDLL("ntdll")
    query = ntdll.NtQueryInformationProcess
    query.argtypes = [
        wintypes.HANDLE,
        ctypes.c_ulong,
        ctypes.c_void_p,
        ctypes.c_ulong,
        ctypes.POINTER(ctypes.c_ulong),
    ]
    query.restype = ctypes.c_long
    info = ProcessBasicInformation()
    returned = ctypes.c_ulong(0)
    if query(handle, 0, ctypes.byref(info), ctypes.sizeof(info), ctypes.byref(returned)) != 0:
        return None
    if not info.PebBaseAddress:
        return None
    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
    read = kernel32.ReadProcessMemory
    read.argtypes = [
        wintypes.HANDLE,
        ctypes.c_void_p,
        ctypes.c_void_p,
        ctypes.c_size_t,
        ctypes.POINTER(ctypes.c_size_t),
    ]
    read.restype = wintypes.BOOL

    def read_at(address: int, size: int) -> bytes | None:
        buffer = ctypes.create_string_buffer(size)
        got = ctypes.c_size_t(0)
        if not read(handle, ctypes.c_void_p(address), buffer, size, ctypes.byref(got)):
            return None
        return buffer.raw[: got.value]

    params_raw = read_at(int(info.PebBaseAddress) + 0x20, 8)
    if params_raw is None or len(params_raw) != 8:
        return None
    params = int.from_bytes(params_raw, "little")
    unicode = read_at(params + 0x70, 16)
    if unicode is None or len(unicode) < 16:
        return None
    length = int.from_bytes(unicode[0:2], "little")
    buffer = int.from_bytes(unicode[8:16], "little")
    if length <= 0 or length > 32768 or buffer == 0:
        return None
    encoded = read_at(buffer, length)
    if encoded is None:
        return None
    return encoded.decode("utf-16-le", "replace")


def _windows_argv(command: str) -> tuple[str, ...] | None:
    import ctypes
    from ctypes import wintypes

    shell32 = ctypes.WinDLL("shell32", use_last_error=True)
    parse = shell32.CommandLineToArgvW
    parse.argtypes = [wintypes.LPCWSTR, ctypes.POINTER(ctypes.c_int)]
    parse.restype = ctypes.POINTER(wintypes.LPWSTR)
    count = ctypes.c_int(0)
    array = parse(command, ctypes.byref(count))
    if not array:
        return None
    try:
        return tuple(array[index] for index in range(count.value))
    finally:
        ctypes.windll.kernel32.LocalFree(array)


def _terminate_windows(handle: int) -> bool:
    import ctypes
    from ctypes import wintypes

    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
    terminate = kernel32.TerminateProcess
    terminate.argtypes = [wintypes.HANDLE, wintypes.UINT]
    terminate.restype = wintypes.BOOL
    return bool(terminate(handle, 1))


def _is_shell(argv0: str) -> bool:
    name = os.path.basename(argv0.replace("\\", "/")).lower()
    return name in {
        "sh",
        "bash",
        "dash",
        "zsh",
        "ksh",
        "fish",
        "cmd",
        "cmd.exe",
        "powershell",
        "powershell.exe",
        "pwsh",
        "pwsh.exe",
    }


def _sleep_briefly() -> None:
    import time

    time.sleep(0.01)
