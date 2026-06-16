//! v3 continuation steer — routes step-limit / loop-guard nudges through [`Effect::InjectSteer`].

use zagens_core::chat::{ContentBlock, Message};
use zagens_core::engine::turn_loop::continuation_boundary_policy::OuterBoundaryKind;
use zagens_core::turn::TurnContext;

use super::cycle_briefing_ops::InjectSteerEffectKind;
use super::*;

impl Engine {
    /// Inject a step-limit continuation nudge (v3: planner `InjectSteer`; legacy: session write).
    pub(in crate::core::engine) async fn inject_step_limit_continuation_steer(
        &mut self,
        turn: &TurnContext,
        text: String,
        open_items: u32,
    ) {
        if self.runtime_ext().kernel_machine_mode.uses_v3_turn_loop() {
            self.run_v3_planner_outer_boundary_steer(
                OuterBoundaryKind::StepLimit,
                &turn.id,
                turn.step,
                InjectSteerEffectKind::StepLimitContinuation {
                    text: text.clone(),
                    open_items,
                },
            )
            .await;
            return;
        }
        self.inject_continuation_steer_legacy(&text).await;
        let _ = self
            .tx_event
            .send(Event::status(format!(
                "long_horizon.step_limit_continue: {{\"open_items\":{open_items}}}"
            )))
            .await;
    }

    /// Inject a loop-guard continuation nudge (v3: planner `InjectSteer`; legacy: session write).
    pub(in crate::core::engine) async fn inject_loop_guard_continuation_steer(
        &mut self,
        turn: &TurnContext,
        text: String,
        open_items: u32,
    ) {
        if self.runtime_ext().kernel_machine_mode.uses_v3_turn_loop() {
            self.run_v3_planner_outer_boundary_steer(
                OuterBoundaryKind::LoopGuard,
                &turn.id,
                turn.step,
                InjectSteerEffectKind::LoopGuardContinuation {
                    text: text.clone(),
                    open_items,
                },
            )
            .await;
            return;
        }
        self.inject_continuation_steer_legacy(&text).await;
        let _ = self
            .tx_event
            .send(Event::status(format!(
                "long_horizon.loop_guard_continue: {{\"open_items\":{open_items}}}"
            )))
            .await;
    }

    pub(in crate::core::engine) async fn inject_continuation_steer_legacy(&mut self, text: &str) {
        let workspace = self.session.workspace.clone();
        self.session
            .working_set
            .observe_user_message(text, &workspace);
        self.add_session_message(Message {
            role: "user".to_string(),
            content: vec![ContentBlock::Text {
                text: text.to_string(),
                cache_control: None,
            }],
        })
        .await;
    }
}
