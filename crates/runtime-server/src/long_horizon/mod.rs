//! Long-horizon code task (LHT) harness — Phase 1 forced continue.

mod checkpoint;
mod cycle_band;
mod cycles;
pub(crate) mod handoff;
mod graph;
mod nudge;
mod objective;
pub(crate) mod progress;
pub(crate) mod snapshots;
mod reinject;
mod task_graph;
mod verify;

pub use checkpoint::tool_marks_lht_checkpoint;
pub use cycle_band::{
    context_pressure_ratio, in_lht_warning_band, should_lht_early_advance_cycle,
};
pub use cycles::build_cycles_value;
pub use reinject::{build_objective_reinject_message, should_reinject_this_step};
pub(crate) use nudge::VERIFICATION_RE;
pub(crate) use verify::verify_gate_verdict;

pub use graph::CodeTaskGraph;
pub use handoff::{build_lht_handoff_section, merge_lht_into_handoff};
pub use nudge::{build_nudge_message, LongHorizonSessionState, NudgeDecision};
pub use objective::derive_objective;
pub use task_graph::{
    build_task_graph_value, build_task_graph_value_with_telemetry, TaskGraphTelemetryJson,
};

use std::path::Path;

use deepseek_core::chat::{ContentBlock, Message};
use deepseek_core::long_horizon::LongHorizonConfig;
use deepseek_core::scratchpad::ScratchpadConfig;
use deepseek_core::task_type::TaskType;

use crate::agent_surface::AppMode;
use crate::tools::plan::SharedPlanState;
use crate::tools::todo::SharedTodoList;

/// Inputs for evaluating whether to inject an LHT continue nudge.
pub struct LongHorizonContinueInput<'a> {
    pub config: &'a LongHorizonConfig,
    pub scratchpad: &'a ScratchpadConfig,
    pub task_type: TaskType,
    pub app_mode: AppMode,
    pub workspace: &'a Path,
    pub scratchpad_run_id: Option<&'a str>,
    pub messages: &'a [Message],
    pub lang: &'a str,
    pub plan_state: &'a SharedPlanState,
    pub todos: &'a SharedTodoList,
    pub session: &'a mut LongHorizonSessionState,
    pub already_injected_this_turn: bool,
    pub steps_remaining: u32,
}

/// When audit scratchpad is active and incomplete, audit continue owns the path.
fn audit_scratchpad_blocks_lht(
    workspace: &Path,
    run_id: Option<&str>,
    scratchpad: &ScratchpadConfig,
    messages: &[Message],
) -> bool {
    crate::core::engine::scratchpad_flow::maybe_continue_incomplete_audit(
        workspace,
        run_id,
        scratchpad,
        messages,
    )
    .is_some()
}

/// Outcome of the LHT continue gate. `Skip` carries a stable diagnostic reason
/// (§4.9 observability) so the caller can emit a `long_horizon.gate_skip` event
/// pinpointing *which* guard suppressed the nudge.
pub enum LhtGateOutcome {
    Nudge(Message),
    /// DEMO3 false-green guard: the task graph is otherwise *complete*, but one
    /// or more completed checklist items are runnable acceptances that were
    /// never actually verified (no `[verify:]` prefix and no matching recent
    /// exec). Carries a focused nudge to force real verification. Distinct from
    /// [`Self::Nudge`] so the caller can emit a separate observability node and
    /// avoid muddling the normal continue/conversion telemetry.
    NudgeUnverifiedAcceptance(Message),
    Skip(&'static str),
}

/// Build a continue nudge user message, or `Skip(reason)` when LHT should not fire.
pub async fn maybe_continue_incomplete_code_task(
    input: LongHorizonContinueInput<'_>,
) -> LhtGateOutcome {
    if !input.config.enabled {
        return LhtGateOutcome::Skip("disabled");
    }
    if input.already_injected_this_turn {
        return LhtGateOutcome::Skip("already_injected_this_turn");
    }
    if input.session.paused {
        return LhtGateOutcome::Skip("session_paused");
    }
    if !input.task_type.uses_code_tool_surface() {
        return LhtGateOutcome::Skip("not_code_task");
    }
    if input.app_mode == AppMode::Plan {
        return LhtGateOutcome::Skip("plan_mode");
    }
    if audit_scratchpad_blocks_lht(
        input.workspace,
        input.scratchpad_run_id,
        input.scratchpad,
        input.messages,
    ) {
        return LhtGateOutcome::Skip("audit_owns_path");
    }

    let plan = input.plan_state.lock().await.snapshot();
    let checklist = input.todos.lock().await.snapshot();
    let mut graph = CodeTaskGraph::from_snapshots(&plan, &checklist);

    if graph.is_empty() {
        return LhtGateOutcome::Skip("graph_empty");
    }
    if !graph.incomplete() {
        // DEMO3 root-cause guard: a "complete" graph can still be a false green
        // when a *completed* checklist item reads like a runnable acceptance
        // (build / tests pass / run examples) yet was never actually verified —
        // no `[verify:]` prefix AND no matching recent exec. Rather than let the
        // turn end on that, nudge (bounded) to force real verification. This does
        // NOT touch `completion_pct` / `graph.incomplete()` — the displayed
        // progress stays 100% (DEMO5 #1); only the turn-ending decision is gated.
        let unverified: Vec<String> = checklist
            .items
            .iter()
            .filter(|i| i.status == crate::tools::todo::TodoStatus::Completed)
            .filter(|i| {
                verify::verify_gate_verdict(
                    &i.content,
                    &input.session.recent_verification_cmds,
                    input.lang,
                )
                .0 == "unverified_acceptance"
            })
            .map(|i| verify::strip_verify_prefix(&i.content))
            .collect();
        if !unverified.is_empty()
            && input.session.unverified_acceptance_nudges < nudge::MAX_UNVERIFIED_ACCEPTANCE_NUDGES
        {
            input.session.unverified_acceptance_nudges += 1;
            let text = nudge::build_unverified_acceptance_nudge(&unverified, input.lang);
            return LhtGateOutcome::NudgeUnverifiedAcceptance(Message {
                role: "user".to_string(),
                content: vec![ContentBlock::Text {
                    text,
                    cache_control: None,
                }],
            });
        }
        return LhtGateOutcome::Skip("graph_complete");
    }
    if graph.is_trivial() {
        return LhtGateOutcome::Skip("graph_trivial");
    }

    let (objective, source) = derive_objective(&plan, &checklist, input.messages, input.lang);
    graph.objective = objective;
    graph.objective_source = source;

    let stale = input.session.stale_assistant_turns >= nudge::STALE_ASSISTANT_TURNS;

    // Objective progress signal (§4.8): did the git working tree change since the
    // last nudge? Computed once here (gate已触发，频率低), off the async pool.
    let current_git_signature = if input.config.progress_via_git {
        let ws = input.workspace.to_path_buf();
        tokio::task::spawn_blocking(move || progress::workspace_change_signature(&ws))
            .await
            .ok()
            .flatten()
    } else {
        None
    };
    let git_progress = match (
        current_git_signature.as_ref(),
        input.session.last_nudge_git_signature.as_ref(),
    ) {
        (Some(cur), Some(prev)) => cur != prev,
        _ => false,
    };

    let had_progress = input.session.progress_since_last_nudge || git_progress;
    input.session.progress_since_last_nudge = false;

    // Telemetry (§4.9): a prior nudge that is now followed by qualified progress
    // counts as "converted" — evidence the nudge actually helped.
    if had_progress && input.session.awaiting_nudge_outcome {
        input.session.telemetry.converted += 1;
        input.session.awaiting_nudge_outcome = false;
    }

    let was_blocked = input.session.tracker.is_blocked();
    let decision =
        input
            .session
            .tracker
            .prepare_nudge(graph.in_progress_id, input.config, had_progress);
    match decision {
        NudgeDecision::Skip => return LhtGateOutcome::Skip("nudge_skip"),
        NudgeDecision::MaxReached => return LhtGateOutcome::Skip("nudge_max_reached"),
        NudgeDecision::Blocked => {
            if !was_blocked {
                input.session.telemetry.blocked += 1;
            }
            return LhtGateOutcome::Skip("nudge_blocked");
        }
        NudgeDecision::Nudge { .. } => {}
    }

    let turn_limit_warning = input.steps_remaining <= 3;
    let text = build_nudge_message(
        &graph,
        &graph.objective,
        input.lang,
        turn_limit_warning,
        stale,
    );

    // Record this nudge: store the git baseline for next-turn comparison and
    // arm conversion tracking.
    input.session.last_nudge_git_signature = current_git_signature;
    input.session.telemetry.emitted += 1;
    input.session.awaiting_nudge_outcome = true;

    LhtGateOutcome::Nudge(Message {
        role: "user".to_string(),
        content: vec![ContentBlock::Text {
            text,
            cache_control: None,
        }],
    })
}
