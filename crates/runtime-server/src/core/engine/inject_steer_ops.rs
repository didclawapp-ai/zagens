//! v3 effect interpreter entry for [`Effect::InjectSteer`].

use super::*;

use zagens_core::engine::context::summarize_text;
use zagens_core::engine::kernel_event::KernelEvent;
use zagens_core::engine::turn_machine::emit_kernel_event;

impl Engine {
    /// Inject steer text into the session transcript (mirrors op-loop steer drain).
    pub(in crate::core::engine) async fn run_inject_steer_effect(
        &mut self,
        turn_id: &str,
        step_idx: u32,
        text: String,
    ) {
        if self.try_run_pending_inject_steer_kind().await {
            return;
        }
        self.apply_inject_steer_text(turn_id, step_idx, text).await;
    }

    pub(in crate::core::engine) async fn apply_inject_steer_text(
        &mut self,
        turn_id: &str,
        step_idx: u32,
        text: String,
    ) {
        if self.effect_replay_anchor_only() {
            tracing::info!(
                target: "kernel_v3",
                "replay anchor-only: skipping InjectSteer IO"
            );
            return;
        }
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
                text: steer.clone(),
                cache_control: None,
            }],
        })
        .await;
        let _ = self
            .tx_event
            .send(Event::status(format!(
                "Steer input accepted: {}",
                summarize_text(&steer, 120)
            )))
            .await;
        emit_kernel_event(
            self,
            KernelEvent::SteerInjected {
                turn_id: turn_id.to_string(),
                step_idx,
                text: summarize_text(&steer, 512),
            },
        );
    }
}
