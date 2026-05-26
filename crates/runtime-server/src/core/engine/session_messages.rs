//! Session transcript mutation + `SessionUpdated` event emission.

use crate::core::events::Event;
use crate::models::Message;

use super::Engine;

impl Engine {
    pub(super) async fn emit_session_updated(&self) {
        let _ = self
            .tx_event
            .send(Event::SessionUpdated {
                messages: self.session.messages.clone(),
                system_prompt: self.session.system_prompt.clone(),
                model: self.session.model.clone(),
                workspace: self.session.workspace.clone(),
            })
            .await;
    }

    pub(super) async fn add_session_message(&mut self, message: Message) {
        self.session.add_message(message);
        self.emit_session_updated().await;
    }
}
