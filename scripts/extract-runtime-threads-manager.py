#!/usr/bin/env python3
"""Extract manager.rs from runtime_threads/mod.rs (R-003 A4.6 phase 2)."""

from __future__ import annotations

import re
import sys
from pathlib import Path

MOD = Path("crates/tui/src/runtime_threads/mod.rs")
ROOT = Path("crates/tui/src/runtime_threads")

RANGES = [
    (r"^fn load_routing_rules", r"^pub\(crate\) fn provider_label_for_model"),
    (r"^struct ActiveTurnState", r"^pub fn summarize_text"),
]

HEADER = r'''//! Active engine threads, turns, and live event broadcast (R-003 A4.6).

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::{Mutex, broadcast};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::config::{Config, DEFAULT_TEXT_MODEL, MAX_SUBAGENTS};
use crate::context_snapshot::{ThreadContextSnapshot, build_thread_context_snapshot};
use crate::core::coherence::CoherenceState;
use crate::core::engine::{EngineConfig, EngineHandle, spawn_engine};
use crate::core::events::{Event as EngineEvent, TurnOutcomeStatus};
use crate::core::ops::Op;
use crate::models::{ContentBlock, Message, SystemPrompt, Usage};
use crate::tools::plan::new_shared_plan_state;
use crate::tools::subagent::SubAgentStatus;
use crate::tools::todo::new_shared_todo_list;
use crate::tui::app::AppMode;

use super::events::collect_agent_rebind_hints;
use super::persist::{
    duration_ms, reconstruct_messages_for_store, write_json_atomic, RuntimeThreadStore,
};
use super::types::*;
use super::{
    summarize_text, CompactThreadRequest, CreateThreadRequest, RuntimeThreadManagerConfig,
    StartTurnRequest, SteerTurnRequest, ThreadDetail, ThreadListFilter, UpdateThreadRequest,
    UsageAggregation, UsageGroupBy, EVENT_CHANNEL_CAPACITY, RUNTIME_RESTART_REASON,
    SUMMARY_LIMIT,
};

'''


def find_line(lines: list[str], pattern: str, start: int = 0) -> int:
    rx = re.compile(pattern)
    for i in range(start, len(lines)):
        if rx.match(lines[i]):
            return i
    raise ValueError(f"not found: {pattern!r} from {start + 1}")


def publish(body: list[str]) -> list[str]:
    out: list[str] = []
    for line in body:
        if line.startswith("    fn open_with_store("):
            line = line.replace("fn open_with_store", "pub(crate) fn open_with_store", 1)
        if line.startswith("    fn approval_decision("):
            line = line.replace("fn approval_decision", "pub(crate) fn approval_decision", 1)
        if line.startswith("fn touch_lru") or line.startswith("fn parse_mode") or line.startswith(
            "fn tool_kind_for_name"
        ):
            line = line.replace("fn ", "pub(crate) fn ", 1)
        out.append(line)
    return out


def main() -> int:
    lines = MOD.read_text(encoding="utf-8").splitlines(keepends=True)
    body: list[str] = []
    remove: set[int] = set()
    for s_pat, e_pat in RANGES:
        s = find_line(lines, s_pat)
        e = find_line(lines, e_pat, s)
        body.extend(lines[s:e])
        for i in range(s, e):
            remove.add(i)
        print(f"remove {s + 1}-{e} ({s_pat[:32]})")

    body = publish(body)
    (ROOT / "manager.rs").write_text(HEADER + "".join(body), encoding="utf-8")

    out = [ln for i, ln in enumerate(lines) if i not in remove]
    MOD.write_text("".join(out), encoding="utf-8")
    print(f"manager.rs {len(body)} lines; mod.rs {len(out)} lines")
    return 0


if __name__ == "__main__":
    sys.exit(main())
