//! v3 memory-plane steer — routes scratchpad summary/reminder through [`Effect::InjectSteer`].

use zagens_core::chat::{ContentBlock, Message};
use zagens_core::engine::turn_machine::Effect;

use super::effect_interpreter::EffectInterpreter;
use super::*;

impl Engine {
    /// Inject scratchpad / reminder user text (v3: `Effect::InjectSteer`; legacy: session write).
    pub(in crate::core::engine) async fn inject_memory_plane_steer_message(
        &mut self,
        text: String,
    ) {
        let steer = text.trim().to_string();
        if steer.is_empty() {
            return;
        }
        if self.runtime_ext().kernel_machine_mode.uses_v3_turn_loop() {
            tracing::info!(
                target: "kernel_v3",
                turn_id = ?self.runtime_ext().kernel_active_turn_id,
                step = self.runtime_ext().kernel_active_step,
                "v3 memory-plane: InjectSteer (effect plan)"
            );
            let mut interpreter = EffectInterpreter::new(self);
            let _ = interpreter
                .interpret(Effect::InjectSteer { text: steer })
                .await;
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

#[must_use]
pub(in crate::core::engine) fn user_message_plain_text(message: &Message) -> String {
    message
        .content
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_message_plain_text_joins_text_blocks() {
        let msg = Message {
            role: "user".into(),
            content: vec![
                ContentBlock::Text {
                    text: "line-a".into(),
                    cache_control: None,
                },
                ContentBlock::Text {
                    text: "line-b".into(),
                    cache_control: None,
                },
            ],
        };
        assert_eq!(user_message_plain_text(&msg), "line-a\nline-b");
    }
}
