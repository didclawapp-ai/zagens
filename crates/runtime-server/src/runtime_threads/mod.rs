//! Durable thread/turn/item runtime for the HTTP API and background tasks.
//!
//! Persist types/store live in `deepseek-runtime-orchestrator` (D16 E1-b); this
//! module keeps live engine orchestration (`RuntimeThreadManager`, monitors, …).

pub use std::collections::HashMap;
pub use std::fs;
pub use std::path::PathBuf;
pub use std::sync::Arc;
pub use anyhow::{Context, Result, anyhow, bail};
pub use chrono::{DateTime, Utc};
pub use serde_json::{Value, json};
pub use uuid::Uuid;

pub use crate::config::DEFAULT_TEXT_MODEL;
pub use crate::context_snapshot::{ThreadContextSnapshot, build_thread_context_snapshot};
pub use crate::core::coherence::CoherenceState;
pub use crate::models::{ContentBlock, Message, SystemPrompt};

pub(crate) use deepseek_runtime_orchestrator::runtime_threads::persist;
pub use deepseek_runtime_orchestrator::runtime_threads::types;
pub use deepseek_runtime_orchestrator::runtime_threads::{
    summarize_text, CompactThreadRequest, CreateThreadRequest, EditLastTurnRequest,
    ForkAtUserMessageRequest, ForkAtUserMessageResponse, RoutingRule, RoutingRulesDoc,
    RuntimeThreadManagerConfig, RuntimeThreadStore, StartTurnRequest, SteerTurnRequest,
    ThreadDetail, ThreadListFilter, UpdateThreadRequest, UsageAggregation, UsageBucket,
    UsageGroupBy, UsageTotals, CURRENT_EVENT_SCHEMA_VERSION,
};
pub use deepseek_runtime_orchestrator::runtime_threads::types::*;

pub(crate) const EVENT_CHANNEL_CAPACITY: usize = 4096;
pub(crate) const SUMMARY_LIMIT: usize = 280;
pub(crate) const RUNTIME_RESTART_REASON: &str = "Interrupted by process restart";
pub(crate) use deepseek_runtime_orchestrator::runtime_threads::{
    provider_label_for_model, CURRENT_RUNTIME_SCHEMA_VERSION,
};

mod active;
mod engine_load;
mod events;
pub(crate) mod event_coalesce;
mod manager;
mod turn_lifecycle;
mod turn_control;
mod thread_crud;
mod monitor;
mod routing;
mod turn_wait;

pub use events::{collect_agent_rebind_hints, AgentRebindHint};
pub use manager::{RuntimeThreadManager, SharedRuntimeThreadManager};

#[cfg(test)]
pub(crate) use events::AgentRebindStatus;

#[cfg(test)]
pub(crate) use active::{
    enforce_lru_capacity, touch_lru, ActiveThreadState, ActiveThreads, ActiveTurnState,
    RuntimeApprovalDecision,
};
pub(crate) use manager::tool_kind_for_name;
#[cfg(test)]
pub(crate) use manager::parse_mode;

#[path = "tests.rs"]
#[cfg(test)]
mod tests;
