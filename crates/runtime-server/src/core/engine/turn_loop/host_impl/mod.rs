//! `V3TurnHost` / inner-step implementation for the TUI `Engine` (P2 PR4 step 2).

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::{Mutex as AsyncMutex, RwLock, mpsc};
use zagens_core::chat::{ContentBlock, LlmClient, Message, Tool};
use zagens_core::context_profile::auto_compaction_allowed;
use zagens_core::engine::KernelTurnHost;
use zagens_core::engine::context::estimate_input_tokens_conservative;
use zagens_core::engine::hosts::McpHost;
use zagens_core::engine::kernel_event::{KernelEvent, MessageRange};
use zagens_core::engine::streaming::ToolUseState;
use zagens_core::engine::turn_loop::TurnLoopToolRegistry;
use zagens_core::engine::turn_loop::control::TurnLoopControl;
use zagens_core::engine::turn_loop::exec::{
    ToolExecOutcome, ToolExecutionPlan, ToolPlanApprovalMeta,
};
use zagens_core::engine::turn_loop::{InnerStepHost, TurnLoopOuterHost, TurnLoopSessionHost};
use zagens_core::engine::turn_machine::{
    Effect, KernelEventSink, LiveTurnSnapshot, emit_kernel_event,
};
use zagens_core::turn::{TurnContext, TurnLoopMode};
use zagens_tools::{ToolError, ToolResult};

use crate::core::engine::effect_interpreter::EffectInterpreter;
use crate::core::engine::kernel_outer_boundary::{
    V3OuterBoundaryTurnGrants, verify_turn_outer_boundary_grants,
};

use super::super::scratchpad_flow;
use super::super::tool_catalog::execute_code_execution_tool;
use super::Engine;
use crate::agent_surface::AppMode;
use crate::compaction::{compact_messages_safe, should_compact};
use crate::core::events::Event;
use crate::core::turn::pre_tool_snapshot;
use crate::mcp::McpPool;
use crate::tools::ToolRegistry;
use zagens_core::engine::tool_catalog::{
    active_tools_for_step, ensure_advanced_tooling, execute_tool_search, initial_active_tools,
    maybe_activate_requested_deferred_tool,
};
impl TurnLoopToolRegistry for ToolRegistry {}

mod capacity;
mod no_tool_uses;

/// Reconstruct a minimal evidence envelope from ledger / cite lines in tool output.
fn parse_citations_from_tool_result(content: &str) -> Option<zagens_tools::EvidenceEnvelope> {
    use zagens_tools::{EvidenceCitation, EvidenceEnvelope};
    let mut env = EvidenceEnvelope::new();
    let mut found = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(fact) = trimmed.strip_prefix("- fact: ")
            && let Some((k, v)) = fact.split_once('=')
        {
            env = env.with_fact(k.trim(), v.trim());
            found = true;
            continue;
        }
        let Some(rest) = trimmed
            .strip_prefix("- cite:")
            .or_else(|| trimmed.strip_prefix("cite:"))
            .map(str::trim)
        else {
            continue;
        };
        // `path`, `path:start`, or `path:start-end` (rsplit so `F:` drives survive)
        let (path, start, end) = if let Some((path, lines)) = rest.rsplit_once(':') {
            let line_ok = lines
                .split('-')
                .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()));
            if line_ok {
                if let Some((s, e)) = lines.split_once('-') {
                    (path, s.parse::<u64>().ok(), e.parse::<u64>().ok())
                } else if let Ok(n) = lines.parse::<u64>() {
                    (path, Some(n), Some(n))
                } else {
                    (rest, None, None)
                }
            } else {
                (rest, None, None)
            }
        } else {
            (rest, None, None)
        };
        let path = zagens_core::engine::normalize_repo_path(path);
        let cite = match (start, end) {
            (Some(s), Some(e)) => EvidenceCitation::lines(&path, s, e),
            (Some(s), None) => EvidenceCitation::lines(&path, s, s),
            _ => EvidenceCitation::path(&path),
        };
        env = env.with_citation(cite);
        found = true;
    }
    found.then_some(env)
}

#[async_trait]
impl KernelTurnHost for Engine {
    type V3ToolRegistry = ToolRegistry;

    fn kernel_machine_mode(&self) -> zagens_core::engine::KernelMachineMode {
        self.runtime_ext().kernel_machine_mode
    }

    fn kernel_event_sink(&self) -> Option<&KernelEventSink> {
        self.runtime_ext()
            .kernel_event_writer
            .as_ref()
            .map(|writer| writer.tx())
    }

    fn record_kernel_event(&mut self, event: &KernelEvent) {
        self.runtime_ext_mut()
            .kernel_turn_events
            .record(event.clone());
    }

    fn record_v3_outer_boundary_grant(
        &mut self,
        kind: zagens_core::engine::turn_loop::continuation_boundary_policy::OuterBoundaryKind,
    ) {
        if self.runtime_ext().kernel_machine_mode.uses_v3_turn_loop() {
            self.runtime_ext_mut()
                .kernel_v3_outer_boundary_grants
                .record(kind);
        }
    }

    fn reset_kernel_turn_events(&mut self) {
        self.runtime_ext_mut().kernel_turn_events.reset_turn();
        self.runtime_ext_mut().kernel_v3_outer_boundary_grants =
            V3OuterBoundaryTurnGrants::default();
        self.runtime_ext_mut().kernel_active_cycle_boundary = None;
    }

    fn kernel_turn_events(&self) -> Vec<KernelEvent> {
        self.runtime_ext().kernel_turn_events.turn_events().to_vec()
    }

    fn sync_kernel_turn_frame(&mut self, turn: &TurnContext) {
        let ext = self.runtime_ext_mut();
        ext.kernel_active_turn_id = Some(turn.id.clone());
        ext.kernel_active_step = turn.step;
    }

    fn apply_kernel_resume_hints(&mut self, hints: &zagens_core::engine::KernelResumeHints) {
        let ext = self.runtime_ext_mut();
        if let Some(ref turn_id) = hints.latest_turn_id {
            ext.kernel_active_turn_id = Some(turn_id.clone());
        }
        ext.kernel_active_step = hints.step_idx;
        tracing::info!(
            target: "kernel_resume",
            latest_turn_id = ?hints.latest_turn_id,
            step_idx = hints.step_idx,
            max_steps = hints.max_steps,
            scratchpad_summary_injected = hints.scratchpad_summary_injected,
            active_tool_count = hints.active_tool_count,
            kernel_model_message_count = hints.kernel_model_message_count,
            kernel_model_request_count = hints.kernel_model_request_count,
            kernel_estimated_min_session_messages = hints.kernel_estimated_min_session_messages,
            kernel_transcript_preview_row_count = hints.kernel_transcript_preview_row_count,
            kernel_transcript_preview_body_count = hints.kernel_transcript_preview_body_count,
            "restored kernel turn frame from event log"
        );
    }

    async fn finish_kernel_turn(&mut self, live: &LiveTurnSnapshot) {
        let (events, writer, do_replay_verify) = {
            let ext = self.runtime_ext_mut();
            let do_replay_verify = ext.kernel_machine_mode.uses_replay_verification();
            let events = ext.kernel_turn_events.turn_events().to_vec();
            let writer = ext.kernel_event_writer.clone();
            let outer_boundary_grants = ext.kernel_v3_outer_boundary_grants;
            if do_replay_verify {
                ext.kernel_turn_replay.verify_turn_in_memory(&events, live);
            }
            let counts = zagens_core::engine::replay_effect_counts(&events);
            verify_turn_outer_boundary_grants(&events, &outer_boundary_grants);
            tracing::info!(
                target: "kernel_v3",
                turn_id = %live.turn_id,
                call_model = counts.call_model,
                execute_batch = counts.execute_batch,
                inject_steer = counts.inject_steer,
                run_compaction = counts.run_compaction,
                notify_lsp = counts.notify_lsp,
                request_approval = counts.request_approval,
                sleep = counts.sleep,
                query_memory = counts.query_memory,
                run_layered_context_checkpoint = counts.run_layered_context_checkpoint,
                refresh_system_prompt = counts.refresh_system_prompt,
                emit_artifact = counts.emit_artifact,
                "v3 turn effect replay counts"
            );
            if do_replay_verify {
                let pair = [(live.turn_id.clone(), events.clone())];
                let stats = zagens_core::engine::replay_thread_message_stats(&pair);
                let timeline = zagens_core::engine::replay_thread_message_timeline(&pair);
                if let Some(summary) =
                    zagens_core::engine::verify_message_timeline_coherence(&stats, &timeline)
                {
                    tracing::warn!(
                        target: "kernel_turn_replay",
                        turn_id = %live.turn_id,
                        summary,
                        "message timeline coherence diff at turn end"
                    );
                }
            }
            ext.kernel_turn_events.finish_turn(live);
            (events, writer, do_replay_verify)
        };
        if do_replay_verify && let Some(writer) = writer {
            self.runtime_ext()
                .kernel_turn_replay
                .verify_turn_persisted(writer.as_ref(), &live.turn_id, &events)
                .await;
        }
    }

    async fn try_run_pre_inner_step_baseline(
        &mut self,
        client: &dyn LlmClient,
        turn: &TurnContext,
    ) -> bool {
        if !self.runtime_ext().kernel_machine_mode.uses_v3_turn_loop() {
            return false;
        }
        Engine::run_v3_pre_inner_step_baseline(self, client, &turn.id, turn.step).await;
        true
    }

    async fn try_run_system_prompt_refresh_queries(&mut self, turn: &TurnContext) -> bool {
        if !self.runtime_ext().kernel_machine_mode.uses_v3_turn_loop() {
            return false;
        }
        Engine::run_v3_system_prompt_refresh_queries(self, &turn.id, turn.step).await;
        true
    }

    async fn try_run_system_prompt_refresh(
        &mut self,
        turn: &TurnContext,
        mode: TurnLoopMode,
    ) -> bool {
        if !self.runtime_ext().kernel_machine_mode.uses_v3_turn_loop() {
            return false;
        }
        Engine::run_v3_system_prompt_refresh(self, &turn.id, turn.step, mode).await;
        true
    }

    async fn try_run_v3_turn_step(
        &mut self,
        turn: &mut TurnContext,
        client: &dyn LlmClient,
        mode: TurnLoopMode,
        tool_catalog: &mut [Tool],
        active_tool_names: &mut HashSet<String>,
        force_update_plan_first: bool,
        stream_retry_attempts: &mut u32,
        context_recovery_attempts: &mut u8,
        length_continuations: &mut u32,
        turn_error: &mut Option<String>,
        loop_guard: &mut zagens_core::engine::loop_guard::LoopGuard,
        consecutive_tool_error_steps: u32,
        tool_registry: Option<&Self::V3ToolRegistry>,
    ) -> Option<zagens_core::engine::turn_loop::v3_step::V3StepOutcome> {
        Some(
            super::super::engine_v3_step::run_v3_turn_step(
                self,
                turn,
                client,
                mode,
                tool_catalog,
                active_tool_names,
                force_update_plan_first,
                stream_retry_attempts,
                context_recovery_attempts,
                length_continuations,
                turn_error,
                loop_guard,
                consecutive_tool_error_steps,
                tool_registry,
            )
            .await,
        )
    }
}

fn user_message_text(msg: &Message) -> Option<String> {
    msg.content.iter().find_map(|block| match block {
        ContentBlock::Text { text, .. } => Some(text.clone()),
        _ => None,
    })
}

#[async_trait]
impl zagens_core::engine::turn_loop::TurnLoopSessionHost for Engine {
    fn session_mut(&mut self) -> &mut zagens_core::session::Session {
        &mut self.session
    }
    fn compaction_config(&self) -> &zagens_core::compaction::CompactionConfig {
        &self.config.compaction
    }
    fn workspace(&self) -> &std::path::Path {
        &self.config.workspace
    }
    fn strict_tool_mode(&self) -> bool {
        self.config.strict_tool_mode
    }
    fn scratchpad_config(&self) -> &zagens_core::scratchpad::ScratchpadConfig {
        &self.config.scratchpad
    }
    fn scratchpad_run_id(&self) -> Option<&str> {
        self.scratchpad_run_id.as_deref()
    }
    fn scratchpad_summary_injected_mut(&mut self) -> &mut bool {
        &mut self.scratchpad_summary_injected_this_turn
    }
    fn cancel_token(&self) -> &tokio_util::sync::CancellationToken {
        &self.cancel_token
    }
    fn tx_event(&self) -> &mpsc::Sender<Event> {
        &self.tx_event
    }
    fn rx_steer_mut(&mut self) -> &mut mpsc::Receiver<String> {
        &mut self.rx_steer
    }
    fn tool_exec_lock(&self) -> Arc<RwLock<()>> {
        self.tool_exec_lock.clone()
    }
    fn llm_client(&self) -> Option<Arc<dyn LlmClient>> {
        self.deepseek_client.clone()
    }
    async fn add_session_message(&mut self, message: Message) {
        Engine::add_session_message(self, message).await;
    }
    async fn emit_session_updated(&mut self) {
        Engine::emit_session_updated(self).await;
    }
    fn estimated_input_tokens(&self) -> usize {
        estimate_input_tokens_conservative(
            &self.session.messages,
            self.session.system_prompt.as_ref(),
        )
    }
    fn locale_tag(&self) -> &str {
        self.config.locale_tag.as_str()
    }
}

#[async_trait]
impl zagens_core::engine::turn_loop::InnerStepHost for Engine {
    type ToolRegistry = ToolRegistry;
    type McpPool = McpPool;
    fn prepare_tool_catalog(&self, catalog: &mut Vec<Tool>) {
        ensure_advanced_tooling(catalog);
    }
    fn initial_active_tool_names(&self, catalog: &[Tool]) -> HashSet<String> {
        initial_active_tools(catalog)
    }
    fn active_tools_for_step(
        &self,
        catalog: &[Tool],
        active: &HashSet<String>,
        force_update_plan_first: bool,
    ) -> Vec<Tool> {
        active_tools_for_step(catalog, active, force_update_plan_first)
    }
    fn is_mcp_tool_name(&self, name: &str) -> bool {
        McpPool::is_mcp_tool(name)
    }
    fn maybe_activate_deferred_tool(
        &self,
        tool_name: &str,
        catalog: &[Tool],
        active: &mut HashSet<String>,
    ) -> bool {
        maybe_activate_requested_deferred_tool(tool_name, catalog, active)
    }
    async fn execute_code_execution_tool(
        &self,
        input: &Value,
        workspace: &std::path::Path,
    ) -> Result<ToolResult, ToolError> {
        execute_code_execution_tool(input, workspace).await
    }
    fn execute_tool_search(
        &self,
        tool_name: &str,
        input: &Value,
        catalog: &[Tool],
        active: &mut HashSet<String>,
    ) -> Result<ToolResult, ToolError> {
        execute_tool_search(tool_name, input, catalog, active)
    }
    async fn flush_pending_lsp_diagnostics(&mut self) {
        Engine::flush_pending_lsp_diagnostics(self).await;
    }
    fn decorate_auth_error_message(&self, message: String) -> String {
        Engine::decorate_auth_error_message(self, message)
    }
    async fn run_post_edit_lsp_hook(&mut self, tool_name: &str, tool_input: &Value) {
        Engine::run_post_edit_lsp_hook(self, tool_name, tool_input).await;
    }
    fn record_scratchpad_tool_outcome(&mut self, tool_name: &str, success: bool) {
        scratchpad_flow::record_tool_outcome(&mut self.scratchpad_step, tool_name, success);
    }
    async fn record_long_horizon_tool_outcome(
        &mut self,
        tool_name: &str,
        tool_input: &serde_json::Value,
        result: &str,
        success: bool,
        tool_use_id: &str,
    ) {
        if tool_name == "load_skill"
            && success
            && let Some(name) = tool_input.get("name").and_then(|v| v.as_str())
        {
            crate::core::engine::stage_gate_flow::activate_stage_gate_for_skill(self, name.trim());
        }
        crate::core::engine::stage_gate_flow::after_tool_success(self, tool_name, success).await;
        crate::core::engine::stage_gate_flow::after_harness_assert_tool(
            self, tool_name, tool_input, success,
        )
        .await;

        if success {
            let phase = self.runtime_ext().agent_tool_phase;
            self.runtime_ext_mut().agent_tool_phase = phase.advance(tool_name);
        }

        let hint = zagens_core::engine::FailureHotStart::infer(tool_name, result, success);
        if hint != zagens_core::engine::FailureHotStart::None {
            self.runtime_ext_mut().failure_hot_start = hint;
        } else if success
            && matches!(
                tool_name,
                "diagnostics"
                    | "run_tests"
                    | "investigate"
                    | "answer_from_repo"
                    | "change_and_verify"
            )
        {
            self.runtime_ext_mut().failure_hot_start = zagens_core::engine::FailureHotStart::None;
        }

        if success && let Some(env) = parse_citations_from_tool_result(result) {
            let mut anchors = self.runtime_ext().diff_read_anchors.lock().await;
            anchors.record_from_evidence(Some(tool_use_id), tool_name, &env);
        }

        if success && crate::harness::affected_tests::is_edit_tool(tool_name) {
            if let Some(suffix) = crate::harness::affected_tests::hint_suffix_for_tool(
                &self.session.workspace,
                tool_name,
                tool_input,
            ) {
                let state = &mut self.runtime_ext_mut().long_horizon_state;
                state.pending_tool_result_suffix =
                    Some(match state.pending_tool_result_suffix.take() {
                        Some(existing) => format!("{existing}{suffix}"),
                        None => suffix,
                    });
            }

            // HL-4: optional auto scoped tests via verify-loop (default off).
            if self.config.long_horizon.post_edit_run_tests
                && let Some(auto_suffix) =
                    run_post_edit_tests_verify(self, tool_name, tool_input).await
            {
                let state = &mut self.runtime_ext_mut().long_horizon_state;
                state.pending_tool_result_suffix =
                    Some(match state.pending_tool_result_suffix.take() {
                        Some(existing) => format!("{existing}{auto_suffix}"),
                        None => auto_suffix,
                    });
            }
        }

        if !self.config.long_horizon.enabled {
            return;
        }
        // `success` already encodes exit-0 from the tool layer; do NOT also
        // require the result *text* to contain an "exit code: 0" marker — a
        // successful exec_shell returns raw stdout (e.g. `ok  monkey/lexer …`)
        // with no exit-code line (only failures print one), so that extra check
        // made recording NEVER fire on success and left `recent_verification_cmds`
        // permanently empty → every `[verify:]` item false-mismatched (DEMO5 #2).
        if success && matches!(tool_name, "exec_shell" | "run_tests") {
            let cmd = tool_input
                .get("command")
                .or_else(|| tool_input.get("cmd"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if crate::long_horizon::is_verification_like_command(cmd) {
                self.runtime_ext_mut()
                    .long_horizon_state
                    .record_verification_exec(cmd);
            }
        }

        // Verify gate (DEMO5 #2 / DEMO6): fires on any checklist mutation that
        // can mark items done — per-item `checklist_update`/`todo_update` **and**
        // bulk `checklist_write`. Earlier it only hooked the per-item tools, so
        // when the model completed items via `checklist_write` (as DEMO6 did) the
        // gate never ran and emitted no `verify_gate` nodes. We now scan the
        // post-write snapshot for completed items and run the verdict on each
        // *newly* completed one, deduped via `gated_completed_ids` so a bulk write
        // that re-sends the whole list only fires once per item.
        let verify_suffix = if success
            && matches!(
                tool_name,
                "checklist_update" | "todo_update" | "checklist_write"
            ) {
            let checklist = self.config_ext().todos.lock().await.snapshot();

            // Authoritative checklist sync (CCR progress-desync fix): push the
            // engine's *live* checklist snapshot to the host through the reliable
            // harness-status channel. The persisted checklist that the desktop UI
            // reads is otherwise written ONLY by the monitor's per-tool
            // `ToolCallComplete` hook, which silently misses some checklist
            // mutations — e.g. a `checklist_update` issued in a parallel/deferred
            // tool batch whose `ToolCallStarted` the monitor never tracked, so its
            // completion is dropped by the `tool_items` id-match. The result is a
            // progress bar / checklist frozen mid-task (observed at 7/12 = 58%)
            // even though the engine's task graph is actually complete. Emitting
            // here — where we already hold the authoritative snapshot for every
            // successful checklist mutation — guarantees the UI reconciles to the
            // engine's truth regardless of monitor event-tracking gaps. The JSON
            // is a serialized `TodoListSnapshot`, exactly what `checklist_from_json`
            // and the `panel.checklist` consumer already parse.
            if let Ok(checklist_json) = serde_json::to_string(&checklist) {
                let _ = self
                    .tx_event
                    .send(Event::status(format!(
                        "long_horizon.checklist_persist:{checklist_json}"
                    )))
                    .await;
            }

            let lang = self.config.locale_tag.clone();
            let recent = self
                .runtime_ext()
                .long_horizon_state
                .recent_verification_cmds
                .clone();
            let newly: Vec<(u32, String)> = {
                let state = &mut self.runtime_ext_mut().long_horizon_state;
                checklist
                    .items
                    .iter()
                    .filter(|i| i.status == crate::tools::todo::TodoStatus::Completed)
                    .filter(|i| state.mark_completion_gated(i.id))
                    .map(|i| (i.id, i.content.clone()))
                    .collect()
            };
            // Surface the first actionable advisory (mismatch / unverified).
            let mut chosen: Option<String> = None;
            for (id, content) in newly {
                let (verdict, suffix) =
                    crate::long_horizon::verify_gate_verdict(&content, &recent, &lang);
                // [lht-probe] verdict per newly-completed item → sidecar.log
                // (verified / mismatch / unverified_acceptance / untagged_ok).
                let item_snippet = if content.chars().count() > 60 {
                    format!("{}…", content.chars().take(60).collect::<String>())
                } else {
                    content.clone()
                };
                eprintln!(
                    "[lht-probe] verify_gate tool={tool_name} item={id} verdict={verdict} content={item_snippet:?}"
                );
                // Also surface as a `long_horizon.*` status event so the decision
                // shows up live in the LHT panel "nodes" tab (DEMO5 #3).
                let _ = self
                    .tx_event
                    .send(Event::status(format!(
                        "long_horizon.verify_gate: {{\"item\":{id},\"verdict\":\"{verdict}\"}}"
                    )))
                    .await;
                if suffix.is_some() && chosen.is_none() {
                    chosen = suffix;
                }
            }
            chosen
        } else {
            None
        };

        // Qualified progress (§4.3.1): read-only execs such as `ls`/`echo` do
        // NOT count — exec/test commands must match the verification pattern and
        // exit 0, while write/plan/checklist tools count on success.
        let qualifies = success
            && match tool_name {
                "edit_file" | "write_file" | "apply_patch" | "checklist_update"
                | "checklist_write" | "todo_update" | "update_plan" => true,
                "exec_shell" | "run_tests" => {
                    let cmd = tool_input
                        .get("command")
                        .or_else(|| tool_input.get("cmd"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    crate::long_horizon::VERIFICATION_RE.is_match(cmd)
                }
                _ => false,
            };

        let state = &mut self.runtime_ext_mut().long_horizon_state;
        state.on_assistant_with_tools();
        if crate::long_horizon::tool_marks_lht_checkpoint(tool_name, tool_input, success) {
            state.pending_cycle_at_checkpoint = true;
        }
        if let Some(suffix) = verify_suffix {
            state.pending_tool_result_suffix = Some(suffix);
        }
        if qualifies {
            state.progress_since_last_nudge = true;
        }
        // C1 ("一推到底"): qualified tool progress re-arms the once-per-turn LHT
        // continue nudge. Without this the harness could nudge a prose-only stop
        // exactly once per turn, so a phased refactor stalled after the model
        // finished phase 1 and summarized. Re-arming lets the gate fire again on
        // the next premature stop; the per-item `NudgeTracker` caps
        // (`max_nudges_per_item` / `blocked_nudges_without_progress`) still bound
        // total nudges, so a model that stops without real work is not looped.
        if qualifies {
            self.long_horizon_continue_injected_this_turn = false;
        }
    }
    fn take_long_horizon_tool_suffix(&mut self) -> Option<String> {
        self.runtime_ext_mut()
            .long_horizon_state
            .take_tool_result_suffix()
    }
    fn on_audit_scratchpad_bind_success(
        &mut self,
        mode: TurnLoopMode,
        tool_name: &str,
        catalog: &mut [Tool],
        active: &mut HashSet<String>,
    ) {
        if !scratchpad_flow::is_scratchpad_write_tool(tool_name) {
            return;
        }
        self.sync_scratchpad_run_id_from_wire();
        if self.scratchpad_run_id.is_none() {
            return;
        }
        zagens_core::engine::tool_catalog::activate_audit_subagent_tools(
            catalog,
            mode,
            self.scratchpad_run_id.as_deref(),
            active,
        );
    }
    async fn handle_no_tool_uses(
        &mut self,
        turn: &mut TurnContext,
        pending_steers: &mut Vec<String>,
        current_text_visible: &str,
        has_sendable_assistant_content: bool,
    ) -> TurnLoopControl {
        self.handle_no_tool_uses_turn_loop(
            turn,
            pending_steers,
            current_text_visible,
            has_sendable_assistant_content,
        )
        .await
    }
    fn pre_tool_snapshot(&self, workspace: &std::path::Path, tool_id: &str) {
        pre_tool_snapshot(workspace, tool_id, self.config.snapshots_max_workspace_gb);
    }
    fn effective_reasoning_effort_for_request(&mut self) -> Option<String> {
        zagens_core::engine::turn_loop::resolve_auto_effort(
            self.session.reasoning_effort.as_deref(),
            &self.session.messages,
            |is_subagent, last_msg| {
                crate::auto_reasoning::select(is_subagent, last_msg)
                    .as_setting()
                    .to_string()
            },
        )
    }
    fn parse_streaming_tool_input(&self, buffer: &str) -> Option<Value> {
        super::super::dispatch::parse_tool_input(buffer)
    }
    fn final_streaming_tool_input(&self, state: &ToolUseState) -> Value {
        super::super::dispatch::final_tool_input(state)
    }
    async fn ensure_mcp_pool_for_tools(
        &mut self,
        tool_uses: &[ToolUseState],
    ) -> Option<Arc<AsyncMutex<McpPool>>> {
        if !tool_uses
            .iter()
            .any(|tool| McpPool::is_mcp_tool(&tool.name))
        {
            return None;
        }
        match self.ensure_mcp_pool().await {
            Ok(pool) => Some(pool),
            Err(err) => {
                let _ = self.tx_event.send(Event::status(err.to_string())).await;
                None
            }
        }
    }
    fn resolve_hallucinated_tool_name(
        &self,
        name: &str,
        catalog: &[Tool],
        registry: Option<&Self::ToolRegistry>,
    ) -> Option<String> {
        let registry = registry?;
        let canonical = registry.resolve(name)?;
        if catalog.iter().any(|d| d.name == canonical) {
            Some(canonical.to_string())
        } else {
            None
        }
    }
    fn tool_plan_approval_meta(
        &self,
        tool_name: &str,
        tool_input: &serde_json::Value,
        registry: Option<&Self::ToolRegistry>,
    ) -> ToolPlanApprovalMeta {
        let turn_mode = match self.runtime_ext().turn_app_mode {
            AppMode::Agent => TurnLoopMode::Agent,
            AppMode::Plan => TurnLoopMode::Plan,
            AppMode::Yolo => TurnLoopMode::Yolo,
        };
        crate::tools::policy_bridge::resolve_tool_plan_approval_meta(
            self.runtime_ext().tools_policy,
            turn_mode,
            self.session.trust_mode,
            tool_name,
            tool_input,
            registry,
        )
    }
    fn model_request_fingerprint(
        &self,
        request: &zagens_core::chat::MessageRequest,
    ) -> Option<zagens_core::engine::RequestFingerprint> {
        let fp = crate::request_fingerprint::fingerprint_message_request(request);
        Some(fp)
    }
    fn compiler_request_context(
        &mut self,
        active_tools: Option<&[zagens_core::chat::Tool]>,
    ) -> Option<zagens_core::engine::turn_loop::CompilerRequestContext> {
        use zagens_core::engine::ContextProjection;
        use zagens_core::engine::turn_loop::CompilerRequestContext;

        let mut snapshot =
            crate::context_compiler_shadow::ContextCompilerStateSnapshot::from_session(
                &self.session,
                0,
            );

        // Replace placeholder estimate with the actual serialized tool catalog size.
        if let Some(tools) = active_tools {
            let (builtin, mcp) = crate::context_prompt_segments::split_tool_catalog_tokens(tools);
            snapshot.tools_builtin_tokens = builtin;
            snapshot.tools_mcp_tokens = mcp;
            snapshot.tool_catalog_est_tokens = builtin.saturating_add(mcp);
        }

        // Scratchpad reminder budget estimate (pure-logic, no I/O).
        snapshot.scratchpad_reminder_est_tokens =
            crate::context_compiler_shadow::scratchpad_reminder_est_tokens(
                &self.config.scratchpad,
                &self.scratchpad_step,
            );

        // Build compiler and compile.  If an overflow-recovery budget cap was set
        // by `try_budget_recompile`, use it for `compile_with_budget_override` so
        // Volatile / SemiStatic sources are evicted on this retry request.
        // Consume the cap first (before borrowing `self.session`) so the mutable
        // borrow of `self.0` does not alias the immutable borrow via `proj`.
        let overflow_budget_cap = self.0.overflow_source_budget_cap.take();
        let turn_events = self.runtime_ext().kernel_turn_events.turn_events();
        let projection =
            zagens_core::engine::turn_machine::TurnKernelProjection::from_events(turn_events);
        let queried_sources =
            zagens_core::engine::turn_loop::memory_plane_compiler_policy::compiler_queried_sources_from_projection(
                &projection,
            );

        let compiler = crate::context_compiler_shadow::build_compiler_from_snapshot(&snapshot);
        let proj = ContextProjection::from_session(&self.session, snapshot.step_idx);
        let query_overrides =
            zagens_core::engine::turn_loop::memory_plane_compiler_policy::compiler_budget_overrides_for_queried_sources(
                &queried_sources,
            );

        let compiled = if let Some(budget_cap) = overflow_budget_cap {
            // Applies for exactly one request retry; cap was already consumed above.
            match compiler.compile_with_budget_override(&proj, budget_cap, &query_overrides) {
                Ok(ctx) => ctx,
                Err(_) => compiler.compile(&proj),
            }
        } else {
            compiler.compile(&proj)
        };

        let message_tokens =
            zagens_core::engine::context_usage_breakdown::conversation_message_tokens(
                &self.session.messages,
            );
        let assembly_report = compiled
            .assembly_report
            .clone()
            .with_message_tokens(message_tokens);
        self.runtime_ext_mut().last_context_assembly_report = Some(assembly_report);

        // Determine which sources survived compilation (for eviction-aware assembly).
        let mut has_compaction = compiled
            .contributions
            .iter()
            .any(|c| c.source_id.0 == "memory.compaction" && c.token_count > 0);
        let mut has_working_set = compiled
            .contributions
            .iter()
            .any(|c| c.source_id.0 == "working_set" && c.token_count > 0);

        (has_compaction, has_working_set) = zagens_core::engine::turn_loop::memory_plane_compiler_policy::resolved_compiler_includes_for_queried_sources(
            &queried_sources,
            has_compaction,
            has_working_set,
            !snapshot.compaction_text.is_empty(),
            !snapshot.working_set_text.is_empty(),
        );

        // system_prompt: static base always included; compaction text only if not evicted.
        let system_text = if has_compaction {
            crate::context_compiler_shadow::assemble_system_text_for_v2(&snapshot)
        } else {
            snapshot.static_base_text.clone()
        };
        let system_prompt = if system_text.is_empty() {
            None
        } else {
            Some(zagens_core::chat::SystemPrompt::Text(system_text))
        };

        // turn_meta_text: only when working_set source survived eviction.
        let turn_meta_text = if has_working_set && !snapshot.working_set_text.is_empty() {
            Some(snapshot.working_set_text.clone())
        } else {
            None
        };

        if !queried_sources.is_empty() {
            tracing::info!(
                target: "kernel_v3",
                step = snapshot.step_idx,
                queried = ?queried_sources,
                has_working_set,
                has_compaction,
                "compiler_request_context: memory query sources this step (log projection)"
            );
        }

        Some(CompilerRequestContext {
            system_prompt,
            turn_meta_text,
        })
    }
    async fn execute_tool_plans(
        &mut self,
        mode: TurnLoopMode,
        plans: Vec<ToolExecutionPlan>,
        tool_catalog: &[Tool],
        active_tool_names: &mut HashSet<String>,
        tool_registry: Option<&Self::ToolRegistry>,
        mcp_pool: Option<Arc<AsyncMutex<McpPool>>>,
        tool_exec_lock: Arc<RwLock<()>>,
    ) -> Vec<ToolExecOutcome> {
        super::tool_plans_exec::execute_tool_plans(
            self,
            mode,
            plans,
            tool_catalog,
            active_tool_names,
            tool_registry,
            mcp_pool,
            tool_exec_lock,
        )
        .await
    }
    async fn run_capacity_post_tool_checkpoint(
        &mut self,
        turn: &mut TurnContext,
        mode: TurnLoopMode,
        tool_registry: Option<&Self::ToolRegistry>,
        tool_exec_lock: Arc<RwLock<()>>,
        mcp_pool: Option<Arc<AsyncMutex<Self::McpPool>>>,
        step_error_count: usize,
        consecutive_tool_error_steps: u32,
    ) -> bool {
        self.turn_loop_capacity_post_tool(
            turn,
            mode,
            tool_registry,
            tool_exec_lock,
            mcp_pool,
            step_error_count,
            consecutive_tool_error_steps,
        )
        .await
    }

    async fn push_live_context_panel_events(&mut self) {
        Engine::push_live_context_panel_events(self).await;
    }
}

#[async_trait]
impl zagens_core::engine::turn_loop::TurnLoopOuterHost for Engine {
    fn reset_scratchpad_step(&mut self) {
        self.scratchpad_step.reset();
    }
    async fn refresh_system_prompt(&mut self, mode: TurnLoopMode) {
        Engine::refresh_system_prompt(self, turn_loop_to_app_mode(mode));
    }
    async fn inject_live_steer(&mut self, turn: &TurnContext, steer: String) {
        if self.runtime_ext().kernel_machine_mode.uses_v3_turn_loop() {
            let mut interpreter = EffectInterpreter::new(self);
            let _ = interpreter
                .interpret(Effect::InjectSteer { text: steer })
                .await;
            return;
        }
        Engine::run_inject_steer_effect(self, &turn.id, turn.step, steer).await;
    }
    async fn run_auto_compaction(&mut self, client: &dyn LlmClient, turn: &TurnContext) {
        self.run_pre_inner_step_auto_compaction(client, turn).await;
    }
    async fn run_pre_inner_step_auto_compaction(
        &mut self,
        client: &dyn LlmClient,
        turn: &TurnContext,
    ) {
        if self.runtime_ext().kernel_machine_mode.uses_v3_turn_loop() {
            Engine::run_v3_planner_auto_compaction(self, client, &turn.id, turn.step).await;
            return;
        }

        let compaction_pins = self
            .session
            .working_set
            .pinned_message_indices(&self.session.messages, &self.session.workspace);
        let mut compaction_paths = self.session.working_set.top_paths(24);
        scratchpad_flow::extend_compaction_paths(
            &self.session.workspace,
            self.scratchpad_run_id.as_deref(),
            &mut compaction_paths,
        );

        if !self.config.compaction.enabled
            || !auto_compaction_allowed(&self.session.model, &self.config.cycle)
            || !should_compact(
                &self.session.messages,
                &self.config.compaction,
                Some(&self.session.workspace),
                Some(&compaction_pins),
                Some(&compaction_paths),
            )
        {
            return;
        }

        Engine::route_auto_compaction(self, client, &turn.id).await;
    }
    async fn run_pre_inner_step_layered_context(&mut self) {
        if self.runtime_ext().kernel_machine_mode.uses_v3_turn_loop() {
            let turn_id = self
                .runtime_ext()
                .kernel_active_turn_id
                .clone()
                .unwrap_or_default();
            let step = self.runtime_ext().kernel_active_step;
            Engine::run_v3_planner_layered_context(self, &turn_id, step).await;
            return;
        }
        Engine::layered_context_checkpoint(self).await;
    }
    async fn layered_context_checkpoint(&mut self) {
        self.run_pre_inner_step_layered_context().await;
    }
    async fn recover_context_overflow(
        &mut self,
        client: &dyn LlmClient,
        reason: &str,
        max_output_tokens: u32,
    ) -> bool {
        Engine::recover_context_overflow(self, client, reason, max_output_tokens).await
    }
    async fn run_capacity_pre_request_checkpoint(
        &mut self,
        turn: &TurnContext,
        client: Option<&dyn LlmClient>,
        mode: TurnLoopMode,
    ) -> bool {
        self.turn_loop_capacity_pre_request(turn, client, mode)
            .await
    }
    async fn run_capacity_error_escalation_checkpoint(
        &mut self,
        turn: &mut TurnContext,
        mode: TurnLoopMode,
        step_error_count: usize,
        consecutive_tool_error_steps: u32,
        error_categories: &[zagens_core::error_taxonomy::ErrorCategory],
    ) -> bool {
        self.turn_loop_capacity_error_escalation(
            turn,
            mode,
            step_error_count,
            consecutive_tool_error_steps,
            error_categories,
        )
        .await
    }
    async fn maybe_lht_pre_request_hooks(&mut self, _mode: TurnLoopMode) {
        if !self.config.long_horizon.enabled {
            return;
        }
        let active = self.estimated_input_tokens() as u64;
        let headroom = crate::core::engine::context::turn_response_headroom_tokens();
        let model = self.session.model.clone();
        let thresholds = self.scaled_context_thresholds();
        let in_band =
            crate::long_horizon::in_lht_warning_band(active, headroom, &model, &thresholds);
        let emit_warning = {
            let lh = &self.runtime_ext().long_horizon_state;
            in_band && !lh.last_warning_band_emitted
        };
        if emit_warning {
            let pct = crate::long_horizon::context_pressure_ratio(active, headroom, &model)
                .map(|r| (r * 100.0).round() as u8)
                .unwrap_or(0);
            let _ = self
                .tx_event
                .send(crate::core::events::Event::status(format!(
                    "long_horizon.context_warning: {{\"pressure_pct\":{pct}}}"
                )))
                .await;
        }
        let lh_cfg = self.config.long_horizon.clone();
        let reinject = {
            let lh = &mut self.runtime_ext_mut().long_horizon_state;
            lh.last_warning_band_emitted = in_band;
            lh.assistant_steps = lh.assistant_steps.saturating_add(1);
            crate::long_horizon::should_reinject_this_step(&lh_cfg, lh.assistant_steps)
        };
        if !reinject {
            return;
        }
        let plan = self.config_ext().plan_state.lock().await.snapshot();
        let checklist = self.config_ext().todos.lock().await.snapshot();
        let lang = self.config.locale_tag.as_str();
        let Some(msg) = crate::long_horizon::build_objective_reinject_message(
            &plan,
            &checklist,
            &self.session.messages,
            lang,
        ) else {
            return;
        };
        let Some(text) = user_message_text(&msg) else {
            return;
        };
        if self.runtime_ext().kernel_machine_mode.uses_v3_turn_loop() {
            let mut interpreter = EffectInterpreter::new(self);
            let _ = interpreter.interpret(Effect::InjectSteer { text }).await;
            return;
        }
        Engine::add_session_message(self, msg).await;
    }
    async fn maybe_continue_at_step_limit(&mut self, turn: &TurnContext) -> bool {
        if !self.config.long_horizon.enabled || !self.config.task_type.uses_code_tool_surface() {
            return false;
        }
        let plan = self.config_ext().plan_state.lock().await.snapshot();
        let checklist = self.config_ext().todos.lock().await.snapshot();
        let Some(open) =
            crate::long_horizon::CodeTaskGraph::continuation_open_items(&plan, &checklist)
        else {
            return false;
        };
        let text = crate::long_horizon::build_step_limit_continue_nudge(
            open,
            self.config.locale_tag.as_str(),
        );
        self.inject_step_limit_continuation_steer(turn, text, open)
            .await;
        true
    }
    async fn maybe_continue_after_loop_guard_halt(&mut self, turn: &TurnContext) -> bool {
        // Audit scratchpad recovery is independent of LHT / code-tool surface.
        if let Some((text, pending)) =
            crate::core::engine::scratchpad_flow::maybe_continue_after_loop_guard_halt_audit(
                &self.session.workspace,
                self.scratchpad_run_id.as_deref(),
                &self.config.scratchpad,
            )
        {
            self.inject_loop_guard_continuation_steer(turn, text, pending)
                .await;
            let _ = self
                .tx_event
                .send(Event::status(
                    "Audit scratchpad: loop-guard Halt recovered — continue P2 via scratchpad_defer_remaining",
                ))
                .await;
            return true;
        }

        if !self.config.long_horizon.enabled || !self.config.task_type.uses_code_tool_surface() {
            return false;
        }
        let plan = self.config_ext().plan_state.lock().await.snapshot();
        let checklist = self.config_ext().todos.lock().await.snapshot();
        let Some(open) =
            crate::long_horizon::CodeTaskGraph::continuation_open_items(&plan, &checklist)
        else {
            return false;
        };
        let text = crate::long_horizon::build_loop_guard_continue_nudge(
            open,
            self.config.locale_tag.as_str(),
        );
        self.inject_loop_guard_continuation_steer(turn, text, open)
            .await;
        true
    }
    async fn maybe_cycle_handoff_on_context_overflow(
        &mut self,
        _turn: &TurnContext,
        mode: TurnLoopMode,
    ) -> bool {
        // Only roll a handoff when the cycle mechanism is actually enabled;
        // otherwise there's no briefing/seed machinery to fall back to and the
        // turn fails as before. The handoff itself preserves LHT state
        // (plan / todos / handoff.md) when long-horizon is on.
        if !self.config.cycle.enabled {
            return false;
        }
        Engine::force_cycle_handoff_for_overflow(self, turn_loop_to_app_mode(mode)).await
    }
    async fn maybe_advance_cycle_at_checkpoint(
        &mut self,
        mode: TurnLoopMode,
        _turn: &TurnContext,
    ) -> bool {
        // Only long-horizon code tasks evaluate the cycle gate mid-turn; the
        // between-turns boundary still covers everything else. Plan mode never
        // rolls a cycle, and there's no point without the cycle machinery.
        if mode.is_plan()
            || !self.config.cycle.enabled
            || !self.config.long_horizon.enabled
            || !self.config.task_type.uses_code_tool_surface()
        {
            return false;
        }
        // Push a live context-usage snapshot off the (mid-turn starved) op loop:
        // the monitor forwards this as `panel.context` + `context.usage`, so the
        // Context tab / cycle-pressure bar update every step instead of freezing
        // until turn end (where the op-loop `QueryContext` finally drains). Same
        // channel as `checklist_persist`. Cheap relative to the per-step token
        // estimate the cycle gate already computes below.
        self.push_live_context_panel_events().await;
        // Reuse the exact between-turns gate (threshold + long-horizon
        // early-advance band) and handoff body. At this call site the streaming
        // phase and tool execution have completed, so `in_flight` is false —
        // a clean per-step boundary with no mid-edit/stream cut.
        use zagens_core::engine::turn_loop::continuation_boundary_policy::OuterBoundaryKind;
        Engine::maybe_advance_cycle(
            self,
            turn_loop_to_app_mode(mode),
            Some(OuterBoundaryKind::InTurnCycleAdvance),
        )
        .await
    }
    async fn note_incomplete_stop_if_lht(&mut self) {
        // The turn loop is about to end as `Completed`. If a long-horizon task
        // graph is still incomplete, this is a give-up (nudge budget exhausted,
        // loop-guard continuations spent, REPL/no-tool break, etc.), not a real
        // completion — emit a probe so the UI / sidecar.log don't read a false
        // green. Purely observational; the outcome itself is unchanged.
        if !self.config.long_horizon.enabled || !self.config.task_type.uses_code_tool_surface() {
            return;
        }
        let plan = self.config_ext().plan_state.lock().await.snapshot();
        let checklist = self.config_ext().todos.lock().await.snapshot();
        let Some(open) =
            crate::long_horizon::CodeTaskGraph::task_still_open_for_lht(&plan, &checklist)
        else {
            return;
        };
        let _ = self
            .tx_event
            .send(Event::status(format!(
                "long_horizon.incomplete_stop: {{\"open_items\":{open}}}"
            )))
            .await;
    }
    async fn maybe_inject_scratchpad_summary(&mut self, turn: &TurnContext) -> bool {
        if self.scratchpad_summary_injected_this_turn {
            return false;
        }
        if !self.runtime_ext().kernel_machine_mode.uses_v3_turn_loop() {
            let Some(summary_msg) = scratchpad_flow::maybe_summary_before_final_answer(
                &self.session.workspace,
                self.scratchpad_run_id.as_deref(),
                &self.config.scratchpad,
            ) else {
                return false;
            };
            let text = crate::core::engine::memory_plane_ops::user_message_plain_text(&summary_msg);
            self.inject_memory_plane_steer_message(text).await;
            self.scratchpad_summary_injected_this_turn = true;
            emit_kernel_event(
                self,
                KernelEvent::ScratchpadSummaryInjected {
                    turn_id: turn.id.clone(),
                    at_step: turn.step,
                },
            );
            return true;
        }
        Engine::run_emit_artifact_effect(
            self,
            &turn.id,
            turn.step,
            zagens_core::engine::turn_loop::memory_artifact_policy::MemoryArtifactKind::ScratchpadSnapshot,
            None,
        )
        .await
    }

    async fn maybe_inject_scratchpad_reminder(&mut self, turn: &TurnContext) {
        if self.runtime_ext().kernel_machine_mode.uses_v3_turn_loop() {
            let _ = Engine::run_emit_artifact_effect(
                self,
                &turn.id,
                turn.step,
                zagens_core::engine::turn_loop::memory_artifact_policy::MemoryArtifactKind::ScratchpadReminder,
                None,
            )
            .await;
            return;
        }
        if let Some((reminder, area_path)) = scratchpad_flow::build_readonly_reminder_message(
            &self.session.workspace,
            self.scratchpad_run_id.as_deref(),
            &self.config.scratchpad,
            &self.scratchpad_step,
        ) {
            let text = crate::core::engine::memory_plane_ops::user_message_plain_text(&reminder);
            self.inject_memory_plane_steer_message(text).await;
            emit_kernel_event(
                self,
                KernelEvent::ScratchpadReminderInjected {
                    turn_id: turn.id.clone(),
                    step_idx: turn.step,
                    area_path,
                },
            );
        }
    }
}
#[must_use]
pub(crate) fn app_mode_to_turn_loop(mode: AppMode) -> TurnLoopMode {
    match mode {
        AppMode::Agent => TurnLoopMode::Agent,
        AppMode::Yolo => TurnLoopMode::Yolo,
        AppMode::Plan => TurnLoopMode::Plan,
    }
}

#[must_use]
pub(crate) fn turn_loop_to_app_mode(mode: TurnLoopMode) -> AppMode {
    match mode {
        TurnLoopMode::Agent => AppMode::Agent,
        TurnLoopMode::Yolo => AppMode::Yolo,
        TurnLoopMode::Plan => AppMode::Plan,
    }
}

/// HL-4: run scoped `tests_pass` via `HarnessVerifyLoop::run_with_act` after an edit.
async fn run_post_edit_tests_verify(
    engine: &mut Engine,
    tool_name: &str,
    tool_input: &serde_json::Value,
) -> Option<String> {
    use crate::long_horizon::harness_verify_loop::{
        HarnessVerifyLoop, HarnessVerifyLoopConfig, HarnessVerifyOutcome, VerifyStageSpec,
        outcome_records, record_to_kernel_event,
    };
    use crate::long_horizon::predicate::{CompletionGateExec, names};
    use zagens_core::engine::edited_paths_for_tool;

    let workspace = engine.session.workspace.clone();
    let paths = edited_paths_for_tool(tool_name, tool_input);
    let suggestion = crate::harness::affected_tests::suggest_for_edited_paths(&workspace, &paths)?;

    let stages = [VerifyStageSpec {
        stage: "post_edit".into(),
        predicate: names::TESTS_PASS.into(),
        args: serde_json::json!({
            "cmd": format!("cargo test {}", suggestion.run_tests_args),
        }),
    }];

    let shell_manager = engine.runtime_ext().shell_manager.clone();
    let cancel = engine.cancel_token.clone();
    let exec = CompletionGateExec {
        shell_manager: &shell_manager,
        cancel_token: Some(&cancel),
        progress_tx: None,
    };
    let loop_ = HarnessVerifyLoop::new(&workspace)
        .with_exec(&exec)
        .with_config(HarnessVerifyLoopConfig {
            max_retries: 1,
            timeout_ms: 300_000,
        });

    let outcome = loop_.run_with_act(&stages, |_| async {}).await;
    let records = outcome_records(&outcome).to_vec();
    let turn_id = engine
        .runtime_ext()
        .kernel_active_turn_id
        .clone()
        .unwrap_or_else(|| "unknown".into());
    for record in &records {
        emit_kernel_event(engine, record_to_kernel_event(turn_id.clone(), record));
    }
    engine
        .runtime_ext_mut()
        .long_horizon_state
        .pending_harness_verify
        .extend(records.clone());
    crate::harness::telemetry::append_harness_verify_records(&turn_id, &records);

    let pass = matches!(outcome, HarnessVerifyOutcome::Passed { .. });
    let retry = records.last().map(|r| r.retry_no).unwrap_or(0);
    let detail = records
        .iter()
        .find(|r| !r.pass)
        .and_then(|r| r.suggestion.clone())
        .unwrap_or_default();
    Some(format!(
        "\n\n[HL-4 post_edit_run_tests] `cargo test {}` → {} (retry_no={retry}){extra}",
        suggestion.run_tests_args,
        if pass { "pass" } else { "fail" },
        extra = if detail.is_empty() {
            String::new()
        } else {
            format!("\n{detail}")
        }
    ))
}

#[cfg(test)]
mod m4_drift_guard {
    use super::*;
    use zagens_core::engine::dispatch::is_mcp_tool_name;

    /// M4 cross-verify: the tui-side inherent `McpPool::is_mcp_tool`
    /// (in `crates/tui/src/mcp.rs:1498` — frozen per spike §6 M4
    /// "zero changes to mcp.rs body") and the core-side free function
    /// `zagens_core::engine::dispatch::is_mcp_tool_name` must
    /// produce identical output on every name in a curated set.
    ///
    /// If either definition gains a new prefix / matched literal,
    /// this test fails and forces a mirrored update.
    #[test]
    fn is_mcp_tool_name_matches_tui_mcp_pool() {
        const NAMES: &[&str] = &[
            "mcp_filesystem_read",
            "mcp_filesystem_write",
            "mcp_git_status",
            "mcp_browser_navigate",
            "list_mcp_resources",
            "list_mcp_resource_templates",
            "read_mcp_resource",
            "mcp_",
            "read_file",
            "edit_file",
            "exec_shell",
            "request_user_input",
            "update_plan",
            "tool_search_bm25",
            "",
        ];
        for name in NAMES {
            assert_eq!(
                McpPool::is_mcp_tool(name),
                is_mcp_tool_name(name),
                "drift between tui::mcp::McpPool::is_mcp_tool and \
                 core::engine::dispatch::is_mcp_tool_name on {name:?}"
            );
        }
    }

    /// M4 promoted `TurnLoopMcpPool` to `McpHost`. `McpPool` keeps
    /// satisfying the (deprecated) marker via the blanket impl, so
    /// existing `Self::McpPool: TurnLoopMcpPool` bounds continue to
    /// work. The new `McpHost` default-impl predicates must match
    /// the inherent / free-fn surface.
    #[test]
    fn mcp_pool_satisfies_mcp_host_with_default_impls() {
        fn _accepts<T: McpHost>(_: &T) {}
        let pool: Option<McpPool> = None;
        if let Some(ref p) = pool {
            _accepts(p);
            assert_eq!(
                p.is_mcp_tool("mcp_filesystem_read"),
                McpPool::is_mcp_tool("mcp_filesystem_read")
            );
        }
        // Type-level assertion: McpPool: McpHost.
        const _: fn() = || {
            fn _bound<T: McpHost>() {}
            _bound::<McpPool>();
        };
    }
}
