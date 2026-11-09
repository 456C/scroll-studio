from __future__ import annotations

from datetime import datetime


class FixtureHarness:
    def __init__(self, clock: datetime, pause: bool = False, login_account_id: str | None = None):
        self.clock = clock
        self.pause = pause
        self.login_account_id = login_account_id
        self.redirect_url = None

    def now(self):
        return self.clock

    def next_code(self, world):
        return _next(world, "code", "code_test_")

    def next_credential(self, world):
        number = _next(world, "credential", "", width=16)
        return f"cred_test_{number}", f"sek_test_{number}"


def _next(world, name, prefix, width=16):
    world["runtime"]["counters"][name] += 1
    return prefix + f"{world['runtime']['counters'][name]:0{width}d}"
