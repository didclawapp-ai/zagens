//! v3 continuation steer — routes step-limit / loop-guard nudges through [`Effect::InjectSteer`].

use zagens_core::chat::{ContentBlock, Message};
use zagens_core::engine::turn_machine::Effect;
use zagens_core::turn::TurnContext;

use super::effect_interpreter::EffectInterpreter;
use super::*;
use crate::core::events::Event;

impl Engine {
    /// Inject a step-limit continuation nudge (v3: `Effect::InjectSteer`; legacy: session write).
    pub(in crate::core::engine) async fn inject_step_limit_continuation_steer(
        &mut self,
        turn: &TurnContext,
        text: String,
        open_items: u32,
    ) {
        self.inject_continuation_steer_message(turn, text).await;
        let _ = self
            .tx_event
            .send(Event::status(format!(
                "long_horizon.step_limit_continue: {{\"open_items\":{open_items}}}"
            )))
            .await;
    }

    /// Inject a loop-guard continuation nudge (v3: `Effect::InjectSteer`; legacy: session write).
    pub(in crate::core::engine) async fn inject_loop_guard_continuation_steer(
        &mut self,
        turn: &TurnContext,
        text: String,
        open_items: u32,
    ) {
        self.inject_continuation_steer_message(turn, text).await;
        let _ = self
            .tx_event
            .send(Event::status(format!(
                "long_horizon.loop_guard_continue: {{\"open_items\":{open_items}}}"
            )))
            .await;
    }

    async fn inject_continuation_steer_message(&mut self, _turn: &TurnContext, text: String) {
        if self.runtime_ext().kernel_machine_mode.uses_v3_turn_loop() {
            let mut interpreter = EffectInterpreter::new(self);
            let _ = interpreter.interpret(Effect::InjectSteer { text }).await;
            return;
        }
        let workspace = self.session.workspace.clone();
        self.session
            .working_set
            .observe_user_message(&text, &workspace);
        self.add_session_message(Message {
            role: "user".to_string(),
            content: vec![ContentBlock::Text {
                text,
                cache_control: None,
            }],
        })
        .await;
    }
}
