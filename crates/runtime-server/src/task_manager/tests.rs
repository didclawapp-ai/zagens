use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration as StdDuration;

use anyhow::{Result, bail};
use async_trait::async_trait;
use chrono::Utc;
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::{
    ExecutionTask, NewTaskRequest, TaskExecutionEvent, TaskExecutionResult, TaskExecutor,
    TaskManager, TaskManagerConfig, TaskRecord, TaskStatus,
};

/// Wait for a task to reach a terminal status (tests and API helpers).
pub(super) async fn wait_for_terminal_state(
    manager: &TaskManager,
    task_id: &str,
    timeout: StdDuration,
) -> Result<TaskRecord> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        let task = manager.get_task(task_id).await?;
        if task.status.is_terminal() {
            return Ok(task);
        }
        if std::time::Instant::now() >= deadline {
            bail!("Timed out waiting for task {task_id}");
        }
        sleep(StdDuration::from_millis(50)).await;
    }
}

struct MockExecutor;

#[async_trait]
impl TaskExecutor for MockExecutor {
    async fn execute(
        &self,
        task: ExecutionTask,
        events: mpsc::UnboundedSender<TaskExecutionEvent>,
        cancel: CancellationToken,
    ) -> TaskExecutionResult {
        let _ = events.send(TaskExecutionEvent::Status {
            message: format!("running {}", task.id),
        });
        let _ = events.send(TaskExecutionEvent::ThreadLinked {
            thread_id: "thr_test".to_string(),
            turn_id: "turn_test".to_string(),
        });
        let _ = events.send(TaskExecutionEvent::ToolStarted {
            id: "tool_1".to_string(),
            name: "read_file".to_string(),
            input: serde_json::json!({ "path": "README.md" }),
        });
        sleep(Duration::from_millis(50)).await;
        if cancel.is_cancelled() {
            return TaskExecutionResult {
                status: TaskStatus::Canceled,
                result_text: None,
                error: None,
            };
        }
        let _ = events.send(TaskExecutionEvent::ToolCompleted {
            id: "tool_1".to_string(),
            name: "read_file".to_string(),
            success: true,
            output: "read ok".to_string(),
            metadata: Some(serde_json::json!({
                "duration_ms": 10,
                "task_updates": {
                    "checklist": {
                        "items": [
                            { "id": 1, "content": "read fixture", "status": "in_progress" }
                        ],
                        "completion_pct": 0,
                        "in_progress_id": 1,
                        "updated_at": null
                    }
                }
            })),
        });
        TaskExecutionResult {
            status: TaskStatus::Completed,
            result_text: Some("done".to_string()),
            error: None,
        }
    }
}

fn test_config(root: PathBuf) -> TaskManagerConfig {
    TaskManagerConfig {
        data_dir: root,
        worker_count: 1,
        default_workspace: PathBuf::from("."),
        default_model: "deepseek-v4-flash".to_string(),
        default_mode: "agent".to_string(),
        allow_shell: false,
        trust_mode: false,
        max_subagents: 2,
    }
}

#[tokio::test]
async fn persists_and_recovers_task_records() -> Result<()> {
    let root = std::env::temp_dir().join(format!("deepseek-task-test-{}", Uuid::new_v4()));
    let manager =
        TaskManager::start_with_executor(test_config(root.clone()), Arc::new(MockExecutor)).await?;

    let task = manager
        .add_task(NewTaskRequest::from_prompt("test persistence"))
        .await?;
    let finished = wait_for_terminal_state(&manager, &task.id, Duration::from_secs(3)).await?;
    assert_eq!(finished.status, TaskStatus::Completed);
    assert_eq!(finished.thread_id.as_deref(), Some("thr_test"));
    assert_eq!(finished.turn_id.as_deref(), Some("turn_test"));
    assert_eq!(finished.checklist.items.len(), 1);
    assert_eq!(finished.checklist.in_progress_id, Some(1));

    drop(manager);

    let recovered =
        TaskManager::start_with_executor(test_config(root.clone()), Arc::new(MockExecutor)).await?;
    let loaded = recovered.get_task(&task.id).await?;
    assert_eq!(loaded.status, TaskStatus::Completed);
    assert!(!loaded.timeline.is_empty());
    assert_eq!(loaded.checklist.items[0].content, "read fixture");
    Ok(())
}

#[tokio::test]
async fn record_tool_metadata_updates_explicit_task() -> Result<()> {
    let root = std::env::temp_dir().join(format!("deepseek-task-test-{}", Uuid::new_v4()));
    let manager =
        TaskManager::start_with_executor(test_config(root), Arc::new(MockExecutor)).await?;

    let task = manager
        .add_task(NewTaskRequest::from_prompt("test metadata"))
        .await?;
    let finished = wait_for_terminal_state(&manager, &task.id, Duration::from_secs(3)).await?;
    let updated = manager
        .record_tool_metadata(
            &finished.id,
            &serde_json::json!({
                "task_updates": {
                    "gate": {
                        "id": "gate_test",
                        "gate": "test",
                        "command": "cargo test -p deepseek-runtime-server --lib",
                        "cwd": ".",
                        "exit_code": 0,
                        "status": "passed",
                        "classification": "passed",
                        "duration_ms": 1,
                        "summary": "ok",
                        "log_path": null,
                        "recorded_at": Utc::now()
                    }
                }
            }),
        )
        .await?;

    assert_eq!(updated.gates.len(), 1);
    assert_eq!(updated.gates[0].classification, "passed");
    Ok(())
}

#[tokio::test]
async fn cancel_running_task_marks_canceled() -> Result<()> {
    let root = std::env::temp_dir().join(format!("deepseek-task-test-{}", Uuid::new_v4()));
    let manager =
        TaskManager::start_with_executor(test_config(root), Arc::new(MockExecutor)).await?;

    let task = manager
        .add_task(NewTaskRequest::from_prompt("test cancellation"))
        .await?;

    sleep(Duration::from_millis(10)).await;
    let _ = manager.cancel_task(&task.id).await?;
    let finished = wait_for_terminal_state(&manager, &task.id, Duration::from_secs(3)).await?;
    assert_eq!(finished.status, TaskStatus::Canceled);
    Ok(())
}

#[tokio::test]
async fn rejects_newer_task_schema_on_recovery() -> Result<()> {
    let root = std::env::temp_dir().join(format!("deepseek-task-test-{}", Uuid::new_v4()));
    let manager =
        TaskManager::start_with_executor(test_config(root.clone()), Arc::new(MockExecutor)).await?;

    let task = manager
        .add_task(NewTaskRequest::from_prompt("test schema gate"))
        .await?;
    let _ = wait_for_terminal_state(&manager, &task.id, Duration::from_secs(3)).await?;
    drop(manager);

    let task_path = root.join("tasks").join(format!("{}.json", task.id));
    let mut value: serde_json::Value = serde_json::from_str(&fs::read_to_string(&task_path)?)?;
    value["schema_version"] = serde_json::json!(999);
    fs::write(&task_path, serde_json::to_string_pretty(&value)?)?;

    match TaskManager::start_with_executor(test_config(root), Arc::new(MockExecutor)).await {
        Ok(_) => panic!("manager should reject newer task schema"),
        Err(err) => assert!(err.to_string().contains("newer than supported")),
    }
    Ok(())
}
