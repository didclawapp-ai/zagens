//! Night queue HTTP wire types (`/v1/night-queue/*`) — Phase 1a desktop enqueue.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueueTaskStatus {
    Pending,
    Running,
    Passed,
    Failed,
    RolledBack,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GatePredicateWire {
    pub predicate: String,
    #[serde(default)]
    pub args: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct QueueTaskWire {
    pub id: String,
    pub prompt: String,
    pub status: QueueTaskStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<String>")]
    pub worktree_path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gate: Vec<GatePredicateWire>,
    pub created_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pre_snapshot_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate_summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct NightQueueResponse {
    pub schema_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_run_at: Option<DateTime<Utc>>,
    pub tasks: Vec<QueueTaskWire>,
    #[schemars(with = "String")]
    pub queue_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct NewNightQueueTaskRequest {
    pub prompt: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gate: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<String>")]
    pub gate_file: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate_preset: Option<String>,
    #[serde(default = "default_use_worktree")]
    pub use_worktree: bool,
}

const fn default_use_worktree() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RunNightQueueRequest {
    #[serde(default = "default_max_parallel")]
    pub max_parallel: usize,
    #[serde(default = "default_use_worktree")]
    pub use_worktree: bool,
    #[serde(default = "default_write_briefing")]
    pub write_briefing: bool,
}

const fn default_max_parallel() -> usize {
    1
}

const fn default_write_briefing() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RunNightQueueResponse {
    pub ran: usize,
    pub passed: usize,
    pub failed: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct NightQueueBriefingRequest {
    #[serde(default = "default_write_handoff")]
    pub write_handoff: bool,
}

const fn default_write_handoff() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct NightQueueBriefingResponse {
    pub markdown: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<String>")]
    pub handoff_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GatePresetWire {
    pub id: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GatePresetsResponse {
    pub presets: Vec<GatePresetWire>,
}
