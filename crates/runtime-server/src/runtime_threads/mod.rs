#![cfg_attr(test, allow(unused_imports))]

//! Durable thread/turn/item runtime for the HTTP API and background tasks.
//!
//! Persist types/store live in `zagens-runtime-orchestrator` (D16 E1-b); this
//! module keeps live engine orchestration (`RuntimeThreadManager`, monitors, …).

pub use chrono::Utc;
pub use uuid::Uuid;

pub use crate::config::DEFAULT_TEXT_MODEL;
pub use crate::context_snapshot::{ThreadContextSnapshot, build_thread_context_snapshot};
pub use crate::core::coherence::CoherenceState;
pub use crate::models::{Message, SystemPrompt};

pub(crate) use zagens_runtime_orchestrator::runtime_threads::persist;
pub use zagens_runtime_orchestrator::runtime_threads::types;
pub use zagens_runtime_orchestrator::runtime_threads::types::*;
pub use zagens_runtime_orchestrator::runtime_threads::{
    ChannelEventRequest, ChannelEventResponse, CompactThreadRequest, CreateThreadRequest,
    EditLastTurnRequest, ForkAtUserMessageRequest, ForkAtUserMessageResponse, RoutingRule,
    RuntimeThreadManagerConfig, RuntimeThreadStore, StartTurnOutcome, StartTurnRequest,
    SteerTurnRequest, ThreadDetail, ThreadListFilter, UpdateThreadRequest, UsageGroupBy,
};

pub(crate) use zagens_runtime_orchestrator::runtime_threads::CURRENT_RUNTIME_SCHEMA_VERSION;

/// Concrete engine host types wired by the sidecar (D16 E1-b phase 2).
pub(crate) type RuntimeEnginePolicy = crate::sandbox::SandboxPolicy;
pub(crate) type RuntimeUserInputResponse = crate::tools::user_input::UserInputResponse;

pub(crate) use zagens_runtime_orchestrator::runtime_threads::active::{
    ActiveThreadState as ActiveThreadStateInner, ActiveThreads as ActiveThreadsInner,
    RuntimeApprovalDecision,
};

pub(crate) type ActiveThreadState =
    ActiveThreadStateInner<RuntimeEnginePolicy, RuntimeUserInputResponse>;
pub(crate) type ActiveThreads = ActiveThreadsInner<RuntimeEnginePolicy, RuntimeUserInputResponse>;

#[cfg(test)]
pub(crate) use crate::models::ContentBlock;
#[cfg(test)]
pub(crate) use manager::parse_mode;
#[cfg(test)]
pub(crate) use zagens_runtime_orchestrator::runtime_threads::active::{
    ActiveTurnState, PendingApproval, enforce_lru_capacity, touch_lru,
};
#[cfg(test)]
pub use zagens_runtime_orchestrator::runtime_threads::manager::tool_kind_for_name;
#[cfg(test)]
pub(crate) use zagens_runtime_orchestrator::runtime_threads::manager::{
    EVENT_CHANNEL_CAPACITY, RUNTIME_RESTART_REASON,
};
#[cfg(test)]
pub(crate) use zagens_runtime_orchestrator::runtime_threads::provider_label_for_model;
#[cfg(test)]
pub use zagens_runtime_orchestrator::runtime_threads::routing;
#[cfg(test)]
pub(crate) use zagens_runtime_orchestrator::runtime_threads::thread_crud::SUMMARY_LIMIT;
#[cfg(test)]
pub(crate) use zagens_runtime_orchestrator::runtime_threads::{
    CURRENT_EVENT_SCHEMA_VERSION, RoutingRulesDoc, UsageAggregation, UsageBucket, UsageTotals,
    summarize_text,
};
#[cfg(test)]
pub(crate) use {
    anyhow::{Context, Result, anyhow, bail},
    chrono::DateTime,
    serde_json::{Value, json},
    std::collections::HashMap,
    std::fs,
    std::path::PathBuf,
    std::sync::Arc,
};

mod background_slots;
mod engine_host;
mod engine_spawn;
mod kernel_resume;
mod manager;
mod monitor_host;
mod task_port;
mod thread_crud;
mod worktree_bind;
pub(crate) use worktree_bind::maybe_allocate_craft_worktree;
mod turn_control;
mod turn_lifecycle;
mod turn_wait;

pub use manager::{RuntimeThreadManager, SharedRuntimeThreadManager};
pub use zagens_runtime_orchestrator::runtime_threads::event_coalesce;
pub use zagens_runtime_orchestrator::runtime_threads::events::{
    AgentRebindHint, collect_agent_rebind_hints,
};

#[cfg(test)]
pub(crate) use zagens_runtime_orchestrator::runtime_threads::events::AgentRebindStatus;

#[path = "tests.rs"]
#[cfg(test)]
mod tests;
