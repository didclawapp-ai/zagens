//! Turn execution port for background tasks (D16 E1-b phase 5).

use anyhow::Result;
use async_trait::async_trait;

use super::types::{RuntimeEventRecord, ThreadRecord, TurnRecord};
use super::{CreateThreadRequest, StartTurnRequest};

/// Minimal runtime surface for durable background task workers.
#[async_trait]
pub trait RuntimeThreadTaskPort: Send + Sync {
    async fn create_thread(&self, req: CreateThreadRequest) -> Result<ThreadRecord>;

    async fn start_turn(&self, thread_id: &str, req: StartTurnRequest) -> Result<TurnRecord>;

    async fn interrupt_turn(&self, thread_id: &str, turn_id: &str) -> Result<TurnRecord>;

    async fn events_since_async(
        &self,
        thread_id: &str,
        since_seq: Option<u64>,
    ) -> Result<Vec<RuntimeEventRecord>>;
}
