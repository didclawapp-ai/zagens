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
mod structured_fallback;
mod wait_timeout;

pub use deepseek_core::subagent::{
    SubAgentResult, SubAgentStatus, SubAgentType, VerdictLevel,
};
#[cfg(test)]
pub(crate) use deepseek_core::subagent::{
    MailboxMessage, StructuredVerdict, SubAgentAssignment,
};
#[cfg(test)]
pub use deepseek_core::subagent::VerdictItem;
#[allow(unused_imports)]
pub use mailbox::{Mailbox, MailboxEnvelope, MailboxReceiver};

pub use factory::{SharedSubAgentManager, new_shared_subagent_manager};
pub(crate) use executor::wait_for_result;
#[cfg(test)]
#[allow(deprecated)]
pub(crate) use prompts::{subagent_allowed_tools, subagent_system_prompt};
pub use router::resolve_subagent_assignment_route;
pub use runtime::{SubAgentCompletion, SubAgentRuntime};
pub use types::DEFAULT_MAX_SPAWN_DEPTH;

pub use tools::{
    AgentAssignTool, AgentCancelTool, AgentCloseTool, AgentListTool, AgentResultTool,
    AgentResumeTool, AgentSendInputTool, AgentSpawnTool, AgentWaitTool, DelegateToAgentTool,
};

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) use constants::{
    DEPRECATION_REMOVAL_VERSION, STEP_API_TIMEOUT, SUBAGENT_RESTART_REASON, SUBAGENT_STATE_FILE,
    SUBAGENT_STATE_SCHEMA_VERSION,
};
#[cfg(test)]
pub(crate) use deprecation::wrap_with_deprecation_notice;
#[cfg(test)]
pub(crate) use executor::{
    completion_reason_for_error_str, completion_reason_for_successful_exit,
    emit_parent_completion, subagent_done_sentinel, subagent_failed_sentinel,
};
#[cfg(test)]
pub(crate) use constants::{adaptive_wait_timeout_ms, step_tool_budget, DEFAULT_MAX_STEPS, MAX_RESULT_TIMEOUT_MS, MIN_WAIT_TIMEOUT_MS};
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
#[cfg(test)]
pub(crate) use types::WaitMode;

