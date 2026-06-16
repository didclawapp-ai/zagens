//! v3 memory-plane artifact emission — routes [`Effect::EmitArtifact`] through the interpreter.

use zagens_core::chat::{ContentBlock, Message};
use zagens_core::engine::kernel_event::KernelEvent;
use zagens_core::engine::turn_loop::memory_artifact_policy::MemoryArtifactKind;
use zagens_core::engine::turn_machine::emit_kernel_event;

use super::memory_plane_ops::user_message_plain_text;
use super::scratchpad_flow;
use super::*;

impl Engine {
    /// Emit a memory-plane artifact (scratchpad snapshot / reminder).
    pub(in crate::core::engine) async fn run_emit_artifact_effect(
        &mut self,
        turn_id: &str,
        step_idx: u32,
        kind: MemoryArtifactKind,
        area_hint: Option<String>,
    ) -> bool {
        if self.effect_replay_anchor_only() {
            tracing::info!(
                target: "kernel_v3",
                ?kind,
                "replay anchor-only: skipping EmitArtifact IO"
            );
            return false;
        }
        if !self.runtime_ext().kernel_machine_mode.uses_v3_turn_loop() {
            return false;
        }
        match kind {
            MemoryArtifactKind::ScratchpadSnapshot => {
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
                let text = user_message_plain_text(&summary_msg);
                self.inject_memory_plane_user_message(&text).await;
                self.scratchpad_summary_injected_this_turn = true;
                emit_kernel_event(
                    self,
                    KernelEvent::ScratchpadSummaryInjected {
                        turn_id: turn_id.to_string(),
                        at_step: step_idx,
                    },
                );
                tracing::info!(
                    target: "kernel_v3",
                    turn_id = %turn_id,
                    step = step_idx,
                    kind = kind.as_str(),
                    "v3 memory-plane: EmitArtifact (scratchpad snapshot)"
                );
                true
            }
            MemoryArtifactKind::ScratchpadReminder => {
                let Some((reminder, area_path)) = scratchpad_flow::build_readonly_reminder_message(
                    &self.session.workspace,
                    self.scratchpad_run_id.as_deref(),
                    &self.config.scratchpad,
                    &self.scratchpad_step,
                ) else {
                    return false;
                };
                let text = user_message_plain_text(&reminder);
                self.inject_memory_plane_user_message(&text).await;
                let area_path = area_hint.unwrap_or(area_path);
                emit_kernel_event(
                    self,
                    KernelEvent::ScratchpadReminderInjected {
                        turn_id: turn_id.to_string(),
                        step_idx,
                        area_path,
                    },
                );
                tracing::info!(
                    target: "kernel_v3",
                    turn_id = %turn_id,
                    step = step_idx,
                    kind = kind.as_str(),
                    "v3 memory-plane: EmitArtifact (scratchpad reminder)"
                );
                true
            }
        }
    }

    /// Session write for scratchpad artifacts (no `SteerInjected` double-write).
    async fn inject_memory_plane_user_message(&mut self, text: &str) {
        let steer = text.trim().to_string();
        if steer.is_empty() {
            return;
        }
        let workspace = self.session.workspace.clone();
        self.session
            .working_set
            .observe_user_message(&steer, &workspace);
        self.add_session_message(Message {
            role: "user".to_string(),
            content: vec![ContentBlock::Text {
                text: steer,
                cache_control: None,
            }],
        })
        .await;
    }
}
