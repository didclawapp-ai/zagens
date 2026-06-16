//! Pre-request layered context (Flash seam) checkpoint (#159).

use super::effect_interpreter::EffectInterpreter;
use super::*;
use zagens_core::engine::context::summarize_text;
use zagens_core::engine::hosts::SeamHost;
use zagens_core::engine::kernel_event::KernelEvent;
use zagens_core::engine::turn_machine::{Effect, emit_kernel_event};

impl Engine {
    /// Route layered-context checkpoint through v3 effect plan or legacy direct IO.
    pub(super) async fn layered_context_checkpoint(&mut self) {
        if self.runtime_ext().kernel_machine_mode.uses_v3_turn_loop() {
            tracing::info!(
                target: "kernel_v3",
                turn_id = ?self.runtime_ext().kernel_active_turn_id,
                step = self.runtime_ext().kernel_active_step,
                "v3 layered context: RunLayeredContextCheckpoint (effect plan)"
            );
            let mut interpreter = EffectInterpreter::new(self);
            let _ = interpreter
                .interpret(Effect::RunLayeredContextCheckpoint)
                .await;
            return;
        }
        self.execute_layered_context_checkpoint().await;
    }

    /// Live IO for [`Effect::RunLayeredContextCheckpoint`].
    pub(in crate::core::engine) async fn run_layered_context_checkpoint_effect(&mut self) {
        if self.effect_replay_anchor_only() {
            tracing::info!(
                target: "kernel_v3",
                "replay anchor-only: skipping RunLayeredContextCheckpoint IO"
            );
            return;
        }
        self.execute_layered_context_checkpoint().await;
    }

    /// Run the pre-request layered-context checkpoint (#159). Checks whether
    /// the active input estimate has crossed a soft-seam threshold and, if so,
    /// produces an `<archived_context>` block via Flash and appends it as an
    /// assistant message. Called from `handle_deepseek_turn` before each API
    /// request so the model always has the latest navigation aids.
    ///
    /// M5: all calls dispatch through the `SeamHost` trait (the
    /// `seam_manager` field type stays `Option<SeamManager>` until M7
    /// swaps it to `Option<Box<dyn SeamHost>>`).
    async fn execute_layered_context_checkpoint(&mut self) {
        let Some(ref seam_mgr) = self.seam else {
            return;
        };
        if !SeamHost::config_enabled(seam_mgr.as_ref()) {
            return;
        }

        let highest = SeamHost::highest_level(seam_mgr.as_ref()).await;
        let Some(level) =
            SeamHost::seam_level_for(seam_mgr.as_ref(), self.estimated_input_tokens(), highest)
        else {
            return;
        };

        // Determine the message range to summarize: everything before the
        // verbatim window. The verbatim window (last ~16 turns) stays
        // untouched so the model always has ground-truth recent context.
        let msg_count = self.session.messages.len();
        let verbatim_start = SeamHost::verbatim_window_start(seam_mgr.as_ref(), msg_count);
        if verbatim_start == 0 {
            return; // Not enough messages to summarize.
        }

        let msg_range_end = verbatim_start;
        let pinned = self
            .session
            .working_set
            .pinned_message_indices(&self.session.messages, &self.session.workspace);

        let _ = self
            .tx_event
            .send(Event::status(format!(
                "⏻ producing L{level} context seam ({msg_range_end} messages)…"
            )))
            .await;

        // If we have existing seams, recompact; otherwise produce fresh.
        let existing_seams =
            SeamHost::collect_seam_texts(seam_mgr.as_ref(), &self.session.messages).await;
        let seam_text = if existing_seams.is_empty() {
            match SeamHost::produce_soft_seam(
                seam_mgr.as_ref(),
                &self.session.messages,
                level,
                0,
                msg_range_end,
                Some(&self.session.workspace),
                &pinned,
            )
            .await
            {
                Ok(text) => text,
                Err(err) => {
                    crate::logging::warn(format!("L{level} soft seam failed: {err}"));
                    return;
                }
            }
        } else {
            let recent: Vec<&Message> = (0..msg_range_end)
                .filter_map(|i| self.session.messages.get(i))
                .collect();
            match SeamHost::recompact(
                seam_mgr.as_ref(),
                &existing_seams,
                &recent,
                level,
                0,
                msg_range_end,
            )
            .await
            {
                Ok(text) => text,
                Err(err) => {
                    crate::logging::warn(format!("L{level} recompact failed: {err}"));
                    return;
                }
            }
        };

        if seam_text.is_empty() {
            return;
        }

        // Capture seam count before the mutable borrow below.
        let seam_count = SeamHost::seam_count(seam_mgr.as_ref()).await;

        // Append the seam as an assistant message. This is an append-only
        // operation — no messages are deleted. The prefix cache stays hot.
        self.add_session_message(Message {
            role: "assistant".to_string(),
            content: vec![ContentBlock::Text {
                text: seam_text.clone(),
                cache_control: None,
            }],
        })
        .await;

        let turn_id = self
            .runtime_ext()
            .kernel_active_turn_id
            .clone()
            .unwrap_or_else(|| "layered-context".to_string());
        let step_idx = self.runtime_ext().kernel_active_step;
        emit_kernel_event(
            self,
            KernelEvent::LayeredContextSeamInjected {
                turn_id,
                step_idx,
                level: u32::from(level),
                messages_covered: msg_range_end as u32,
                text_preview: summarize_text(&seam_text, 512),
            },
        );

        let _ = self
            .tx_event
            .send(Event::status(format!(
                "⏻ L{level} seam complete ({seam_count} total, {msg_range_end} messages covered)"
            )))
            .await;
    }
}
