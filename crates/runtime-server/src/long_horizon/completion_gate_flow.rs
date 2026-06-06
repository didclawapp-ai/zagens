//! Composable harness gate orchestration (layers 2+3 at `graph_complete`).
//!
//! Layer-2 entries come from three sources merged into one harness-active run:
//! the operator manifest (`gate.verify`), the model's own `[verify:]` replay,
//! and the toolchain-detected build/test gate (see [`super::generic_gate`]).
//! Each entry's [`VerifySource`] picks which mode (operator `mode` /
//! `auto_verify_replay` / `toolchain_gate`) governs it, so a single run can mix
//! enforce (force rework) and observe (record only) failures.

use std::collections::HashMap;
use std::path::Path;

use deepseek_core::chat::{ContentBlock, Message};
use deepseek_core::long_horizon::{
    CompletionGateConfig, CompletionGateMode, CompletionGateVerifyEntry, VerifySource,
};

use super::LhtGateOutcome;
use super::completion_audit::{CompletionAuditResult, audit_deliverables_async};
use super::deliverable_manifest;
use super::gate_telemetry::CompletionGateEvent;
use super::generic_gate::{
    collect_model_verify_entries, detect_toolchain_entries, merge_verify_entries,
    resolve_project_root,
};
use super::integration_gate;
use super::manifest_gate::{
    CompletionGateExec, ManifestGateResult, VerifyExitClass, VerifyRunResult, run_manifest_gate,
};
use super::nudge::{
    LongHorizonSessionState, build_deliverables_failed_nudge, build_manifest_failed_nudge,
};
use super::progress::workspace_change_signature;
use super::stub_gate;
use crate::tools::todo::TodoListSnapshot;

/// Is a failing entry from this source a hard (enforce) failure?
fn source_enforced(source: VerifySource, gate: &CompletionGateConfig) -> bool {
    match source {
        VerifySource::Operator => gate.mode == CompletionGateMode::Enforce,
        VerifySource::ModelDeclared => gate.auto_verify_replay.is_enforce(),
        VerifySource::Toolchain => gate.toolchain_gate.is_enforce(),
    }
}

/// Evaluate completion gate when the task graph is otherwise complete.
pub async fn evaluate_completion_gate(
    workspace: &Path,
    gate: &CompletionGateConfig,
    checklist: &TodoListSnapshot,
    session: &mut LongHorizonSessionState,
    lang: &str,
    steps_remaining: u32,
    exec: Option<&CompletionGateExec<'_>>,
) -> LhtGateOutcome {
    if !gate.is_active() {
        return LhtGateOutcome::Skip("graph_complete");
    }
    if session.completion_gate_evaluating {
        return LhtGateOutcome::Skip("manifest_gate_reentry");
    }
    session.completion_gate_evaluating = true;
    session.pending_gate_events.clear();

    let runtime_deliverables =
        deliverable_manifest::merge_runtime_deliverables(workspace, &gate.deliverable);

    CompletionGateEvent::queue_manifest_start(
        &mut session.pending_gate_events,
        gate.verify.len() as u32,
        runtime_deliverables.len() as u32,
        gate.mode,
    );

    let pre_git = workspace_change_signature(workspace);
    let outcome = evaluate_completion_gate_inner(
        workspace,
        gate,
        checklist,
        session,
        lang,
        steps_remaining,
        exec,
        &runtime_deliverables,
    )
    .await;

    if let Some(post) = workspace_change_signature(workspace) {
        session.last_nudge_git_signature = Some(post.clone());
        session.harness_side_effect_signature = pre_git;
        session.suppress_git_progress_baseline = Some(post);
    }
    session.completion_gate_evaluating = false;
    outcome
}

/// Failure classification for one layer-2 run, split by governing mode.
struct Layer2Split {
    enforced_failing: Vec<String>,
    observed_failing: Vec<String>,
}

fn classify_failures(
    result: &ManifestGateResult,
    entries: &[CompletionGateVerifyEntry],
    gate: &CompletionGateConfig,
) -> Layer2Split {
    let source_by_id: HashMap<&str, VerifySource> =
        entries.iter().map(|e| (e.id.as_str(), e.source)).collect();
    let mut enforced_failing = Vec::new();
    let mut observed_failing = Vec::new();
    for r in &result.results {
        let is_fail = r.exit_code != 0 || r.exit_class != VerifyExitClass::Ok;
        if !is_fail {
            continue;
        }
        let source = source_by_id
            .get(r.id.as_str())
            .copied()
            .unwrap_or(VerifySource::Operator);
        if source_enforced(source, gate) {
            enforced_failing.push(r.id.clone());
        } else {
            observed_failing.push(r.id.clone());
        }
    }
    Layer2Split {
        enforced_failing,
        observed_failing,
    }
}

#[allow(clippy::too_many_arguments)]
async fn evaluate_completion_gate_inner(
    workspace: &Path,
    gate: &CompletionGateConfig,
    checklist: &TodoListSnapshot,
    session: &mut LongHorizonSessionState,
    lang: &str,
    steps_remaining: u32,
    exec: Option<&CompletionGateExec<'_>>,
    runtime_deliverables: &[deepseek_core::long_horizon::CompletionGateDeliverableEntry],
) -> LhtGateOutcome {
    // Task-agnostic stub / incompleteness gate (§ stub gate). Runs first because
    // it is a cheap filesystem scan (no command exec): if the workspace still
    // ships blocking-class stubs (`todo!()` / `unimplemented!()` /
    // `NotImplementedError` / "not implemented" throws) there is no point
    // running the build/test gate to "prove" a green that hides a missing
    // feature. `observe` records counts and falls through; `enforce` blocks.
    if gate.stub_gate.is_on()
        && let Some(outcome) =
            evaluate_stub_gate(workspace, gate, session, lang, steps_remaining).await
    {
        return outcome;
    }

    // Resolve where build/test commands actually run. Normally the workspace
    // root is the project root, but a model can nest the whole project one
    // level down (e.g. `microstack/go.mod`); running `go build ./...` from the
    // workspace root then fails with "no go.mod" — a false gate failure. The
    // toolchain detector and the verify executor both use this resolved root so
    // a nested module no longer breaks the gate. Layer-3 deliverable reconciliation
    // keeps using the workspace root (operator manifests declare paths relative
    // to it; that path is only exercised when the project is at the root).
    let cmd_root = resolve_project_root(workspace);

    // Assemble the effective layer-2 entries from all three sources.
    // P1-3: polyglot/Tauri detection uses the full workspace root (not only
    // `cmd_root`) so npm+cargo layouts prefer cargo probes over root npm test.
    let toolchain = detect_toolchain_entries(workspace, gate.toolchain_gate);
    let model = collect_model_verify_entries(checklist, gate.auto_verify_replay);
    let effective = merge_verify_entries(&gate.verify, toolchain, model);

    let mut layer2_cache: Option<ManifestGateResult> = None;

    if !effective.is_empty() {
        let Some(exec) = exec else {
            return LhtGateOutcome::Skip("manifest_gate_no_exec");
        };
        session.manifest_gate_rounds = session.manifest_gate_rounds.saturating_add(1);
        let result = run_manifest_gate(&cmd_root, &effective, exec).await;
        let split = classify_failures(&result, &effective, gate);
        record_first_gap(
            session,
            split.enforced_failing.len() + split.observed_failing.len(),
        );
        session.last_manifest_gate = Some(result.clone());
        layer2_cache = Some(result.clone());

        CompletionGateEvent::queue_manifest_result(
            &mut session.pending_gate_events,
            &result,
            &effective,
            session.manifest_gate_rounds,
            split.enforced_failing.len() as u32,
            split.observed_failing.len() as u32,
        );

        // Infra-strike accounting on the **enforced** failing subset only — an
        // observe-only infra failure must never push toward an honest stop.
        let enforced_results: Vec<&VerifyRunResult> = result
            .results
            .iter()
            .filter(|r| split.enforced_failing.contains(&r.id))
            .collect();
        if let Some(outcome) = check_infra_strikes(session, gate, &enforced_results) {
            return outcome;
        }

        if !split.enforced_failing.is_empty() {
            if let Some(outcome) = check_exhausted(
                session,
                gate,
                steps_remaining,
                "manifest_rounds_exhausted",
                &split.enforced_failing,
                &[],
            ) {
                return outcome;
            }
            if steps_remaining == 0 {
                return audit_unmet_outcome(
                    "steps_and_manifest_exhausted",
                    &split.enforced_failing,
                    &[],
                    session,
                );
            }
            return nudge_manifest(
                session,
                &result,
                &split.enforced_failing,
                lang,
                session.tracker.is_blocked(),
            );
        }

        if !split.observed_failing.is_empty() {
            session.completion_gate_observe_pending = true;
        }
    }

    // P1-4 / P1′: cross-layer integration heuristics.
    let cross = integration_gate::evaluate_cross_layer_gaps(workspace);
    if !cross.enforce.is_empty() && gate.mode == CompletionGateMode::Enforce {
        record_first_gap(session, cross.enforce.len());
        session.last_integration_gap_count = Some(cross.enforce.len() as u32);
        session.integration_gate_rounds = session.integration_gate_rounds.saturating_add(1);
        CompletionGateEvent::queue_integration_observe(
            &mut session.pending_gate_events,
            &cross.enforce,
            true,
        );
        if session.integration_gate_rounds <= gate.max_manifest_rounds {
            if steps_remaining == 0 {
                return audit_unmet_outcome("steps_and_integration_exhausted", &[], &[], session);
            }
            let text = super::nudge::build_integration_incomplete_nudge(&cross.enforce, lang);
            return LhtGateOutcome::NudgeIntegrationIncomplete(user_message(text));
        }
    }
    if !cross.observe.is_empty() {
        record_first_gap(session, cross.observe.len());
        session.last_integration_gap_count = Some(
            session
                .last_integration_gap_count
                .unwrap_or(0)
                .max(cross.observe.len() as u32),
        );
        session.completion_gate_observe_pending = true;
        CompletionGateEvent::queue_integration_observe(
            &mut session.pending_gate_events,
            &cross.observe,
            false,
        );
    }

    // Layer 3: operator manifest + workspace overlay + auto IPC discovery (P1c).
    if !runtime_deliverables.is_empty() {
        session.audit_rounds = session.audit_rounds.saturating_add(1);
        let audit = audit_deliverables_async(
            workspace,
            runtime_deliverables,
            true,
            &[],
            session.manifest_gate_rounds.max(1),
            exec,
        )
        .await;
        record_first_gap(
            session,
            audit.failing_gates.len() + audit.missing_deliverables.len(),
        );
        session.last_completion_audit = Some(audit.clone());
        CompletionGateEvent::queue_completion_audit(&mut session.pending_gate_events, &audit);

        if !audit.pass {
            if gate.mode == CompletionGateMode::Enforce {
                let missing_ids: Vec<String> = audit
                    .missing_deliverables
                    .iter()
                    .map(|m| m.id.clone())
                    .collect();
                if let Some(outcome) = check_exhausted(
                    session,
                    gate,
                    steps_remaining,
                    "audit_rounds_exhausted",
                    &audit.failing_gates,
                    &missing_ids,
                ) {
                    return outcome;
                }
                if steps_remaining == 0 {
                    return audit_unmet_outcome(
                        "steps_and_manifest_exhausted",
                        &audit.failing_gates,
                        &missing_ids,
                        session,
                    );
                }
                return nudge_deliverables(session, &audit, lang, session.tracker.is_blocked());
            }
            session.completion_gate_observe_pending = true;
            return LhtGateOutcome::ObserveManifestGate {
                failing_gate_ids: audit.failing_gates.clone(),
                audit: Some(audit),
            };
        }
    }

    if session.completion_gate_observe_pending {
        let failing = session
            .last_manifest_gate
            .as_ref()
            .map(|r| r.failing_ids.clone())
            .unwrap_or_default();
        return LhtGateOutcome::ObserveManifestGate {
            failing_gate_ids: failing,
            audit: session.last_completion_audit.clone(),
        };
    }

    // `layer2_cache` is retained for diagnostics / same-round trust semantics.
    let _ = layer2_cache;
    LhtGateOutcome::Skip("graph_complete")
}

/// Task-agnostic stub / incompleteness gate. Scans the workspace for
/// "intentionally unfinished" markers and, in `enforce`, blocks `graph_complete`
/// (bounded by `max_manifest_rounds`) until the blocking-class stubs are gone.
/// Returns `Some(outcome)` only when it short-circuits (enforce block / honest
/// stop); `None` falls through to the layer-2/3 gates. `observe` always returns
/// `None` after recording telemetry.
async fn evaluate_stub_gate(
    workspace: &Path,
    gate: &CompletionGateConfig,
    session: &mut LongHorizonSessionState,
    lang: &str,
    steps_remaining: u32,
) -> Option<LhtGateOutcome> {
    let enforce = gate.stub_gate.is_enforce();
    let mode = if enforce { "enforce" } else { "observe" };
    let ws = workspace.to_path_buf();
    // Filesystem-only scan — keep it off the async reactor.
    let scan = tokio::task::spawn_blocking(move || stub_gate::scan_workspace_stubs(&ws))
        .await
        .unwrap_or_default();

    session
        .pending_gate_events
        .push(CompletionGateEvent::StubGate {
            payload_json: stub_gate::telemetry_payload(&scan, mode),
        });

    let blocking = scan.blocking();
    if blocking.is_empty() {
        return None;
    }
    record_first_gap(session, blocking.len());

    if !enforce {
        // Observe: record the gap but allow the turn to complete ("先量后调").
        session.completion_gate_observe_pending = true;
        return None;
    }

    session.stub_gate_rounds = session.stub_gate_rounds.saturating_add(1);
    let ids: Vec<String> = blocking
        .iter()
        .take(12)
        .map(|h| format!("{}:{}", h.file, h.line))
        .collect();

    if session.stub_gate_rounds > gate.max_manifest_rounds {
        return Some(audit_unmet_outcome(
            "stub_rounds_exhausted",
            &ids,
            &[],
            session,
        ));
    }
    if steps_remaining == 0 {
        return Some(audit_unmet_outcome(
            "steps_and_stub_exhausted",
            &ids,
            &[],
            session,
        ));
    }
    if session.tracker.is_blocked() {
        session.gate_reinject_while_blocked += 1;
    }
    let text = super::nudge::build_stubs_found_nudge(&blocking, lang);
    Some(LhtGateOutcome::NudgeStubsFound(user_message(text)))
}

/// Honest stop when consecutive enforced layer-2 rounds fail only on infra
/// errors (command not found / timeout / crash) — don't loop forcing the model
/// to "fix" an environment problem (§6.4). Operates on the enforced subset.
fn check_infra_strikes(
    session: &mut LongHorizonSessionState,
    gate: &CompletionGateConfig,
    enforced_failing: &[&VerifyRunResult],
) -> Option<LhtGateOutcome> {
    if enforced_failing.is_empty() {
        session.consecutive_infra_gate_strikes = 0;
        return None;
    }
    let infra_only = enforced_failing.iter().all(|r| {
        matches!(
            r.exit_class,
            VerifyExitClass::Infra | VerifyExitClass::Timeout | VerifyExitClass::Cancelled
        )
    });
    if infra_only {
        session.consecutive_infra_gate_strikes =
            session.consecutive_infra_gate_strikes.saturating_add(1);
        if session.consecutive_infra_gate_strikes >= gate.max_infra_strikes {
            let ids: Vec<String> = enforced_failing.iter().map(|r| r.id.clone()).collect();
            return Some(audit_unmet_outcome("gate_infra_error", &ids, &[], session));
        }
    } else {
        session.consecutive_infra_gate_strikes = 0;
    }
    None
}

fn check_exhausted(
    session: &mut LongHorizonSessionState,
    gate: &CompletionGateConfig,
    steps_remaining: u32,
    round_reason: &'static str,
    failing_gates: &[String],
    missing_ids: &[String],
) -> Option<LhtGateOutcome> {
    let rounds_exhausted = match round_reason {
        "manifest_rounds_exhausted" => session.manifest_gate_rounds > gate.max_manifest_rounds,
        "audit_rounds_exhausted" => session.audit_rounds > gate.max_audit_rounds,
        _ => false,
    };
    if !rounds_exhausted {
        return None;
    }
    let reason = if steps_remaining == 0 {
        "steps_and_manifest_exhausted"
    } else {
        round_reason
    };
    Some(audit_unmet_outcome(
        reason,
        failing_gates,
        missing_ids,
        session,
    ))
}

fn record_first_gap(session: &mut LongHorizonSessionState, gap_count: usize) {
    if session.first_gate_gap_count.is_none() && gap_count > 0 {
        session.first_gate_gap_count = Some(gap_count as u32);
    }
}

fn nudge_manifest(
    session: &mut LongHorizonSessionState,
    result: &ManifestGateResult,
    enforced_failing: &[String],
    lang: &str,
    nudge_blocked: bool,
) -> LhtGateOutcome {
    if nudge_blocked {
        session.gate_reinject_while_blocked += 1;
    }
    let failing: Vec<&VerifyRunResult> = result
        .results
        .iter()
        .filter(|r| enforced_failing.contains(&r.id))
        .collect();
    let text = build_manifest_failed_nudge(&failing, lang);
    LhtGateOutcome::NudgeManifestFailed(user_message(text))
}

fn nudge_deliverables(
    session: &mut LongHorizonSessionState,
    audit: &CompletionAuditResult,
    lang: &str,
    nudge_blocked: bool,
) -> LhtGateOutcome {
    if nudge_blocked {
        session.gate_reinject_while_blocked += 1;
    }
    let text = build_deliverables_failed_nudge(audit, lang);
    LhtGateOutcome::NudgeDeliverablesMissing(user_message(text))
}

fn audit_unmet_outcome(
    reason: &'static str,
    failing_gates: &[String],
    missing_ids: &[String],
    session: &mut LongHorizonSessionState,
) -> LhtGateOutcome {
    LhtGateOutcome::AuditUnmet {
        reason,
        failing_gates: failing_gates.to_vec(),
        missing_deliverable_ids: missing_ids.to_vec(),
        manifest_round: session.manifest_gate_rounds,
        audit_round: session.audit_rounds,
        first_gap_count: session.first_gate_gap_count,
    }
}

fn user_message(text: String) -> Message {
    Message {
        role: "user".to_string(),
        content: vec![ContentBlock::Text {
            text,
            cache_control: None,
        }],
    }
}
