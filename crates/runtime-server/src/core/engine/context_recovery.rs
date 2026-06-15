//! RLM turns and emergency context recovery / token budgeting.

use super::context_trim::trim_messages_partition_aware;

use super::*;

impl Engine {
    /// Handle a Recursive Language Model (RLM) query — Algorithm 1 from
    /// Zhang et al. (arXiv:2512.24601).
    ///
    /// The prompt is stored as PROMPT in a REPL variable. The root LLM
    /// only sees metadata about the REPL state, never the prompt text
    /// directly. The model generates Python code, which is executed by
    /// the REPL. When FINAL() is called, the loop ends.
    pub(super) async fn handle_rlm(
        &mut self,
        content: String,
        model: String,
        child_model: String,
        max_depth: u32,
    ) {
        use crate::rlm::turn::run_rlm_turn;

        let Some(client) = self.deepseek_client.clone() else {
            let err = self
                .deepseek_client_error
                .as_deref()
                .map(|s| s.to_string())
                .unwrap_or_else(|| "API client not configured".to_string());
            let _ = self
                .tx_event
                .send(Event::error(ErrorEnvelope::fatal_auth(format!(
                    "RLM error: {err}"
                ))))
                .await;
            return;
        };

        let _ = self
            .tx_event
            .send(Event::status("RLM turn started".to_string()))
            .await;

        let result = run_rlm_turn(
            client.clone(),
            model,
            content,
            child_model,
            self.tx_event.clone(),
            max_depth,
        )
        .await;

        let has_error = result.error.is_some();
        if let Some(ref err) = result.error {
            let _ = self
                .tx_event
                .send(Event::error(ErrorEnvelope::tool(format!(
                    "RLM error: {err}"
                ))))
                .await;
        }

        if !result.answer.is_empty() {
            // Add the final answer as an assistant message in the session.
            self.add_session_message(crate::models::Message {
                role: "assistant".to_string(),
                content: vec![crate::models::ContentBlock::Text {
                    text: result.answer.clone(),
                    cache_control: None,
                }],
            })
            .await;

            let _ = self
                .tx_event
                .send(Event::MessageDelta {
                    index: 0,
                    content: result.answer.clone(),
                })
                .await;
            let _ = self
                .tx_event
                .send(Event::MessageComplete { index: 0 })
                .await;
        }

        let _ = self
            .tx_event
            .send(Event::TurnComplete {
                usage: result.usage,
                last_request_input_tokens: self.session.last_api_input_tokens,
                status: if has_error {
                    crate::core::events::TurnOutcomeStatus::Failed
                } else {
                    crate::core::events::TurnOutcomeStatus::Completed
                },
                error: result.error.clone(),
                step_count: 0,
                tool_names: vec![],
                end_reason: result.error,
            })
            .await;
    }

    pub(super) fn estimated_input_tokens(&self) -> usize {
        estimate_input_tokens_conservative(
            &self.session.messages,
            self.session.system_prompt.as_ref(),
        )
    }

    pub(super) fn trim_oldest_messages_to_budget(&mut self, target_input_budget: usize) -> usize {
        trim_messages_partition_aware(
            &mut self.0.session.messages,
            self.0.session.system_prompt.as_ref(),
            target_input_budget,
            &self.0.session.workspace,
            &self.0.session.working_set,
            self.0.scratchpad_run_id.as_deref(),
        )
    }

    /// Attempt to reduce the context footprint via the `ContextCompiler` budget
    /// solver before falling back to LLM-backed compaction.
    ///
    /// **P2-D caller contract:** callers invoke this before the compaction /
    /// partition-drain path.  When the compiler has sources registered and the
    /// solver successfully evicts Volatile or shrinks SemiStatic Elastic sources
    /// to fit within `total_budget`, returns `Some(compiled)` with
    /// `overflow_recovered = true`.  The caller MUST set
    /// `self.0.overflow_source_budget_cap` to the resulting source-token total
    /// so that the next `compiler_request_context` call applies the eviction.
    ///
    /// Returns `None` in any of these cases:
    /// - `context.compiler = "legacy"` (kill-switch active)
    /// - compiler has no registered sources
    /// - eviction alone is not enough (`Err(CompileError::Overflow)`) — caller
    ///   should fall through to the LLM compaction path
    /// - message tokens alone already exceed `total_budget`
    pub(super) fn try_budget_recompile(
        &self,
        total_budget: u32,
    ) -> Option<zagens_core::engine::CompiledContext> {
        use zagens_core::engine::{
            ContextCompilerMode, ContextProjection, estimate_input_tokens_conservative,
        };

        let Ok(config) = crate::config::Config::load(None, None) else {
            return None;
        };
        if config.context_compiler_mode() == ContextCompilerMode::Legacy {
            return None;
        }

        let snapshot = crate::context_compiler_shadow::ContextCompilerStateSnapshot::from_session(
            &self.session,
            0,
        );
        let compiler = crate::context_compiler_shadow::build_compiler_from_snapshot(&snapshot);
        if compiler.source_count() == 0 {
            return None;
        }

        // Estimate message-only tokens (system prompt is a compiler source, not messages).
        let message_tokens =
            estimate_input_tokens_conservative(&self.session.messages, None) as u32;
        // If messages alone already exceed the total budget, source eviction won't help.
        let source_budget = total_budget.checked_sub(message_tokens)?;

        let proj = ContextProjection::from_session(&self.session, 0);
        match compiler.compile_with_budget_override(&proj, source_budget, &[]) {
            Ok(ctx) if ctx.overflow_recovered => {
                tracing::debug!(
                    target = "context_compiler",
                    source_tokens = ctx.total_tokens,
                    source_budget,
                    message_tokens,
                    "budget solver evicted volatile sources to fit context window"
                );
                Some(ctx)
            }
            // No eviction needed at source level, or even max eviction is insufficient.
            Ok(_) | Err(_) => None,
        }
    }

    pub(super) async fn recover_context_overflow(
        &mut self,
        client: &dyn crate::llm_client::LlmClient,
        reason: &str,
        requested_output_tokens: u32,
    ) -> bool {
        let Some(target_budget) =
            context_input_budget(&self.session.model, requested_output_tokens)
        else {
            return false;
        };

        // P2-D: attempt compiler budget solver first.  When sources are
        // registered and eviction is sufficient, set the budget cap so that the
        // next `compiler_request_context` call applies the eviction and return
        // immediately (skip LLM compaction entirely for this overflow).
        if let Some(recompiled) = self.try_budget_recompile(target_budget as u32) {
            // Store the source-token total as the budget cap for the next request.
            self.0.overflow_source_budget_cap = Some(recompiled.total_tokens);
            return true;
        }

        let id = format!("compact_{}", &uuid::Uuid::new_v4().to_string()[..8]);
        let start_message = format!("Emergency context compaction started ({reason})");
        self.emit_compaction_started(id.clone(), true, start_message)
            .await;

        let before_tokens = self.estimated_input_tokens();
        let before_count = self.session.messages.len();

        let mut retries_used = 0u32;
        let mut summary_prompt = None;
        let mut compacted_messages = self.session.messages.clone();

        let mut forced_config = self.config.compaction.clone();
        forced_config.enabled = true;
        forced_config.token_threshold = forced_config
            .token_threshold
            .min(target_budget.saturating_sub(1))
            .max(1);
        // v0.8.11: forced compaction (capacity guardrail) bypasses the floor
        // because we're at a hard ceiling and have to free budget regardless
        // of cache cost.
        forced_config.auto_floor_tokens = 0;

        match compact_messages_safe(
            client,
            &self.session.messages,
            &forced_config,
            Some(&self.session.workspace),
            None,
            None,
        )
        .await
        {
            Ok(result) => {
                retries_used = result.retries_used;
                compacted_messages = result.messages;
                summary_prompt = result.summary_prompt;
            }
            Err(err) => {
                let _ = self
                    .tx_event
                    .send(Event::status(format!(
                        "Emergency compaction API pass failed: {err}. Falling back to local trim."
                    )))
                    .await;
            }
        }

        if !compacted_messages.is_empty() || self.session.messages.is_empty() {
            self.session.messages = compacted_messages;
        }
        self.merge_compaction_summary(summary_prompt);

        let trimmed = self.trim_oldest_messages_to_budget(target_budget);
        self.emit_session_updated().await;
        let after_tokens = self.estimated_input_tokens();
        let after_count = self.session.messages.len();
        let recovered = after_tokens <= target_budget
            && (after_tokens < before_tokens || after_count < before_count || trimmed > 0);

        if recovered {
            let removed = before_count.saturating_sub(after_count);
            let mut details = format!(
                "Emergency compaction complete: {before_count} → {after_count} messages ({removed} removed), ~{before_tokens} → ~{after_tokens} tokens"
            );
            if retries_used > 0 {
                details.push_str(&format!(" ({} retries)", retries_used));
            }
            if trimmed > 0 {
                details.push_str(&format!(", trimmed {trimmed} oldest"));
            }
            self.emit_compaction_completed(
                id,
                true,
                details.clone(),
                Some(before_count),
                Some(after_count),
            )
            .await;
            let _ = self.tx_event.send(Event::status(details)).await;
            return true;
        }

        let message = format!(
            "Emergency context compaction failed to reduce request below model limit \
             (estimate ~{} tokens, budget ~{}).",
            after_tokens, target_budget
        );
        self.emit_compaction_failed(id, true, message.clone()).await;
        let _ = self.tx_event.send(Event::status(message)).await;
        false
    }
}
