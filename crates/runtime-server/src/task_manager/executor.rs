use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio::time::{Duration, sleep};
use tokio_util::sync::CancellationToken;

use crate::runtime_threads::{CreateThreadRequest, RuntimeTurnStatus, StartTurnRequest};
use zagens_runtime_orchestrator::runtime_threads::RuntimeThreadTaskPort;

use super::TaskStatus;
use super::helpers::{TIMELINE_SUMMARY_LIMIT, summarize_text};

#[derive(Debug, Clone)]
pub struct ExecutionTask {
    pub(crate) id: String,
    pub(crate) prompt: String,
    pub(crate) model: String,
    pub(crate) workspace: PathBuf,
    pub(crate) mode_label: String,
    pub(crate) allow_shell: bool,
    pub(crate) trust_mode: bool,
    pub(crate) auto_approve: bool,
}

/// Event stream produced by an executor while a task runs.
#[derive(Debug, Clone)]
pub enum TaskExecutionEvent {
    ThreadLinked {
        thread_id: String,
        turn_id: String,
    },
    Status {
        message: String,
    },
    MessageDelta {
        content: String,
    },
    ToolStarted {
        id: String,
        name: String,
        input: Value,
    },
    ToolProgress {
        id: String,
        output: String,
    },
    ToolCompleted {
        id: String,
        name: String,
        success: bool,
        output: String,
        metadata: Option<Value>,
    },
    Error {
        message: String,
    },
    RuntimeEvent {
        seq: u64,
        event: String,
        summary: String,
    },
}

/// Final executor result.
#[derive(Debug, Clone)]
pub struct TaskExecutionResult {
    pub status: TaskStatus,
    pub result_text: Option<String>,
    pub error: Option<String>,
}

/// Abstraction for task execution.
#[async_trait]
pub trait TaskExecutor: Send + Sync {
    async fn execute(
        &self,
        task: ExecutionTask,
        events: mpsc::UnboundedSender<TaskExecutionEvent>,
        cancel: CancellationToken,
    ) -> TaskExecutionResult;
}

/// Engine-backed executor (DeepSeek-only).
pub struct EngineTaskExecutor {
    runtime: Arc<dyn RuntimeThreadTaskPort>,
}

impl EngineTaskExecutor {
    #[must_use]
    pub fn new(runtime: Arc<dyn RuntimeThreadTaskPort>) -> Self {
        Self { runtime }
    }
}

#[async_trait]
impl TaskExecutor for EngineTaskExecutor {
    async fn execute(
        &self,
        task: ExecutionTask,
        events: mpsc::UnboundedSender<TaskExecutionEvent>,
        cancel: CancellationToken,
    ) -> TaskExecutionResult {
        let thread = match self
            .runtime
            .create_thread(CreateThreadRequest {
                model: Some(task.model.clone()),
                workspace: Some(task.workspace.clone()),
                mode: Some(task.mode_label.clone()),
                allow_shell: Some(task.allow_shell),
                trust_mode: Some(task.trust_mode),
                auto_approve: Some(task.auto_approve),
                archived: false,
                system_prompt: None,
                task_id: Some(task.id.clone()),
                task_type: None,
                use_worktree: None,
            })
            .await
        {
            Ok(thread) => thread,
            Err(err) => {
                return TaskExecutionResult {
                    status: TaskStatus::Failed,
                    result_text: None,
                    error: Some(format!("Failed to create runtime thread: {err}")),
                };
            }
        };

        let turn = match self
            .runtime
            .start_turn(
                &thread.id,
                StartTurnRequest {
                    prompt: task.prompt.clone(),
                    input_summary: Some(summarize_text(&task.prompt, TIMELINE_SUMMARY_LIMIT)),
                    model: Some(task.model.clone()),
                    mode: Some(task.mode_label.clone()),
                    allow_shell: Some(task.allow_shell),
                    trust_mode: Some(task.trust_mode),
                    auto_approve: Some(task.auto_approve),
                    route_intent: None,
                    ..Default::default()
                },
            )
            .await
        {
            Ok(turn) => turn,
            Err(err) => {
                return TaskExecutionResult {
                    status: TaskStatus::Failed,
                    result_text: None,
                    error: Some(format!("Failed to start task: {err}")),
                };
            }
        };

        let _ = events.send(TaskExecutionEvent::ThreadLinked {
            thread_id: thread.id.clone(),
            turn_id: turn.id.clone(),
        });
        let _ = events.send(TaskExecutionEvent::Status {
            message: format!("Task {} started", task.id),
        });

        let mut final_text = String::new();
        let mut seen_seq = 0u64;
        let mut cancel_requested = false;
        let mut terminal_status: Option<RuntimeTurnStatus> = None;
        let mut terminal_error: Option<String> = None;

        loop {
            if cancel.is_cancelled() && !cancel_requested {
                cancel_requested = true;
                let _ = self.runtime.interrupt_turn(&thread.id, &turn.id).await;
                let _ = events.send(TaskExecutionEvent::Status {
                    message: "Cancellation requested".to_string(),
                });
            }

            let batch = match self
                .runtime
                .events_since_async(&thread.id, Some(seen_seq))
                .await
            {
                Ok(batch) => batch,
                Err(err) => {
                    return TaskExecutionResult {
                        status: TaskStatus::Failed,
                        result_text: if final_text.trim().is_empty() {
                            None
                        } else {
                            Some(final_text)
                        },
                        error: Some(format!("Failed to read runtime events: {err}")),
                    };
                }
            };

            for event in batch {
                seen_seq = seen_seq.max(event.seq);
                let _ = events.send(TaskExecutionEvent::RuntimeEvent {
                    seq: event.seq,
                    event: event.event.clone(),
                    summary: summarize_text(&event.payload.to_string(), TIMELINE_SUMMARY_LIMIT),
                });

                match event.event.as_str() {
                    "item.delta" => {
                        let kind = event
                            .payload
                            .get("kind")
                            .and_then(Value::as_str)
                            .unwrap_or_default();
                        if kind == "agent_message" {
                            if let Some(content) =
                                event.payload.get("delta").and_then(Value::as_str)
                            {
                                final_text.push_str(content);
                                let _ = events.send(TaskExecutionEvent::MessageDelta {
                                    content: content.to_string(),
                                });
                            }
                        } else if kind == "tool_call" {
                            let output = event
                                .payload
                                .get("delta")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string();
                            let _ = events.send(TaskExecutionEvent::ToolProgress {
                                id: event.item_id.clone().unwrap_or_default(),
                                output,
                            });
                        }
                    }
                    "item.started" => {
                        if let Some(tool) = event.payload.get("tool") {
                            let id = tool
                                .get("id")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string();
                            let name = tool
                                .get("name")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string();
                            let input = tool.get("input").cloned().unwrap_or_else(|| json!({}));
                            let _ =
                                events.send(TaskExecutionEvent::ToolStarted { id, name, input });
                        }
                    }
                    "item.completed" | "item.failed" => {
                        if let Some(item) = event.payload.get("item") {
                            let kind = item.get("kind").and_then(Value::as_str).unwrap_or_default();
                            if kind == "tool_call"
                                || kind == "file_change"
                                || kind == "command_execution"
                            {
                                let id = item
                                    .get("id")
                                    .and_then(Value::as_str)
                                    .unwrap_or_default()
                                    .to_string();
                                let name = item
                                    .get("summary")
                                    .and_then(Value::as_str)
                                    .unwrap_or("tool")
                                    .split(':')
                                    .next()
                                    .unwrap_or("tool")
                                    .trim()
                                    .to_string();
                                let output = item
                                    .get("detail")
                                    .and_then(Value::as_str)
                                    .unwrap_or_default()
                                    .to_string();
                                let metadata = item.get("metadata").cloned();
                                let _ = events.send(TaskExecutionEvent::ToolCompleted {
                                    id,
                                    name,
                                    success: event.event == "item.completed",
                                    output,
                                    metadata,
                                });
                            } else if kind == "status" {
                                let message = item
                                    .get("detail")
                                    .and_then(Value::as_str)
                                    .or_else(|| item.get("summary").and_then(Value::as_str))
                                    .unwrap_or_default()
                                    .to_string();
                                let _ = events.send(TaskExecutionEvent::Status { message });
                            } else if kind == "error" {
                                let message = item
                                    .get("detail")
                                    .and_then(Value::as_str)
                                    .or_else(|| item.get("summary").and_then(Value::as_str))
                                    .unwrap_or_default()
                                    .to_string();
                                let _ = events.send(TaskExecutionEvent::Error { message });
                            }
                        }
                    }
                    "turn.completed" => {
                        if let Some(turn_payload) = event.payload.get("turn") {
                            let status = turn_payload
                                .get("status")
                                .and_then(Value::as_str)
                                .unwrap_or("failed");
                            terminal_status = Some(match status {
                                "completed" => RuntimeTurnStatus::Completed,
                                "interrupted" => RuntimeTurnStatus::Interrupted,
                                "canceled" => RuntimeTurnStatus::Canceled,
                                _ => RuntimeTurnStatus::Failed,
                            });
                            terminal_error = turn_payload
                                .get("error")
                                .and_then(Value::as_str)
                                .map(ToString::to_string);
                        } else {
                            terminal_status = Some(RuntimeTurnStatus::Completed);
                        }
                    }
                    _ => {}
                }
            }

            if terminal_status.is_some() {
                break;
            }

            sleep(Duration::from_millis(40)).await;
        }

        match terminal_status.unwrap_or(RuntimeTurnStatus::Failed) {
            RuntimeTurnStatus::Completed => TaskExecutionResult {
                status: TaskStatus::Completed,
                result_text: if final_text.trim().is_empty() {
                    None
                } else {
                    Some(final_text)
                },
                error: None,
            },
            RuntimeTurnStatus::Interrupted | RuntimeTurnStatus::Canceled => TaskExecutionResult {
                status: TaskStatus::Canceled,
                result_text: if final_text.trim().is_empty() {
                    None
                } else {
                    Some(final_text)
                },
                error: None,
            },
            RuntimeTurnStatus::Queued
            | RuntimeTurnStatus::InProgress
            | RuntimeTurnStatus::Failed => TaskExecutionResult {
                status: TaskStatus::Failed,
                result_text: if final_text.trim().is_empty() {
                    None
                } else {
                    Some(final_text)
                },
                error: terminal_error.or_else(|| Some("Task ended unexpectedly".to_string())),
            },
        }
    }
}
