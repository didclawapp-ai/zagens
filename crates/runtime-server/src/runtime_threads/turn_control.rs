//! Turn control — interrupt/steer via Deref; compaction via orchestrator + host.

use anyhow::Result;

use super::{CompactThreadRequest, RuntimeThreadManager, TurnRecord};

impl RuntimeThreadManager {
    pub async fn compact_thread(
        &self,
        thread_id: &str,
        req: CompactThreadRequest,
    ) -> Result<TurnRecord> {
        zagens_runtime_orchestrator::runtime_threads::turn_control::compact_thread(
            self, self, thread_id, req,
        )
        .await
    }
}
