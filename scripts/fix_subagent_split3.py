#!/usr/bin/env python3
"""Third-pass: visibility + test exports + tool imports."""
from __future__ import annotations

import pathlib
import re

SUBAGENT = pathlib.Path(__file__).resolve().parents[1] / "crates/runtime-server/src/tools/subagent"

# runtime: DEFAULT_MAX_SPAWN_DEPTH lives in types
runtime = SUBAGENT / "runtime.rs"
rtx = runtime.read_text(encoding="utf-8")
rtx = rtx.replace(
    "use super::constants::{DEFAULT_MAX_SPAWN_DEPTH, STEP_API_TIMEOUT};",
    "use super::constants::STEP_API_TIMEOUT;\nuse super::types::DEFAULT_MAX_SPAWN_DEPTH;",
)
runtime.write_text(rtx, encoding="utf-8")

# manager: pub(crate) agents + pub(super) update methods
mgr = SUBAGENT / "manager.rs"
mt = mgr.read_text(encoding="utf-8")
mt = mt.replace("    agents: HashMap<String, SubAgent>,", "    pub(crate) agents: HashMap<String, SubAgent>,")
mt = mt.replace("    fn update_from_result", "    pub(super) fn update_from_result")
mt = mt.replace("    fn update_failed", "    pub(super) fn update_failed")
mgr.write_text(mt, encoding="utf-8")

# registry: pub(super) on impl methods + new
reg = SUBAGENT / "registry.rs"
rt = reg.read_text(encoding="utf-8")
for old in [
    "    fn new(",
    "    fn is_tool_allowed",
    "    fn tools_for_model",
    "    fn unavailable_allowed_tools",
    "    async fn execute",
]:
    rt = rt.replace(old, "    pub(super) " + old.strip())
reg.write_text(rt, encoding="utf-8")

# executor: pub(super) wait fns
ex = SUBAGENT / "executor.rs"
et = ex.read_text(encoding="utf-8")
et = et.replace("async fn wait_for_result", "pub(super) async fn wait_for_result")
et = et.replace("async fn wait_for_agents", "pub(super) async fn wait_for_agents")
(SUBAGENT / "executor.rs").write_text(et, encoding="utf-8")

# factory: pub(crate) default_state_path for tests
fac = SUBAGENT / "factory.rs"
ft = fac.read_text(encoding="utf-8")
ft = ft.replace("pub(super) fn default_state_path", "pub(crate) fn default_state_path")
fac.write_text(ft, encoding="utf-8")

# prompts/registry test exports via pub(crate) on functions
prompts = SUBAGENT / "prompts.rs"
pt = prompts.read_text(encoding="utf-8")
pt = pt.replace("pub(super) fn parse_structured_verdict", "pub(crate) fn parse_structured_verdict")
prompts.write_text(pt, encoding="utf-8")

reg = SUBAGENT / "registry.rs"
rt = reg.read_text(encoding="utf-8")
rt = rt.replace("pub(super) fn build_allowed_tools", "pub(crate) fn build_allowed_tools")
reg.write_text(rt, encoding="utf-8")

# mod.rs test re-exports
mod = SUBAGENT / "mod.rs"
md = mod.read_text(encoding="utf-8")
if "#[cfg(test)]" not in md or "wait_for_result" not in md:
    md += """

#[cfg(test)]
pub(crate) use executor::wait_for_result;
#[cfg(test)]
pub(crate) use factory::default_state_path;
#[cfg(test)]
pub(crate) use prompts::parse_structured_verdict;
#[cfg(test)]
pub(crate) use registry::build_allowed_tools;
"""
    mod.write_text(md, encoding="utf-8")

# Fix ToolContext in all tool files
for tool in (SUBAGENT / "tools").glob("*.rs"):
    if tool.name == "mod.rs":
        continue
    text = tool.read_text(encoding="utf-8")
    if "ToolContext" not in text:
        text = text.replace(
            "ToolCapability, ToolError",
            "ToolCapability, ToolContext, ToolError",
        )
    if "Duration::" in text and "use std::time::Duration" not in text:
        text = "use std::time::Duration;\n" + text
    if "HashMap" in text and "use std::collections::HashMap" not in text:
        text = "use std::collections::HashMap;\n" + text
    tool.write_text(text, encoding="utf-8")

print("third-pass done")
