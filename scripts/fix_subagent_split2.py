#!/usr/bin/env python3
"""Second-pass fixes for subagent module split."""
from __future__ import annotations

import pathlib
import re

SUBAGENT = pathlib.Path(__file__).resolve().parents[1] / "crates/runtime-server/src/tools/subagent"

# Slim preludes for leaf modules
RESIDENT_PRELUDE = "use anyhow::Result;\n"
PROMPT_TEXT_HEADER = "#![allow(clippy::needless_raw_string_hashes)]\n"

TOOL_PRELUDE = """
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{Value, json};

use deepseek_core::subagent::{SubAgentAssignment, SubAgentResult, SubAgentStatus, SubAgentType};
use crate::tools::spec::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec,
    optional_bool, optional_u64, required_str,
};
""".strip()

PRELUDE_START = "use std::collections::{HashMap, VecDeque};"
PRELUDE_END = "use super::mailbox::{Mailbox, MailboxEnvelope, MailboxReceiver};"
PRELUDE_RE = re.compile(
    re.escape(PRELUDE_START) + r".*?" + re.escape(PRELUDE_END) + r"\n\n?",
    re.DOTALL,
)

# Fix tool preludes
for tool in (SUBAGENT / "tools").glob("*.rs"):
    if tool.name == "mod.rs":
        continue
    text = tool.read_text(encoding="utf-8")
    if PRELUDE_START in text:
        text = PRELUDE_RE.sub(TOOL_PRELUDE + "\n\n", text, count=1)
    elif "ToolContext" not in text:
        # already slim — ensure ToolContext present
        text = text.replace(
            "ToolCapability, ToolError",
            "ToolCapability, ToolContext, ToolError",
        )
    tool.write_text(text, encoding="utf-8")

# resident.rs slim
(SUBAGENT / "resident.rs").write_text(
    RESIDENT_PRELUDE
    + "\n\n"
    + (SUBAGENT / "resident.rs")
    .read_text(encoding="utf-8")
    .split("static RESIDENT_LEASES", 1)[1],
    encoding="utf-8",
)

# Add promote lease helper
resident = SUBAGENT / "resident.rs"
rt = resident.read_text(encoding="utf-8")
if "upgrade_pending_resident_lease" not in rt:
    rt += """
/// Replace a pending resident-file lease placeholder with the spawned agent id.
pub(super) fn upgrade_pending_resident_lease(file_path: &str, agent_id: &str) {
    if let Some(lock) = RESIDENT_LEASES.get()
        && let Ok(mut guard) = lock.lock()
        && let Some(owner) = guard.get_mut(file_path)
        && owner == "pending"
    {
        *owner = agent_id.to_string();
    }
}
"""
    resident.write_text(rt, encoding="utf-8")

# prompt_text: pub(super) const
pt = SUBAGENT / "prompt_text.rs"
ptx = pt.read_text(encoding="utf-8")
if not ptx.startswith("#!"):
    ptx = PROMPT_TEXT_HEADER + ptx
ptx = re.sub(r"\nconst ([A-Z_]+_AGENT_PROMPT)", r"\npub(super) const \1", ptx)
pt.write_text(ptx, encoding="utf-8")

# runtime.rs extra imports after mailbox use
runtime = SUBAGENT / "runtime.rs"
rtx = runtime.read_text(encoding="utf-8")
extra = """
use super::constants::{DEFAULT_MAX_SPAWN_DEPTH, STEP_API_TIMEOUT};
use super::factory::SharedSubAgentManager;
use super::types::SubAgentInput;
"""
if "factory::SharedSubAgentManager" not in rtx:
    rtx = rtx.replace(
        "use super::types::SubAgentSpawnOptions;",
        extra + "\nuse super::types::SubAgentSpawnOptions;",
    )
    runtime.write_text(rtx, encoding="utf-8")

# types.rs constants import
types = SUBAGENT / "types.rs"
ty = types.read_text(encoding="utf-8")
if "SUBAGENT_STATE_SCHEMA_VERSION" in ty and "constants::" not in ty:
    ty = ty.replace(
        "use super::mailbox::{Mailbox, MailboxEnvelope, MailboxReceiver};",
        "use super::mailbox::{Mailbox, MailboxEnvelope, MailboxReceiver};\n\nuse super::constants::SUBAGENT_STATE_SCHEMA_VERSION;",
    )
    types.write_text(ty, encoding="utf-8")

# manager.rs imports
mgr = SUBAGENT / "manager.rs"
mt = mgr.read_text(encoding="utf-8")
repls = [
    (
        "use super::executor::run_subagent_task;",
        "use super::executor::{run_subagent_task, SubAgentTask};\nuse super::parse::normalize_role_alias;\nuse super::types::SubAgentInput;",
    ),
]
for a, b in repls:
    if b.split("\n")[0] not in mt:
        mt = mt.replace(a, b)
mgr.write_text(mt, encoding="utf-8")

# executor.rs SubAgentCompletion from runtime
exec_path = SUBAGENT / "executor.rs"
ex = exec_path.read_text(encoding="utf-8")
ex = ex.replace(
    "use super::types::{SubAgentCompletion, SubAgentInput, WaitMode};",
    "use super::runtime::SubAgentCompletion;\nuse super::types::{SubAgentInput, WaitMode};",
)
ex = ex.replace("struct SubAgentTask", "pub(super) struct SubAgentTask")
ex = ex.replace("async fn run_subagent_task", "pub(super) async fn run_subagent_task")
exec_path.write_text(ex, encoding="utf-8")

# spawn.rs fixes
spawn = SUBAGENT / "tools/spawn.rs"
st = spawn.read_text(encoding="utf-8")
st = st.replace(
    "use super::super::resident::{release_resident_file_lease, try_claim_resident_file_lease};",
    "use super::super::parse::configured_model_for_role_or_type;\nuse super::super::resident::{release_resident_file_lease, try_claim_resident_file_lease, upgrade_pending_resident_lease};",
)
st = re.sub(
    r"if let Some\(ref file_path\) = spawn_request\.resident_file\s*\n\s*&& let Some\(lock\) = RESIDENT_LEASES\.get\(\).*?\n\s*\{\n\s*\*owner = result\.agent_id\.clone\(\);\n\s*\}",
    "if let Some(ref file_path) = spawn_request.resident_file {\n            upgrade_pending_resident_lease(file_path, &result.agent_id);\n        }",
    st,
    flags=re.DOTALL,
)
spawn.write_text(st, encoding="utf-8")

# delegate.rs AgentSpawnTool
delegate = SUBAGENT / "tools/delegate.rs"
dt = delegate.read_text(encoding="utf-8")
if "AgentSpawnTool" in dt and "super::spawn" not in dt:
    dt = dt.replace(
        "use super::super::runtime::SubAgentRuntime;",
        "use super::super::runtime::SubAgentRuntime;\nuse super::spawn::AgentSpawnTool;",
    )
    delegate.write_text(dt, encoding="utf-8")

# list.rs COMPLETED_AGENT_RETENTION
list_f = SUBAGENT / "tools/list.rs"
lt = list_f.read_text(encoding="utf-8")
if "COMPLETED_AGENT_RETENTION" in lt and "constants::" not in lt:
    lt = lt.replace(
        "use super::super::runtime::SubAgentRuntime;",
        "use super::super::constants::COMPLETED_AGENT_RETENTION;\nuse super::super::runtime::SubAgentRuntime;",
    )
    list_f.write_text(lt, encoding="utf-8")

print("second-pass done")
