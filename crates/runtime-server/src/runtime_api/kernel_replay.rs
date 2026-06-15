//! Kernel turn replay API — load persisted events and verify coherence (P3B-6b).

use axum::Json;
use axum::extract::Path;
use serde::Serialize;

use zagens_core::engine::turn_machine::{replay_turn_projection, verify_turn_replay_coherence};
use zagens_runtime_adapters::persist::KernelEventWriter;

use super::ApiError;

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
    let report = replay_turn_projection(&events);
    let coherence_error = verify_turn_replay_coherence(&events, None);
    let outcome = report.outcome.as_ref().map(|o| format!("{o:?}"));
    Ok(Json(KernelTurnReplayResponse {
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
    }))
}

/// Best-effort replay sanity when resuming a session-linked thread (observability only).
pub(crate) fn log_kernel_replay_for_turn(turn_id: &str) {
    let Some(writer) = KernelEventWriter::try_open_default() else {
        return;
    };
    let Ok(events) = writer.load_turn_events_sync(turn_id) else {
        return;
    };
    if events.is_empty() {
        return;
    }
    if let Some(summary) = verify_turn_replay_coherence(&events, None) {
        eprintln!("[resume-session] kernel replay coherence diff for turn {turn_id}: {summary}");
    }
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
    }
}
