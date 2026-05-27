//! Sidecar `RuntimeThreadTaskPort` — thread create/start for background tasks.

use anyhow::Result;
use async_trait::async_trait;

use deepseek_runtime_orchestrator::runtime_threads::{
    CreateThreadRequest, RuntimeEventRecord, RuntimeThreadTaskPort, StartTurnRequest,
    ThreadRecord, TurnRecord,
};

use super::RuntimeThreadManager;

#[async_trait]
impl RuntimeThreadTaskPort for RuntimeThreadManager {
    async fn create_thread(&self, req: CreateThreadRequest) -> Result<ThreadRecord> {
        RuntimeThreadManager::create_thread(self, req).await
    }

    async fn start_turn(&self, thread_id: &str, req: StartTurnRequest) -> Result<TurnRecord> {
        RuntimeThreadManager::start_turn(self, thread_id, req).await
    }

    async fn interrupt_turn(&self, thread_id: &str, turn_id: &str) -> Result<TurnRecord> {
        use std::ops::Deref;
        self.deref().interrupt_turn(thread_id, turn_id).await
    }

    async fn events_since_async(
        &self,
        thread_id: &str,
        since_seq: Option<u64>,
    ) -> Result<Vec<RuntimeEventRecord>> {
        use std::ops::Deref;
        self.deref().events_since_async(thread_id, since_seq).await
    }
}
