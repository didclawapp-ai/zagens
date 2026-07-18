//! Persistence for `.zagens/night_queue.json` and queue event log.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use uuid::Uuid;
use zagens_config::workspace_meta_file_write;

use super::model::{
    GatePredicateSpec, NightQueueDocument, QUEUE_FILE, QueueEventRecord, QueueTask, QueueTaskStatus,
};

const QUEUE_LOCK_STALE: Duration = Duration::from_secs(120);
const QUEUE_LOCK_POLL: Duration = Duration::from_millis(50);
const QUEUE_LOCK_MAX_WAIT: Duration = Duration::from_secs(30);

struct QueueStoreLock(PathBuf);

impl QueueStoreLock {
    fn acquire(workspace: &Path) -> Result<Self> {
        let path = workspace_meta_file_write(workspace, "night_queue.lock");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let started = Instant::now();
        loop {
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(_) => return Ok(Self(path)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    if queue_lock_stale(&path) {
                        let _ = fs::remove_file(&path);
                        continue;
                    }
                    if started.elapsed() >= QUEUE_LOCK_MAX_WAIT {
                        anyhow::bail!("timed out waiting for night queue lock");
                    }
                    std::thread::sleep(QUEUE_LOCK_POLL);
                }
                Err(e) => return Err(e.into()),
            }
        }
    }
}

impl Drop for QueueStoreLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn queue_lock_stale(path: &Path) -> bool {
    let Ok(meta) = fs::metadata(path) else {
        return true;
    };
    let Ok(modified) = meta.modified() else {
        return true;
    };
    modified.elapsed().unwrap_or(Duration::ZERO) > QUEUE_LOCK_STALE
}

pub fn queue_path(workspace: &Path) -> PathBuf {
    workspace_meta_file_write(workspace, QUEUE_FILE)
}

pub fn events_path(workspace: &Path) -> PathBuf {
    workspace_meta_file_write(workspace, "queue_events.jsonl")
}

pub fn load(workspace: &Path) -> Result<NightQueueDocument> {
    let path = queue_path(workspace);
    if !path.exists() {
        return Ok(NightQueueDocument::default());
    }
    let raw = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    if raw.trim().is_empty() {
        return Ok(NightQueueDocument::default());
    }
    serde_json::from_str(&raw).with_context(|| format!("parse {}", path.display()))
}

pub fn save(workspace: &Path, doc: &NightQueueDocument) -> Result<()> {
    let path = queue_path(workspace);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(doc)?;
    fs::write(&path, json).with_context(|| format!("write {}", path.display()))
}

pub fn append_event(workspace: &Path, event: &QueueEventRecord) -> Result<()> {
    let path = events_path(workspace);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("open {}", path.display()))?;
    let line = serde_json::to_string(event)?;
    writeln!(file, "{line}")?;
    Ok(())
}

/// Atomically claim up to `max` pending tasks and mark them running.
pub fn claim_pending_tasks(workspace: &Path, max: usize) -> Result<Vec<QueueTask>> {
    let _lock = QueueStoreLock::acquire(workspace)?;
    let mut doc = load(workspace)?;
    let ids: Vec<String> = doc
        .tasks
        .iter()
        .filter(|t| t.status == QueueTaskStatus::Pending)
        .take(max.max(1))
        .map(|t| t.id.clone())
        .collect();
    let mut claimed = Vec::with_capacity(ids.len());
    for id in ids {
        let Some(task) = doc.tasks.iter_mut().find(|t| t.id == id) else {
            continue;
        };
        task.status = QueueTaskStatus::Running;
        task.started_at = Some(Utc::now());
        claimed.push(task.clone());
    }
    save(workspace, &doc)?;
    Ok(claimed)
}

pub fn persist_task(workspace: &Path, task: QueueTask) -> Result<()> {
    let _lock = QueueStoreLock::acquire(workspace)?;
    let mut doc = load(workspace)?;
    let Some(slot) = doc.tasks.iter_mut().find(|t| t.id == task.id) else {
        anyhow::bail!("queue task not found: {}", task.id);
    };
    // Do not let an in-flight runner overwrite a user cancel.
    if slot.status == QueueTaskStatus::Canceled && task.status != QueueTaskStatus::Canceled {
        return Ok(());
    }
    *slot = task;
    save(workspace, &doc)
}

pub fn finalize_run(workspace: &Path, last_run_at: DateTime<Utc>) -> Result<NightQueueDocument> {
    let _lock = QueueStoreLock::acquire(workspace)?;
    let mut doc = load(workspace)?;
    doc.last_run_at = Some(last_run_at);
    save(workspace, &doc)?;
    Ok(doc)
}

pub fn enqueue(
    workspace: &Path,
    prompt: String,
    gate: Vec<GatePredicateSpec>,
    use_worktree: bool,
) -> Result<QueueTask> {
    let _lock = QueueStoreLock::acquire(workspace)?;
    let mut doc = load(workspace)?;
    let task = QueueTask {
        id: format!("q-{}", Uuid::new_v4()),
        prompt,
        status: QueueTaskStatus::Pending,
        worktree_path: if use_worktree {
            Some(PathBuf::from("<allocate-on-run>"))
        } else {
            None
        },
        gate,
        created_at: Utc::now(),
        started_at: None,
        finished_at: None,
        pre_snapshot_id: None,
        gate_summary: None,
        error: None,
    };
    append_event(
        workspace,
        &QueueEventRecord {
            kind: "queue_enqueued".to_string(),
            ts: Utc::now(),
            task_id: task.id.clone(),
            payload: Some(serde_json::json!({
                "prompt_preview": preview(&task.prompt, 120),
                "gate_count": task.gate.len(),
            })),
        },
    )?;
    doc.tasks.push(task.clone());
    save(workspace, &doc)?;
    Ok(task)
}

pub fn update_task(workspace: &Path, task: QueueTask) -> Result<()> {
    persist_task(workspace, task)
}

/// Remove a task from the queue. Running tasks cannot be removed (cancel/stop first).
pub fn remove_task(workspace: &Path, task_id: &str) -> Result<QueueTask> {
    let _lock = QueueStoreLock::acquire(workspace)?;
    let mut doc = load(workspace)?;
    let idx = doc
        .tasks
        .iter()
        .position(|t| t.id == task_id)
        .ok_or_else(|| anyhow::anyhow!("queue task not found: {task_id}"))?;
    if doc.tasks[idx].status == QueueTaskStatus::Running {
        anyhow::bail!("cannot remove running task {task_id}; stop or cancel it first");
    }
    let removed = doc.tasks.remove(idx);
    append_event(
        workspace,
        &QueueEventRecord {
            kind: "queue_removed".into(),
            ts: Utc::now(),
            task_id: removed.id.clone(),
            payload: Some(serde_json::json!({ "status": format!("{:?}", removed.status) })),
        },
    )?;
    save(workspace, &doc)?;
    Ok(removed)
}

/// Cancel a pending task, or mark a stuck/running task canceled (caller stops the batch).
pub fn cancel_task(workspace: &Path, task_id: &str) -> Result<QueueTask> {
    let _lock = QueueStoreLock::acquire(workspace)?;
    let mut doc = load(workspace)?;
    let task = doc
        .tasks
        .iter_mut()
        .find(|t| t.id == task_id)
        .ok_or_else(|| anyhow::anyhow!("queue task not found: {task_id}"))?;
    match task.status {
        QueueTaskStatus::Pending | QueueTaskStatus::Running => {
            task.status = QueueTaskStatus::Canceled;
            task.finished_at = Some(Utc::now());
            if task.error.is_none() {
                task.error = Some("canceled by user".into());
            }
        }
        QueueTaskStatus::Canceled => {}
        other => anyhow::bail!("cannot cancel task in status {other:?}"),
    }
    let out = task.clone();
    append_event(
        workspace,
        &QueueEventRecord {
            kind: "queue_canceled".into(),
            ts: Utc::now(),
            task_id: out.id.clone(),
            payload: None,
        },
    )?;
    save(workspace, &doc)?;
    Ok(out)
}

/// Reclaim stale `running` tasks when no in-process batch is active.
pub fn reclaim_stale_running(workspace: &Path) -> Result<Vec<QueueTask>> {
    let _lock = QueueStoreLock::acquire(workspace)?;
    let mut doc = load(workspace)?;
    let mut reclaimed = Vec::new();
    for task in &mut doc.tasks {
        if task.status != QueueTaskStatus::Running {
            continue;
        }
        task.status = QueueTaskStatus::Canceled;
        task.finished_at = Some(Utc::now());
        task.error = Some("reclaimed: stale running (no active batch)".into());
        reclaimed.push(task.clone());
        append_event(
            workspace,
            &QueueEventRecord {
                kind: "queue_reclaimed".into(),
                ts: Utc::now(),
                task_id: task.id.clone(),
                payload: None,
            },
        )?;
    }
    if !reclaimed.is_empty() {
        save(workspace, &doc)?;
    }
    Ok(reclaimed)
}

/// Drop terminal tasks (passed / failed / rolled_back / canceled).
pub fn clear_finished(workspace: &Path) -> Result<usize> {
    let _lock = QueueStoreLock::acquire(workspace)?;
    let mut doc = load(workspace)?;
    let before = doc.tasks.len();
    doc.tasks.retain(|t| {
        matches!(
            t.status,
            QueueTaskStatus::Pending | QueueTaskStatus::Running
        )
    });
    let removed = before - doc.tasks.len();
    if removed > 0 {
        append_event(
            workspace,
            &QueueEventRecord {
                kind: "queue_cleared_finished".into(),
                ts: Utc::now(),
                task_id: "*".into(),
                payload: Some(serde_json::json!({ "removed": removed })),
            },
        )?;
        save(workspace, &doc)?;
    }
    Ok(removed)
}

/// Re-enqueue a copy of a finished/canceled task as pending.
pub fn retry_task(workspace: &Path, task_id: &str) -> Result<QueueTask> {
    let _lock = QueueStoreLock::acquire(workspace)?;
    let mut doc = load(workspace)?;
    let src = doc
        .tasks
        .iter()
        .find(|t| t.id == task_id)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("queue task not found: {task_id}"))?;
    if !matches!(
        src.status,
        QueueTaskStatus::Failed
            | QueueTaskStatus::RolledBack
            | QueueTaskStatus::Canceled
            | QueueTaskStatus::Passed
    ) {
        anyhow::bail!("can only retry finished tasks (got {:?})", src.status);
    }
    let use_worktree = src.worktree_path.is_some();
    let task = QueueTask {
        id: format!("q-{}", Uuid::new_v4()),
        prompt: src.prompt.clone(),
        status: QueueTaskStatus::Pending,
        worktree_path: if use_worktree {
            Some(PathBuf::from("<allocate-on-run>"))
        } else {
            None
        },
        gate: src.gate.clone(),
        created_at: Utc::now(),
        started_at: None,
        finished_at: None,
        pre_snapshot_id: None,
        gate_summary: None,
        error: None,
    };
    append_event(
        workspace,
        &QueueEventRecord {
            kind: "queue_retried".into(),
            ts: Utc::now(),
            task_id: task.id.clone(),
            payload: Some(serde_json::json!({ "from": src.id })),
        },
    )?;
    doc.tasks.push(task.clone());
    save(workspace, &doc)?;
    Ok(task)
}

/// Mark a running task canceled mid-batch (used by the runner after stop).
pub fn mark_canceled(workspace: &Path, task_id: &str, reason: &str) -> Result<QueueTask> {
    let _lock = QueueStoreLock::acquire(workspace)?;
    let mut doc = load(workspace)?;
    let task = doc
        .tasks
        .iter_mut()
        .find(|t| t.id == task_id)
        .ok_or_else(|| anyhow::anyhow!("queue task not found: {task_id}"))?;
    task.status = QueueTaskStatus::Canceled;
    task.finished_at = Some(Utc::now());
    task.error = Some(reason.to_string());
    let out = task.clone();
    save(workspace, &doc)?;
    Ok(out)
}

/// Restore claimed-but-not-started tasks to pending after a batch stop.
pub fn restore_pending(workspace: &Path, task_ids: &[String]) -> Result<()> {
    if task_ids.is_empty() {
        return Ok(());
    }
    let _lock = QueueStoreLock::acquire(workspace)?;
    let mut doc = load(workspace)?;
    let mut changed = false;
    for id in task_ids {
        if let Some(task) = doc.tasks.iter_mut().find(|t| t.id == *id)
            && task.status == QueueTaskStatus::Running
        {
            task.status = QueueTaskStatus::Pending;
            task.started_at = None;
            task.worktree_path = task
                .worktree_path
                .as_ref()
                .map(|_| PathBuf::from("<allocate-on-run>"));
            changed = true;
        }
    }
    if changed {
        save(workspace, &doc)?;
    }
    Ok(())
}

#[must_use]
pub fn preview(text: &str, max: usize) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= max {
        trimmed.to_string()
    } else {
        format!("{}…", trimmed.chars().take(max).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};
    use std::thread;
    use tempfile::TempDir;

    #[test]
    fn cancel_pending_and_clear_finished() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path();
        let task = enqueue(workspace, "pending".into(), vec![], false).unwrap();
        let canceled = cancel_task(workspace, &task.id).unwrap();
        assert_eq!(canceled.status, QueueTaskStatus::Canceled);
        let removed = clear_finished(workspace).unwrap();
        assert_eq!(removed, 1);
        assert!(load(workspace).unwrap().tasks.is_empty());
    }

    #[test]
    fn retry_creates_new_pending() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path();
        let task = enqueue(workspace, "retry-me".into(), vec![], true).unwrap();
        cancel_task(workspace, &task.id).unwrap();
        let again = retry_task(workspace, &task.id).unwrap();
        assert_eq!(again.status, QueueTaskStatus::Pending);
        assert_ne!(again.id, task.id);
        assert_eq!(again.prompt, "retry-me");
        assert!(again.worktree_path.is_some());
    }

    #[test]
    fn persist_does_not_revive_canceled() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path();
        let mut task = enqueue(workspace, "x".into(), vec![], false).unwrap();
        task.status = QueueTaskStatus::Running;
        persist_task(workspace, task.clone()).unwrap();
        cancel_task(workspace, &task.id).unwrap();
        task.status = QueueTaskStatus::Running;
        persist_task(workspace, task).unwrap();
        let doc = load(workspace).unwrap();
        assert_eq!(doc.tasks[0].status, QueueTaskStatus::Canceled);
    }

    #[test]
    fn concurrent_enqueue_preserves_both_tasks() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path();
        let barrier = Arc::new(Barrier::new(2));
        let b1 = barrier.clone();
        let b2 = barrier.clone();
        let ws1 = workspace.to_path_buf();
        let ws2 = workspace.to_path_buf();

        let h1 = thread::spawn(move || {
            b1.wait();
            enqueue(&ws1, "task-a".into(), vec![], false).unwrap()
        });
        let h2 = thread::spawn(move || {
            b2.wait();
            enqueue(&ws2, "task-b".into(), vec![], false).unwrap()
        });

        let _ = h1.join().unwrap();
        let _ = h2.join().unwrap();
        let doc = load(workspace).unwrap();
        assert_eq!(doc.tasks.len(), 2);
    }
}
