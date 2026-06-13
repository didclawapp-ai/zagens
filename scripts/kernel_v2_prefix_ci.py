#!/usr/bin/env python3
"""Kernel v2 M5 prefix stability CI gate.

Runs the Rust request_fingerprint test suite (static/full prefix hashes,
workspace-seed fixture stability, jitter detection). Intended for CI and
local verify before prompt/tool-catalog changes.
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent


def main() -> int:
    cmd = [
        "cargo",
        "test",
        "-p",
        "zagens-cli",
        "request_fingerprint",
        "--",
        "--nocapture",
    ]
    print("+", " ".join(cmd), flush=True)
    result = subprocess.run(cmd, cwd=REPO, check=False)
    if result.returncode != 0:
        print("kernel_v2_prefix_ci: request_fingerprint tests failed", file=sys.stderr)
    return result.returncode


if __name__ == "__main__":
    sys.exit(main())
