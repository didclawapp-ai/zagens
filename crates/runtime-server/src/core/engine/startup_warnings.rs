//! One-shot startup warnings surfaced to the UI via status events.

use crate::core::events::Event;

use super::Engine;

impl Engine {
    pub(super) async fn emit_pending_startup_warnings(&mut self) {
        if let Some(warning) = self.runtime_ext_mut().sandbox_init_warning.take() {
            let _ = self.tx_event.send(Event::status(warning)).await;
        }
    }
}
