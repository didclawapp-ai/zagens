#!/usr/bin/env python3
"""Extract runtime_api submodules from mod.rs (R-003 A4.5)."""

from __future__ import annotations

import re
import sys
from pathlib import Path

MOD = Path("crates/tui/src/runtime_api/mod.rs")
ROOT = Path("crates/tui/src/runtime_api")

SKILLS_RANGES = [
    (r"^struct SkillEntry", r"^struct McpServerEntry"),
    (r"^async fn list_skills", r"^async fn list_mcp_servers"),
    (r"^fn validate_skill_directory_name", r"^fn load_mcp_config_or_default"),
]

MCP_RANGES = [
    (r"^struct McpServerEntry", r"^struct AutomationRunsQuery"),
    (r"^async fn list_mcp_servers", r"^async fn list_automations"),
    (r"^fn load_mcp_config_or_default", r"^struct UsageQuery"),
]

AUTOMATIONS_RANGES = [
    (r"^struct AutomationRunsQuery", r"^fn deserialize_query_bool_option"),
    (r"^async fn list_automations", r"^pub\(crate\) fn truncate_text"),
    (r"^fn map_automation_err", r"^pub\(crate\) fn map_thread_err"),
]

SKILLS_HANDLERS = (
    "list_skills",
    "create_skill",
    "import_skill_local",
    "install_skill_remote",
)

MCP_HANDLERS = (
    "list_mcp_servers",
    "merge_mcp_config_json",
    "add_mcp_server",
    "get_mcp_server",
    "update_mcp_server",
    "delete_mcp_server",
    "list_mcp_tools",
)

AUTOMATIONS_HANDLERS = (
    "list_automations",
    "create_automation",
    "get_automation",
    "update_automation",
    "delete_automation",
    "run_automation",
    "pause_automation",
    "resume_automation",
    "list_automation_runs",
)

SKILLS_HEADER = """//! Skills install/list HTTP handlers (R-003 A4.5).

use std::fs;
use std::path::{Path, PathBuf};

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::skills::install::{
    import_local_directory, InstallError, InstallOutcome, InstallSource, DEFAULT_MAX_SIZE_BYTES,
};
use crate::skills::{install, SkillRegistry};

use super::{ApiError, RuntimeApiState};

"""

MCP_HEADER = """//! MCP server registry HTTP handlers (R-003 A4.5).

use std::collections::HashSet;
use std::fs;
use std::time::Duration;

use axum::extract::{Path as AxumPath, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::mcp::{McpConfig, McpPool, McpServerConfig};

use super::{ApiError, RuntimeApiState};

"""

AUTOMATIONS_HEADER = """//! Scheduled automations HTTP handlers (R-003 A4.5).

use axum::extract::{Path as AxumPath, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;

use crate::automation_manager::{
    AutomationRecord, AutomationRunRecord, CreateAutomationRequest, UpdateAutomationRequest,
};

use super::{ApiError, RuntimeApiState};

"""


def find_line(lines: list[str], pattern: str, start: int = 0) -> int:
    rx = re.compile(pattern)
    for i in range(start, len(lines)):
        if rx.match(lines[i]):
            return i
    raise ValueError(f"pattern not found: {pattern!r} (from line {start + 1})")


def collect(lines: list[str], ranges: list[tuple[str, str]]) -> list[str]:
    out: list[str] = []
    for start_pat, end_pat in ranges:
        s = find_line(lines, start_pat)
        e = find_line(lines, end_pat, s)
        out.extend(lines[s:e])
        print(f"  chunk {s + 1}-{e}")
    return out


def publish(body: list[str], handlers: tuple[str, ...]) -> list[str]:
    result: list[str] = []
    for line in body:
        for h in handlers:
            if line.startswith(f"async fn {h}"):
                line = line.replace(f"async fn {h}", f"pub(crate) async fn {h}", 1)
                break
        if line.startswith("struct ") and not line.startswith("pub("):
            line = line.replace("struct ", "pub(crate) struct ", 1)
        result.append(line)
    return result


def main() -> int:
    lines = MOD.read_text(encoding="utf-8").splitlines(keepends=True)

    all_remove: set[int] = set()
    for name, ranges in (
        ("skills", SKILLS_RANGES),
        ("mcp", MCP_RANGES),
        ("automations", AUTOMATIONS_RANGES),
    ):
        for start_pat, end_pat in ranges:
            s = find_line(lines, start_pat)
            e = find_line(lines, end_pat, s)
            for i in range(s, e):
                all_remove.add(i)

    print("skills.rs:")
    skills_body = publish(collect(lines, SKILLS_RANGES), SKILLS_HANDLERS)
    (ROOT / "skills.rs").write_text(SKILLS_HEADER + "".join(skills_body), encoding="utf-8")

    print("mcp.rs:")
    mcp_body = publish(collect(lines, MCP_RANGES), MCP_HANDLERS)
    (ROOT / "mcp.rs").write_text(MCP_HEADER + "".join(mcp_body), encoding="utf-8")

    print("automations.rs:")
    auto_body = publish(collect(lines, AUTOMATIONS_RANGES), AUTOMATIONS_HANDLERS)
    (ROOT / "automations.rs").write_text(
        AUTOMATIONS_HEADER + "".join(auto_body), encoding="utf-8"
    )

    out = [ln for i, ln in enumerate(lines) if i not in all_remove]
    MOD.write_text("".join(out), encoding="utf-8")
    print(
        f"wrote skills={len(skills_body)} mcp={len(mcp_body)} auto={len(auto_body)} "
        f"mod.rs={len(out)} lines"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
