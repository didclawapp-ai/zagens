#!/usr/bin/env python3
"""Make subagent internals visible to integration tests (child of mod.rs)."""
from __future__ import annotations

import pathlib
import re

SUBAGENT = pathlib.Path(__file__).resolve().parents[1] / "crates/runtime-server/src/tools/subagent"

# pub(super) -> pub(crate) for fns/types tests need via super::*
for rel in [
    "parse.rs",
    "router.rs",
    "registry.rs",
    "executor.rs",
    "deprecation.rs",
    "resident.rs",
    "prompts.rs",
    "constants.rs",
]:
    p = SUBAGENT / rel
    text = p.read_text(encoding="utf-8")
    text = text.replace("pub(super) fn ", "pub(crate) fn ")
    text = text.replace("pub(super) async fn ", "pub(crate) async fn ")
    text = text.replace("pub(super) struct SubAgentToolRegistry", "pub(crate) struct SubAgentToolRegistry")
    text = text.replace("pub(super) const ", "pub(crate) const ")
    p.write_text(text, encoding="utf-8")

mod = SUBAGENT / "mod.rs"
md = mod.read_text(encoding="utf-8")
# Replace old test exports block
test_exports = '''
#[cfg(test)]
pub(crate) use constants::{
    DEPRECATION_REMOVAL_VERSION, STEP_API_TIMEOUT, SUBAGENT_RESTART_REASON, SUBAGENT_STATE_FILE,
    SUBAGENT_STATE_SCHEMA_VERSION,
};
#[cfg(test)]
pub(crate) use deprecation::wrap_with_deprecation_notice;
#[cfg(test)]
pub(crate) use executor::{
    emit_parent_completion, subagent_done_sentinel, subagent_failed_sentinel, wait_for_result,
};
#[cfg(test)]
pub(crate) use factory::default_state_path;
#[cfg(test)]
pub(crate) use manager::SubAgentManager;
#[cfg(test)]
pub(crate) use parse::{
    build_assignment_prompt, configured_model_for_role_or_type, normalize_requested_subagent_model,
    parse_assign_request, parse_spawn_request, parse_wait_ids, parse_wait_mode,
};
#[cfg(test)]
pub(crate) use prompts::{build_subagent_system_prompt, parse_structured_verdict};
#[cfg(test)]
pub(crate) use registry::{
    SubAgentToolRegistry, build_allowed_tools, subagent_status_name, summarize_subagent_result,
};
#[cfg(test)]
pub(crate) use resident::{
    release_resident_file_lease, release_resident_leases_for, try_claim_resident_file_lease,
};
#[cfg(test)]
pub(crate) use router::{fallback_subagent_assignment_route, subagent_router_prompt};
#[cfg(test)]
pub(crate) use runtime::SubAgent;
'''
if "build_assignment_prompt" not in md:
    md = md.rstrip() + test_exports + "\n"
else:
    md = re.sub(r"\n#\[cfg\(test\)\]\npub\(crate\) use executor::wait_for_result;.*", test_exports, md, flags=re.DOTALL)

mod.write_text(md, encoding="utf-8")
print("test visibility done")
