//! `TurnLoopHost` implementation for the TUI `Engine` (P2 PR4 step 2).

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::{Mutex as AsyncMutex, RwLock, mpsc};
use zagens_core::chat::{ContentBlock, LlmClient, Message, Tool};
use zagens_core::engine::KernelTurnHost;
use zagens_core::engine::TurnLoopHost;
use zagens_core::engine::context::estimate_input_tokens_conservative;
use zagens_core::engine::hosts::McpHost;
use zagens_core::engine::kernel_event::{KernelEvent, MessageRange};
use zagens_core::engine::streaming::ToolUseState;
use zagens_core::engine::turn_loop::TurnLoopToolRegistry;
use zagens_core::engine::turn_loop::control::TurnLoopControl;
use zagens_core::engine::turn_loop::exec::{
    ToolExecOutcome, ToolExecutionPlan, ToolPlanApprovalMeta,
};
use zagens_core::engine::turn_machine::{KernelEventSink, LiveTurnSnapshot, emit_kernel_event};
use zagens_core::turn::{TurnContext, TurnLoopMode};
use zagens_tools::{ToolError, ToolResult};

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

impl KernelTurnHost for Engine {
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
            .kernel_projection_shadow
            .record(event.clone());
    }

    fn reset_kernel_projection_shadow(&mut self) {
        self.runtime_ext_mut().kernel_projection_shadow.reset_turn();
    }

    fn kernel_shadow_turn_events(&self) -> Vec<KernelEvent> {
        self.runtime_ext()
            .kernel_projection_shadow
            .turn_events()
            .to_vec()
    }

    fn sync_kernel_turn_frame(&mut self, turn: &TurnContext) {
        let ext = self.runtime_ext_mut();
        ext.kernel_active_turn_id = Some(turn.id.clone());
        ext.kernel_active_step = turn.step;
    }
}

#[async_trait]
impl TurnLoopHost for Engine {
    type ToolRegistry = ToolRegistry;
    type McpPool = McpPool;

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

    fn reset_scratchpad_step(&mut self) {
        self.scratchpad_step.reset();
    }

    async fn refresh_system_prompt(&mut self, mode: TurnLoopMode) {
        Engine::refresh_system_prompt(self, turn_loop_to_app_mode(mode));
    }

    async fn add_session_message(&mut self, message: Message) {
        Engine::add_session_message(self, message).await;
    }

    async fn emit_session_updated(&mut self) {
        Engine::emit_session_updated(self).await;
    }

    async fn run_auto_compaction(&mut self, client: &dyn LlmClient, turn: &TurnContext) {
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

        let compaction_id = format!("compact_{}", &uuid::Uuid::new_v4().to_string()[..8]);
        Engine::emit_compaction_started(
            self,
            compaction_id.clone(),
            true,
            "Auto context compaction started".to_string(),
        )
        .await;
        let _ = self
            .tx_event
            .send(Event::status("Auto-compacting context...".to_string()))
            .await;
        let auto_messages_before = self.session.messages.len();
        self.fire_pre_compact(self.runtime_ext().turn_app_mode, false);
        match compact_messages_safe(
            client,
            &self.session.messages,
            &self.config.compaction,
            Some(&self.session.workspace),
            Some(&compaction_pins),
            Some(&compaction_paths),
        )
        .await
        {
            Ok(result) => {
                if !result.messages.is_empty() || self.session.messages.is_empty() {
                    let auto_messages_after = result.messages.len();
                    self.session.messages = result.messages;
                    Engine::merge_compaction_summary(self, result.summary_prompt);
                    Engine::emit_session_updated(self).await;
                    let removed = auto_messages_before.saturating_sub(auto_messages_after);
                    let status = if result.retries_used > 0 {
                        format!(
                            "Auto-compaction complete: {auto_messages_before} → {auto_messages_after} messages ({removed} removed, {} retries)",
                            result.retries_used
                        )
                    } else {
                        format!(
                            "Auto-compaction complete: {auto_messages_before} → {auto_messages_after} messages ({removed} removed)"
                        )
                    };
                    Engine::emit_compaction_completed(
                        self,
                        compaction_id.clone(),
                        true,
                        status.clone(),
                        Some(auto_messages_before),
                        Some(auto_messages_after),
                    )
                    .await;
                    self.fire_post_compact(
                        self.runtime_ext().turn_app_mode,
                        false,
                        auto_messages_before,
                        auto_messages_after,
                    );
                    if let Some(artifact) = result.artifact {
                        emit_kernel_event(
                            self,
                            KernelEvent::CompactionArtifactCreated {
                                turn_id: turn.id.clone(),
                                artifact_id: artifact.id,
                                replaced_range: MessageRange {
                                    from: artifact.replaced_start as u32,
                                    to: artifact
                                        .replaced_end
                                        .saturating_sub(1)
                                        .max(artifact.replaced_start)
                                        as u32,
                                },
                                summary_token_count: artifact.summary_tokens,
                            },
                        );
                    }
                    let _ = self.tx_event.send(Event::status(status)).await;
                } else {
                    let message = "Auto-compaction skipped: empty result".to_string();
                    Engine::emit_compaction_failed(
                        self,
                        compaction_id.clone(),
                        true,
                        message.clone(),
                    )
                    .await;
                    let _ = self.tx_event.send(Event::status(message)).await;
                }
            }
            Err(err) => {
                let message = format!("Auto-compaction failed: {err}");
                Engine::emit_compaction_failed(self, compaction_id, true, message.clone()).await;
                let _ = self.tx_event.send(Event::status(message)).await;
            }
        }
    }

    fn estimated_input_tokens(&self) -> usize {
        estimate_input_tokens_conservative(
            &self.session.messages,
            self.session.system_prompt.as_ref(),
        )
    }

    async fn flush_pending_lsp_diagnostics(&mut self) {
        Engine::flush_pending_lsp_diagnostics(self).await;
    }

    async fn layered_context_checkpoint(&mut self) {
        Engine::layered_context_checkpoint(self).await;
    }

    fn decorate_auth_error_message(&self, message: String) -> String {
        Engine::decorate_auth_error_message(self, message)
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
        _result: &str,
        success: bool,
    ) {
        if !self.config.long_horizon.enabled {
            return;
        }
        // `success` already encodes exit-0 from the tool layer; do NOT also
        // require the result *text* to contain an "exit code: 0" marker — a
        // successful exec_shell returns raw stdout (e.g. `ok  monkey/lexer …`)
        // with no exit-code line (only failures print one), so that extra check
        // made recording NEVER fire on success and left `recent_verification_cmds`
        // permanently empty → every `[verify:]` item false-mismatched (DEMO5 #2).
        if success
            && matches!(tool_name, "exec_shell" | "run_tests")
            && crate::long_horizon::VERIFICATION_RE.is_match(
                tool_input
                    .get("command")
                    .or_else(|| tool_input.get("cmd"))
                    .and_then(|v| v.as_str())
                    .unwrap_or(""),
            )
        {
            let cmd = tool_input
                .get("command")
                .or_else(|| tool_input.get("cmd"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            self.runtime_ext_mut()
                .long_horizon_state
                .record_verification_exec(cmd);
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

    async fn maybe_lht_pre_request_hooks(&mut self, _mode: TurnLoopMode) {
        if !self.config.long_horizon.enabled {
            return;
        }
        let active = self.estimated_input_tokens() as u64;
        let headroom = crate::core::engine::context::turn_response_headroom_tokens();
        let model = self.session.model.clone();
        let in_band = crate::long_horizon::in_lht_warning_band(active, headroom, &model);
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
        Engine::add_session_message(self, msg).await;
    }

    async fn maybe_continue_at_step_limit(&mut self, _turn: &TurnContext) -> bool {
        // Only long-horizon code tasks convert step-exhaustion into a
        // continuation; everything else terminates at the cap as before.
        if !self.config.long_horizon.enabled || !self.config.task_type.uses_code_tool_surface() {
            return false;
        }
        let plan = self.config_ext().plan_state.lock().await.snapshot();
        let checklist = self.config_ext().todos.lock().await.snapshot();
        let graph = crate::long_horizon::CodeTaskGraph::from_snapshots(&plan, &checklist);
        // Nothing to continue toward: no graph, already complete, or trivial.
        if graph.is_empty() || !graph.incomplete() || graph.is_trivial() {
            return false;
        }
        let open = graph.open_items;
        let text = if self.config.locale_tag.starts_with("zh") {
            format!(
                "已达本轮工具步数上限,但长程任务尚未完成(还剩 {open} 项)。请继续推进未完成的清单项:聚焦下一个 in_progress / pending 项,对声称完成的项用其 `[verify:]` 命令实跑验证,不要重复已完成的工作,也不要在此停下。"
            )
        } else {
            format!(
                "Hit the per-turn tool-step budget, but the long-horizon task is not finished ({open} item(s) left). Keep going on the unfinished checklist: focus the next in_progress / pending item, verify any claimed-done item by actually running its `[verify:]` command, do not repeat completed work, and do not stop here."
            )
        };
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
                "long_horizon.step_limit_continue: {{\"open_items\":{open}}}"
            )))
            .await;
        true
    }

    async fn maybe_continue_after_loop_guard_halt(&mut self, _turn: &TurnContext) -> bool {
        // Only long-horizon code tasks convert a loop-guard halt into a
        // continuation; everything else terminates as before.
        if !self.config.long_horizon.enabled || !self.config.task_type.uses_code_tool_surface() {
            return false;
        }
        let plan = self.config_ext().plan_state.lock().await.snapshot();
        let checklist = self.config_ext().todos.lock().await.snapshot();
        let graph = crate::long_horizon::CodeTaskGraph::from_snapshots(&plan, &checklist);
        if graph.is_empty() || !graph.incomplete() || graph.is_trivial() {
            return false;
        }
        let open = graph.open_items;
        let text = if self.config.locale_tag.starts_with("zh") {
            format!(
                "检测到你在重复调用同一个反复失败的工具,已被循环保护中断。长程任务尚未完成(还剩 {open} 项)。不要再用相同参数重试同一工具——换一种方法:换工具、改参数、或先读取相关文件/错误输出定位根因,然后继续推进未完成的清单项。不要在此停下。"
            )
        } else {
            format!(
                "You got stuck repeatedly calling the same failing tool and the loop guard halted the turn. The long-horizon task is not finished ({open} item(s) left). Do NOT retry the same tool with the same arguments — change approach: switch tools, change the arguments, or read the relevant file / error output to find the root cause first, then keep going on the unfinished checklist. Do not stop here."
            )
        };
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
                "long_horizon.loop_guard_continue: {{\"open_items\":{open}}}"
            )))
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
        // the monitor forwards this as `panel.context`, so the Context tab /
        // cycle-pressure bar update every step instead of freezing until turn
        // end (where the op-loop `QueryContext` finally drains). Same channel as
        // `checklist_persist`. Cheap relative to the per-step token estimate the
        // cycle gate already computes below.
        if let Ok(json) = serde_json::to_string(&self.engine_context_snapshot()) {
            let _ = self
                .tx_event
                .send(Event::status(format!(
                    "long_horizon.context_snapshot:{json}"
                )))
                .await;
        }
        // Reuse the exact between-turns gate (threshold + long-horizon
        // early-advance band) and handoff body. At this call site the streaming
        // phase and tool execution have completed, so `in_flight` is false —
        // a clean per-step boundary with no mid-edit/stream cut.
        Engine::maybe_advance_cycle(self, turn_loop_to_app_mode(mode)).await
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
        let graph = crate::long_horizon::CodeTaskGraph::from_snapshots(&plan, &checklist);
        if graph.is_empty() || !graph.incomplete() || graph.is_trivial() {
            return;
        }
        let open = graph.open_items;
        let _ = self
            .tx_event
            .send(Event::status(format!(
                "long_horizon.incomplete_stop: {{\"open_items\":{open}}}"
            )))
            .await;
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

    async fn maybe_inject_scratchpad_summary(&mut self, turn: &TurnContext) -> bool {
        if self.scratchpad_summary_injected_this_turn {
            return false;
        }
        let Some(summary_msg) = scratchpad_flow::maybe_summary_before_final_answer(
            &self.session.workspace,
            self.scratchpad_run_id.as_deref(),
            &self.config.scratchpad,
        ) else {
            return false;
        };
        Engine::add_session_message(self, summary_msg).await;
        self.scratchpad_summary_injected_this_turn = true;
        emit_kernel_event(
            self,
            KernelEvent::ScratchpadSummaryInjected {
                turn_id: turn.id.clone(),
                at_step: turn.step,
            },
        );
        true
    }

    async fn maybe_inject_scratchpad_reminder(&mut self, turn: &TurnContext) {
        if let Some((reminder, area_path)) = scratchpad_flow::build_readonly_reminder_message(
            &self.session.workspace,
            self.scratchpad_run_id.as_deref(),
            &self.config.scratchpad,
            &self.scratchpad_step,
        ) {
            Engine::add_session_message(self, reminder).await;
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
        use zagens_core::engine::token_estimate::estimate_text_tokens;
        use zagens_core::engine::turn_loop::CompilerRequestContext;

        let mut snapshot =
            crate::context_compiler_shadow::ContextCompilerStateSnapshot::from_session(
                &self.session,
                0,
            );

        // Replace placeholder estimate with the actual serialized tool catalog size.
        if let Some(tools) = active_tools {
            let json = serde_json::to_string(tools).unwrap_or_default();
            snapshot.tool_catalog_est_tokens = estimate_text_tokens(&json) as u32;
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

        let compiler = crate::context_compiler_shadow::build_compiler_from_snapshot(&snapshot);
        let proj = ContextProjection::from_session(&self.session, snapshot.step_idx);

        let compiled = if let Some(budget_cap) = overflow_budget_cap {
            // Applies for exactly one request retry; cap was already consumed above.
            match compiler.compile_with_budget_override(&proj, budget_cap, &[]) {
                Ok(ctx) => ctx,
                Err(_) => compiler.compile(&proj),
            }
        } else {
            compiler.compile(&proj)
        };

        // Determine which sources survived compilation (for eviction-aware assembly).
        let has_compaction = compiled
            .contributions
            .iter()
            .any(|c| c.source_id.0 == "memory.compaction" && c.token_count > 0);
        let has_working_set = compiled
            .contributions
            .iter()
            .any(|c| c.source_id.0 == "working_set" && c.token_count > 0);

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
        tool_registry: Option<&Self::ToolRegistry>,
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

    async fn finish_kernel_turn_shadow(&mut self, live: &LiveTurnSnapshot) {
        let (events, writer, do_shadow) = {
            let ext = self.runtime_ext_mut();
            let do_shadow = ext.kernel_machine_mode.uses_effect_replay_shadow();
            let events = ext.kernel_projection_shadow.turn_events().to_vec();
            let writer = ext.kernel_event_writer.clone();
            if do_shadow {
                ext.kernel_effect_shadow.verify_turn(&events);
                ext.kernel_guard_shadow.verify_turn(&events);
                ext.kernel_memory_shadow.verify_turn(&events);
                ext.kernel_replay_shadow
                    .verify_turn_in_memory(&events, live);
            }
            ext.kernel_projection_shadow.finish_turn(live);
            (events, writer, do_shadow)
        };
        if do_shadow {
            if let Some(writer) = writer {
                self.runtime_ext()
                    .kernel_replay_shadow
                    .verify_turn_persisted(writer.as_ref(), &live.turn_id, &events)
                    .await;
            }
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
