//! HTTP / SSE task-graph JSON (LHT Phase 2).

use deepseek_core::chat::Message;
use deepseek_core::long_horizon::LongHorizonConfig;
use serde::Serialize;
use serde_json::{json, Value};

use crate::tools::plan::{PlanSnapshot, StepStatus};
use crate::tools::todo::{TodoListSnapshot, TodoStatus};

use super::graph::CodeTaskGraph;
use super::nudge::LongHorizonSessionState;
use super::objective::derive_objective;
use super::verify::{parse_verify_command, strip_verify_prefix};

#[derive(Debug, Clone, Serialize)]
pub struct TaskGraphPhaseJson {
    pub step: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskGraphChecklistJson {
    pub id: u32,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verify_command: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskGraphTelemetryJson {
    pub emitted: u32,
    pub converted: u32,
    pub blocked: u32,
    pub conversion_pct: u8,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskGraphResponse {
    pub objective: String,
    pub objective_source: String,
    pub phases: Vec<TaskGraphPhaseJson>,
    pub checklist: Vec<TaskGraphChecklistJson>,
    pub completion_pct: u8,
    pub open_items: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub in_progress_id: Option<u32>,
    pub incomplete: bool,
    pub lht_enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lht_blocked: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nudge_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub telemetry: Option<TaskGraphTelemetryJson>,
}

#[must_use]
pub fn build_task_graph_response(
    plan: &PlanSnapshot,
    checklist: &TodoListSnapshot,
    messages: &[Message],
    lang: &str,
    lht: &LongHorizonConfig,
    session: Option<&LongHorizonSessionState>,
) -> TaskGraphResponse {
    let (lht_blocked, nudge_count, telemetry) = session
        .map(|s| {
            (
                Some(s.tracker.is_blocked() || s.paused),
                Some(s.tracker.max_item_nudge_count()),
                Some(TaskGraphTelemetryJson {
                    emitted: s.telemetry.emitted,
                    converted: s.telemetry.converted,
                    blocked: s.telemetry.blocked,
                    conversion_pct: s.telemetry.conversion_pct(),
                }),
            )
        })
        .unwrap_or((None, None, None));
    assemble_task_graph(
        plan, checklist, messages, lang, lht, lht_blocked, nudge_count, telemetry,
    )
}

/// Assemble a task-graph response from explicit harness telemetry (real-time
/// cache path; no live engine / op loop). Objective falls back to plan /
/// checklist when `messages` is empty.
#[allow(clippy::too_many_arguments)]
#[must_use]
fn assemble_task_graph(
    plan: &PlanSnapshot,
    checklist: &TodoListSnapshot,
    messages: &[Message],
    lang: &str,
    lht: &LongHorizonConfig,
    lht_blocked: Option<bool>,
    nudge_count: Option<u32>,
    telemetry: Option<TaskGraphTelemetryJson>,
) -> TaskGraphResponse {
    let mut graph = CodeTaskGraph::from_snapshots(plan, checklist);
    let (objective, source) = derive_objective(plan, checklist, messages, lang);
    graph.objective = objective.clone();
    graph.objective_source = source;

    let phases = graph
        .phases
        .iter()
        .map(|p| TaskGraphPhaseJson {
            step: p.step.clone(),
            status: step_status_str(&p.status).to_string(),
        })
        .collect();

    let checklist_json = graph
        .checklist
        .iter()
        .map(|c| TaskGraphChecklistJson {
            id: c.id,
            content: strip_verify_prefix(&c.content),
            verify_command: parse_verify_command(&c.content),
            status: todo_status_str(c.status).to_string(),
        })
        .collect();

    TaskGraphResponse {
        objective,
        objective_source: source.to_string(),
        phases,
        checklist: checklist_json,
        completion_pct: graph.completion_pct,
        open_items: graph.open_items,
        in_progress_id: graph.in_progress_id,
        incomplete: graph.incomplete() && lht.enabled,
        lht_enabled: lht.enabled,
        lht_blocked,
        nudge_count,
        telemetry,
    }
}

#[must_use]
pub fn build_task_graph_value(
    plan: &PlanSnapshot,
    checklist: &TodoListSnapshot,
    messages: &[Message],
    lang: &str,
    lht: &LongHorizonConfig,
    session: Option<&LongHorizonSessionState>,
) -> Value {
    serde_json::to_value(build_task_graph_response(
        plan, checklist, messages, lang, lht, session,
    ))
    .unwrap_or_else(|_| json!({ "error": "task_graph_serialize_failed" }))
}

/// Real-time task-graph from persisted snapshots + cached telemetry — never
/// touches the engine op loop, so it stays live during a long turn (§4.9).
#[must_use]
pub fn build_task_graph_value_with_telemetry(
    plan: &PlanSnapshot,
    checklist: &TodoListSnapshot,
    lang: &str,
    lht: &LongHorizonConfig,
    lht_blocked: Option<bool>,
    nudge_count: Option<u32>,
    telemetry: Option<TaskGraphTelemetryJson>,
) -> Value {
    serde_json::to_value(assemble_task_graph(
        plan,
        checklist,
        &[],
        lang,
        lht,
        lht_blocked,
        nudge_count,
        telemetry,
    ))
    .unwrap_or_else(|_| json!({ "error": "task_graph_serialize_failed" }))
}

fn step_status_str(status: &StepStatus) -> &'static str {
    match status {
        StepStatus::Pending => "pending",
        StepStatus::InProgress => "in_progress",
        StepStatus::Completed => "completed",
    }
}

fn todo_status_str(status: TodoStatus) -> &'static str {
    status.as_str()
}
