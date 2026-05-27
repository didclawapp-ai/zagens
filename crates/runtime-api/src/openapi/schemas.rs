//! HTTP wire types for OpenAPI `components.schemas` (D8).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::task::{TaskCounts, TaskRecord, TaskStatus, TaskSummary, TasksResponse};

use deepseek_core::coherence::CoherenceState;
use deepseek_core::models::{ServerToolUsage, Usage};
use deepseek_runtime_adapters::persist::SessionMetadata;
use deepseek_runtime_orchestrator::runtime_threads::{
    CreateThreadRequest, RoutingRule, RoutingRulesDoc, RuntimeTurnStatus, StartTurnRequest,
    SteerTurnRequest, ThreadDetail, ThreadRecord, TurnItemKind, TurnItemLifecycleStatus,
    TurnItemRecord, TurnRecord, UpdateThreadRequest, UsageAggregation, UsageBucket, UsageTotals,
};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SessionsListResponse {
    pub sessions: Vec<SessionMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SessionDetailResponse {
    pub metadata: SessionMetadata,
    pub messages: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ResumeSessionResponse {
    pub thread_id: String,
    pub session_id: String,
    pub message_count: usize,
    pub state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct StreamTurnRequest {
    pub prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_shell: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trust_mode: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_approve: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route_intent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ThreadSummary {
    pub id: String,
    pub title: String,
    pub preview: String,
    pub model: String,
    pub mode: String,
    pub archived: bool,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_turn_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_turn_status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct StartTurnResponse {
    pub thread: ThreadRecord,
    pub turn: TurnRecord,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ErrorBody {
    pub error: String,
}

/// Core schemas exported to OpenAPI components.
pub const SCHEMA_EXPORTS: &[(&str, fn() -> schemars::Schema)] = &[
    ("CoherenceState", || schemars::schema_for!(CoherenceState)),
    ("RuntimeTurnStatus", || schemars::schema_for!(RuntimeTurnStatus)),
    ("TurnItemKind", || schemars::schema_for!(TurnItemKind)),
    ("TurnItemLifecycleStatus", || schemars::schema_for!(TurnItemLifecycleStatus)),
    ("Usage", || schemars::schema_for!(Usage)),
    ("ServerToolUsage", || schemars::schema_for!(ServerToolUsage)),
    ("SessionMetadata", || schemars::schema_for!(SessionMetadata)),
    ("SessionsListResponse", || schemars::schema_for!(SessionsListResponse)),
    ("SessionDetailResponse", || schemars::schema_for!(SessionDetailResponse)),
    ("ResumeSessionResponse", || schemars::schema_for!(ResumeSessionResponse)),
    ("ThreadRecord", || schemars::schema_for!(ThreadRecord)),
    ("TurnItemRecord", || schemars::schema_for!(TurnItemRecord)),
    ("ThreadDetail", || schemars::schema_for!(ThreadDetail)),
    ("ThreadSummary", || schemars::schema_for!(ThreadSummary)),
    ("TurnRecord", || schemars::schema_for!(TurnRecord)),
    ("CreateThreadRequest", || schemars::schema_for!(CreateThreadRequest)),
    ("UpdateThreadRequest", || schemars::schema_for!(UpdateThreadRequest)),
    ("StartTurnRequest", || schemars::schema_for!(StartTurnRequest)),
    ("SteerTurnRequest", || schemars::schema_for!(SteerTurnRequest)),
    ("StartTurnResponse", || schemars::schema_for!(StartTurnResponse)),
    ("StreamTurnRequest", || schemars::schema_for!(StreamTurnRequest)),
    ("RoutingRule", || schemars::schema_for!(RoutingRule)),
    ("RoutingRulesDoc", || schemars::schema_for!(RoutingRulesDoc)),
    ("UsageTotals", || schemars::schema_for!(UsageTotals)),
    ("UsageBucket", || schemars::schema_for!(UsageBucket)),
    ("UsageAggregation", || schemars::schema_for!(UsageAggregation)),
    ("ErrorBody", || schemars::schema_for!(ErrorBody)),
    ("TaskRecord", || schemars::schema_for!(TaskRecord)),
    ("TaskSummary", || schemars::schema_for!(TaskSummary)),
    ("TaskCounts", || schemars::schema_for!(TaskCounts)),
    ("TasksResponse", || schemars::schema_for!(TasksResponse)),
    ("TaskStatus", || schemars::schema_for!(TaskStatus)),
];
