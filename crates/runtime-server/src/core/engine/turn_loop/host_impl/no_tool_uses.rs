//! Steer / sub-agent / inline REPL when the model returns no tool calls (P2 PR6c).

use std::sync::Arc;

use zagens_core::chat::{ContentBlock, Message};
use zagens_core::engine::context::summarize_text;
use zagens_core::engine::turn_loop::TurnLoopOuterHost;
use zagens_core::engine::turn_loop::control::TurnLoopControl;
use zagens_core::engine::turn_machine::emit_kernel_event;
use zagens_core::turn::{TurnContext, TurnOutcomeStatus};

use super::super::Engine;
use crate::core::events::Event;
use crate::long_horizon::harness_verify_loop::{
    harness_verify_status_message, record_to_kernel_event,
};

/// Drain any pending sub-agent completion notifications (non-blocking).
pub(super) fn drain_subagent_completions(
    rx: &Arc<
        tokio::sync::Mutex<
            tokio::sync::mpsc::UnboundedReceiver<crate::tools::subagent::SubAgentCompletion>,
        >,
    >,
    out: &mut Vec<crate::tools::subagent::SubAgentCompletion>,
) {
    if let Ok(mut guard) = rx.try_lock() {
        while let Ok(c) = guard.try_recv() {
            out.push(c);
        }
    }
}

impl Engine {
    async fn maybe_handle_macro_gate_outcome(
        &mut self,
        gate: crate::long_horizon::LhtGateOutcome,
    ) -> bool {
        match gate {
            crate::long_horizon::LhtGateOutcome::MacroCraftSpawn { task_id } => {
                let locale = self.config.locale_tag.clone();
                let prompt = crate::long_horizon::macro_loop::build_craft_review_prompt(&locale);
                match self.spawn_macro_craft_review(&task_id, &prompt).await {
                    Ok(outcome) => {
                        let lh = &mut self.runtime_ext_mut().long_horizon_state;
                        lh.macro_craft_agent_id = Some(outcome.agent_id.clone());
                        lh.macro_phase = zagens_core::long_horizon::MacroPhase::Craft;
                        let cycle = lh.macro_cycles_used;
                        let _ = self
                            .tx_event
                            .send(Event::status(format!(
                                "long_horizon.macro_craft_start: {{\"task_id\":\"{task_id}\",\"agent_id\":\"{}\",\"macro_cycle\":{cycle}}}",
                                outcome.agent_id
                            )))
                            .await;
                        let _ = self
                            .tx_event
                            .send(Event::status(format!(
                                "long_horizon.macro_phase: {{\"phase\":\"craft\",\"macro_cycle\":{cycle}}}"
                            )))
                            .await;
                        self.long_horizon_continue_injected_this_turn = true;
                        true
                    }
                    Err(err) => {
                        let _ = self
                            .tx_event
                            .send(Event::status(format!(
                                "long_horizon.macro_craft_start: {{\"error\":\"{err:?}\"}}"
                            )))
                            .await;
                        false
                    }
                }
            }
            crate::long_horizon::LhtGateOutcome::MacroRemediation(msg) => {
                Engine::add_session_message(self, msg).await;
                self.long_horizon_continue_injected_this_turn = true;
                let phase = self.runtime_ext().long_horizon_state.macro_phase.as_str();
                let _ = self
                    .tx_event
                    .send(Event::status(format!(
                        "long_horizon.macro_phase: {{\"phase\":\"{phase}\"}}"
                    )))
                    .await;
                true
            }
            crate::long_horizon::LhtGateOutcome::MacroUnmet {
                remaining_blockers,
                macro_cycles_used,
            } => {
                let count = remaining_blockers.len();
                let _ = self
                    .tx_event
                    .send(Event::status(format!(
                        "long_horizon.macro_unmet: {{\"remaining_blockers\":{count},\"macro_cycles_used\":{macro_cycles_used}}}"
                    )))
                    .await;
                false
            }
            _ => false,
        }
    }

    async fn maybe_inject_incomplete_audit_continue(&mut self) -> bool {
        if self.scratchpad_audit_continue_injected_this_turn {
            return false;
        }
        let Some(msg) = crate::core::engine::scratchpad_flow::maybe_continue_incomplete_audit(
            &self.session.workspace,
            self.scratchpad_run_id.as_deref(),
            &self.config.scratchpad,
            &self.session.messages,
        ) else {
            return false;
        };
        Engine::add_session_message(self, msg).await;
        self.scratchpad_audit_continue_injected_this_turn = true;
        let _ = self
            .tx_event
            .send(Event::status(
                "Audit scratchpad incomplete — continuing turn (P2 gates unmet)",
            ))
            .await;
        true
    }

    async fn maybe_inject_incomplete_lht_continue(&mut self, turn: &TurnContext) -> bool {
        if self.long_horizon_continue_injected_this_turn {
            return false;
        }

        let lh_config = self.config.long_horizon.clone();
        let scratchpad = self.config.scratchpad.clone();
        let task_type = self.config.task_type;
        let locale = self.config.locale_tag.clone();
        let workspace = self.session.workspace.clone();
        let run_id = self.scratchpad_run_id.clone();
        let messages = self.session.messages.clone();
        let plan_state = self.config_ext().plan_state.clone();
        let todos = self.config_ext().todos.clone();
        let app_mode = self.runtime_ext().turn_app_mode;
        let steps_remaining = turn.steps_remaining();

        self.runtime_ext_mut()
            .long_horizon_state
            .on_assistant_no_tools();
        let blocked_before = self.runtime_ext().long_horizon_state.tracker.is_blocked();
        let converted_before = self.runtime_ext().long_horizon_state.telemetry.converted;

        let macro_cfg = lh_config.macro_loop.clone();
        if macro_cfg.enabled {
            let resume = crate::long_horizon::macro_loop::try_resume_pending_macro_remediation(
                &workspace,
                &mut self.runtime_ext_mut().long_horizon_state,
                &macro_cfg,
                &todos,
                &locale,
            )
            .await;
            if let Some(gate) = resume
                && self.maybe_handle_macro_gate_outcome(gate).await
            {
                return true;
            }
        }

        let shell_manager = std::sync::Arc::clone(&self.runtime_ext().shell_manager);
        let cancel_token = self.0.cancel_token.clone();
        let gate_exec = crate::long_horizon::CompletionGateExec {
            shell_manager: &shell_manager,
            cancel_token: Some(&cancel_token),
        };

        let lht_mode_override = self.runtime_ext().turn_lht_mode;
        let thread_id = self.session.id.clone();
        let lht_client = self.deepseek_client.clone();
        let lht_model = self.session.model.clone();
        let input = crate::long_horizon::LongHorizonContinueInput {
            config: &lh_config,
            lht_mode_override,
            scratchpad: &scratchpad,
            task_type,
            app_mode,
            workspace: &workspace,
            scratchpad_run_id: run_id.as_deref(),
            messages: &messages,
            lang: &locale,
            plan_state: &plan_state,
            todos: &todos,
            session: &mut self.runtime_ext_mut().long_horizon_state,
            thread_id: &thread_id,
            already_injected_this_turn: false,
            steps_remaining,
            gate_exec: Some(gate_exec),
            llm_client: lht_client,
            llm_model: &lht_model,
        };

        let gate = crate::long_horizon::maybe_continue_incomplete_code_task(input).await;

        for event in std::mem::take(
            &mut self
                .runtime_ext_mut()
                .long_horizon_state
                .pending_gate_events,
        ) {
            let _ = self
                .tx_event
                .send(Event::status(event.status_message()))
                .await;
        }

        for record in std::mem::take(
            &mut self
                .runtime_ext_mut()
                .long_horizon_state
                .pending_harness_verify,
        ) {
            let _ = self
                .tx_event
                .send(Event::status(harness_verify_status_message(&record)))
                .await;
            emit_kernel_event(self, record_to_kernel_event(turn.id.clone(), &record));
        }

        // Telemetry (§4.9): emit a `nudge_outcome` whenever a prior nudge just
        // converted into qualified progress — the evidence we want for tuning.
        let converted_now = self.runtime_ext().long_horizon_state.telemetry.converted;
        if converted_now > converted_before {
            let _ = self
                .tx_event
                .send(Event::status(format!(
                    "long_horizon.nudge_outcome: {{\"converted\":{converted_now}}}"
                )))
                .await;
        }

        let msg = match gate {
            crate::long_horizon::LhtGateOutcome::Nudge(msg) => msg,
            crate::long_horizon::LhtGateOutcome::NudgeAdversarialGaps(msg) => {
                // §6.7 enforce: gap candidates reinjected as checklist items.
                Engine::add_session_message(self, msg).await;
                self.long_horizon_continue_injected_this_turn = true;
                let audit_round = self
                    .runtime_ext()
                    .long_horizon_state
                    .adversarial_audit_rounds;
                let _ = self
                    .tx_event
                    .send(Event::status(format!(
                        "long_horizon.adversarial_audit: {{\"enforce\":true,\"reinject\":true,\"audit_round\":{audit_round}}}"
                    )))
                    .await;
                return true;
            }
            crate::long_horizon::LhtGateOutcome::NudgeManifestFailed(msg)
            | crate::long_horizon::LhtGateOutcome::NudgeDeliverablesMissing(msg) => {
                Engine::add_session_message(self, msg).await;
                self.long_horizon_continue_injected_this_turn = true;
                let rounds = self.runtime_ext().long_horizon_state.manifest_gate_rounds;
                let audit_rounds = self.runtime_ext().long_horizon_state.audit_rounds;
                let blocked = self
                    .runtime_ext()
                    .long_horizon_state
                    .gate_reinject_while_blocked;
                let _ = self
                    .tx_event
                    .send(Event::status(format!(
                        "long_horizon.manifest_gate: {{\"enforce\":true,\"reinject\":true,\"manifest_round\":{rounds},\"audit_round\":{audit_rounds},\"gate_reinject_while_blocked\":{blocked}}}"
                    )))
                    .await;
                return true;
            }
            crate::long_horizon::LhtGateOutcome::NudgeStubsFound(msg) => {
                // Generic stub gate fired in enforce: the graph is "complete" and
                // the code compiles, but blocking-class stub markers remain. Inject
                // the focused nudge and emit a *distinct* node so the LHT panel /
                // sidecar.log show the "compiles but feature is a stub" block
                // separately from verify-command failures.
                Engine::add_session_message(self, msg).await;
                self.long_horizon_continue_injected_this_turn = true;
                let rounds = self.runtime_ext().long_horizon_state.stub_gate_rounds;
                let blocked = self
                    .runtime_ext()
                    .long_horizon_state
                    .gate_reinject_while_blocked;
                let _ = self
                    .tx_event
                    .send(Event::status(format!(
                        "long_horizon.stub_gate: {{\"enforce\":true,\"reinject\":true,\"stub_round\":{rounds},\"gate_reinject_while_blocked\":{blocked}}}"
                    )))
                    .await;
                return true;
            }
            crate::long_horizon::LhtGateOutcome::NudgePlanRequired(msg) => {
                // Strict-mode plan-bootstrap: the model was working with an empty
                // task graph (no plan/checklist). Inject the "establish a plan"
                // nudge to keep the turn alive until a plan exists. The
                // `long_horizon.plan_gate` telemetry node was already queued via
                // pending_gate_events and emitted above.
                Engine::add_session_message(self, msg).await;
                self.long_horizon_continue_injected_this_turn = true;
                let rounds = self.runtime_ext().long_horizon_state.plan_gate_rounds;
                let _ = self
                    .tx_event
                    .send(Event::status(format!(
                        "long_horizon.plan_gate: {{\"enforce\":true,\"reinject\":true,\"plan_round\":{rounds}}}"
                    )))
                    .await;
                return true;
            }
            crate::long_horizon::LhtGateOutcome::ObserveManifestGate {
                failing_gate_ids,
                audit,
            } => {
                let missing = audit
                    .as_ref()
                    .map(|a| a.missing_deliverables.len())
                    .unwrap_or(0);
                let first_gap = self
                    .runtime_ext()
                    .long_horizon_state
                    .first_gate_gap_count
                    .unwrap_or(0);
                let _ = self
                    .tx_event
                    .send(Event::status(format!(
                        "long_horizon.manifest_gate: {{\"observe\":true,\"failing_gates\":{},\"missing_deliverables\":{missing},\"first_gap_count\":{first_gap}}}",
                        failing_gate_ids.len()
                    )))
                    .await;
                return false;
            }
            crate::long_horizon::LhtGateOutcome::AuditUnmet {
                reason,
                failing_gates,
                missing_deliverable_ids,
                manifest_round,
                audit_round,
                first_gap_count,
            } => {
                let first = first_gap_count.unwrap_or(0);
                let _ = self
                    .tx_event
                    .send(Event::status(format!(
                        "long_horizon.audit_unmet: {{\"reason\":\"{reason}\",\"failing_gates\":{},\"missing_deliverables\":{},\"manifest_round\":{manifest_round},\"audit_round\":{audit_round},\"first_gap_count\":{first}}}",
                        failing_gates.len(),
                        missing_deliverable_ids.len()
                    )))
                    .await;
                return false;
            }
            crate::long_horizon::LhtGateOutcome::NudgeUnverifiedAcceptance(msg) => {
                // DEMO3 false-green guard fired: the graph is "complete" but a
                // completed item is an unverified runnable acceptance. Inject the
                // focused nudge and emit a *distinct* node (not the generic
                // continue_injected) so the LHT panel / sidecar.log show the
                // guard explicitly and the normal continue/conversion telemetry
                // is not muddied.
                Engine::add_session_message(self, msg).await;
                self.long_horizon_continue_injected_this_turn = true;
                let count = self
                    .runtime_ext()
                    .long_horizon_state
                    .unverified_acceptance_nudges;
                let _ = self
                    .tx_event
                    .send(Event::status(format!(
                        "long_horizon.unverified_acceptance_nudge: {{\"count\":{count}}}"
                    )))
                    .await;
                return true;
            }
            crate::long_horizon::LhtGateOutcome::NudgeVerifyMismatch(msg) => {
                // P0-2: completed item has `[verify:]` but no matching recent
                // exec — tagged without running. Same injection pattern as
                // unverified_acceptance but distinct telemetry.
                Engine::add_session_message(self, msg).await;
                self.long_horizon_continue_injected_this_turn = true;
                let count = self.runtime_ext().long_horizon_state.verify_mismatch_nudges;
                let _ = self
                    .tx_event
                    .send(Event::status(format!(
                        "long_horizon.verify_mismatch_nudge: {{\"count\":{count}}}"
                    )))
                    .await;
                return true;
            }
            crate::long_horizon::LhtGateOutcome::NudgeInsufficientVerify(msg) => {
                Engine::add_session_message(self, msg).await;
                self.long_horizon_continue_injected_this_turn = true;
                let count = self
                    .runtime_ext()
                    .long_horizon_state
                    .insufficient_verify_nudges;
                let _ = self
                    .tx_event
                    .send(Event::status(format!(
                        "long_horizon.insufficient_verify_nudge: {{\"count\":{count}}}"
                    )))
                    .await;
                return true;
            }
            crate::long_horizon::LhtGateOutcome::NudgePlanChecklistDrift(msg) => {
                Engine::add_session_message(self, msg).await;
                self.long_horizon_continue_injected_this_turn = true;
                let count = self
                    .runtime_ext()
                    .long_horizon_state
                    .plan_checklist_drift_nudges;
                let _ = self
                    .tx_event
                    .send(Event::status(format!(
                        "long_horizon.plan_checklist_drift_nudge: {{\"count\":{count}}}"
                    )))
                    .await;
                return true;
            }
            crate::long_horizon::LhtGateOutcome::NudgeIntegrationIncomplete(msg) => {
                Engine::add_session_message(self, msg).await;
                self.long_horizon_continue_injected_this_turn = true;
                let rounds = self
                    .runtime_ext()
                    .long_horizon_state
                    .integration_gate_rounds;
                let blocked = self
                    .runtime_ext()
                    .long_horizon_state
                    .gate_reinject_while_blocked;
                let _ = self
                    .tx_event
                    .send(Event::status(format!(
                        "long_horizon.integration_gate: {{\"enforce\":true,\"reinject\":true,\"round\":{rounds},\"gate_reinject_while_blocked\":{blocked}}}"
                    )))
                    .await;
                return true;
            }
            gate @ (crate::long_horizon::LhtGateOutcome::MacroCraftSpawn { .. }
            | crate::long_horizon::LhtGateOutcome::MacroRemediation(_)
            | crate::long_horizon::LhtGateOutcome::MacroUnmet { .. }) => {
                return self.maybe_handle_macro_gate_outcome(gate).await;
            }
            crate::long_horizon::LhtGateOutcome::Skip(reason) => {
                // §4.9 observability: emit exactly which guard suppressed the nudge,
                // alongside the engine-side state, so "it didn't fire" becomes
                // "it skipped at <reason> with <facts>" in a single run.
                let plan = plan_state.lock().await.snapshot();
                let todo = todos.lock().await.snapshot();
                let graph = crate::long_horizon::CodeTaskGraph::from_snapshots(&plan, &todo);
                let in_progress = graph
                    .in_progress_id
                    .map_or_else(|| "null".to_string(), |id| id.to_string());
                let _ = self
                    .tx_event
                    .send(Event::status(format!(
                        "long_horizon.gate_skip: {{\"reason\":\"{reason}\",\"enabled\":{},\"app_mode\":\"{:?}\",\"code_surface\":{},\"empty\":{},\"incomplete\":{},\"trivial\":{},\"in_progress_id\":{in_progress},\"open_items\":{}}}",
                        lh_config.enabled,
                        app_mode,
                        task_type.uses_code_tool_surface(),
                        graph.is_empty(),
                        graph.incomplete(),
                        graph.is_trivial(),
                        graph.open_items,
                    )))
                    .await;

                if reason == "macro_await_confirm" {
                    let micro_passed = !self
                        .runtime_ext()
                        .long_horizon_state
                        .macro_after_audit_unmet;
                    let hint = crate::long_horizon::macro_loop::build_confirm_prompt(
                        &self.config.locale_tag,
                        micro_passed,
                    );
                    let _ = self
                        .tx_event
                        .send(Event::status(format!(
                            "long_horizon.macro_phase: {{\"phase\":\"implement\",\"awaiting_confirm\":true,\"hint\":{}}}",
                            serde_json::to_string(&hint).unwrap_or_else(|_| "\"\"".into())
                        )))
                        .await;
                }

                let blocked_now = self.runtime_ext().long_horizon_state.tracker.is_blocked();
                if blocked_before || blocked_now {
                    let _ = self
                        .tx_event
                        .send(Event::status(format!(
                            "long_horizon.blocked: {{\"open_items\":{},\"reason\":\"max_nudges_without_progress\"}}",
                            graph.open_items
                        )))
                        .await;
                }
                return false;
            }
        };

        Engine::add_session_message(self, msg).await;
        self.long_horizon_continue_injected_this_turn = true;

        let plan = plan_state.lock().await.snapshot();
        let todo = todos.lock().await.snapshot();
        let open = crate::long_horizon::CodeTaskGraph::from_snapshots(&plan, &todo).open_items;
        let (nudge_count, emitted, converted) = {
            let lh = &self.runtime_ext().long_horizon_state;
            (
                lh.tracker.max_item_nudge_count(),
                lh.telemetry.emitted,
                lh.telemetry.converted,
            )
        };
        let _ = self
            .tx_event
            .send(Event::status(format!(
                "long_horizon.continue_injected: {{\"open_items\":{open},\"nudge_count\":{nudge_count},\"emitted\":{emitted},\"converted\":{converted}}}"
            )))
            .await;
        true
    }

    /// "一推到底" auto-continue override (C2). Called only after the routine LHT
    /// continue gate ([`Self::maybe_inject_incomplete_lht_continue`]) has already
    /// declined — i.e. the nudge tracker has given up (`blocked` / max nudges) or
    /// skipped — yet the task graph is still genuinely incomplete. When
    /// `[long_horizon] auto_continue = true`, this resets the tracker give-up
    /// state and re-injects a forceful continue message so the turn keeps moving
    /// to the next phase instead of ending as a false green. Bounded per turn by
    /// `max_auto_continue_rounds`, so a model that truly cannot progress still
    /// terminates the turn. Returns `true` if the turn should keep going.
    async fn maybe_auto_continue_incomplete_lht(&mut self) -> bool {
        let cfg = self.config.long_horizon.clone();
        if !cfg.enabled || !cfg.auto_continue {
            return false;
        }
        // Code-surface agent tasks only; never override a user "stop" steer.
        let app_mode = self.runtime_ext().turn_app_mode;
        if !matches!(
            app_mode,
            crate::agent_surface::AppMode::Agent | crate::agent_surface::AppMode::Yolo
        ) {
            return false;
        }
        if !self.config.task_type.uses_code_tool_surface() {
            return false;
        }
        if self.runtime_ext().long_horizon_state.paused {
            return false;
        }

        // Only override for a genuinely incomplete, non-trivial task graph.
        let plan_state = self.config_ext().plan_state.clone();
        let todos = self.config_ext().todos.clone();
        let plan = plan_state.lock().await.snapshot();
        let todo = todos.lock().await.snapshot();
        let graph = crate::long_horizon::CodeTaskGraph::from_snapshots(&plan, &todo);
        if graph.is_empty() || graph.is_trivial() || !graph.incomplete() {
            return false;
        }

        // Hard per-turn ceiling: bound the give-up override.
        if self.long_horizon_auto_continue_rounds >= cfg.max_auto_continue_rounds {
            let _ = self
                .tx_event
                .send(Event::status(format!(
                    "long_horizon.auto_continue_exhausted: {{\"max\":{},\"open_items\":{}}}",
                    cfg.max_auto_continue_rounds, graph.open_items
                )))
                .await;
            return false;
        }

        // Fresh nudge budget for the next phase + re-arm the once-per-turn gate.
        self.runtime_ext_mut()
            .long_horizon_state
            .tracker
            .clear_blocked();
        self.long_horizon_continue_injected_this_turn = false;
        self.long_horizon_auto_continue_rounds =
            self.long_horizon_auto_continue_rounds.saturating_add(1);
        let round = self.long_horizon_auto_continue_rounds;

        let locale = self.config.locale_tag.clone();
        let text = crate::long_horizon::build_auto_continue_message(&graph, round, &locale);
        Engine::add_session_message(
            self,
            Message {
                role: "user".to_string(),
                content: vec![ContentBlock::Text {
                    text,
                    cache_control: None,
                }],
            },
        )
        .await;
        let _ = self
            .tx_event
            .send(Event::status(format!(
                "long_horizon.auto_continue: {{\"round\":{round},\"max\":{},\"open_items\":{}}}",
                cfg.max_auto_continue_rounds, graph.open_items
            )))
            .await;
        true
    }

    pub(super) async fn handle_no_tool_uses_turn_loop(
        &mut self,
        turn: &mut TurnContext,
        pending_steers: &mut Vec<String>,
        current_text_visible: &str,
        has_sendable_assistant_content: bool,
    ) -> TurnLoopControl {
        if self.maybe_inject_scratchpad_summary(turn).await && !pending_steers.is_empty() {
            for steer in pending_steers.drain(..) {
                let workspace = self.0.session.workspace.clone();
                self.0
                    .session
                    .working_set
                    .observe_user_message(&steer, &workspace);
                Engine::add_session_message(
                    self,
                    Message {
                        role: "user".to_string(),
                        content: vec![ContentBlock::Text {
                            text: steer,
                            cache_control: None,
                        }],
                    },
                )
                .await;
            }
            turn.next_step();
            return TurnLoopControl::Continue;
        }

        if !pending_steers.is_empty() {
            for steer in pending_steers.drain(..) {
                let workspace = self.0.session.workspace.clone();
                self.0
                    .session
                    .working_set
                    .observe_user_message(&steer, &workspace);
                Engine::add_session_message(
                    self,
                    Message {
                        role: "user".to_string(),
                        content: vec![ContentBlock::Text {
                            text: steer,
                            cache_control: None,
                        }],
                    },
                )
                .await;
            }
            turn.next_step();
            return TurnLoopControl::Continue;
        }

        let mut completions: Vec<crate::tools::subagent::SubAgentCompletion> = Vec::new();
        {
            let rx = Arc::clone(&self.runtime_ext().rx_subagent_completion);
            drain_subagent_completions(&rx, &mut completions);
        }
        if completions.is_empty() {
            // M3: route through `SubAgentHost::running_count` so the future
            // core-side Engine can swap `subagent_manager` to a trait object.
            let running = {
                let manager = Arc::clone(&self.runtime_ext().subagent_manager);
                manager.write().await.running_count()
            };
            if running > 0 {
                let rx = Arc::clone(&self.runtime_ext().rx_subagent_completion);
                let _ = self
                    .tx_event
                    .send(Event::status(format!(
                        "Waiting on {running} sub-agent(s) to complete..."
                    )))
                    .await;
                let cancel = self.0.cancel_token.clone();
                tokio::select! {
                    biased;
                    () = cancel.cancelled() => {
                        let _ = self
                            .tx_event
                            .send(Event::status(
                                "Request cancelled while waiting for sub-agents",
                            ))
                            .await;
                        return TurnLoopControl::Return(TurnOutcomeStatus::Interrupted, None);
                    }
                    Some(c) = {
                        let rx_wait = Arc::clone(&rx);
                        async move { rx_wait.lock().await.recv().await }
                    } => {
                        completions.push(c);
                        if let Ok(mut guard) = rx.try_lock() {
                            while let Ok(extra) = guard.try_recv() {
                                completions.push(extra);
                            }
                        }
                    }
                    Some(steer) = self.0.rx_steer.recv() => {
                        let trimmed = steer.trim().to_string();
                        if !trimmed.is_empty() {
                            self.runtime_ext_mut()
                                .long_horizon_state
                                .on_steer(&trimmed);
                            let workspace = self.0.session.workspace.clone();
                            self.0
                                .session
                                .working_set
                                .observe_user_message(&trimmed, &workspace);
                            Engine::add_session_message(
                                self,
                                Message {
                                    role: "user".to_string(),
                                    content: vec![ContentBlock::Text {
                                        text: trimmed.clone(),
                                        cache_control: None,
                                    }],
                                },
                            )
                            .await;
                            let _ = self.tx_event.send(Event::status(format!(
                                "Steer input accepted: {}",
                                summarize_text(&trimmed, 120)
                            ))).await;
                        }
                        turn.next_step();
                        return TurnLoopControl::Continue;
                    }
                }
            } else {
                // P1-8: completion may land after the first drain but before
                // running_count() observed zero — try once more.
                let rx = Arc::clone(&self.runtime_ext().rx_subagent_completion);
                drain_subagent_completions(&rx, &mut completions);
            }
        }
        if !completions.is_empty() {
            let count = completions.len();
            let macro_craft_id = self
                .runtime_ext()
                .long_horizon_state
                .macro_craft_agent_id
                .clone();
            let macro_task_id = self.runtime_ext().long_horizon_state.macro_task_id.clone();
            for c in completions {
                if macro_craft_id.as_deref() == Some(c.agent_id.as_str())
                    && let Some(task_id) = macro_task_id.as_deref()
                {
                    let workspace = self.session.workspace.clone();
                    let todos = self.config_ext().todos.clone();
                    let locale = self.config.locale_tag.clone();
                    let macro_cfg = self.config.long_horizon.macro_loop.clone();
                    if let Some(outcome) =
                        crate::long_horizon::macro_loop::on_craft_review_complete(
                            &workspace,
                            task_id,
                            &mut self.runtime_ext_mut().long_horizon_state,
                            &macro_cfg,
                            &todos,
                            &locale,
                        )
                        .await
                    {
                        let blockers = match &outcome {
                            crate::long_horizon::LhtGateOutcome::MacroUnmet {
                                remaining_blockers,
                                ..
                            } => remaining_blockers.len(),
                            _ => 0,
                        };
                        let _ = self
                                .tx_event
                                .send(Event::status(format!(
                                    "long_horizon.macro_craft_result: {{\"task_id\":\"{task_id}\",\"blockers_count\":{blockers}}}"
                                )))
                                .await;
                        if self.maybe_handle_macro_gate_outcome(outcome).await {
                            turn.next_step();
                            return TurnLoopControl::Continue;
                        }
                    } else {
                        let _ = self
                                .tx_event
                                .send(Event::status(format!(
                                    "long_horizon.macro_craft_result: {{\"task_id\":\"{task_id}\",\"blockers_count\":0}}"
                                )))
                                .await;
                    }
                }
                let workspace = self.0.session.workspace.clone();
                self.0
                    .session
                    .working_set
                    .observe_user_message(&c.payload, &workspace);
                Engine::add_session_message(
                    self,
                    Message {
                        role: "user".to_string(),
                        content: vec![ContentBlock::Text {
                            text: c.payload,
                            cache_control: None,
                        }],
                    },
                )
                .await;
            }
            let _ = self
                .tx_event
                .send(Event::status(format!(
                    "Resuming turn with {count} sub-agent completion(s)"
                )))
                .await;
            turn.next_step();
            return TurnLoopControl::Continue;
        }

        if has_sendable_assistant_content
            && crate::repl::sandbox::has_repl_block(current_text_visible)
        {
            let repl_blocks = crate::repl::sandbox::extract_repl_blocks(current_text_visible);
            let mut runtime = match crate::repl::runtime::PythonRuntime::new().await {
                Ok(rt) => rt,
                Err(e) => {
                    let _ = self
                        .tx_event
                        .send(Event::status(format!("REPL init failed: {e}")))
                        .await;
                    return TurnLoopControl::Break;
                }
            };

            let mut final_result: Option<String> = None;
            for (i, block) in repl_blocks.iter().enumerate() {
                let round_num = i + 1;
                let _ = self
                    .tx_event
                    .send(Event::status(format!(
                        "REPL round {round_num}: executing..."
                    )))
                    .await;

                match runtime.execute(&block.code).await {
                    Ok(round) => {
                        if let Some(val) = &round.final_value {
                            let _ = self
                                .tx_event
                                .send(Event::status(format!(
                                    "REPL round {round_num}: FINAL result obtained"
                                )))
                                .await;
                            final_result = Some(val.clone());
                            break;
                        }

                        let feedback = if round.has_error {
                            format!(
                                "[REPL round {round_num} error]\nstdout:\n{}\nstderr:\n{}",
                                round.stdout, round.stderr
                            )
                        } else {
                            format!("[REPL round {round_num} output]\n{}", round.stdout)
                        };
                        Engine::add_session_message(
                            self,
                            Message {
                                role: "user".to_string(),
                                content: vec![ContentBlock::Text {
                                    text: feedback,
                                    cache_control: None,
                                }],
                            },
                        )
                        .await;
                    }
                    Err(e) => {
                        let _ = self
                            .tx_event
                            .send(Event::status(format!("REPL round {round_num} failed: {e}")))
                            .await;
                        Engine::add_session_message(
                            self,
                            Message {
                                role: "user".to_string(),
                                content: vec![ContentBlock::Text {
                                    text: format!("[REPL round {round_num} execution failed]\n{e}"),
                                    cache_control: None,
                                }],
                            },
                        )
                        .await;
                    }
                }
            }

            if let Some(final_val) = final_result {
                if let Some(last_msg) = self.session.messages.last_mut()
                    && last_msg.role == "assistant"
                {
                    for block in &mut last_msg.content {
                        if let ContentBlock::Text { text, .. } = block {
                            *text = final_val;
                            break;
                        }
                    }
                }
                Engine::emit_session_updated(self).await;
                return TurnLoopControl::Break;
            }

            turn.next_step();
            return TurnLoopControl::Continue;
        }

        if self.maybe_inject_incomplete_audit_continue().await {
            turn.next_step();
            return TurnLoopControl::Continue;
        }

        if self.maybe_inject_incomplete_lht_continue(turn).await {
            // LHT harness nudge: Continue without bumping step (§4.6).
            return TurnLoopControl::Continue;
        }

        // C2 ("一推到底"): the routine nudge gate gave up but the task is still
        // incomplete — when auto_continue is enabled, override the give-up and
        // keep the turn alive (bounded by `max_auto_continue_rounds`).
        if self.maybe_auto_continue_incomplete_lht().await {
            return TurnLoopControl::Continue;
        }

        if self
            .maybe_await_macro_craft_completion(turn)
            .await
            .is_some()
        {
            return TurnLoopControl::Continue;
        }

        TurnLoopControl::Break
    }

    /// Hold the turn open while LHT CRAFT review runs, then inject remediation.
    async fn maybe_await_macro_craft_completion(&mut self, turn: &mut TurnContext) -> Option<()> {
        self.runtime_ext()
            .long_horizon_state
            .macro_craft_agent_id
            .as_ref()?;

        let rx = Arc::clone(&self.runtime_ext().rx_subagent_completion);
        let manager = Arc::clone(&self.runtime_ext().subagent_manager);
        let running = manager.write().await.running_count();
        if running == 0 {
            let workspace = self.session.workspace.clone();
            let todos = self.config_ext().todos.clone();
            let locale = self.config.locale_tag.clone();
            let macro_cfg = self.config.long_horizon.macro_loop.clone();
            if let Some(outcome) =
                crate::long_horizon::macro_loop::try_resume_pending_macro_remediation(
                    &workspace,
                    &mut self.runtime_ext_mut().long_horizon_state,
                    &macro_cfg,
                    &todos,
                    &locale,
                )
                .await
                && self.maybe_handle_macro_gate_outcome(outcome).await
            {
                turn.next_step();
                return Some(());
            }
            return None;
        }

        let _ = self
            .tx_event
            .send(Event::status(format!(
                "Waiting on CRAFT review sub-agent ({running} sub-agent(s) running)..."
            )))
            .await;

        let cancel = self.0.cancel_token.clone();
        let mut completions: Vec<crate::tools::subagent::SubAgentCompletion> = Vec::new();
        tokio::select! {
            biased;
            () = cancel.cancelled() => {
                let _ = self
                    .tx_event
                    .send(Event::status(
                        "Request cancelled while waiting for CRAFT review",
                    ))
                    .await;
                return None;
            }
            Some(c) = {
                let rx_wait = Arc::clone(&rx);
                async move { rx_wait.lock().await.recv().await }
            } => {
                completions.push(c);
                if let Ok(mut guard) = rx.try_lock() {
                    while let Ok(extra) = guard.try_recv() {
                        completions.push(extra);
                    }
                }
            }
        }

        let macro_craft_id = self
            .runtime_ext()
            .long_horizon_state
            .macro_craft_agent_id
            .clone();
        let macro_task_id = self.runtime_ext().long_horizon_state.macro_task_id.clone();
        for c in completions {
            if macro_craft_id.as_deref() == Some(c.agent_id.as_str())
                && let Some(task_id) = macro_task_id.as_deref()
            {
                let workspace = self.session.workspace.clone();
                let todos = self.config_ext().todos.clone();
                let locale = self.config.locale_tag.clone();
                let macro_cfg = self.config.long_horizon.macro_loop.clone();
                if let Some(outcome) = crate::long_horizon::macro_loop::on_craft_review_complete(
                    &workspace,
                    task_id,
                    &mut self.runtime_ext_mut().long_horizon_state,
                    &macro_cfg,
                    &todos,
                    &locale,
                )
                .await
                {
                    let blockers = match &outcome {
                        crate::long_horizon::LhtGateOutcome::MacroUnmet {
                            remaining_blockers,
                            ..
                        } => remaining_blockers.len(),
                        _ => 0,
                    };
                    let _ = self
                            .tx_event
                            .send(Event::status(format!(
                                "long_horizon.macro_craft_result: {{\"task_id\":\"{task_id}\",\"blockers_count\":{blockers}}}"
                            )))
                            .await;
                    if self.maybe_handle_macro_gate_outcome(outcome).await {
                        turn.next_step();
                        return Some(());
                    }
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod drain_tests {
    use std::sync::Arc;

    use tokio::sync::{Mutex, mpsc};

    use super::drain_subagent_completions;
    use crate::tools::subagent::SubAgentCompletion;

    #[test]
    fn second_drain_captures_completion_after_empty_first_drain() {
        let (tx, rx) = mpsc::unbounded_channel();
        let rx = Arc::new(Mutex::new(rx));
        let mut first = Vec::new();
        drain_subagent_completions(&rx, &mut first);
        assert!(first.is_empty());

        tx.send(SubAgentCompletion {
            agent_id: "agent_test".into(),
            payload: "done".into(),
        })
        .expect("send completion");

        let mut second = Vec::new();
        drain_subagent_completions(&rx, &mut second);
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].agent_id, "agent_test");
    }
}
