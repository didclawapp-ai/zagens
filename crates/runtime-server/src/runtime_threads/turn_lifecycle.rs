//! Turn start — delegates to orchestrator with sidecar `RuntimeThreadHost`.

use anyhow::Result;

use super::{
    EditLastTurnRequest, RuntimeThreadManager, StartTurnOutcome, StartTurnRequest, TurnRecord,
};

impl RuntimeThreadManager {
    pub async fn start_turn(
        &self,
        thread_id: &str,
        req: StartTurnRequest,
    ) -> Result<StartTurnOutcome> {
        zagens_runtime_orchestrator::runtime_threads::turn_lifecycle::start_turn(
            self, self, thread_id, req,
        )
        .await
    }

    pub async fn edit_last_turn(
        &self,
        thread_id: &str,
        req: EditLastTurnRequest,
    ) -> Result<StartTurnOutcome> {
        zagens_runtime_orchestrator::runtime_threads::turn_lifecycle::edit_last_turn(
            self, self, thread_id, req,
        )
        .await
    }

    /// Convenience wrapper for callers that only need the started turn record.
    pub async fn start_turn_record(
        &self,
        thread_id: &str,
        req: StartTurnRequest,
    ) -> Result<TurnRecord> {
        Ok(self.start_turn(thread_id, req).await?.turn)
    }
}
