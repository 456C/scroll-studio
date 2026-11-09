from __future__ import annotations

import secrets
from datetime import datetime, timezone


def server_now():
    return datetime.now(timezone.utc)


def new_credential():
    return "cred_live_" + secrets.token_hex(16), "sek_live_" + secrets.token_hex(32)


def new_authorization_code():
    return secrets.token_urlsafe(32)
