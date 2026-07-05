//! Night queue task model (Phase 1a).

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const QUEUE_SCHEMA_VERSION: u32 = 1;
pub const QUEUE_FILE: &str = "night_queue.json";
pub const BRIEFING_MARKER: &str = "<!-- night-queue:briefing -->";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueueTaskStatus {
    Pending,
    Running,
    Passed,
    Failed,
    RolledBack,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatePredicateSpec {
    pub predicate: String,
    #[serde(default)]
    pub args: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueTask {
    pub id: String,
    pub prompt: String,
    pub status: QueueTaskStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree_path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gate: Vec<GatePredicateSpec>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NightQueueDocument {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub tasks: Vec<QueueTask>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_run_at: Option<DateTime<Utc>>,
}

fn default_schema_version() -> u32 {
    QUEUE_SCHEMA_VERSION
}

impl Default for NightQueueDocument {
    fn default() -> Self {
        Self {
            schema_version: QUEUE_SCHEMA_VERSION,
            tasks: Vec::new(),
            last_run_at: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueEventRecord {
    pub kind: String,
    pub ts: DateTime<Utc>,
    pub task_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<Value>,
}
