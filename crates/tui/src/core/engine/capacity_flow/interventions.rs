//! Capacity guardrail interventions: context refresh, tool replay, replan.

use std::sync::Arc;

use tokio::sync::{Mutex as AsyncMutex, RwLock};

use crate::compaction::{compact_messages_safe, should_compact};
use crate::mcp::McpPool;
use crate::models::{ContentBlock, Message};
use crate::tui::app::AppMode;

use super::super::tool_catalog::REQUEST_USER_INPUT_NAME;
use super::super::*;

impl Engine {
    pub(in crate::core::engine) async fn apply_targeted_context_refresh(
        &mut self,
        turn: &TurnContext,
        client: Option<&(dyn crate::llm_client::LlmClient)>,
        mode: AppMode,
        snapshot: Option<&CapacitySnapshot>,
    ) -> bool {
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
            )
            .await
            {
                Ok(result) => {
                    if !result.messages.is_empty() || self.session.messages.is_empty() {
                        self.session.messages = result.messages;
                        self.merge_compaction_summary(result.summary_prompt);
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

        let canonical = self.build_canonical_state(turn, None);
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
        self.refresh_system_prompt(mode);
        self.emit_session_updated().await;

        let after_tokens = self.estimated_input_tokens();
        self.emit_capacity_intervention(
            turn,
            GuardrailAction::TargetedContextRefresh,
            before_tokens,
            after_tokens,
            None,
            false,
        )
        .await;
        self.capacity_controller
            .mark_intervention_applied(self.turn_counter, GuardrailAction::TargetedContextRefresh);
        true
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::core::engine) async fn apply_verify_with_tool_replay(
        &mut self,
        turn: &TurnContext,
        mode: AppMode,
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
                self.capacity_controller
                    .mark_replay_failed(self.turn_counter);
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
            self.capacity_controller
                .mark_replay_failed(self.turn_counter);
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
        self.refresh_system_prompt(mode);
        self.emit_session_updated().await;

        let after_tokens = self.estimated_input_tokens();
        self.emit_capacity_intervention(
            turn,
            GuardrailAction::VerifyWithToolReplay,
            before_tokens,
            after_tokens,
            Some(replay_outcome),
            false,
        )
        .await;
        self.capacity_controller
            .mark_intervention_applied(self.turn_counter, GuardrailAction::VerifyWithToolReplay);
        true
    }

    pub(in crate::core::engine) async fn apply_verify_and_replan(
        &mut self,
        turn: &TurnContext,
        mode: AppMode,
        snapshot: Option<&CapacitySnapshot>,
        reason: &str,
    ) -> bool {
        let before_tokens = self.estimated_input_tokens();
        let canonical = self.build_canonical_state(turn, Some(reason));
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
        let latest_verified = self
            .session
            .messages
            .iter()
            .rev()
            .find(|msg| {
                msg.role == "user"
                    && msg.content.iter().any(|block| match block {
                        ContentBlock::ToolResult { content, .. } => {
                            content.contains("[verification replay]")
                        }
                        _ => false,
                    })
            })
            .cloned();

        // Take ownership of the old messages before clearing so a crash
        // during rebuild won't lose the session history (#D3 / H11).
        let _old_messages = std::mem::take(&mut self.session.messages);
        // old_messages is dropped only after the rebuild block completes.
        if let Some(msg) = latest_user {
            self.session.messages.push(msg);
        }
        if let Some(msg) = latest_verified {
            self.session.messages.push(msg);
        }

        self.merge_compaction_summary(Some(self.canonical_prompt(
            &canonical,
            &pointer,
            GuardrailAction::VerifyAndReplan,
            Some("Replan now from canonical state. Keep steps minimal and verifiable."),
        )));
        self.refresh_system_prompt(mode);
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
        )
        .await;
        self.capacity_controller
            .mark_intervention_applied(self.turn_counter, GuardrailAction::VerifyAndReplan);
        true
    }
}
