//! Map persisted/broadcast [`RuntimeEventRecord`]s to engine [`Event`]s for the TUI.

use chrono::Utc;
use serde_json::Value;
use zagens_core::cycle::CycleBriefing;
use zagens_core::error_taxonomy::{ErrorCategory, ErrorEnvelope, ErrorSeverity};
use zagens_core::events::Event;
use zagens_core::models::Usage;
use zagens_core::turn::TurnOutcomeStatus;
use zagens_tools::{ToolError, ToolResult};

use crate::runtime_threads::RuntimeEventRecord;

/// Convert a runtime thread event into a UI-facing engine event (when applicable).
#[must_use]
pub fn map_record(record: &RuntimeEventRecord) -> Option<Event> {
    let payload = &record.payload;
    match record.event.as_str() {
        "item.delta" => map_item_delta(payload),
        "item.started" => map_item_started(payload),
        "item.completed" | "item.failed" | "item.interrupted" => {
            map_item_finished(record.event.as_str(), payload)
        }
        "approval.required" => {
            let id = payload.get("id").and_then(|v| v.as_str())?.to_string();
            let tool_name = payload
                .get("tool_name")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let description = payload
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let approval_key = payload
                .get("approval_key")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            Some(Event::ApprovalRequired {
                id,
                tool_name,
                description,
                approval_key,
            })
        }
        "turn.completed" => map_turn_completed(payload),
        "agent.spawned" => {
            let id = payload.get("agent_id").and_then(|v| v.as_str())?;
            if id.is_empty() {
                return None;
            }
            let prompt = payload
                .get("prompt")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            Some(Event::AgentSpawned {
                id: id.to_string(),
                prompt,
            })
        }
        "agent.progress" => {
            let id = payload.get("agent_id").and_then(|v| v.as_str())?;
            if id.is_empty() {
                return None;
            }
            let status = payload
                .get("status")
                .and_then(|v| v.as_str())
                .or_else(|| {
                    payload
                        .get("item")
                        .and_then(|item| item.get("detail").and_then(|v| v.as_str()))
                })
                .unwrap_or_default()
                .to_string();
            Some(Event::AgentProgress {
                id: id.to_string(),
                status,
            })
        }
        "agent.completed" => {
            let id = payload.get("agent_id").and_then(|v| v.as_str())?;
            if id.is_empty() {
                return None;
            }
            let result = payload
                .get("item")
                .and_then(|item| item.get("detail").and_then(|v| v.as_str()))
                .unwrap_or_default()
                .to_string();
            Some(Event::AgentComplete {
                id: id.to_string(),
                result,
            })
        }
        "harness.cycle_advanced" => {
            let from = payload.get("from").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let to = payload.get("to").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let cycle = payload
                .get("cycle")
                .and_then(|v| v.as_u64())
                .unwrap_or(from as u64) as u32;
            let briefing_preview = payload
                .get("briefing_preview")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let token_estimate = payload
                .get("briefing_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize;
            Some(Event::CycleAdvanced {
                from,
                to,
                briefing: CycleBriefing {
                    cycle,
                    timestamp: Utc::now(),
                    briefing_text: briefing_preview,
                    token_estimate,
                },
            })
        }
        "craft.verdict" => Some(Event::CraftVerdict {
            agent_id: payload
                .get("agent_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            agent_type: payload
                .get("agent_type")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            task_id: payload
                .get("task_id")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            verdict: payload
                .get("verdict")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            summary: payload
                .get("summary")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            items: payload.get("items").cloned().unwrap_or(Value::Null),
        }),
        "craft.board_updated" => Some(Event::CraftBoardUpdated {
            task_id: payload
                .get("task_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            partition: payload
                .get("partition")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            agent_id: payload
                .get("agent_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
        }),
        _ => None,
    }
}

fn map_item_delta(payload: &Value) -> Option<Event> {
    let kind = payload
        .get("kind")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    match kind {
        "agent_message" => {
            let content = payload
                .get("delta")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            Some(Event::MessageDelta { index: 0, content })
        }
        "thinking" => {
            let content = payload
                .get("delta")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            Some(Event::ThinkingDelta { index: 0, content })
        }
        "tool_call" => {
            let output = payload
                .get("delta")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            Some(Event::ToolCallProgress {
                id: String::new(),
                output,
            })
        }
        _ => None,
    }
}

fn map_item_started(payload: &Value) -> Option<Event> {
    let item = payload.get("item")?;
    let kind = item
        .get("kind")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    if kind == "agent_message" {
        return Some(Event::MessageStarted { index: 0 });
    }
    let tool = payload.get("tool")?;
    let id = tool.get("id").and_then(|v| v.as_str())?.to_string();
    let name = tool
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let input = tool.get("input").cloned().unwrap_or(Value::Null);
    Some(Event::ToolCallStarted { id, name, input })
}

fn map_item_finished(event_name: &str, payload: &Value) -> Option<Event> {
    let item = payload.get("item")?;
    let kind = item
        .get("kind")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    match kind {
        "agent_message" => Some(Event::MessageComplete { index: 0 }),
        "tool_call" | "file_change" | "command_execution" => {
            let id = payload
                .get("tool")
                .and_then(|t| t.get("id"))
                .and_then(|v| v.as_str())
                .or_else(|| item.get("id").and_then(|v| v.as_str()))?
                .to_string();
            let name = payload
                .get("tool")
                .and_then(|t| t.get("name"))
                .and_then(|v| v.as_str())
                .or_else(|| {
                    item.get("metadata")
                        .and_then(|m| m.get("tool_name"))
                        .and_then(|v| v.as_str())
                })
                .unwrap_or("tool")
                .to_string();
            let success = event_name == "item.completed";
            let detail = item
                .get("detail")
                .and_then(|v| v.as_str())
                .or_else(|| item.get("summary").and_then(|v| v.as_str()))
                .unwrap_or_default();
            let result = if success {
                Ok(ToolResult::success(detail))
            } else {
                Err(ToolError::ExecutionFailed {
                    message: detail.to_string(),
                })
            };
            Some(Event::ToolCallComplete { id, name, result })
        }
        "status" => {
            let message = item
                .get("detail")
                .and_then(|v| v.as_str())
                .or_else(|| item.get("summary").and_then(|v| v.as_str()))
                .unwrap_or_default()
                .to_string();
            Some(Event::Status { message })
        }
        "error" => {
            let message = item
                .get("detail")
                .and_then(|v| v.as_str())
                .or_else(|| item.get("summary").and_then(|v| v.as_str()))
                .unwrap_or("error")
                .to_string();
            Some(Event::Error {
                envelope: ErrorEnvelope::new(
                    ErrorCategory::Internal,
                    ErrorSeverity::Error,
                    false,
                    "runtime_error",
                    message,
                ),
                recoverable: false,
            })
        }
        _ => None,
    }
}

fn map_turn_completed(payload: &Value) -> Option<Event> {
    let turn = payload.get("turn")?;
    let usage = turn.get("usage").and_then(parse_usage).unwrap_or_default();
    let last_request_input_tokens = turn
        .get("last_request_input_tokens")
        .and_then(|v| v.as_u64())
        .map(|n| n as u32);
    let error = turn
        .get("error")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let status = turn
        .get("status")
        .and_then(|v| v.as_str())
        .map(runtime_turn_status_to_outcome)
        .unwrap_or(TurnOutcomeStatus::Completed);
    let summary = payload.get("turn_summary");
    let step_count = summary
        .and_then(|s| s.get("step_count"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    let tool_names = summary
        .and_then(|s| s.get("tool_names"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let end_reason = summary
        .and_then(|s| s.get("end_reason"))
        .and_then(|v| v.as_str())
        .map(str::to_string);
    Some(Event::TurnComplete {
        usage,
        last_request_input_tokens,
        status,
        error,
        step_count,
        tool_names,
        end_reason,
    })
}

fn parse_usage(value: &Value) -> Option<Usage> {
    serde_json::from_value(value.clone()).ok()
}

fn runtime_turn_status_to_outcome(status: &str) -> TurnOutcomeStatus {
    match status {
        "failed" => TurnOutcomeStatus::Failed,
        "interrupted" | "canceled" => TurnOutcomeStatus::Interrupted,
        _ => TurnOutcomeStatus::Completed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn record(event: &str, payload: Value) -> RuntimeEventRecord {
        RuntimeEventRecord {
            schema_version: 2,
            seq: 1,
            timestamp: Utc::now(),
            thread_id: "thr_test".to_string(),
            turn_id: Some("turn_test".to_string()),
            item_id: None,
            event: event.to_string(),
            payload,
        }
    }

    #[test]
    fn maps_agent_message_delta() {
        let ev = map_record(&record(
            "item.delta",
            json!({ "kind": "agent_message", "delta": "hello" }),
        ))
        .expect("delta");
        assert!(matches!(ev, Event::MessageDelta { content, .. } if content == "hello"));
    }

    #[test]
    fn maps_turn_completed() {
        let ev = map_record(&record(
            "turn.completed",
            json!({
                "turn": {
                    "status": "completed",
                    "usage": { "input_tokens": 1, "output_tokens": 2 },
                    "error": null
                },
                "turn_summary": {
                    "step_count": 3,
                    "tool_names": ["read_file"],
                    "end_reason": "end_turn"
                }
            }),
        ))
        .expect("turn");
        assert!(matches!(
            ev,
            Event::TurnComplete {
                step_count: 3,
                end_reason: Some(reason),
                ..
            } if reason == "end_turn"
        ));
    }

    #[test]
    fn maps_approval_required() {
        let ev = map_record(&record(
            "approval.required",
            json!({
                "id": "tc1",
                "tool_name": "bash",
                "description": "run ls"
            }),
        ))
        .expect("approval");
        assert!(matches!(
            ev,
            Event::ApprovalRequired { id, tool_name, .. }
                if id == "tc1" && tool_name == "bash"
        ));
    }
}
