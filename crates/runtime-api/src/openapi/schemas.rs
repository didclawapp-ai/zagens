//! HTTP wire types for OpenAPI `components.schemas` (D8).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::task::{TaskCounts, TaskRecord, TaskStatus, TaskSummary, TasksResponse};

use zagens_core::coherence::CoherenceState;
use zagens_core::engine::ReplayEffectCounts;
use zagens_core::models::{ServerToolUsage, Usage};
use zagens_runtime_adapters::persist::SessionMetadata;
use zagens_runtime_orchestrator::runtime_threads::{
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
pub struct ResumeSessionKernelReplay {
    pub turn_count: usize,
    pub turns_with_events: usize,
    pub turns_coherent: usize,
    pub all_coherent: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_turn_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_step_idx: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_max_steps: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_tool_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_message_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_planned_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_coverage_ok: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_coverage_summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_timeline_ok: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_timeline_summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kernel_model_request_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kernel_estimated_min_session_messages: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_role_index_ok: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_role_index_summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_memory_plane_ok: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_memory_plane_summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_compaction_depth_ok: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_compaction_summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_compaction_artifact_ok: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_compaction_artifact_summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_continuation_anchor_ok: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_continuation_anchor_summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_notify_lsp_anchor_ok: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_notify_lsp_anchor_summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_request_approval_anchor_ok: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_request_approval_anchor_summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_memory_plane_replay_anchor_ok: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_memory_plane_replay_anchor_summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_compaction_replay_anchor_ok: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_compaction_replay_anchor_summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replay_effect_counts: Option<ReplayEffectCounts>,
    /// Anchor-class replay effect total (`inject_steer` + `run_compaction` + `notify_lsp`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replay_anchor_effect_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kernel_transcript_preview_row_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_transcript_preview_ok: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_transcript_preview_body_ok: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ResumeSessionResponse {
    pub thread_id: String,
    pub session_id: String,
    pub message_count: usize,
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kernel_replay: Option<ResumeSessionKernelReplay>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub queued: Option<zagens_runtime_orchestrator::runtime_threads::PromptAdmission>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ErrorBody {
    pub error: String,
}

/// One OpenAPI component schema export (`name`, `schema_for!` thunk).
pub type SchemaExportFn = fn() -> schemars::Schema;

/// Build a static `schemars::Schema` from a hand-written JSON value.
///
/// Used for wire types whose nested config (`zagens_core::long_horizon`,
/// `features`) does not derive `JsonSchema`, so `schema_for!` is unavailable.
/// Kept permissive (`additionalProperties: true`) for forward compatibility
/// with the overlay `extras` catch-all.
fn schema_from_json(value: Value) -> schemars::Schema {
    serde_json::from_value(value).expect("static openapi schema is valid")
}

fn thread_config_overlay_schema() -> schemars::Schema {
    schema_from_json(json!({
        "type": "object",
        "description": "Per-session config overlay (C scheme). Omitted/absent field = inherit global base. Unknown keys are preserved via the `extras` catch-all.",
        "additionalProperties": true,
        "properties": {
            "long_horizon": { "type": "object", "additionalProperties": true },
            "compaction": { "type": "object", "additionalProperties": true },
            "features": { "type": "object", "additionalProperties": true },
            "memory": { "type": "object", "additionalProperties": true },
            "topic_memory": { "type": "object", "additionalProperties": true },
            "lsp": { "type": "object", "additionalProperties": true },
            "snapshots": { "type": "object", "additionalProperties": true },
            "approval_policy": { "type": "string" },
            "lht_composer_mode": { "type": "string", "enum": ["auto", "strict", "off"] }
        }
    }))
}

fn thread_config_response_schema() -> schemars::Schema {
    schema_from_json(json!({
        "type": "object",
        "description": "Resolved per-thread config: global baseline, raw overlay, and merged effective view.",
        "required": ["base", "effective"],
        "properties": {
            "base": { "$ref": "#/components/schemas/ThreadConfigOverlay" },
            "overlay": { "$ref": "#/components/schemas/ThreadConfigOverlay" },
            "effective": { "$ref": "#/components/schemas/ThreadConfigOverlay" }
        }
    }))
}

fn runtime_active_turns_schema() -> schemars::Schema {
    schema_from_json(json!({
        "type": "object",
        "description": "Runtime-wide active turn probe used by the sidecar restart gate.",
        "required": ["count", "threads"],
        "properties": {
            "count": { "type": "integer", "minimum": 0 },
            "threads": {
                "type": "array",
                "items": {
                    "type": "object",
                    "required": ["thread_id", "status"],
                    "properties": {
                        "thread_id": { "type": "string" },
                        "turn_id": { "type": "string" },
                        "status": { "type": "string" }
                    }
                }
            }
        }
    }))
}

/// Core schemas exported to OpenAPI components.
pub const SCHEMA_EXPORTS: &[(&str, SchemaExportFn)] = &[
    ("CoherenceState", || schemars::schema_for!(CoherenceState)),
    ("RuntimeTurnStatus", || {
        schemars::schema_for!(RuntimeTurnStatus)
    }),
    ("TurnItemKind", || schemars::schema_for!(TurnItemKind)),
    ("TurnItemLifecycleStatus", || {
        schemars::schema_for!(TurnItemLifecycleStatus)
    }),
    ("Usage", || schemars::schema_for!(Usage)),
    ("ServerToolUsage", || schemars::schema_for!(ServerToolUsage)),
    ("SessionMetadata", || schemars::schema_for!(SessionMetadata)),
    ("SessionsListResponse", || {
        schemars::schema_for!(SessionsListResponse)
    }),
    ("SessionDetailResponse", || {
        schemars::schema_for!(SessionDetailResponse)
    }),
    ("ResumeSessionKernelReplay", || {
        schemars::schema_for!(ResumeSessionKernelReplay)
    }),
    ("ReplayEffectCounts", || {
        schemars::schema_for!(ReplayEffectCounts)
    }),
    ("ResumeSessionResponse", || {
        schemars::schema_for!(ResumeSessionResponse)
    }),
    ("ThreadRecord", || schemars::schema_for!(ThreadRecord)),
    ("TurnItemRecord", || schemars::schema_for!(TurnItemRecord)),
    ("ThreadDetail", || schemars::schema_for!(ThreadDetail)),
    ("ThreadSummary", || schemars::schema_for!(ThreadSummary)),
    ("TurnRecord", || schemars::schema_for!(TurnRecord)),
    ("CreateThreadRequest", || {
        schemars::schema_for!(CreateThreadRequest)
    }),
    ("UpdateThreadRequest", || {
        schemars::schema_for!(UpdateThreadRequest)
    }),
    ("StartTurnRequest", || {
        schemars::schema_for!(StartTurnRequest)
    }),
    ("SteerTurnRequest", || {
        schemars::schema_for!(SteerTurnRequest)
    }),
    ("StartTurnResponse", || {
        schemars::schema_for!(StartTurnResponse)
    }),
    ("StreamTurnRequest", || {
        schemars::schema_for!(StreamTurnRequest)
    }),
    ("RoutingRule", || schemars::schema_for!(RoutingRule)),
    ("RoutingRulesDoc", || schemars::schema_for!(RoutingRulesDoc)),
    ("UsageTotals", || schemars::schema_for!(UsageTotals)),
    ("UsageBucket", || schemars::schema_for!(UsageBucket)),
    ("UsageAggregation", || {
        schemars::schema_for!(UsageAggregation)
    }),
    ("ErrorBody", || schemars::schema_for!(ErrorBody)),
    ("TaskRecord", || schemars::schema_for!(TaskRecord)),
    ("TaskSummary", || schemars::schema_for!(TaskSummary)),
    ("TaskCounts", || schemars::schema_for!(TaskCounts)),
    ("TasksResponse", || schemars::schema_for!(TasksResponse)),
    ("TaskStatus", || schemars::schema_for!(TaskStatus)),
    ("ThreadConfigOverlay", thread_config_overlay_schema),
    ("ThreadConfigResponse", thread_config_response_schema),
    ("RuntimeActiveTurns", runtime_active_turns_schema),
];
