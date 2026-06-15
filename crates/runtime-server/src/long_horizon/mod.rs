//! Long-horizon code task (LHT) harness — Phase 1 forced continue.

pub(crate) mod adversarial_audit;
mod checkpoint;
mod completion_audit;
mod completion_gate_flow;
pub(crate) mod completion_gate_panel;
mod cycle_band;
mod cycles;
mod deliverable_manifest;
mod gate_telemetry;
mod generic_gate;
mod go_toolchain_audit;
mod graph;
pub(crate) mod handoff;
mod integration_gate;
pub(crate) mod macro_loop;
pub(crate) mod macro_loop_panel;
mod manifest_gate;
mod nudge;
mod objective;
mod plan_drift;
pub(crate) mod progress;
mod reinject;
pub(crate) mod snapshots;
mod stub_gate;
mod task_graph;
mod verify;
mod verify_platform;

pub use checkpoint::tool_marks_lht_checkpoint;
pub use cycle_band::{context_pressure_ratio, in_lht_warning_band, should_lht_early_advance_cycle};
pub use cycles::build_cycles_value;
pub(crate) use nudge::VERIFICATION_RE;
pub use reinject::{build_objective_reinject_message, should_reinject_this_step};
pub(crate) use verify::{parse_all_req_tags, verify_gate_verdict};

pub use completion_gate_panel::CompletionGatePanelJson;
pub use graph::CodeTaskGraph;
pub use handoff::{build_lht_handoff_section, merge_lht_into_handoff};
pub use manifest_gate::CompletionGateExec;
pub use nudge::{
    LongHorizonSessionState, NudgeDecision, build_auto_continue_message, build_nudge_message,
};
pub use objective::derive_objective;
pub use task_graph::{
    TaskGraphTelemetryJson, build_task_graph_value, build_task_graph_value_with_telemetry,
};

use std::path::Path;
use std::sync::Arc;

use zagens_core::chat::{ContentBlock, LlmClient, Message};
use zagens_core::long_horizon::{CompletionGateMode, GenericGateMode, LhtMode, LongHorizonConfig};
use zagens_core::scratchpad::ScratchpadConfig;
use zagens_core::task_type::TaskType;

use crate::agent_surface::AppMode;
use crate::tools::plan::SharedPlanState;
use crate::tools::todo::SharedTodoList;

/// Inputs for evaluating whether to inject an LHT continue nudge.
pub struct LongHorizonContinueInput<'a> {
    pub config: &'a LongHorizonConfig,
    /// Per-turn override of `config.mode` from the UI LHT toggle (`None` = use
    /// the configured default). Lets a session force `Strict` without editing
    /// `config.toml`.
    pub lht_mode_override: Option<LhtMode>,
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
    pub thread_id: &'a str,
    pub already_injected_this_turn: bool,
    pub steps_remaining: u32,
    /// Shell execution for layer-2 manifest oracle (§6.4). `None` skips active exec.
    pub gate_exec: Option<CompletionGateExec<'a>>,
    /// LLM client for §6.7 adversarial audit (single read-only `create_message`
    /// call). `None` skips the audit even when enabled in config.
    pub llm_client: Option<Arc<dyn LlmClient>>,
    /// Model name to use for the adversarial audit call.
    pub llm_model: &'a str,
}

/// When audit scratchpad is active and incomplete, audit continue owns the path.
fn audit_scratchpad_blocks_lht(
    workspace: &Path,
    run_id: Option<&str>,
    scratchpad: &ScratchpadConfig,
    messages: &[Message],
) -> bool {
    crate::core::engine::scratchpad_flow::maybe_continue_incomplete_audit(
        workspace, run_id, scratchpad, messages,
    )
    .is_some()
}

/// Count assistant tool-use blocks across the thread — a task-agnostic "real
/// work is underway" signal used by the strict-mode plan-bootstrap gate so it
/// only fires once the model has actually started doing things without a plan.
fn count_tool_uses(messages: &[Message]) -> usize {
    messages
        .iter()
        .flat_map(|m| m.content.iter())
        .filter(|b| matches!(b, ContentBlock::ToolUse { .. }))
        .count()
}

/// Strict-mode plan-bootstrap gate (empty graph): if the thread already shows
/// substantive tool activity but no plan/checklist, force the model to plan
/// first (bounded). Returns `None` to fall through to the normal `graph_empty`
/// skip (trivial / too-early / rounds exhausted).
fn evaluate_plan_bootstrap(
    messages: &[Message],
    session: &mut LongHorizonSessionState,
    lang: &str,
) -> Option<LhtGateOutcome> {
    if count_tool_uses(messages) < nudge::MIN_TOOL_USES_FOR_PLAN_GATE {
        return None; // trivial / too early — don't force a plan
    }
    if session.plan_gate_rounds >= nudge::MAX_PLAN_GATE_ROUNDS {
        // Model won't plan after repeated nudges — stop honestly, don't loop.
        session
            .pending_gate_events
            .push(gate_telemetry::CompletionGateEvent::plan_gate(
                false,
                session.plan_gate_rounds,
            ));
        return None;
    }
    session.plan_gate_rounds += 1;
    session
        .pending_gate_events
        .push(gate_telemetry::CompletionGateEvent::plan_gate(
            true,
            session.plan_gate_rounds,
        ));
    Some(LhtGateOutcome::NudgePlanRequired(Message {
        role: "user".to_string(),
        content: vec![ContentBlock::Text {
            text: nudge::build_plan_required_nudge(lang),
            cache_control: None,
        }],
    }))
}

/// In strict mode, return a copy of the completion-gate config with layer-2/3
/// gates raised to `enforce` (P0-1): top-level `mode`, `stub_gate`, and the
/// task-agnostic layer-2 sources `auto_verify_replay` / `toolchain_gate`.
fn strict_completion_gate(
    base: &zagens_core::long_horizon::CompletionGateConfig,
    mode: LhtMode,
) -> zagens_core::long_horizon::CompletionGateConfig {
    let mut gate = base.clone();
    if mode.is_strict() {
        if gate.mode != CompletionGateMode::Enforce {
            gate.mode = CompletionGateMode::Enforce;
        }
        if !gate.stub_gate.is_enforce() {
            gate.stub_gate = GenericGateMode::Enforce;
        }
        if gate.auto_verify_replay.is_on() && !gate.auto_verify_replay.is_enforce() {
            gate.auto_verify_replay = GenericGateMode::Enforce;
        }
        if gate.toolchain_gate.is_on() && !gate.toolchain_gate.is_enforce() {
            gate.toolchain_gate = GenericGateMode::Enforce;
        }
    }
    gate
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
    /// P0-2: completed items carry `[verify: cmd]` but no matching recent exec
    /// (`verify_gate verdict=mismatch`). Distinct telemetry from unverified.
    NudgeVerifyMismatch(Message),
    /// P0-3: checklist is "complete" but too few items carry `[verify:]` — the
    /// MicroStack false-green pattern (many `untagged_ok`, build/vet only).
    NudgeInsufficientVerify(Message),
    /// P1-5: checklist marks a plan phase done while that plan step is still
    /// pending/in_progress — sync plan before ending the turn.
    NudgePlanChecklistDrift(Message),
    /// P1′: enforce-mode cross-layer integration gaps (`electron/` remains, …).
    NudgeIntegrationIncomplete(Message),
    /// Layer-2 manifest gate: harness-active verify commands failed (§6.1).
    NudgeManifestFailed(Message),
    /// Generic stub / incompleteness gate: blocking-class markers (`todo!()`,
    /// `unimplemented!()`, `NotImplementedError`, "not implemented" throws) were
    /// found in `enforce` mode. Distinct from [`Self::NudgeManifestFailed`] so the
    /// LHT panel / telemetry show the "compiles but feature is a stub"
    /// false-completion block separately from verify-command failures.
    NudgeStubsFound(Message),
    /// Strict-mode plan-bootstrap gate: a code-surface task is proceeding with an
    /// **empty** task graph (no plan/checklist) despite real tool activity. The
    /// runtime forces the model to establish a visible plan before continuing,
    /// so the rest of the LHT net (progress, completion gate, stub gate) cannot
    /// be silently bypassed by never planning. Strict mode only.
    NudgePlanRequired(Message),
    /// Layer-3 deliverable manifest reconciliation failed (§6.2).
    NudgeDeliverablesMissing(Message),
    /// Observe mode: record gaps but allow `graph_complete` (§7.3).
    ObserveManifestGate {
        failing_gate_ids: Vec<String>,
        audit: Option<completion_audit::CompletionAuditResult>,
    },
    /// Bounded gate rounds exhausted — honest stop without fake green (§7.1).
    AuditUnmet {
        reason: &'static str,
        failing_gates: Vec<String>,
        missing_deliverable_ids: Vec<String>,
        manifest_round: u32,
        audit_round: u32,
        first_gap_count: Option<u32>,
    },
    /// Phase 4: spawn CRAFT Review sub-agent (harness-driven).
    MacroCraftSpawn {
        task_id: String,
    },
    /// Phase 4: remediation segment after blockers → checklist.
    MacroRemediation(Message),
    /// Phase 4: macro cycles exhausted with open CRAFT gaps.
    MacroUnmet {
        remaining_blockers: Vec<String>,
        macro_cycles_used: u32,
    },
    /// §6.7: adversarial audit found gaps in enforce mode — reinject nudge.
    NudgeAdversarialGaps(Message),
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

    let effective_mode = input.lht_mode_override.unwrap_or(input.config.mode);

    let plan = input.plan_state.lock().await.snapshot();
    let checklist = input.todos.lock().await.snapshot();
    let mut graph = CodeTaskGraph::from_snapshots(&plan, &checklist);

    if graph.is_empty() {
        // Strict mode: a code task may not free-style past an empty graph once
        // real work is underway. Force a plan first so the rest of the net is
        // not silently bypassed. Auto mode keeps the historical skip.
        if effective_mode.is_strict()
            && let Some(outcome) =
                evaluate_plan_bootstrap(input.messages, &mut *input.session, input.lang)
        {
            return outcome;
        }
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
        // P0-2: `[verify:]` prefix present but no matching recent exec — tagged
        // without running. Check after unverified so both guards can fire on
        // separate turns (bounded each).
        let mismatched: Vec<(String, String)> = checklist
            .items
            .iter()
            .filter(|i| i.status == crate::tools::todo::TodoStatus::Completed)
            .filter_map(|i| {
                if verify::verify_gate_verdict(
                    &i.content,
                    &input.session.recent_verification_cmds,
                    input.lang,
                )
                .0 != "mismatch"
                {
                    return None;
                }
                verify::parse_verify_command(&i.content)
                    .map(|cmd| (verify::strip_verify_prefix(&i.content), cmd))
            })
            .collect();
        if !mismatched.is_empty()
            && input.session.verify_mismatch_nudges < nudge::MAX_VERIFY_MISMATCH_NUDGES
        {
            input.session.verify_mismatch_nudges += 1;
            let text = nudge::build_verify_mismatch_nudge(&mismatched, input.lang);
            return LhtGateOutcome::NudgeVerifyMismatch(Message {
                role: "user".to_string(),
                content: vec![ContentBlock::Text {
                    text,
                    cache_control: None,
                }],
            });
        }
        // P0-3: large checklist with zero `[verify:]` tags — block graph_complete
        // on build/vet/toolchain alone (MicroStack01/02 false-green pattern).
        let completed_count = checklist
            .items
            .iter()
            .filter(|i| i.status == crate::tools::todo::TodoStatus::Completed)
            .count();
        let verify_tagged_count = checklist
            .items
            .iter()
            .filter(|i| i.status == crate::tools::todo::TodoStatus::Completed)
            .filter(|i| verify::parse_verify_command(&i.content).is_some())
            .count();
        if completed_count >= nudge::MIN_CHECKLIST_ITEMS_FOR_VERIFY_RATIO
            && verify_tagged_count < nudge::MIN_VERIFY_TAGGED_ITEMS
            && input.session.insufficient_verify_nudges < nudge::MAX_INSUFFICIENT_VERIFY_NUDGES
        {
            input.session.insufficient_verify_nudges += 1;
            let text = nudge::build_insufficient_verify_nudge(completed_count, input.lang);
            return LhtGateOutcome::NudgeInsufficientVerify(Message {
                role: "user".to_string(),
                content: vec![ContentBlock::Text {
                    text,
                    cache_control: None,
                }],
            });
        }
        // P1-5: plan/checklist drift — checklist names a phase as done while
        // the matching plan step is still open.
        let drift = plan_drift::find_plan_checklist_drift(&plan, &checklist);
        if !drift.is_empty()
            && input.session.plan_checklist_drift_nudges < nudge::MAX_PLAN_CHECKLIST_DRIFT_NUDGES
        {
            input.session.plan_checklist_drift_nudges += 1;
            let text = nudge::build_plan_checklist_drift_nudge(&drift, input.lang);
            return LhtGateOutcome::NudgePlanChecklistDrift(Message {
                role: "user".to_string(),
                content: vec![ContentBlock::Text {
                    text,
                    cache_control: None,
                }],
            });
        }
        // Strict mode tightens the completion/stub gates to `enforce` so the
        // full net (false-green + stub block) applies even if the operator left
        // them on `observe` in config — the user explicitly opted into LHT.
        let effective_gate = strict_completion_gate(&input.config.completion_gate, effective_mode);
        let gate_outcome = completion_gate_flow::evaluate_completion_gate(
            input.workspace,
            &effective_gate,
            &checklist,
            input.session,
            input.lang,
            input.steps_remaining,
            input.gate_exec.as_ref(),
        )
        .await;
        let latest_user = latest_user_text(input.messages);
        if matches!(&gate_outcome, LhtGateOutcome::Skip("graph_complete")) {
            let macro_outcome = macro_loop::evaluate_macro_loop(
                macro_loop::MacroLoopInput {
                    config: &input.config.macro_loop,
                    effective_mode,
                    workspace: input.workspace,
                    checklist: &checklist,
                    session: input.session,
                    lang: input.lang,
                    thread_id: input.thread_id,
                    latest_user_text: latest_user,
                },
                macro_loop::MacroTrigger::MicroPass,
            );
            let macro_gate = macro_loop::macro_outcome_to_gate(macro_outcome);
            if !matches!(macro_gate, LhtGateOutcome::Skip(_)) {
                return macro_gate;
            }
            // §6.7 adversarial audit: only runs when macro loop is inactive and
            // machine gates are green.  No release/veto power — observe mode just
            // emits telemetry; enforce mode adds gap candidates as checklist items.
            if input.config.adversarial_audit.enabled {
                if let Some(client) = input.llm_client.as_deref() {
                    if let Some(audit_result) = adversarial_audit::run_adversarial_audit(
                        &input.config.adversarial_audit,
                        input.session,
                        client,
                        input.llm_model,
                        input.workspace,
                        &checklist,
                        input.lang,
                    )
                    .await
                    {
                        use serde_json;
                        let payload_json = serde_json::to_string(&audit_result).unwrap_or_default();
                        input.session.pending_gate_events.push(
                            gate_telemetry::CompletionGateEvent::AdversarialAudit { payload_json },
                        );
                        if !audit_result.was_bounded
                            && !audit_result.gap_candidates.is_empty()
                            && input.config.adversarial_audit.mode
                                == zagens_core::long_horizon::CompletionGateMode::Enforce
                        {
                            let msg = adversarial_audit::build_gap_reinject_message(
                                &audit_result.gap_candidates,
                                input.lang,
                            );
                            return LhtGateOutcome::NudgeAdversarialGaps(msg);
                        }
                    }
                }
            }
            return LhtGateOutcome::Skip("graph_complete");
        }
        if let LhtGateOutcome::AuditUnmet { reason, .. } = &gate_outcome {
            nudge::capture_manifest_gate_hints(input.session);
            let macro_outcome = macro_loop::evaluate_macro_loop(
                macro_loop::MacroLoopInput {
                    config: &input.config.macro_loop,
                    effective_mode,
                    workspace: input.workspace,
                    checklist: &checklist,
                    session: input.session,
                    lang: input.lang,
                    thread_id: input.thread_id,
                    latest_user_text: latest_user,
                },
                macro_loop::MacroTrigger::AuditUnmet { reason },
            );
            if !matches!(macro_outcome, macro_loop::MacroLoopOutcome::Inactive) {
                return macro_loop::macro_outcome_to_gate(macro_outcome);
            }
        }
        return gate_outcome;
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
    let git_progress = progress::git_counts_as_progress(
        input.session,
        current_git_signature.as_ref(),
        input.session.last_nudge_git_signature.as_ref(),
    );

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

fn latest_user_text(messages: &[Message]) -> Option<&str> {
    messages
        .iter()
        .rev()
        .find(|m| m.role == "user")
        .and_then(|m| {
            m.content.iter().find_map(|b| match b {
                ContentBlock::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
        })
}

#[cfg(test)]
mod plan_bootstrap_tests {
    use super::*;

    fn tool_use_msg(id: &str) -> Message {
        Message {
            role: "assistant".to_string(),
            content: vec![ContentBlock::ToolUse {
                id: id.to_string(),
                name: "read_file".to_string(),
                input: serde_json::json!({}),
                caller: None,
            }],
        }
    }

    fn text_msg(t: &str) -> Message {
        Message {
            role: "assistant".to_string(),
            content: vec![ContentBlock::Text {
                text: t.to_string(),
                cache_control: None,
            }],
        }
    }

    #[test]
    fn count_tool_uses_only_counts_tool_use_blocks() {
        let msgs = vec![text_msg("hi"), tool_use_msg("a"), tool_use_msg("b")];
        assert_eq!(count_tool_uses(&msgs), 2);
    }

    #[test]
    fn too_few_tool_uses_does_not_force_plan() {
        let mut session = LongHorizonSessionState::default();
        // Only one tool call — trivial / too early, must fall through.
        let msgs = vec![tool_use_msg("a")];
        assert!(evaluate_plan_bootstrap(&msgs, &mut session, "en").is_none());
        assert_eq!(session.plan_gate_rounds, 0);
    }

    #[test]
    fn real_work_without_plan_forces_plan_then_gives_up() {
        let mut session = LongHorizonSessionState::default();
        let msgs: Vec<Message> = (0..nudge::MIN_TOOL_USES_FOR_PLAN_GATE)
            .map(|i| tool_use_msg(&format!("t{i}")))
            .collect();

        // First MAX rounds nudge to establish a plan.
        for round in 1..=nudge::MAX_PLAN_GATE_ROUNDS {
            let out = evaluate_plan_bootstrap(&msgs, &mut session, "en");
            assert!(matches!(out, Some(LhtGateOutcome::NudgePlanRequired(_))));
            assert_eq!(session.plan_gate_rounds, round);
        }
        // Beyond the bound: give up (honest stop), do not loop forever.
        assert!(evaluate_plan_bootstrap(&msgs, &mut session, "en").is_none());
        assert_eq!(session.plan_gate_rounds, nudge::MAX_PLAN_GATE_ROUNDS);
    }

    #[test]
    fn strict_completion_gate_raises_modes() {
        use zagens_core::long_horizon::{
            CompletionGateConfig, CompletionGateMode, GenericGateMode,
        };
        let mut base = CompletionGateConfig::default();
        base.auto_verify_replay = GenericGateMode::Observe;
        base.toolchain_gate = GenericGateMode::Observe;
        let auto = strict_completion_gate(&base, LhtMode::Auto);
        assert_eq!(auto.mode, base.mode);
        let strict = strict_completion_gate(&base, LhtMode::Strict);
        assert_eq!(strict.mode, CompletionGateMode::Enforce);
        assert!(strict.stub_gate.is_enforce());
        assert_eq!(strict.auto_verify_replay, GenericGateMode::Enforce);
        assert_eq!(strict.toolchain_gate, GenericGateMode::Enforce);
    }
}
