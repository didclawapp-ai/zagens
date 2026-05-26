//! D7 C4 — read-only runtime thread listing for `deepseek thread list --source runtime`.
//!
//! Opens the same `runtime.db` path as sidecar (`RuntimeThreadManagerConfig` semantics)
//! without linking `deepseek-tui`.

use std::path::PathBuf;

use anyhow::{Context, Result};
use rusqlite::Connection;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeThreadRow {
    pub id: String,
    pub title: Option<String>,
    pub model: String,
    pub workspace: String,
    pub updated_at: String,
}

/// Resolve `runtime.db` using `DEEPSEEK_TASKS_DIR` / `DEEPSEEK_RUNTIME_DIR` (sidecar-aligned).
pub fn resolve_runtime_db_path() -> Option<PathBuf> {
    let tasks_dir = default_tasks_dir();
    let runtime_root = std::env::var("DEEPSEEK_RUNTIME_DIR")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| tasks_dir.join("runtime"));
    let db = runtime_root.join("runtime.db");
    if db.is_file() {
        Some(db)
    } else {
        None
    }
}

fn default_tasks_dir() -> PathBuf {
    if let Ok(path) = std::env::var("DEEPSEEK_TASKS_DIR") {
        if !path.trim().is_empty() {
            return PathBuf::from(path);
        }
    }
    if let Some(home) = dirs::home_dir() {
        return home.join(".deepseek").join("tasks");
    }
    PathBuf::from(".deepseek").join("tasks")
}

/// List threads from production runtime SQLite (read-only).
pub fn list_runtime_threads(include_archived: bool, limit: Option<usize>) -> Result<Vec<RuntimeThreadRow>> {
    let db_path = resolve_runtime_db_path().with_context(|| {
        "runtime.db not found; start sidecar or set DEEPSEEK_RUNTIME_DIR / DEEPSEEK_TASKS_DIR"
    })?;
    list_runtime_threads_at(&db_path, include_archived, limit)
}

fn list_runtime_threads_at(
    db_path: &std::path::Path,
    include_archived: bool,
    limit: Option<usize>,
) -> Result<Vec<RuntimeThreadRow>> {
    let db = Connection::open(db_path)
        .with_context(|| format!("open runtime db {}", db_path.display()))?;
    db.execute_batch("PRAGMA query_only = ON;")
        .context("enable query_only")?;

    let sql = if include_archived {
        "SELECT id, title, model, workspace, updated_at FROM threads ORDER BY updated_at DESC"
    } else {
        "SELECT id, title, model, workspace, updated_at FROM threads WHERE archived = 0 ORDER BY updated_at DESC"
    };
    let mut stmt = db.prepare(sql)?;
    let rows = stmt
        .query_map([], |row| {
            Ok(RuntimeThreadRow {
                id: row.get(0)?,
                title: row.get::<_, Option<String>>(1)?,
                model: row.get(2)?,
                workspace: row.get(3)?,
                updated_at: row.get(4)?,
            })
        })?
        .filter_map(|r| r.ok())
        .collect::<Vec<_>>();

    Ok(match limit {
        Some(n) => rows.into_iter().take(n).collect(),
        None => rows,
    })
}

pub fn print_runtime_threads(include_archived: bool, limit: Option<usize>) -> Result<()> {
    let threads = list_runtime_threads(include_archived, limit)?;
    if threads.is_empty() {
        eprintln!("(no runtime threads in {})", resolve_runtime_db_path().map(|p| p.display().to_string()).unwrap_or_default());
        return Ok(());
    }
    for t in threads {
        println!(
            "{} | {} | {} | {}",
            t.id,
            t.title.unwrap_or_else(|| "(untitled)".to_string()),
            t.model,
            t.workspace
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn list_runtime_threads_reads_sqlite() {
        let dir = tempdir().unwrap();
        let runtime_dir = dir.path().join("runtime");
        fs::create_dir_all(&runtime_dir).unwrap();
        let db_path = runtime_dir.join("runtime.db");
        let db = Connection::open(&db_path).unwrap();
        db.execute_batch(
            "CREATE TABLE threads (
                id TEXT PRIMARY KEY,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                model TEXT NOT NULL DEFAULT '',
                workspace TEXT NOT NULL DEFAULT '.',
                mode TEXT NOT NULL DEFAULT 'agent',
                allow_shell INTEGER NOT NULL DEFAULT 0,
                trust_mode INTEGER NOT NULL DEFAULT 0,
                auto_approve INTEGER NOT NULL DEFAULT 0,
                archived INTEGER NOT NULL DEFAULT 0,
                title TEXT
            );",
        )
        .unwrap();
        db.execute(
            "INSERT INTO threads (id, created_at, updated_at, model, workspace, title, archived)
             VALUES ('thr_1', '2025-01-01T00:00:00Z', '2025-01-02T00:00:00Z', 'm', '/ws', 'T1', 0)",
            [],
        )
        .unwrap();

        let rows = list_runtime_threads_at(&db_path, false, None).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "thr_1");
    }
}
