//! v3 memory-plane steer — legacy non-v3 session writes for memory injections.

use zagens_core::chat::{ContentBlock, Message};

use super::*;

impl Engine {
    /// Legacy session write for memory-plane user text (non-v3 path only).
    pub(in crate::core::engine) async fn inject_memory_plane_steer_message(
        &mut self,
        text: String,
    ) {
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
