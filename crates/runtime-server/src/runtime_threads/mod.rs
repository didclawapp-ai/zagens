//! Durable thread/turn/item runtime for the HTTP API and background tasks.
//!
//! This module keeps DeepSeek-only execution while exposing Codex-like lifecycle
//! semantics (threads, turns, items, interrupt/steer, and replayable events).

use std::collections::HashMap;
use std::fs::{self};
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::config::DEFAULT_TEXT_MODEL;
use crate::context_snapshot::{ThreadContextSnapshot, build_thread_context_snapshot};
use crate::core::coherence::CoherenceState;
use crate::models::{ContentBlock, Message, SystemPrompt};

pub(crate) const EVENT_CHANNEL_CAPACITY: usize = 4096;
const MAX_ACTIVE_THREADS_DEFAULT: usize = 8;
pub(crate) const SUMMARY_LIMIT: usize = 280;
/// Bumped to 2 for v0.6.6 — see issue #124. The persisted thread/turn/item
/// records didn't change shape, but the live engine semantics did: cycle
/// boundaries advance the `Session.cycle_count` and produce archived JSONL
/// files at `~/.deepseek/sessions/<id>/cycles/<n>.jsonl`. A v1 reader on a
/// session written by v2 wouldn't know about the cycle archive directory and
/// might misinterpret message counts; bumping is the safe choice.
const CURRENT_RUNTIME_SCHEMA_VERSION: u32 = 2;

/// Stable SSE / HTTP compat surface version (`API_DESIGN.md` §3.2.1).
/// Persisted `RuntimeEventRecord.schema_version` uses the same numeric value.
pub const CURRENT_EVENT_SCHEMA_VERSION: u32 = CURRENT_RUNTIME_SCHEMA_VERSION;
pub(crate) const RUNTIME_RESTART_REASON: &str = "Interrupted by process restart";

mod active;
mod engine_load;
mod events;
pub(crate) mod event_coalesce;
mod manager;
mod turn_lifecycle;
mod turn_control;
mod thread_crud;
mod monitor;
mod persist;
mod routing;
mod thread_message_turn_port;
mod turn_wait;
mod types;

pub use events::{collect_agent_rebind_hints, AgentRebindHint};
pub use manager::{RuntimeThreadManager, SharedRuntimeThreadManager};
pub use types::*;

#[cfg(test)]
pub(crate) use events::AgentRebindStatus;
#[cfg(test)]
pub(crate) use persist::RuntimeThreadStore;
#[cfg(test)]
pub(crate) use thread_message_turn_port::RuntimeThreadMessageTurnPort;

#[cfg(test)]
pub(crate) use active::{
    enforce_lru_capacity, touch_lru, ActiveThreadState, ActiveThreads, ActiveTurnState,
    RuntimeApprovalDecision,
};
pub(crate) use manager::tool_kind_for_name;
#[cfg(test)]
pub(crate) use manager::parse_mode;

#[derive(Debug, Clone)]
pub struct RuntimeThreadManagerConfig {
    pub data_dir: PathBuf,
    pub task_data_dir: PathBuf,
    pub max_active_threads: usize,
    /// HTTP/API clients must call `resolve-approval` before this elapses, or the
    /// tool is auto-denied (same default as historical TUI behavior: 120s).
    /// Override with `DEEPSEEK_RUNTIME_APPROVAL_TIMEOUT_SECS`.
    pub http_approval_timeout_secs: u64,
}

impl RuntimeThreadManagerConfig {
    #[must_use]
    pub fn from_task_data_dir(task_data_dir: PathBuf) -> Self {
        let data_dir = if let Ok(override_dir) = std::env::var("DEEPSEEK_RUNTIME_DIR") {
            if override_dir.trim().is_empty() {
                task_data_dir.join("runtime")
            } else {
                PathBuf::from(override_dir)
            }
        } else {
            task_data_dir.join("runtime")
        };
        let http_approval_timeout_secs = std::env::var("DEEPSEEK_RUNTIME_APPROVAL_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .filter(|&s| s > 0)
            .unwrap_or(120);

        Self {
            data_dir,
            task_data_dir,
            max_active_threads: MAX_ACTIVE_THREADS_DEFAULT,
            http_approval_timeout_secs,
        }
    }
}

/// Visibility filter for `list_threads`. Default is `ActiveOnly`. The runtime
/// API exposes this as the combination of `include_archived` and
/// `archived_only` query params (see `runtime_api.rs`); whalescale#260 / #563.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ThreadListFilter {
    /// Only `archived = false` threads. The original default.
    #[default]
    ActiveOnly,
    /// Active and archived threads, sorted as the store returns them.
    IncludeArchived,
    /// Only `archived = true` threads.
    ArchivedOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, JsonSchema)]
pub struct CreateThreadRequest {
    pub model: Option<String>,
    #[schemars(schema_with = "crate::json_schema_util::path_as_string")]
    pub workspace: Option<PathBuf>,
    pub mode: Option<String>,
    pub allow_shell: Option<bool>,
    pub trust_mode: Option<bool>,
    pub auto_approve: Option<bool>,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub system_prompt: Option<String>,
    #[serde(default)]
    pub task_id: Option<String>,
    /// `auto` | `office` | `code` — resolved to `office`/`code` on create.
    #[serde(default)]
    pub task_type: Option<String>,
}

/// Mutable fields accepted by `PATCH /v1/threads/{id}`.
///
/// Each field is optional — missing means "no change". Extended in v0.8.10
/// (#562, whalescale#256) so the UI can flip persistent thread state without
/// having to recreate a thread or pass per-turn overrides on every send.
#[derive(Debug, Clone, Serialize, Deserialize, Default, JsonSchema)]
pub struct UpdateThreadRequest {
    pub archived: Option<bool>,
    pub allow_shell: Option<bool>,
    pub trust_mode: Option<bool>,
    pub auto_approve: Option<bool>,
    pub model: Option<String>,
    pub mode: Option<String>,
    pub title: Option<String>,
    pub system_prompt: Option<String>,
    /// When set, rebind tool workspace for this thread. Path must exist and be a
    /// directory. Relative paths resolve against the runtime API default workspace.
    /// Unloads cached engine entry when changed (disallowed while a turn is in progress).
    #[serde(default)]
    pub workspace: Option<String>,
    #[serde(default)]
    pub scratchpad_run_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RoutingRule {
    pub intent: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RoutingRulesDoc {
    pub rules: Vec<RoutingRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct StartTurnRequest {
    pub prompt: String,
    #[serde(default)]
    pub input_summary: Option<String>,
    pub model: Option<String>,
    pub mode: Option<String>,
    pub allow_shell: Option<bool>,
    pub trust_mode: Option<bool>,
    pub auto_approve: Option<bool>,
    #[serde(default)]
    pub route_intent: Option<String>,
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default)]
    pub top_p: Option<f32>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
}

impl Default for StartTurnRequest {
    fn default() -> Self {
        Self {
            prompt: String::new(),
            input_summary: None,
            model: None,
            mode: None,
            allow_shell: None,
            trust_mode: None,
            auto_approve: None,
            route_intent: None,
            temperature: None,
            top_p: None,
            max_tokens: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EditLastTurnRequest {
    pub content: String,
    pub model: Option<String>,
    pub mode: Option<String>,
    pub allow_shell: Option<bool>,
    pub trust_mode: Option<bool>,
    pub auto_approve: Option<bool>,
    #[serde(default)]
    pub route_intent: Option<String>,
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default)]
    pub top_p: Option<f32>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ForkAtUserMessageRequest {
    pub depth_from_tail: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ForkAtUserMessageResponse {
    pub thread: ThreadRecord,
    pub original_user_text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SteerTurnRequest {
    pub prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, JsonSchema)]
pub struct CompactThreadRequest {
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ThreadDetail {
    pub thread: ThreadRecord,
    pub turns: Vec<TurnRecord>,
    pub items: Vec<TurnItemRecord>,
    pub latest_seq: u64,
}

/// Aggregation key for `aggregate_usage`. Whalescale#261 / #564.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageGroupBy {
    Day,
    Model,
    Provider,
    Thread,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct UsageTotals {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_tokens: u64,
    pub reasoning_tokens: u64,
    pub cost_usd: f64,
    pub turns: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct UsageBucket {
    pub key: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_tokens: u64,
    pub reasoning_tokens: u64,
    pub cost_usd: f64,
    pub turns: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UsageAggregation {
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
    pub group_by: String,
    pub totals: UsageTotals,
    pub buckets: Vec<UsageBucket>,
}

/// Best-effort provider classification from a model name. Used as a grouping
/// key for `/v1/usage?group_by=provider`. Cost-tracking already runs the
/// model→pricing→cost path; this only labels the bucket.
pub(crate) fn provider_label_for_model(model: &str) -> &'static str {
    if model.starts_with("deepseek-ai/") {
        "nvidia-nim"
    } else if model.starts_with("deepseek-") {
        "deepseek"
    } else if model.starts_with("openai/") || model.starts_with("anthropic/") {
        "openrouter"
    } else {
        "unknown"
    }
}

pub fn summarize_text(text: &str, limit: usize) -> String {
    let take = limit.saturating_sub(3);
    let mut count = 0;
    let mut out = String::new();
    for ch in text.chars() {
        if count >= take {
            out.push_str("...");
            return out;
        }
        if ch.is_control() && ch != '\n' && ch != '\t' {
            continue;
        }
        out.push(ch);
        count += 1;
    }
    out
}

#[path = "tests.rs"]
#[cfg(test)]
mod tests;
