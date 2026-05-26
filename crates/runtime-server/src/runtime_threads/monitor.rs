//! Turn monitor: engine event loop to persisted turns/items (A4.6 extract).

use std::collections::HashMap;

use anyhow::{Result, anyhow};
use chrono::Utc;
use serde_json::json;
use uuid::Uuid;

use crate::core::engine::EngineHandle;
use crate::core::events::{Event as EngineEvent, TurnOutcomeStatus, TurnSummary};
use crate::models::Usage;
use crate::tools::subagent::SubAgentStatus;

use super::active::{touch_lru, PendingApproval, RuntimeApprovalDecision};
use super::persist::duration_ms;
use super::types::*;
use super::{
    summarize_text, tool_kind_for_name, RuntimeThreadManager, SUMMARY_LIMIT,
    CURRENT_RUNTIME_SCHEMA_VERSION,
};

impl RuntimeThreadManager {
    pub(crate) async fn monitor_turn(
        &self,
        thread_id: String,
        turn_id: String,
        engine: EngineHandle,
    ) -> Result<()> {
        tracing::info!(
            thread_id = %thread_id,
            turn_id = %turn_id,
            "monitor_turn start"
        );

        let mut current_message_item: Option<(String, String)> = None;
        // Synthetic item id for thinking/reasoning deltas (not persisted as a TurnItem).
        let mut thinking_stream_item_id: Option<String> = None;
        let mut tool_items: HashMap<String, String> = HashMap::new();
        let mut compaction_items: HashMap<String, String> = HashMap::new();
        let mut turn_usage: Option<Usage> = None;
        let mut turn_last_request_input_tokens: Option<u32> = None;
        let mut turn_status = RuntimeTurnStatus::Completed;
        let mut turn_error: Option<String> = None;
        let mut turn_summary: Option<TurnSummary> = None;

        loop {
            let event = {
                let mut rx = engine.rx_event.write().await;
                rx.recv().await
            };
            let Some(event) = event else {
                if self
                    .is_interrupt_requested(&thread_id, &turn_id)
                    .await
                    .unwrap_or(false)
                {
                    turn_status = RuntimeTurnStatus::Interrupted;
                }
                break;
            };

            match event {
                EngineEvent::TurnStarted { .. } => {
                    self.emit_event(
                        &thread_id,
                        Some(&turn_id),
                        None,
                        "turn.lifecycle",
                        json!({ "status": "in_progress" }),
                    )
                    .await?;
                }
                EngineEvent::ThinkingStarted { .. } => {
                    thinking_stream_item_id =
                        Some(format!("item_{}", &Uuid::new_v4().to_string()[..8]));
                }
                EngineEvent::ThinkingDelta { content, .. } => {
                    if let Some(ref item_id) = thinking_stream_item_id {
                        self.emit_event(
                            &thread_id,
                            Some(&turn_id),
                            Some(item_id.as_str()),
                            "item.delta",
                            json!({ "delta": content, "kind": "thinking" }),
                        )
                        .await?;
                    }
                }
                EngineEvent::ThinkingComplete { .. } => {
                    thinking_stream_item_id = None;
                }
                EngineEvent::MessageStarted { .. } => {
                    let item_id = format!("item_{}", &Uuid::new_v4().to_string()[..8]);
                    let item = TurnItemRecord {
                        schema_version: CURRENT_RUNTIME_SCHEMA_VERSION,
                        id: item_id.clone(),
                        turn_id: turn_id.clone(),
                        kind: TurnItemKind::AgentMessage,
                        status: TurnItemLifecycleStatus::InProgress,
                        summary: String::new(),
                        detail: Some(String::new()),
                        metadata: None,
                        artifact_refs: Vec::new(),
                        started_at: Some(Utc::now()),
                        ended_at: None,
                    };
                    self.save_item_and_attach_blocking(&item, &turn_id).await?;
                    self.emit_event(
                        &thread_id,
                        Some(&turn_id),
                        Some(&item_id),
                        "item.started",
                        json!({ "item": item }),
                    )
                    .await?;
                    current_message_item = Some((item_id, String::new()));
                }
                EngineEvent::MessageDelta { content, .. } => {
                    if let Some((item_id, text)) = current_message_item.as_mut() {
                        text.push_str(&content);
                        self.emit_event(
                            &thread_id,
                            Some(&turn_id),
                            Some(item_id),
                            "item.delta",
                            json!({ "delta": content, "kind": "agent_message" }),
                        )
                        .await?;
                    }
                }
                EngineEvent::MessageComplete { .. } => {
                    if let Some((item_id, text)) = current_message_item.take() {
                        let item = self
                            .update_and_save_item_blocking(&item_id, |item| {
                                item.status = TurnItemLifecycleStatus::Completed;
                                item.summary = summarize_text(&text, SUMMARY_LIMIT);
                                item.detail = Some(text);
                                item.ended_at = Some(Utc::now());
                            })
                            .await?;
                        self.emit_event(
                            &thread_id,
                            Some(&turn_id),
                            Some(&item_id),
                            "item.completed",
                            json!({ "item": item }),
                        )
                        .await?;
                    }
                    let mgr = self.clone();
                    let tid = thread_id.clone();
                    let tturn = turn_id.clone();
                    tokio::spawn(async move {
                        let _ = mgr.emit_panel_context(&tid, &tturn).await;
                    });
                }
                EngineEvent::ToolCallStarted { id, name, input } => {
                    let item_id = format!("item_{}", &Uuid::new_v4().to_string()[..8]);
                    tool_items.insert(id.clone(), item_id.clone());
                    let kind = tool_kind_for_name(&name);
                    let summary = summarize_text(&format!("{name} started"), SUMMARY_LIMIT);
                    let item = TurnItemRecord {
                        schema_version: CURRENT_RUNTIME_SCHEMA_VERSION,
                        id: item_id.clone(),
                        turn_id: turn_id.clone(),
                        kind,
                        status: TurnItemLifecycleStatus::InProgress,
                        summary,
                        detail: Some(serde_json::to_string(&input).unwrap_or_default()),
                        metadata: None,
                        artifact_refs: Vec::new(),
                        started_at: Some(Utc::now()),
                        ended_at: None,
                    };
                    self.save_item_and_attach_blocking(&item, &turn_id).await?;
                    self.emit_event(
                        &thread_id,
                        Some(&turn_id),
                        Some(&item_id),
                        "item.started",
                        json!({ "item": item, "tool": { "id": id, "name": name, "input": input } }),
                    )
                    .await?;
                }
                EngineEvent::ToolCallProgress { id, output } => {
                    if let Some(item_id) = tool_items.get(&id) {
                        self.emit_event(
                            &thread_id,
                            Some(&turn_id),
                            Some(item_id),
                            "item.delta",
                            json!({ "delta": output, "kind": "tool_call" }),
                        )
                        .await?;
                    }
                }
                EngineEvent::ToolCallComplete { id, name, result } => {
                    if let Some(item_id) = tool_items.remove(&id) {
                        let item = self
                            .update_and_save_item_blocking(&item_id, |item| {
                                let now = Utc::now();
                                item.ended_at = Some(now);
                                match &result {
                                    Ok(output) => {
                                        item.status = if output.success {
                                            TurnItemLifecycleStatus::Completed
                                        } else {
                                            TurnItemLifecycleStatus::Failed
                                        };
                                        item.summary = summarize_text(
                                            &format!("{name}: {}", output.content),
                                            SUMMARY_LIMIT,
                                        );
                                        item.detail = Some(output.content.clone());
                                        item.metadata = output.metadata.clone();
                                        item.artifact_refs =
                                            crate::tools::large_output_router::artifact_refs_from_tool_output(
                                                None,
                                                &output.content,
                                                output.metadata.as_ref(),
                                            );
                                    }
                                    Err(err) => {
                                        item.status = TurnItemLifecycleStatus::Failed;
                                        item.summary = summarize_text(
                                            &format!("{name} failed: {err}"),
                                            SUMMARY_LIMIT,
                                        );
                                        item.detail = Some(err.to_string());
                                    }
                                }
                            })
                            .await?;
                        self.emit_event(
                            &thread_id,
                            Some(&turn_id),
                            Some(&item_id),
                            if item.status == TurnItemLifecycleStatus::Completed {
                                "item.completed"
                            } else {
                                "item.failed"
                            },
                            // Include engine tool-call id so compat SSE (`tool.completed`) matches `tool.started`.
                            json!({ "item": item, "tool": { "id": id, "name": name } }),
                        )
                        .await?;

                        // Cache checklist snapshot for the WebView checklist panel
                        if matches!(
                            name.as_str(),
                            "checklist_write"
                                | "checklist_add"
                                | "checklist_update"
                                | "todo_write"
                                | "todo_add"
                                | "todo_update"
                        ) {
                            if let Ok(output) = &result {
                                if output.success {
                                    if let Some(meta) = &output.metadata {
                                        if let Some(task_updates) = meta.get("task_updates") {
                                            if let Some(checklist_json) =
                                                task_updates.get("checklist")
                                            {
                                                if let Ok(json_str) =
                                                    serde_json::to_string(checklist_json)
                                                {
                                                    self.persist_thread_checklist(
                                                        &thread_id,
                                                        &json_str,
                                                    );
                                                    let _ = self
                                                        .emit_panel_checklist(
                                                            &thread_id,
                                                            &turn_id,
                                                        )
                                                        .await;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if Self::checklist_tool_needs_panel_push(name.as_str()) {
                            let _ = self
                                .emit_panel_checklist(&thread_id, &turn_id)
                                .await;
                        }
                        if Self::scratchpad_tool_needs_panel_push(name.as_str()) {
                            if result.as_ref().is_ok_and(|o| o.success) {
                                let _ = self
                                    .emit_panel_scratchpad(&thread_id, &turn_id)
                                    .await;
                            }
                        }
                    }
                }
                EngineEvent::CompactionStarted { id, auto, message } => {
                    let item_id = format!("item_{}", &Uuid::new_v4().to_string()[..8]);
                    compaction_items.insert(id.clone(), item_id.clone());
                    let item = TurnItemRecord {
                        schema_version: CURRENT_RUNTIME_SCHEMA_VERSION,
                        id: item_id.clone(),
                        turn_id: turn_id.clone(),
                        kind: TurnItemKind::ContextCompaction,
                        status: TurnItemLifecycleStatus::InProgress,
                        summary: summarize_text(&message, SUMMARY_LIMIT),
                        detail: Some(message.clone()),
                        metadata: None,
                        artifact_refs: Vec::new(),
                        started_at: Some(Utc::now()),
                        ended_at: None,
                    };
                    self.save_item_and_attach_blocking(&item, &turn_id).await?;
                    self.emit_event(
                        &thread_id,
                        Some(&turn_id),
                        Some(&item_id),
                        "item.started",
                        json!({ "item": item, "auto": auto }),
                    )
                    .await?;
                }
                EngineEvent::CompactionCompleted {
                    id,
                    auto,
                    message,
                    messages_before,
                    messages_after,
                } => {
                    if let Some(item_id) = compaction_items.remove(&id) {
                        let item = self
                            .update_and_save_item_blocking(&item_id, |item| {
                                item.status = TurnItemLifecycleStatus::Completed;
                                item.summary = summarize_text(&message, SUMMARY_LIMIT);
                                item.detail = Some(message);
                                item.ended_at = Some(Utc::now());
                            })
                            .await?;
                        self.emit_event(
                            &thread_id,
                            Some(&turn_id),
                            Some(&item_id),
                            "item.completed",
                            json!({
                                "item": item,
                                "auto": auto,
                                "messages_before": messages_before,
                                "messages_after": messages_after,
                            }),
                        )
                        .await?;
                    }
                }
                EngineEvent::CompactionFailed { id, auto, message } => {
                    if let Some(item_id) = compaction_items.remove(&id) {
                        let item = self
                            .update_and_save_item_blocking(&item_id, |item| {
                                item.status = TurnItemLifecycleStatus::Failed;
                                item.summary = summarize_text(&message, SUMMARY_LIMIT);
                                item.detail = Some(message);
                                item.ended_at = Some(Utc::now());
                            })
                            .await?;
                        self.emit_event(
                            &thread_id,
                            Some(&turn_id),
                            Some(&item_id),
                            "item.failed",
                            json!({ "item": item, "auto": auto }),
                        )
                        .await?;
                    }
                }
                EngineEvent::CycleAdvanced { from, to, briefing } => {
                    // Surface the cycle boundary in the runtime event timeline so
                    // background-task subscribers and replay see it. The actual
                    // archive write is the engine's responsibility (see
                    // `cycle_manager::archive_cycle`); this event is informational.
                    self.emit_event(
                        &thread_id,
                        Some(&turn_id),
                        None,
                        "cycle.advanced",
                        json!({
                            "from": from,
                            "to": to,
                            "briefing_tokens": briefing.token_estimate,
                            "cycle": briefing.cycle,
                            "timestamp": briefing.timestamp,
                        }),
                    )
                    .await?;
                }
                EngineEvent::CoherenceState {
                    state,
                    label,
                    description,
                    reason,
                } => {
                    let mut thread = self.store.load_thread(&thread_id)?;
                    thread.coherence_state = state;
                    thread.updated_at = Utc::now();
                    {
                        let store = self.store.clone();
                        let thread_clone = thread.clone();
                        tokio::task::spawn_blocking(move || store.save_thread(&thread_clone))
                            .await
                            .map_err(|e| anyhow!("save thread panicked: {e}"))??;
                    }
                    self.emit_event(
                        &thread_id,
                        Some(&turn_id),
                        None,
                        "coherence.state",
                        json!({
                            "state": state,
                            "label": label,
                            "description": description,
                            "reason": reason,
                            "thread": thread,
                        }),
                    )
                    .await?;
                }
                EngineEvent::CapacityDecision {
                    risk_band,
                    action,
                    reason,
                    ..
                } => {
                    let message = format!(
                        "Capacity decision: risk={risk_band} action={action} reason={reason}"
                    );
                    let item = TurnItemRecord {
                        schema_version: CURRENT_RUNTIME_SCHEMA_VERSION,
                        id: format!("item_{}", &Uuid::new_v4().to_string()[..8]),
                        turn_id: turn_id.clone(),
                        kind: TurnItemKind::Status,
                        status: TurnItemLifecycleStatus::Completed,
                        summary: summarize_text(&message, SUMMARY_LIMIT),
                        detail: Some(message),
                        metadata: None,
                        artifact_refs: Vec::new(),
                        started_at: Some(Utc::now()),
                        ended_at: Some(Utc::now()),
                    };
                    self.save_item_and_attach_blocking(&item, &turn_id).await?;
                    self.emit_event(
                        &thread_id,
                        Some(&turn_id),
                        Some(&item.id),
                        "item.completed",
                        json!({ "item": item }),
                    )
                    .await?;
                }
                EngineEvent::CapacityIntervention {
                    action,
                    before_prompt_tokens,
                    after_prompt_tokens,
                    replay_outcome,
                    replan_performed,
                    ..
                } => {
                    let message = format!(
                        "Capacity intervention: {action} (~{before_prompt_tokens} -> ~{after_prompt_tokens}) replay={:?} replan={replan_performed}",
                        replay_outcome
                    );
                    let item = TurnItemRecord {
                        schema_version: CURRENT_RUNTIME_SCHEMA_VERSION,
                        id: format!("item_{}", &Uuid::new_v4().to_string()[..8]),
                        turn_id: turn_id.clone(),
                        kind: TurnItemKind::Status,
                        status: TurnItemLifecycleStatus::Completed,
                        summary: summarize_text(&message, SUMMARY_LIMIT),
                        detail: Some(message),
                        metadata: None,
                        artifact_refs: Vec::new(),
                        started_at: Some(Utc::now()),
                        ended_at: Some(Utc::now()),
                    };
                    self.save_item_and_attach_blocking(&item, &turn_id).await?;
                    self.emit_event(
                        &thread_id,
                        Some(&turn_id),
                        Some(&item.id),
                        "item.completed",
                        json!({ "item": item }),
                    )
                    .await?;
                }
                EngineEvent::CapacityMemoryPersistFailed { action, error, .. } => {
                    let message =
                        format!("Capacity memory persist failed: action={action} error={error}");
                    let item = TurnItemRecord {
                        schema_version: CURRENT_RUNTIME_SCHEMA_VERSION,
                        id: format!("item_{}", &Uuid::new_v4().to_string()[..8]),
                        turn_id: turn_id.clone(),
                        kind: TurnItemKind::Status,
                        status: TurnItemLifecycleStatus::Failed,
                        summary: summarize_text(&message, SUMMARY_LIMIT),
                        detail: Some(message),
                        metadata: None,
                        artifact_refs: Vec::new(),
                        started_at: Some(Utc::now()),
                        ended_at: Some(Utc::now()),
                    };
                    self.save_item_and_attach_blocking(&item, &turn_id).await?;
                    self.emit_event(
                        &thread_id,
                        Some(&turn_id),
                        Some(&item.id),
                        "item.failed",
                        json!({ "item": item }),
                    )
                    .await?;
                }
                EngineEvent::AgentSpawned { id, prompt } => {
                    let message = format!(
                        "Sub-agent {id} spawned: {}",
                        summarize_text(&prompt, SUMMARY_LIMIT)
                    );
                    let item = TurnItemRecord {
                        schema_version: CURRENT_RUNTIME_SCHEMA_VERSION,
                        id: format!("item_{}", &Uuid::new_v4().to_string()[..8]),
                        turn_id: turn_id.clone(),
                        kind: TurnItemKind::Status,
                        status: TurnItemLifecycleStatus::Completed,
                        summary: summarize_text(&message, SUMMARY_LIMIT),
                        detail: Some(message),
                        metadata: None,
                        artifact_refs: Vec::new(),
                        started_at: Some(Utc::now()),
                        ended_at: Some(Utc::now()),
                    };
                    self.save_item_and_attach_blocking(&item, &turn_id).await?;
                    self.emit_event(
                        &thread_id,
                        Some(&turn_id),
                        Some(&item.id),
                        "agent.spawned",
                        json!({ "item": item, "agent_id": id, "prompt": prompt }),
                    )
                    .await?;
                }
                EngineEvent::AgentProgress { id, status } => {
                    let message = format!("Sub-agent {id}: {status}");
                    let item = TurnItemRecord {
                        schema_version: CURRENT_RUNTIME_SCHEMA_VERSION,
                        id: format!("item_{}", &Uuid::new_v4().to_string()[..8]),
                        turn_id: turn_id.clone(),
                        kind: TurnItemKind::Status,
                        status: TurnItemLifecycleStatus::Completed,
                        summary: summarize_text(&message, SUMMARY_LIMIT),
                        detail: Some(message),
                        metadata: None,
                        artifact_refs: Vec::new(),
                        started_at: Some(Utc::now()),
                        ended_at: Some(Utc::now()),
                    };
                    self.save_item_and_attach_blocking(&item, &turn_id).await?;
                    self.emit_event(
                        &thread_id,
                        Some(&turn_id),
                        Some(&item.id),
                        "agent.progress",
                        json!({ "item": item, "agent_id": id }),
                    )
                    .await?;
                }
                EngineEvent::AgentComplete { id, result } => {
                    let message = format!(
                        "Sub-agent {id} completed: {}",
                        summarize_text(&result, SUMMARY_LIMIT)
                    );
                    let item = TurnItemRecord {
                        schema_version: CURRENT_RUNTIME_SCHEMA_VERSION,
                        id: format!("item_{}", &Uuid::new_v4().to_string()[..8]),
                        turn_id: turn_id.clone(),
                        kind: TurnItemKind::Status,
                        status: TurnItemLifecycleStatus::Completed,
                        summary: summarize_text(&message, SUMMARY_LIMIT),
                        detail: Some(message),
                        metadata: None,
                        artifact_refs: Vec::new(),
                        started_at: Some(Utc::now()),
                        ended_at: Some(Utc::now()),
                    };
                    self.save_item_and_attach_blocking(&item, &turn_id).await?;
                    self.emit_event(
                        &thread_id,
                        Some(&turn_id),
                        Some(&item.id),
                        "agent.completed",
                        json!({ "item": item, "agent_id": id }),
                    )
                    .await?;
                }
                EngineEvent::CraftVerdict {
                    agent_id,
                    agent_type,
                    task_id,
                    verdict,
                    summary,
                    items,
                } => {
                    self.emit_event(
                        &thread_id,
                        Some(&turn_id),
                        None,
                        "craft.verdict",
                        json!({
                            "agent_id": agent_id,
                            "agent_type": agent_type,
                            "task_id": task_id,
                            "verdict": verdict,
                            "summary": summary,
                            "items": items,
                        }),
                    )
                    .await?;
                }
                EngineEvent::CraftBoardUpdated {
                    task_id,
                    partition,
                    agent_id,
                } => {
                    self.emit_event(
                        &thread_id,
                        Some(&turn_id),
                        None,
                        "craft.board_updated",
                        json!({
                            "task_id": task_id,
                            "partition": partition,
                            "agent_id": agent_id,
                        }),
                    )
                    .await?;
                }
                EngineEvent::AgentList { agents } => {
                    let running = agents
                        .iter()
                        .filter(|agent| matches!(agent.status, SubAgentStatus::Running))
                        .count();
                    let interrupted = agents
                        .iter()
                        .filter(|agent| matches!(agent.status, SubAgentStatus::Interrupted(_)))
                        .count();
                    let completed = agents
                        .iter()
                        .filter(|agent| matches!(agent.status, SubAgentStatus::Completed))
                        .count();
                    let message = format!(
                        "Sub-agent list refreshed: {} total ({running} running, {interrupted} interrupted, {completed} completed)",
                        agents.len()
                    );
                    let item = TurnItemRecord {
                        schema_version: CURRENT_RUNTIME_SCHEMA_VERSION,
                        id: format!("item_{}", &Uuid::new_v4().to_string()[..8]),
                        turn_id: turn_id.clone(),
                        kind: TurnItemKind::Status,
                        status: TurnItemLifecycleStatus::Completed,
                        summary: summarize_text(&message, SUMMARY_LIMIT),
                        detail: Some(message),
                        metadata: None,
                        artifact_refs: Vec::new(),
                        started_at: Some(Utc::now()),
                        ended_at: Some(Utc::now()),
                    };
                    self.save_item_and_attach_blocking(&item, &turn_id).await?;
                    self.emit_event(
                        &thread_id,
                        Some(&turn_id),
                        Some(&item.id),
                        "agent.list",
                        json!({ "item": item, "agents": agents }),
                    )
                    .await?;
                }
                EngineEvent::ApprovalRequired {
                    id,
                    tool_name,
                    description,
                    ..
                } => {
                    if self
                        .active_turn_flags(&thread_id, &turn_id)
                        .await
                        .is_none()
                    {
                        let _ = engine.deny_tool_call(id).await;
                        continue;
                    }
                    let (auto_approve, trust_mode) = self
                        .active_turn_flags(&thread_id, &turn_id)
                        .await
                        .unwrap_or((false, false));
                    match Self::approval_decision(auto_approve, trust_mode, false) {
                        RuntimeApprovalDecision::ApproveTool => {
                            let _ = engine.approve_tool_call(id).await;
                        }
                        RuntimeApprovalDecision::DenyTool
                        | RuntimeApprovalDecision::RetryWithFullAccess => {
                            // Register pending before SSE/JSONL emit so HTTP
                            // `resolve-approval` cannot race `approval.required`.
                            let timeout_secs = self.manager_cfg.http_approval_timeout_secs.max(1);
                            let deadline = tokio::time::Instant::now()
                                + std::time::Duration::from_secs(timeout_secs);
                            {
                                let mut active = self.active.lock().await;
                                active.pending_approvals.insert(
                                    id.clone(),
                                    PendingApproval {
                                        thread_id: thread_id.clone(),
                                        turn_id: turn_id.clone(),
                                        tool_call_id: id.clone(),
                                        deadline,
                                    },
                                );
                            }

                            self.emit_event(
                                &thread_id,
                                Some(&turn_id),
                                None,
                                "approval.required",
                                json!({
                                    "id": id,
                                    "tool_name": tool_name,
                                    "description": description,
                                }),
                            )
                            .await?;

                            let this = self.clone();
                            let engine_handle = engine.clone();
                            let tool_id = id.clone();
                            tokio::spawn(async move {
                                tokio::time::sleep_until(deadline).await;
                                let mut active = this.active.lock().await;
                                if active.pending_approvals.remove(&tool_id).is_some() {
                                    drop(active);
                                    let _ = engine_handle.deny_tool_call(tool_id).await;
                                }
                            });
                        }
                    }
                }
                EngineEvent::ElevationRequired {
                    tool_id,
                    tool_name,
                    denial_reason,
                    ..
                } => {
                    self.emit_event(
                        &thread_id,
                        Some(&turn_id),
                        None,
                        "sandbox.denied",
                        json!({
                            "tool_id": tool_id,
                            "tool_name": tool_name,
                            "reason": denial_reason,
                        }),
                    )
                    .await?;
                    let (auto_approve, trust_mode) = self
                        .active_turn_flags(&thread_id, &turn_id)
                        .await
                        .unwrap_or((false, false));
                    match Self::approval_decision(auto_approve, trust_mode, true) {
                        RuntimeApprovalDecision::RetryWithFullAccess => {
                            let _ = engine
                                .retry_tool_with_policy(
                                    tool_id,
                                    crate::sandbox::SandboxPolicy::DangerFullAccess,
                                )
                                .await;
                        }
                        RuntimeApprovalDecision::ApproveTool
                        | RuntimeApprovalDecision::DenyTool => {
                            let _ = engine.deny_tool_call(tool_id).await;
                        }
                    }
                }
                EngineEvent::Status { message } => {
                    let item = TurnItemRecord {
                        schema_version: CURRENT_RUNTIME_SCHEMA_VERSION,
                        id: format!("item_{}", &Uuid::new_v4().to_string()[..8]),
                        turn_id: turn_id.clone(),
                        kind: TurnItemKind::Status,
                        status: TurnItemLifecycleStatus::Completed,
                        summary: summarize_text(&message, SUMMARY_LIMIT),
                        detail: Some(message.clone()),
                        metadata: None,
                        artifact_refs: Vec::new(),
                        started_at: Some(Utc::now()),
                        ended_at: Some(Utc::now()),
                    };
                    self.save_item_and_attach_blocking(&item, &turn_id).await?;
                    self.emit_event(
                        &thread_id,
                        Some(&turn_id),
                        Some(&item.id),
                        "item.completed",
                        json!({ "item": item }),
                    )
                    .await?;
                }
                EngineEvent::Error { envelope, .. } => {
                    turn_status = RuntimeTurnStatus::Failed;
                    turn_error = Some(envelope.message.clone());
                    let message = envelope.message.clone();
                    let item = TurnItemRecord {
                        schema_version: CURRENT_RUNTIME_SCHEMA_VERSION,
                        id: format!("item_{}", &Uuid::new_v4().to_string()[..8]),
                        turn_id: turn_id.clone(),
                        kind: TurnItemKind::Error,
                        status: TurnItemLifecycleStatus::Failed,
                        summary: summarize_text(&message, SUMMARY_LIMIT),
                        detail: Some(message),
                        metadata: None,
                        artifact_refs: Vec::new(),
                        started_at: Some(Utc::now()),
                        ended_at: Some(Utc::now()),
                    };
                    self.save_item_and_attach_blocking(&item, &turn_id).await?;
                    self.emit_event(
                        &thread_id,
                        Some(&turn_id),
                        Some(&item.id),
                        "item.failed",
                        json!({ "item": item }),
                    )
                    .await?;
                }
                EngineEvent::TurnComplete {
                    usage,
                    last_request_input_tokens,
                    status,
                    error,
                    step_count,
                    tool_names,
                    end_reason,
                } => {
                    turn_usage = Some(usage);
                    turn_last_request_input_tokens = last_request_input_tokens;
                    turn_status = match status {
                        TurnOutcomeStatus::Completed => RuntimeTurnStatus::Completed,
                        TurnOutcomeStatus::Interrupted => RuntimeTurnStatus::Interrupted,
                        TurnOutcomeStatus::Failed => RuntimeTurnStatus::Failed,
                    };
                    if let Some(err) = error {
                        turn_error = Some(err);
                    }
                    let summary = TurnSummary::new(step_count, tool_names, end_reason);
                    summary.log_turn_complete(&turn_id, status, Some(&thread_id));
                    turn_summary = Some(summary);
                    let _ = self.emit_panel_context(&thread_id, &turn_id).await;
                    let _ = self.emit_panel_scratchpad(&thread_id, &turn_id).await;
                    let _ = self.emit_panel_checklist(&thread_id, &turn_id).await;
                    break;
                }
                _ => {}
            }
        }

        if self
            .is_interrupt_requested(&thread_id, &turn_id)
            .await
            .unwrap_or(false)
        {
            turn_status = RuntimeTurnStatus::Interrupted;
        }

        if let Some((item_id, text)) = current_message_item.take() {
            let mut item = self.store.load_item(&item_id)?;
            if turn_status == RuntimeTurnStatus::Interrupted {
                item.status = TurnItemLifecycleStatus::Interrupted;
            } else {
                item.status = TurnItemLifecycleStatus::Completed;
            }
            item.summary = summarize_text(&text, SUMMARY_LIMIT);
            item.detail = Some(text);
            item.ended_at = Some(Utc::now());
            {
                let store = self.store.clone();
                let item_clone = item.clone();
                tokio::task::spawn_blocking(move || store.save_item(&item_clone))
                    .await
                    .map_err(|e| anyhow!("save item panicked: {e}"))??;
            }
            self.emit_event(
                &thread_id,
                Some(&turn_id),
                Some(&item_id),
                if item.status == TurnItemLifecycleStatus::Interrupted {
                    "item.interrupted"
                } else {
                    "item.completed"
                },
                json!({ "item": item }),
            )
            .await?;
        }

        let ended_at = Utc::now();
        let mut turn = self.store.load_turn(&turn_id)?;
        turn.status = turn_status;
        turn.ended_at = Some(ended_at);
        turn.duration_ms = turn.started_at.map(|start| duration_ms(start, ended_at));
        turn.usage = turn_usage;
        turn.last_request_input_tokens = turn_last_request_input_tokens;
        turn.error = turn_error;

        let mut thread = self.get_thread(&thread_id).await?;
        thread.latest_turn_id = Some(turn_id.clone());
        thread.updated_at = Utc::now();

        {
            let store = self.store.clone();
            let turn_clone = turn.clone();
            let thread_clone = thread.clone();
            tokio::task::spawn_blocking(move || -> Result<()> {
                store.save_turn(&turn_clone)?;
                store.save_thread(&thread_clone)?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow!("save turn completion panicked: {e}"))??;
        }

        self.emit_event(
            &thread_id,
            Some(&turn_id),
            None,
            "turn.completed",
            {
                let mut payload = json!({ "turn": turn.clone() });
                if let Some(ref summary) = turn_summary {
                    if let Some(obj) = payload.as_object_mut() {
                        obj.insert("turn_summary".to_string(), summary.to_value());
                    }
                }
                payload
            },
        )
        .await?;

        {
            let mut active = self.active.lock().await;
            if let Some(state) = active.engines.get_mut(&thread_id)
                && state
                    .active_turn
                    .as_ref()
                    .is_some_and(|t| t.turn_id == turn_id)
            {
                state.active_turn = None;
            }
            touch_lru(&mut active.lru, &thread_id);
        }

        Ok(())
    }

    async fn save_item_and_attach_blocking(
        &self,
        item: &TurnItemRecord,
        turn_id: &str,
    ) -> Result<()> {
        let store = self.store.clone();
        let item = item.clone();
        let turn_id = turn_id.to_string();
        tokio::task::spawn_blocking(move || -> Result<()> {
            store.save_item(&item)?;
            let mut turn = store.load_turn(&turn_id)?;
            if !turn.item_ids.iter().any(|id| id == &item.id) {
                turn.item_ids.push(item.id.clone());
                store.save_turn(&turn)?;
            }
            Ok(())
        })
        .await
        .map_err(|e| anyhow!("save_item_and_attach panicked: {e}"))?
    }

    async fn update_and_save_item_blocking(
        &self,
        item_id: &str,
        update_fn: impl FnOnce(&mut TurnItemRecord),
    ) -> Result<TurnItemRecord> {
        let store = self.store.clone();
        let item_id = item_id.to_string();
        let mut item = tokio::task::spawn_blocking(move || store.load_item(&item_id))
            .await
            .map_err(|e| anyhow!("load_item panicked: {e}"))??;
        update_fn(&mut item);
        let store = self.store.clone();
        let item_clone = item.clone();
        tokio::task::spawn_blocking(move || store.save_item(&item_clone))
            .await
            .map_err(|e| anyhow!("save_item panicked: {e}"))??;
        Ok(item)
    }

    async fn is_interrupt_requested(&self, thread_id: &str, turn_id: &str) -> Result<bool> {
        let active = self.active.lock().await;
        let Some(state) = active.engines.get(thread_id) else {
            return Ok(false);
        };
        let Some(turn) = state.active_turn.as_ref() else {
            return Ok(false);
        };
        Ok(turn.turn_id == turn_id && turn.interrupt_requested)
    }

}
