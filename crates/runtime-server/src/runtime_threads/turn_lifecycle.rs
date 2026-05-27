//! Turn start — delegates to orchestrator with sidecar `RuntimeThreadHost`.

use anyhow::Result;

use super::{EditLastTurnRequest, RuntimeThreadManager, StartTurnRequest, TurnRecord};

impl RuntimeThreadManager {
    pub async fn start_turn(&self, thread_id: &str, req: StartTurnRequest) -> Result<TurnRecord> {
        deepseek_runtime_orchestrator::runtime_threads::turn_lifecycle::start_turn(
            self, self, thread_id, req,
        )
        .await
    }

    pub async fn edit_last_turn(
        &self,
        thread_id: &str,
        req: EditLastTurnRequest,
    ) -> Result<TurnRecord> {
        deepseek_runtime_orchestrator::runtime_threads::turn_lifecycle::edit_last_turn(
            self, self, thread_id, req,
        )
        .await
    }
}
