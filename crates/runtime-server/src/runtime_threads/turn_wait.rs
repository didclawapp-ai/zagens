//! Wait helpers for runtime turn completion (PR5 sidecar bridge).

use std::time::{Duration, Instant};

use anyhow::{Result, bail};
use tokio::time::sleep;

use super::types::{RuntimeTurnStatus, TurnItemKind, TurnRecord};
use super::RuntimeThreadManager;

impl RuntimeThreadManager {
    /// Poll the store until the turn reaches a terminal status or times out.
    pub async fn wait_turn_terminal(
        &self,
        turn_id: &str,
        timeout: Duration,
    ) -> Result<TurnRecord> {
        let deadline = Instant::now() + timeout;
        loop {
            let turn = self.store.load_turn(turn_id)?;
            if matches!(
                turn.status,
                RuntimeTurnStatus::Completed
                    | RuntimeTurnStatus::Failed
                    | RuntimeTurnStatus::Interrupted
                    | RuntimeTurnStatus::Canceled
            ) {
                return Ok(turn);
            }
            if Instant::now() >= deadline {
                bail!("Timed out waiting for turn {turn_id}");
            }
            sleep(Duration::from_millis(20)).await;
        }
    }

    /// Concatenate completed agent message item bodies for a turn.
    pub fn assistant_text_for_turn(&self, turn: &TurnRecord) -> Result<String> {
        let mut parts = Vec::new();
        for item_id in &turn.item_ids {
            let item = self.store.load_item(item_id)?;
            if item.kind == TurnItemKind::AgentMessage
                && item.status == super::types::TurnItemLifecycleStatus::Completed
                && let Some(detail) = item.detail.filter(|text| !text.trim().is_empty())
            {
                parts.push(detail);
            }
        }
        Ok(parts.join("\n\n"))
    }
}
