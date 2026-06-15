//! Background drain for [`KernelEvent`] double-write (Phase 3b batch 2).
//!
//! Opens (or creates) `sessions.db`, ensures the `kernel_events` table exists,
//! and spawns a tokio task that batches events from an unbounded channel into
//! SQLite via [`KernelEventLog`].  Turn-loop code emits through the returned
//! [`KernelEventSink`] without blocking on disk I/O.

use std::path::Path;
use std::sync::{Arc, Mutex as StdMutex};

use anyhow::Context as _;
use rusqlite::Connection;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{debug, warn};
use zagens_core::engine::kernel_event::KernelEvent;
use zagens_core::engine::turn_machine::KernelEventSink;

use super::kernel_event_log::{KernelEventLog, ensure_kernel_events_table};
use super::session_manager::default_sessions_dir;

/// Owns the channel sender, shared DB handle, and the background drain task handle.
///
/// Drop shuts down the drain loop (sender closed → task exits after flushing).
pub struct KernelEventWriter {
    tx: KernelEventSink,
    db: Arc<StdMutex<Connection>>,
    _drain: JoinHandle<()>,
}

impl KernelEventWriter {
    /// Open the default `~/.zagens/sessions/sessions.db` and start draining.
    /// Returns `None` when the sessions directory cannot be resolved (e.g.
    /// headless CI without home dir) — double-write is silently disabled.
    pub fn try_open_default() -> Option<Self> {
        let dir = default_sessions_dir().ok()?;
        std::fs::create_dir_all(&dir).ok()?;
        let db_path = dir.join("sessions.db");
        match Self::try_open(&db_path) {
            Ok(writer) => Some(writer),
            Err(err) => {
                warn!(target: "kernel_event", %err, "kernel event log disabled");
                None
            }
        }
    }

    /// Open (or create) `db_path` and start the drain task.
    pub fn try_open(db_path: &Path) -> anyhow::Result<Self> {
        let conn = Connection::open(db_path)
            .with_context(|| format!("open kernel event db {}", db_path.display()))?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")
            .context("set SQLite pragmas")?;
        ensure_kernel_events_table(&conn).context("ensure kernel_events table")?;

        seed_schema_version_if_empty(&conn)?;

        let db = Arc::new(StdMutex::new(conn));
        let (tx, mut rx) = mpsc::unbounded_channel::<KernelEvent>();

        let db_path_log = db_path.to_path_buf();
        let db_drain = Arc::clone(&db);
        let drain = tokio::spawn(async move {
            while let Some(first) = rx.recv().await {
                let mut batch = vec![first];
                while let Ok(more) = rx.try_recv() {
                    batch.push(more);
                }
                let db = Arc::clone(&db_drain);
                let count = batch.len();
                let write_result = tokio::task::spawn_blocking(move || append_batch(&db, batch))
                    .await
                    .context("kernel event drain join");
                match write_result {
                    Ok(Ok(())) => {
                        debug!(
                            target: "kernel_event",
                            count,
                            db = %db_path_log.display(),
                            "appended kernel events"
                        );
                    }
                    Ok(Err(err)) | Err(err) => {
                        warn!(
                            target: "kernel_event",
                            %err,
                            count,
                            "kernel event append failed"
                        );
                    }
                }
            }
            debug!(target: "kernel_event", "kernel event drain stopped");
        });

        Ok(Self {
            tx,
            db,
            _drain: drain,
        })
    }

    /// Load all persisted events for `turn_id` (blocking read on the shared DB).
    pub fn load_turn_events_sync(&self, turn_id: &str) -> anyhow::Result<Vec<KernelEvent>> {
        let db = self
            .db
            .lock()
            .map_err(|_| anyhow::anyhow!("kernel event db mutex poisoned"))?;
        let log = KernelEventLog::new(&db);
        Ok(log
            .load_turn_events(turn_id)?
            .into_iter()
            .map(|env| env.event)
            .collect())
    }

    /// Compare in-memory turn events with the SQLite log (Phase 3b replay gate).
    #[must_use]
    pub fn verify_persisted_turn_matches(
        &self,
        turn_id: &str,
        in_memory: &[KernelEvent],
    ) -> Option<String> {
        let loaded = match self.load_turn_events_sync(turn_id) {
            Ok(events) => events,
            Err(err) => return Some(format!("load failed: {err}")),
        };
        if loaded.len() != in_memory.len() {
            return Some(format!(
                "persist count {} != memory {}",
                loaded.len(),
                in_memory.len()
            ));
        }
        for (idx, (a, b)) in loaded.iter().zip(in_memory.iter()).enumerate() {
            let a_json = serde_json::to_string(a).ok();
            let b_json = serde_json::to_string(b).ok();
            if a_json != b_json {
                return Some(format!(
                    "event mismatch at index {idx}: persist={} memory={}",
                    a.kind_str(),
                    b.kind_str()
                ));
            }
        }
        None
    }

    #[must_use]
    pub fn sink(&self) -> KernelEventSink {
        self.tx.clone()
    }

    /// Borrow the live sender (for `TurnLoopHost::kernel_event_sink`).
    #[must_use]
    pub fn tx(&self) -> &KernelEventSink {
        &self.tx
    }
}

fn seed_schema_version_if_empty(db: &Connection) -> anyhow::Result<()> {
    let count: i64 = db.query_row("SELECT COUNT(*) FROM kernel_events", [], |row| row.get(0))?;
    if count == 0 {
        let mut log = KernelEventLog::new(db);
        log.append(KernelEvent::SchemaVersion { version: 1 })?;
    }
    Ok(())
}

fn append_batch(db: &StdMutex<Connection>, events: Vec<KernelEvent>) -> anyhow::Result<()> {
    let db = db
        .lock()
        .map_err(|_| anyhow::anyhow!("kernel event db mutex poisoned"))?;
    let next_seq = KernelEventLog::peek_next_seq(&db)?;
    let mut log = KernelEventLog::with_next_seq(&db, next_seq);
    log.append_batch(events)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use zagens_core::engine::kernel_event::TurnOutcome;
    use zagens_core::turn::TurnLoopMode;

    #[tokio::test]
    async fn writer_drains_events_to_sqlite() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path: PathBuf = dir.path().join("sessions.db");
        let writer = KernelEventWriter::try_open(&db_path).expect("open writer");
        let sink = writer.sink();

        sink.send(KernelEvent::TurnStarted {
            turn_id: "t-writer-001".into(),
            mode: TurnLoopMode::Agent,
            input_text: "hello".into(),
            max_steps: 5,
        })
        .expect("send");
        sink.send(KernelEvent::TurnEnded {
            turn_id: "t-writer-001".into(),
            outcome: TurnOutcome::Completed,
            total_steps: 1,
        })
        .expect("send");

        drop(sink);
        drop(writer);

        // Allow drain task to finish.
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let db = Connection::open(&db_path).expect("reopen");
        let count: i64 = db
            .query_row("SELECT COUNT(*) FROM kernel_events", [], |r| r.get(0))
            .expect("count");
        // schema_version + turn_started + turn_ended
        assert_eq!(count, 3, "expected schema_version + 2 turn events");

        let log = KernelEventLog::new(&db);
        let envelopes = log
            .load_turn_events("t-writer-001")
            .expect("load turn events");
        assert_eq!(envelopes.len(), 2);

        let writer2 = KernelEventWriter::try_open(&db_path).expect("reopen writer");
        let loaded = writer2
            .load_turn_events_sync("t-writer-001")
            .expect("load sync");
        assert_eq!(loaded.len(), 2);
        let in_memory: Vec<KernelEvent> = envelopes.into_iter().map(|e| e.event).collect();
        assert!(
            writer2
                .verify_persisted_turn_matches("t-writer-001", &in_memory)
                .is_none()
        );
    }
}
