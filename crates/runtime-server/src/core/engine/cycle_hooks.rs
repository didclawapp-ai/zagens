//! Cycle boundary, system prompt refresh, and compaction summary merge.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::compaction::merge_system_prompts;
use crate::cycle_manager::{
    CycleBriefing, StructuredState, archive_cycle, build_seed_messages, estimate_briefing_tokens,
    produce_briefing, should_advance_cycle,
};
use crate::long_horizon::{
    context_pressure_ratio, in_lht_warning_band, should_lht_early_advance_cycle,
};
use crate::core::events::Event;
use crate::models::SystemPrompt;
use crate::prompts;
use crate::agent_surface::AppMode;

use super::context::turn_response_headroom_tokens;
use super::scratchpad_flow;
use super::Engine;

impl Engine {
    /// Advance checkpoint-restart cycle when input estimate crosses threshold (#124).
    pub(super) async fn maybe_advance_cycle(&mut self, mode: AppMode) {
        let active = self.estimated_input_tokens() as u64;
        let headroom = turn_response_headroom_tokens();
        let model = self.session.model.clone();
        let lht_enabled = self.config.long_horizon.enabled;
        let threshold =
            should_advance_cycle(active, headroom, &model, &self.config.cycle, false);
        let lht_early = {
            let lh = &mut self.runtime_ext_mut().long_horizon_state;
            let pending = lh.pending_cycle_at_checkpoint;
            let early = should_lht_early_advance_cycle(
                active,
                headroom,
                &model,
                lht_enabled,
                pending,
            );
            if early {
                lh.pending_cycle_at_checkpoint = false;
            }
            early
        };
        if !threshold && !lht_early {
            return;
        }

        let reason = if lht_early && !threshold {
            "long-horizon checkpoint"
        } else {
            "context threshold"
        };
        let _ = self.perform_cycle_advance(mode, reason).await;
    }

    /// Force a cycle handoff regardless of the threshold gate. Used as a
    /// last-resort recovery when an in-flight turn's context overflows the
    /// model budget and emergency compaction can't get back under it: instead
    /// of hard-failing the turn (dumping a manual `/compact` on the user),
    /// roll the cycle so the next step starts from a small `<carry_forward>`
    /// briefing seed plus preserved structured state (plan / todos / working
    /// set / handoff.md). Returns `true` when the swap happened (caller can
    /// retry the request) and `false` when the briefing turn failed (caller
    /// falls back to the original hard failure).
    pub(super) async fn force_cycle_handoff_for_overflow(&mut self, mode: AppMode) -> bool {
        self.perform_cycle_advance(mode, "context overflow").await
    }

    /// Body of a cycle advance: produce the model-curated briefing, archive
    /// the outgoing cycle, capture structured state, build the seed messages,
    /// and atomically swap the session buffer. Returns `true` when the swap
    /// completed and `false` when the briefing turn failed (no swap done).
    async fn perform_cycle_advance(&mut self, mode: AppMode, reason: &str) -> bool {
        let lht_enabled = self.config.long_horizon.enabled;
        let Some(client) = self.deepseek_client.clone() else {
            crate::logging::warn(
                "Cycle boundary skipped: API client not configured for briefing turn",
            );
            return false;
        };

        let from = self.session.cycle_count;
        let to = from.saturating_add(1);
        let archive_started = self.session.current_cycle_started;
        let max_briefing_tokens = self.config.cycle.briefing_max_for(&self.session.model);

        let _ = self
            .tx_event
            .send(Event::status(format!(
                "↻ context refreshing (cycle {from} → {to}, {reason}, generating briefing…)"
            )))
            .await;

        // 1. Generate the model-curated briefing. Prefer the Flash seam
        //    manager (#159) for cost and speed; fall back to the main model
        //    (legacy produce_briefing) when the seam manager isn't available.
        //
        // M5: calls go through the `SeamHost` trait — same call shape,
        // explicit UFCS to keep the trait import obvious at the seam.
        let briefing_text = if let Some(ref seam_mgr) = self.seam {
            use deepseek_core::engine::hosts::SeamHost;
            let seams = SeamHost::collect_seam_texts(seam_mgr.as_ref(), &self.session.messages).await;
            let state_text = {
                let s = StructuredState::capture(
                    mode.label(),
                    self.config.workspace.clone(),
                    std::env::current_dir().ok(),
                    &self.session.working_set,
                    &self.config_ext().todos,
                    &self.config_ext().plan_state,
                    Some(&self.runtime_ext().subagent_manager),
                )
                .await;
                s.to_system_block()
            };
            match SeamHost::produce_flash_briefing(seam_mgr.as_ref(), &seams, state_text.as_deref()).await {
                Ok(text) => text,
                Err(err) => {
                    crate::logging::warn(format!(
                        "Flash briefing failed, falling back to main model: {err}"
                    ));
                    match produce_briefing(
                        client.as_ref(),
                        &self.session.model,
                        &self.session.messages,
                        max_briefing_tokens,
                    )
                    .await
                    {
                        Ok(text) => text,
                        Err(err2) => {
                            crate::logging::warn(format!(
                                "Cycle briefing turn failed; skipping cycle advance: {err2}"
                            ));
                            let _ = self
                                .tx_event
                                .send(Event::status(format!(
                                    "鈫?cycle handoff failed (continuing in cycle {from}): {err2}"
                                )))
                                .await;
                            return false;
                        }
                    }
                }
            }
        } else {
            match produce_briefing(
                client.as_ref(),
                &self.session.model,
                &self.session.messages,
                max_briefing_tokens,
            )
            .await
            {
                Ok(text) => text,
                Err(err) => {
                    crate::logging::warn(format!(
                        "Cycle briefing turn failed; skipping cycle advance: {err}"
                    ));
                    let _ = self
                        .tx_event
                        .send(Event::status(format!(
                            "鈫?cycle handoff failed (continuing in cycle {from}): {err}"
                        )))
                        .await;
                    return false;
                }
            }
        };

        let briefing_tokens = estimate_briefing_tokens(&briefing_text);
        let now = chrono::Utc::now();
        let briefing = CycleBriefing {
            cycle: to,
            timestamp: now,
            briefing_text: briefing_text.clone(),
            token_estimate: briefing_tokens,
        };

        // 2. Archive the cycle to disk. If the archive write fails we still
        //    proceed with the swap 鈥?the briefing alone preserves enough
        //    state to continue, and the user can recover the lost archive
        //    from their session log if needed.
        match archive_cycle(
            &self.session.id,
            to,
            &self.session.messages,
            &self.session.model,
            archive_started,
        ) {
            Ok(path) => {
                crate::logging::info(format!("Cycle {to} archived to {}", path.display()));
            }
            Err(err) => {
                crate::logging::warn(format!(
                    "Failed to archive cycle {to}; continuing with swap: {err}"
                ));
            }
        }

        // 3. Capture structured state. Locks are held only for the snapshot.
        let state = StructuredState::capture(
            mode.label(),
            self.config.workspace.clone(),
            std::env::current_dir().ok(),
            &self.session.working_set,
            &self.config_ext().todos,
            &self.config_ext().plan_state,
            Some(&self.runtime_ext().subagent_manager),
        )
        .await;
        let mut state_block = state.to_system_block();
        if let Some(line) = scratchpad_flow::scratchpad_handoff_line(
            &self.session.workspace,
            self.scratchpad_run_id.as_deref(),
        ) {
            state_block = Some(match state_block {
                Some(existing) => format!("{existing}\n\n{line}"),
                None => line,
            });
        }

        // 4. Build the seed messages. The next cycle starts with the
        //    base system prompt (refreshed below) and these seeds.
        let seed_messages = build_seed_messages(
            state_block.as_deref(),
            Some(&briefing),
            None, // pending_user_message 鈥?pulled from steer/queue elsewhere
        );

        // 5. Atomic swap.
        self.session.messages = seed_messages;
        self.session.cycle_count = to;
        self.session.current_cycle_started = now;
        self.session.cycle_briefings.push(briefing.clone());
        // Reset seam tracking for the new cycle. M5: trait dispatch.
        if let Some(ref seam_mgr) = self.seam {
            use deepseek_core::engine::hosts::SeamHost;
            SeamHost::reset(seam_mgr.as_ref()).await;
        }
        // Drop any compaction summary 鈥?that path is incompatible with the
        // fresh-context model and would Frankenstein-merge with the briefing.
        self.session.compaction_summary_prompt = None;
        self.refresh_system_prompt(mode);
        self.emit_session_updated().await;

        let _ = self
            .tx_event
            .send(Event::CycleAdvanced {
                from,
                to,
                briefing: briefing.clone(),
            })
            .await;
        let _ = self
            .tx_event
            .send(Event::status(format!(
                "鈫?context refreshed (cycle {from} 鈫?{to}, briefing: {briefing_tokens} tokens carried)"
            )))
            .await;

        if lht_enabled {
            let plan = self.config_ext().plan_state.lock().await.snapshot();
            let checklist = self.config_ext().todos.lock().await.snapshot();
            if let Some(section) =
                crate::long_horizon::build_lht_handoff_section(to, &plan, &checklist)
            {
                let workspace = self.session.workspace.clone();
                let section_owned = section;
                if let Ok(Err(io_err)) = tokio::task::spawn_blocking(move || {
                    crate::long_horizon::merge_lht_into_handoff(&workspace, &section_owned)
                })
                .await
                {
                    crate::logging::warn(format!("LHT handoff block write failed: {io_err}"));
                }
            }
        }

        true
    }

    /// Refresh the system prompt based on current mode and context.
    pub(super) fn refresh_system_prompt(&mut self, mode: AppMode) {
        self.refresh_system_prompt_with_arbitration(
            mode,
            crate::topic_memory::PromptInjectionArbitration::none(),
        );
    }

    /// Refresh the system prompt, optionally omitting lower-priority injections (B2.1).
    pub(super) fn refresh_system_prompt_with_arbitration(
        &mut self,
        mode: AppMode,
        arbitration: crate::topic_memory::PromptInjectionArbitration,
    ) {
        let user_memory_block = if arbitration.omit_user_memory {
            None
        } else {
            crate::memory::compose_block(self.config.memory_enabled, &self.config.memory_path)
        };
        let query_hint =
            crate::topic_memory::last_user_query_from_messages(&self.session.messages);
        let topic_memory_block = if arbitration.omit_topic_memory {
            None
        } else {
            // M5: dispatch through TopicMemoryHost — runtime owns its
            // settings (set at Engine::new from config.topic_memory).
            use deepseek_core::engine::hosts::TopicMemoryHost;
            TopicMemoryHost::compose_block(&mut *self.topic_memory, query_hint.as_deref())
        };
        let base = prompts::system_prompt_for_mode_with_context_skills_session_and_approval(
            mode,
            &self.config.workspace,
            None,
            Some(&self.config.skills_dir),
            Some(&self.config.instructions),
                prompts::PromptSessionContext {
                    user_memory_block: user_memory_block.as_deref(),
                    topic_memory_block: topic_memory_block.as_deref(),
                    goal_objective: self.config.goal_objective.as_deref(),
                    locale_tag: &self.config.locale_tag,
                    task_type: self.config.task_type,
                },
            self.session.approval_mode,
        );
        let stable_prompt =
            merge_system_prompts(Some(&base), self.session.compaction_summary_prompt.clone());
        let stable_hash = system_prompt_hash(stable_prompt.as_ref());
        if self.session.last_system_prompt_hash != Some(stable_hash) {
            self.session.system_prompt = stable_prompt;
            self.session.last_system_prompt_hash = Some(stable_hash);
        }
    }

    pub(super) fn merge_compaction_summary(&mut self, summary_prompt: Option<SystemPrompt>) {
        if let Some(prompt) = summary_prompt {
            self.session.compaction_summary_prompt = merge_system_prompts(
                self.session.compaction_summary_prompt.as_ref(),
                Some(prompt.clone()),
            );
            let merged = merge_system_prompts(self.session.system_prompt.as_ref(), Some(prompt));
            self.session.last_system_prompt_hash = Some(system_prompt_hash(merged.as_ref()));
            self.session.system_prompt = merged;
        }

        // C0: keep scratchpad L0 pointer in compaction summary (not full P2 layered text).
        if let Some(scratchpad_l0) = scratchpad_flow::scratchpad_compaction_system_prompt(
            &self.session.workspace,
            self.scratchpad_run_id.as_deref(),
            &self.config.scratchpad,
        ) {
            self.session.compaction_summary_prompt = merge_system_prompts(
                self.session.compaction_summary_prompt.as_ref(),
                Some(scratchpad_l0.clone()),
            );
            let merged =
                merge_system_prompts(self.session.system_prompt.as_ref(), Some(scratchpad_l0));
            self.session.last_system_prompt_hash = Some(system_prompt_hash(merged.as_ref()));
            self.session.system_prompt = merged;
        }
    }
}

pub(super) fn system_prompt_hash(prompt: Option<&SystemPrompt>) -> u64 {
    let mut hasher = DefaultHasher::new();
    match prompt {
        Some(SystemPrompt::Text(text)) => {
            0u8.hash(&mut hasher);
            text.hash(&mut hasher);
        }
        Some(SystemPrompt::Blocks(blocks)) => {
            1u8.hash(&mut hasher);
            for block in blocks {
                block.block_type.hash(&mut hasher);
                block.text.hash(&mut hasher);
                if let Some(cache_control) = &block.cache_control {
                    cache_control.cache_type.hash(&mut hasher);
                }
            }
        }
        None => {
            2u8.hash(&mut hasher);
        }
    }
    hasher.finish()
}

