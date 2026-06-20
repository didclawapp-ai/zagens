//! Background drain for [`KernelEvent`] double-write (Phase 3b batch 2).
//!
//! Opens (or creates) `sessions.db`, ensures the `kernel_events` table exists,
//! and spawns a tokio task that batches events from an unbounded channel into
//! SQLite via [`KernelEventLog`].  Turn-loop code emits through the returned
//! [`KernelEventSink`] without blocking on disk I/O.

use std::path::Path;
use std::sync::{Arc, Mutex as StdMutex};

use anyhow::Context as _;
use rusqlite::{Connection, params};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{debug, warn};
use zagens_core::engine::kernel_event::{KernelEvent, KernelEventEnvelope};
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
        Ok(self
            .load_turn_envelopes_sync(turn_id)?
            .into_iter()
            .map(|env| env.event)
            .collect())
    }

    /// Load full envelopes (seq / ts_ms) for trace export.
    pub fn load_turn_envelopes_sync(
        &self,
        turn_id: &str,
    ) -> anyhow::Result<Vec<KernelEventEnvelope>> {
        let db = self
            .db
            .lock()
            .map_err(|_| anyhow::anyhow!("kernel event db mutex poisoned"))?;
        let log = KernelEventLog::new(&db);
        Ok(log.load_turn_events(turn_id)?)
    }

    /// Diagnostic snapshot for trace-export error messages: total `kernel_events`
    /// row count plus up to `limit` distinct `turn_id` values (excluding the
    /// synthetic `schema_version` row whose `turn_id` is NULL). Helps distinguish
    /// "sessions.db is empty" vs "turn_id mismatch" vs "wrong sessions.db path".
    ///
    /// Samples both the earliest and latest distinct turn_ids so the caller can
    /// tell whether the running runtime ever wrote `turn_…` ids at all.
    pub fn diagnose_turn_ids(
        &self,
        limit: usize,
    ) -> anyhow::Result<(u64, Vec<String>, Vec<String>)> {
        let db = self
            .db
            .lock()
            .map_err(|_| anyhow::anyhow!("kernel event db mutex poisoned"))?;
        let total: u64 = db.query_row(
            "SELECT COUNT(*) FROM kernel_events WHERE turn_id IS NOT NULL",
            [],
            |r| r.get(0),
        )?;
        let earliest = distinct_turn_ids(&db, "ASC", limit)?;
        let latest = distinct_turn_ids(&db, "DESC", limit)?;
        Ok((total, earliest, latest))
    }

    /// Count distinct `turn_id` values matching a SQL LIKE pattern (e.g. `'turn_%'`).
    /// Used to confirm whether sessions.db contains orchestrator-style `turn_…`
    /// ids at all, or only engine-internal UUIDs.
    pub fn count_turn_ids_like(&self, like_pattern: &str) -> anyhow::Result<u64> {
        let db = self
            .db
            .lock()
            .map_err(|_| anyhow::anyhow!("kernel event db mutex poisoned"))?;
        let count: u64 = db.query_row(
            "SELECT COUNT(DISTINCT turn_id) FROM kernel_events WHERE turn_id LIKE ?1",
            params![like_pattern],
            |r| r.get(0),
        )?;
        Ok(count)
    }

    /// Try to locate a turn_id in `kernel_events` when the runtime store's turn id
    /// does not match exactly. Returns the first matching `turn_id` (and its row
    /// count) when the runtime store id is a prefix of the sessions.db turn_id,
    /// or vice-versa. Used by trace-export to recover historical turns whose
    /// engine-internal UUID differs from the orchestrator `turn_…` id.
    pub fn resolve_turn_id_alias(&self, turn_id: &str) -> anyhow::Result<Option<(String, u64)>> {
        let db = self
            .db
            .lock()
            .map_err(|_| anyhow::anyhow!("kernel event db mutex poisoned"))?;
        // Case 1: sessions.db turn_id starts with the runtime turn id (runtime id
        // is a short prefix like `turn_c775345b`, sessions.db has full UUID).
        let mut stmt = db.prepare(
            "SELECT turn_id, COUNT(*) FROM kernel_events \
             WHERE turn_id LIKE ?1 || '%' AND turn_id IS NOT NULL \
             GROUP BY turn_id ORDER BY COUNT(*) DESC LIMIT 1",
        )?;
        let prefix_match = stmt
            .query_row(params![turn_id], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, u64>(1)?))
            })
            .ok();
        if prefix_match.is_some() {
            return Ok(prefix_match);
        }
        // Case 2: runtime turn id starts with the sessions.db turn_id (reverse).
        let mut stmt2 = db.prepare(
            "SELECT turn_id, COUNT(*) FROM kernel_events \
             WHERE ?1 LIKE turn_id || '%' AND turn_id IS NOT NULL \
             GROUP BY turn_id ORDER BY COUNT(*) DESC LIMIT 1",
        )?;
        let suffix_match = stmt2
            .query_row(params![turn_id], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, u64>(1)?))
            })
            .ok();
        Ok(suffix_match)
    }

    /// Load kernel events whose `ts_ms` falls within `[from_ms, to_ms]` inclusive,
    /// regardless of `turn_id`. Used by trace-export to recover historical turns
    /// whose engine-internal UUID is not persisted in runtime.db (pre-fix data).
    /// Returns envelopes ordered by `seq ASC`.
    pub fn load_events_by_time_window(
        &self,
        from_ms: u64,
        to_ms: u64,
    ) -> anyhow::Result<Vec<KernelEventEnvelope>> {
        let db = self
            .db
            .lock()
            .map_err(|_| anyhow::anyhow!("kernel event db mutex poisoned"))?;
        let log = KernelEventLog::new(&db);
        log.load_events_by_time_window(from_ms, to_ms)
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

/// Return up to `limit` distinct `turn_id` values ordered by first-seen `seq`
/// in the given direction (`ASC` = earliest, `DESC` = latest). Excludes the
/// synthetic `schema_version` row whose `turn_id` is NULL.
fn distinct_turn_ids(
    db: &Connection,
    direction: &str,
    limit: usize,
) -> anyhow::Result<Vec<String>> {
    let sql = format!(
        "SELECT turn_id FROM ( \
           SELECT turn_id, MIN(seq) AS first_seq FROM kernel_events \
           WHERE turn_id IS NOT NULL GROUP BY turn_id \
         ) ORDER BY first_seq {direction} LIMIT ?1"
    );
    let mut stmt = db.prepare(&sql)?;
    let rows = stmt.query_map(params![limit as i64], |r| {
        let s: String = r.get(0)?;
        Ok(s)
    })?;
    let mut ids = Vec::new();
    for row in rows {
        ids.push(row?);
    }
    Ok(ids)
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

    /// `diagnose_turn_ids` / `count_turn_ids_like` / `resolve_turn_id_alias` /
    /// `load_events_by_time_window` underpin the trace-export recovery path for
    /// historical turns whose engine-internal UUID is not persisted in runtime.db.
    #[tokio::test]
    async fn diagnose_and_recover_helpers_work() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path: PathBuf = dir.path().join("sessions.db");
        // `try_open` spawns a drain task; we only need the DB handle, so open a
        // raw connection and reuse the shared table helpers.
        {
            let db = Connection::open(&db_path).expect("open");
            ensure_kernel_events_table(&db).expect("ensure table");
            let mut log = KernelEventLog::new(&db);
            log.append(KernelEvent::TurnStarted {
                turn_id: "turn_aaaa1111".into(),
                mode: TurnLoopMode::Agent,
                input_text: "first".into(),
                max_steps: 5,
            })
            .expect("append t1 start");
            log.append(KernelEvent::TurnEnded {
                turn_id: "turn_aaaa1111".into(),
                outcome: TurnOutcome::Completed,
                total_steps: 1,
            })
            .expect("append t1 end");
            log.append(KernelEvent::TurnStarted {
                turn_id: "uuid-bbbb-2222".into(),
                mode: TurnLoopMode::Agent,
                input_text: "second".into(),
                max_steps: 5,
            })
            .expect("append t2 start");
            // Capture ts_ms of the last event for the time-window assertion.
        }
        let writer = KernelEventWriter::try_open(&db_path).expect("reopen writer");

        // diagnose_turn_ids: total = 3 (excludes schema_version NULL), earliest
        // distinct turn_ids ordered by first-seen seq.
        let (total, earliest, latest) = writer.diagnose_turn_ids(10).expect("diagnose");
        assert_eq!(total, 3);
        assert_eq!(earliest.first().map(String::as_str), Some("turn_aaaa1111"));
        assert!(latest.iter().any(|s| s == "uuid-bbbb-2222"));

        // count_turn_ids_like: only one `turn_…` prefix id.
        assert_eq!(writer.count_turn_ids_like("turn_%").expect("count"), 1);
        assert_eq!(writer.count_turn_ids_like("uuid-%").expect("count uuid"), 1);

        // resolve_turn_id_alias: exact runtime id matches a sessions.db id
        // directly (Case 2 reverse-prefix path, identity match).
        let alias = writer
            .resolve_turn_id_alias("turn_aaaa1111")
            .expect("resolve");
        assert!(alias.is_some());
        assert_eq!(alias.unwrap().0, "turn_aaaa1111");
        // No alias for an unknown id.
        assert!(
            writer
                .resolve_turn_id_alias("turn_does_not_exist")
                .expect("resolve missing")
                .is_none()
        );

        // load_events_by_time_window: a wide window returns all 3 events; a
        // window in the far past returns none.
        let all = writer
            .load_events_by_time_window(0, i64::MAX as u64)
            .expect("window all");
        assert_eq!(all.len(), 3);
        let none = writer
            .load_events_by_time_window(0, 1)
            .expect("window empty");
        assert!(none.is_empty());
    }
}
