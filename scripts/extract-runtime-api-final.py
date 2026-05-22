#!/usr/bin/env python3
"""Extract workspace/health/usage modules and tests.rs from runtime_api/mod.rs."""

from __future__ import annotations

import re
import sys
from pathlib import Path

MOD = Path("crates/tui/src/runtime_api/mod.rs")
ROOT = Path("crates/tui/src/runtime_api")

REMOVE = [
    (r"^struct HealthResponse", r"^struct WorkspaceStatusResponse"),
    (r"^async fn health\(\)", r"^async fn workspace_status"),
    (r"^async fn workspace_status", r"^pub\(crate\) fn truncate_text"),
    (r"^fn collect_workspace_status", r"^#\[derive\(Debug, Deserialize\)\]\nstruct UsageQuery"),
    (r"^#\[derive\(Debug, Deserialize\)\]\nstruct UsageQuery", r"^const DEFAULT_CORS_ORIGINS"),
]

# Fix UsageQuery - use line patterns without embedded newline
REMOVE = [
    (r"^struct HealthResponse", r"^struct WorkspaceStatusResponse"),
    (r"^async fn health\(\)", r"^async fn workspace_status"),
    (r"^async fn workspace_status", r"^pub\(crate\) fn truncate_text"),
    (r"^fn collect_workspace_status", r"^struct UsageQuery"),
    (r"^struct UsageQuery", r"^const DEFAULT_CORS_ORIGINS"),
]

HANDLERS = (
    "health",
    "internal_probe",
    "workspace_status",
    "get_usage",
    "get_routing_rules",
    "set_routing_rules",
    "rebuild_symbol_index",
)

HEALTH_HEADER = """//! Health and internal probe routes (R-003 A4.5).

use axum::extract::State;
use axum::Json;
use serde::Serialize;

use super::RuntimeApiState;

"""

WORKSPACE_HEADER = """//! Workspace git status (R-003 A4.5).

use std::path::PathBuf;
use std::process::Command;

use axum::extract::State;
use axum::Json;
use serde::Serialize;

use super::{ApiError, RuntimeApiState};

"""

USAGE_HEADER = """//! Usage aggregation, routing rules, symbol index rebuild (R-003 A4.5).

use std::path::PathBuf;

use axum::extract::{Query, State};
use axum::Json;
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::runtime_threads::UsageGroupBy;

use super::{ApiError, RuntimeApiState};

"""

TESTS_HEADER = """//! Integration tests for the runtime HTTP API (R-003 A4.5).

"""


def find_line(lines: list[str], pattern: str, start: int = 0) -> int:
    rx = re.compile(pattern)
    for i in range(start, len(lines)):
        if rx.match(lines[i]):
            return i
    raise ValueError(f"not found: {pattern!r} from {start + 1}")


def slice_ranges(lines: list[str], ranges: list[tuple[str, str]]) -> list[str]:
    out: list[str] = []
    for s_pat, e_pat in ranges:
        s = find_line(lines, s_pat)
        e = find_line(lines, e_pat, s)
        out.extend(lines[s:e])
        print(f"  {s + 1}-{e} {s_pat[:36]}")
    return out


def publish(body: list[str]) -> list[str]:
    result = []
    for line in body:
        for h in HANDLERS:
            if line.startswith(f"async fn {h}"):
                line = line.replace(f"async fn {h}", f"pub(crate) async fn {h}", 1)
                break
        if line.startswith("struct ") and not line.startswith("pub("):
            line = line.replace("struct ", "pub(crate) struct ", 1)
        if line.startswith("fn parse_iso8601") or line.startswith("fn collect_workspace") or line.startswith("fn run_git"):
            line = line.replace("fn ", "pub(crate) fn ", 1)
        result.append(line)
    return result


def main() -> int:
    text = MOD.read_text(encoding="utf-8")
    lines = text.splitlines(keepends=True)

    health_body = publish(
        slice_ranges(
            lines,
            [
                (r"^struct HealthResponse", r"^struct InternalProbeResponse"),
                (r"^struct InternalProbeResponse", r"^struct WorkspaceStatusResponse"),
                (r"^async fn health\(\)", r"^async fn workspace_status"),
            ],
        )
    )
    (ROOT / "health.rs").write_text(HEALTH_HEADER + "".join(health_body), encoding="utf-8")

    workspace_body = publish(
        slice_ranges(
            lines,
            [
                (r"^struct WorkspaceStatusResponse", r"^/// Accept"),
                (r"^async fn workspace_status", r"^pub\(crate\) fn truncate_text"),
                (r"^fn collect_workspace_status", r"^struct UsageQuery"),
            ],
        )
    )
    (ROOT / "workspace.rs").write_text(WORKSPACE_HEADER + "".join(workspace_body), encoding="utf-8")

    usage_body = publish(
        slice_ranges(lines, [(r"^struct UsageQuery", r"^const DEFAULT_CORS_ORIGINS")])
    )
    (ROOT / "usage.rs").write_text(USAGE_HEADER + "".join(usage_body), encoding="utf-8")

    # tests: from `    use super::*;` inside mod tests through closing `}`
    t_start = find_line(lines, r"^mod tests \{")
    use_super = find_line(lines, r"^    use super::\*;", t_start)
    t_end = len(lines) - 1
    while t_end > use_super and lines[t_end].strip() != "}":
        t_end -= 1
    tests_body = TESTS_HEADER + "".join(lines[use_super:t_end])
    (ROOT / "tests.rs").write_text(tests_body, encoding="utf-8")

    remove: set[int] = set()
    for s_pat, e_pat in REMOVE:
        s = find_line(lines, s_pat)
        e = find_line(lines, e_pat, s)
        for i in range(s, e):
            remove.add(i)
    for i in range(t_start, len(lines)):
        remove.add(i)

    out = [ln for i, ln in enumerate(lines) if i not in remove]
    out.append("\n#[cfg(test)]\n#[path = \"tests.rs\"]\nmod tests;\n")
    MOD.write_text("".join(out), encoding="utf-8")
    print(f"mod.rs -> {len(out)} lines; tests.rs -> {t_end - use_super} lines")
    return 0


if __name__ == "__main__":
    sys.exit(main())
