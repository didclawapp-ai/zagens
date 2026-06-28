//! Capacity guardrail interventions: context refresh, tool replay, replan.

use std::sync::Arc;

use tokio::sync::{Mutex as AsyncMutex, RwLock};

use crate::compaction::{compact_messages_safe, should_compact};
use crate::mcp::McpPool;
use crate::models::{ContentBlock, Message};
use zagens_core::context_profile::{auto_compaction_allowed, is_large_context_profile};
use zagens_core::turn::TurnLoopMode;

use super::super::tool_catalog::REQUEST_USER_INPUT_NAME;
use super::super::turn_loop::host_impl::turn_loop_to_app_mode;
use super::super::*;
use super::refresh_system_prompt_for_turn_mode_under_capacity;

impl Engine {
    fn log_large_profile_capacity_fallback(step: u32, action: &str) {
        tracing::info!(
            target: "context_profile",
            step,
            action,
            "Large profile TargetedContextRefresh fallback"
        );
    }

    async fn finish_targeted_context_refresh(
        &mut self,
        turn: &TurnContext,
        mode: TurnLoopMode,
        snapshot: Option<&CapacitySnapshot>,
        before_tokens: usize,
        fallback_note: Option<&str>,
    ) -> bool {
        let canonical = self.build_canonical_state(turn, fallback_note);
        let source_message_ids = self.capacity_source_message_ids(turn);
        let record = self.build_capacity_record(
            turn,
            GuardrailAction::TargetedContextRefresh,
            snapshot,
            canonical.clone(),
            source_message_ids,
            None,
        );
        let pointer = self
            .persist_capacity_record(turn, GuardrailAction::TargetedContextRefresh, &record)
            .await;
        self.merge_compaction_summary(Some(self.canonical_prompt(
            &canonical,
            &pointer,
            GuardrailAction::TargetedContextRefresh,
            None,
        )));
        refresh_system_prompt_for_turn_mode_under_capacity(self, mode);
        self.emit_session_updated().await;

        let after_tokens = self.estimated_input_tokens();
        self.emit_capacity_intervention(
            turn,
            GuardrailAction::TargetedContextRefresh,
            before_tokens,
            after_tokens,
            fallback_note.map(str::to_string),
            false,
            fallback_note.map(|note| vec![note.to_string()]),
        )
        .await;
        self.0.capacity_controller.mark_intervention_applied(
            self.0.turn_counter,
            GuardrailAction::TargetedContextRefresh,
        );
        true
    }

    async fn apply_large_profile_targeted_context_refresh(
        &mut self,
        turn: &TurnContext,
        client: Option<&dyn crate::llm_client::LlmClient>,
        mode: TurnLoopMode,
        snapshot: Option<&CapacitySnapshot>,
    ) -> bool {
        let before_tokens = self.estimated_input_tokens();
        let target_budget = context_input_budget(&self.session.model, TURN_MAX_OUTPUT_TOKENS)
            .unwrap_or(self.config.compaction.token_threshold.max(1));

        // 1. Prefer a clean cycle handoff over destructive compaction.
        // Call `perform_cycle_advance` directly — not `force_cycle_handoff_for_overflow` —
        // because this path often runs inside v3 `RunCompaction(CapacityTrim)` and routing
        // through the effect interpreter would recurse.
        Self::log_large_profile_capacity_fallback(1, "force_cycle_handoff");
        if self
            .perform_cycle_advance(turn_loop_to_app_mode(mode), "capacity targeted refresh")
            .await
        {
            let after_tokens = self.estimated_input_tokens();
            self.emit_capacity_intervention(
                turn,
                GuardrailAction::TargetedContextRefresh,
                before_tokens,
                after_tokens,
                Some("force_cycle_handoff".to_string()),
                false,
                Some(vec!["force_cycle_handoff".to_string()]),
            )
            .await;
            self.0.capacity_controller.mark_intervention_applied(
                self.0.turn_counter,
                GuardrailAction::TargetedContextRefresh,
            );
            return true;
        }

        // 2. Soft seam checkpoint when layered context is enabled (P0: usually skipped).
        if self.seam.is_some() {
            use zagens_core::engine::hosts::SeamHost;
            if SeamHost::config_enabled(self.seam.as_ref().unwrap().as_ref()) {
                Self::log_large_profile_capacity_fallback(2, "seam_checkpoint");
                self.run_layered_context_checkpoint_effect().await;
                if self.estimated_input_tokens() <= target_budget {
                    return self
                        .finish_targeted_context_refresh(
                            turn,
                            mode,
                            snapshot,
                            before_tokens,
                            Some("seam_checkpoint"),
                        )
                        .await;
                }
            }
        }

        let mut refreshed = false;

        // 3. Overflow-style forced compaction (bypasses auto_compaction_allowed).
        if self.estimated_input_tokens() > target_budget {
            Self::log_large_profile_capacity_fallback(3, "forced_compaction");
            if let Some(client) = client {
                let mut forced_config = self.config.compaction.clone();
                forced_config.enabled = true;
                forced_config.token_threshold = forced_config
                    .token_threshold
                    .min(target_budget.saturating_sub(1))
                    .max(1);
                forced_config.auto_floor_tokens = 0;

                match compact_messages_safe(
                    client,
                    &self.session.messages,
                    &forced_config,
                    Some(&self.session.workspace),
                    None,
                    None,
                    false,
                )
                .await
                {
                    Ok(result) => {
                        if !result.messages.is_empty() || self.session.messages.is_empty() {
                            self.session.messages = result.messages;
                            self.apply_compaction_result(
                                result.summary_prompt,
                                result.summary_message,
                            );
                            refreshed = true;
                        }
                    }
                    Err(err) => {
                        let _ = self
                            .tx_event
                            .send(Event::status(format!(
                                "Large profile forced compaction failed: {err}. Falling back to local trim."
                            )))
                            .await;
                    }
                }
            }
        }

        // 4. Last-resort trim.
        if !refreshed && self.estimated_input_tokens() > target_budget {
            Self::log_large_profile_capacity_fallback(4, "trim_oldest_messages");
            let trimmed = self.trim_oldest_messages_to_budget(target_budget);
            refreshed = trimmed > 0;
            if trimmed > 0 {
                let _ = self
                    .tx_event
                    .send(Event::status(format!(
                        "Large profile capacity trim: removed {trimmed} oldest message(s)"
                    )))
                    .await;
            }
        }

        if !refreshed {
            return false;
        }

        self.finish_targeted_context_refresh(
            turn,
            mode,
            snapshot,
            before_tokens,
            Some("forced_compaction_or_trim"),
        )
        .await
    }

    pub(in crate::core::engine) async fn apply_targeted_context_refresh(
        &mut self,
        turn: &TurnContext,
        client: Option<&dyn crate::llm_client::LlmClient>,
        mode: TurnLoopMode,
        snapshot: Option<&CapacitySnapshot>,
    ) -> bool {
        if is_large_context_profile(&self.session.model) {
            return self
                .apply_large_profile_targeted_context_refresh(turn, client, mode, snapshot)
                .await;
        }

        let before_tokens = self.estimated_input_tokens();
        let compaction_pins = self
            .session
            .working_set
            .pinned_message_indices(&self.session.messages, &self.session.workspace);
        let mut compaction_paths = self.session.working_set.top_paths(24);
        crate::core::engine::scratchpad_flow::extend_compaction_paths(
            &self.session.workspace,
            self.scratchpad_run_id.as_deref(),
            &mut compaction_paths,
        );

        let mut refreshed = false;
        let should_run_summary_compaction = self.config.compaction.enabled
            && auto_compaction_allowed(&self.session.model, &self.config.cycle)
            && should_compact(
                &self.session.messages,
                &self.config.compaction,
                Some(&self.session.workspace),
                Some(&compaction_pins),
                Some(&compaction_paths),
            );
        if should_run_summary_compaction && let Some(client) = client {
            match compact_messages_safe(
                client,
                &self.session.messages,
                &self.config.compaction,
                Some(&self.session.workspace),
                Some(&compaction_pins),
                Some(&compaction_paths),
                false,
            )
            .await
            {
                Ok(result) => {
                    if !result.messages.is_empty() || self.session.messages.is_empty() {
                        self.session.messages = result.messages;
                        self.apply_compaction_result(result.summary_prompt, result.summary_message);
                        refreshed = true;
                    }
                }
                Err(err) => {
                    let _ = self
                        .tx_event
                        .send(Event::status(format!(
                            "Capacity refresh compaction failed: {err}. Falling back to local trim."
                        )))
                        .await;
                }
            }
        }

        if !refreshed {
            let target_budget = context_input_budget(&self.session.model, TURN_MAX_OUTPUT_TOKENS)
                .unwrap_or(self.config.compaction.token_threshold.max(1));
            if self.estimated_input_tokens() > target_budget {
                let trimmed = self.trim_oldest_messages_to_budget(target_budget);
                refreshed = trimmed > 0;
            }
        }

        if !refreshed {
            return false;
        }

        self.finish_targeted_context_refresh(turn, mode, snapshot, before_tokens, None)
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::core::engine) async fn apply_verify_with_tool_replay(
        &mut self,
        turn: &TurnContext,
        mode: TurnLoopMode,
        snapshot: Option<&CapacitySnapshot>,
        tool_registry: Option<&crate::tools::ToolRegistry>,
        tool_exec_lock: Arc<RwLock<()>>,
        mut mcp_pool: Option<Arc<AsyncMutex<McpPool>>>,
    ) -> bool {
        let before_tokens = self.estimated_input_tokens();
        let Some(candidate) = self.select_replay_candidate(turn, tool_registry) else {
            return false;
        };

        if McpPool::is_mcp_tool(&candidate.name) && mcp_pool.is_none() {
            mcp_pool = self.ensure_mcp_pool().await.ok();
        }

        let supports_parallel = if McpPool::is_mcp_tool(&candidate.name) {
            mcp_tool_is_parallel_safe(&candidate.name)
        } else {
            tool_registry
                .and_then(|registry| registry.get(&candidate.name))
                .is_some_and(|spec| spec.supports_parallel())
        };
        let interactive = (candidate.name == "exec_shell"
            && candidate
                .input
                .get("interactive")
                .and_then(serde_json::Value::as_bool)
                == Some(true))
            || candidate.name == REQUEST_USER_INPUT_NAME;

        let replay_result = Self::execute_tool_with_lock(
            tool_exec_lock,
            supports_parallel,
            interactive,
            self.tx_event.clone(),
            candidate.name.clone(),
            candidate.input.clone(),
            tool_registry,
            mcp_pool.clone(),
            None,
            None,
            None,
        )
        .await;

        let (pass, replay_outcome, diff_summary) = match replay_result {
            Ok(output) => {
                let original = candidate.result.as_deref().unwrap_or_default();
                let replay = output.content.as_str();
                let equal = original.trim() == replay.trim();
                let diff = if equal {
                    "output_match".to_string()
                } else {
                    format!(
                        "output_mismatch: original='{}' replay='{}'",
                        summarize_text(original, 140),
                        summarize_text(replay, 140)
                    )
                };
                (
                    equal,
                    if equal {
                        "pass".to_string()
                    } else {
                        "conflict".to_string()
                    },
                    diff,
                )
            }
            Err(err) => {
                self.0
                    .capacity_controller
                    .mark_replay_failed(self.0.turn_counter);
                (
                    false,
                    "error".to_string(),
                    format!("replay_error: {}", summarize_text(&err.to_string(), 180)),
                )
            }
        };

        let verification_note = format!(
            "[verification replay] tool={} pass={} details={}",
            candidate.name, pass, diff_summary
        );
        self.add_session_message(Message {
            role: "user".to_string(),
            content: vec![ContentBlock::ToolResult {
                tool_use_id: candidate.id.clone(),
                content: verification_note.clone(),
                is_error: None,
                content_blocks: None,
            }],
        })
        .await;

        if !pass {
            self.0
                .capacity_controller
                .mark_replay_failed(self.0.turn_counter);
        }

        let canonical = self.build_canonical_state(
            turn,
            Some(if pass {
                "replay verification passed"
            } else {
                "replay verification failed or conflicted"
            }),
        );
        let replay_info = Some(ReplayInfo {
            tool_id: candidate.id.clone(),
            tool_name: candidate.name.clone(),
            pass,
            diff_summary: diff_summary.clone(),
        });
        let source_message_ids = self.capacity_source_message_ids(turn);
        let record = self.build_capacity_record(
            turn,
            GuardrailAction::VerifyWithToolReplay,
            snapshot,
            canonical.clone(),
            source_message_ids,
            replay_info,
        );
        let pointer = self
            .persist_capacity_record(turn, GuardrailAction::VerifyWithToolReplay, &record)
            .await;
        self.merge_compaction_summary(Some(self.canonical_prompt(
            &canonical,
            &pointer,
            GuardrailAction::VerifyWithToolReplay,
            Some(&verification_note),
        )));
        refresh_system_prompt_for_turn_mode_under_capacity(self, mode);
        self.emit_session_updated().await;

        let after_tokens = self.estimated_input_tokens();
        self.emit_capacity_intervention(
            turn,
            GuardrailAction::VerifyWithToolReplay,
            before_tokens,
            after_tokens,
            Some(replay_outcome),
            false,
            None,
        )
        .await;
        self.0
            .capacity_controller
            .mark_intervention_applied(self.0.turn_counter, GuardrailAction::VerifyWithToolReplay);
        false
    }

    pub(in crate::core::engine) async fn apply_verify_and_replan(
        &mut self,
        turn: &TurnContext,
        mode: TurnLoopMode,
        snapshot: Option<&CapacitySnapshot>,
        reason: &str,
    ) -> bool {
        let before_tokens = self.estimated_input_tokens();
        let mut fallback_chain = vec!["verify_and_replan".to_string()];

        // P4-1: prefer cycle handoff over destructive canonical replan.
        // Use `perform_cycle_advance` directly (not `force_cycle_handoff_for_overflow`)
        // to avoid v3 effect-planner recursion when invoked from RunCompaction handoff.
        fallback_chain.push("cycle_handoff".to_string());
        if self
            .perform_cycle_advance(
                super::super::turn_loop::host_impl::turn_loop_to_app_mode(mode),
                reason,
            )
            .await
        {
            fallback_chain.push("cycle_handoff_ok".to_string());
            let after_tokens = self.estimated_input_tokens();
            self.emit_capacity_intervention(
                turn,
                GuardrailAction::VerifyAndReplan,
                before_tokens,
                after_tokens,
                None,
                false,
                Some(fallback_chain),
            )
            .await;
            self.0
                .capacity_controller
                .mark_intervention_applied(self.0.turn_counter, GuardrailAction::VerifyAndReplan);
            return true;
        }
        fallback_chain.push("canonical_replan".to_string());

        let plan = self.config_ext().plan_state.lock().await.snapshot();
        let checklist = self.config_ext().todos.lock().await.snapshot();
        let canonical = self.build_canonical_state_enriched(turn, Some(reason), &plan, &checklist);
        let source_message_ids = self.capacity_source_message_ids(turn);
        let record = self.build_capacity_record(
            turn,
            GuardrailAction::VerifyAndReplan,
            snapshot,
            canonical.clone(),
            source_message_ids,
            None,
        );
        let pointer = self
            .persist_capacity_record(turn, GuardrailAction::VerifyAndReplan, &record)
            .await;

        let latest_user = self
            .session
            .messages
            .iter()
            .rev()
            .find(|msg| {
                msg.role == "user"
                    && msg
                        .content
                        .iter()
                        .any(|block| matches!(block, ContentBlock::Text { .. }))
            })
            .cloned();

        // Take ownership of the old messages before clearing so a crash
        // during rebuild won't lose the session history (#D3 / H11).
        let _old_messages = std::mem::take(&mut self.session.messages);
        // P4-2: keep the last user turn only; plan/todos/working_set live in canonical state.
        if let Some(msg) = latest_user {
            self.session.messages.push(msg);
        }

        self.merge_compaction_summary(Some(self.canonical_prompt(
            &canonical,
            &pointer,
            GuardrailAction::VerifyAndReplan,
            Some("Replan now from canonical state. Keep steps minimal and verifiable."),
        )));
        refresh_system_prompt_for_turn_mode_under_capacity(self, mode);
        self.emit_session_updated().await;

        let _ = self
            .tx_event
            .send(Event::status(
                "Capacity guardrail: context reset to canonical state; replanning step."
                    .to_string(),
            ))
            .await;

        let after_tokens = self.estimated_input_tokens();
        self.emit_capacity_intervention(
            turn,
            GuardrailAction::VerifyAndReplan,
            before_tokens,
            after_tokens,
            None,
            true,
            Some(fallback_chain),
        )
        .await;
        self.0
            .capacity_controller
            .mark_intervention_applied(self.0.turn_counter, GuardrailAction::VerifyAndReplan);
        true
    }
}
