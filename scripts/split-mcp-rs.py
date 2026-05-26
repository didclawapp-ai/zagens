#!/usr/bin/env python3
"""Split mcp.rs into mcp/ submodule (D1)."""

from __future__ import annotations

from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SRC = ROOT / "crates/tui/src/mcp.rs"
OUT = ROOT / "crates/tui/src/mcp"

RANGES: dict[str, list[tuple[int, int]]] = {
    "diagnostics.rs": [(24, 84)],
    "config.rs": [(88, 188)],
    "types.rs": [(190, 253)],
    "transport.rs": [(255, 528)],
    "connection.rs": [(533, 965)],
    "pool.rs": [(967, 1505)],
    "config_io.rs": [(1507, 1921)],
    "format.rs": [(1927, 1954)],
}

HEADERS = {
    "diagnostics.rs": """use reqwest;

""",
    "config.rs": """use std::time::Duration;

use serde::{Deserialize, Serialize};

""",
    "types.rs": """use serde::{Deserialize, Serialize};

""",
    "transport.rs": """use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::process::{Child, ChildStdin, ChildStdout};

use crate::network_policy::{Decision, NetworkPolicyDecider, host_from_url};

use super::config::McpServerConfig;
use super::diagnostics::{bounded_body_excerpt, mask_url_secrets, ERROR_BODY_PREVIEW_BYTES};
use super::types::{McpPrompt, McpPromptArgument, McpResource, McpResourceTemplate, McpTool};

""",
    "connection.rs": """use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use anyhow::{Context, Result};
use serde_json::json;
use tokio::process::Command;

use crate::network_policy::NetworkPolicyDecider;

use super::config::{McpServerConfig, McpTimeouts};
use super::transport::{McpTransport, SseTransport, StdioTransport};
use super::types::{
    ConnectionState, McpPrompt, McpResource, McpResourceTemplate, McpTool,
};

""",
    "pool.rs": """use std::collections::HashMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde_json::json;

use crate::network_policy::NetworkPolicyDecider;

use super::config::{McpConfig, McpServerConfig};
use super::connection::McpConnection;
use super::types::{McpPrompt, McpResource, McpResourceTemplate, McpTool};

""",
    "config_io.rs": """use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::network_policy::NetworkPolicyDecider;
use crate::utils::write_atomic;

use super::config::{McpConfig, McpServerConfig};
use super::pool::McpPool;

""",
    "format.rs": """use serde_json;

""",
}


def slice_lines(lines: list[str], start: int, end: int) -> list[str]:
    return lines[start - 1 : end]


def extract_body(lines: list[str], ranges: list[tuple[int, int]]) -> list[str]:
    body: list[str] = []
    for start, end in ranges:
        body.extend(slice_lines(lines, start, end))
    return body


def main() -> None:
    if not SRC.exists():
        raise SystemExit(f"missing {SRC}")
    lines = SRC.read_text(encoding="utf-8-sig", errors="replace").splitlines(keepends=True)
    OUT.mkdir(parents=True, exist_ok=True)

    for name, ranges in RANGES.items():
        body = extract_body(lines, ranges)
        hdr = HEADERS.get(name, "")
        (OUT / name).write_text(hdr + "".join(body), encoding="utf-8")

    test_start = next(i for i, l in enumerate(lines) if l.strip() == "mod tests {") + 1
    test_end = len(lines) - 1
    while test_end > test_start and lines[test_end].strip() != "}":
        test_end -= 1
    tests_body = lines[test_start:test_end]
    while tests_body and tests_body[0].strip() in {"use super::*;", ""}:
        tests_body.pop(0)

    tests_hdr = """use self::config::{McpConfig, McpServerConfig};
use self::diagnostics::{mask_url_secrets, redact_body_preview};
use self::transport::StdioTransport;
use anyhow::Result;
use std::time::Duration;

"""
    (OUT / "tests.inc.rs").write_text(tests_hdr + "".join(tests_body), encoding="utf-8")

    mod_rs = """//! Async MCP (Model Context Protocol) implementation.

mod config;
mod config_io;
mod connection;
mod diagnostics;
mod format;
mod pool;
mod transport;
mod types;

pub use config::{McpConfig, McpServerConfig, McpTimeouts};
pub use config_io::{
    add_server_config, discover_manager_snapshot, get_server_entry, init_config, load_config,
    manager_snapshot_from_config, merge_mcp_json_fragment, remove_server_config,
    remove_server_from_config, replace_server_in_config, save_config, set_server_enabled,
    McpDiscoveredItem, McpManagerSnapshot, McpServerSnapshot, McpWriteStatus,
};
pub use connection::McpConnection;
pub use format::format_tool_result;
pub use pool::McpPool;
pub use transport::McpTransport;
pub use types::{
    ConnectionState, McpPrompt, McpPromptArgument, McpResource, McpResourceTemplate, McpTool,
};

#[cfg(test)]
include!("tests.inc.rs");
"""
    (OUT / "mod.rs").write_text(mod_rs, encoding="utf-8")
    SRC.unlink()
    print("ok", OUT)


if __name__ == "__main__":
    main()
