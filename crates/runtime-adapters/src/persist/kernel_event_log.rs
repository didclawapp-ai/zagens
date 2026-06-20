//! Append-only SQLite log for [`KernelEvent`] records — Phase 3a double-write.
//!
//! ## Purpose
//! Written by the double-write adapter during Phase 3a; **not yet read** by any
//! production code path.  Phase 3b will add the projection/replay consumer.
//!
//! ## Schema (additive, backward-compatible)
//! ```sql
//! CREATE TABLE IF NOT EXISTS kernel_events (
//!     id      INTEGER PRIMARY KEY AUTOINCREMENT,
//!     seq     INTEGER NOT NULL,
//!     ts_ms   INTEGER NOT NULL,
//!     kind    TEXT    NOT NULL,
//!     turn_id TEXT,           -- NULL only for schema_version sentinel
//!     payload TEXT    NOT NULL -- JSON-serialised KernelEvent
//! );
//! ```
//! Adding columns in future schema versions: `ALTER TABLE ADD COLUMN … DEFAULT …`.
//! Removing columns: increment schema_version and provide an upcast script.
//!
//! ## WAL / performance
//! Inherits the WAL-mode connection from `open_sqlite_session_db`.  Each
//! [`KernelEventLog::append`] call is a single `INSERT`; callers may batch via
//! `append_batch` to amortise transaction overhead.

use anyhow::Context as _;
use rusqlite::{Connection, params};
use zagens_core::engine::kernel_event::{KernelEvent, KernelEventEnvelope};

// ── Schema migration ─────────────────────────────────────────────────────────

/// Ensure the `kernel_events` table and index exist.
/// Safe to call on every startup — uses `CREATE … IF NOT EXISTS`.
pub fn ensure_kernel_events_table(db: &Connection) -> anyhow::Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS kernel_events (
            id      INTEGER PRIMARY KEY AUTOINCREMENT,
            seq     INTEGER NOT NULL,
            ts_ms   INTEGER NOT NULL,
            kind    TEXT    NOT NULL,
            turn_id TEXT,
            payload TEXT    NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_kernel_events_turn
            ON kernel_events(turn_id);
        CREATE INDEX IF NOT EXISTS idx_kernel_events_seq
            ON kernel_events(seq);",
    )
    .context("Failed to create kernel_events table")
}

// ── Log writer ───────────────────────────────────────────────────────────────

/// Append-only writer for the `kernel_events` table.
///
/// Maintains a monotone `seq` counter per [`KernelEventLog`] instance
/// (per-session / per-sidecar lifetime).  The counter is **not** persisted
/// across restarts; callers that need stable cross-restart ordering should
/// use the SQLite `id` column (autoincrement) instead.
pub struct KernelEventLog<'conn> {
    db: &'conn Connection,
    next_seq: u64,
}

impl<'conn> KernelEventLog<'conn> {
    /// Create a writer that shares the given (already-migrated) connection.
    /// Call [`ensure_kernel_events_table`] before constructing this.
    ///
    /// Starts `seq` at 0 — suitable for empty in-memory test DBs. Production
    /// callers should prefer [`Self::with_next_seq`] after [`peek_next_seq`].
    pub fn new(db: &'conn Connection) -> Self {
        Self { db, next_seq: 0 }
    }

    /// Resume appending after restart using the next free sequence number.
    pub fn with_next_seq(db: &'conn Connection, next_seq: u64) -> Self {
        Self { db, next_seq }
    }

    /// Next `seq` value to assign (max existing + 1, or 0 when empty).
    pub fn peek_next_seq(db: &Connection) -> anyhow::Result<u64> {
        let max: i64 = db.query_row(
            "SELECT COALESCE(MAX(seq), -1) FROM kernel_events",
            [],
            |row| row.get(0),
        )?;
        Ok(max.saturating_add(1).max(0) as u64)
    }

    /// Append one event.  The `ts_ms` is the Unix time in milliseconds at
    /// call time.
    pub fn append(&mut self, event: KernelEvent) -> anyhow::Result<()> {
        let seq = self.next_seq;
        self.next_seq += 1;
        let ts_ms = unix_ms_now();
        let kind = event.kind_str().to_string();
        let turn_id = event.turn_id().map(str::to_string);
        let payload = serde_json::to_string(&event).context("KernelEvent serialization failed")?;
        self.db
            .execute(
                "INSERT INTO kernel_events (seq, ts_ms, kind, turn_id, payload)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![seq, ts_ms, kind, turn_id, payload],
            )
            .context("INSERT INTO kernel_events failed")?;
        Ok(())
    }

    /// Append multiple events in a single transaction.
    pub fn append_batch(
        &mut self,
        events: impl IntoIterator<Item = KernelEvent>,
    ) -> anyhow::Result<()> {
        let tx_result: anyhow::Result<()> = (|| {
            self.db
                .execute_batch("BEGIN")
                .context("BEGIN transaction")?;
            for ev in events {
                self.append(ev)?;
            }
            self.db
                .execute_batch("COMMIT")
                .context("COMMIT transaction")?;
            Ok(())
        })();
        if tx_result.is_err() {
            let _ = self.db.execute_batch("ROLLBACK");
        }
        tx_result
    }

    /// Load all events for a given `turn_id` in sequence order.
    /// Used by Phase 3b replay and completeness verification tests.
    pub fn load_turn_events(&self, turn_id: &str) -> anyhow::Result<Vec<KernelEventEnvelope>> {
        let mut stmt = self.db.prepare(
            "SELECT seq, ts_ms, kind, payload FROM kernel_events
             WHERE turn_id = ?1 ORDER BY seq ASC",
        )?;
        let rows = stmt.query_map(params![turn_id], |row| {
            Ok((
                row.get::<_, u64>(0)?,
                row.get::<_, u64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;
        let mut envelopes = Vec::new();
        for row in rows {
            let (seq, ts_ms, kind, payload) = row.context("row read")?;
            let event: KernelEvent =
                serde_json::from_str(&payload).context("KernelEvent deserialize")?;
            envelopes.push(KernelEventEnvelope {
                seq,
                ts_ms,
                kind,
                event,
            });
        }
        Ok(envelopes)
    }

    /// Load all events whose `ts_ms` falls within `[from_ms, to_ms]` inclusive,
    /// ordered by `seq ASC`. Used by trace-export to recover historical turns
    /// whose engine-internal `turn_id` (UUID) is not persisted in runtime.db
    /// (pre-fix data, see CHANGELOG [Unreleased] trace export fix).
    pub fn load_events_by_time_window(
        &self,
        from_ms: u64,
        to_ms: u64,
    ) -> anyhow::Result<Vec<KernelEventEnvelope>> {
        let mut stmt = self.db.prepare(
            "SELECT seq, ts_ms, kind, payload FROM kernel_events
             WHERE ts_ms BETWEEN ?1 AND ?2 ORDER BY seq ASC",
        )?;
        let rows = stmt.query_map(params![from_ms, to_ms], |row| {
            Ok((
                row.get::<_, u64>(0)?,
                row.get::<_, u64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;
        let mut envelopes = Vec::new();
        for row in rows {
            let (seq, ts_ms, kind, payload) = row.context("row read")?;
            let event: KernelEvent =
                serde_json::from_str(&payload).context("KernelEvent deserialize")?;
            envelopes.push(KernelEventEnvelope {
                seq,
                ts_ms,
                kind,
                event,
            });
        }
        Ok(envelopes)
    }
}

fn unix_ms_now() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use rusqlite::Connection;
    use zagens_core::engine::kernel_event::{KernelEvent, TurnOutcome};
    use zagens_core::turn::TurnLoopMode;

    use super::*;

    fn in_memory_db() -> Connection {
        let db = Connection::open_in_memory().expect("in-memory DB");
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")
            .expect("pragmas");
        ensure_kernel_events_table(&db).expect("ensure table");
        db
    }

    #[test]
    fn append_and_load_round_trip() {
        let db = in_memory_db();
        let mut log = KernelEventLog::new(&db);
        let tid = "t-rt-001".to_string();

        log.append(KernelEvent::TurnStarted {
            turn_id: tid.clone(),
            mode: TurnLoopMode::Agent,
            input_text: "do the thing".into(),
            max_steps: 10,
        })
        .expect("append TurnStarted");

        log.append(KernelEvent::TurnEnded {
            turn_id: tid.clone(),
            outcome: TurnOutcome::Completed,
            total_steps: 1,
        })
        .expect("append TurnEnded");

        let envelopes = log.load_turn_events(&tid).expect("load");
        assert_eq!(envelopes.len(), 2);
        assert_eq!(envelopes[0].event.kind_str(), "turn_started");
        assert_eq!(envelopes[1].event.kind_str(), "turn_ended");
        // Sequence is monotone.
        assert!(envelopes[0].seq < envelopes[1].seq);
    }

    #[test]
    fn append_batch_is_atomic() {
        let db = in_memory_db();
        let mut log = KernelEventLog::new(&db);
        let tid = "t-batch-001".to_string();

        let events = vec![
            KernelEvent::TurnStarted {
                turn_id: tid.clone(),
                mode: TurnLoopMode::Yolo,
                input_text: "batch test".into(),
                max_steps: 5,
            },
            KernelEvent::ScratchpadSummaryInjected {
                turn_id: tid.clone(),
                at_step: 2,
            },
            KernelEvent::TurnEnded {
                turn_id: tid.clone(),
                outcome: TurnOutcome::Interrupted,
                total_steps: 3,
            },
        ];

        log.append_batch(events).expect("batch");

        let envelopes = log.load_turn_events(&tid).expect("load");
        assert_eq!(envelopes.len(), 3);
    }

    #[test]
    fn schema_version_event_has_null_turn_id() {
        let db = in_memory_db();
        let mut log = KernelEventLog::new(&db);

        // schema_version has no turn_id — stored as NULL.
        log.append(KernelEvent::SchemaVersion { version: 1 })
            .expect("schema version event");

        // Verify via direct SQL that turn_id IS NULL.
        let turn_id_val: Option<String> = db
            .query_row(
                "SELECT turn_id FROM kernel_events WHERE kind = 'schema_version'",
                [],
                |row| row.get(0),
            )
            .expect("query");
        assert!(turn_id_val.is_none());
    }

    #[test]
    fn ensure_table_is_idempotent() {
        let db = in_memory_db();
        // Calling twice should not error.
        ensure_kernel_events_table(&db).expect("second call");
    }
}
