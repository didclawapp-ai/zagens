use std::collections::{HashMap, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, anyhow, bail};
use chrono::Utc;
use serde_json::Value;
use tokio::sync::{Mutex, Notify, mpsc};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::config::Config;
use crate::runtime_threads::{
    RuntimeThreadManager, RuntimeThreadManagerConfig, SharedRuntimeThreadManager,
};
use crate::utils::spawn_supervised;

use super::MAX_WORKERS;
use super::config::TaskManagerConfig;
use super::executor::{
    EngineTaskExecutor, ExecutionTask, TaskExecutionEvent, TaskExecutionResult, TaskExecutor,
};
use super::helpers::{
    ARTIFACT_THRESHOLD, TIMELINE_SUMMARY_LIMIT, duration_ms, resolve_task_id, sanitize_filename,
    summarize_json, summarize_text,
};
use super::persist::{QueueFile, load_state, write_json_atomic};
use super::{
    CURRENT_TASK_SCHEMA_VERSION, NewTaskRequest, TaskArtifactRef, TaskAttemptRecord,
    TaskChecklistState, TaskCounts, TaskGateRecord, TaskGithubEvent, TaskRecord, TaskStatus,
    TaskSummary, TaskTimelineEntry, TaskToolCallSummary, TaskToolStatus,
};

/// Thread-safe task manager.
pub type SharedTaskManager = Arc<TaskManager>;

pub struct TaskManager {
    cfg: TaskManagerConfig,
    executor: Arc<dyn TaskExecutor>,
    tasks_dir: PathBuf,
    artifacts_dir: PathBuf,
    queue_path: PathBuf,
    state: Mutex<ManagerState>,
    notify: Notify,
    cancel_token: CancellationToken,
}

struct ManagerState {
    tasks: HashMap<String, TaskRecord>,
    queue: VecDeque<String>,
    running_cancel: HashMap<String, CancellationToken>,
}

impl TaskManager {
    /// Start the manager with the default DeepSeek executor.
    pub async fn start(cfg: TaskManagerConfig, api_config: Config) -> Result<SharedTaskManager> {
        let runtime_threads = Arc::new(RuntimeThreadManager::open(
            api_config.clone(),
            cfg.default_workspace.clone(),
            RuntimeThreadManagerConfig::from_task_data_dir(cfg.data_dir.clone()),
        )?);
        Self::start_with_runtime_manager(cfg, api_config, runtime_threads).await
    }

    /// Start the manager with an injected runtime thread manager.
    pub async fn start_with_runtime_manager(
        cfg: TaskManagerConfig,
        _api_config: Config,
        runtime_threads: SharedRuntimeThreadManager,
    ) -> Result<SharedTaskManager> {
        let executor: Arc<dyn TaskExecutor> =
            Arc::new(EngineTaskExecutor::new(runtime_threads.clone()));
        let manager = Self::start_with_executor(cfg, executor).await?;
        runtime_threads.attach_task_manager(manager.clone());
        Ok(manager)
    }

    /// Start the manager with a custom executor (used for tests).
    pub async fn start_with_executor(
        cfg: TaskManagerConfig,
        executor: Arc<dyn TaskExecutor>,
    ) -> Result<SharedTaskManager> {
        let workers = cfg.worker_count.clamp(1, MAX_WORKERS);
        let tasks_dir = cfg.data_dir.join("tasks");
        let artifacts_dir = cfg.data_dir.join("artifacts");
        let queue_path = cfg.data_dir.join("queue.json");
        fs::create_dir_all(&tasks_dir)
            .with_context(|| format!("Failed to create tasks dir {}", tasks_dir.display()))?;
        fs::create_dir_all(&artifacts_dir).with_context(|| {
            format!(
                "Failed to create task artifacts dir {}",
                artifacts_dir.display()
            )
        })?;

        let (tasks, queue) = load_state(&tasks_dir, &queue_path)?;

        let cancel_token = CancellationToken::new();
        let manager = Arc::new(Self {
            cfg,
            executor,
            tasks_dir,
            artifacts_dir,
            queue_path,
            state: Mutex::new(ManagerState {
                tasks,
                queue,
                running_cancel: HashMap::new(),
            }),
            notify: Notify::new(),
            cancel_token: cancel_token.clone(),
        });

        {
            let state = manager.state.lock().await;
            manager.persist_all_locked(&state)?;
        }

        for _ in 0..workers {
            let manager_clone = Arc::clone(&manager);
            spawn_supervised(
                "task-manager-worker",
                std::panic::Location::caller(),
                async move {
                    manager_clone.worker_loop().await;
                },
            );
        }

        Ok(manager)
    }

    #[allow(dead_code)] // Public API for external callers (runtime API)
    pub fn shutdown(&self) {
        self.cancel_token.cancel();
    }

    #[allow(dead_code)] // Public API for external callers
    pub fn is_shutdown(&self) -> bool {
        self.cancel_token.is_cancelled()
    }

    /// Enqueue a new task.
    pub async fn add_task(&self, req: NewTaskRequest) -> Result<TaskRecord> {
        let prompt = req.prompt.trim().to_string();
        if prompt.is_empty() {
            bail!("Task prompt cannot be empty");
        }

        let task = TaskRecord {
            schema_version: CURRENT_TASK_SCHEMA_VERSION,
            id: format!("task_{}", &Uuid::new_v4().to_string()[..8]),
            prompt,
            model: req.model.unwrap_or_else(|| self.cfg.default_model.clone()),
            workspace: req
                .workspace
                .unwrap_or_else(|| self.cfg.default_workspace.clone()),
            mode: req.mode.unwrap_or_else(|| self.cfg.default_mode.clone()),
            allow_shell: req.allow_shell.unwrap_or(self.cfg.allow_shell),
            trust_mode: self.cfg.trust_mode && req.trust_mode.unwrap_or(false),
            auto_approve: req.auto_approve.unwrap_or(true),
            status: TaskStatus::Queued,
            created_at: Utc::now(),
            started_at: None,
            ended_at: None,
            duration_ms: None,
            result_summary: None,
            result_detail_path: None,
            error: None,
            thread_id: None,
            turn_id: None,
            runtime_event_count: 0,
            checklist: TaskChecklistState::default(),
            gates: Vec::new(),
            attempts: Vec::new(),
            artifacts: Vec::new(),
            github_events: Vec::new(),
            tool_calls: Vec::new(),
            timeline: vec![TaskTimelineEntry {
                timestamp: Utc::now(),
                kind: "queued".to_string(),
                summary: "Task queued".to_string(),
                detail_path: None,
            }],
        };

        {
            let mut state = self.state.lock().await;
            state.queue.push_back(task.id.clone());
            state.tasks.insert(task.id.clone(), task.clone());
            self.persist_all_locked(&state)?;
        }
        self.notify.notify_one();
        Ok(task)
    }

    /// List tasks, newest first.
    pub async fn list_tasks(&self, limit: Option<usize>) -> Vec<TaskSummary> {
        let state = self.state.lock().await;
        let mut items = state
            .tasks
            .values()
            .map(TaskSummary::from)
            .collect::<Vec<_>>();
        items.sort_by_key(|i| std::cmp::Reverse(i.created_at));
        if let Some(limit) = limit {
            items.truncate(limit);
        }
        items
    }

    /// Retrieve a task by full id or prefix.
    pub async fn get_task(&self, id_or_prefix: &str) -> Result<TaskRecord> {
        let state = self.state.lock().await;
        let id = resolve_task_id(&state.tasks, id_or_prefix)?;
        state
            .tasks
            .get(&id)
            .cloned()
            .ok_or_else(|| anyhow!("Task not found: {id_or_prefix}"))
    }

    /// Cancel a queued or running task by id/prefix.
    pub async fn cancel_task(&self, id_or_prefix: &str) -> Result<TaskRecord> {
        let mut state = self.state.lock().await;
        let id = resolve_task_id(&state.tasks, id_or_prefix)?;
        let now = Utc::now();

        let mut cancel_running = false;
        {
            let task = state
                .tasks
                .get_mut(&id)
                .ok_or_else(|| anyhow!("Task not found: {id}"))?;
            match task.status {
                TaskStatus::Queued => {
                    task.status = TaskStatus::Canceled;
                    task.ended_at = Some(now);
                    task.duration_ms = Some(0);
                    task.timeline.push(TaskTimelineEntry {
                        timestamp: now,
                        kind: "canceled".to_string(),
                        summary: "Task canceled before execution".to_string(),
                        detail_path: None,
                    });
                    state.queue.retain(|queued_id| queued_id != &id);
                }
                TaskStatus::Running => {
                    cancel_running = true;
                    task.timeline.push(TaskTimelineEntry {
                        timestamp: now,
                        kind: "cancel_requested".to_string(),
                        summary: "Cancellation requested".to_string(),
                        detail_path: None,
                    });
                }
                _ => {}
            }
        }

        if cancel_running && let Some(token) = state.running_cancel.get(&id) {
            token.cancel();
        }

        self.persist_all_locked(&state)?;
        state
            .tasks
            .get(&id)
            .cloned()
            .ok_or_else(|| anyhow!("Task not found: {id}"))
    }

    /// Remove completed, failed, and canceled tasks from memory and disk.
    pub async fn clear_terminal_tasks(&self) -> Result<usize> {
        let mut state = self.state.lock().await;
        let ids: Vec<String> = state
            .tasks
            .values()
            .filter(|t| {
                matches!(
                    t.status,
                    TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Canceled
                )
            })
            .map(|t| t.id.clone())
            .collect();
        if ids.is_empty() {
            return Ok(0);
        }
        for id in &ids {
            state.tasks.remove(id);
            state.queue.retain(|queued| queued != id);
            state.running_cancel.remove(id);
            let path = self.tasks_dir.join(format!("{id}.json"));
            if path.is_file() {
                fs::remove_file(&path)
                    .with_context(|| format!("Failed to remove task file {}", path.display()))?;
            }
        }
        self.persist_queue_locked(&state.queue)?;
        Ok(ids.len())
    }

    /// Return aggregate status counters.
    pub async fn counts(&self) -> TaskCounts {
        let state = self.state.lock().await;
        let mut counts = TaskCounts::default();
        for task in state.tasks.values() {
            match task.status {
                TaskStatus::Queued => counts.queued += 1,
                TaskStatus::Running => counts.running += 1,
                TaskStatus::Completed => counts.completed += 1,
                TaskStatus::Failed => counts.failed += 1,
                TaskStatus::Canceled => counts.canceled += 1,
            }
        }
        counts
    }

    /// Root directory for durable task state.
    #[must_use]
    pub fn data_dir(&self) -> PathBuf {
        self.cfg.data_dir.clone()
    }

    /// Resolve a task artifact reference to an absolute path.
    #[must_use]
    pub fn artifact_absolute_path(&self, path: &Path) -> PathBuf {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.cfg.data_dir.join(path)
        }
    }

    /// Write a durable task artifact and return the persisted path reference.
    pub fn write_task_artifact(
        &self,
        task_id: &str,
        label: &str,
        content: &str,
    ) -> Result<PathBuf> {
        self.write_artifact(task_id, label, content)
    }

    /// Apply model-visible tool metadata to a task and persist it.
    pub async fn record_tool_metadata(
        &self,
        id_or_prefix: &str,
        metadata: &Value,
    ) -> Result<TaskRecord> {
        let mut state = self.state.lock().await;
        let id = resolve_task_id(&state.tasks, id_or_prefix)?;
        let updated = {
            let task = state
                .tasks
                .get_mut(&id)
                .ok_or_else(|| anyhow!("Task not found: {id}"))?;
            self.apply_task_update_metadata(task, Some(metadata))?;
            task.clone()
        };
        self.persist_task_locked(&updated)?;
        Ok(updated)
    }

    async fn worker_loop(self: Arc<Self>) {
        loop {
            if self.cancel_token.is_cancelled() {
                tracing::debug!("Worker exiting due to shutdown");
                break;
            }
            let next = {
                let mut state = self.state.lock().await;
                match state.queue.pop_front() {
                    None => None,
                    Some(task_id) => {
                        if let Some(task) = state.tasks.get_mut(&task_id) {
                            if task.status != TaskStatus::Queued {
                                let _ = self.persist_queue_locked(&state.queue);
                                None
                            } else {
                                let now = Utc::now();
                                task.status = TaskStatus::Running;
                                task.started_at = Some(now);
                                task.ended_at = None;
                                task.duration_ms = None;
                                task.error = None;
                                task.timeline.push(TaskTimelineEntry {
                                    timestamp: now,
                                    kind: "running".to_string(),
                                    summary: "Task started".to_string(),
                                    detail_path: None,
                                });

                                let request = {
                                    ExecutionTask {
                                        id: task.id.clone(),
                                        prompt: task.prompt.clone(),
                                        model: task.model.clone(),
                                        workspace: task.workspace.clone(),
                                        mode_label: task.mode.clone(),
                                        allow_shell: task.allow_shell,
                                        trust_mode: task.trust_mode,
                                        auto_approve: task.auto_approve,
                                    }
                                };
                                let cancel = CancellationToken::new();
                                state.running_cancel.insert(task_id.clone(), cancel.clone());

                                if let Err(err) = self.persist_all_locked(&state) {
                                    tracing::error!("Failed to persist task start: {err}");
                                }
                                Some((task_id, request, cancel))
                            }
                        } else {
                            let _ = self.persist_queue_locked(&state.queue);
                            None
                        }
                    }
                }
            };

            let Some((task_id, request, cancel)) = next else {
                tokio::select! {
                    _ = self.cancel_token.cancelled() => {
                        tracing::debug!("Worker exiting during wait");
                        break;
                    }
                    _ = self.notify.notified() => {}
                }
                continue;
            };

            self.run_task(task_id, request, cancel).await;
        }
    }

    async fn run_task(&self, task_id: String, request: ExecutionTask, cancel: CancellationToken) {
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();
        let exec_fut = self
            .executor
            .execute(request.clone(), event_tx, cancel.clone());
        tokio::pin!(exec_fut);

        let result = loop {
            tokio::select! {
                maybe_event = event_rx.recv() => {
                    if let Some(event) = maybe_event
                        && let Err(err) = self.apply_execution_event(&task_id, event).await
                    {
                        tracing::error!("Failed to apply task event for {task_id}: {err}");
                    }
                }
                exec_result = &mut exec_fut => {
                    break exec_result;
                }
            }
        };

        while let Ok(event) = event_rx.try_recv() {
            if let Err(err) = self.apply_execution_event(&task_id, event).await {
                tracing::error!("Failed to apply trailing task event for {task_id}: {err}");
            }
        }

        if let Err(err) = self
            .finish_task(&task_id, result, cancel, &request.mode_label)
            .await
        {
            tracing::error!("Failed to finalize task {task_id}: {err}");
        }
    }

    async fn apply_execution_event(&self, task_id: &str, event: TaskExecutionEvent) -> Result<()> {
        let mut state = self.state.lock().await;
        let Some(task) = state.tasks.get_mut(task_id) else {
            return Ok(());
        };

        match event {
            TaskExecutionEvent::ThreadLinked { thread_id, turn_id } => {
                task.thread_id = Some(thread_id.clone());
                task.turn_id = Some(turn_id.clone());
                task.timeline.push(TaskTimelineEntry {
                    timestamp: Utc::now(),
                    kind: "runtime_link".to_string(),
                    summary: format!("Linked runtime thread {thread_id} turn {turn_id}"),
                    detail_path: None,
                });
            }
            TaskExecutionEvent::Status { message } => {
                task.timeline.push(TaskTimelineEntry {
                    timestamp: Utc::now(),
                    kind: "status".to_string(),
                    summary: summarize_text(&message, TIMELINE_SUMMARY_LIMIT),
                    detail_path: None,
                });
            }
            TaskExecutionEvent::MessageDelta { content } => {
                if !content.trim().is_empty() {
                    task.timeline.push(TaskTimelineEntry {
                        timestamp: Utc::now(),
                        kind: "message".to_string(),
                        summary: summarize_text(&content, TIMELINE_SUMMARY_LIMIT),
                        detail_path: None,
                    });
                }
            }
            TaskExecutionEvent::ToolStarted { id, name, input } => {
                let input_summary = summarize_json(&input);
                task.tool_calls.push(TaskToolCallSummary {
                    id: id.clone(),
                    name: name.clone(),
                    status: TaskToolStatus::Running,
                    started_at: Utc::now(),
                    ended_at: None,
                    duration_ms: None,
                    input_summary: input_summary.clone(),
                    output_summary: None,
                    detail_path: None,
                    patch_ref: None,
                });
                let summary = input_summary
                    .map(|s| format!("{name} started ({s})"))
                    .unwrap_or_else(|| format!("{name} started"));
                task.timeline.push(TaskTimelineEntry {
                    timestamp: Utc::now(),
                    kind: "tool_started".to_string(),
                    summary,
                    detail_path: None,
                });
            }
            TaskExecutionEvent::ToolProgress { id, output } => {
                task.timeline.push(TaskTimelineEntry {
                    timestamp: Utc::now(),
                    kind: "tool_progress".to_string(),
                    summary: format!(
                        "{id}: {}",
                        summarize_text(&output, TIMELINE_SUMMARY_LIMIT.saturating_sub(8))
                    ),
                    detail_path: None,
                });
            }
            TaskExecutionEvent::ToolCompleted {
                id,
                name,
                success,
                output,
                metadata,
            } => {
                let now = Utc::now();
                let detail_path = self.artifact_if_large(task_id, &name, &output)?;
                let output_summary = summarize_text(&output, TIMELINE_SUMMARY_LIMIT);
                let patch_ref = if name == "apply_patch" {
                    detail_path.clone()
                } else {
                    None
                };

                if let Some(call) = task.tool_calls.iter_mut().find(|call| call.id == id) {
                    call.status = if success {
                        TaskToolStatus::Success
                    } else {
                        TaskToolStatus::Failed
                    };
                    call.ended_at = Some(now);
                    call.duration_ms = Some(duration_ms(call.started_at, now));
                    call.output_summary = Some(output_summary.clone());
                    call.detail_path = detail_path.clone();
                    call.patch_ref = patch_ref.clone();

                    if call.duration_ms.is_none()
                        && let Some(duration) = metadata
                            .as_ref()
                            .and_then(|m| m.get("duration_ms"))
                            .and_then(Value::as_u64)
                    {
                        call.duration_ms = Some(duration);
                    }
                }

                let status = if success { "success" } else { "failed" };
                task.timeline.push(TaskTimelineEntry {
                    timestamp: now,
                    kind: "tool_completed".to_string(),
                    summary: format!("{name} {status}: {output_summary}"),
                    detail_path: detail_path.clone(),
                });
                if let Some(patch_ref) = patch_ref {
                    task.timeline.push(TaskTimelineEntry {
                        timestamp: now,
                        kind: "patch_ref".to_string(),
                        summary: format!("Patch artifact: {}", patch_ref.display()),
                        detail_path: Some(patch_ref),
                    });
                }

                self.apply_task_update_metadata(task, metadata.as_ref())?;
            }
            TaskExecutionEvent::Error { message } => {
                task.timeline.push(TaskTimelineEntry {
                    timestamp: Utc::now(),
                    kind: "error".to_string(),
                    summary: summarize_text(&message, TIMELINE_SUMMARY_LIMIT),
                    detail_path: None,
                });
            }
            TaskExecutionEvent::RuntimeEvent {
                seq,
                event,
                summary,
            } => {
                task.runtime_event_count = task.runtime_event_count.saturating_add(1);
                task.timeline.push(TaskTimelineEntry {
                    timestamp: Utc::now(),
                    kind: "runtime_event".to_string(),
                    summary: format!("#{seq} {event}: {summary}"),
                    detail_path: None,
                });
            }
        }

        self.persist_task_locked(task)?;
        Ok(())
    }

    async fn finish_task(
        &self,
        task_id: &str,
        mut result: TaskExecutionResult,
        cancel: CancellationToken,
        mode_label: &str,
    ) -> Result<()> {
        let mut state = self.state.lock().await;
        state.running_cancel.remove(task_id);
        let Some(task) = state.tasks.get_mut(task_id) else {
            return Ok(());
        };

        let now = Utc::now();
        if cancel.is_cancelled() && result.status == TaskStatus::Completed {
            result.status = TaskStatus::Canceled;
            result.result_text = None;
            result.error = None;
        }

        task.status = result.status;
        task.mode = mode_label.to_string();
        task.ended_at = Some(now);
        task.duration_ms = task.started_at.map(|start| duration_ms(start, now));
        task.error = result.error.clone();
        task.timeline.push(TaskTimelineEntry {
            timestamp: now,
            kind: "finished".to_string(),
            summary: match result.status {
                TaskStatus::Completed => "Task completed".to_string(),
                TaskStatus::Failed => format!(
                    "Task failed: {}",
                    result
                        .error
                        .as_deref()
                        .map(|e| summarize_text(e, TIMELINE_SUMMARY_LIMIT))
                        .unwrap_or_else(|| "unknown error".to_string())
                ),
                TaskStatus::Canceled => "Task canceled".to_string(),
                TaskStatus::Queued | TaskStatus::Running => {
                    format!("Task ended in unexpected state: {}", mode_label)
                }
            },
            detail_path: None,
        });

        if let Some(text) = result.result_text {
            let detail_path = self.artifact_if_large(task_id, "result", &text)?;
            task.result_summary = Some(summarize_text(&text, TIMELINE_SUMMARY_LIMIT));
            task.result_detail_path = detail_path.clone();
            if let Some(detail_path) = detail_path {
                task.timeline.push(TaskTimelineEntry {
                    timestamp: now,
                    kind: "result_ref".to_string(),
                    summary: format!("Result artifact: {}", detail_path.display()),
                    detail_path: Some(detail_path),
                });
            }
        } else if result.status == TaskStatus::Completed {
            task.result_summary = Some("(no textual output)".to_string());
        }

        self.persist_all_locked(&state)?;
        Ok(())
    }

    fn artifact_if_large(
        &self,
        task_id: &str,
        label: &str,
        content: &str,
    ) -> Result<Option<PathBuf>> {
        if content.len() < ARTIFACT_THRESHOLD {
            return Ok(None);
        }
        self.write_artifact(task_id, label, content).map(Some)
    }

    fn write_artifact(&self, task_id: &str, label: &str, content: &str) -> Result<PathBuf> {
        let artifact_dir = self.artifacts_dir.join(task_id);
        fs::create_dir_all(&artifact_dir)
            .with_context(|| format!("Failed to create artifact dir {}", artifact_dir.display()))?;
        let stamp = Utc::now().format("%Y%m%dT%H%M%S%.3fZ");
        let filename = format!("{stamp}_{}.txt", sanitize_filename(label));
        let absolute = artifact_dir.join(filename);
        fs::write(&absolute, content)
            .with_context(|| format!("Failed to write artifact {}", absolute.display()))?;
        let relative = absolute
            .strip_prefix(&self.cfg.data_dir)
            .map(PathBuf::from)
            .unwrap_or(absolute);
        Ok(relative)
    }

    fn apply_task_update_metadata(
        &self,
        task: &mut TaskRecord,
        metadata: Option<&Value>,
    ) -> Result<()> {
        let Some(updates) = metadata.and_then(|m| m.get("task_updates")) else {
            return Ok(());
        };
        let now = Utc::now();

        if let Some(value) = updates.get("checklist") {
            let mut checklist: TaskChecklistState = serde_json::from_value(value.clone())
                .context("Failed to parse checklist task update")?;
            checklist.updated_at = checklist.updated_at.or(Some(now));
            task.checklist = checklist;
            task.timeline.push(TaskTimelineEntry {
                timestamp: now,
                kind: "checklist".to_string(),
                summary: format!(
                    "Checklist updated: {} item(s), {}% complete",
                    task.checklist.items.len(),
                    task.checklist.completion_pct
                ),
                detail_path: None,
            });
        }

        if let Some(value) = updates.get("gate") {
            let gate: TaskGateRecord = serde_json::from_value(value.clone())
                .context("Failed to parse gate task update")?;
            let summary = format!("Gate {} {}: {}", gate.gate, gate.status, gate.summary);
            task.gates.retain(|existing| existing.id != gate.id);
            task.gates.push(gate.clone());
            task.timeline.push(TaskTimelineEntry {
                timestamp: now,
                kind: "gate".to_string(),
                summary: summarize_text(&summary, TIMELINE_SUMMARY_LIMIT),
                detail_path: gate.log_path,
            });
        }

        if let Some(value) = updates.get("attempt") {
            let attempt: TaskAttemptRecord = serde_json::from_value(value.clone())
                .context("Failed to parse attempt task update")?;
            task.attempts.retain(|existing| existing.id != attempt.id);
            task.attempts.push(attempt.clone());
            task.timeline.push(TaskTimelineEntry {
                timestamp: now,
                kind: "pr_attempt".to_string(),
                summary: format!(
                    "Attempt {}/{} recorded for {}",
                    attempt.attempt_index, attempt.attempt_count, attempt.attempt_group_id
                ),
                detail_path: attempt.patch_path,
            });
        }

        if let Some(value) = updates.get("artifacts")
            && let Some(items) = value.as_array()
        {
            for item in items {
                let artifact: TaskArtifactRef = serde_json::from_value(item.clone())
                    .context("Failed to parse artifact task update")?;
                task.timeline.push(TaskTimelineEntry {
                    timestamp: now,
                    kind: "artifact".to_string(),
                    summary: format!("{}: {}", artifact.label, artifact.summary),
                    detail_path: Some(artifact.path.clone()),
                });
                task.artifacts.push(artifact);
            }
        }

        if let Some(value) = updates.get("github_event") {
            let event: TaskGithubEvent = serde_json::from_value(value.clone())
                .context("Failed to parse GitHub task update")?;
            task.timeline.push(TaskTimelineEntry {
                timestamp: now,
                kind: "github".to_string(),
                summary: format!(
                    "{} {}#{}: {}",
                    event.action, event.target, event.number, event.summary
                ),
                detail_path: None,
            });
            task.github_events.push(event);
        }

        Ok(())
    }

    fn persist_all_locked(&self, state: &ManagerState) -> Result<()> {
        self.persist_queue_locked(&state.queue)?;
        for task in state.tasks.values() {
            self.persist_task_locked(task)?;
        }
        Ok(())
    }

    fn persist_queue_locked(&self, queue: &VecDeque<String>) -> Result<()> {
        write_json_atomic(
            &self.queue_path,
            &QueueFile {
                queue: queue.iter().cloned().collect(),
            },
        )
    }

    fn persist_task_locked(&self, task: &TaskRecord) -> Result<()> {
        let path = self.tasks_dir.join(format!("{}.json", task.id));
        write_json_atomic(&path, task)
    }
}
