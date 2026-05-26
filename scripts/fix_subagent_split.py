#!/usr/bin/env python3
"""Post-process split subagent modules: fix tool preludes and visibility."""
from __future__ import annotations

import pathlib
import re

SUBAGENT = pathlib.Path(__file__).resolve().parents[1] / "crates/runtime-server/src/tools/subagent"

TOOL_PRELUDE = """
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{Value, json};

use deepseek_core::subagent::{SubAgentAssignment, SubAgentResult, SubAgentStatus, SubAgentType};
use crate::tools::spec::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec,
    optional_bool, optional_u64, required_str,
};
""".strip()

ROOT_PRELUDE_END = "use super::mailbox::{Mailbox, MailboxEnvelope, MailboxReceiver};"

# Strip bloated prelude from tools/*.rs
PRELUDE_START = "use std::collections::{HashMap, VecDeque};"
PRELUDE_RE = re.compile(
    re.escape(PRELUDE_START) + r".*?" + re.escape(ROOT_PRELUDE_END) + r"\n\n",
    re.DOTALL,
)

for tool in (SUBAGENT / "tools").glob("*.rs"):
    if tool.name == "mod.rs":
        continue
    text = tool.read_text(encoding="utf-8")
    text = PRELUDE_RE.sub(TOOL_PRELUDE + "\n\n", text, count=1)
    tool.write_text(text, encoding="utf-8")
    print(f"fixed prelude: {tool.name}")

# router: allow re-export
router = SUBAGENT / "router.rs"
rt = router.read_text(encoding="utf-8")
rt = rt.replace(
    "pub(crate) async fn resolve_subagent_assignment_route",
    "pub async fn resolve_subagent_assignment_route",
)
router.write_text(rt, encoding="utf-8")

# prompts: craft path
prompts = SUBAGENT / "prompts.rs"
pt = prompts.read_text(encoding="utf-8")
pt = pt.replace("craft::", "super::craft::")
prompts.write_text(pt, encoding="utf-8")

# send: SubAgentInput
send = SUBAGENT / "tools/send.rs"
st = send.read_text(encoding="utf-8")
if "SubAgentInput" in st and "types::SubAgentInput" not in st:
    st = st.replace(
        "use super::super::types::SubAgentInput;",
        "use super::super::types::SubAgentInput;",
    )
    if "use super::super::types::SubAgentInput" not in st:
        st = st.replace(
            "use super::super::runtime::SubAgentRuntime;",
            "use super::super::runtime::SubAgentRuntime;\nuse super::super::types::SubAgentInput;",
        )
    send.write_text(st, encoding="utf-8")

# executor: SubAgentCompletion
exec_path = SUBAGENT / "executor.rs"
ex = exec_path.read_text(encoding="utf-8")
if "SubAgentCompletion" in ex and "types::SubAgentCompletion" not in ex:
    ex = ex.replace(
        "use super::factory::SharedSubAgentManager;",
        "use super::factory::SharedSubAgentManager;\nuse super::types::SubAgentCompletion;",
    )
    exec_path.write_text(ex, encoding="utf-8")

# manager: SubAgent from runtime
mgr = SUBAGENT / "manager.rs"
mt = mgr.read_text(encoding="utf-8")
if "SubAgent" in mt and "runtime::SubAgent" not in mt:
    mt = mt.replace(
        "use super::executor::run_subagent_task;",
        "use super::executor::run_subagent_task;\nuse super::runtime::{SubAgent, SubAgentRuntime};",
    )
    mgr.write_text(mt, encoding="utf-8")

print("post-process done")
