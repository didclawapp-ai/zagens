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
                _ => {}
            }
        }
    }
    stats
}

/// Best-effort coverage check: session JSON message count vs kernel log counters (observability).
#[must_use]
pub fn verify_session_message_coverage(
    session_message_count: usize,
    stats: &ThreadMessageReplayStats,
) -> Option<String> {
    if stats.model_message_count == 0 {
        return None;
    }
    // Assistant turns in session often pair with model messages; allow slack for system/user-only rows.
    let expected_min = stats.model_message_count as usize;
    if session_message_count >= expected_min {
        return None;
    }
    Some(format!(
        "session messages ({session_message_count}) below kernel model_message events ({expected_min})"
    ))
}

/// Build thread replay report and the latest non-empty turn projection (resume substrate).
#[must_use]
pub fn replay_thread_projection(
    thread_id: &str,
    turn_events: &[(String, Vec<KernelEvent>)],
) -> ThreadReplayProjection {
    let report = build_thread_replay_report(thread_id, turn_events);
    let message_stats = replay_thread_message_stats(turn_events);
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
    }
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
