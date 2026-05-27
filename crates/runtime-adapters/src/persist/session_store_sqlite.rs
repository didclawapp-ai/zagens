#![allow(dead_code)]
/// SQLite-backed session store. Provides the same semantics as the
/// JSON-per-file SessionManager but with far better I/O performance:
/// a single WAL sync per transaction instead of one fsync per file.
///
/// ## Async / blocking policy (A1.3)
///
/// - **TUI interactive path:** [`crate::tui::persistence_actor`] coalesces
///   checkpoint/session snapshots off the event-loop worker (latest-wins).
/// - **HTTP runtime path:** session persist runs inside `spawn_blocking`
///   (see `runtime_api::threads`).
/// - **Direct callers:** treat `save_session_sqlite` as blocking I/O; do not
///   call from async contexts without `spawn_blocking`.
use std::path::PathBuf;

use anyhow::{Context, bail};
use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};

use crate::persist::session_manager::{
    SavedSession, SessionContextReference, SessionMetadata,
};

const CURRENT_META_VERSION: u32 = 1;

fn ensure_sessions_runtime_thread_id_column(db: &Connection) -> anyhow::Result<()> {
    let mut stmt = db.prepare("PRAGMA table_info(sessions)")?;
    let has_col = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .filter_map(|r| r.ok())
        .any(|name| name == "runtime_thread_id");
    if !has_col {
        db.execute("ALTER TABLE sessions ADD COLUMN runtime_thread_id TEXT", [])?;
    }
    Ok(())
}

fn runtime_thread_id_sql(id: &Option<String>) -> &str {
    id.as_deref().unwrap_or("")
}

fn runtime_thread_id_from_sql(raw: &str) -> Option<String> {
    if raw.is_empty() {
        None
    } else {
        Some(raw.to_string())
    }
}

/// Opens (or creates) the SQLite DB at `db_path`.
/// If JSON files exist in `sessions_dir` and the DB is empty, auto-migrates.
pub fn open_sqlite_session_db(
    db_path: &std::path::Path,
    sessions_dir: &std::path::Path,
) -> anyhow::Result<Connection> {
    let db = Connection::open(db_path).context("Failed to open SQLite session DB")?;

    db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")
        .context("Failed to set SQLite pragmas")?;

    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS _meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS sessions (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL DEFAULT '',
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            message_count INTEGER NOT NULL DEFAULT 0,
            total_tokens INTEGER NOT NULL DEFAULT 0,
            model TEXT NOT NULL DEFAULT '',
            workspace TEXT NOT NULL DEFAULT '.',
            mode TEXT,
            system_prompt TEXT,
            messages_json TEXT NOT NULL DEFAULT '[]',
            context_refs_json TEXT NOT NULL DEFAULT '[]',
            runtime_thread_id TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_sessions_updated ON sessions(updated_at);
        CREATE INDEX IF NOT EXISTS idx_sessions_workspace ON sessions(workspace);",
    )
    .context("Failed to create session tables")?;

    ensure_sessions_runtime_thread_id_column(&db)?;

    // Check if migration is needed
    let needs_migration: bool = db
        .query_row(
            "SELECT value FROM _meta WHERE key = 'version'",
            [],
            |row| row.get::<_, String>(0),
        )
        .ok()
        .is_none();

    if needs_migration {
        migrate_json_sessions(&db, sessions_dir)?;
        db.execute(
            "INSERT OR REPLACE INTO _meta (key, value) VALUES ('version', ?1)",
            params![CURRENT_META_VERSION.to_string()],
        )?;
    }

    Ok(db)
}

fn migrate_json_sessions(db: &Connection, sessions_dir: &std::path::Path) -> anyhow::Result<()> {
    let dir = std::fs::read_dir(sessions_dir);
    let dir = match dir {
        Ok(d) => d,
        Err(_) => return Ok(()),
    };

    let tx = db.unchecked_transaction()?;

    for entry in dir.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let session: SavedSession = match serde_json::from_str(&content) {
            Ok(s) => s,
            Err(_) => continue,
        };

        let messages_json = serde_json::to_string(&session.messages).unwrap_or_default();
        let context_refs_json =
            serde_json::to_string(&session.context_references).unwrap_or_default();
        let created_at = session.metadata.created_at.to_rfc3339();
        let updated_at = session.metadata.updated_at.to_rfc3339();
        let mode = session.metadata.mode.as_deref().unwrap_or("");
        let workspace = session.metadata.workspace.display().to_string();
        let system_prompt = session.system_prompt.as_deref().unwrap_or("");
        let runtime_thread_id = runtime_thread_id_sql(&session.metadata.runtime_thread_id);

        tx.execute(
            "INSERT OR REPLACE INTO sessions
             (id, title, created_at, updated_at, message_count, total_tokens, model, workspace, mode, system_prompt, messages_json, context_refs_json, runtime_thread_id)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
            params![
                session.metadata.id,
                session.metadata.title,
                created_at,
                updated_at,
                session.metadata.message_count as i64,
                session.metadata.total_tokens as i64,
                session.metadata.model,
                workspace,
                mode,
                system_prompt,
                messages_json,
                context_refs_json,
                runtime_thread_id,
            ],
        )?;
    }

    tx.commit()?;
    eprintln!(
        "[session-store] migrated {} sessions to SQLite",
        db.query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get::<_, i64>(0))
            .unwrap_or(0)
    );
    Ok(())
}

pub fn save_session_sqlite(db: &Connection, session: &SavedSession) -> anyhow::Result<()> {
    let messages_json = serde_json::to_string(&session.messages).unwrap_or_default();
    let context_refs_json =
        serde_json::to_string(&session.context_references).unwrap_or_default();
    let created_at = session.metadata.created_at.to_rfc3339();
    let updated_at = session.metadata.updated_at.to_rfc3339();
    let mode = session.metadata.mode.as_deref().unwrap_or("");
    let workspace = session.metadata.workspace.display().to_string();
    let system_prompt = session.system_prompt.as_deref().unwrap_or("");
    let runtime_thread_id = runtime_thread_id_sql(&session.metadata.runtime_thread_id);

    db.execute(
        "INSERT OR REPLACE INTO sessions
         (id, title, created_at, updated_at, message_count, total_tokens, model, workspace, mode, system_prompt, messages_json, context_refs_json, runtime_thread_id)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
        params![
            session.metadata.id,
            session.metadata.title,
            created_at,
            updated_at,
            session.metadata.message_count as i64,
            session.metadata.total_tokens as i64,
            session.metadata.model,
            workspace,
            mode,
            system_prompt,
            messages_json,
            context_refs_json,
            runtime_thread_id,
        ],
    ).context("save_session_sqlite")?;

    // Enforce MAX_SESSIONS via LRU deletion
    cleanup_old_sqlite(db, 50)?;

    Ok(())
}

pub fn load_session_sqlite(db: &Connection, id: &str) -> anyhow::Result<SavedSession> {
    let id = id.trim();
    if id.is_empty() {
        bail!("Session id cannot be empty");
    }
    if !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        bail!("Invalid session id '{id}'");
    }

    let mut stmt = db.prepare(
        "SELECT id, title, created_at, updated_at, message_count, total_tokens, model, workspace, mode, system_prompt, messages_json, context_refs_json, runtime_thread_id
         FROM sessions WHERE id = ?1",
    )?;

    stmt.query_row(params![id], |row| {
        let id: String = row.get(0)?;
        let title: String = row.get(1)?;
        let created_at: String = row.get(2)?;
        let updated_at: String = row.get(3)?;
        let message_count: i64 = row.get(4)?;
        let total_tokens: i64 = row.get(5)?;
        let model: String = row.get(6)?;
        let workspace: String = row.get(7)?;
        let mode: String = row.get(8)?;
        let system_prompt: String = row.get(9)?;
        let messages_json: String = row.get(10)?;
        let context_refs_json: String = row.get(11)?;
        let runtime_thread_id_raw: String = row.get(12)?;

        let metadata = SessionMetadata {
            id,
            title,
            created_at: DateTime::parse_from_rfc3339(&created_at)
                .map(|d| d.with_timezone(&Utc))
                .unwrap_or_default(),
            updated_at: DateTime::parse_from_rfc3339(&updated_at)
                .map(|d| d.with_timezone(&Utc))
                .unwrap_or_default(),
            message_count: message_count as usize,
            total_tokens: total_tokens as u64,
            model,
            workspace: PathBuf::from(workspace),
            mode: if mode.is_empty() { None } else { Some(mode) },
            runtime_thread_id: runtime_thread_id_from_sql(&runtime_thread_id_raw),
        };
        let messages: Vec<crate::models::Message> =
            serde_json::from_str(&messages_json).unwrap_or_default();
        let context_references: Vec<SessionContextReference> =
            serde_json::from_str(&context_refs_json).unwrap_or_default();

        Ok(SavedSession {
            schema_version: 1,
            metadata,
            messages,
            system_prompt: if system_prompt.is_empty() {
                None
            } else {
                Some(system_prompt)
            },
            context_references,
        })
    })
    .map_err(|e| {
        if matches!(e, rusqlite::Error::QueryReturnedNoRows) {
            anyhow::anyhow!("session {id} not found")
        } else {
            anyhow::Error::from(e).context("load_session_sqlite query")
        }
    })
}

pub fn list_sessions_sqlite(db: &Connection) -> anyhow::Result<Vec<SessionMetadata>> {
    let mut stmt = db.prepare(
        "SELECT id, title, created_at, updated_at, message_count, total_tokens, model, workspace, mode, runtime_thread_id
         FROM sessions ORDER BY updated_at DESC",
    )?;

    let sessions = stmt
        .query_map([], |row| {
            let id: String = row.get(0)?;
            let title: String = row.get(1)?;
            let created_at: String = row.get(2)?;
            let updated_at: String = row.get(3)?;
            let message_count: i64 = row.get(4)?;
            let total_tokens: i64 = row.get(5)?;
            let model: String = row.get(6)?;
            let workspace: String = row.get(7)?;
            let mode: String = row.get(8)?;
            let runtime_thread_id_raw: String = row.get(9)?;

            Ok(SessionMetadata {
                id,
                title,
                created_at: DateTime::parse_from_rfc3339(&created_at)
                    .map(|d| d.with_timezone(&Utc))
                    .unwrap_or_default(),
                updated_at: DateTime::parse_from_rfc3339(&updated_at)
                    .map(|d| d.with_timezone(&Utc))
                    .unwrap_or_default(),
                message_count: message_count as usize,
                total_tokens: total_tokens as u64,
                model,
                workspace: PathBuf::from(workspace),
                mode: if mode.is_empty() { None } else { Some(mode) },
                runtime_thread_id: runtime_thread_id_from_sql(&runtime_thread_id_raw),
            })
        })?
        .filter_map(|r| r.ok())
        .collect();

    Ok(sessions)
}

pub fn search_sessions_sqlite(db: &Connection, query: &str) -> anyhow::Result<Vec<SessionMetadata>> {
    let all = list_sessions_sqlite(db)?;
    let query_lower = query.to_lowercase();
    Ok(all
        .into_iter()
        .filter(|s| s.title.to_lowercase().contains(&query_lower))
        .collect())
}

pub fn delete_session_sqlite(db: &Connection, id: &str) -> anyhow::Result<()> {
    let id = id.trim();
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        bail!("Invalid session id '{id}'");
    }
    let affected = db.execute("DELETE FROM sessions WHERE id = ?1", params![id]).context("delete_session_sqlite")?;
    if affected == 0 {
        bail!("session {id} not found");
    }
    Ok(())
}

pub fn get_latest_session_for_workspace_sqlite(
    db: &Connection,
    workspace: &std::path::Path,
) -> anyhow::Result<Option<SessionMetadata>> {
    let workspace_str = workspace.display().to_string();
    // Match by path prefix equality (same as JSON version's workspace_scope_matches)
    let mut stmt = db.prepare(
        "SELECT id, title, created_at, updated_at, message_count, total_tokens, model, workspace, mode, runtime_thread_id
         FROM sessions WHERE workspace = ?1
         ORDER BY updated_at DESC LIMIT 1",
    )?;

    let result = stmt.query_row(params![workspace_str], |row| {
        let id: String = row.get(0)?;
        let title: String = row.get(1)?;
        let created_at: String = row.get(2)?;
        let updated_at: String = row.get(3)?;
        let message_count: i64 = row.get(4)?;
        let total_tokens: i64 = row.get(5)?;
        let model: String = row.get(6)?;
        let workspace: String = row.get(7)?;
        let mode: String = row.get(8)?;
        let runtime_thread_id_raw: String = row.get(9)?;

        Ok(SessionMetadata {
            id,
            title,
            created_at: DateTime::parse_from_rfc3339(&created_at)
                .map(|d| d.with_timezone(&Utc))
                .unwrap_or_default(),
            updated_at: DateTime::parse_from_rfc3339(&updated_at)
                .map(|d| d.with_timezone(&Utc))
                .unwrap_or_default(),
            message_count: message_count as usize,
            total_tokens: total_tokens as u64,
            model,
            workspace: PathBuf::from(workspace),
            mode: if mode.is_empty() { None } else { Some(mode) },
            runtime_thread_id: runtime_thread_id_from_sql(&runtime_thread_id_raw),
        })
    });

    match result {
        Ok(meta) => Ok(Some(meta)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(anyhow::anyhow!("query error: {e}")),
    }
}

fn cleanup_old_sqlite(db: &Connection, max_sessions: usize) -> anyhow::Result<()> {
    // Delete oldest sessions beyond the limit
    db.execute(
        "DELETE FROM sessions WHERE id NOT IN (
            SELECT id FROM sessions ORDER BY updated_at DESC LIMIT ?1
        )",
        params![max_sessions as i64],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use tempfile::tempdir;

    #[test]
    fn sqlite_session_runtime_thread_id_round_trip() {
        let dir = tempdir().unwrap();
        let sessions_dir = dir.path().join("sessions");
        std::fs::create_dir_all(&sessions_dir).unwrap();
        let db_path = sessions_dir.join("sessions.db");
        let db = open_sqlite_session_db(&db_path, &sessions_dir).unwrap();

        let now = Utc::now();
        let session = SavedSession {
            schema_version: 1,
            metadata: SessionMetadata {
                id: "sess-1".to_string(),
                title: "test".to_string(),
                created_at: now,
                updated_at: now,
                message_count: 0,
                total_tokens: 0,
                model: "m".to_string(),
                workspace: PathBuf::from("."),
                mode: None,
                runtime_thread_id: Some("thr_abc".to_string()),
            },
            messages: vec![],
            system_prompt: None,
            context_references: vec![],
        };

        save_session_sqlite(&db, &session).unwrap();
        let loaded = load_session_sqlite(&db, "sess-1").unwrap();
        assert_eq!(
            loaded.metadata.runtime_thread_id.as_deref(),
            Some("thr_abc")
        );

        let listed = list_sessions_sqlite(&db).unwrap();
        assert_eq!(
            listed[0].runtime_thread_id.as_deref(),
            Some("thr_abc")
        );
    }

    #[test]
    fn sqlite_alter_adds_runtime_thread_id_column() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("legacy.db");
        {
            let db = Connection::open(&db_path).unwrap();
            db.execute_batch(
                "CREATE TABLE sessions (
                    id TEXT PRIMARY KEY,
                    title TEXT NOT NULL DEFAULT '',
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    message_count INTEGER NOT NULL DEFAULT 0,
                    total_tokens INTEGER NOT NULL DEFAULT 0,
                    model TEXT NOT NULL DEFAULT '',
                    workspace TEXT NOT NULL DEFAULT '.',
                    mode TEXT,
                    system_prompt TEXT,
                    messages_json TEXT NOT NULL DEFAULT '[]',
                    context_refs_json TEXT NOT NULL DEFAULT '[]'
                );",
            )
            .unwrap();
        }
        let db = Connection::open(&db_path).unwrap();
        ensure_sessions_runtime_thread_id_column(&db).unwrap();
        ensure_sessions_runtime_thread_id_column(&db).unwrap();
    }
}
