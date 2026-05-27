//! Durable thread/turn/item types and disk store (D16 E1-b).

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub mod persist;
pub mod types;

pub use persist::RuntimeThreadStore;
pub use types::*;

pub const CURRENT_RUNTIME_SCHEMA_VERSION: u32 = 2;

/// Stable SSE / HTTP compat surface version (`API_DESIGN.md` §3.2.1).
pub const CURRENT_EVENT_SCHEMA_VERSION: u32 = CURRENT_RUNTIME_SCHEMA_VERSION;

const MAX_ACTIVE_THREADS_DEFAULT: usize = 8;

#[derive(Debug, Clone)]
pub struct RuntimeThreadManagerConfig {
    pub data_dir: PathBuf,
    pub task_data_dir: PathBuf,
    pub max_active_threads: usize,
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ThreadListFilter {
    #[default]
    ActiveOnly,
    IncludeArchived,
    ArchivedOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, JsonSchema)]
pub struct CreateThreadRequest {
    pub model: Option<String>,
    #[schemars(schema_with = "deepseek_runtime_adapters::json_schema_util::path_as_string")]
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
    #[serde(default)]
    pub task_type: Option<String>,
}

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

pub fn provider_label_for_model(model: &str) -> &'static str {
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

#[must_use]
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
