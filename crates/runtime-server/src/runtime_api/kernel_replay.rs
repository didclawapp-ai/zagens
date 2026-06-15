//! Kernel turn/thread replay API — load persisted events and verify coherence (P3B-6b/6c).

use axum::Json;
use axum::extract::{Path, Query, State};
use serde::{Deserialize, Serialize};

use zagens_core::engine::turn_machine::{
    SessionMessageRoleIndex, ThreadReplayProjection, build_session_compaction_artifact_index,
    build_session_message_coverage, build_session_message_role_index,
    build_session_message_timeline_coverage, replay_thread_projection, replay_turn_projection,
    verify_message_timeline_coherence, verify_turn_replay_coherence,
};

use crate::core::engine::kernel_compaction_artifact_shadow::record_message_compaction_artifact_check;
use crate::core::engine::kernel_continuation_anchor_shadow::record_continuation_anchor_check;
use crate::core::engine::kernel_message_compaction_shadow::record_message_compaction_depth_check;
use crate::core::engine::kernel_message_coverage_shadow::record_message_coverage_check;
use crate::core::engine::kernel_message_memory_plane_shadow::record_message_memory_plane_check;
use crate::core::engine::kernel_message_role_shadow::record_message_role_index_check;
use crate::core::engine::kernel_message_timeline_shadow::record_timeline_coherence_check;
use crate::core::engine::kernel_notify_lsp_anchor_shadow::record_notify_lsp_anchor_check;
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
    scratchpad_summary_count: u32,
    scratchpad_reminder_count: u32,
    cycle_briefing_count: u32,
    step_limit_continuation_count: u32,
    loop_guard_continuation_count: u32,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct KernelThreadCompactionReplayEntry {
    turn_id: String,
    artifact_id: String,
    replaced_from: u32,
    replaced_to: u32,
    messages_removed_count: u32,
    summary_token_count: u32,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct KernelThreadCompactionReplayIndex {
    artifact_count: u32,
    messages_removed_estimate: u32,
    peak_session_depth_hint: u32,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct KernelThreadMessageTimelineEntry {
    turn_id: String,
    step_idx: u32,
    block_count: u32,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct KernelThreadMessageCoverage {
    session_message_count: usize,
    kernel_model_message_count: u32,
    coverage_ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    summary: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct KernelThreadMessageTimelineCoverage {
    session_message_count: usize,
    kernel_model_message_count: u32,
    timeline_anchor_count: usize,
    model_request_count: u32,
    coherence_ok: bool,
    coverage_ok: bool,
    timeline_vs_session_ok: bool,
    timeline_vs_requests_ok: bool,
    estimated_min_session_messages: u32,
    plane_depth_ok: bool,
    role_index_ok: bool,
    memory_plane_user_ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    session_assistant_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    session_tool_result_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    session_text_user_count: Option<u32>,
    kernel_min_assistant_messages: u32,
    kernel_min_tool_result_messages: u32,
    kernel_min_memory_injected_user_messages: u32,
    compaction_depth_ok: bool,
    compaction_messages_removed_estimate: u32,
    compaction_restored_session_estimate: u32,
    compaction_peak_session_depth_hint: u32,
    compaction_artifact_ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    session_compaction_artifact_count: Option<u32>,
    continuation_anchor_ok: bool,
    notify_lsp_anchor_ok: bool,
    overall_ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    summary: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct KernelThreadMessagePlaneIndex {
    model_request_count: u32,
    model_message_count: u32,
    tool_call_planned_count: u32,
    steer_injection_count: u32,
    estimated_min_session_messages: u32,
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
    message_timeline: Vec<KernelThreadMessageTimelineEntry>,
    message_plane_index: KernelThreadMessagePlaneIndex,
    compaction_timeline: Vec<KernelThreadCompactionReplayEntry>,
    compaction_index: KernelThreadCompactionReplayIndex,
    #[serde(skip_serializing_if = "Option::is_none")]
    message_coverage: Option<KernelThreadMessageCoverage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message_timeline_coverage: Option<KernelThreadMessageTimelineCoverage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    continuation_anchor_ok: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    continuation_anchor_summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    notify_lsp_anchor_ok: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    notify_lsp_anchor_summary: Option<String>,
    turns: Vec<KernelThreadTurnReplayEntry>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct KernelThreadReplayQuery {
    /// Optional session JSON message count for coverage comparison (observability).
    pub session_message_count: Option<usize>,
    /// Optional session assistant row count (`?session_assistant_count=N`).
    pub session_assistant_count: Option<usize>,
    /// Optional session tool-result row count (`?session_tool_result_count=N`).
    pub session_tool_result_count: Option<usize>,
    /// Optional session text-only user row count (`?session_text_user_count=N`).
    pub session_text_user_count: Option<usize>,
}

fn role_index_for_replay(
    messages: Option<&[zagens_core::chat::Message]>,
    query: &KernelThreadReplayQuery,
) -> Option<SessionMessageRoleIndex> {
    if let Some(messages) = messages {
        return Some(build_session_message_role_index(messages));
    }
    if query.session_assistant_count.is_none()
        && query.session_tool_result_count.is_none()
        && query.session_text_user_count.is_none()
    {
        return None;
    }
    Some(SessionMessageRoleIndex {
        user_message_count: 0,
        assistant_message_count: query.session_assistant_count.unwrap_or(0) as u32,
        tool_result_message_count: query.session_tool_result_count.unwrap_or(0) as u32,
        text_user_message_count: query.session_text_user_count.unwrap_or(0) as u32,
        total_message_count: query.session_message_count.unwrap_or(0) as u32,
    })
}

fn timeline_coverage_for_projection(
    session_message_count: usize,
    projection: &ThreadReplayProjection,
    role_index: Option<&SessionMessageRoleIndex>,
    session_compaction: Option<
        &[zagens_core::engine::turn_machine::SessionCompactionArtifactEntry],
    >,
) -> Option<zagens_core::engine::turn_machine::SessionMessageTimelineCoverage> {
    build_session_message_timeline_coverage(
        session_message_count,
        projection,
        role_index,
        session_compaction,
    )
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
    session_messages: Option<&[zagens_core::chat::Message]>,
    session_compaction_artifacts: Option<&[zagens_core::compaction::CompactionArtifact]>,
) -> Option<ResumeSessionKernelReplay> {
    let projection = collect_thread_kernel_replay(manager, thread_id).ok()?;
    if projection.report.turns_with_events == 0 {
        return None;
    }
    let role_index = session_messages.map(build_session_message_role_index);
    let session_compaction =
        session_compaction_artifacts.map(build_session_compaction_artifact_index);
    if let Some(messages) = session_messages {
        log_session_message_plane_checks(
            messages.len(),
            &projection,
            role_index.as_ref(),
            session_compaction.as_deref(),
        );
    }
    let plane_coverage = session_messages
        .map(|messages| messages.len())
        .and_then(|count| {
            timeline_coverage_for_projection(
                count,
                &projection,
                role_index.as_ref(),
                session_compaction.as_deref(),
            )
        });
    let coverage = plane_coverage
        .as_ref()
        .map(|cov| KernelThreadMessageCoverage {
            session_message_count: cov.session_message_count,
            kernel_model_message_count: cov.kernel_model_message_count,
            coverage_ok: cov.coverage_ok,
            summary: if cov.coverage_ok {
                None
            } else {
                build_session_message_coverage(cov.session_message_count, &projection.message_stats)
                    .and_then(|c| c.summary)
            },
        });
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
        message_coverage_ok: plane_coverage.as_ref().map(|c| c.coverage_ok),
        message_coverage_summary: coverage.and_then(|c| c.summary),
        message_timeline_ok: plane_coverage.as_ref().map(|c| c.overall_ok),
        message_timeline_summary: plane_coverage.as_ref().and_then(|c| c.summary.clone()),
        kernel_model_request_count: Some(projection.message_stats.model_request_count),
        kernel_estimated_min_session_messages: Some(
            projection
                .message_plane_index
                .estimated_min_session_messages,
        ),
        message_role_index_ok: plane_coverage.as_ref().map(|c| c.role_index_ok),
        message_role_index_summary: plane_coverage
            .as_ref()
            .filter(|c| !c.role_index_ok)
            .and_then(|c| c.summary.clone()),
        message_memory_plane_ok: plane_coverage.as_ref().map(|c| c.memory_plane_user_ok),
        message_memory_plane_summary: plane_coverage
            .as_ref()
            .filter(|c| !c.memory_plane_user_ok)
            .and_then(|c| c.summary.clone()),
        message_compaction_depth_ok: plane_coverage.as_ref().map(|c| c.compaction_depth_ok),
        message_compaction_summary: plane_coverage
            .as_ref()
            .filter(|c| !c.compaction_depth_ok)
            .and_then(|c| c.summary.clone()),
        message_compaction_artifact_ok: plane_coverage.as_ref().map(|c| c.compaction_artifact_ok),
        message_compaction_artifact_summary: plane_coverage
            .as_ref()
            .filter(|c| !c.compaction_artifact_ok)
            .and_then(|c| c.summary.clone()),
        message_continuation_anchor_ok: plane_coverage.as_ref().map(|c| c.continuation_anchor_ok),
        message_continuation_anchor_summary: plane_coverage
            .as_ref()
            .filter(|c| !c.continuation_anchor_ok)
            .and_then(|c| c.summary.clone()),
        message_notify_lsp_anchor_ok: plane_coverage.as_ref().map(|c| c.notify_lsp_anchor_ok),
        message_notify_lsp_anchor_summary: plane_coverage
            .as_ref()
            .filter(|c| !c.notify_lsp_anchor_ok)
            .and_then(|c| c.summary.clone()),
    })
}

/// Log session JSON vs kernel log message-plane checks (coverage + timeline).
pub(crate) fn log_session_message_plane_checks(
    session_message_count: usize,
    projection: &ThreadReplayProjection,
    role_index: Option<&SessionMessageRoleIndex>,
    session_compaction: Option<
        &[zagens_core::engine::turn_machine::SessionCompactionArtifactEntry],
    >,
) {
    let Some(cov) = timeline_coverage_for_projection(
        session_message_count,
        projection,
        role_index,
        session_compaction,
    ) else {
        return;
    };
    record_timeline_coherence_check(cov.coherence_ok && cov.timeline_vs_requests_ok);
    record_message_coverage_check(cov.coverage_ok);
    if let Some(role_ok) = role_index.map(|_| cov.role_index_ok) {
        record_message_role_index_check(role_ok);
    }
    if role_index.is_some() {
        record_message_memory_plane_check(cov.memory_plane_user_ok);
    }
    if projection.compaction_index.artifact_count > 0 {
        record_message_compaction_depth_check(cov.compaction_depth_ok);
    }
    if session_compaction.is_some() {
        record_message_compaction_artifact_check(cov.compaction_artifact_ok);
    }
    if projection.message_stats.step_limit_continuation_count > 0
        || projection.message_stats.loop_guard_continuation_count > 0
    {
        record_continuation_anchor_check(cov.continuation_anchor_ok);
    }
    if projection.message_stats.tool_call_planned_count > 0 {
        record_notify_lsp_anchor_check(cov.notify_lsp_anchor_ok);
    }
    if !cov.timeline_vs_session_ok
        || !cov.plane_depth_ok
        || !cov.role_index_ok
        || !cov.memory_plane_user_ok
        || !cov.compaction_depth_ok
        || !cov.compaction_artifact_ok
        || !cov.continuation_anchor_ok
        || !cov.notify_lsp_anchor_ok
    {
        record_timeline_coherence_check(false);
    }
    if let Some(summary) = cov.summary {
        eprintln!(
            "[resume-session] kernel message plane diff (thread {}): {summary}",
            projection.report.thread_id
        );
    }
}

/// Log when session JSON row count looks thinner than kernel model_message events.
pub(crate) fn log_session_message_coverage(
    session_message_count: usize,
    projection: &ThreadReplayProjection,
    role_index: Option<&SessionMessageRoleIndex>,
    session_compaction: Option<
        &[zagens_core::engine::turn_machine::SessionCompactionArtifactEntry],
    >,
) {
    log_session_message_plane_checks(
        session_message_count,
        projection,
        role_index,
        session_compaction,
    );
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
    Query(query): Query<KernelThreadReplayQuery>,
) -> Result<Json<KernelThreadReplayResponse>, ApiError> {
    let manager = state.runtime_threads.clone();
    let thread_id_for_load = thread_id.clone();
    let projection = tokio::task::spawn_blocking(move || {
        collect_thread_kernel_replay(manager.as_ref(), &thread_id_for_load)
    })
    .await
    .map_err(|e| ApiError::internal(format!("kernel thread replay task panicked: {e}")))??;

    let timeline_ok =
        verify_message_timeline_coherence(&projection.message_stats, &projection.message_timeline)
            .is_none();
    record_timeline_coherence_check(timeline_ok);

    let role_index = role_index_for_replay(None, &query);
    let plane_coverage = query.session_message_count.and_then(|count| {
        timeline_coverage_for_projection(count, &projection, role_index.as_ref(), None)
    });

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
        scratchpad_summary_count: projection.message_stats.scratchpad_summary_count,
        scratchpad_reminder_count: projection.message_stats.scratchpad_reminder_count,
        cycle_briefing_count: projection.message_stats.cycle_briefing_count,
        step_limit_continuation_count: projection.message_stats.step_limit_continuation_count,
        loop_guard_continuation_count: projection.message_stats.loop_guard_continuation_count,
    };
    let message_plane_index = KernelThreadMessagePlaneIndex {
        model_request_count: projection.message_plane_index.model_request_count,
        model_message_count: projection.message_plane_index.model_message_count,
        tool_call_planned_count: projection.message_plane_index.tool_call_planned_count,
        steer_injection_count: projection.message_plane_index.steer_injection_count,
        estimated_min_session_messages: projection
            .message_plane_index
            .estimated_min_session_messages,
    };
    let compaction_timeline = projection
        .compaction_timeline
        .into_iter()
        .map(|entry| KernelThreadCompactionReplayEntry {
            turn_id: entry.turn_id,
            artifact_id: entry.artifact_id,
            replaced_from: entry.replaced_from,
            replaced_to: entry.replaced_to,
            messages_removed_count: entry.messages_removed_count,
            summary_token_count: entry.summary_token_count,
        })
        .collect();
    let compaction_index = KernelThreadCompactionReplayIndex {
        artifact_count: projection.compaction_index.artifact_count,
        messages_removed_estimate: projection.compaction_index.messages_removed_estimate,
        peak_session_depth_hint: projection.compaction_index.peak_session_depth_hint,
    };
    let message_timeline = projection
        .message_timeline
        .into_iter()
        .map(|entry| KernelThreadMessageTimelineEntry {
            turn_id: entry.turn_id,
            step_idx: entry.step_idx,
            block_count: entry.block_count,
        })
        .collect();
    let message_coverage = plane_coverage.as_ref().map(|cov| {
        record_message_coverage_check(cov.coverage_ok);
        if projection.compaction_index.artifact_count > 0 {
            record_message_compaction_depth_check(cov.compaction_depth_ok);
        }
        record_timeline_coherence_check(
            cov.coherence_ok
                && cov.timeline_vs_session_ok
                && cov.timeline_vs_requests_ok
                && cov.plane_depth_ok
                && cov.role_index_ok
                && cov.memory_plane_user_ok
                && cov.compaction_depth_ok
                && cov.compaction_artifact_ok
                && cov.continuation_anchor_ok
                && cov.notify_lsp_anchor_ok,
        );
        KernelThreadMessageCoverage {
            session_message_count: cov.session_message_count,
            kernel_model_message_count: cov.kernel_model_message_count,
            coverage_ok: cov.coverage_ok,
            summary: if cov.coverage_ok {
                None
            } else {
                build_session_message_coverage(cov.session_message_count, &projection.message_stats)
                    .and_then(|c| c.summary)
            },
        }
    });
    let message_timeline_coverage = plane_coverage.map(|cov| KernelThreadMessageTimelineCoverage {
        session_message_count: cov.session_message_count,
        kernel_model_message_count: cov.kernel_model_message_count,
        timeline_anchor_count: cov.timeline_anchor_count,
        model_request_count: cov.model_request_count,
        coherence_ok: cov.coherence_ok,
        coverage_ok: cov.coverage_ok,
        timeline_vs_session_ok: cov.timeline_vs_session_ok,
        timeline_vs_requests_ok: cov.timeline_vs_requests_ok,
        estimated_min_session_messages: cov.estimated_min_session_messages,
        plane_depth_ok: cov.plane_depth_ok,
        role_index_ok: cov.role_index_ok,
        memory_plane_user_ok: cov.memory_plane_user_ok,
        session_assistant_count: cov.session_assistant_count,
        session_tool_result_count: cov.session_tool_result_count,
        session_text_user_count: cov.session_text_user_count,
        kernel_min_assistant_messages: cov.kernel_min_assistant_messages,
        kernel_min_tool_result_messages: cov.kernel_min_tool_result_messages,
        kernel_min_memory_injected_user_messages: cov.kernel_min_memory_injected_user_messages,
        compaction_depth_ok: cov.compaction_depth_ok,
        compaction_messages_removed_estimate: cov.compaction_messages_removed_estimate,
        compaction_restored_session_estimate: cov.compaction_restored_session_estimate,
        compaction_peak_session_depth_hint: cov.compaction_peak_session_depth_hint,
        compaction_artifact_ok: cov.compaction_artifact_ok,
        session_compaction_artifact_count: cov.session_compaction_artifact_count,
        continuation_anchor_ok: cov.continuation_anchor_ok,
        notify_lsp_anchor_ok: cov.notify_lsp_anchor_ok,
        overall_ok: cov.overall_ok,
        summary: cov.summary,
    });

    let has_continuation = projection.message_stats.step_limit_continuation_count > 0
        || projection.message_stats.loop_guard_continuation_count > 0;
    if has_continuation {
        record_continuation_anchor_check(projection.continuation_anchor_ok);
    }
    if projection.message_stats.tool_call_planned_count > 0 {
        record_notify_lsp_anchor_check(projection.notify_lsp_anchor_ok);
    }

    Ok(Json(KernelThreadReplayResponse {
        thread_id: report.thread_id,
        turn_count: report.turn_count,
        turns_with_events: report.turns_with_events,
        turns_coherent: report.turns_coherent,
        all_coherent: report.all_coherent,
        latest_projection,
        message_stats,
        message_timeline,
        message_plane_index,
        compaction_timeline,
        compaction_index,
        message_coverage,
        message_timeline_coverage,
        continuation_anchor_ok: if has_continuation {
            Some(projection.continuation_anchor_ok)
        } else {
            None
        },
        continuation_anchor_summary: if has_continuation && !projection.continuation_anchor_ok {
            projection.continuation_anchor_summary.clone()
        } else {
            None
        },
        notify_lsp_anchor_ok: if projection.message_stats.tool_call_planned_count > 0 {
            Some(projection.notify_lsp_anchor_ok)
        } else {
            None
        },
        notify_lsp_anchor_summary: if projection.message_stats.tool_call_planned_count > 0
            && !projection.notify_lsp_anchor_ok
        {
            projection.notify_lsp_anchor_summary.clone()
        } else {
            None
        },
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
