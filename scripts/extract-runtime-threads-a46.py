#!/usr/bin/env python3
"""Extract persist.rs and events.rs from runtime_threads/mod.rs (R-003 A4.6 phase 1)."""

from __future__ import annotations

import re
import sys
from pathlib import Path

MOD = Path("crates/tui/src/runtime_threads/mod.rs")
ROOT = Path("crates/tui/src/runtime_threads")

PERSIST_RANGES = [
    (r"^pub struct RuntimeThreadStore", r"^impl RuntimeThreadStore"),
    (r"^impl RuntimeThreadStore", r"^pub struct RuntimeThreadManagerConfig"),
    (r"^    pub fn aggregate_usage_linear", r"^/// Best-effort provider"),
]
PERSIST_HELPERS = [(r"^fn duration_ms", r"^fn reconstruct_messages_for_store")]
PERSIST_HELPERS2 = [(r"^fn write_json_atomic", r"^#\[cfg\(test\)\]")]

EVENTS_RANGES = [
    (r"^/// One sub-agent rebind hint", r"^pub fn summarize_text"),
]

PERSIST_HEADER = """//! Thread/turn/item disk store and usage aggregation (R-003 A4.6).

use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Utc};
use serde::Serialize;
use tokio::sync::Mutex;

use crate::models::{ContentBlock, Message};
use crate::thread_store_sqlite;

use super::types::*;
use super::{CURRENT_RUNTIME_SCHEMA_VERSION, SUMMARY_LIMIT};

"""

EVENTS_HEADER = """//! Event timeline helpers for agent card rebind (R-003 A4.6).

use super::types::RuntimeEventRecord;

"""


def find_line(lines: list[str], pattern: str, start: int = 0) -> int:
    rx = re.compile(pattern)
    for i in range(start, len(lines)):
        if rx.match(lines[i]):
            return i
    raise ValueError(f"pattern not found: {pattern!r} from line {start + 1}")


def collect(lines: list[str], ranges: list[tuple[str, str]]) -> list[str]:
    out: list[str] = []
    for s_pat, e_pat in ranges:
        s = find_line(lines, s_pat)
        e = find_line(lines, e_pat, s)
        out.extend(lines[s:e])
        print(f"  {s + 1}-{e} {s_pat[:40]}")
    return out


def publish_persist(body: list[str]) -> list[str]:
    out = []
    for line in body:
        if (
            line.startswith("fn duration_ms")
            or line.startswith("fn reconstruct_messages")
            or line.startswith("fn write_json_atomic")
        ):
            line = line.replace("fn ", "pub(super) fn ", 1)
        out.append(line)
    return out


def main() -> int:
    lines = MOD.read_text(encoding="utf-8").splitlines(keepends=True)

    print("persist.rs:")
    persist_body = publish_persist(
        collect(lines, PERSIST_RANGES)
        + collect(lines, PERSIST_HELPERS)
        + collect(lines, PERSIST_HELPERS2)
    )
    (ROOT / "persist.rs").write_text(PERSIST_HEADER + "".join(persist_body), encoding="utf-8")

    print("events.rs:")
    events_body = collect(lines, EVENTS_RANGES)
    (ROOT / "events.rs").write_text(EVENTS_HEADER + "".join(events_body), encoding="utf-8")

    remove: set[int] = set()
    all_ranges = PERSIST_RANGES + PERSIST_HELPERS + PERSIST_HELPERS2 + EVENTS_RANGES
    for s_pat, e_pat in all_ranges:
        s = find_line(lines, s_pat)
        e = find_line(lines, e_pat, s)
        for i in range(s, e):
            remove.add(i)

    out = [ln for i, ln in enumerate(lines) if i not in remove]
    MOD.write_text("".join(out), encoding="utf-8")
    print(f"mod.rs -> {len(out)} lines; persist={len(persist_body)} events={len(events_body)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
