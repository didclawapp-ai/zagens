//! Keep `Engine.scratchpad_run_id` aligned with the tool wire slot.

use super::Engine;

impl Engine {
    pub(in crate::core::engine) fn wire_scratchpad_run_id(&self) -> Option<String> {
        self.config_ext()
            .runtime_services
            .wire
            .scratchpad_run_id
            .lock()
            .ok()
            .and_then(|guard| guard.clone())
    }

    /// Copy the wire slot into engine state when a run is bound (turn start or mid-turn init).
    pub(in crate::core::engine) fn sync_scratchpad_run_id_from_wire(&mut self) {
        if let Some(run_id) = self.wire_scratchpad_run_id()
            && !run_id.trim().is_empty()
        {
            self.scratchpad_run_id = Some(run_id);
        }
    }
}
