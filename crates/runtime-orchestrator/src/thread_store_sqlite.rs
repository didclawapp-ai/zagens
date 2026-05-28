#![allow(dead_code)]
/// SQLite-backed thread/turn/item store. Replaces the JSON-per-file pattern
/// with WAL-mode SQLite for dramatically better I/O performance during seed
/// operations (750+ atomic writes → single transaction).
///
/// ## Async / blocking policy (A1.3)
///
/// Runtime HTTP paths call these functions from `spawn_blocking` (event append,
/// turn/item/thread saves). WAL + `synchronous=NORMAL` keeps fsync off the
/// tokio worker; see [`docs/tech/adr/RUNTIME_BASELINE.md`](../../../docs/tech/adr/RUNTIME_BASELINE.md)
/// § crash-safe checkpoint.

use std::path::PathBuf;

use anyhow::Context;
use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};

use crate::models::Usage;
use crate::runtime_threads::{
    RuntimeEventRecord, RuntimeStoreState, RuntimeTurnStatus, ThreadRecord,
    TurnItemKind, TurnItemLifecycleStatus, TurnItemRecord, TurnRecord,
    UsageAggregation, UsageBucket, UsageGroupBy, UsageTotals,
};

const CURRENT_META_VERSION: u32 = 1;

fn ensure_threads_scratchpad_run_id_column(db: &Connection) -> anyhow::Result<()> {
    let mut stmt = db.prepare("PRAGMA table_info(threads)")?;
    let has_col = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .filter_map(|r| r.ok())
        .any(|name| name == "scratchpad_run_id");
    if !has_col {
        db.execute("ALTER TABLE threads ADD COLUMN scratchpad_run_id TEXT", [])?;
    }
    Ok(())
}

fn ensure_threads_scratchpad_run_history_column(db: &Connection) -> anyhow::Result<()> {
    let mut stmt = db.prepare("PRAGMA table_info(threads)")?;
    let has_col = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .filter_map(|r| r.ok())
        .any(|name| name == "scratchpad_run_history_json");
    if !has_col {
        db.execute("ALTER TABLE threads ADD COLUMN scratchpad_run_history_json TEXT", [])?;
    }
    Ok(())
}

fn ensure_threads_checklist_json_column(db: &Connection) -> anyhow::Result<()> {
    let mut stmt = db.prepare("PRAGMA table_info(threads)")?;
    let has_col = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .filter_map(|r| r.ok())
        .any(|name| name == "checklist_json");
    if !has_col {
        db.execute("ALTER TABLE threads ADD COLUMN checklist_json TEXT", [])?;
    }
    Ok(())
}

fn ensure_threads_task_type_column(db: &Connection) -> anyhow::Result<()> {
    let mut stmt = db.prepare("PRAGMA table_info(threads)")?;
    let has_col = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .filter_map(|r| r.ok())
        .any(|name| name == "task_type");
    if !has_col {
        db.execute(
            "ALTER TABLE threads ADD COLUMN task_type TEXT NOT NULL DEFAULT 'code'",
            [],
        )?;
    }
    Ok(())
}

fn ensure_turns_last_request_input_tokens_column(db: &Connection) -> anyhow::Result<()> {
    let mut stmt = db.prepare("PRAGMA table_info(turns)")?;
    let has_col = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .filter_map(|r| r.ok())
        .any(|name| name == "last_request_input_tokens");
    if !has_col {
        db.execute("ALTER TABLE turns ADD COLUMN last_request_input_tokens INTEGER", [])?;
    }
    Ok(())
}

pub fn open_sqlite_thread_db(
    db_path: &std::path::Path,
    threads_dir: &std::path::Path,
) -> anyhow::Result<(Connection, RuntimeStoreState)> {
    let db = Connection::open(db_path).context("Failed to open SQLite runtime DB")?;

    db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")
        .context("Failed to set SQLite pragmas")?;

    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS _meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS threads (
            id TEXT PRIMARY KEY,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            model TEXT NOT NULL DEFAULT '',
            workspace TEXT NOT NULL DEFAULT '.',
            mode TEXT NOT NULL DEFAULT 'agent',
            allow_shell INTEGER NOT NULL DEFAULT 0,
            trust_mode INTEGER NOT NULL DEFAULT 0,
            auto_approve INTEGER NOT NULL DEFAULT 0,
            latest_turn_id TEXT,
            latest_response_bookmark TEXT,
            archived INTEGER NOT NULL DEFAULT 0,
            system_prompt TEXT,
            task_id TEXT,
            title TEXT,
            coherence_state_json TEXT NOT NULL DEFAULT '{}',
            task_type TEXT NOT NULL DEFAULT 'code'
        );
        CREATE TABLE IF NOT EXISTS turns (
            id TEXT PRIMARY KEY,
            thread_id TEXT NOT NULL,
            status TEXT NOT NULL,
            input_summary TEXT NOT NULL DEFAULT '',
            created_at TEXT NOT NULL,
            started_at TEXT,
            ended_at TEXT,
            duration_ms INTEGER,
            usage_json TEXT,
            error TEXT,
            item_ids_json TEXT NOT NULL DEFAULT '[]',
            steer_count INTEGER NOT NULL DEFAULT 0
        );
        CREATE INDEX IF NOT EXISTS idx_turns_thread_id ON turns(thread_id);
        CREATE INDEX IF NOT EXISTS idx_turns_status ON turns(status);
        CREATE TABLE IF NOT EXISTS items (
            id TEXT PRIMARY KEY,
            turn_id TEXT NOT NULL,
            kind TEXT NOT NULL,
            status TEXT NOT NULL,
            summary TEXT NOT NULL DEFAULT '',
            detail TEXT,
            metadata_json TEXT,
            artifact_refs_json TEXT NOT NULL DEFAULT '[]',
            started_at TEXT,
            ended_at TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_items_turn_id ON items(turn_id);
        CREATE TABLE IF NOT EXISTS events (
            seq INTEGER PRIMARY KEY,
            timestamp TEXT NOT NULL,
            thread_id TEXT NOT NULL,
            turn_id TEXT,
            item_id TEXT,
            event TEXT NOT NULL,
            payload_json TEXT NOT NULL DEFAULT '{}'
        );
        CREATE INDEX IF NOT EXISTS idx_events_thread_id ON events(thread_id);
        CREATE INDEX IF NOT EXISTS idx_events_seq ON events(seq);",
    )
    .context("Failed to create runtime tables")?;
    ensure_threads_task_type_column(&db)?;
    ensure_threads_scratchpad_run_id_column(&db)?;
    ensure_threads_scratchpad_run_history_column(&db)?;
    ensure_threads_checklist_json_column(&db)?;
    ensure_turns_last_request_input_tokens_column(&db)?;

    let needs_migration: bool = db
        .query_row(
            "SELECT value FROM _meta WHERE key = 'version'",
            [],
            |row| row.get::<_, String>(0),
        )
        .ok()
        .is_none();

    let state = if needs_migration {
        let state = migrate_json_threads(&db, threads_dir)?;
        db.execute(
            "INSERT OR REPLACE INTO _meta (key, value) VALUES ('version', ?1)",
            params![CURRENT_META_VERSION.to_string()],
        )?;
        state
    } else {
        // Load state from DB
        let next_seq: i64 = db
            .query_row(
                "SELECT MAX(seq) + 1 FROM events",
                [],
                |row| row.get(0),
            )
            .unwrap_or(1);
        RuntimeStoreState {
            schema_version: 2,
            next_seq: next_seq.max(1) as u64,
        }
    };

    Ok((db, state))
}

fn migrate_json_threads(
    db: &Connection,
    threads_dir: &std::path::Path,
) -> anyhow::Result<RuntimeStoreState> {
    let dir = match std::fs::read_dir(threads_dir) {
        Ok(d) => d,
        Err(_) => return Ok(RuntimeStoreState { schema_version: 2, next_seq: 1 }),
    };

    // threads_dir is <root>/threads; parent is <root>/
    let root = threads_dir.parent().unwrap_or(threads_dir);
    let turns_dir = root.join("turns");
    let items_dir = root.join("items");
    let events_dir = root.join("events");
    let state_path = root.join("state.json");

    let tx = db.unchecked_transaction()?;

    // Migrate threads
    let mut migrated_threads = 0usize;
    for entry in dir.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let thread: ThreadRecord = match serde_json::from_str(&content) {
            Ok(t) => t,
            Err(_) => continue,
        };
        let coherence_json = serde_json::to_string(&thread.coherence_state).unwrap_or_default();
        tx.execute(
            "INSERT OR REPLACE INTO threads
             (id, created_at, updated_at, model, workspace, mode, allow_shell, trust_mode, auto_approve, latest_turn_id, latest_response_bookmark, archived, system_prompt, task_id, title, coherence_state_json, task_type)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)",
            params![
                thread.id,
                thread.created_at.to_rfc3339(),
                thread.updated_at.to_rfc3339(),
                thread.model,
                thread.workspace.display().to_string(),
                thread.mode,
                thread.allow_shell as i32,
                thread.trust_mode as i32,
                thread.auto_approve as i32,
                thread.latest_turn_id,
                thread.latest_response_bookmark,
                thread.archived as i32,
                thread.system_prompt,
                thread.task_id,
                thread.title,
                coherence_json,
                thread.task_type,
            ],
        )?;
        migrated_threads += 1;
    }

    // Migrate turns
    let mut migrated_turns = 0usize;
    if turns_dir.is_dir() {
        for entry in std::fs::read_dir(&turns_dir).into_iter().flatten().filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let turn: TurnRecord = match serde_json::from_str(&content) {
                Ok(t) => t,
                Err(_) => continue,
            };
            let usage_json = turn.usage.as_ref().map(|u| serde_json::to_string(u).unwrap_or_default());
            let item_ids_json = serde_json::to_string(&turn.item_ids).unwrap_or_default();
            tx.execute(
                "INSERT OR REPLACE INTO turns
                 (id, thread_id, status, input_summary, created_at, started_at, ended_at, duration_ms, usage_json, error, item_ids_json, steer_count)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
                params![
                    turn.id,
                    turn.thread_id,
                    turn.status.to_string(),
                    turn.input_summary,
                    turn.created_at.to_rfc3339(),
                    turn.started_at.map(|t| t.to_rfc3339()),
                    turn.ended_at.map(|t| t.to_rfc3339()),
                    turn.duration_ms.map(|d| d as i64),
                    usage_json,
                    turn.error,
                    item_ids_json,
                    turn.steer_count as i64,
                ],
            )?;
            migrated_turns += 1;
        }
    }

    // Migrate items
    let mut migrated_items = 0usize;
    if items_dir.is_dir() {
        for entry in std::fs::read_dir(&items_dir).into_iter().flatten().filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let item: TurnItemRecord = match serde_json::from_str(&content) {
                Ok(i) => i,
                Err(_) => continue,
            };
            let metadata_json = item.metadata.map(|v| serde_json::to_string(&v).unwrap_or_default());
            let artifact_refs_json = serde_json::to_string(&item.artifact_refs).unwrap_or_default();
            tx.execute(
                "INSERT OR REPLACE INTO items
                 (id, turn_id, kind, status, summary, detail, metadata_json, artifact_refs_json, started_at, ended_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                params![
                    item.id,
                    item.turn_id,
                    format!("{:?}", item.kind),
                    format!("{:?}", item.status),
                    item.summary,
                    item.detail,
                    metadata_json,
                    artifact_refs_json,
                    item.started_at.map(|t| t.to_rfc3339()),
                    item.ended_at.map(|t| t.to_rfc3339()),
                ],
            )?;
            migrated_items += 1;
        }
    }

    // Migrate events
    let mut migrated_events = 0usize;
    if events_dir.is_dir() {
        for entry in std::fs::read_dir(&events_dir).into_iter().flatten().filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(_) => continue,
            };
            for line in content.lines() {
                if line.trim().is_empty() {
                    continue;
                }
                if let Ok(ev) = serde_json::from_str::<RuntimeEventRecord>(line) {
                    tx.execute(
                        "INSERT OR REPLACE INTO events (seq, timestamp, thread_id, turn_id, item_id, event, payload_json) VALUES (?1,?2,?3,?4,?5,?6,?7)",
                        params![
                            ev.seq as i64,
                            ev.timestamp.to_rfc3339(),
                            ev.thread_id,
                            ev.turn_id,
                            ev.item_id,
                            ev.event,
                            serde_json::to_string(&ev.payload).unwrap_or_default(),
                        ],
                    )?;
                    migrated_events += 1;
                }
            }
        }
    }

    // Load original state.json for next_seq
    let next_seq = std::fs::read_to_string(&state_path)
        .ok()
        .and_then(|c| serde_json::from_str::<RuntimeStoreState>(&c).ok())
        .map(|s| s.next_seq)
        .unwrap_or(1);

    tx.commit()?;
    eprintln!(
        "[thread-store] migration done: {} threads, {} turns, {} items, {} events",
        migrated_threads, migrated_turns, migrated_items, migrated_events,
    );

    Ok(RuntimeStoreState {
        schema_version: 2,
        next_seq,
    })
}

fn thread_record_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ThreadRecord> {
    let workspace: String = row.get(4)?;
    let coherence_json: String = row.get(15)?;
    let task_type: String = row.get(16).unwrap_or_else(|_| "code".to_string());
    let scratchpad_run_id: Option<String> = row.get(17).ok();
    let checklist_json: Option<String> = row.get(18).ok();
    let history_json: Option<String> = row.get(19).ok();
    let scratchpad_run_history = history_json
        .as_deref()
        .and_then(|raw| serde_json::from_str::<Vec<String>>(raw).ok())
        .filter(|v| !v.is_empty());
    let checklist_snapshot = checklist_json
        .as_deref()
        .and_then(|raw| serde_json::from_str(raw).ok());
    Ok(ThreadRecord {
        schema_version: 2,
        id: row.get(0)?,
        created_at: parse_ts(row.get::<_, String>(1)?)?,
        updated_at: parse_ts(row.get::<_, String>(2)?)?,
        model: row.get(3)?,
        workspace: PathBuf::from(workspace),
        mode: row.get(5)?,
        allow_shell: row.get::<_, i32>(6)? != 0,
        trust_mode: row.get::<_, i32>(7)? != 0,
        auto_approve: row.get::<_, i32>(8)? != 0,
        latest_turn_id: row.get(9)?,
        latest_response_bookmark: row.get(10)?,
        archived: row.get::<_, i32>(11)? != 0,
        system_prompt: row.get(12)?,
        task_id: row.get(13)?,
        title: row.get(14)?,
        task_type,
        coherence_state: serde_json::from_str(&coherence_json).unwrap_or_default(),
        scratchpad_run_id,
        scratchpad_run_history,
        checklist_snapshot,
    })
}

// === Thread operations ===

pub fn save_thread_sqlite(db: &Connection, thread: &ThreadRecord) -> anyhow::Result<()> {
    let coherence_json = serde_json::to_string(&thread.coherence_state).unwrap_or_default();
    let checklist_json = thread
        .checklist_snapshot
        .as_ref()
        .and_then(|v| serde_json::to_string(v).ok());
    let history_json = thread
        .scratchpad_run_history
        .as_ref()
        .and_then(|v| serde_json::to_string(v).ok());
    db.execute(
        "INSERT OR REPLACE INTO threads
         (id, created_at, updated_at, model, workspace, mode, allow_shell, trust_mode, auto_approve, latest_turn_id, latest_response_bookmark, archived, system_prompt, task_id, title, coherence_state_json, task_type, scratchpad_run_id, checklist_json, scratchpad_run_history_json)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20)",
        params![
            thread.id,
            thread.created_at.to_rfc3339(),
            thread.updated_at.to_rfc3339(),
            thread.model,
            thread.workspace.display().to_string(),
            thread.mode,
            thread.allow_shell as i32,
            thread.trust_mode as i32,
            thread.auto_approve as i32,
            thread.latest_turn_id,
            thread.latest_response_bookmark,
            thread.archived as i32,
            thread.system_prompt,
            thread.task_id,
            thread.title,
            coherence_json,
            thread.task_type,
            thread.scratchpad_run_id,
            checklist_json,
            history_json,
        ],
    )?;
    Ok(())
}

pub fn load_thread_sqlite(db: &Connection, thread_id: &str) -> anyhow::Result<ThreadRecord> {
    db.query_row(
        "SELECT id, created_at, updated_at, model, workspace, mode, allow_shell, trust_mode, auto_approve, latest_turn_id, latest_response_bookmark, archived, system_prompt, task_id, title, coherence_state_json, task_type, scratchpad_run_id, checklist_json, scratchpad_run_history_json FROM threads WHERE id = ?1",
        params![thread_id],
        |row| thread_record_from_row(row),
    )
    .map_err(|e| anyhow::anyhow!("thread not found ({thread_id}): {e}"))
}

pub fn list_threads_sqlite(db: &Connection) -> anyhow::Result<Vec<ThreadRecord>> {
    let mut stmt = db.prepare(
        "SELECT id, created_at, updated_at, model, workspace, mode, allow_shell, trust_mode, auto_approve, latest_turn_id, latest_response_bookmark, archived, system_prompt, task_id, title, coherence_state_json, task_type, scratchpad_run_id, checklist_json, scratchpad_run_history_json FROM threads ORDER BY updated_at DESC",
    )?;
    let threads = stmt
        .query_map([], thread_record_from_row)?
        .filter_map(|r| r.ok())
        .collect();
    Ok(threads)
}

// === Turn operations ===

pub fn save_turn_sqlite(db: &Connection, turn: &TurnRecord) -> anyhow::Result<()> {
    let usage_json = turn.usage.as_ref().map(|u| serde_json::to_string(u).unwrap_or_default());
    let item_ids_json = serde_json::to_string(&turn.item_ids).unwrap_or_default();
    db.execute(
        "INSERT OR REPLACE INTO turns
         (id, thread_id, status, input_summary, created_at, started_at, ended_at, duration_ms, usage_json, error, item_ids_json, steer_count, last_request_input_tokens)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
        params![
            turn.id,
            turn.thread_id,
            turn.status.to_string(),
            turn.input_summary,
            turn.created_at.to_rfc3339(),
            turn.started_at.map(|t| t.to_rfc3339()),
            turn.ended_at.map(|t| t.to_rfc3339()),
            turn.duration_ms.map(|d| d as i64),
            usage_json,
            turn.error,
            item_ids_json,
            turn.steer_count as i64,
            turn.last_request_input_tokens.map(i64::from),
        ],
    )?;
    Ok(())
}

pub fn load_turn_sqlite(db: &Connection, turn_id: &str) -> anyhow::Result<TurnRecord> {
    db.query_row(
        "SELECT id, thread_id, status, input_summary, created_at, started_at, ended_at, duration_ms, usage_json, error, item_ids_json, steer_count, last_request_input_tokens FROM turns WHERE id = ?1",
        params![turn_id],
        |row| {
            let status_str: String = row.get(2)?;
            let usage_json: Option<String> = row.get(8)?;
            let item_ids_json: String = row.get(10)?;
            Ok(TurnRecord {
                schema_version: 2,
                id: row.get(0)?,
                thread_id: row.get(1)?,
                status: parse_turn_status(&status_str),
                input_summary: row.get(3)?,
                created_at: parse_ts(row.get::<_, String>(4)?)?,
                started_at: parse_ts_opt(row.get::<_, Option<String>>(5)?)?,
                ended_at: parse_ts_opt(row.get::<_, Option<String>>(6)?)?,
                duration_ms: row.get::<_, Option<i64>>(7)?.map(|d| d as u64),
                usage: usage_json.and_then(|s| serde_json::from_str(&s).ok()),
                last_request_input_tokens: row
                    .get::<_, Option<i64>>(12)?
                    .map(|t| t as u32),
                error: row.get(9)?,
                item_ids: serde_json::from_str(&item_ids_json).unwrap_or_default(),
                steer_count: row.get::<_, i64>(11)? as usize,
            })
        },
    ).map_err(|e| anyhow::anyhow!("turn not found ({turn_id}): {e}"))
}

pub fn list_turns_for_thread_sqlite(db: &Connection, thread_id: &str) -> anyhow::Result<Vec<TurnRecord>> {
    let mut stmt = db.prepare(
        "SELECT id, thread_id, status, input_summary, created_at, started_at, ended_at, duration_ms, usage_json, error, item_ids_json, steer_count, last_request_input_tokens FROM turns WHERE thread_id = ?1 ORDER BY created_at ASC",
    )?;
    let turns = stmt
        .query_map(params![thread_id], |row| {
            let status_str: String = row.get(2)?;
            let usage_json: Option<String> = row.get(8)?;
            let item_ids_json: String = row.get(10)?;
            Ok(TurnRecord {
                schema_version: 2,
                id: row.get(0)?,
                thread_id: row.get(1)?,
                status: parse_turn_status(&status_str),
                input_summary: row.get(3)?,
                created_at: parse_ts(row.get::<_, String>(4)?)?,
                started_at: parse_ts_opt(row.get::<_, Option<String>>(5)?)?,
                ended_at: parse_ts_opt(row.get::<_, Option<String>>(6)?)?,
                duration_ms: row.get::<_, Option<i64>>(7)?.map(|d| d as u64),
                usage: usage_json.and_then(|s| serde_json::from_str(&s).ok()),
                last_request_input_tokens: row
                    .get::<_, Option<i64>>(12)?
                    .map(|t| t as u32),
                error: row.get(9)?,
                item_ids: serde_json::from_str(&item_ids_json).unwrap_or_default(),
                steer_count: row.get::<_, i64>(11)? as usize,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();
    Ok(turns)
}

pub fn list_incomplete_turns_sqlite(db: &Connection) -> anyhow::Result<Vec<TurnRecord>> {
    let mut stmt = db.prepare(
        "SELECT id, thread_id, status, input_summary, created_at, started_at, ended_at, duration_ms, usage_json, error, item_ids_json, steer_count, last_request_input_tokens FROM turns WHERE status IN ('Queued', 'InProgress')",
    )?;
    let turns = stmt
        .query_map([], |row| {
            let status_str: String = row.get(2)?;
            let usage_json: Option<String> = row.get(8)?;
            let item_ids_json: String = row.get(10)?;
            Ok(TurnRecord {
                schema_version: 2,
                id: row.get(0)?,
                thread_id: row.get(1)?,
                status: parse_turn_status(&status_str),
                input_summary: row.get(3)?,
                created_at: parse_ts(row.get::<_, String>(4)?)?,
                started_at: parse_ts_opt(row.get::<_, Option<String>>(5)?)?,
                ended_at: parse_ts_opt(row.get::<_, Option<String>>(6)?)?,
                duration_ms: row.get::<_, Option<i64>>(7)?.map(|d| d as u64),
                usage: usage_json.and_then(|s| serde_json::from_str(&s).ok()),
                last_request_input_tokens: row
                    .get::<_, Option<i64>>(12)?
                    .map(|t| t as u32),
                error: row.get(9)?,
                item_ids: serde_json::from_str(&item_ids_json).unwrap_or_default(),
                steer_count: row.get::<_, i64>(11)? as usize,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();
    Ok(turns)
}

// === Item operations ===

pub fn save_item_sqlite(db: &Connection, item: &TurnItemRecord) -> anyhow::Result<()> {
    let metadata_json = item.metadata.as_ref().map(|v| serde_json::to_string(v).unwrap_or_default());
    let artifact_refs_json = serde_json::to_string(&item.artifact_refs).unwrap_or_default();
    db.execute(
        "INSERT OR REPLACE INTO items
         (id, turn_id, kind, status, summary, detail, metadata_json, artifact_refs_json, started_at, ended_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
        params![
            item.id,
            item.turn_id,
            format!("{:?}", item.kind),
            format!("{:?}", item.status),
            item.summary,
            item.detail,
            metadata_json,
            artifact_refs_json,
            item.started_at.map(|t| t.to_rfc3339()),
            item.ended_at.map(|t| t.to_rfc3339()),
        ],
    )?;
    Ok(())
}

pub fn load_item_sqlite(db: &Connection, item_id: &str) -> anyhow::Result<TurnItemRecord> {
    db.query_row(
        "SELECT id, turn_id, kind, status, summary, detail, metadata_json, artifact_refs_json, started_at, ended_at FROM items WHERE id = ?1",
        params![item_id],
        |row| {
            let kind_str: String = row.get(2)?;
            let status_str: String = row.get(3)?;
            let metadata_json: Option<String> = row.get(6)?;
            let artifact_refs_json: String = row.get(7)?;
            Ok(TurnItemRecord {
                schema_version: 2,
                id: row.get(0)?,
                turn_id: row.get(1)?,
                kind: parse_item_kind(&kind_str),
                status: parse_item_status(&status_str),
                summary: row.get(4)?,
                detail: row.get(5)?,
                metadata: metadata_json.and_then(|s| serde_json::from_str(&s).ok()),
                artifact_refs: serde_json::from_str(&artifact_refs_json).unwrap_or_default(),
                started_at: parse_ts_opt(row.get::<_, Option<String>>(8)?)?,
                ended_at: parse_ts_opt(row.get::<_, Option<String>>(9)?)?,
            })
        },
    ).map_err(|e| anyhow::anyhow!("item not found ({item_id}): {e}"))
}

pub fn list_items_for_turn_sqlite(db: &Connection, turn_id: &str) -> anyhow::Result<Vec<TurnItemRecord>> {
    let mut stmt = db.prepare(
        "SELECT id, turn_id, kind, status, summary, detail, metadata_json, artifact_refs_json, started_at, ended_at FROM items WHERE turn_id = ?1 ORDER BY started_at ASC",
    )?;
    let items = stmt
        .query_map(params![turn_id], |row| {
            let kind_str: String = row.get(2)?;
            let status_str: String = row.get(3)?;
            let metadata_json: Option<String> = row.get(6)?;
            let artifact_refs_json: String = row.get(7)?;
            Ok(TurnItemRecord {
                schema_version: 2,
                id: row.get(0)?,
                turn_id: row.get(1)?,
                kind: parse_item_kind(&kind_str),
                status: parse_item_status(&status_str),
                summary: row.get(4)?,
                detail: row.get(5)?,
                metadata: metadata_json.and_then(|s| serde_json::from_str(&s).ok()),
                artifact_refs: serde_json::from_str(&artifact_refs_json).unwrap_or_default(),
                started_at: parse_ts_opt(row.get::<_, Option<String>>(8)?)?,
                ended_at: parse_ts_opt(row.get::<_, Option<String>>(9)?)?,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();
    Ok(items)
}

// === Event operations ===

pub fn append_event_sqlite(db: &Connection, event: &RuntimeEventRecord, next_seq: u64) -> anyhow::Result<()> {
    db.execute(
        "INSERT INTO events (seq, timestamp, thread_id, turn_id, item_id, event, payload_json) VALUES (?1,?2,?3,?4,?5,?6,?7)",
        params![
            next_seq as i64,
            event.timestamp.to_rfc3339(),
            event.thread_id,
            event.turn_id,
            event.item_id,
            event.event,
            serde_json::to_string(&event.payload).unwrap_or_default(),
        ],
    )?;
    Ok(())
}

pub fn events_since_sqlite(db: &Connection, thread_id: &str, since_seq: u64) -> anyhow::Result<Vec<RuntimeEventRecord>> {
    let mut stmt = db.prepare(
        "SELECT seq, timestamp, thread_id, turn_id, item_id, event, payload_json FROM events WHERE thread_id = ?1 AND seq > ?2 ORDER BY seq ASC",
    )?;
    let events = stmt
        .query_map(params![thread_id, since_seq as i64], |row| {
            let payload_json: String = row.get(6)?;
            Ok(RuntimeEventRecord {
                schema_version: 2,
                seq: row.get::<_, i64>(0)? as u64,
                timestamp: parse_ts(row.get::<_, String>(1)?)?,
                thread_id: row.get(2)?,
                turn_id: row.get(3)?,
                item_id: row.get(4)?,
                event: row.get(5)?,
                payload: serde_json::from_str(&payload_json).unwrap_or_default(),
            })
        })?
        .filter_map(|r| r.ok())
        .collect();
    Ok(events)
}

// === Usage aggregation ===

pub fn aggregate_usage_linear_sqlite(
    db: &Connection,
    since: Option<chrono::DateTime<chrono::Utc>>,
    until: Option<chrono::DateTime<chrono::Utc>>,
    group_by: UsageGroupBy,
) -> anyhow::Result<UsageAggregation> {
    let mut stmt = db.prepare(
        "SELECT t.thread_id, t.usage_json, t.created_at, r.model
         FROM turns t JOIN threads r ON t.thread_id = r.id
         WHERE t.usage_json IS NOT NULL",
    )?;
    use std::collections::BTreeMap;
    let mut buckets: BTreeMap<String, UsageBucket> = BTreeMap::new();
    let mut totals = UsageTotals::default();
    let mut cache_telemetry_incomplete = false;

    let rows = stmt.query_map([], |row| {
        let thread_id: String = row.get(0)?;
        let usage_json: String = row.get(1)?;
        let created_at_str: String = row.get(2)?;
        let model: String = row.get(3)?;
        Ok((thread_id, usage_json, created_at_str, model))
    })?;

    for row in rows {
        let Ok((thread_id, usage_json, created_at_str, model)) = row else {
            continue;
        };
        let usage: Usage = match serde_json::from_str(&usage_json) {
            Ok(u) => u,
            Err(_) => continue,
        };
        let Some(created_at) = parse_ts(created_at_str).ok() else {
            continue;
        };

        if let Some(s) = since
            && created_at < s
        {
            continue;
        }
        if let Some(u) = until
            && created_at > u
        {
            continue;
        }

        let key = match group_by {
            UsageGroupBy::Day => created_at.format("%Y-%m-%d").to_string(),
            UsageGroupBy::Model => model.clone(),
            UsageGroupBy::Provider => {
                crate::runtime_threads::provider_label_for_model(&model).to_string()
            }
            UsageGroupBy::Thread => thread_id.clone(),
        };
        let bucket = buckets.entry(key.clone()).or_insert_with(|| UsageBucket {
            key,
            ..UsageBucket::default()
        });
        crate::usage_aggregate::accumulate_turn_usage(
            &mut totals,
            bucket,
            &model,
            &usage,
            &mut cache_telemetry_incomplete,
        );
    }

    crate::usage_aggregate::finalize_usage_totals(&mut totals);
    for bucket in buckets.values_mut() {
        crate::usage_aggregate::finalize_usage_bucket(bucket);
    }

    let group_by_str = match group_by {
        UsageGroupBy::Day => "day",
        UsageGroupBy::Model => "model",
        UsageGroupBy::Provider => "provider",
        UsageGroupBy::Thread => "thread",
    }
    .to_string();

    Ok(UsageAggregation {
        since,
        until,
        group_by: group_by_str,
        totals,
        buckets: buckets.into_values().collect(),
        cache_telemetry_incomplete,
    })
}

// === Helpers ===

fn parse_ts(s: String) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(&s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("invalid RFC3339 timestamp '{s}': {e}"),
                )),
            )
        })
}

fn parse_ts_opt(value: Option<String>) -> rusqlite::Result<Option<DateTime<Utc>>> {
    match value {
        Some(s) => parse_ts(s).map(Some),
        None => Ok(None),
    }
}

fn parse_turn_status(s: &str) -> RuntimeTurnStatus {
    match s {
        "Queued" => RuntimeTurnStatus::Queued,
        "InProgress" => RuntimeTurnStatus::InProgress,
        "Completed" => RuntimeTurnStatus::Completed,
        "Failed" => RuntimeTurnStatus::Failed,
        "Interrupted" => RuntimeTurnStatus::Interrupted,
        "Canceled" => RuntimeTurnStatus::Canceled,
        _ => RuntimeTurnStatus::Queued,
    }
}

fn parse_item_kind(s: &str) -> TurnItemKind {
    match s {
        "UserMessage" => TurnItemKind::UserMessage,
        "AgentMessage" => TurnItemKind::AgentMessage,
        "ToolCall" => TurnItemKind::ToolCall,
        "FileChange" => TurnItemKind::FileChange,
        "CommandExecution" => TurnItemKind::CommandExecution,
        "ContextCompaction" => TurnItemKind::ContextCompaction,
        "Status" => TurnItemKind::Status,
        "Error" => TurnItemKind::Error,
        _ => TurnItemKind::Status,
    }
}

fn parse_item_status(s: &str) -> TurnItemLifecycleStatus {
    match s {
        "Queued" => TurnItemLifecycleStatus::Queued,
        "InProgress" => TurnItemLifecycleStatus::InProgress,
        "Completed" => TurnItemLifecycleStatus::Completed,
        "Failed" => TurnItemLifecycleStatus::Failed,
        "Interrupted" => TurnItemLifecycleStatus::Interrupted,
        "Canceled" => TurnItemLifecycleStatus::Canceled,
        _ => TurnItemLifecycleStatus::Queued,
    }
}

impl std::fmt::Display for RuntimeTurnStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
