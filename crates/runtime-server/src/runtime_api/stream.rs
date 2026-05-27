//! SSE streaming handlers and compat event mapping.
//!
//! Exports:
//! - `stream_turn` — `POST /v1/stream` (create thread + start turn in one call)
//! - `stream_thread_events` — `GET /v1/threads/{id}/events` (replay + live tail)

use std::convert::Infallible;
use std::path::PathBuf;
use std::time::Duration;

use async_stream::stream;
use axum::extract::{Path as AxumPath, Query, State};
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::Json;
use futures_util::Stream;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::broadcast::error::RecvError;

use crate::config::DEFAULT_TEXT_MODEL;
use crate::runtime_threads::event_coalesce::coalesce_delta_events;
use crate::runtime_threads::{CreateThreadRequest, StartTurnRequest};

use super::{map_thread_err, ApiError, RuntimeApiState};

#[derive(Debug, Deserialize)]
pub(super) struct StreamTurnRequest {
    prompt: String,
    model: Option<String>,
    mode: Option<String>,
    workspace: Option<PathBuf>,
    allow_shell: Option<bool>,
    trust_mode: Option<bool>,
    auto_approve: Option<bool>,
    #[serde(default)]
    route_intent: Option<String>,
    #[serde(default)]
    task_type: Option<String>,
    #[serde(default)]
    temperature: Option<f32>,
    #[serde(default)]
    top_p: Option<f32>,
    #[serde(default)]
    max_tokens: Option<u32>,
}

/// Accept `true`/`false`, `1`/`0`, and `yes`/`no` in query strings (desktop used `replay_only=1`).
fn deserialize_query_bool_option<'de, D>(deserializer: D) -> Result<Option<bool>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw: Option<String> = Option::deserialize(deserializer)?;
    match raw.as_deref() {
        None => Ok(None),
        Some("") => Ok(None),
        Some("1") | Some("true") | Some("True") | Some("yes") | Some("Yes") => Ok(Some(true)),
        Some("0") | Some("false") | Some("False") | Some("no") | Some("No") => Ok(Some(false)),
        Some(other) => Err(serde::de::Error::custom(format!(
            "invalid boolean for replay_only: '{other}' (use true/false or 1/0)"
        ))),
    }
}

#[derive(Debug, Deserialize)]
pub(super) struct ThreadEventsQuery {
    since_seq: Option<u64>,
    /// When true, emit persisted backlog only and close — for desktop session restore replay.
    #[serde(default, deserialize_with = "deserialize_query_bool_option")]
    replay_only: Option<bool>,
}

// ── Event payload helper ──────────────────────────────────────────────

pub(super) fn runtime_event_payload(
    event: crate::runtime_threads::RuntimeEventRecord,
) -> serde_json::Value {
    json!({
        "event_schema_version": event.schema_version,
        "seq": event.seq,
        "timestamp": event.timestamp,
        "thread_id": event.thread_id,
        "turn_id": event.turn_id,
        "item_id": event.item_id,
        "event": event.event,
        "payload": event.payload,
    })
}

// ── SSE frame builders (shared by stream_turn / stream_thread_events) ─

/// Emit an SSE frame with `event:` name (no `id:`).
pub(super) fn sse_json(event: &str, payload: serde_json::Value) -> SseEvent {
    let data = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string());
    SseEvent::default().event(event).data(data)
}

/// SSE frame with `id:` set to the durable monotonic `seq`.
///
/// Clients MAY use browser `Last-Event-ID` on reconnect; the authoritative cursor is still
/// `since_seq` on `GET /v1/threads/{id}/events` (this handler does not read `Last-Event-ID`
/// today).
pub(super) fn sse_json_seq(seq: u64, event: &str, payload: serde_json::Value) -> SseEvent {
    let data = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string());
    SseEvent::default()
        .event(event)
        .id(seq.to_string())
        .data(data)
}

// ── Compat event mapping ──────────────────────────────────────────────

/// Map a raw runtime event to a compat SSE event name and payload.
///
/// # SSE event names (stable compat surface):
///
/// | SSE `event`      | Source                 | Description               |
/// |------------------|------------------------|---------------------------|
/// | `thinking.delta` | `item.delta` (thinking) | Reasoning / thinking increment |
/// | `message.delta`  | `item.delta` (agent)   | Assistant text increment  |
/// | `tool.progress`  | `item.delta` (tool)    | Tool output streaming     |
/// | `tool.started`   | `item.started` (tool)  | Tool call began           |
/// | `tool.completed` | `item.completed` (tool)| Tool call finished        |
/// | `status`         | `item.completed`       | Status/notification       |
/// | `error`          | `item.completed`       | Error message             |
/// | `approval.required` | `approval.required` | Exec approval needed      |
/// | `sandbox.denied` | `sandbox.denied`       | Sandbox policy denied     |
/// | `turn.completed` | `turn.completed`       | Turn ended with usage     |
///
/// Additional events emitted outside this function by stream_turn:
/// - `turn.started` — emitted as first frame
/// - `done` — emitted as final frame to close SSE
pub(super) fn map_compat_stream_event(
    event: &crate::runtime_threads::RuntimeEventRecord,
) -> Option<SseEvent> {
    let payload = &event.payload;
    match event.event.as_str() {
        "item.delta" => {
            let kind = payload
                .get("kind")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if kind == "agent_message" {
                let content = payload
                    .get("delta")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                Some(sse_json_seq(
                    event.seq,
                    "message.delta",
                    json!({ "content": content }),
                ))
            } else if kind == "thinking" {
                let content = payload
                    .get("delta")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                Some(sse_json_seq(
                    event.seq,
                    "thinking.delta",
                    json!({ "content": content }),
                ))
            } else if kind == "tool_call" {
                let output = payload
                    .get("delta")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                Some(sse_json_seq(
                    event.seq,
                    "tool.progress",
                    json!({ "output": output }),
                ))
            } else {
                None
            }
        }
        "item.started" => {
            let tool = payload.get("tool")?;
            let id = tool.get("id").cloned().unwrap_or(Value::Null);
            let name = tool.get("name").cloned().unwrap_or(Value::Null);
            let input = tool.get("input").cloned().unwrap_or(Value::Null);
            Some(sse_json_seq(
                event.seq,
                "tool.started",
                json!({
                    "id": id,
                    "name": name,
                    "input": input,
                }),
            ))
        }
        "item.completed" | "item.failed" => {
            let item = payload.get("item")?;
            let kind = item
                .get("kind")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if kind == "tool_call" || kind == "file_change" || kind == "command_execution" {
                let id = payload
                    .get("tool")
                    .and_then(|t| t.get("id"))
                    .cloned()
                    .unwrap_or_else(|| item.get("id").cloned().unwrap_or(Value::Null));
                let success = event.event == "item.completed";
                let output = item.get("detail").cloned().unwrap_or_else(|| {
                    Value::String(
                        item.get("summary")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string(),
                    )
                });
                Some(sse_json_seq(
                    event.seq,
                    "tool.completed",
                    json!({
                        "id": id,
                        "success": success,
                        "output": output,
                    }),
                ))
            } else if kind == "status" {
                let message = item
                    .get("detail")
                    .and_then(|v| v.as_str())
                    .or_else(|| item.get("summary").and_then(|v| v.as_str()))
                    .unwrap_or_default();
                Some(sse_json_seq(
                    event.seq,
                    "status",
                    json!({ "message": message }),
                ))
            } else if kind == "error" {
                let message = item
                    .get("detail")
                    .and_then(|v| v.as_str())
                    .or_else(|| item.get("summary").and_then(|v| v.as_str()))
                    .unwrap_or_default();
                Some(sse_json_seq(
                    event.seq,
                    "error",
                    json!({ "message": message }),
                ))
            } else {
                None
            }
        }
        "approval.required" => Some(sse_json_seq(
            event.seq,
            "approval.required",
            payload.clone(),
        )),
        "sandbox.denied" => Some(sse_json_seq(event.seq, "sandbox.denied", payload.clone())),
        "turn.completed" => {
            let usage = payload
                .get("turn")
                .and_then(|turn| turn.get("usage"))
                .cloned()
                .unwrap_or(json!(null));
            let turn_summary = payload.get("turn_summary").cloned();
            let mut body = json!({ "usage": usage });
            if let Some(ref summary) = turn_summary {
                if let Some(obj) = body.as_object_mut() {
                    obj.insert("turn_summary".to_string(), summary.clone());
                }
            }
            Some(sse_json_seq(event.seq, "turn.completed", body))
        }
        "agent.spawned" => {
            let agent_id = payload
                .get("agent_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if agent_id.is_empty() {
                return None;
            }
            let prompt = payload
                .get("prompt")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            Some(sse_json_seq(
                event.seq,
                "agent.spawned",
                json!({
                    "agent_id": agent_id,
                    "prompt": prompt,
                }),
            ))
        }
        "agent.progress" => {
            let agent_id = payload
                .get("agent_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if agent_id.is_empty() {
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
                .unwrap_or_default();
            Some(sse_json_seq(
                event.seq,
                "agent.progress",
                json!({ "agent_id": agent_id, "status": status }),
            ))
        }
        "agent.completed" => {
            let agent_id = payload
                .get("agent_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if agent_id.is_empty() {
                return None;
            }
            let result = payload
                .get("item")
                .and_then(|item| item.get("detail").and_then(|v| v.as_str()))
                .unwrap_or_default();
            Some(sse_json_seq(
                event.seq,
                "agent.completed",
                json!({ "agent_id": agent_id, "result": result }),
            ))
        }
        "agent.list" => {
            let agents = payload.get("agents").cloned().unwrap_or(json!([]));
            Some(sse_json_seq(
                event.seq,
                "agent.list",
                json!({ "agents": agents }),
            ))
        }
        "craft.verdict" => Some(sse_json_seq(
            event.seq,
            "craft.verdict",
            payload.clone(),
        )),
        "craft.board_updated" => Some(sse_json_seq(
            event.seq,
            "craft.board_updated",
            payload.clone(),
        )),
        "panel.checklist" | "panel.scratchpad" | "panel.context" => {
            Some(sse_json_seq(event.seq, event.event.as_str(), payload.clone()))
        }
        _ => None,
    }
}

// ── SSE endpoint handlers ─────────────────────────────────────────────

pub(super) async fn stream_thread_events(
    State(state): State<RuntimeApiState>,
    AxumPath(id): AxumPath<String>,
    Query(query): Query<ThreadEventsQuery>,
) -> Result<Sse<impl Stream<Item = Result<SseEvent, Infallible>>>, ApiError> {
    let _ = state
        .runtime_threads
        .get_thread(&id)
        .await
        .map_err(map_thread_err)?;

    let backlog = state
        .runtime_threads
        .events_since_async(&id, query.since_seq)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    let mut last_seq = query.since_seq.unwrap_or(0);
    if let Some(last) = backlog.last() {
        last_seq = last.seq;
    }

    let replay_only = query.replay_only.unwrap_or(false);
    let mut live = state.runtime_threads.subscribe_events();
    let thread_id = id.clone();
    let runtime_threads = state.runtime_threads.clone();
    let stream = stream! {
        for event in coalesce_delta_events(backlog) {
            let seq = event.seq;
            let event_name = event.event.clone();
            let payload = runtime_event_payload(event);
            yield Ok(sse_json_seq(seq, &event_name, payload));
        }
        if replay_only {
            return;
        }
        loop {
            match live.recv().await {
                Ok(event) => {
                    if event.thread_id != thread_id {
                        continue;
                    }
                    if event.seq <= last_seq {
                        continue;
                    }
                    last_seq = event.seq;
                    let seq = event.seq;
                    let event_name = event.event.clone();
                    let payload = runtime_event_payload(event);
                    yield Ok(sse_json_seq(seq, &event_name, payload));
                }
                Err(RecvError::Lagged(_)) => {
                    // B3.3: consumer fell behind — catch up from store and merge deltas.
                    let catchup = runtime_threads
                        .events_since_async(&thread_id, Some(last_seq))
                        .await
                        .unwrap_or_default();
                    for event in coalesce_delta_events(catchup) {
                        if event.thread_id != thread_id {
                            continue;
                        }
                        if event.seq <= last_seq {
                            continue;
                        }
                        last_seq = event.seq;
                        let seq = event.seq;
                        let event_name = event.event.clone();
                        let payload = runtime_event_payload(event);
                        yield Ok(sse_json_seq(seq, &event_name, payload));
                    }
                }
                Err(RecvError::Closed) => break,
            }
        }
    };

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keepalive"),
    ))
}

pub(super) async fn stream_turn(
    State(state): State<RuntimeApiState>,
    Json(req): Json<StreamTurnRequest>,
) -> Result<Sse<impl Stream<Item = Result<SseEvent, Infallible>>>, ApiError> {
    if req.prompt.trim().is_empty() {
        return Err(ApiError::bad_request("prompt is required"));
    }

    let model = req.model.clone().unwrap_or_else(|| {
        state
            .config
            .default_text_model
            .clone()
            .unwrap_or_else(|| DEFAULT_TEXT_MODEL.to_string())
    });
    let workspace = req
        .workspace
        .clone()
        .unwrap_or_else(|| state.workspace.clone());
    let mode = req.mode.clone().unwrap_or_else(|| "agent".to_string());
    let allow_shell = req.allow_shell.unwrap_or(state.config.allow_shell());
    let trust_mode = req.trust_mode.unwrap_or(false);
    let auto_approve = req.auto_approve.unwrap_or(false);
    let prompt = req.prompt;
    let task_type = crate::task_type::resolve_task_type(
        req.task_type.as_deref(),
        &workspace,
        Some(prompt.as_str()),
    );

    let thread = state
        .runtime_threads
        .create_thread(CreateThreadRequest {
            model: Some(model.clone()),
            workspace: Some(workspace.clone()),
            mode: Some(mode.clone()),
            allow_shell: Some(allow_shell),
            trust_mode: Some(trust_mode),
            auto_approve: Some(auto_approve),
            archived: true,
            system_prompt: None,
            task_id: None,
            task_type: Some(task_type.as_str().to_string()),
        })
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create stream thread: {e}")))?;

    let turn = state
        .runtime_threads
        .start_turn(
            &thread.id,
            StartTurnRequest {
                prompt,
                input_summary: None,
                model: Some(model.clone()),
                mode: Some(mode.clone()),
                allow_shell: Some(allow_shell),
                trust_mode: Some(trust_mode),
                auto_approve: Some(auto_approve),
                route_intent: req.route_intent.clone(),
                temperature: req.temperature,
                top_p: req.top_p,
                max_tokens: req.max_tokens,
            },
        )
        .await
        .map_err(|e| ApiError::internal(format!("Failed to start stream turn: {e}")))?;

    let backlog = state
        .runtime_threads
        .events_since_async(&thread.id, None)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to load stream backlog: {e}")))?;
    let mut live = state.runtime_threads.subscribe_events();
    let thread_id = thread.id.clone();
    let turn_id = turn.id.clone();
    let runtime_threads = state.runtime_threads.clone();
    let mut last_seq = backlog.last().map(|e| e.seq).unwrap_or(0);

    let stream = stream! {
        yield Ok(sse_json("turn.started", json!({
            "thread_id": thread.id,
            "turn_id": turn.id,
            "model": model,
            "mode": mode,
            "workspace": workspace,
        })));

        for event in coalesce_delta_events(backlog) {
            if event.thread_id != thread_id || event.turn_id.as_deref() != Some(&turn_id) {
                continue;
            }
            last_seq = last_seq.max(event.seq);
            if let Some(mapped) = map_compat_stream_event(&event) {
                yield Ok(mapped);
            }
            if event.event == "turn.completed" {
                yield Ok(sse_json("done", json!({})));
                return;
            }
        }

        loop {
            match live.recv().await {
                Ok(event) => {
                    if event.thread_id != thread_id || event.turn_id.as_deref() != Some(&turn_id) {
                        continue;
                    }
                    last_seq = last_seq.max(event.seq);
                    if let Some(mapped) = map_compat_stream_event(&event) {
                        yield Ok(mapped);
                    }
                    if event.event == "turn.completed" {
                        break;
                    }
                }
                Err(RecvError::Lagged(_)) => {
                    let catchup = runtime_threads
                        .events_since_async(&thread_id, Some(last_seq))
                        .await
                        .unwrap_or_default();
                    for event in coalesce_delta_events(catchup) {
                        if event.thread_id != thread_id || event.turn_id.as_deref() != Some(&turn_id) {
                            continue;
                        }
                        if event.seq <= last_seq {
                            continue;
                        }
                        last_seq = event.seq;
                        if let Some(mapped) = map_compat_stream_event(&event) {
                            yield Ok(mapped);
                        }
                        if event.event == "turn.completed" {
                            yield Ok(sse_json("done", json!({})));
                            return;
                        }
                    }
                }
                Err(RecvError::Closed) => {
                    yield Ok(sse_json("error", json!({ "message": "event channel closed" })));
                    break;
                }
            }
        }

        yield Ok(sse_json("done", json!({})));
    };

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keepalive"),
    ))
}

// ── Contract tests (A+.2) ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_threads::RuntimeEventRecord;
    use axum::response::IntoResponse;
    use chrono::Utc;
    use serde_json::json;
    use std::convert::Infallible;

    fn record(event: &str, payload: serde_json::Value) -> RuntimeEventRecord {
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

    /// Serialize a single SSE event to its wire-format text.
    async fn render(event: SseEvent) -> String {
        let stream = async_stream::stream! {
            yield Ok::<_, Infallible>(event);
        };
        let body =
            axum::body::to_bytes(Sse::new(stream).into_response().into_body(), usize::MAX)
                .await
                .unwrap();
        String::from_utf8_lossy(&body).to_string()
    }

    // ── Stable event subset v1 — one test per documented event ─────────

    #[tokio::test]
    async fn maps_message_delta() {
        let r = record("item.delta", json!({"kind": "agent_message", "delta": "hello"}));
        let sse = map_compat_stream_event(&r).expect("should map message.delta");
        let text = render(sse).await;
        assert!(text.contains("event: message.delta"));
        assert!(text.contains("\"content\":\"hello\""));
    }

    #[tokio::test]
    async fn maps_thinking_delta() {
        let r = record("item.delta", json!({"kind": "thinking", "delta": "hmm"}));
        let sse = map_compat_stream_event(&r).expect("should map thinking.delta");
        let text = render(sse).await;
        assert!(text.contains("event: thinking.delta"));
        assert!(text.contains("\"content\":\"hmm\""));
    }

    #[tokio::test]
    async fn maps_tool_progress() {
        let r = record("item.delta", json!({"kind": "tool_call", "delta": "building..."}));
        let sse = map_compat_stream_event(&r).expect("should map tool.progress");
        let text = render(sse).await;
        assert!(text.contains("event: tool.progress"));
        assert!(text.contains("\"output\":\"building...\""));
    }

    #[tokio::test]
    async fn maps_tool_started() {
        let r = record(
            "item.started",
            json!({"tool": {"id": "t1", "name": "read_file", "input": {"path": "x"}}}),
        );
        let sse = map_compat_stream_event(&r).expect("should map tool.started");
        let text = render(sse).await;
        assert!(text.contains("event: tool.started"));
        assert!(text.contains("\"name\":\"read_file\""));
    }

    #[tokio::test]
    async fn maps_tool_completed() {
        let r = record(
            "item.completed",
            json!({"item": {"kind": "tool_call", "id": "t1", "detail": "ok"}}),
        );
        let sse = map_compat_stream_event(&r).expect("should map tool.completed");
        let text = render(sse).await;
        assert!(text.contains("event: tool.completed"));
        assert!(text.contains("\"success\":true"));
    }

    #[tokio::test]
    async fn maps_tool_failed() {
        let r = record(
            "item.failed",
            json!({"item": {"kind": "command_execution", "id": "t2", "summary": "exit 1"}}),
        );
        let sse = map_compat_stream_event(&r).expect("should map tool.completed (failed)");
        let text = render(sse).await;
        assert!(text.contains("event: tool.completed"));
        assert!(text.contains("\"success\":false"));
    }

    #[tokio::test]
    async fn maps_status() {
        let r = record(
            "item.completed",
            json!({"item": {"kind": "status", "detail": "compaction done"}}),
        );
        let sse = map_compat_stream_event(&r).expect("should map status");
        let text = render(sse).await;
        assert!(text.contains("event: status"));
        assert!(text.contains("compaction done"));
    }

    #[tokio::test]
    async fn maps_error() {
        let r = record(
            "item.completed",
            json!({"item": {"kind": "error", "detail": "something broke"}}),
        );
        let sse = map_compat_stream_event(&r).expect("should map error");
        let text = render(sse).await;
        assert!(text.contains("event: error"));
        assert!(text.contains("something broke"));
    }

    #[tokio::test]
    async fn maps_approval_required() {
        let r = record("approval.required", json!({"id": "ap1", "tool_name": "exec_shell"}));
        let sse = map_compat_stream_event(&r).expect("should map approval.required");
        let text = render(sse).await;
        assert!(text.contains("event: approval.required"));
        assert!(text.contains("\"id\":\"ap1\""));
    }

    #[tokio::test]
    async fn maps_sandbox_denied() {
        let r = record("sandbox.denied", json!({"reason": "network"}));
        let sse = map_compat_stream_event(&r).expect("should map sandbox.denied");
        let text = render(sse).await;
        assert!(text.contains("event: sandbox.denied"));
    }

    #[tokio::test]
    async fn maps_turn_completed_with_usage_and_summary() {
        let r = record(
            "turn.completed",
            json!({
                "turn": {"usage": {"input_tokens": 100, "output_tokens": 50}},
                "turn_summary": {"step_count": 3, "tool_names": ["read_file"], "end_reason": null}
            }),
        );
        let sse = map_compat_stream_event(&r).expect("should map turn.completed");
        let text = render(sse).await;
        assert!(text.contains("event: turn.completed"));
        assert!(text.contains("\"input_tokens\":100"));
        assert!(text.contains("\"turn_summary\""));
    }

    #[tokio::test]
    async fn maps_agent_spawned() {
        let r = record("agent.spawned", json!({"agent_id": "a1", "prompt": "do it"}));
        let sse = map_compat_stream_event(&r).expect("should map agent.spawned");
        let text = render(sse).await;
        assert!(text.contains("event: agent.spawned"));
    }

    #[tokio::test]
    async fn maps_agent_spawned_no_prompt() {
        let r = record("agent.spawned", json!({"agent_id": "a1"}));
        let sse = map_compat_stream_event(&r).expect("should map agent.spawned without prompt");
        let text = render(sse).await;
        assert!(text.contains("event: agent.spawned"));
    }

    #[tokio::test]
    async fn maps_agent_progress() {
        let r = record("agent.progress", json!({"agent_id": "a1", "status": "working"}));
        let sse = map_compat_stream_event(&r).expect("should map agent.progress");
        let text = render(sse).await;
        assert!(text.contains("event: agent.progress"));
    }

    #[tokio::test]
    async fn maps_agent_completed() {
        let r = record("agent.completed", json!({"agent_id": "a1", "result": "done"}));
        let sse = map_compat_stream_event(&r).expect("should map agent.completed");
        let text = render(sse).await;
        assert!(text.contains("event: agent.completed"));
    }

    async fn maps_craft_events() {
        let r = record(
            "craft.verdict",
            json!({"agent_id": "a1", "verdict": "BLOCKER", "task_id": "t1"}),
        );
        let sse = map_compat_stream_event(&r).expect("should map craft.verdict");
        let text = render(sse).await;
        assert!(text.contains("event: craft.verdict"));

        let r = record(
            "craft.board_updated",
            json!({"task_id": "t1", "partition": "reviewer"}),
        );
        let sse = map_compat_stream_event(&r).expect("should map craft.board_updated");
        let text = render(sse).await;
        assert!(text.contains("event: craft.board_updated"));
    }

    #[tokio::test]
    async fn maps_agent_list() {
        let r = record("agent.list", json!({"agents": [{"id": "a1"}]}));
        let sse = map_compat_stream_event(&r).expect("should map agent.list");
        let text = render(sse).await;
        assert!(text.contains("event: agent.list"));
    }

    #[tokio::test]
    async fn maps_panel_events() {
        for ev in ["panel.checklist", "panel.scratchpad", "panel.context"] {
            let r = record(ev, json!({"data": []}));
            let sse = map_compat_stream_event(&r)
                .unwrap_or_else(|| panic!("should map {ev}"));
            let text = render(sse).await;
            assert!(
                text.contains(&format!("event: {ev}")),
                "missing event: {ev}"
            );
        }
    }

    #[test]
    fn runtime_event_payload_includes_event_schema_version() {
        let r = record("turn.started", json!({"turn": {"id": "t1"}}));
        let payload = runtime_event_payload(r);
        assert_eq!(
            payload["event_schema_version"],
            crate::runtime_threads::CURRENT_EVENT_SCHEMA_VERSION
        );
        assert_eq!(payload["event"], "turn.started");
    }

    // ── Boundary / negative cases ──────────────────────────────────────

    #[test]
    fn unknown_event_returns_none() {
        let r = record("some.unknown.event", json!({}));
        assert!(map_compat_stream_event(&r).is_none());
    }

    #[test]
    fn item_delta_unknown_kind_returns_none() {
        let r = record("item.delta", json!({"kind": "weird", "delta": "x"}));
        assert!(map_compat_stream_event(&r).is_none());
    }

    #[test]
    fn agent_spawned_empty_id_returns_none() {
        let r = record("agent.spawned", json!({"agent_id": ""}));
        assert!(map_compat_stream_event(&r).is_none());
    }

    #[test]
    fn agent_progress_empty_id_returns_none() {
        let r = record("agent.progress", json!({"agent_id": ""}));
        assert!(map_compat_stream_event(&r).is_none());
    }

    #[test]
    fn agent_completed_empty_id_returns_none() {
        let r = record("agent.completed", json!({"agent_id": ""}));
        assert!(map_compat_stream_event(&r).is_none());
    }

    #[test]
    fn tool_started_without_tool_field_returns_none() {
        let r = record("item.started", json!({"something": "else"}));
        assert!(map_compat_stream_event(&r).is_none());
    }

    #[test]
    fn item_completed_without_item_field_returns_none() {
        let r = record("item.completed", json!({"not_item": {}}));
        assert!(map_compat_stream_event(&r).is_none());
    }

}