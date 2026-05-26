#!/usr/bin/env python3
"""Split tools/subagent/mod.rs into maintainable modules (D16 E2)."""
from __future__ import annotations

import pathlib
import textwrap

ROOT = pathlib.Path(__file__).resolve().parents[1]
SUBAGENT = ROOT / "crates/runtime-server/src/tools/subagent"
SRC = SUBAGENT / "mod.rs"

# 1-indexed inclusive ranges
RANGES: dict[str, tuple[int, int]] = {
    "resident.rs": (53, 89),
    "constants.rs": (91, 183),
    "deprecation.rs": (185, 235),
    "types.rs": (371, 493),
    "runtime.rs": (495, 803),
    "manager.rs": (804, 1434),
    "factory.rs": (1436, 1480),
    "tools/spawn.rs": (1484, 1824),
    "tools/result.rs": (1826, 1914),
    "tools/cancel.rs": (1916, 1972),
    "tools/close.rs": (1979, 2047),
    "tools/resume.rs": (2049, 2111),
    "tools/list.rs": (1974, 1977),  # struct only; impl appended below
    "tools/send.rs": (2162, 2256),
    "tools/assign.rs": (2258, 2350),
    "tools/wait.rs": (2352, 2495),
    "tools/delegate.rs": (2497, 2593),
    "executor.rs": (2622, 3170),  # SubAgentTask..wait_for_agents (skip build_subagent block)
    "parse.rs": (3171, 3474),
    "router.rs": (3476, 3637),
    "registry.rs": (3743, 3945),
    "prompt_text.rs": (3947, 4282),
}

PRELUDE = textwrap.dedent(
    """
    use std::collections::{HashMap, VecDeque};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    use anyhow::{Result, anyhow};
    use async_trait::async_trait;
    use serde::{Deserialize, Serialize};
    use serde_json::{Value, json};
    use tokio::sync::{Mutex, RwLock, mpsc};
    use tokio::task::JoinHandle;
    use tokio_util::sync::CancellationToken;
    use uuid::Uuid;

    use crate::config::MAX_SUBAGENTS;
    use deepseek_core::events::Event;
    use crate::models::{ContentBlock, Message, MessageRequest, SystemPrompt, Tool};
    use crate::tools::plan::{PlanState, SharedPlanState};
    use crate::tools::registry::{ToolRegistry, ToolRegistryBuilder};
    use crate::tools::spec::{
        ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec,
        optional_bool, optional_u64, required_str,
    };
    use crate::tools::todo::{SharedTodoList, TodoList};
    use crate::utils::spawn_supervised;

    use super::blackboard::{read_blackboard_section, write_blackboard_partition};
    use deepseek_core::subagent::{
        MailboxMessage, StructuredVerdict, SubAgentAssignment, SubAgentResult, SubAgentStatus,
        SubAgentType, VerdictLevel,
    };
    use super::mailbox::{Mailbox, MailboxEnvelope, MailboxReceiver};
    """
).strip()

FILE_USES: dict[str, str] = {
    "types.rs": "",
    "runtime.rs": "use super::types::SubAgentSpawnOptions;\n",
    "manager.rs": textwrap.dedent(
        """
        use super::constants::*;
        use super::registry::build_allowed_tools;
        use super::resident::{release_resident_file_lease, release_resident_leases_for, try_claim_resident_file_lease};
        use super::executor::run_subagent_task;
        use super::factory::{epoch_millis_now, instant_from_duration, write_json_atomic};
        use super::types::{PersistedSubAgent, PersistedSubAgentState, SubAgentSpawnOptions, SpawnRequest};
        """
    ).strip(),
    "factory.rs": "use super::manager::SubAgentManager;\nuse super::constants::{SUBAGENT_STATE_FILE};\n",
    "executor.rs": textwrap.dedent(
        """
        use super::constants::*;
        use super::prompts::{build_subagent_system_prompt, parse_structured_verdict};
        use super::registry::{SubAgentToolRegistry, summarize_subagent_result};
        use super::resident::release_resident_leases_for;
        use super::runtime::{SubAgent, SubAgentRuntime};
        use super::factory::SharedSubAgentManager;
        """
    ).strip(),
    "parse.rs": "use super::constants::VALID_SUBAGENT_TYPES;\nuse super::types::{AssignRequest, SpawnRequest, WaitMode};\n",
    "router.rs": "use super::runtime::SubAgentRuntime;\n",
    "registry.rs": "use super::runtime::SubAgentRuntime;\n",
    "prompts.rs": "use super::prompt_text::*;\nuse super::craft;\n",
    "tools/spawn.rs": textwrap.dedent(
        """
        use super::super::constants::*;
        use super::super::deprecation::wrap_with_deprecation_notice;
        use super::super::factory::SharedSubAgentManager;
        use super::super::parse::parse_spawn_request;
        use super::super::registry::summarize_subagent_result;
        use super::super::resident::{release_resident_file_lease, try_claim_resident_file_lease};
        use super::super::router::resolve_subagent_assignment_route;
        use super::super::runtime::SubAgentRuntime;
        use super::super::types::SubAgentSpawnOptions;
        use super::super::constants::whale_nickname_for_index;
        """
    ).strip(),
    "tools/wait.rs": textwrap.dedent(
        """
        use super::super::constants::*;
        use super::super::deprecation::wrap_with_deprecation_notice;
        use super::super::executor::{wait_for_agents, wait_for_result};
        use super::super::factory::SharedSubAgentManager;
        use super::super::parse::{parse_wait_ids, parse_wait_mode};
        use super::super::registry::{subagent_status_name, summarize_subagent_result};
        use super::super::runtime::SubAgentRuntime;
        """
    ).strip(),
    "tools/result.rs": textwrap.dedent(
        """
        use super::super::constants::*;
        use super::super::deprecation::wrap_with_deprecation_notice;
        use super::super::executor::wait_for_result;
        use super::super::factory::SharedSubAgentManager;
        use super::super::registry::summarize_subagent_result;
        use super::super::runtime::SubAgentRuntime;
        """
    ).strip(),
    "tools/cancel.rs": textwrap.dedent(
        """
        use super::super::deprecation::wrap_with_deprecation_notice;
        use super::super::factory::SharedSubAgentManager;
        use super::super::runtime::SubAgentRuntime;
        """
    ).strip(),
    "tools/list.rs": textwrap.dedent(
        """
        use super::super::deprecation::wrap_with_deprecation_notice;
        use super::super::factory::SharedSubAgentManager;
        use super::super::registry::{subagent_status_name, summarize_subagent_result};
        use super::super::runtime::SubAgentRuntime;
        """
    ).strip(),
    "tools/close.rs": textwrap.dedent(
        """
        use super::super::deprecation::wrap_with_deprecation_notice;
        use super::super::factory::SharedSubAgentManager;
        use super::super::runtime::SubAgentRuntime;
        """
    ).strip(),
    "tools/resume.rs": textwrap.dedent(
        """
        use super::super::deprecation::wrap_with_deprecation_notice;
        use super::super::factory::SharedSubAgentManager;
        use super::super::runtime::SubAgentRuntime;
        """
    ).strip(),
    "tools/send.rs": textwrap.dedent(
        """
        use super::super::deprecation::wrap_with_deprecation_notice;
        use super::super::factory::SharedSubAgentManager;
        use super::super::parse::{optional_input_str, parse_text_or_items};
        use super::super::runtime::SubAgentRuntime;
        use super::super::types::SubAgentInput;
        """
    ).strip(),
    "tools/assign.rs": textwrap.dedent(
        """
        use super::super::deprecation::wrap_with_deprecation_notice;
        use super::super::factory::SharedSubAgentManager;
        use super::super::parse::parse_assign_request;
        use super::super::runtime::SubAgentRuntime;
        """
    ).strip(),
    "tools/delegate.rs": textwrap.dedent(
        """
        use super::super::constants::DEFAULT_RESULT_TIMEOUT_MS;
        use super::super::deprecation::wrap_with_deprecation_notice;
        use super::super::executor::wait_for_result;
        use super::super::factory::SharedSubAgentManager;
        use super::super::parse::parse_spawn_request;
        use super::super::registry::summarize_subagent_result;
        use super::super::runtime::SubAgentRuntime;
        use super::super::types::SubAgentSpawnOptions;
        """
    ).strip(),
}

VISIBILITY: dict[str, list[tuple[str, str]]] = {
    "resident.rs": [
        ("pub(crate) fn", "pub(super) fn"),
    ],
    "deprecation.rs": [("fn wrap_with_deprecation_notice", "pub(super) fn wrap_with_deprecation_notice")],
    "constants.rs": [
        ("const DEFAULT_MAX_STEPS", "pub(super) const DEFAULT_MAX_STEPS"),
        ("const TOOL_TIMEOUT", "pub(super) const TOOL_TIMEOUT"),
        ("const STEP_API_TIMEOUT", "pub(super) const STEP_API_TIMEOUT"),
        ("fn step_api_timeout_error", "pub(super) fn step_api_timeout_error"),
        ("const RESULT_POLL_INTERVAL", "pub(super) const RESULT_POLL_INTERVAL"),
        ("const DEFAULT_RESULT_TIMEOUT_MS", "pub(super) const DEFAULT_RESULT_TIMEOUT_MS"),
        ("const MIN_WAIT_TIMEOUT_MS", "pub(super) const MIN_WAIT_TIMEOUT_MS"),
        ("const MAX_RESULT_TIMEOUT_MS", "pub(super) const MAX_RESULT_TIMEOUT_MS"),
        ("const COMPLETED_AGENT_RETENTION", "pub(super) const COMPLETED_AGENT_RETENTION"),
        ("const SUBAGENT_STATE_SCHEMA_VERSION", "pub(super) const SUBAGENT_STATE_SCHEMA_VERSION"),
        ("const SUBAGENT_STATE_FILE", "pub(super) const SUBAGENT_STATE_FILE"),
        ("const SUBAGENT_RESTART_REASON", "pub(super) const SUBAGENT_RESTART_REASON"),
        ("const VALID_SUBAGENT_TYPES", "pub(super) const VALID_SUBAGENT_TYPES"),
        ("const DEPRECATION_REMOVAL_VERSION", "pub(super) const DEPRECATION_REMOVAL_VERSION"),
    ],
    "types.rs": [
        ("pub(crate) struct SubAgentSpawnOptions", "pub(super) struct SubAgentSpawnOptions"),
        ("enum WaitMode", "pub(super) enum WaitMode"),
        ("struct SubAgentInput", "pub(super) struct SubAgentInput"),
        ("struct SpawnRequest", "pub(super) struct SpawnRequest"),
        ("struct AssignRequest", "pub(super) struct AssignRequest"),
        ("struct PersistedSubAgent", "pub(super) struct PersistedSubAgent"),
        ("struct PersistedSubAgentState", "pub(super) struct PersistedSubAgentState"),
    ],
    "factory.rs": [
        ("fn default_state_path", "pub(super) fn default_state_path"),
        ("fn epoch_millis_now", "pub(super) fn epoch_millis_now"),
        ("fn instant_from_duration", "pub(super) fn instant_from_duration"),
        ("fn write_json_atomic", "pub(super) fn write_json_atomic"),
    ],
    "registry.rs": [
        ("fn build_allowed_tools", "pub(super) fn build_allowed_tools"),
        ("fn read_only_tool_cap", "pub(super) fn read_only_tool_cap"),
        ("fn summarize_subagent_result", "pub(super) fn summarize_subagent_result"),
        ("fn subagent_status_name", "pub(super) fn subagent_status_name"),
        ("fn truncate_preview", "pub(super) fn truncate_preview"),
        ("struct SubAgentToolRegistry", "pub(super) struct SubAgentToolRegistry"),
    ],
    "parse.rs": [
        ("fn parse_wait_mode", "pub(super) fn parse_wait_mode"),
        ("fn parse_wait_ids", "pub(super) fn parse_wait_ids"),
        ("fn optional_input_str", "pub(super) fn optional_input_str"),
        ("fn parse_text_or_items", "pub(super) fn parse_text_or_items"),
        ("fn parse_optional_text_or_items", "pub(super) fn parse_optional_text_or_items"),
        ("fn parse_items_text", "pub(super) fn parse_items_text"),
        ("fn parse_spawn_request", "pub(super) fn parse_spawn_request"),
        ("fn parse_optional_subagent_model", "pub(super) fn parse_optional_subagent_model"),
        ("fn parse_optional_cwd", "pub(super) fn parse_optional_cwd"),
        ("fn parse_assign_request", "pub(super) fn parse_assign_request"),
        ("fn normalize_role_alias", "pub(super) fn normalize_role_alias"),
        ("fn build_assignment_prompt", "pub(super) fn build_assignment_prompt"),
    ],
    "router.rs": [
        ("pub(crate) struct SubAgentResolvedRoute", "pub(super) struct SubAgentResolvedRoute"),
        ("fn fallback_subagent_assignment_route", "pub(super) fn fallback_subagent_assignment_route"),
        ("async fn subagent_flash_router", "pub(super) async fn subagent_flash_router"),
        ("fn subagent_router_prompt", "pub(super) fn subagent_router_prompt"),
        ("fn truncate_subagent_router_prompt", "pub(super) fn truncate_subagent_router_prompt"),
        ("fn message_response_text", "pub(super) fn message_response_text"),
    ],
    "executor.rs": [
        ("fn emit_agent_progress", "pub(super) fn emit_agent_progress"),
    ],
    "prompts.rs": [
        ("fn build_subagent_system_prompt", "pub(super) fn build_subagent_system_prompt"),
        ("fn parse_structured_verdict", "pub(super) fn parse_structured_verdict"),
    ],
}

NEW_MOD = textwrap.dedent(
    """
    //! Sub-agent spawning system.

    pub mod blackboard;
    pub mod craft;
    pub mod mailbox;

    mod constants;
    mod deprecation;
    mod executor;
    mod factory;
    mod manager;
    mod parse;
    mod prompt_text;
    mod prompts;
    mod registry;
    mod resident;
    mod router;
    mod runtime;
    mod tools;
    mod types;

    pub use deepseek_core::subagent::{
        MailboxMessage, StructuredVerdict, SubAgentAssignment, SubAgentResult, SubAgentStatus,
        SubAgentType, VerdictLevel,
    };
    #[cfg(test)]
    pub use deepseek_core::subagent::VerdictItem;
    #[allow(unused_imports)]
    pub use mailbox::{Mailbox, MailboxEnvelope, MailboxReceiver};

    pub use constants::{WHALE_NICKNAMES, whale_nickname_for_index};
    pub use factory::{SharedSubAgentManager, new_shared_subagent_manager};
    pub use prompts::{subagent_allowed_tools, subagent_system_prompt};
    pub use router::resolve_subagent_assignment_route;
    pub use runtime::SubAgentRuntime;
    pub use types::{SubAgentCompletion, DEFAULT_MAX_SPAWN_DEPTH};

    pub use tools::{
        AgentAssignTool, AgentCancelTool, AgentCloseTool, AgentListTool, AgentResultTool,
        AgentResumeTool, AgentSendInputTool, AgentSpawnTool, AgentWaitTool, DelegateToAgentTool,
    };

    #[cfg(test)]
    mod tests;
    """
).strip()


def slice_lines(all_lines: list[str], start: int, end: int) -> str:
    return "".join(all_lines[start - 1 : end])


def apply_visibility(text: str, rel: str) -> str:
    for old, new in VISIBILITY.get(rel, []):
        text = text.replace(old, new)
    return text


def write_file(rel: str, body: str, *, with_prelude: bool) -> None:
    path = SUBAGENT / rel
    path.parent.mkdir(parents=True, exist_ok=True)
    extra = FILE_USES.get(rel, "")
    parts = []
    if with_prelude:
        parts.append(PRELUDE)
    if extra:
        parts.append(extra)
    parts.append(body)
    path.write_text("\n\n".join(p for p in parts if p) + "\n", encoding="utf-8")


def main() -> None:
    all_lines = SRC.read_text(encoding="utf-8").splitlines(keepends=True)
    backup = SUBAGENT / "mod.rs.bak"
    if not backup.exists():
        backup.write_text("".join(all_lines), encoding="utf-8")

    for rel, (start, end) in RANGES.items():
        body = apply_visibility(slice_lines(all_lines, start, end), rel)
        with_prelude = not rel.startswith("prompt_text")
        write_file(rel, body, with_prelude=with_prelude)
        print(f"wrote {rel}")

    # list tool impl
    list_path = SUBAGENT / "tools/list.rs"
    list_body = list_path.read_text(encoding="utf-8")
    list_body += apply_visibility(slice_lines(all_lines, 2113, 2160), "tools/list.rs")
    list_path.write_text(list_body, encoding="utf-8")

    # prompts.rs: advisory fns + build + verdict parse
    prompts_body = (
        apply_visibility(slice_lines(all_lines, 241, 369), "prompts.rs")
        + apply_visibility(slice_lines(all_lines, 2606, 2620), "prompts.rs")
        + apply_visibility(slice_lines(all_lines, 4284, 4337), "prompts.rs")
    )
    write_file("prompts.rs", prompts_body, with_prelude=True)

    tools_mod = textwrap.dedent(
        """
        mod assign;
        mod cancel;
        mod close;
        mod delegate;
        mod list;
        mod result;
        mod resume;
        mod send;
        mod spawn;
        mod wait;

        pub use assign::AgentAssignTool;
        pub use cancel::AgentCancelTool;
        pub use close::AgentCloseTool;
        pub use delegate::DelegateToAgentTool;
        pub use list::AgentListTool;
        pub use result::AgentResultTool;
        pub use resume::AgentResumeTool;
        pub use send::AgentSendInputTool;
        pub use spawn::AgentSpawnTool;
        pub use wait::AgentWaitTool;
        """
    ).strip()
    (SUBAGENT / "tools/mod.rs").write_text(tools_mod + "\n", encoding="utf-8")

    (SUBAGENT / "mod.rs").write_text(NEW_MOD + "\n", encoding="utf-8")
    print("done")


if __name__ == "__main__":
    main()
