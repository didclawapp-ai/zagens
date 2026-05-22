#!/usr/bin/env python3
"""Remove thread-handler blocks from runtime_api/mod.rs (R-003 A4.4)."""

from __future__ import annotations

import re
import sys
from pathlib import Path

MOD = Path("crates/tui/src/runtime_api/mod.rs")

# (start_pattern, end_pattern_exclusive) — end line is first match of end_pattern
RANGES = [
    (r"^struct ThreadsQuery", r"^struct WorkspaceStatusResponse"),
    (r"^struct PersistThreadSessionRequest", r"^async fn create_thread"),
    (r"^async fn create_thread", r"^async fn workspace_status"),
    (r"^// --- Thread workspace:", r"^async fn list_skills"),
    (r"^async fn get_thread\b", r"^async fn list_tasks\b"),
]


def find_line(lines: list[str], pattern: str, start: int = 0) -> int:
    rx = re.compile(pattern)
    for i in range(start, len(lines)):
        if rx.match(lines[i]):
            return i
    raise ValueError(f"pattern not found: {pattern!r} (from line {start + 1})")


def main() -> int:
    text = MOD.read_text(encoding="utf-8")
    lines = text.splitlines(keepends=True)

    remove: set[int] = set()
    for start_pat, end_pat in RANGES:
        s = find_line(lines, start_pat)
        e = find_line(lines, end_pat, s)
        for i in range(s, e):
            remove.add(i)
        print(f"remove lines {s + 1}-{e} ({end_pat})")

    out = [ln for i, ln in enumerate(lines) if i not in remove]
    MOD.write_text("".join(out), encoding="utf-8")
    print(f"wrote {len(out)} lines (removed {len(remove)})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
