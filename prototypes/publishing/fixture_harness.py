from __future__ import annotations

from datetime import datetime


class FixtureHarness:
    def __init__(self, clock: datetime, pause: bool = False):
        self.clock = clock
        self.pause = pause
        self.redirect_url = None

    def now(self):
        return self.clock

    def next_code(self, world):
        return _next(world, "code", "code_test_")

    def next_credential(self, world):
        number = int(_next(world, "credential", "", width=1))
        return f"cred_live_{number:032x}", f"sek_live_{number:064x}"

    def next_session(self, world):
        return "sess_test_" + _next(world, "session", "", width=16)

    def next_csrf(self, world):
        return "csrf_test_" + _next(world, "csrf", "", width=16)


def _next(world, name, prefix, width=16):
    world["runtime"]["counters"][name] += 1
    return prefix + f"{world['runtime']['counters'][name]:0{width}d}"
