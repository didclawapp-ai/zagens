//! Authoritative thread streaming status owner (`thread.status`) for desktop multi-session UI.
//!
//! `ThreadStatusOwner` is the sole in-memory mutator; `set_thread_status` is the sole emit path.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::Mutex;

use super::RuntimeEventRecord;
use super::RuntimeThreadManager;

/// Coarse thread lifecycle status mirrored by the desktop web UI.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadStreamStatus {
    Streaming,
    AwaitingApproval,
    Idle,
    Error,
}

impl ThreadStreamStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Streaming => "streaming",
            Self::AwaitingApproval => "awaiting_approval",
            Self::Idle => "idle",
            Self::Error => "error",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim() {
            "streaming" => Some(Self::Streaming),
            "awaiting_approval" => Some(Self::AwaitingApproval),
            "idle" => Some(Self::Idle),
            "error" => Some(Self::Error),
            _ => None,
        }
    }

    pub const fn is_active(self) -> bool {
        matches!(self, Self::Streaming | Self::AwaitingApproval)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ThreadStatusEntry {
    pub status: ThreadStreamStatus,
    pub seq: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
}

/// In-memory owner — idle/error entries are removed ("absent = idle").
#[derive(Clone, Default)]
pub struct ThreadStatusOwner {
    entries: Arc<Mutex<HashMap<String, ThreadStatusEntry>>>,
}

impl ThreadStatusOwner {
    pub fn new() -> Self {
        Self {
            entries: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn list(&self) -> Vec<(String, ThreadStatusEntry)> {
        let map = self.entries.lock().await;
        let mut out: Vec<_> = map
            .iter()
            .map(|(id, entry)| (id.clone(), entry.clone()))
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    /// Threads with an in-flight turn (streaming or awaiting approval).
    pub async fn active_list(&self) -> Vec<(String, ThreadStatusEntry)> {
        let map = self.entries.lock().await;
        let mut out: Vec<_> = map
            .iter()
            .filter(|(_, entry)| entry.status.is_active())
            .map(|(id, entry)| (id.clone(), entry.clone()))
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    pub async fn active_count(&self) -> usize {
        self.active_list().await.len()
    }

    /// Apply a status transition to the in-memory owner.
    ///
    /// `seq` is the persisted event seq. Invariant: emits go through
    /// [`RuntimeThreadManager::set_thread_status`], whose `emit_event` allocates a
    /// strictly increasing seq under a single writer, so per-thread `seq` is
    /// monotonic here. The `debug_assert` guards against a future caller breaking
    /// that ordering. idle/error are removed ("absent = idle").
    pub async fn apply_local(
        &self,
        thread_id: &str,
        turn_id: Option<&str>,
        status: ThreadStreamStatus,
        seq: u64,
    ) {
        let mut map = self.entries.lock().await;
        if !status.is_active() {
            map.remove(thread_id);
            return;
        }
        debug_assert!(
            map.get(thread_id).is_none_or(|prev| seq >= prev.seq),
            "thread.status seq must be monotonic per thread (prev > incoming)",
        );
        map.insert(
            thread_id.to_string(),
            ThreadStatusEntry {
                status,
                seq,
                turn_id: turn_id.map(str::to_string),
            },
        );
    }
}

impl<P, R> RuntimeThreadManager<P, R>
where
    P: Send + Sync + Clone + 'static,
    R: Send + Sync + Clone + 'static,
{
    /// Emit a durable `thread.status` record, update the in-memory owner, and broadcast live.
    pub async fn set_thread_status(
        &self,
        thread_id: &str,
        turn_id: Option<&str>,
        status: ThreadStreamStatus,
    ) -> Result<RuntimeEventRecord> {
        let record = self
            .emit_event(
                thread_id,
                turn_id,
                None,
                "thread.status",
                json!({ "status": status.as_str() }),
            )
            .await?;
        self.thread_status
            .apply_local(thread_id, turn_id, status, record.seq)
            .await;
        Ok(record)
    }

    /// Snapshot of non-idle threads for global status SSE connect.
    pub async fn thread_status_list(&self) -> Vec<(String, ThreadStatusEntry)> {
        self.thread_status.list().await
    }

    /// Back-compat alias — prefer [`set_thread_status`](Self::set_thread_status).
    pub async fn emit_thread_status(
        &self,
        thread_id: &str,
        turn_id: Option<&str>,
        status: ThreadStreamStatus,
    ) -> Result<()> {
        self.set_thread_status(thread_id, turn_id, status).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn owner_removes_idle_and_keeps_active() {
        let owner = ThreadStatusOwner::new();
        owner
            .apply_local("thr_a", Some("turn_1"), ThreadStreamStatus::Streaming, 10)
            .await;
        assert_eq!(owner.list().await.len(), 1);
        owner
            .apply_local("thr_a", Some("turn_1"), ThreadStreamStatus::Idle, 20)
            .await;
        assert!(owner.list().await.is_empty());
    }
}
