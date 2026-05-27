//! Persistent background task manager for DeepSeek agent work.
//!
//! Tasks are durable across restarts and execute with a bounded worker pool.
//! Execution stays DeepSeek-only and now links every task to runtime
//! thread/turn records for unified timelines.

pub(crate) const DEFAULT_WORKERS: usize = 2;
pub(crate) const MAX_WORKERS: usize = 8;

mod config;
mod executor;
mod helpers;
mod manager;
mod persist;

#[cfg(test)]
mod tests;

pub use config::TaskManagerConfig;
pub use deepseek_runtime_api::task::{
    CURRENT_TASK_SCHEMA_VERSION, NewTaskRequest, TaskArtifactRef, TaskAttemptRecord,
    TaskChecklistItem, TaskChecklistState, TaskCounts, TaskGateRecord, TaskGithubEvent,
    TaskRecord, TaskStatus, TaskSummary, TaskTimelineEntry, TaskToolCallSummary, TaskToolStatus,
};
pub use executor::{
    EngineTaskExecutor, ExecutionTask, TaskExecutionEvent, TaskExecutionResult, TaskExecutor,
};
pub use manager::{SharedTaskManager, TaskManager};
pub use persist::default_tasks_dir;
