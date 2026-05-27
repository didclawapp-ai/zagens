//! Runtime HTTP/SSE API for local DeepSeek automation.

use std::path::PathBuf;
use std::sync::Arc;

use crate::automation_manager::SharedAutomationManager;
use crate::config::Config;
use crate::runtime_threads::SharedRuntimeThreadManager;
use crate::session_manager::SessionManager;
use crate::task_manager::SharedTaskManager;

pub mod openapi;

mod automations;
mod blackboards;
mod mcp;
mod router;
mod sessions;
mod skills;
mod state;
mod stream;
mod tasks;
mod topic_memory;
mod threads;
mod usage;
mod workspace;

pub(crate) use blackboards::{get_blackboard, list_blackboards};
pub(crate) use topic_memory::get_topic_memory;
pub(crate) use automations::{
    create_automation, delete_automation, get_automation, list_automation_runs, list_automations,
    pause_automation, resume_automation, run_automation, update_automation,
};
pub(crate) use usage::{get_routing_rules, get_usage, rebuild_symbol_index, set_routing_rules};
pub(crate) use workspace::workspace_status;
pub(crate) use mcp::{
    add_mcp_server, delete_mcp_server, get_mcp_server, list_mcp_servers, list_mcp_tools,
    merge_mcp_config_json, update_mcp_server,
};
pub(crate) use sessions::{
    delete_session, get_resume_task, get_session, list_sessions, resume_session_thread,
};
pub(crate) use skills::{create_skill, import_skill_local, install_skill_remote, list_skills};
pub(crate) use tasks::{cancel_task, clear_tasks, create_task, get_task, list_tasks};
pub(crate) use threads::{
    browse_thread_workspace, browse_workspace_by_root, compact_thread, create_thread,
    edit_last_thread_turn, fork_thread, fork_thread_at_user_message, get_thread, get_thread_checklist, get_thread_context,
    get_thread_scratchpad_status, init_thread_scratchpad, interrupt_thread_turn, list_thread_snapshots, list_threads,
    list_threads_summary, persist_thread_session, read_thread_workspace_file,
    read_workspace_file_by_root, resolve_approval, restore_thread_snapshot, resume_thread,
    start_thread_turn, steer_thread_turn, update_thread,
};

pub use router::build_router;

pub(crate) use sessions::ResumeTaskTracker;

#[cfg(test)]
pub(crate) use deepseek_runtime_api::cors_layer;

#[derive(Clone)]
pub struct RuntimeApiState {
    config: Config,
    workspace: PathBuf,
    task_manager: SharedTaskManager,
    runtime_threads: SharedRuntimeThreadManager,
    cors_origins: Vec<String>,
    mcp_config_path: PathBuf,
    automations: SharedAutomationManager,
    runtime_token: Option<String>,
    process_started_at_ms: u128,
    token_fingerprint: Arc<String>,
    shared_session_manager: Arc<SessionManager>,
    resume_tracker: sessions::ResumeTaskTracker,
}

impl RuntimeApiState {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        config: Config,
        workspace: PathBuf,
        task_manager: SharedTaskManager,
        runtime_threads: SharedRuntimeThreadManager,
        cors_origins: Vec<String>,
        mcp_config_path: PathBuf,
        automations: SharedAutomationManager,
        runtime_token: Option<String>,
        process_started_at_ms: u128,
        token_fingerprint: Arc<String>,
        shared_session_manager: Arc<SessionManager>,
        resume_tracker: sessions::ResumeTaskTracker,
    ) -> Self {
        Self {
            config,
            workspace,
            task_manager,
            runtime_threads,
            cors_origins,
            mcp_config_path,
            automations,
            runtime_token,
            process_started_at_ms,
            token_fingerprint,
            shared_session_manager,
            resume_tracker,
        }
    }
}

pub(crate) fn truncate_text(text: &str, max_chars: usize) -> String {
    let char_count = text.chars().count();
    if char_count <= max_chars {
        return text.to_string();
    }
    let truncated: String = text.chars().take(max_chars.saturating_sub(3)).collect();
    format!("{truncated}...")
}

pub(crate) use deepseek_runtime_api::ApiError;

pub(crate) fn map_thread_err(err: anyhow::Error) -> ApiError {
    let message = err.to_string();
    if message.contains("not found") {
        ApiError::not_found(message)
    } else if message.contains("already has an active turn")
        || message.contains("No active turn")
        || message.contains("is not active")
        || message.contains("no pending approval for")
        || message.contains("pending approval scope mismatch")
    {
        ApiError::conflict(message)
    } else {
        ApiError::bad_request(message)
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
