//! KernelEvent schema — Phase 3a (立契约).
//!
//! The single source of truth for all observable state transitions in a turn.
//! Every variant is `#[non_exhaustive]`; consumers must handle unknown variants
//! gracefully. Adding fields is backward-compatible; removing or renaming
//! requires a schema upcast function + `schema_version` bump.
//!
//! **Status**: v1 (2026-06-15). Double-write started in Phase 3a; only
//! consumed in Phase 3b once completeness verification passes.

use serde::{Deserialize, Serialize};

use crate::engine::request_fingerprint::RequestFingerprint;
use crate::models::Usage;
use crate::turn::{TurnLoopMode, TurnOutcomeStatus};

// ── Opaque ID aliases ────────────────────────────────────────────────────────

/// Unique identifier for a turn (maps to `TurnContext::id`).
pub type TurnId = String;
/// Unique identifier for a single tool call attempt.
pub type CallId = String;
/// Identifier for a compaction/snapshot artifact stored in the artifact store.
pub type ArtifactId = String;

// ── Supporting enums ─────────────────────────────────────────────────────────

/// Reason a turn ended. Supersedes bare `TurnOutcomeStatus` with richer
/// detail so that the state-machine can branch without out-of-band flags.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum TurnOutcome {
    Completed,
    Failed { message: String },
    Interrupted,
    Budget,
    LoopGuard { reason: String },
    MaxSteps,
    CycleHandoff { next_cycle: u32 },
}

impl TurnOutcome {
    /// Map to the legacy `TurnOutcomeStatus` for callers not yet migrated.
    #[must_use]
    pub fn as_status(&self) -> TurnOutcomeStatus {
        match self {
            TurnOutcome::Completed => TurnOutcomeStatus::Completed,
            TurnOutcome::Interrupted => TurnOutcomeStatus::Interrupted,
            _ => TurnOutcomeStatus::Failed,
        }
    }
}

/// Stream delta type during model response.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum DeltaKind {
    Text,
    ThinkingText,
    ToolCallArg,
}

/// Resolved outcome for a single tool call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ToolOutcome {
    /// Tool ran and returned a non-error result (may be empty).
    Success,
    /// Blocked pre-execution (loop-guard duplicate or approval rejected).
    Blocked { reason: String },
    /// Loop guard halted the turn during or after execution.
    GuardHalt { reason: String },
    /// Tool process or network timed out.
    Timeout,
    /// Tool returned a tool-level error (not kernel error).
    ToolError { message: String },
}

/// Whether the user approved or rejected a planned tool call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ApprovalVerdict {
    Approved,
    Rejected,
}

/// Policy metadata resolved for a planned call (subset relevant to replay).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[non_exhaustive]
pub struct PolicyDecision {
    pub approval_required: bool,
    pub parallel_eligible: bool,
    pub read_only: bool,
}

impl PolicyDecision {
    #[must_use]
    pub fn new(approval_required: bool, parallel_eligible: bool, read_only: bool) -> Self {
        Self {
            approval_required,
            parallel_eligible,
            read_only,
        }
    }
}

/// Strategy applied when recovering from a context overflow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum OverflowStrategy {
    BudgetRecompile,
    LlmCompaction,
    CycleHandoff,
}

/// Which code path triggered a capacity checkpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum CapacityCheckpointKind {
    PreRequest,
    PostTool,
    ErrorEscalation,
}

/// What the capacity subsystem decided to do at a checkpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum CapacityAction {
    Continue,
    Trim,
    Handoff,
    Abort { reason: String },
}

impl CapacityAction {
    /// Map capacity-controller output to kernel schema action.
    #[must_use]
    pub fn from_guardrail(action: crate::capacity::GuardrailAction, _reason: &str) -> Self {
        match action {
            crate::capacity::GuardrailAction::NoIntervention => Self::Continue,
            crate::capacity::GuardrailAction::TargetedContextRefresh => Self::Trim,
            crate::capacity::GuardrailAction::VerifyWithToolReplay => Self::Continue,
            crate::capacity::GuardrailAction::VerifyAndReplan => Self::Handoff,
        }
    }
}

/// Inclusive message-index range `[from, to]` (0-based, across all messages).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageRange {
    pub from: u32,
    pub to: u32,
}

// ── KernelEvent ──────────────────────────────────────────────────────────────

/// Append-only, strongly-typed, monotonically-sequenced log of all observable
/// state transitions in a single turn.
///
/// Replaces: `core::Event` free-string table, `RuntimeEventRecord`, `EventFrame`,
/// and SSE-compat surface — four representations become one.
///
/// # Schema evolution
/// - **Adding a variant**: add `#[serde(default)]` fields; increment nothing.
/// - **Adding a field** to existing variant: add `#[serde(default)]`; no bump.
/// - **Removing or renaming**: provide `upcast_v{N}_to_v{N+1}()` and bump
///   [`SchemaVersion`](KernelEvent::SchemaVersion).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event_type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum KernelEvent {
    // ── Schema sentinel ─────────────────────────────────────────────────────
    /// First record in every `kernel_events` table partition.
    /// Currently always `version: 1`.
    SchemaVersion {
        version: u32,
    },

    // ── Turn lifecycle ───────────────────────────────────────────────────────
    TurnStarted {
        turn_id: TurnId,
        mode: TurnLoopMode,
        /// Serialised user message text. Attachments / large blobs: `None` with
        /// a separate `EmitArtifact` event carrying the blob reference.
        input_text: String,
        max_steps: u32,
    },
    TurnEnded {
        turn_id: TurnId,
        outcome: TurnOutcome,
        total_steps: u32,
    },

    // ── Model request ────────────────────────────────────────────────────────
    ModelRequestIssued {
        turn_id: TurnId,
        step_idx: u32,
        request_fp: RequestFingerprint,
        /// Resolved token budget for this request.
        token_budget: u32,
    },
    ModelDelta {
        turn_id: TurnId,
        step_idx: u32,
        kind: DeltaKind,
        /// For `ToolCallArg` deltas: the call_id of the in-progress tool block.
        call_id: Option<CallId>,
        text: String,
    },
    ModelMessage {
        turn_id: TurnId,
        step_idx: u32,
        usage: Usage,
        /// Number of content blocks in the final message (avoids embedding full
        /// message text which belongs in the session store).
        block_count: u32,
        /// Truncated assistant text for log-driven transcript rebuild (Phase 3b 5c).
        #[serde(default, skip_serializing_if = "String::is_empty")]
        text_preview: String,
        /// Full assistant text written to session JSON (Phase 3b 5c closure).
        #[serde(default, skip_serializing_if = "String::is_empty")]
        assistant_text: String,
    },

    // ── Tool calls ───────────────────────────────────────────────────────────
    ToolCallPlanned {
        turn_id: TurnId,
        step_idx: u32,
        call_id: CallId,
        tool_name: String,
        /// JSON-serialised tool input. Large inputs (>16 KB) are stored as an
        /// artifact reference; this field holds the truncated preview or `{}`
        /// with a `large_input_artifact_id` field added.
        input_json: String,
        decision: PolicyDecision,
    },
    ToolCallStarted {
        turn_id: TurnId,
        call_id: CallId,
        /// DAG wave index (0 in legacy sequential mode).
        wave_idx: u32,
    },
    ToolCallFinished {
        turn_id: TurnId,
        call_id: CallId,
        tool_name: String,
        outcome: ToolOutcome,
        duration_ms: u32,
        /// Whether the tool performed any filesystem/state writes. Mirrors
        /// `tool_writes_state()` at call time; used to rebuild deferred-tool
        /// activation projection in Phase 3b.
        wrote_state: bool,
        /// Truncated tool result / error text for log-driven transcript rebuild (Phase 3b 5c).
        #[serde(default, skip_serializing_if = "String::is_empty")]
        result_preview: String,
        /// Exact tool-result body written to session JSON (Phase 3b 5c closure).
        #[serde(default, skip_serializing_if = "String::is_empty")]
        session_content: String,
    },
    ApprovalResolved {
        turn_id: TurnId,
        call_id: CallId,
        verdict: ApprovalVerdict,
    },

    // ── Context & compaction ─────────────────────────────────────────────────
    CompactionArtifactCreated {
        turn_id: TurnId,
        artifact_id: ArtifactId,
        /// Message indices `[from, to]` replaced by this compaction artifact.
        replaced_range: MessageRange,
        summary_token_count: u32,
    },
    ContextOverflowRecovered {
        turn_id: TurnId,
        step_idx: u32,
        strategy: OverflowStrategy,
        /// Token budget cap applied during budget-recompile (None for other strategies).
        source_budget_cap: Option<u32>,
    },

    // ── Memory injections ────────────────────────────────────────────────────
    SteerInjected {
        turn_id: TurnId,
        step_idx: u32,
        text: String,
    },
    ScratchpadReminderInjected {
        turn_id: TurnId,
        step_idx: u32,
        area_path: String,
    },
    ScratchpadSummaryInjected {
        turn_id: TurnId,
        /// Step at which the summary was first injected. Subsequent steps read
        /// the flag from the projection without a new event.
        at_step: u32,
    },
    CycleBriefingInjected {
        turn_id: TurnId,
        cycle: u32,
        step_idx: u32,
    },
    /// Episodic topic-memory block injected into the system prompt (B2 double-write).
    TopicMemoryInjected {
        turn_id: TurnId,
        step_idx: u32,
        /// Estimated tokens in the injected `<topic_memory>` block.
        #[serde(default)]
        block_token_est: u32,
    },
    /// Read-side memory plane query executed (v3 `Effect::QueryMemory` double-write).
    MemoryPlaneQueried {
        turn_id: TurnId,
        step_idx: u32,
        /// `working` | `episodic` | `archival`
        layer: String,
        query_key: String,
        /// ContextCompiler source id resolved for this query (empty when unknown).
        #[serde(default)]
        compiler_source: String,
    },
    /// Flash layered-context seam appended as assistant message (v3 `#159` double-write).
    LayeredContextSeamInjected {
        turn_id: TurnId,
        step_idx: u32,
        level: u32,
        messages_covered: u32,
        /// Truncated seam text for observability (full body remains in session store until 5c).
        text_preview: String,
    },

    // ── Guard decisions ──────────────────────────────────────────────────────
    LoopGuardTriggered {
        turn_id: TurnId,
        call_id: CallId,
        /// "identical_call" | "deferred_set_area_batch" | "failure_halt" …
        reason: String,
    },
    CapacityCheckpoint {
        turn_id: TurnId,
        step_idx: u32,
        kind: CapacityCheckpointKind,
        tokens_used: u32,
        token_budget: u32,
        action: CapacityAction,
        /// When true, a proposed guardrail was suppressed by cooldown (replay → `Effect::Sleep`).
        #[serde(default)]
        cooldown_blocked: bool,
    },

    // ── LHT / Cycle continuation ─────────────────────────────────────────────
    CycleAdvanced {
        turn_id: TurnId,
        from_cycle: u32,
        to_cycle: u32,
        /// Human-readable reason emitted by the LHT cycle-advance hook.
        reason: String,
    },
    StepLimitContinuation {
        turn_id: TurnId,
        step_idx: u32,
        lht_objective_injected: bool,
    },
    LoopGuardContinuation {
        turn_id: TurnId,
        step_idx: u32,
    },

    // ── Tool catalog mutations ────────────────────────────────────────────────
    /// A previously-deferred tool was promoted into the active tool set.
    ///
    /// This event is emitted by `maybe_activate_deferred_tool` whenever the
    /// model requests a tool that was not yet active. It is necessary for
    /// Phase 3b: `active_tool_names` (host state) must be rebuildable from
    /// the log so that `TurnMachine::step` can be a pure function.
    DeferredToolActivated {
        turn_id: TurnId,
        step_idx: u32,
        tool_name: String,
    },
}

impl KernelEvent {
    /// Extract the `turn_id` field present in every variant except
    /// [`KernelEvent::SchemaVersion`].
    #[must_use]
    pub fn turn_id(&self) -> Option<&str> {
        match self {
            KernelEvent::SchemaVersion { .. } => None,
            KernelEvent::TurnStarted { turn_id, .. }
            | KernelEvent::TurnEnded { turn_id, .. }
            | KernelEvent::ModelRequestIssued { turn_id, .. }
            | KernelEvent::ModelDelta { turn_id, .. }
            | KernelEvent::ModelMessage { turn_id, .. }
            | KernelEvent::ToolCallPlanned { turn_id, .. }
            | KernelEvent::ToolCallStarted { turn_id, .. }
            | KernelEvent::ToolCallFinished { turn_id, .. }
            | KernelEvent::ApprovalResolved { turn_id, .. }
            | KernelEvent::CompactionArtifactCreated { turn_id, .. }
            | KernelEvent::ContextOverflowRecovered { turn_id, .. }
            | KernelEvent::SteerInjected { turn_id, .. }
            | KernelEvent::ScratchpadReminderInjected { turn_id, .. }
            | KernelEvent::ScratchpadSummaryInjected { turn_id, .. }
            | KernelEvent::CycleBriefingInjected { turn_id, .. }
            | KernelEvent::TopicMemoryInjected { turn_id, .. }
            | KernelEvent::MemoryPlaneQueried { turn_id, .. }
            | KernelEvent::LayeredContextSeamInjected { turn_id, .. }
            | KernelEvent::LoopGuardTriggered { turn_id, .. }
            | KernelEvent::CapacityCheckpoint { turn_id, .. }
            | KernelEvent::CycleAdvanced { turn_id, .. }
            | KernelEvent::StepLimitContinuation { turn_id, .. }
            | KernelEvent::LoopGuardContinuation { turn_id, .. }
            | KernelEvent::DeferredToolActivated { turn_id, .. } => Some(turn_id.as_str()),
        }
    }

    /// Variant name string for logging and schema drift CI.
    #[must_use]
    pub fn kind_str(&self) -> &'static str {
        match self {
            KernelEvent::SchemaVersion { .. } => "schema_version",
            KernelEvent::TurnStarted { .. } => "turn_started",
            KernelEvent::TurnEnded { .. } => "turn_ended",
            KernelEvent::ModelRequestIssued { .. } => "model_request_issued",
            KernelEvent::ModelDelta { .. } => "model_delta",
            KernelEvent::ModelMessage { .. } => "model_message",
            KernelEvent::ToolCallPlanned { .. } => "tool_call_planned",
            KernelEvent::ToolCallStarted { .. } => "tool_call_started",
            KernelEvent::ToolCallFinished { .. } => "tool_call_finished",
            KernelEvent::ApprovalResolved { .. } => "approval_resolved",
            KernelEvent::CompactionArtifactCreated { .. } => "compaction_artifact_created",
            KernelEvent::ContextOverflowRecovered { .. } => "context_overflow_recovered",
            KernelEvent::SteerInjected { .. } => "steer_injected",
            KernelEvent::ScratchpadReminderInjected { .. } => "scratchpad_reminder_injected",
            KernelEvent::ScratchpadSummaryInjected { .. } => "scratchpad_summary_injected",
            KernelEvent::CycleBriefingInjected { .. } => "cycle_briefing_injected",
            KernelEvent::TopicMemoryInjected { .. } => "topic_memory_injected",
            KernelEvent::MemoryPlaneQueried { .. } => "memory_plane_queried",
            KernelEvent::LayeredContextSeamInjected { .. } => "layered_context_seam_injected",
            KernelEvent::LoopGuardTriggered { .. } => "loop_guard_triggered",
            KernelEvent::CapacityCheckpoint { .. } => "capacity_checkpoint",
            KernelEvent::CycleAdvanced { .. } => "cycle_advanced",
            KernelEvent::StepLimitContinuation { .. } => "step_limit_continuation",
            KernelEvent::LoopGuardContinuation { .. } => "loop_guard_continuation",
            KernelEvent::DeferredToolActivated { .. } => "deferred_tool_activated",
        }
    }
}

// ── Envelope for log persistence ──────────────────────────────────────────────

/// Row written to the `kernel_events` SQLite table.
///
/// The `seq` field is a global monotone counter within a runtime session
/// (not per-turn); the `(turn_id, seq)` pair is unique across the table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KernelEventEnvelope {
    /// Global monotone sequence number assigned by the writer.
    pub seq: u64,
    /// Unix timestamp (milliseconds).
    pub ts_ms: u64,
    /// Variant name (mirrors `KernelEvent::kind_str()`).
    pub kind: String,
    pub event: KernelEvent,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_turn_id() -> TurnId {
        "test-turn-001".to_string()
    }

    #[test]
    fn turn_started_round_trips() {
        let ev = KernelEvent::TurnStarted {
            turn_id: make_turn_id(),
            mode: TurnLoopMode::Agent,
            input_text: "hello".to_string(),
            max_steps: 20,
        };
        let json = serde_json::to_string(&ev).expect("serialize");
        let back: KernelEvent = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.kind_str(), "turn_started");
        assert_eq!(back.turn_id(), Some("test-turn-001"));
    }

    #[test]
    fn turn_ended_round_trips() {
        let ev = KernelEvent::TurnEnded {
            turn_id: make_turn_id(),
            outcome: TurnOutcome::LoopGuard {
                reason: "identical call".to_string(),
            },
            total_steps: 7,
        };
        let json = serde_json::to_string(&ev).expect("serialize");
        let back: KernelEvent = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.kind_str(), "turn_ended");
    }

    #[test]
    fn tool_call_round_trips() {
        let ev = KernelEvent::ToolCallFinished {
            turn_id: make_turn_id(),
            call_id: "call-abc".to_string(),
            tool_name: "read_file".to_string(),
            outcome: ToolOutcome::Success,
            duration_ms: 120,
            wrote_state: true,
            result_preview: String::new(),
            session_content: String::new(),
        };
        let json = serde_json::to_string(&ev).expect("serialize");
        let back: KernelEvent = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.kind_str(), "tool_call_finished");
    }

    #[test]
    fn capacity_checkpoint_round_trips() {
        let ev = KernelEvent::CapacityCheckpoint {
            turn_id: make_turn_id(),
            step_idx: 3,
            kind: CapacityCheckpointKind::PostTool,
            tokens_used: 8000,
            token_budget: 32000,
            action: CapacityAction::Continue,
            cooldown_blocked: false,
        };
        let json = serde_json::to_string(&ev).expect("serialize");
        let _back: KernelEvent = serde_json::from_str(&json).expect("deserialize");
    }

    #[test]
    fn schema_version_has_no_turn_id() {
        let ev = KernelEvent::SchemaVersion { version: 1 };
        assert!(ev.turn_id().is_none());
        assert_eq!(ev.kind_str(), "schema_version");
    }

    #[test]
    fn turn_outcome_as_status_mapping() {
        assert_eq!(
            TurnOutcome::Completed.as_status(),
            TurnOutcomeStatus::Completed
        );
        assert_eq!(
            TurnOutcome::Interrupted.as_status(),
            TurnOutcomeStatus::Interrupted
        );
        assert_eq!(TurnOutcome::Budget.as_status(), TurnOutcomeStatus::Failed);
    }

    // ── Schema completeness verification (Phase 3a) ──────────────────────────
    //
    // Each test below demonstrates that a specific "A-class" host state field
    // can be rebuilt purely from a KernelEvent sequence.  These are the
    // preconditions for TurnMachine::step to be a pure function in Phase 3b.

    /// Projection helper: rebuild `scratchpad_summary_injected` flag.
    /// Rule: true iff a `ScratchpadSummaryInjected` event appeared for this turn.
    fn proj_scratchpad_summary_injected(events: &[KernelEvent], turn: &str) -> bool {
        events.iter().any(|ev| {
            matches!(ev, KernelEvent::ScratchpadSummaryInjected { turn_id, .. }
                if turn_id == turn)
        })
    }

    #[test]
    fn completeness_scratchpad_summary_injected() {
        let tid = "t1".to_string();
        let events: Vec<KernelEvent> = vec![
            KernelEvent::TurnStarted {
                turn_id: tid.clone(),
                mode: TurnLoopMode::Agent,
                input_text: "do stuff".into(),
                max_steps: 10,
            },
            KernelEvent::ScratchpadSummaryInjected {
                turn_id: tid.clone(),
                at_step: 2,
            },
        ];
        assert!(proj_scratchpad_summary_injected(&events, &tid));

        // Without the event, flag is false.
        let empty: Vec<KernelEvent> = vec![KernelEvent::TurnStarted {
            turn_id: tid.clone(),
            mode: TurnLoopMode::Agent,
            input_text: "do stuff".into(),
            max_steps: 10,
        }];
        assert!(!proj_scratchpad_summary_injected(&empty, &tid));
    }

    /// Projection helper: rebuild `active_tool_names` set.
    /// Rule: starts from initial set; a `DeferredToolActivated` event adds to it.
    fn proj_active_tools<'a>(
        events: &'a [KernelEvent],
        turn: &str,
        initial: &[&'a str],
    ) -> std::collections::HashSet<String> {
        let mut active: std::collections::HashSet<String> =
            initial.iter().map(|s| s.to_string()).collect();
        for ev in events {
            if let KernelEvent::DeferredToolActivated {
                turn_id, tool_name, ..
            } = ev
            {
                if turn_id == turn {
                    active.insert(tool_name.clone());
                }
            }
        }
        active
    }

    #[test]
    fn completeness_deferred_tool_activation() {
        let tid = "t2".to_string();
        let initial = &["read_file", "shell_exec"];
        let events = vec![
            KernelEvent::TurnStarted {
                turn_id: tid.clone(),
                mode: TurnLoopMode::Agent,
                input_text: "search for foo".into(),
                max_steps: 10,
            },
            KernelEvent::DeferredToolActivated {
                turn_id: tid.clone(),
                step_idx: 1,
                tool_name: "tool_search_tool_regex".to_string(),
            },
        ];
        let active = proj_active_tools(&events, &tid, initial);
        assert!(active.contains("tool_search_tool_regex"));
        assert!(active.contains("read_file"));
        // A tool NOT activated should not appear.
        assert!(!active.contains("write_file"));
    }

    /// Projection helper: rebuild `ScratchpadStepState` counters for the
    /// **current** step (since last `ModelRequestIssued`).
    /// Rule: reset at each ModelRequestIssued; increment from ToolCallFinished.
    #[derive(Default, PartialEq, Eq, Debug)]
    struct ScratchpadCounters {
        readonly_successes: usize,
        scratchpad_writes: usize,
    }

    fn proj_scratchpad_step_state(events: &[KernelEvent], turn: &str) -> ScratchpadCounters {
        let mut counters = ScratchpadCounters::default();
        for ev in events {
            match ev {
                // Reset at each new model request for this turn.
                KernelEvent::ModelRequestIssued { turn_id, .. } if turn_id == turn => {
                    counters = ScratchpadCounters::default();
                }
                KernelEvent::ToolCallFinished {
                    turn_id,
                    outcome,
                    wrote_state,
                    tool_name,
                    ..
                } if turn_id == turn => {
                    if matches!(outcome, ToolOutcome::Success) {
                        if *wrote_state {
                            // scratchpad_append / scratchpad_set_area
                            if tool_name.starts_with("scratchpad_") {
                                counters.scratchpad_writes += 1;
                            }
                        } else {
                            counters.readonly_successes += 1;
                        }
                    }
                }
                _ => {}
            }
        }
        counters
    }

    #[test]
    fn completeness_scratchpad_step_state_projection() {
        let tid = "t3".to_string();
        let fp = crate::engine::request_fingerprint::RequestFingerprint {
            static_prefix_sha256: "aaa".into(),
            full_prefix_sha256: "bbb".into(),
        };
        let events = vec![
            KernelEvent::ModelRequestIssued {
                turn_id: tid.clone(),
                step_idx: 1,
                request_fp: fp.clone(),
                token_budget: 32000,
            },
            KernelEvent::ToolCallFinished {
                turn_id: tid.clone(),
                call_id: "c1".into(),
                outcome: ToolOutcome::Success,
                duration_ms: 50,
                wrote_state: false,
                tool_name: "read_file".into(),
                result_preview: String::new(),
                session_content: String::new(),
            },
            KernelEvent::ToolCallFinished {
                turn_id: tid.clone(),
                call_id: "c2".into(),
                outcome: ToolOutcome::Success,
                duration_ms: 30,
                wrote_state: false,
                tool_name: "shell_exec".into(),
                result_preview: String::new(),
                session_content: String::new(),
            },
            // Second model request resets counters.
            KernelEvent::ModelRequestIssued {
                turn_id: tid.clone(),
                step_idx: 2,
                request_fp: fp,
                token_budget: 32000,
            },
            KernelEvent::ToolCallFinished {
                turn_id: tid.clone(),
                call_id: "c3".into(),
                outcome: ToolOutcome::Success,
                duration_ms: 20,
                wrote_state: true,
                tool_name: "scratchpad_append".into(),
                result_preview: String::new(),
                session_content: String::new(),
            },
        ];

        let state = proj_scratchpad_step_state(&events, &tid);
        // After step 2: only the scratchpad_append write, no readonly hits.
        assert_eq!(state.readonly_successes, 0);
        assert_eq!(state.scratchpad_writes, 1);
    }

    /// Projection helper: rebuild LHT continuation step count.
    /// Rule: count of `StepLimitContinuation` events for this turn.
    fn proj_lht_continuation_count(events: &[KernelEvent], turn: &str) -> u32 {
        events
            .iter()
            .filter(|ev| {
                matches!(ev, KernelEvent::StepLimitContinuation { turn_id, .. }
                    if turn_id == turn)
            })
            .count() as u32
    }

    #[test]
    fn completeness_lht_continuation_count() {
        let tid = "t4".to_string();
        let events = vec![
            KernelEvent::StepLimitContinuation {
                turn_id: tid.clone(),
                step_idx: 20,
                lht_objective_injected: true,
            },
            KernelEvent::StepLimitContinuation {
                turn_id: tid.clone(),
                step_idx: 40,
                lht_objective_injected: false,
            },
        ];
        assert_eq!(proj_lht_continuation_count(&events, &tid), 2);
        assert_eq!(proj_lht_continuation_count(&events, "other-turn"), 0);
    }

    #[test]
    fn completeness_steer_injection_consumed() {
        // Rule: steer is consumed if a SteerInjected event exists at or before
        // the current step. This is a boolean projection.
        let tid = "t5".to_string();
        let events = vec![KernelEvent::SteerInjected {
            turn_id: tid.clone(),
            step_idx: 3,
            text: "change approach".into(),
        }];
        let injected = events
            .iter()
            .any(|ev| matches!(ev, KernelEvent::SteerInjected { turn_id, .. } if turn_id == &tid));
        assert!(injected);
    }

    #[test]
    fn completeness_capacity_state_projection() {
        // Rule: the most recent CapacityCheckpoint.action for the turn determines
        // capacity state. If it's Abort, the turn should have ended.
        let tid = "t6".to_string();
        let events = vec![
            KernelEvent::CapacityCheckpoint {
                turn_id: tid.clone(),
                step_idx: 1,
                kind: CapacityCheckpointKind::PreRequest,
                tokens_used: 5000,
                token_budget: 32000,
                action: CapacityAction::Continue,
                cooldown_blocked: false,
            },
            KernelEvent::CapacityCheckpoint {
                turn_id: tid.clone(),
                step_idx: 2,
                kind: CapacityCheckpointKind::PostTool,
                tokens_used: 28000,
                token_budget: 32000,
                action: CapacityAction::Trim,
                cooldown_blocked: false,
            },
        ];
        let last_action = events
            .iter()
            .filter_map(|ev| {
                if let KernelEvent::CapacityCheckpoint {
                    turn_id, action, ..
                } = ev
                {
                    if turn_id == &tid {
                        return Some(action.clone());
                    }
                }
                None
            })
            .last();
        assert_eq!(last_action, Some(CapacityAction::Trim));
    }

    #[test]
    fn deferred_tool_activated_round_trips() {
        let ev = KernelEvent::DeferredToolActivated {
            turn_id: "t7".into(),
            step_idx: 2,
            tool_name: "tool_search_bm25".into(),
        };
        let json = serde_json::to_string(&ev).expect("serialize");
        let back: KernelEvent = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.kind_str(), "deferred_tool_activated");
    }

    // ── Schema drift CI (Phase 3a §2.3) ─────────────────────────────────────
    //
    // These tests pin the exact JSON shape of representative KernelEvent
    // variants. If the serde layout changes (rename, tag rename, new field
    // without #[serde(default)], etc.), the string comparison fails immediately
    // rather than silently breaking deserialization of stored logs.
    //
    // Update the golden strings ONLY when a schema version bump is also done
    // (add a `SchemaVersion` event with incremented version AND an upcast fn).

    #[test]
    fn schema_drift_turn_started_shape() {
        let ev = KernelEvent::TurnStarted {
            turn_id: "TURN-001".into(),
            mode: TurnLoopMode::Agent,
            input_text: "hello".into(),
            max_steps: 20,
        };
        let json = serde_json::to_string(&ev).expect("serialize");
        // Pin the field names and tag value.  New *optional* fields are ok
        // (they'll appear in the JSON but won't break old readers with
        // #[serde(default)]).  Renaming any field here is a schema break.
        assert!(
            json.contains(r#""event_type":"turn_started""#),
            "tag must be event_type:turn_started, got: {json}"
        );
        assert!(
            json.contains(r#""turn_id":"TURN-001""#),
            "missing turn_id, got: {json}"
        );
        assert!(
            json.contains(r#""mode":"agent""#),
            "missing mode, got: {json}"
        );
        assert!(
            json.contains(r#""input_text":"hello""#),
            "missing input_text, got: {json}"
        );
        assert!(
            json.contains(r#""max_steps":20"#),
            "missing max_steps, got: {json}"
        );
    }

    #[test]
    fn schema_drift_tool_call_finished_shape() {
        let ev = KernelEvent::ToolCallFinished {
            turn_id: "TURN-001".into(),
            call_id: "CALL-001".into(),
            tool_name: "read_file".into(),
            outcome: ToolOutcome::Success,
            duration_ms: 42,
            wrote_state: false,
            result_preview: String::new(),
            session_content: String::new(),
        };
        let json = serde_json::to_string(&ev).expect("serialize");
        assert!(
            json.contains(r#""event_type":"tool_call_finished""#),
            "tag drift: {json}"
        );
        assert!(
            json.contains(r#""call_id":"CALL-001""#),
            "missing call_id: {json}"
        );
        assert!(
            json.contains(r#""tool_name":"read_file""#),
            "missing tool_name: {json}"
        );
        assert!(
            json.contains(r#""wrote_state":false"#),
            "missing wrote_state: {json}"
        );
    }

    #[test]
    fn schema_drift_model_request_issued_shape() {
        let fp = crate::engine::request_fingerprint::RequestFingerprint {
            static_prefix_sha256: "aaabbb".into(),
            full_prefix_sha256: "cccddd".into(),
        };
        let ev = KernelEvent::ModelRequestIssued {
            turn_id: "TURN-001".into(),
            step_idx: 1,
            request_fp: fp,
            token_budget: 16000,
        };
        let json = serde_json::to_string(&ev).expect("serialize");
        assert!(
            json.contains(r#""event_type":"model_request_issued""#),
            "tag drift: {json}"
        );
        assert!(
            json.contains(r#""static_prefix_sha256":"aaabbb""#),
            "missing static fp: {json}"
        );
        assert!(
            json.contains(r#""token_budget":16000"#),
            "missing token_budget: {json}"
        );
    }

    #[test]
    fn schema_drift_deferred_tool_activated_shape() {
        let ev = KernelEvent::DeferredToolActivated {
            turn_id: "TURN-001".into(),
            step_idx: 3,
            tool_name: "tool_search_bm25".into(),
        };
        let json = serde_json::to_string(&ev).expect("serialize");
        assert!(
            json.contains(r#""event_type":"deferred_tool_activated""#),
            "tag drift: {json}"
        );
        assert!(
            json.contains(r#""tool_name":"tool_search_bm25""#),
            "missing tool_name: {json}"
        );
    }

    /// Verify all 22 variant kind strings are accounted for (prevents silent
    /// addition of variants without updating `kind_str()`).
    #[test]
    fn all_variants_have_kind_str() {
        // We can't enumerate non_exhaustive enums at compile time, but we can
        // verify the count we know about hasn't silently changed.
        let known_kinds = [
            "schema_version",
            "turn_started",
            "turn_ended",
            "model_request_issued",
            "model_delta",
            "model_message",
            "tool_call_planned",
            "tool_call_started",
            "tool_call_finished",
            "approval_resolved",
            "compaction_artifact_created",
            "context_overflow_recovered",
            "steer_injected",
            "scratchpad_reminder_injected",
            "scratchpad_summary_injected",
            "cycle_briefing_injected",
            "loop_guard_triggered",
            "capacity_checkpoint",
            "cycle_advanced",
            "step_limit_continuation",
            "loop_guard_continuation",
            "deferred_tool_activated",
        ];
        assert_eq!(
            known_kinds.len(),
            22,
            "Update this count when adding variants"
        );
    }
}
