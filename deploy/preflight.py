"""Fail closed for the draft systemd service, without writing profile data."""
import ipaddress
import os
from pathlib import Path
import sys


def check(environ):
    if environ.get("CHAO_OWNER_BOOTSTRAP", "0").lower() not in ("0", "false", ""):
        raise ValueError("Owner bootstrap must be disabled")
    if not ipaddress.ip_address(environ.get("GAME_HOST", "127.0.0.1")).is_loopback:
        raise ValueError("The Rust authority must listen on loopback")
    data = Path(environ.get("CHAO_DATA_DIR", ""))
    if not data.is_absolute():
        raise ValueError("CHAO_DATA_DIR must be absolute")
    if environ.get("CHAO_SHUTDOWN_STDIN", "0") != "0":
        raise ValueError("systemd must use SIGTERM, not launcher stdin")


if __name__ == "__main__":
    # systemd has already loaded EnvironmentFile; do not parse shell/env syntax.
    check(os.environ)
    print("Private single-authority configuration accepted")
