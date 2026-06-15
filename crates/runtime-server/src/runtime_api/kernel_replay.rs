//! Kernel turn/thread replay API — load persisted events and verify coherence (P3B-6b/6c).

use axum::Json;
use axum::extract::{Path, State};
use serde::Serialize;

use zagens_core::engine::turn_machine::{
    ThreadReplayProjection, replay_thread_projection, replay_turn_projection,
    verify_session_message_coverage, verify_turn_replay_coherence,
};
use zagens_runtime_adapters::persist::KernelEventWriter;
use zagens_runtime_api::ResumeSessionKernelReplay;

use super::ApiError;
use super::RuntimeApiState;
use crate::runtime_threads::RuntimeThreadManager;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct KernelTurnReplayResponse {
    turn_id: String,
    event_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    outcome: Option<String>,
    coherence_ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    coherence_error: Option<String>,
    scratchpad_summary_injected: bool,
    scratchpad_reminder_count: u32,
    compaction_artifact_count: u32,
    cycle_briefing_count: u32,
    step_limit_continuations: u32,
    loop_guard_continuations: u32,
    cycle_handoff_attempts: u32,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct KernelThreadTurnReplayEntry {
    turn_id: String,
    event_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    outcome: Option<String>,
    coherence_ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    coherence_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct KernelThreadMessageReplayStats {
    model_request_count: u32,
    model_message_count: u32,
    tool_call_planned_count: u32,
    steer_injection_count: u32,
    compaction_artifact_count: u32,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct KernelThreadLatestProjection {
    turn_id: String,
    step_idx: u32,
    max_steps: u32,
    scratchpad_summary_injected: bool,
    active_tool_count: u32,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct KernelThreadReplayResponse {
    thread_id: String,
    turn_count: usize,
    turns_with_events: usize,
    turns_coherent: usize,
    all_coherent: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    latest_projection: Option<KernelThreadLatestProjection>,
    message_stats: KernelThreadMessageReplayStats,
    turns: Vec<KernelThreadTurnReplayEntry>,
}

fn turn_replay_response(
    turn_id: String,
    events: Vec<zagens_core::engine::kernel_event::KernelEvent>,
) -> KernelTurnReplayResponse {
    let report = replay_turn_projection(&events);
    let coherence_error = verify_turn_replay_coherence(&events, None);
    let outcome = report.outcome.as_ref().map(|o| format!("{o:?}"));
    KernelTurnReplayResponse {
        turn_id,
        event_count: report.event_count,
        outcome,
        coherence_ok: coherence_error.is_none(),
        coherence_error,
        scratchpad_summary_injected: report.projection.scratchpad_summary_injected,
        scratchpad_reminder_count: report.projection.scratchpad_reminder_count,
        compaction_artifact_count: report.projection.compaction_artifact_count,
        cycle_briefing_count: report.projection.cycle_briefing_count,
        step_limit_continuations: report.projection.step_limit_continuations,
        loop_guard_continuations: report.projection.loop_guard_continuations,
        cycle_handoff_attempts: report.projection.cycle_handoff_attempts,
    }
}

fn list_thread_turn_ids(
    manager: &RuntimeThreadManager,
    thread_id: &str,
) -> Result<Vec<String>, ApiError> {
    manager
        .load_thread_sync(thread_id)
        .map_err(|e| ApiError::not_found(format!("thread not found: {e}")))?;
    manager
        .store
        .list_turns_for_thread(thread_id)
        .map(|turns| turns.into_iter().map(|t| t.id).collect())
        .map_err(|e| ApiError::internal(format!("list turns: {e}")))
}

fn load_thread_turn_events(
    turn_ids: &[String],
) -> Option<Vec<(String, Vec<zagens_core::engine::kernel_event::KernelEvent>)>> {
    let writer = KernelEventWriter::try_open_default()?;
    let mut turn_events = Vec::with_capacity(turn_ids.len());
    for turn_id in turn_ids {
        let events = writer.load_turn_events_sync(turn_id).unwrap_or_default();
        turn_events.push((turn_id.clone(), events));
    }
    Some(turn_events)
}

/// Load persisted kernel events for all turns on a thread and build replay projection.
pub(crate) fn collect_thread_kernel_replay(
    manager: &RuntimeThreadManager,
    thread_id: &str,
) -> Result<ThreadReplayProjection, ApiError> {
    let turn_ids = list_thread_turn_ids(manager, thread_id)?;
    let turn_events = load_thread_turn_events(&turn_ids)
        .ok_or_else(|| ApiError::internal("kernel event log unavailable".to_string()))?;
    Ok(replay_thread_projection(thread_id, &turn_events))
}

pub(crate) fn resume_session_kernel_replay_summary(
    manager: &RuntimeThreadManager,
    thread_id: &str,
    session_message_count: Option<usize>,
) -> Option<ResumeSessionKernelReplay> {
    let projection = collect_thread_kernel_replay(manager, thread_id).ok()?;
    if projection.report.turns_with_events == 0 {
        return None;
    }
    if let Some(count) = session_message_count {
        log_session_message_coverage(count, &projection);
    }
    let report = projection.report;
    Some(ResumeSessionKernelReplay {
        turn_count: report.turn_count,
        turns_with_events: report.turns_with_events,
        turns_coherent: report.turns_coherent,
        all_coherent: report.all_coherent,
        latest_turn_id: projection.latest_turn_id,
        latest_step_idx: Some(projection.latest_projection.step_idx),
        latest_max_steps: Some(projection.latest_projection.max_steps),
        active_tool_count: Some(projection.latest_projection.active_tool_names.len() as u32),
        model_message_count: Some(projection.message_stats.model_message_count),
        tool_call_planned_count: Some(projection.message_stats.tool_call_planned_count),
    })
}

/// Log when session JSON row count looks thinner than kernel model_message events.
pub(crate) fn log_session_message_coverage(
    session_message_count: usize,
    projection: &ThreadReplayProjection,
) {
    if let Some(summary) =
        verify_session_message_coverage(session_message_count, &projection.message_stats)
    {
        eprintln!(
            "[resume-session] kernel message coverage diff (thread {}): {summary}",
            projection.report.thread_id
        );
    }
}

pub(crate) async fn get_kernel_turn_replay(
    Path(turn_id): Path<String>,
) -> Result<Json<KernelTurnReplayResponse>, ApiError> {
    let writer = KernelEventWriter::try_open_default()
        .ok_or_else(|| ApiError::internal("kernel event log unavailable".to_string()))?;
    let events = writer
        .load_turn_events_sync(&turn_id)
        .map_err(|e| ApiError::internal(format!("load kernel events: {e}")))?;
    if events.is_empty() {
        return Err(ApiError::not_found(format!(
            "no kernel events for turn {turn_id}"
        )));
    }
    Ok(Json(turn_replay_response(turn_id, events)))
}

pub(crate) async fn get_kernel_thread_replay(
    State(state): State<RuntimeApiState>,
    Path(thread_id): Path<String>,
) -> Result<Json<KernelThreadReplayResponse>, ApiError> {
    let manager = state.runtime_threads.clone();
    let thread_id_for_load = thread_id.clone();
    let projection = tokio::task::spawn_blocking(move || {
        collect_thread_kernel_replay(manager.as_ref(), &thread_id_for_load)
    })
    .await
    .map_err(|e| ApiError::internal(format!("kernel thread replay task panicked: {e}")))??;

    let report = projection.report;
    let turns = report
        .turns
        .into_iter()
        .map(|entry| KernelThreadTurnReplayEntry {
            turn_id: entry.turn_id,
            event_count: entry.event_count,
            outcome: entry.outcome.as_ref().map(|o| format!("{o:?}")),
            coherence_ok: entry.coherence_ok,
            coherence_error: entry.coherence_error,
        })
        .collect();

    let latest_projection =
        projection
            .latest_turn_id
            .as_ref()
            .map(|turn_id| KernelThreadLatestProjection {
                turn_id: turn_id.clone(),
                step_idx: projection.latest_projection.step_idx,
                max_steps: projection.latest_projection.max_steps,
                scratchpad_summary_injected: projection
                    .latest_projection
                    .scratchpad_summary_injected,
                active_tool_count: projection.latest_projection.active_tool_names.len() as u32,
            });
    let message_stats = KernelThreadMessageReplayStats {
        model_request_count: projection.message_stats.model_request_count,
        model_message_count: projection.message_stats.model_message_count,
        tool_call_planned_count: projection.message_stats.tool_call_planned_count,
        steer_injection_count: projection.message_stats.steer_injection_count,
        compaction_artifact_count: projection.message_stats.compaction_artifact_count,
    };

    Ok(Json(KernelThreadReplayResponse {
        thread_id: report.thread_id,
        turn_count: report.turn_count,
        turns_with_events: report.turns_with_events,
        turns_coherent: report.turns_coherent,
        all_coherent: report.all_coherent,
        latest_projection,
        message_stats,
        turns,
    }))
}

/// Best-effort replay sanity when resuming a session-linked thread (observability only).
pub(crate) fn log_kernel_replay_for_turn(turn_id: &str) {
    let ids = [turn_id.to_string()];
    log_kernel_replay_for_turns(&ids);
}

/// Verify coherence for every turn id in the slice (skips missing/empty logs).
pub(crate) fn log_kernel_replay_for_turns(turn_ids: &[String]) {
    let Some(writer) = KernelEventWriter::try_open_default() else {
        return;
    };
    for turn_id in turn_ids {
        let Ok(events) = writer.load_turn_events_sync(turn_id) else {
            continue;
        };
        if events.is_empty() {
            continue;
        }
        if let Some(summary) = verify_turn_replay_coherence(&events, None) {
            eprintln!(
                "[resume-session] kernel replay coherence diff for turn {turn_id}: {summary}"
            );
        }
    }
}

/// Load all turn ids for a thread and run kernel replay observability checks.
pub(crate) fn log_kernel_replay_for_thread(manager: &RuntimeThreadManager, thread_id: &str) {
    let Ok(turns) = manager.store.list_turns_for_thread(thread_id) else {
        return;
    };
    let turn_ids: Vec<String> = turns.into_iter().map(|t| t.id).collect();
    if turn_ids.is_empty() {
        return;
    }
    log_kernel_replay_for_turns(&turn_ids);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use zagens_core::engine::kernel_event::{KernelEvent, TurnOutcome};
    use zagens_core::turn::TurnLoopMode;

    #[tokio::test]
    async fn get_kernel_turn_replay_round_trip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path: PathBuf = dir.path().join("sessions.db");
        let writer = KernelEventWriter::try_open(&db_path).expect("open");
        let sink = writer.sink();
        sink.send(KernelEvent::TurnStarted {
            turn_id: "t-replay-api".into(),
            mode: TurnLoopMode::Agent,
            input_text: "hi".into(),
            max_steps: 5,
        })
        .expect("send");
        sink.send(KernelEvent::TurnEnded {
            turn_id: "t-replay-api".into(),
            outcome: TurnOutcome::Completed,
            total_steps: 1,
        })
        .expect("send");
        drop(sink);
        drop(writer);
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // API helper uses default path — call load directly for unit test.
        let writer2 = KernelEventWriter::try_open(&db_path).expect("reopen");
        let events = writer2.load_turn_events_sync("t-replay-api").expect("load");
        assert_eq!(events.len(), 2);
        assert!(verify_turn_replay_coherence(&events, None).is_none());
        let resp = turn_replay_response("t-replay-api".into(), events);
        assert!(resp.coherence_ok);
        assert_eq!(resp.event_count, 2);
    }
}
