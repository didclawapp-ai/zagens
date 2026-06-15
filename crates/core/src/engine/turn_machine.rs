//! TurnMachine trait + Effect enum — Phase 3b skeleton.
//!
//! This module defines the **pure-function state machine** interface that will
//! replace the `TurnLoopHost` / `op_loop` command-pattern in Phase 3b. During
//! Phase 3a / 3b-batch-1 it is **not yet wired into production**; the existing
//! `handle_deepseek_turn` path continues to run. The shadow-mode comparison
//! (batch 1 exit gate) feeds this interface.
//!
//! ## Layering
//! - `TurnKernelProjection` — rebuilt solely from [`KernelEvent`] log; no IO.
//! - `Effect` — the machine's only way to request IO from the host.
//! - `TurnMachine::step` — pure function: same inputs → same outputs.
//! - `KernelEventSink` — lightweight channel for double-write during Phase 3a/3b.

use std::collections::HashSet;

use tokio::sync::mpsc;

use crate::chat::{ContentBlock, Message};
use crate::engine::kernel_event::{CapacityAction, KernelEvent, TurnOutcome};
use crate::models::Usage;
use crate::turn::{TurnLoopMode, TurnOutcomeStatus};

// ── KernelEventSink ───────────────────────────────────────────────────────────

/// Lightweight fire-and-forget sender used for Phase 3a double-write.
///
/// Backed by an `mpsc::UnboundedSender`; a background task (Phase 3b:
/// `KernelEventLog` writer) drains it.  `send` never blocks.
///
/// Cloning is cheap (`Arc` under the hood).
pub type KernelEventSink = mpsc::UnboundedSender<KernelEvent>;

/// Emit a [`KernelEvent`] to an optional sink, ignoring send errors.
///
/// Used in `run.rs`, `streaming_phase.rs`, and `tool_phase.rs` at each
/// observation point.  A `None` sink is a no-op (all non-L2 hosts).
#[inline]
pub fn emit_kernel(sink: Option<&KernelEventSink>, event: KernelEvent) {
    if let Some(tx) = sink {
        let _ = tx.send(event);
    }
}

/// Emit through the optional sink **and** the host shadow accumulator (L2).
#[inline]
pub fn emit_kernel_event<H: crate::engine::kernel_turn_host::KernelTurnHost>(
    host: &mut H,
    event: KernelEvent,
) {
    host.record_kernel_event(&event);
    emit_kernel(host.kernel_event_sink(), event);
}

// ── TurnKernelProjection ─────────────────────────────────────────────────────

/// Snapshot of host state rebuildable purely from a `KernelEvent` log.
///
/// In Phase 3b this replaces the host-trait accessors (A-class fields in
/// the Phase 3a completeness inventory). All fields must be derivable from
/// the event log alone — see `kernel_event_completeness` tests.
#[derive(Debug, Default, Clone)]
pub struct TurnKernelProjection {
    // ── Turn metadata ────────────────────────────────────────────────────────
    pub turn_id: String,
    pub mode: Option<TurnLoopMode>,
    pub step_idx: u32,
    pub max_steps: u32,

    // ── Message counters (rebuilt from ModelMessage events) ──────────────────
    pub model_message_count: u32,
    pub total_usage: Usage,

    // ── Tool catalog ─────────────────────────────────────────────────────────
    /// Tools currently active (initial set + all `DeferredToolActivated` events).
    pub active_tool_names: HashSet<String>,
    /// Call-ids of all planned tool calls in the **current step**.
    pub pending_call_ids: Vec<String>,

    // ── Context flags ────────────────────────────────────────────────────────
    pub scratchpad_summary_injected: bool,
    pub scratchpad_reminder_count: u32,
    pub compaction_artifact_count: u32,
    pub cycle_briefing_count: u32,
    /// Count of `SteerInjected` events seen this turn.
    pub steer_injection_count: u32,

    // ── Scratchpad step counters (reset at ModelRequestIssued) ───────────────
    pub readonly_tool_successes: u32,
    pub scratchpad_writes_this_step: u32,

    // ── Continuation counters ────────────────────────────────────────────────
    pub step_limit_continuations: u32,
    pub loop_guard_continuations: u32,
    pub cycle_handoff_attempts: u32,

    // ── Capacity ─────────────────────────────────────────────────────────────
    pub last_capacity_action: Option<CapacityAction>,

    // ── Termination ─────────────────────────────────────────────────────────
    pub outcome: Option<TurnOutcome>,
}

impl TurnKernelProjection {
    /// Apply a single event and update the projection in place.
    ///
    /// This is the Phase 3b "projection function"; its correctness is covered
    /// by the Phase 3a completeness tests in `kernel_event.rs`.
    pub fn apply(&mut self, event: &KernelEvent) {
        match event {
            KernelEvent::TurnStarted {
                turn_id,
                mode,
                max_steps,
                ..
            } => {
                self.turn_id = turn_id.clone();
                self.mode = Some(*mode);
                self.max_steps = *max_steps;
                self.step_idx = 0;
            }

            KernelEvent::TurnEnded { outcome, .. } => {
                self.outcome = Some(outcome.clone());
            }

            KernelEvent::ModelRequestIssued { step_idx, .. } => {
                // Reset per-step scratchpad counters.
                self.step_idx = *step_idx;
                self.readonly_tool_successes = 0;
                self.scratchpad_writes_this_step = 0;
                self.pending_call_ids.clear();
            }

            KernelEvent::ModelMessage { usage, .. } => {
                self.model_message_count += 1;
                self.total_usage.input_tokens = self
                    .total_usage
                    .input_tokens
                    .saturating_add(usage.input_tokens);
                self.total_usage.output_tokens = self
                    .total_usage
                    .output_tokens
                    .saturating_add(usage.output_tokens);
            }

            KernelEvent::ToolCallPlanned { call_id, .. } => {
                self.pending_call_ids.push(call_id.clone());
            }

            KernelEvent::ToolCallFinished {
                call_id,
                tool_name,
                outcome,
                wrote_state,
                ..
            } => {
                self.pending_call_ids.retain(|id| id != call_id);
                if matches!(outcome, crate::engine::kernel_event::ToolOutcome::Success) {
                    if *wrote_state && tool_name.starts_with("scratchpad_") {
                        self.scratchpad_writes_this_step += 1;
                    } else if !wrote_state {
                        self.readonly_tool_successes += 1;
                    }
                }
            }

            KernelEvent::DeferredToolActivated { tool_name, .. } => {
                self.active_tool_names.insert(tool_name.clone());
            }

            KernelEvent::ScratchpadSummaryInjected { .. } => {
                self.scratchpad_summary_injected = true;
            }

            KernelEvent::ScratchpadReminderInjected { .. } => {
                self.scratchpad_reminder_count += 1;
            }

            KernelEvent::CompactionArtifactCreated { .. } => {
                self.compaction_artifact_count += 1;
            }

            KernelEvent::CycleBriefingInjected { .. } => {
                self.cycle_briefing_count += 1;
            }

            KernelEvent::SteerInjected { .. } => {
                self.steer_injection_count += 1;
            }

            KernelEvent::CapacityCheckpoint { action, .. } => {
                self.last_capacity_action = Some(action.clone());
            }

            KernelEvent::StepLimitContinuation { .. } => {
                self.step_limit_continuations += 1;
            }

            KernelEvent::LoopGuardContinuation { .. } => {
                self.loop_guard_continuations += 1;
            }

            KernelEvent::CycleAdvanced { .. } => {
                self.cycle_handoff_attempts += 1;
            }

            KernelEvent::ContextOverflowRecovered {
                strategy: crate::engine::kernel_event::OverflowStrategy::CycleHandoff,
                ..
            } => {
                self.cycle_handoff_attempts += 1;
            }

            _ => {}
        }
    }

    /// Rebuild a projection from a sequence of events (for replay / testing).
    pub fn from_events(events: &[KernelEvent]) -> Self {
        let mut p = Self::default();
        for ev in events {
            p.apply(ev);
        }
        p
    }
}

// ── Live snapshot (shadow compare) ───────────────────────────────────────────

/// Host-visible turn fields sampled at turn end for shadow projection diff.
///
/// Built by `run.rs` from `TurnContext` + loop-local counters + host flags.
/// Compared against [`TurnKernelProjection`] rebuilt from the emitted event log.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LiveTurnSnapshot {
    pub turn_id: String,
    pub step_idx: u32,
    pub max_steps: u32,
    pub scratchpad_summary_injected: bool,
    pub step_limit_continuations: u32,
    pub loop_guard_continuations: u32,
    pub cycle_handoff_attempts: u32,
}

/// Compare projection rebuilt from events against the live host snapshot.
///
/// Returns `None` when equivalent; otherwise a human-readable diff summary.
#[must_use]
pub fn compare_projection_to_live(
    live: &LiveTurnSnapshot,
    proj: &TurnKernelProjection,
) -> Option<String> {
    let mut diffs = Vec::new();
    if live.turn_id != proj.turn_id {
        diffs.push(format!(
            "turn_id live={} proj={}",
            live.turn_id, proj.turn_id
        ));
    }
    if live.step_idx != proj.step_idx {
        diffs.push(format!(
            "step_idx live={} proj={}",
            live.step_idx, proj.step_idx
        ));
    }
    if live.max_steps != proj.max_steps {
        diffs.push(format!(
            "max_steps live={} proj={}",
            live.max_steps, proj.max_steps
        ));
    }
    if live.scratchpad_summary_injected != proj.scratchpad_summary_injected {
        diffs.push(format!(
            "scratchpad_summary_injected live={} proj={}",
            live.scratchpad_summary_injected, proj.scratchpad_summary_injected
        ));
    }
    if live.step_limit_continuations != proj.step_limit_continuations {
        diffs.push(format!(
            "step_limit_continuations live={} proj={}",
            live.step_limit_continuations, proj.step_limit_continuations
        ));
    }
    if live.loop_guard_continuations != proj.loop_guard_continuations {
        diffs.push(format!(
            "loop_guard_continuations live={} proj={}",
            live.loop_guard_continuations, proj.loop_guard_continuations
        ));
    }
    if live.cycle_handoff_attempts != proj.cycle_handoff_attempts {
        diffs.push(format!(
            "cycle_handoff_attempts live={} proj={}",
            live.cycle_handoff_attempts, proj.cycle_handoff_attempts
        ));
    }
    if diffs.is_empty() {
        None
    } else {
        Some(diffs.join("; "))
    }
}

// ── Effect ────────────────────────────────────────────────────────────────────

/// IO intent emitted by `TurnMachine::step`.
///
/// The host's `EffectInterpreter` matches on this and performs the actual IO.
/// In Phase 3b batch 2 the interpreter replaces `run_streaming_phase` /
/// `run_tool_execution_phase` calls.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum Effect {
    /// Issue an LLM request and stream the response.
    CallModel { token_budget: u32 },
    /// Execute a batch of approved tool calls (DAG-scheduled).
    ExecuteBatch { call_ids: Vec<String> },
    /// Request user approval for a planned tool call.
    RequestApproval {
        call_id: String,
        description: String,
    },
    /// Inject a steer/system message into the session.
    InjectSteer { text: String },
    /// Trigger run_auto_compaction.
    RunCompaction,
    /// Notify LSP after an edit-generating tool.
    NotifyLsp { tool_name: String },
    /// Sleep until a deadline (capacity back-off).
    Sleep { millis: u64 },
}

// ── StepOutput ────────────────────────────────────────────────────────────────

/// Output of a single `TurnMachine::step` call.
#[derive(Debug, Default)]
pub struct StepOutput {
    /// Events emitted by this step (written to the KernelEvent log).
    pub emitted_events: Vec<KernelEvent>,
    /// IO effects the host must execute.
    pub effects: Vec<Effect>,
    /// If `Some`, the turn has ended.
    pub halt: Option<TurnOutcome>,
}

impl StepOutput {
    pub fn halt(outcome: TurnOutcome) -> Self {
        Self {
            halt: Some(outcome),
            ..Default::default()
        }
    }
}

// ── TurnMachine trait ─────────────────────────────────────────────────────────

/// Pure-function state machine for a single agent turn.
///
/// ### Invariants
/// - `step` must not perform IO.
/// - `step` must be deterministic given (`projection`, `event`).
/// - All state visible to `step` must be in `projection` (rebuilt from the log).
///
/// Phase 3b wires this into an `EffectInterpreter` loop that replaces
/// `handle_deepseek_turn`'s direct host calls.
pub trait TurnMachine: Send + Sync {
    fn step(&mut self, projection: &TurnKernelProjection, event: KernelEvent) -> StepOutput;
}

/// Pass-through replay machine: mirrors observed events into effects/halt.
///
/// Used in shadow mode to validate that the event log is sufficient to drive
/// a pure state machine without consulting live host state.
#[derive(Debug, Default)]
pub struct ReplayTurnMachine;

impl TurnMachine for ReplayTurnMachine {
    fn step(&mut self, projection: &TurnKernelProjection, event: KernelEvent) -> StepOutput {
        let mut out = StepOutput {
            emitted_events: vec![event.clone()],
            ..Default::default()
        };
        match &event {
            KernelEvent::TurnEnded { outcome, .. } => {
                out.halt = Some(outcome.clone());
            }
            KernelEvent::ModelRequestIssued { token_budget, .. } => {
                out.effects.push(Effect::CallModel {
                    token_budget: *token_budget,
                });
            }
            KernelEvent::SteerInjected { text, .. } => {
                out.effects.push(Effect::InjectSteer {
                    text: text.to_string(),
                });
            }
            KernelEvent::ToolCallPlanned { call_id, .. } => {
                out.effects.push(Effect::ExecuteBatch {
                    call_ids: vec![call_id.clone()],
                });
            }
            KernelEvent::ToolCallFinished {
                tool_name,
                wrote_state,
                ..
            } => {
                if *wrote_state && is_lsp_notify_tool(tool_name) {
                    out.effects.push(Effect::NotifyLsp {
                        tool_name: tool_name.clone(),
                    });
                }
            }
            KernelEvent::CapacityCheckpoint { action, .. } => {
                if matches!(action, CapacityAction::Trim | CapacityAction::Handoff) {
                    out.effects.push(Effect::RunCompaction);
                }
            }
            KernelEvent::StepLimitContinuation { .. }
            | KernelEvent::LoopGuardContinuation { .. } => {
                out.effects.push(Effect::InjectSteer {
                    text: String::new(),
                });
            }
            _ => {
                let _ = projection;
            }
        }
        out
    }
}

fn is_lsp_notify_tool(name: &str) -> bool {
    matches!(
        name,
        "edit_file" | "write_file" | "apply_patch" | "multi_tool_use.parallel"
    ) || name.starts_with("edit_")
}

/// Verify that [`ReplayTurnMachine`] effect counts match observed event counts.
///
/// Returns `None` when the replay chain is internally consistent; used by
/// Phase 3b effect shadow bake (`[kernel] machine = "shadow"`).
#[must_use]
pub fn verify_effect_replay_chain(events: &[KernelEvent]) -> Option<String> {
    let mut machine = ReplayTurnMachine;
    let mut projection = TurnKernelProjection::default();
    let mut call_model_effects = 0u32;
    let mut execute_batch_effects = 0u32;
    let mut model_requests = 0u32;
    let mut tool_planned = 0u32;
    let mut halt: Option<TurnOutcome> = None;

    for event in events {
        match &event {
            KernelEvent::ModelRequestIssued { .. } => model_requests += 1,
            KernelEvent::ToolCallPlanned { .. } => tool_planned += 1,
            _ => {}
        }
        let out = machine.step(&projection, event.clone());
        projection.apply(&event);
        for effect in &out.effects {
            match effect {
                Effect::CallModel { .. } => call_model_effects += 1,
                Effect::ExecuteBatch { .. } => execute_batch_effects += 1,
                _ => {}
            }
        }
        if let Some(h) = out.halt {
            halt = Some(h);
        }
    }

    let mut diffs = Vec::new();
    if call_model_effects != model_requests {
        diffs.push(format!(
            "CallModel effects ({call_model_effects}) != ModelRequestIssued events ({model_requests})"
        ));
    }
    if execute_batch_effects != tool_planned {
        diffs.push(format!(
            "ExecuteBatch effects ({execute_batch_effects}) != ToolCallPlanned events ({tool_planned})"
        ));
    }
    if !events
        .iter()
        .any(|e| matches!(e, KernelEvent::TurnEnded { .. }))
    {
        diffs.push("missing TurnEnded event".into());
    } else if halt.is_none() {
        diffs.push("ReplayTurnMachine did not halt on TurnEnded".into());
    }
    if diffs.is_empty() {
        None
    } else {
        Some(diffs.join("; "))
    }
}

/// Verify LHT / guard continuation counters match the event log projection.
#[must_use]
pub fn verify_guard_projection_chain(events: &[KernelEvent]) -> Option<String> {
    let projection = TurnKernelProjection::from_events(events);
    let mut step_limit = 0u32;
    let mut loop_guard = 0u32;
    let mut cycle_handoffs = 0u32;
    let mut capacity_checkpoints = 0u32;

    for event in events {
        match event {
            KernelEvent::StepLimitContinuation { .. } => step_limit += 1,
            KernelEvent::LoopGuardContinuation { .. } => loop_guard += 1,
            KernelEvent::ContextOverflowRecovered {
                strategy: crate::engine::kernel_event::OverflowStrategy::CycleHandoff,
                ..
            }
            | KernelEvent::CycleAdvanced { .. } => cycle_handoffs += 1,
            KernelEvent::CapacityCheckpoint { .. } => capacity_checkpoints += 1,
            _ => {}
        }
    }

    let mut diffs = Vec::new();
    if projection.step_limit_continuations != step_limit {
        diffs.push(format!(
            "step_limit_continuations proj={} events={step_limit}",
            projection.step_limit_continuations
        ));
    }
    if projection.loop_guard_continuations != loop_guard {
        diffs.push(format!(
            "loop_guard_continuations proj={} events={loop_guard}",
            projection.loop_guard_continuations
        ));
    }
    if projection.cycle_handoff_attempts != cycle_handoffs {
        diffs.push(format!(
            "cycle_handoff_attempts proj={} events={cycle_handoffs}",
            projection.cycle_handoff_attempts
        ));
    }
    if capacity_checkpoints > 0 && projection.last_capacity_action.is_none() {
        diffs.push(
            "capacity checkpoints present but projection last_capacity_action is None".into(),
        );
    }
    if diffs.is_empty() {
        None
    } else {
        Some(diffs.join("; "))
    }
}

/// Verify memory-plane event counters match the event log projection.
#[must_use]
pub fn verify_memory_projection_chain(events: &[KernelEvent]) -> Option<String> {
    let projection = TurnKernelProjection::from_events(events);
    let mut summary_injected = 0u32;
    let mut reminder_injected = 0u32;
    let mut compaction_artifacts = 0u32;
    let mut cycle_briefings = 0u32;

    for event in events {
        match event {
            KernelEvent::ScratchpadSummaryInjected { .. } => summary_injected += 1,
            KernelEvent::ScratchpadReminderInjected { .. } => reminder_injected += 1,
            KernelEvent::CompactionArtifactCreated { .. } => compaction_artifacts += 1,
            KernelEvent::CycleBriefingInjected { .. } => cycle_briefings += 1,
            _ => {}
        }
    }

    let mut diffs = Vec::new();
    if projection.scratchpad_summary_injected != (summary_injected > 0) {
        diffs.push(format!(
            "scratchpad_summary_injected proj={} events={summary_injected}",
            projection.scratchpad_summary_injected
        ));
    }
    if projection.scratchpad_reminder_count != reminder_injected {
        diffs.push(format!(
            "scratchpad_reminder_count proj={} events={reminder_injected}",
            projection.scratchpad_reminder_count
        ));
    }
    if projection.compaction_artifact_count != compaction_artifacts {
        diffs.push(format!(
            "compaction_artifact_count proj={} events={compaction_artifacts}",
            projection.compaction_artifact_count
        ));
    }
    if projection.cycle_briefing_count != cycle_briefings {
        diffs.push(format!(
            "cycle_briefing_count proj={} events={cycle_briefings}",
            projection.cycle_briefing_count
        ));
    }
    if diffs.is_empty() {
        None
    } else {
        Some(diffs.join("; "))
    }
}

// ── Turn replay (Phase 3b batch 6 — resume foundation) ───────────────────────

/// Projection rebuilt from a turn's event log — the Phase 3b resume substrate.
#[derive(Debug, Clone)]
pub struct TurnReplayReport {
    pub event_count: usize,
    pub projection: TurnKernelProjection,
    pub outcome: Option<TurnOutcome>,
}

/// Rebuild [`TurnKernelProjection`] and outcome purely from an event sequence.
#[must_use]
pub fn replay_turn_projection(events: &[KernelEvent]) -> TurnReplayReport {
    let projection = TurnKernelProjection::from_events(events);
    TurnReplayReport {
        event_count: events.len(),
        outcome: projection.outcome.clone(),
        projection,
    }
}

/// Unified replay gate: projection/live parity + effect/guard/memory chains.
///
/// Returns `None` when the log is sufficient to drive resume/replay; used by
/// Phase 3b replay shadow bake and future session resume.
#[must_use]
pub fn verify_turn_replay_coherence(
    events: &[KernelEvent],
    live: Option<&LiveTurnSnapshot>,
) -> Option<String> {
    let mut diffs = Vec::new();

    if let Some(live) = live {
        let projection = TurnKernelProjection::from_events(events);
        if let Some(summary) = compare_projection_to_live(live, &projection) {
            diffs.push(format!("live_projection: {summary}"));
        }
    }
    if let Some(summary) = verify_effect_replay_chain(events) {
        diffs.push(format!("effect: {summary}"));
    }
    if let Some(summary) = verify_guard_projection_chain(events) {
        diffs.push(format!("guard: {summary}"));
    }
    if let Some(summary) = verify_memory_projection_chain(events) {
        diffs.push(format!("memory: {summary}"));
    }
    if !events
        .iter()
        .any(|e| matches!(e, KernelEvent::TurnEnded { .. }))
    {
        diffs.push("missing TurnEnded event".into());
    }

    if diffs.is_empty() {
        None
    } else {
        Some(diffs.join("; "))
    }
}

/// Per-turn replay summary for thread-level aggregation (Phase 3b batch 6c).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadTurnReplaySummary {
    pub turn_id: String,
    pub event_count: usize,
    pub coherence_ok: bool,
    pub coherence_error: Option<String>,
    pub outcome: Option<TurnOutcome>,
}

/// Thread-level replay report built from persisted turn event logs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadReplayReport {
    pub thread_id: String,
    pub turn_count: usize,
    pub turns_with_events: usize,
    pub turns_coherent: usize,
    pub all_coherent: bool,
    pub turns: Vec<ThreadTurnReplaySummary>,
}

/// Build a thread replay report from `(turn_id, events)` pairs.
///
/// Turns with empty event logs are omitted; coherence is evaluated only when
/// events are present.
#[must_use]
pub fn build_thread_replay_report(
    thread_id: &str,
    turn_events: &[(String, Vec<KernelEvent>)],
) -> ThreadReplayReport {
    let mut turns = Vec::new();
    let mut turns_with_events = 0usize;
    let mut turns_coherent = 0usize;

    for (turn_id, events) in turn_events {
        if events.is_empty() {
            continue;
        }
        turns_with_events += 1;
        let report = replay_turn_projection(events);
        let coherence_error = verify_turn_replay_coherence(events, None);
        let coherence_ok = coherence_error.is_none();
        if coherence_ok {
            turns_coherent += 1;
        }
        turns.push(ThreadTurnReplaySummary {
            turn_id: turn_id.clone(),
            event_count: report.event_count,
            coherence_ok,
            coherence_error,
            outcome: report.outcome,
        });
    }

    ThreadReplayReport {
        thread_id: thread_id.to_string(),
        turn_count: turn_events.len(),
        turns_with_events,
        turns_coherent,
        all_coherent: turns_with_events > 0 && turns_coherent == turns_with_events,
        turns,
    }
}

/// Thread-level replay substrate: per-turn coherence report plus latest turn projection.
#[derive(Debug, Clone)]
pub struct ThreadReplayProjection {
    pub report: ThreadReplayReport,
    pub latest_turn_id: Option<String>,
    pub latest_projection: TurnKernelProjection,
    pub message_stats: ThreadMessageReplayStats,
    /// Per-step model message anchors from the log (block counts only; text stays in session store).
    pub message_timeline: Vec<ThreadMessageTimelineEntry>,
    /// Aggregated message-plane index (counts only; session depth estimates).
    pub message_plane_index: ThreadMessagePlaneIndex,
    /// Compaction artifact anchors from kernel logs (replaced_range metadata only).
    pub compaction_timeline: Vec<ThreadCompactionReplayEntry>,
    pub compaction_index: ThreadCompactionReplayIndex,
}

/// One compaction artifact anchor rebuildable from kernel logs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadCompactionReplayEntry {
    pub turn_id: String,
    pub artifact_id: String,
    /// Inclusive replaced message index range (`from`..=`to`).
    pub replaced_from: u32,
    pub replaced_to: u32,
    pub messages_removed_count: u32,
    pub summary_token_count: u32,
}

/// Aggregated compaction metadata rebuildable from kernel event logs.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ThreadCompactionReplayIndex {
    pub artifact_count: u32,
    pub messages_removed_estimate: u32,
    /// Lower-bound session depth at compaction time (`max(replaced_to) + 1`).
    pub peak_session_depth_hint: u32,
}

/// One `ModelMessage` anchor rebuildable from kernel logs (no message body).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadMessageTimelineEntry {
    pub turn_id: String,
    pub step_idx: u32,
    pub block_count: u32,
}

/// Structured session JSON vs kernel log message coverage (observability).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionMessageCoverage {
    pub session_message_count: usize,
    pub kernel_model_message_count: u32,
    pub coverage_ok: bool,
    pub summary: Option<String>,
}

/// Aggregated message-plane counters rebuildable from kernel event logs (not full text).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ThreadMessageReplayStats {
    pub turns_with_events: usize,
    pub model_request_count: u32,
    pub model_message_count: u32,
    pub tool_call_planned_count: u32,
    pub steer_injection_count: u32,
    pub compaction_artifact_count: u32,
    pub scratchpad_summary_count: u32,
    pub scratchpad_reminder_count: u32,
    pub cycle_briefing_count: u32,
    pub step_limit_continuation_count: u32,
    pub loop_guard_continuation_count: u32,
}

/// Message-plane index rebuildable from kernel logs (no message bodies).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ThreadMessagePlaneIndex {
    pub model_request_count: u32,
    pub model_message_count: u32,
    pub tool_call_planned_count: u32,
    pub steer_injection_count: u32,
    /// Lower-bound session rows: model assistant messages + tool result messages.
    pub estimated_min_session_messages: u32,
}

/// Build a message-plane index from aggregated thread stats.
#[must_use]
pub fn replay_thread_message_plane_index(
    stats: &ThreadMessageReplayStats,
) -> ThreadMessagePlaneIndex {
    ThreadMessagePlaneIndex {
        model_request_count: stats.model_request_count,
        model_message_count: stats.model_message_count,
        tool_call_planned_count: stats.tool_call_planned_count,
        steer_injection_count: stats.steer_injection_count,
        estimated_min_session_messages: stats
            .model_message_count
            .saturating_add(stats.tool_call_planned_count),
    }
}

/// Role-indexed session message counts (observability; no body rebuild).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SessionMessageRoleIndex {
    pub user_message_count: u32,
    pub assistant_message_count: u32,
    pub tool_result_message_count: u32,
    /// User rows with text blocks and no `tool_result` (steer, scratchpad injects, initial prompt).
    pub text_user_message_count: u32,
    pub total_message_count: u32,
}

/// Kernel-log lower bounds for memory-plane user injections (steer / scratchpad).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct KernelMemoryPlaneUserEstimate {
    pub min_steer_user_messages: u32,
    pub min_scratchpad_summary_user_messages: u32,
    pub min_scratchpad_reminder_user_messages: u32,
    pub min_continuation_user_messages: u32,
    pub min_memory_injected_user_messages: u32,
}

/// Kernel-log lower bounds for role-indexed session rows (assistant + tool results).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct KernelMessageRoleEstimate {
    pub min_assistant_messages: u32,
    pub min_tool_result_messages: u32,
    pub min_steer_user_messages: u32,
}

/// Build role-indexed counts from session JSON messages.
#[must_use]
pub fn build_session_message_role_index(messages: &[Message]) -> SessionMessageRoleIndex {
    let mut user_message_count = 0u32;
    let mut assistant_message_count = 0u32;
    let mut tool_result_message_count = 0u32;
    let mut text_user_message_count = 0u32;
    for msg in messages {
        match msg.role.as_str() {
            "user" => {
                user_message_count += 1;
                if message_has_tool_result(msg) {
                    tool_result_message_count += 1;
                } else if message_has_text_block(msg) {
                    text_user_message_count += 1;
                }
            }
            "assistant" => assistant_message_count += 1,
            _ => {}
        }
    }
    SessionMessageRoleIndex {
        user_message_count,
        assistant_message_count,
        tool_result_message_count,
        text_user_message_count,
        total_message_count: messages.len() as u32,
    }
}

fn message_has_text_block(msg: &Message) -> bool {
    msg.content
        .iter()
        .any(|block| matches!(block, ContentBlock::Text { .. }))
}

fn message_has_tool_result(msg: &Message) -> bool {
    msg.content
        .iter()
        .any(|block| matches!(block, ContentBlock::ToolResult { .. }))
}

/// Build kernel-log role lower bounds from aggregated thread stats.
#[must_use]
pub fn replay_kernel_message_role_estimate(
    stats: &ThreadMessageReplayStats,
) -> KernelMessageRoleEstimate {
    KernelMessageRoleEstimate {
        min_assistant_messages: stats.model_message_count,
        min_tool_result_messages: stats.tool_call_planned_count,
        min_steer_user_messages: stats.steer_injection_count,
    }
}

/// Build kernel-log lower bounds for memory-plane user injections.
#[must_use]
pub fn replay_kernel_memory_plane_user_estimate(
    stats: &ThreadMessageReplayStats,
) -> KernelMemoryPlaneUserEstimate {
    let min_steer_user_messages = stats.steer_injection_count;
    let min_scratchpad_summary_user_messages = stats.scratchpad_summary_count;
    let min_scratchpad_reminder_user_messages = stats.scratchpad_reminder_count;
    let min_continuation_user_messages = stats
        .step_limit_continuation_count
        .saturating_add(stats.loop_guard_continuation_count);
    let min_memory_injected_user_messages = min_steer_user_messages
        .saturating_add(min_scratchpad_summary_user_messages)
        .saturating_add(min_scratchpad_reminder_user_messages)
        .saturating_add(min_continuation_user_messages);
    KernelMemoryPlaneUserEstimate {
        min_steer_user_messages,
        min_scratchpad_summary_user_messages,
        min_scratchpad_reminder_user_messages,
        min_continuation_user_messages,
        min_memory_injected_user_messages,
    }
}

/// Weak check: session text-user rows can host kernel memory-plane injections.
#[must_use]
pub fn verify_session_memory_plane_user_depth(
    session: &SessionMessageRoleIndex,
    kernel: &KernelMemoryPlaneUserEstimate,
) -> Option<String> {
    if kernel.min_memory_injected_user_messages == 0 {
        return None;
    }
    if session.text_user_message_count >= kernel.min_memory_injected_user_messages {
        None
    } else {
        Some(format!(
            "session text user messages ({}) below kernel memory-plane injections ({})",
            session.text_user_message_count, kernel.min_memory_injected_user_messages
        ))
    }
}

/// Verify session role counts can host kernel-log assistant + tool-result rows.
#[must_use]
pub fn verify_session_role_index(
    session: &SessionMessageRoleIndex,
    kernel: &KernelMessageRoleEstimate,
) -> Option<String> {
    let mut issues = Vec::new();
    if session.assistant_message_count < kernel.min_assistant_messages {
        issues.push(format!(
            "session assistant messages ({}) below kernel model_message events ({})",
            session.assistant_message_count, kernel.min_assistant_messages
        ));
    }
    if session.tool_result_message_count < kernel.min_tool_result_messages {
        issues.push(format!(
            "session tool_result messages ({}) below kernel tool_call_planned events ({})",
            session.tool_result_message_count, kernel.min_tool_result_messages
        ));
    }
    if issues.is_empty() {
        None
    } else {
        Some(issues.join("; "))
    }
}

/// Count inclusive-range messages removed by a compaction artifact.
#[must_use]
pub fn compaction_messages_removed_count(replaced_from: u32, replaced_to: u32) -> u32 {
    if replaced_to >= replaced_from {
        replaced_to - replaced_from + 1
    } else {
        0
    }
}

/// Rebuild compaction artifact anchors across a thread from persisted kernel events.
#[must_use]
pub fn replay_thread_compaction_timeline(
    turn_events: &[(String, Vec<KernelEvent>)],
) -> Vec<ThreadCompactionReplayEntry> {
    let mut timeline = Vec::new();
    for (_, events) in turn_events {
        for event in events {
            if let KernelEvent::CompactionArtifactCreated {
                turn_id,
                artifact_id,
                replaced_range,
                summary_token_count,
            } = event
            {
                let messages_removed_count =
                    compaction_messages_removed_count(replaced_range.from, replaced_range.to);
                timeline.push(ThreadCompactionReplayEntry {
                    turn_id: turn_id.clone(),
                    artifact_id: artifact_id.clone(),
                    replaced_from: replaced_range.from,
                    replaced_to: replaced_range.to,
                    messages_removed_count,
                    summary_token_count: *summary_token_count,
                });
            }
        }
    }
    timeline
}

/// Build aggregated compaction index from compaction timeline anchors.
#[must_use]
pub fn replay_thread_compaction_index(
    timeline: &[ThreadCompactionReplayEntry],
) -> ThreadCompactionReplayIndex {
    let mut messages_removed_estimate = 0u32;
    let mut peak_session_depth_hint = 0u32;
    for entry in timeline {
        messages_removed_estimate =
            messages_removed_estimate.saturating_add(entry.messages_removed_count);
        peak_session_depth_hint = peak_session_depth_hint.max(entry.replaced_to.saturating_add(1));
    }
    ThreadCompactionReplayIndex {
        artifact_count: timeline.len() as u32,
        messages_removed_estimate,
        peak_session_depth_hint,
    }
}

/// Weak check: current session + compaction-removed rows cover kernel plane estimate.
#[must_use]
pub fn verify_session_compaction_depth(
    session_message_count: usize,
    compaction: &ThreadCompactionReplayIndex,
    plane_index: &ThreadMessagePlaneIndex,
) -> Option<String> {
    if compaction.artifact_count == 0 {
        return None;
    }
    let restored = session_message_count as u32 + compaction.messages_removed_estimate;
    let min_needed = plane_index.estimated_min_session_messages;
    if restored < min_needed {
        Some(format!(
            "session ({session_message_count}) + compaction removed ({removed}) = {restored} below kernel plane estimate ({min_needed})",
            removed = compaction.messages_removed_estimate
        ))
    } else {
        None
    }
}

/// Verify session JSON depth can host the kernel log message-plane estimate.
#[must_use]
pub fn verify_session_message_plane_depth(
    session_message_count: usize,
    index: &ThreadMessagePlaneIndex,
) -> Option<String> {
    let min = index.estimated_min_session_messages as usize;
    if session_message_count < min {
        Some(format!(
            "session messages ({session_message_count}) below kernel plane estimate ({min})"
        ))
    } else {
        None
    }
}

/// Verify a step slice has exactly one `ModelMessage` when a model request was issued.
#[must_use]
pub fn verify_step_model_message_anchor(
    turn_events: &[KernelEvent],
    step_idx: u32,
) -> Option<String> {
    let step_events = events_for_step(turn_events, step_idx);
    if step_events.is_empty() {
        return None;
    }
    let has_request = step_events.iter().any(
        |e| matches!(e, KernelEvent::ModelRequestIssued { step_idx: s, .. } if *s == step_idx),
    );
    let message_count = step_events
        .iter()
        .filter(|e| matches!(e, KernelEvent::ModelMessage { step_idx: s, .. } if *s == step_idx))
        .count();
    if has_request && message_count == 0 {
        return Some(format!(
            "step {step_idx} has ModelRequestIssued but no ModelMessage"
        ));
    }
    if has_request && message_count > 1 {
        return Some(format!(
            "step {step_idx} has {message_count} ModelMessage events (expected 1)"
        ));
    }
    None
}

/// Count message-plane events across all turns on a thread.
#[must_use]
pub fn replay_thread_message_stats(
    turn_events: &[(String, Vec<KernelEvent>)],
) -> ThreadMessageReplayStats {
    let mut stats = ThreadMessageReplayStats::default();
    for (_, events) in turn_events {
        if events.is_empty() {
            continue;
        }
        stats.turns_with_events += 1;
        for event in events {
            match event {
                KernelEvent::ModelRequestIssued { .. } => stats.model_request_count += 1,
                KernelEvent::ModelMessage { .. } => stats.model_message_count += 1,
                KernelEvent::ToolCallPlanned { .. } => stats.tool_call_planned_count += 1,
                KernelEvent::SteerInjected { .. } => stats.steer_injection_count += 1,
                KernelEvent::CompactionArtifactCreated { .. } => {
                    stats.compaction_artifact_count += 1
                }
                KernelEvent::ScratchpadSummaryInjected { .. } => {
                    stats.scratchpad_summary_count += 1
                }
                KernelEvent::ScratchpadReminderInjected { .. } => {
                    stats.scratchpad_reminder_count += 1
                }
                KernelEvent::CycleBriefingInjected { .. } => stats.cycle_briefing_count += 1,
                KernelEvent::StepLimitContinuation { .. } => {
                    stats.step_limit_continuation_count += 1
                }
                KernelEvent::LoopGuardContinuation { .. } => {
                    stats.loop_guard_continuation_count += 1
                }
                _ => {}
            }
        }
    }
    stats
}

/// Rebuild model-message anchors across a thread from persisted kernel events.
#[must_use]
pub fn replay_thread_message_timeline(
    turn_events: &[(String, Vec<KernelEvent>)],
) -> Vec<ThreadMessageTimelineEntry> {
    let mut timeline = Vec::new();
    for (_, events) in turn_events {
        for event in events {
            if let KernelEvent::ModelMessage {
                turn_id,
                step_idx,
                block_count,
                ..
            } = event
            {
                timeline.push(ThreadMessageTimelineEntry {
                    turn_id: turn_id.clone(),
                    step_idx: *step_idx,
                    block_count: *block_count,
                });
            }
        }
    }
    timeline
}

/// Best-effort coverage check: session JSON message count vs kernel log counters (observability).
#[must_use]
pub fn build_session_message_coverage(
    session_message_count: usize,
    stats: &ThreadMessageReplayStats,
) -> Option<SessionMessageCoverage> {
    if stats.model_message_count == 0 {
        return None;
    }
    let kernel_model_message_count = stats.model_message_count;
    let expected_min = kernel_model_message_count as usize;
    let coverage_ok = session_message_count >= expected_min;
    let summary = if coverage_ok {
        None
    } else {
        Some(format!(
            "session messages ({session_message_count}) below kernel model_message events ({expected_min})"
        ))
    };
    Some(SessionMessageCoverage {
        session_message_count,
        kernel_model_message_count,
        coverage_ok,
        summary,
    })
}

/// Best-effort coverage check: session JSON message count vs kernel log counters (observability).
#[must_use]
pub fn verify_session_message_coverage(
    session_message_count: usize,
    stats: &ThreadMessageReplayStats,
) -> Option<String> {
    build_session_message_coverage(session_message_count, stats).and_then(|c| c.summary)
}

/// Verify internal consistency between timeline anchors and aggregated message stats.
#[must_use]
pub fn verify_message_timeline_coherence(
    stats: &ThreadMessageReplayStats,
    timeline: &[ThreadMessageTimelineEntry],
) -> Option<String> {
    let timeline_len = timeline.len();
    if timeline_len as u32 != stats.model_message_count {
        return Some(format!(
            "timeline entries ({timeline_len}) != model_message_count ({})",
            stats.model_message_count
        ));
    }
    for entry in timeline {
        if entry.block_count == 0 {
            return Some(format!(
                "turn {} step {} has block_count 0",
                entry.turn_id, entry.step_idx
            ));
        }
    }
    None
}

/// Verify session message depth can host every timeline anchor (no body rebuild).
#[must_use]
pub fn verify_message_timeline_vs_session(
    session_message_count: usize,
    timeline: &[ThreadMessageTimelineEntry],
) -> Option<String> {
    if timeline.is_empty() {
        return None;
    }
    let min_messages = timeline.len();
    if session_message_count < min_messages {
        Some(format!(
            "session messages ({session_message_count}) below timeline anchors ({min_messages})"
        ))
    } else {
        None
    }
}

/// Structured session + timeline + kernel log message-plane checks (observability).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionMessageTimelineCoverage {
    pub session_message_count: usize,
    pub kernel_model_message_count: u32,
    pub timeline_anchor_count: usize,
    pub model_request_count: u32,
    pub estimated_min_session_messages: u32,
    pub coherence_ok: bool,
    pub coverage_ok: bool,
    pub timeline_vs_session_ok: bool,
    pub timeline_vs_requests_ok: bool,
    pub plane_depth_ok: bool,
    pub role_index_ok: bool,
    pub memory_plane_user_ok: bool,
    pub session_assistant_count: Option<u32>,
    pub session_tool_result_count: Option<u32>,
    pub session_text_user_count: Option<u32>,
    pub kernel_min_assistant_messages: u32,
    pub kernel_min_tool_result_messages: u32,
    pub kernel_min_memory_injected_user_messages: u32,
    pub compaction_depth_ok: bool,
    pub compaction_messages_removed_estimate: u32,
    pub compaction_restored_session_estimate: u32,
    pub compaction_peak_session_depth_hint: u32,
    pub overall_ok: bool,
    pub summary: Option<String>,
}

/// Verify timeline anchors do not exceed model request count on the thread.
#[must_use]
pub fn verify_timeline_vs_request_count(
    stats: &ThreadMessageReplayStats,
    timeline: &[ThreadMessageTimelineEntry],
) -> Option<String> {
    let anchors = timeline.len() as u32;
    if anchors > stats.model_request_count {
        Some(format!(
            "timeline anchors ({anchors}) exceed model_request_count ({})",
            stats.model_request_count
        ))
    } else {
        None
    }
}

/// Build unified session / timeline / kernel log message-plane coverage report.
#[must_use]
pub fn build_session_message_timeline_coverage(
    session_message_count: usize,
    projection: &ThreadReplayProjection,
    role_index: Option<&SessionMessageRoleIndex>,
) -> Option<SessionMessageTimelineCoverage> {
    let stats = &projection.message_stats;
    let timeline = &projection.message_timeline;
    let has_message_plane = stats.model_message_count > 0 || !timeline.is_empty();
    let has_memory_plane = stats.steer_injection_count > 0
        || stats.scratchpad_summary_count > 0
        || stats.scratchpad_reminder_count > 0
        || stats.step_limit_continuation_count > 0
        || stats.loop_guard_continuation_count > 0;
    let has_compaction = projection.compaction_index.artifact_count > 0;
    if !has_message_plane && !has_memory_plane && !has_compaction {
        return None;
    }
    let plane_index = &projection.message_plane_index;
    let compaction_index = &projection.compaction_index;
    let role_estimate = replay_kernel_message_role_estimate(stats);
    let memory_estimate = replay_kernel_memory_plane_user_estimate(stats);
    let coherence_ok = verify_message_timeline_coherence(stats, timeline).is_none();
    let coverage = build_session_message_coverage(session_message_count, stats);
    let coverage_ok = coverage.as_ref().map(|c| c.coverage_ok).unwrap_or(true);
    let timeline_vs_session_ok =
        verify_message_timeline_vs_session(session_message_count, timeline).is_none();
    let timeline_vs_requests_ok = verify_timeline_vs_request_count(stats, timeline).is_none();
    let plane_depth_ok =
        verify_session_message_plane_depth(session_message_count, &plane_index).is_none();
    let role_index_ok = role_index
        .map(|idx| verify_session_role_index(idx, &role_estimate).is_none())
        .unwrap_or(true);
    let memory_plane_user_ok = role_index
        .map(|idx| verify_session_memory_plane_user_depth(idx, &memory_estimate).is_none())
        .unwrap_or(true);
    let compaction_depth_ok =
        verify_session_compaction_depth(session_message_count, compaction_index, plane_index)
            .is_none();
    let compaction_restored_session_estimate =
        session_message_count as u32 + compaction_index.messages_removed_estimate;
    let overall_ok = coherence_ok
        && coverage_ok
        && timeline_vs_session_ok
        && timeline_vs_requests_ok
        && plane_depth_ok
        && role_index_ok
        && memory_plane_user_ok
        && compaction_depth_ok;

    let mut summaries = Vec::new();
    if let Some(s) = verify_message_timeline_coherence(stats, timeline) {
        summaries.push(s);
    }
    if let Some(s) = coverage.and_then(|c| c.summary) {
        summaries.push(s);
    }
    if let Some(s) = verify_message_timeline_vs_session(session_message_count, timeline) {
        summaries.push(s);
    }
    if let Some(s) = verify_timeline_vs_request_count(stats, timeline) {
        summaries.push(s);
    }
    if let Some(s) = verify_session_message_plane_depth(session_message_count, plane_index) {
        summaries.push(s);
    }
    if let Some(s) =
        verify_session_compaction_depth(session_message_count, compaction_index, plane_index)
    {
        summaries.push(s);
    }
    if let Some(idx) = role_index {
        if let Some(s) = verify_session_role_index(idx, &role_estimate) {
            summaries.push(s);
        }
        if let Some(s) = verify_session_memory_plane_user_depth(idx, &memory_estimate) {
            summaries.push(s);
        }
    }

    Some(SessionMessageTimelineCoverage {
        session_message_count,
        kernel_model_message_count: stats.model_message_count,
        timeline_anchor_count: timeline.len(),
        model_request_count: stats.model_request_count,
        estimated_min_session_messages: plane_index.estimated_min_session_messages,
        coherence_ok,
        coverage_ok,
        timeline_vs_session_ok,
        timeline_vs_requests_ok,
        plane_depth_ok,
        role_index_ok,
        memory_plane_user_ok,
        session_assistant_count: role_index.map(|idx| idx.assistant_message_count),
        session_tool_result_count: role_index.map(|idx| idx.tool_result_message_count),
        session_text_user_count: role_index.map(|idx| idx.text_user_message_count),
        kernel_min_assistant_messages: role_estimate.min_assistant_messages,
        kernel_min_tool_result_messages: role_estimate.min_tool_result_messages,
        kernel_min_memory_injected_user_messages: memory_estimate.min_memory_injected_user_messages,
        compaction_depth_ok,
        compaction_messages_removed_estimate: compaction_index.messages_removed_estimate,
        compaction_restored_session_estimate,
        compaction_peak_session_depth_hint: compaction_index.peak_session_depth_hint,
        overall_ok,
        summary: if summaries.is_empty() {
            None
        } else {
            Some(summaries.join("; "))
        },
    })
}

/// Build thread replay report and the latest non-empty turn projection (resume substrate).
#[must_use]
pub fn replay_thread_projection(
    thread_id: &str,
    turn_events: &[(String, Vec<KernelEvent>)],
) -> ThreadReplayProjection {
    let report = build_thread_replay_report(thread_id, turn_events);
    let message_stats = replay_thread_message_stats(turn_events);
    let message_timeline = replay_thread_message_timeline(turn_events);
    let message_plane_index = replay_thread_message_plane_index(&message_stats);
    let compaction_timeline = replay_thread_compaction_timeline(turn_events);
    let compaction_index = replay_thread_compaction_index(&compaction_timeline);
    let (latest_turn_id, latest_projection) = turn_events
        .iter()
        .rev()
        .find(|(_, events)| !events.is_empty())
        .map(|(turn_id, events)| {
            (
                Some(turn_id.clone()),
                TurnKernelProjection::from_events(events),
            )
        })
        .unwrap_or((None, TurnKernelProjection::default()));
    ThreadReplayProjection {
        report,
        latest_turn_id,
        latest_projection,
        message_stats,
        message_timeline,
        message_plane_index,
        compaction_timeline,
        compaction_index,
    }
}

/// Host-visible fields restored from a thread's latest kernel projection on engine load.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct KernelResumeHints {
    pub latest_turn_id: Option<String>,
    pub step_idx: u32,
    pub max_steps: u32,
    pub scratchpad_summary_injected: bool,
    pub active_tool_count: u32,
    /// Aggregated `ModelMessage` count on the linked thread (log substrate; text stays in session store).
    pub kernel_model_message_count: u32,
    /// Aggregated `ModelRequestIssued` count on the linked thread.
    pub kernel_model_request_count: u32,
    /// Lower-bound session rows estimated from kernel log (assistant + tool results).
    pub kernel_estimated_min_session_messages: u32,
}

/// Extract resume hints from the latest turn projection (log-driven resume substrate).
#[must_use]
pub fn kernel_resume_hints_from_projection(proj: &TurnKernelProjection) -> KernelResumeHints {
    KernelResumeHints {
        latest_turn_id: if proj.turn_id.is_empty() {
            None
        } else {
            Some(proj.turn_id.clone())
        },
        step_idx: proj.step_idx,
        max_steps: proj.max_steps,
        scratchpad_summary_injected: proj.scratchpad_summary_injected,
        active_tool_count: proj.active_tool_names.len() as u32,
        kernel_model_message_count: 0,
        kernel_model_request_count: 0,
        kernel_estimated_min_session_messages: 0,
    }
}

/// Resume hints from thread projection plus aggregated message counters.
#[must_use]
pub fn kernel_resume_hints_from_thread_projection(
    projection: &ThreadReplayProjection,
) -> KernelResumeHints {
    let mut hints = kernel_resume_hints_from_projection(&projection.latest_projection);
    hints.kernel_model_message_count = projection.message_stats.model_message_count;
    hints.kernel_model_request_count = projection.message_stats.model_request_count;
    hints.kernel_estimated_min_session_messages = projection
        .message_plane_index
        .estimated_min_session_messages;
    hints
}

/// Replay all IO effects implied by a turn's event log via [`ReplayTurnMachine`].
#[must_use]
pub fn replay_turn_effects(events: &[KernelEvent]) -> Vec<Effect> {
    let mut machine = ReplayTurnMachine;
    let mut projection = TurnKernelProjection::default();
    let mut effects = Vec::new();
    for event in events {
        let out = machine.step(&projection, event.clone());
        projection.apply(&event);
        effects.extend(out.effects);
    }
    effects
}

/// Count CallModel / ExecuteBatch effects from a replay chain (shadow/v3 observability).
#[must_use]
pub fn replay_effect_counts(events: &[KernelEvent]) -> (u32, u32) {
    let effects = replay_turn_effects(events);
    let mut call_model = 0u32;
    let mut execute_batch = 0u32;
    for effect in effects {
        match effect {
            Effect::CallModel { .. } => call_model += 1,
            Effect::ExecuteBatch { .. } => execute_batch += 1,
            _ => {}
        }
    }
    (call_model, execute_batch)
}

/// Slice a turn log down to one step's events (from `ModelRequestIssued` through tool work).
#[must_use]
pub fn events_for_step(events: &[KernelEvent], step_idx: u32) -> Vec<KernelEvent> {
    let mut in_step = false;
    let mut out = Vec::new();
    for event in events {
        match event {
            KernelEvent::ModelRequestIssued { step_idx: s, .. } if *s == step_idx => {
                in_step = true;
                out.push(event.clone());
            }
            KernelEvent::ModelRequestIssued { .. } if in_step => break,
            KernelEvent::TurnEnded { .. } if in_step => break,
            _ if in_step => out.push(event.clone()),
            _ => {}
        }
    }
    out
}

/// Replay IO effects for a single step slice.
#[must_use]
pub fn replay_step_effects(events: &[KernelEvent], step_idx: u32) -> Vec<Effect> {
    replay_turn_effects(&events_for_step(events, step_idx))
}

/// Planned v3 step effects before tool outcomes are known (`ExecuteBatch` when tools planned).
#[must_use]
pub fn plan_v3_step_effects(token_budget: u32, tool_call_ids: &[String]) -> Vec<Effect> {
    let mut effects = vec![Effect::CallModel { token_budget }];
    for call_id in tool_call_ids {
        effects.push(Effect::ExecuteBatch {
            call_ids: vec![call_id.clone()],
        });
    }
    effects
}

/// Verify replay effect counts for one step match the executed tool batch size.
#[must_use]
pub fn verify_step_effect_parity(
    turn_events: &[KernelEvent],
    step_idx: u32,
    executed_tool_count: u32,
) -> Option<String> {
    let step_events = events_for_step(turn_events, step_idx);
    if step_events.is_empty() {
        return None;
    }
    let (call_model, execute_batch) = replay_effect_counts(&step_events);
    let mut diffs = Vec::new();
    if call_model != 1 {
        diffs.push(format!(
            "step {step_idx} CallModel replay count {call_model} != 1"
        ));
    }
    if execute_batch != executed_tool_count {
        diffs.push(format!(
            "step {step_idx} ExecuteBatch replay count {execute_batch} != executed {executed_tool_count}"
        ));
    }
    if diffs.is_empty() {
        None
    } else {
        Some(diffs.join("; "))
    }
}

impl Effect {
    #[must_use]
    pub fn kind_str(&self) -> &'static str {
        match self {
            Effect::CallModel { .. } => "call_model",
            Effect::ExecuteBatch { .. } => "execute_batch",
            Effect::RequestApproval { .. } => "request_approval",
            Effect::InjectSteer { .. } => "inject_steer",
            Effect::RunCompaction => "run_compaction",
            Effect::NotifyLsp { .. } => "notify_lsp",
            Effect::Sleep { .. } => "sleep",
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Map `TurnOutcomeStatus` (legacy) to the richer `TurnOutcome` (v3 schema).
#[must_use]
pub fn outcome_from_status(status: TurnOutcomeStatus, error: Option<String>) -> TurnOutcome {
    match status {
        TurnOutcomeStatus::Completed => TurnOutcome::Completed,
        TurnOutcomeStatus::Interrupted => TurnOutcome::Interrupted,
        TurnOutcomeStatus::Failed => TurnOutcome::Failed {
            message: error.unwrap_or_default(),
        },
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::engine::kernel_event::{KernelEvent, ToolOutcome, TurnOutcome};
    use crate::engine::request_fingerprint::RequestFingerprint;
    use crate::turn::TurnLoopMode;

    fn make_fp() -> RequestFingerprint {
        RequestFingerprint {
            static_prefix_sha256: "aaa".into(),
            full_prefix_sha256: "bbb".into(),
        }
    }

    #[test]
    fn projection_rebuilds_active_tool_names() {
        let events = vec![
            KernelEvent::TurnStarted {
                turn_id: "t1".into(),
                mode: TurnLoopMode::Agent,
                input_text: "test".into(),
                max_steps: 10,
            },
            KernelEvent::DeferredToolActivated {
                turn_id: "t1".into(),
                step_idx: 1,
                tool_name: "tool_search_bm25".into(),
            },
        ];
        let p = TurnKernelProjection::from_events(&events);
        assert!(p.active_tool_names.contains("tool_search_bm25"));
    }

    #[test]
    fn projection_resets_step_counters_on_model_request() {
        let events = vec![
            KernelEvent::ModelRequestIssued {
                turn_id: "t1".into(),
                step_idx: 1,
                request_fp: make_fp(),
                token_budget: 32000,
            },
            KernelEvent::ToolCallFinished {
                turn_id: "t1".into(),
                call_id: "c1".into(),
                tool_name: "read_file".into(),
                outcome: ToolOutcome::Success,
                duration_ms: 10,
                wrote_state: false,
            },
            // Second request resets counters.
            KernelEvent::ModelRequestIssued {
                turn_id: "t1".into(),
                step_idx: 2,
                request_fp: make_fp(),
                token_budget: 32000,
            },
        ];
        let p = TurnKernelProjection::from_events(&events);
        assert_eq!(p.readonly_tool_successes, 0);
        assert_eq!(p.step_idx, 2);
    }

    #[test]
    fn projection_tracks_scratchpad_summary_injected() {
        let events = vec![KernelEvent::ScratchpadSummaryInjected {
            turn_id: "t1".into(),
            at_step: 3,
        }];
        let p = TurnKernelProjection::from_events(&events);
        assert!(p.scratchpad_summary_injected);
    }

    #[test]
    fn projection_tracks_continuations() {
        let events = vec![
            KernelEvent::StepLimitContinuation {
                turn_id: "t1".into(),
                step_idx: 20,
                lht_objective_injected: true,
            },
            KernelEvent::LoopGuardContinuation {
                turn_id: "t1".into(),
                step_idx: 21,
            },
        ];
        let p = TurnKernelProjection::from_events(&events);
        assert_eq!(p.step_limit_continuations, 1);
        assert_eq!(p.loop_guard_continuations, 1);
    }

    #[test]
    fn projection_tracks_memory_plane_events() {
        let events = vec![
            KernelEvent::ScratchpadReminderInjected {
                turn_id: "t1".into(),
                step_idx: 2,
                area_path: "src/main.rs".into(),
            },
            KernelEvent::CompactionArtifactCreated {
                turn_id: "t1".into(),
                artifact_id: "art-1".into(),
                replaced_range: crate::engine::kernel_event::MessageRange { from: 1, to: 5 },
                summary_token_count: 120,
            },
            KernelEvent::CycleBriefingInjected {
                turn_id: "t1".into(),
                cycle: 2,
                step_idx: 3,
            },
        ];
        let p = TurnKernelProjection::from_events(&events);
        assert_eq!(p.scratchpad_reminder_count, 1);
        assert_eq!(p.compaction_artifact_count, 1);
        assert_eq!(p.cycle_briefing_count, 1);
        assert!(verify_memory_projection_chain(&events).is_none());
    }

    #[test]
    fn replay_coherence_passes_golden_fixtures() {
        let events = vec![
            KernelEvent::TurnStarted {
                turn_id: "t1".into(),
                mode: TurnLoopMode::Agent,
                input_text: "hi".into(),
                max_steps: 5,
            },
            KernelEvent::TurnEnded {
                turn_id: "t1".into(),
                outcome: TurnOutcome::Completed,
                total_steps: 1,
            },
        ];
        let report = replay_turn_projection(&events);
        assert_eq!(report.event_count, 2);
        assert_eq!(report.outcome, Some(TurnOutcome::Completed));
        assert!(verify_turn_replay_coherence(&events, None).is_none());
    }

    #[test]
    fn outcome_mapping_covers_all_statuses() {
        assert_eq!(
            outcome_from_status(TurnOutcomeStatus::Completed, None),
            TurnOutcome::Completed
        );
        assert_eq!(
            outcome_from_status(TurnOutcomeStatus::Interrupted, None),
            TurnOutcome::Interrupted
        );
        assert!(matches!(
            outcome_from_status(TurnOutcomeStatus::Failed, Some("boom".into())),
            TurnOutcome::Failed { message } if message == "boom"
        ));
    }

    #[test]
    fn emit_kernel_no_op_on_none() {
        // Should not panic.
        emit_kernel(None, KernelEvent::SchemaVersion { version: 1 });
    }

    #[test]
    fn emit_kernel_sends_when_some() {
        let (tx, mut rx) = mpsc::unbounded_channel::<KernelEvent>();
        emit_kernel(Some(&tx), KernelEvent::SchemaVersion { version: 1 });
        let ev = rx.try_recv().expect("event received");
        assert_eq!(ev.kind_str(), "schema_version");
    }

    #[test]
    fn compare_projection_detects_step_mismatch() {
        let live = LiveTurnSnapshot {
            turn_id: "t1".into(),
            step_idx: 5,
            max_steps: 20,
            ..Default::default()
        };
        let proj = TurnKernelProjection {
            turn_id: "t1".into(),
            step_idx: 3,
            max_steps: 20,
            ..Default::default()
        };
        let diff = compare_projection_to_live(&live, &proj);
        assert!(diff.is_some());
        assert!(diff.unwrap().contains("step_idx"));
    }

    #[test]
    fn replay_turn_machine_emits_call_model_effect() {
        let mut machine = ReplayTurnMachine;
        let proj = TurnKernelProjection::default();
        let out = machine.step(
            &proj,
            KernelEvent::ModelRequestIssued {
                turn_id: "t1".into(),
                step_idx: 1,
                request_fp: make_fp(),
                token_budget: 4096,
            },
        );
        assert_eq!(out.effects.len(), 1);
        assert!(matches!(
            out.effects[0],
            Effect::CallModel { token_budget: 4096 }
        ));
    }

    #[test]
    fn verify_effect_replay_chain_passes_minimal_turn() {
        let events = vec![
            KernelEvent::TurnStarted {
                turn_id: "t1".into(),
                mode: TurnLoopMode::Agent,
                input_text: "x".into(),
                max_steps: 5,
            },
            KernelEvent::ModelRequestIssued {
                turn_id: "t1".into(),
                step_idx: 1,
                request_fp: make_fp(),
                token_budget: 1024,
            },
            KernelEvent::TurnEnded {
                turn_id: "t1".into(),
                outcome: TurnOutcome::Completed,
                total_steps: 1,
            },
        ];
        assert!(verify_effect_replay_chain(&events).is_none());
    }

    #[test]
    fn verify_effect_replay_chain_detects_missing_turn_ended() {
        let events = vec![KernelEvent::TurnStarted {
            turn_id: "t1".into(),
            mode: TurnLoopMode::Agent,
            input_text: "x".into(),
            max_steps: 5,
        }];
        let msg = verify_effect_replay_chain(&events).expect("diff");
        assert!(msg.contains("TurnEnded"));
    }

    #[test]
    fn replay_turn_effects_matches_pure_read_fixture() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/harness/kernel-v3-replay/pure_read.json");
        let raw = std::fs::read_to_string(&path).expect("read fixture");
        let events: Vec<KernelEvent> = serde_json::from_str(&raw).expect("parse");
        let (call_model, execute_batch) = replay_effect_counts(&events);
        assert_eq!(call_model, 1);
        assert_eq!(execute_batch, 1);
        assert!(verify_effect_replay_chain(&events).is_none());
    }

    #[test]
    fn events_for_step_and_parity_on_pure_read_fixture() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/harness/kernel-v3-replay/pure_read.json");
        let raw = std::fs::read_to_string(&path).expect("read fixture");
        let events: Vec<KernelEvent> = serde_json::from_str(&raw).expect("parse");
        let step_events = events_for_step(&events, 1);
        assert_eq!(step_events.len(), 5);
        assert!(verify_step_effect_parity(&events, 1, 1).is_none());
        let planned = plan_v3_step_effects(8192, &["call-read-1".into()]);
        assert_eq!(planned.len(), 2);
    }

    #[test]
    fn kernel_resume_hints_from_latest_projection() {
        let events = vec![
            KernelEvent::TurnStarted {
                turn_id: "t-resume".into(),
                mode: TurnLoopMode::Agent,
                input_text: "hi".into(),
                max_steps: 12,
            },
            KernelEvent::TurnEnded {
                turn_id: "t-resume".into(),
                outcome: TurnOutcome::Completed,
                total_steps: 3,
            },
        ];
        let proj = TurnKernelProjection::from_events(&events);
        let hints = kernel_resume_hints_from_projection(&proj);
        assert_eq!(hints.latest_turn_id.as_deref(), Some("t-resume"));
        assert_eq!(hints.max_steps, 12);
    }

    #[test]
    fn replay_thread_message_stats_on_pure_read_fixture() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/harness/kernel-v3-replay/pure_read.json");
        let raw = std::fs::read_to_string(&path).expect("read fixture");
        let events: Vec<KernelEvent> = serde_json::from_str(&raw).expect("parse");
        let stats = replay_thread_message_stats(&[("t1".into(), events)]);
        assert_eq!(stats.model_message_count, 1);
        assert_eq!(stats.tool_call_planned_count, 1);
        assert_eq!(stats.model_request_count, 1);
    }

    #[test]
    fn verify_session_message_coverage_allows_equal_or_greater() {
        let stats = ThreadMessageReplayStats {
            turns_with_events: 1,
            model_message_count: 2,
            ..Default::default()
        };
        assert!(verify_session_message_coverage(3, &stats).is_none());
        assert!(verify_session_message_coverage(1, &stats).is_some());
        let cov = build_session_message_coverage(1, &stats).expect("coverage");
        assert!(!cov.coverage_ok);
        assert!(cov.summary.is_some());
        let ok = build_session_message_coverage(3, &stats).expect("coverage");
        assert!(ok.coverage_ok);
        assert!(ok.summary.is_none());
    }

    #[test]
    fn replay_thread_message_timeline_on_pure_read_fixture() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/harness/kernel-v3-replay/pure_read.json");
        let raw = std::fs::read_to_string(&path).expect("read fixture");
        let events: Vec<KernelEvent> = serde_json::from_str(&raw).expect("parse");
        let timeline = replay_thread_message_timeline(&[("t1".into(), events)]);
        assert_eq!(timeline.len(), 1);
        assert_eq!(timeline[0].step_idx, 1);
        assert_eq!(timeline[0].block_count, 2);
    }

    #[test]
    fn verify_message_timeline_coherence_on_pure_read_fixture() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/harness/kernel-v3-replay/pure_read.json");
        let raw = std::fs::read_to_string(&path).expect("read fixture");
        let events: Vec<KernelEvent> = serde_json::from_str(&raw).expect("parse");
        let turn_events = [("t1".into(), events)];
        let stats = replay_thread_message_stats(&turn_events);
        let timeline = replay_thread_message_timeline(&turn_events);
        assert!(verify_message_timeline_coherence(&stats, &timeline).is_none());
        assert!(verify_message_timeline_vs_session(2, &timeline).is_none());
        assert!(verify_message_timeline_vs_session(0, &timeline).is_some());
        assert!(verify_timeline_vs_request_count(&stats, &timeline).is_none());
    }

    #[test]
    fn build_session_message_timeline_coverage_on_pure_read_fixture() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/harness/kernel-v3-replay/pure_read.json");
        let raw = std::fs::read_to_string(&path).expect("read fixture");
        let events: Vec<KernelEvent> = serde_json::from_str(&raw).expect("parse");
        let projection = replay_thread_projection("t1", &[("t1".into(), events.clone())]);
        let cov = build_session_message_timeline_coverage(3, &projection, None).expect("coverage");
        assert!(cov.overall_ok);
        assert!(cov.plane_depth_ok);
        assert_eq!(cov.estimated_min_session_messages, 2);
        assert_eq!(
            projection
                .message_plane_index
                .estimated_min_session_messages,
            2
        );
        assert!(verify_step_model_message_anchor(&events, 1).is_none());
    }

    #[test]
    fn verify_session_message_plane_depth_on_pure_read_fixture() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/harness/kernel-v3-replay/pure_read.json");
        let raw = std::fs::read_to_string(&path).expect("read fixture");
        let events: Vec<KernelEvent> = serde_json::from_str(&raw).expect("parse");
        let projection = replay_thread_projection("t1", &[("t1".into(), events)]);
        assert!(verify_session_message_plane_depth(2, &projection.message_plane_index).is_none());
        assert!(verify_session_message_plane_depth(1, &projection.message_plane_index).is_some());
    }

    #[test]
    fn verify_session_role_index_on_pure_read_fixture() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/harness/kernel-v3-replay/pure_read.json");
        let raw = std::fs::read_to_string(&path).expect("read fixture");
        let events: Vec<KernelEvent> = serde_json::from_str(&raw).expect("parse");
        let projection = replay_thread_projection("t1", &[("t1".into(), events)]);
        let kernel = replay_kernel_message_role_estimate(&projection.message_stats);
        let role_index = SessionMessageRoleIndex {
            user_message_count: 2,
            assistant_message_count: 1,
            tool_result_message_count: 1,
            text_user_message_count: 1,
            total_message_count: 3,
        };
        assert!(verify_session_role_index(&role_index, &kernel).is_none());
        let thin = SessionMessageRoleIndex {
            assistant_message_count: 0,
            tool_result_message_count: 0,
            text_user_message_count: 0,
            ..role_index
        };
        assert!(verify_session_role_index(&thin, &kernel).is_some());
        let cov = build_session_message_timeline_coverage(3, &projection, Some(&role_index))
            .expect("coverage");
        assert!(cov.role_index_ok);
        assert!(cov.overall_ok);
    }

    #[test]
    fn build_session_message_role_index_counts_tool_results() {
        use crate::chat::{ContentBlock, Message};
        let messages = [
            Message {
                role: "user".to_string(),
                content: vec![ContentBlock::Text {
                    text: "hi".into(),
                    cache_control: None,
                }],
            },
            Message {
                role: "assistant".to_string(),
                content: vec![ContentBlock::Text {
                    text: "ok".into(),
                    cache_control: None,
                }],
            },
            Message {
                role: "user".to_string(),
                content: vec![ContentBlock::ToolResult {
                    tool_use_id: "t1".into(),
                    content: "done".into(),
                    is_error: None,
                    content_blocks: None,
                }],
            },
        ];
        let idx = build_session_message_role_index(&messages);
        assert_eq!(idx.user_message_count, 2);
        assert_eq!(idx.assistant_message_count, 1);
        assert_eq!(idx.tool_result_message_count, 1);
        assert_eq!(idx.text_user_message_count, 1);
        assert_eq!(idx.total_message_count, 3);
    }

    #[test]
    fn verify_session_memory_plane_user_depth_on_scratchpad_fixture() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/harness/kernel-v3-replay/scratchpad_compaction.json");
        let raw = std::fs::read_to_string(&path).expect("read fixture");
        let events: Vec<KernelEvent> = serde_json::from_str(&raw).expect("parse");
        let projection = replay_thread_projection("t1", &[("t1".into(), events)]);
        let memory = replay_kernel_memory_plane_user_estimate(&projection.message_stats);
        assert_eq!(memory.min_scratchpad_reminder_user_messages, 1);
        assert_eq!(memory.min_scratchpad_summary_user_messages, 1);
        assert_eq!(memory.min_memory_injected_user_messages, 2);
        let role_index = SessionMessageRoleIndex {
            user_message_count: 3,
            assistant_message_count: 0,
            tool_result_message_count: 0,
            text_user_message_count: 3,
            total_message_count: 3,
        };
        assert!(verify_session_memory_plane_user_depth(&role_index, &memory).is_none());
        let thin = SessionMessageRoleIndex {
            text_user_message_count: 1,
            ..role_index
        };
        assert!(verify_session_memory_plane_user_depth(&thin, &memory).is_some());
        let cov = build_session_message_timeline_coverage(3, &projection, Some(&role_index))
            .expect("coverage");
        assert!(cov.memory_plane_user_ok);
    }

    #[test]
    fn replay_thread_compaction_timeline_on_manual_compaction_fixture() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/harness/kernel-v3-replay/manual_compaction.json");
        let raw = std::fs::read_to_string(&path).expect("read fixture");
        let events: Vec<KernelEvent> = serde_json::from_str(&raw).expect("parse");
        let timeline = replay_thread_compaction_timeline(&[("t1".into(), events)]);
        assert_eq!(timeline.len(), 1);
        assert_eq!(timeline[0].replaced_from, 4);
        assert_eq!(timeline[0].replaced_to, 19);
        assert_eq!(timeline[0].messages_removed_count, 16);
        let index = replay_thread_compaction_index(&timeline);
        assert_eq!(index.messages_removed_estimate, 16);
        assert_eq!(index.peak_session_depth_hint, 20);
    }

    #[test]
    fn verify_session_compaction_depth_on_manual_compaction_fixture() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/harness/kernel-v3-replay/manual_compaction.json");
        let raw = std::fs::read_to_string(&path).expect("read fixture");
        let events: Vec<KernelEvent> = serde_json::from_str(&raw).expect("parse");
        let projection = replay_thread_projection("t1", &[("t1".into(), events)]);
        assert!(
            verify_session_compaction_depth(
                5,
                &projection.compaction_index,
                &projection.message_plane_index
            )
            .is_none()
        );
        let stats = ThreadMessageReplayStats {
            model_message_count: 10,
            tool_call_planned_count: 10,
            compaction_artifact_count: 1,
            ..Default::default()
        };
        let plane = replay_thread_message_plane_index(&stats);
        let compaction = ThreadCompactionReplayIndex {
            artifact_count: 1,
            messages_removed_estimate: 10,
            peak_session_depth_hint: 20,
        };
        assert!(verify_session_compaction_depth(0, &compaction, &plane).is_some());
        assert!(verify_session_compaction_depth(15, &compaction, &plane).is_none());
    }

    #[test]
    fn replay_thread_message_stats_counts_continuations_on_lht_fixture() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/harness/kernel-v3-replay/lht_continue.json");
        let raw = std::fs::read_to_string(&path).expect("read fixture");
        let events: Vec<KernelEvent> = serde_json::from_str(&raw).expect("parse");
        let stats = replay_thread_message_stats(&[("t1".into(), events)]);
        assert_eq!(stats.step_limit_continuation_count, 1);
        assert_eq!(stats.loop_guard_continuation_count, 1);
        assert_eq!(stats.steer_injection_count, 1);
        let memory = replay_kernel_memory_plane_user_estimate(&stats);
        assert_eq!(memory.min_continuation_user_messages, 2);
        assert_eq!(memory.min_memory_injected_user_messages, 3);
    }

    #[test]
    fn replay_thread_projection_picks_latest_non_empty_turn() {
        let good = vec![
            KernelEvent::TurnStarted {
                turn_id: "t1".into(),
                mode: TurnLoopMode::Agent,
                input_text: "a".into(),
                max_steps: 5,
            },
            KernelEvent::TurnEnded {
                turn_id: "t1".into(),
                outcome: TurnOutcome::Completed,
                total_steps: 1,
            },
        ];
        let later = vec![
            KernelEvent::TurnStarted {
                turn_id: "t2".into(),
                mode: TurnLoopMode::Agent,
                input_text: "b".into(),
                max_steps: 8,
            },
            KernelEvent::StepLimitContinuation {
                turn_id: "t2".into(),
                step_idx: 8,
                lht_objective_injected: true,
            },
            KernelEvent::TurnEnded {
                turn_id: "t2".into(),
                outcome: TurnOutcome::Completed,
                total_steps: 9,
            },
        ];
        let projection =
            replay_thread_projection("thread-x", &[("t1".into(), good), ("t2".into(), later)]);
        assert_eq!(projection.latest_turn_id.as_deref(), Some("t2"));
        assert_eq!(projection.latest_projection.turn_id, "t2");
        assert_eq!(projection.latest_projection.step_limit_continuations, 1);
        assert!(projection.report.all_coherent);
    }

    #[test]
    fn build_thread_replay_report_skips_empty_and_aggregates_coherence() {
        let good = vec![
            KernelEvent::TurnStarted {
                turn_id: "t-good".into(),
                mode: TurnLoopMode::Agent,
                input_text: "hi".into(),
                max_steps: 5,
            },
            KernelEvent::TurnEnded {
                turn_id: "t-good".into(),
                outcome: TurnOutcome::Completed,
                total_steps: 1,
            },
        ];
        let bad = vec![KernelEvent::TurnStarted {
            turn_id: "t-bad".into(),
            mode: TurnLoopMode::Agent,
            input_text: "x".into(),
            max_steps: 5,
        }];
        let report = build_thread_replay_report(
            "thread-1",
            &[
                ("t-empty".into(), vec![]),
                ("t-good".into(), good),
                ("t-bad".into(), bad),
            ],
        );
        assert_eq!(report.turn_count, 3);
        assert_eq!(report.turns_with_events, 2);
        assert_eq!(report.turns_coherent, 1);
        assert!(!report.all_coherent);
        assert_eq!(report.turns.len(), 2);
    }
}
