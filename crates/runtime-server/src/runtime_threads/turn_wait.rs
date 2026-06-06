//! Wait helpers for runtime turn completion (PR5 sidecar bridge).

use std::time::Duration;

use anyhow::Result;

use super::RuntimeThreadManager;
use super::types::TurnRecord;

impl RuntimeThreadManager {
    /// Poll the store until the turn reaches a terminal status or times out.
    pub async fn wait_turn_terminal(&self, turn_id: &str, timeout: Duration) -> Result<TurnRecord> {
        deepseek_runtime_orchestrator::runtime_threads::turn_wait::wait_turn_terminal(
            &self.store,
            turn_id,
            timeout,
        )
        .await
    }

    /// Concatenate completed agent message item bodies for a turn.
    pub fn assistant_text_for_turn(&self, turn: &TurnRecord) -> Result<String> {
        deepseek_runtime_orchestrator::runtime_threads::turn_wait::assistant_text_for_turn(
            &self.store,
            turn,
        )
    }
}
