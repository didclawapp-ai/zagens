//! SQLite persistence for compaction artifacts (kernel-v2 Phase 2-C).
//!
//! The `compaction_artifacts` table is **additive-only** — older binaries that
//! do not import this module simply ignore the table.  Columns are never
//! removed; new optional columns may be added via `ALTER TABLE … ADD COLUMN`
//! migrations following the same pattern as `ensure_sessions_runtime_thread_id_column`.
//!
//! ## Table schema
//!
//! ```sql
//! CREATE TABLE IF NOT EXISTS compaction_artifacts (
//!     id               TEXT    PRIMARY KEY,
//!     session_id       TEXT    NOT NULL,
//!     created_at_ms    INTEGER NOT NULL,
//!     replaced_start   INTEGER NOT NULL,
//!     replaced_end     INTEGER NOT NULL,
//!     replaced_messages_json TEXT NOT NULL DEFAULT '[]',
//!     summary          TEXT    NOT NULL DEFAULT '',
//!     original_tokens  INTEGER NOT NULL DEFAULT 0,
//!     summary_tokens   INTEGER NOT NULL DEFAULT 0
//! );
//! ```
//!
//! ## Usage
//!
//! 1. Call [`ensure_compaction_artifacts_table`] once after opening the DB.
//! 2. Call [`save_compaction_artifact`] after each successful compaction.
//! 3. Call [`load_compaction_artifacts`] to read all artifacts for a session
//!    (ordered oldest → newest).
//! 4. Call [`delete_compaction_artifacts_for_session`] when deleting a session.

use anyhow::Context;
use rusqlite::{Connection, params};
use zagens_core::compaction::CompactionArtifact;

/// Ensures the `compaction_artifacts` table exists in `db`.
///
/// Safe to call on every DB open — `CREATE TABLE IF NOT EXISTS` is idempotent.
pub fn ensure_compaction_artifacts_table(db: &Connection) -> anyhow::Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS compaction_artifacts (
            id                     TEXT    PRIMARY KEY,
            session_id             TEXT    NOT NULL,
            created_at_ms          INTEGER NOT NULL,
            replaced_start         INTEGER NOT NULL,
            replaced_end           INTEGER NOT NULL,
            replaced_messages_json TEXT    NOT NULL DEFAULT '[]',
            summary                TEXT    NOT NULL DEFAULT '',
            original_tokens        INTEGER NOT NULL DEFAULT 0,
            summary_tokens         INTEGER NOT NULL DEFAULT 0
        );
        CREATE INDEX IF NOT EXISTS idx_artifacts_session_time
            ON compaction_artifacts(session_id, created_at_ms);",
    )
    .context("Failed to create compaction_artifacts table")
}

/// Persist one [`CompactionArtifact`] to `db`.
///
/// On conflict (same `id`) replaces the existing row — this is a no-op in
/// practice since artifact IDs are UUIDs generated at creation time.
pub fn save_compaction_artifact(
    db: &Connection,
    artifact: &CompactionArtifact,
) -> anyhow::Result<()> {
    db.execute(
        "INSERT OR REPLACE INTO compaction_artifacts
             (id, session_id, created_at_ms, replaced_start, replaced_end,
              replaced_messages_json, summary, original_tokens, summary_tokens)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            artifact.id,
            artifact.session_id,
            artifact.created_at_ms,
            artifact.replaced_start as i64,
            artifact.replaced_end as i64,
            artifact.replaced_messages_json,
            artifact.summary,
            artifact.original_tokens as i64,
            artifact.summary_tokens as i64,
        ],
    )
    .context("Failed to save compaction artifact")?;
    Ok(())
}

/// Load all artifacts for `session_id`, ordered oldest → newest.
pub fn load_compaction_artifacts(
    db: &Connection,
    session_id: &str,
) -> anyhow::Result<Vec<CompactionArtifact>> {
    let mut stmt = db.prepare(
        "SELECT id, session_id, created_at_ms, replaced_start, replaced_end,
                replaced_messages_json, summary, original_tokens, summary_tokens
         FROM compaction_artifacts
         WHERE session_id = ?1
         ORDER BY created_at_ms ASC",
    )?;
    let rows = stmt.query_map(params![session_id], |row| {
        Ok(CompactionArtifact {
            id: row.get(0)?,
            session_id: row.get(1)?,
            created_at_ms: row.get(2)?,
            replaced_start: row.get::<_, i64>(3)? as usize,
            replaced_end: row.get::<_, i64>(4)? as usize,
            replaced_messages_json: row.get(5)?,
            summary: row.get(6)?,
            original_tokens: row.get::<_, i64>(7)? as u32,
            summary_tokens: row.get::<_, i64>(8)? as u32,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>()
        .context("Failed to load compaction artifacts")
}

/// Delete all artifacts for `session_id` (call when deleting a session).
pub fn delete_compaction_artifacts_for_session(
    db: &Connection,
    session_id: &str,
) -> anyhow::Result<()> {
    db.execute(
        "DELETE FROM compaction_artifacts WHERE session_id = ?1",
        params![session_id],
    )
    .context("Failed to delete compaction artifacts")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        ensure_compaction_artifacts_table(&db).unwrap();
        db
    }

    fn sample_artifact(id: &str, session_id: &str, start: usize, end: usize) -> CompactionArtifact {
        CompactionArtifact {
            id: id.to_string(),
            session_id: session_id.to_string(),
            created_at_ms: 1_000_000,
            replaced_start: start,
            replaced_end: end,
            replaced_messages_json: "[{\"role\":\"user\",\"content\":[]}]".to_string(),
            summary: "summary text".to_string(),
            original_tokens: 500,
            summary_tokens: 50,
        }
    }

    #[test]
    fn save_and_load_roundtrip() {
        let db = test_db();
        let art = sample_artifact("art-1", "sess-a", 0, 4);
        save_compaction_artifact(&db, &art).unwrap();

        let loaded = load_compaction_artifacts(&db, "sess-a").unwrap();
        assert_eq!(loaded.len(), 1);
        let l = &loaded[0];
        assert_eq!(l.id, "art-1");
        assert_eq!(l.session_id, "sess-a");
        assert_eq!(l.replaced_start, 0);
        assert_eq!(l.replaced_end, 4);
        assert_eq!(l.summary, "summary text");
        assert_eq!(l.original_tokens, 500);
        assert_eq!(l.summary_tokens, 50);
    }

    #[test]
    fn load_returns_empty_for_unknown_session() {
        let db = test_db();
        let result = load_compaction_artifacts(&db, "nonexistent").unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn multiple_artifacts_ordered_oldest_first() {
        let db = test_db();
        let mut art2 = sample_artifact("art-2", "sess-b", 5, 9);
        art2.created_at_ms = 2_000_000;
        let mut art1 = sample_artifact("art-1", "sess-b", 0, 4);
        art1.created_at_ms = 1_000_000;
        // insert out of order
        save_compaction_artifact(&db, &art2).unwrap();
        save_compaction_artifact(&db, &art1).unwrap();

        let loaded = load_compaction_artifacts(&db, "sess-b").unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].id, "art-1"); // oldest first
        assert_eq!(loaded[1].id, "art-2");
    }

    #[test]
    fn delete_removes_only_target_session() {
        let db = test_db();
        save_compaction_artifact(&db, &sample_artifact("art-a1", "sess-a", 0, 3)).unwrap();
        save_compaction_artifact(&db, &sample_artifact("art-b1", "sess-b", 0, 3)).unwrap();

        delete_compaction_artifacts_for_session(&db, "sess-a").unwrap();

        assert!(load_compaction_artifacts(&db, "sess-a").unwrap().is_empty());
        assert_eq!(load_compaction_artifacts(&db, "sess-b").unwrap().len(), 1);
    }

    /// **P2-C reversibility gate.**
    ///
    /// Given a compaction artifact, we can recover the original messages from
    /// `replaced_messages_json` and reconstruct the full session.
    #[test]
    fn reversibility_original_messages_preserved() {
        let db = test_db();

        // Simulate: 6 messages [0..6], messages 0..4 summarised, message 5 pinned.
        let original_msgs_json = serde_json::json!([
            {"role": "user", "content": [{"type": "text", "text": "hello"}]},
            {"role": "assistant", "content": [{"type": "text", "text": "hi"}]},
            {"role": "user", "content": [{"type": "text", "text": "how are you?"}]},
            {"role": "assistant", "content": [{"type": "text", "text": "great"}]},
        ])
        .to_string();

        let art = CompactionArtifact {
            id: "rev-test".to_string(),
            session_id: "sess-rev".to_string(),
            created_at_ms: 999,
            replaced_start: 0,
            replaced_end: 4,
            replaced_messages_json: original_msgs_json.clone(),
            summary: "User greeted, assistant responded warmly.".to_string(),
            original_tokens: 40,
            summary_tokens: 8,
        };

        save_compaction_artifact(&db, &art).unwrap();
        let loaded = load_compaction_artifacts(&db, "sess-rev").unwrap();
        let recovered = &loaded[0];

        // Verify: the full original messages can be recovered
        let recovered_msgs: serde_json::Value =
            serde_json::from_str(&recovered.replaced_messages_json).unwrap();
        assert_eq!(
            recovered_msgs.as_array().unwrap().len(),
            4,
            "should recover all 4 original messages"
        );
        // First message still intact
        assert_eq!(
            recovered_msgs[0]["content"][0]["text"], "hello",
            "original message content preserved"
        );

        // Verify artifact range metadata
        assert_eq!(recovered.replaced_count(), 4);
        assert!(recovered.covers_index(0));
        assert!(recovered.covers_index(3));
        assert!(!recovered.covers_index(4));
    }
}
