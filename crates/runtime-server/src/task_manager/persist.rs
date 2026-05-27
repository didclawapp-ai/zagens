use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use super::{CURRENT_TASK_SCHEMA_VERSION, TaskRecord, TaskStatus, TaskTimelineEntry};

#[derive(Debug, Serialize, Deserialize, Default)]
pub(super) struct QueueFile {
    pub queue: Vec<String>,
}

pub(super) fn load_state(
    tasks_dir: &Path,
    queue_path: &Path,
) -> Result<(HashMap<String, TaskRecord>, VecDeque<String>)> {
    let mut tasks = HashMap::new();
    if tasks_dir.exists() {
        for entry in fs::read_dir(tasks_dir)
            .with_context(|| format!("Failed to read tasks dir {}", tasks_dir.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "json") {
                continue;
            }
            let content = fs::read_to_string(&path)
                .with_context(|| format!("Failed to read task file {}", path.display()))?;
            let mut task: TaskRecord = serde_json::from_str(&content)
                .with_context(|| format!("Failed to parse task file {}", path.display()))?;
            if task.schema_version > CURRENT_TASK_SCHEMA_VERSION {
                bail!(
                    "Task schema v{} is newer than supported v{}",
                    task.schema_version,
                    CURRENT_TASK_SCHEMA_VERSION
                );
            }
            if task.status == TaskStatus::Running {
                task.status = TaskStatus::Queued;
                task.started_at = None;
                task.ended_at = None;
                task.duration_ms = None;
                task.timeline.push(TaskTimelineEntry {
                    timestamp: Utc::now(),
                    kind: "recovered".to_string(),
                    summary: "Recovered from restart and re-queued".to_string(),
                    detail_path: None,
                });
            }
            tasks.insert(task.id.clone(), task);
        }
    }

    let mut queue = if queue_path.exists() {
        let content = fs::read_to_string(queue_path)
            .with_context(|| format!("Failed to read queue file {}", queue_path.display()))?;
        let parsed: QueueFile = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse queue file {}", queue_path.display()))?;
        VecDeque::from(parsed.queue)
    } else {
        VecDeque::new()
    };

    queue.retain(|id| {
        tasks
            .get(id)
            .is_some_and(|task| task.status == TaskStatus::Queued)
    });

    let known = queue.iter().cloned().collect::<HashSet<_>>();
    let mut missing = tasks
        .values()
        .filter(|task| task.status == TaskStatus::Queued && !known.contains(&task.id))
        .map(|task| task.id.clone())
        .collect::<Vec<_>>();
    missing.sort();
    for id in missing {
        queue.push_back(id);
    }

    Ok((tasks, queue))
}
pub(super) fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }
    let payload = serde_json::to_string_pretty(value)?;
    crate::utils::write_atomic(path, payload.as_bytes())
        .with_context(|| format!("Failed to write {}", path.display()))
}

/// Default task persistence location (`~/.deepseek/tasks`).
#[must_use]
pub fn default_tasks_dir() -> PathBuf {
    if let Ok(path) = std::env::var("DEEPSEEK_TASKS_DIR")
        && !path.trim().is_empty()
    {
        return PathBuf::from(path);
    }
    if let Some(home) = dirs::home_dir() {
        return home.join(".deepseek").join("tasks");
    }
    PathBuf::from(".deepseek").join("tasks")
}
