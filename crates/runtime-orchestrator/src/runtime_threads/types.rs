//! Durable thread/turn/item data types — shared across the runtime threads module.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use deepseek_core::coherence::CoherenceState;
use crate::models::Usage;

use super::CURRENT_RUNTIME_SCHEMA_VERSION;

const fn default_runtime_schema_version() -> u32 {
    CURRENT_RUNTIME_SCHEMA_VERSION
}

pub fn default_thread_task_type() -> String {
    deepseek_core::task_type::TaskType::Code.as_str().to_string()
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeTurnStatus {
    Queued,
    InProgress,
    Completed,
    Failed,
    Interrupted,
    Canceled,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TurnItemKind {
    UserMessage,
    AgentMessage,
    ToolCall,
    FileChange,
    CommandExecution,
    ContextCompaction,
    Status,
    Error,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TurnItemLifecycleStatus {
    Queued,
    InProgress,
    Completed,
    Failed,
    Interrupted,
    Canceled,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ThreadRecord {
    #[serde(default = "default_runtime_schema_version")]
    pub schema_version: u32,
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub model: String,
    #[schemars(schema_with = "deepseek_runtime_adapters::json_schema_util::path_as_string")]
    pub workspace: PathBuf,
    pub mode: String,
    pub allow_shell: bool,
    pub trust_mode: bool,
    pub auto_approve: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_turn_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_response_bookmark: Option<String>,
    #[serde(default)]
    pub archived: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default = "default_thread_task_type")]
    pub task_type: String,
    #[serde(default)]
    pub coherence_state: CoherenceState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scratchpad_run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checklist_snapshot: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TurnRecord {
    #[serde(default = "default_runtime_schema_version")]
    pub schema_version: u32,
    pub id: String,
    pub thread_id: String,
    pub status: RuntimeTurnStatus,
    pub input_summary: String,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_request_input_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default)]
    pub item_ids: Vec<String>,
    #[serde(default)]
    pub steer_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TurnItemRecord {
    #[serde(default = "default_runtime_schema_version")]
    pub schema_version: u32,
    pub id: String,
    pub turn_id: String,
    pub kind: TurnItemKind,
    pub status: TurnItemLifecycleStatus,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
    #[serde(default)]
    #[schemars(schema_with = "deepseek_runtime_adapters::json_schema_util::path_vec_as_strings")]
    pub artifact_refs: Vec<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeEventRecord {
    #[serde(default = "default_runtime_schema_version")]
    pub schema_version: u32,
    pub seq: u64,
    pub timestamp: DateTime<Utc>,
    pub thread_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_id: Option<String>,
    pub event: String,
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeStoreState {
    #[serde(default = "default_runtime_schema_version")]
    pub schema_version: u32,
    pub next_seq: u64,
}

impl Default for RuntimeStoreState {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_RUNTIME_SCHEMA_VERSION,
            next_seq: 1,
        }
    }
}
