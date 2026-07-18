//! Runtime HTTP/SSE API for local DeepSeek automation.

use std::path::PathBuf;
use std::sync::Arc;

use crate::automation_manager::SharedAutomationManager;
use crate::config::Config;
use crate::runtime_threads::SharedRuntimeThreadManager;
use crate::session_manager::SessionManager;
use crate::task_manager::SharedTaskManager;

pub mod openapi;

mod agent_health;
mod automations;
mod blackboards;
pub(crate) mod channel_events;
pub(crate) mod kernel_replay;
mod mcp;
mod night_queue;
mod office;
mod router;
mod runtime_status;
mod sessions;
mod skills;
mod state;
mod stream;
mod symbol_search;
mod tasks;
mod thread_config;
mod threads;
mod topic_memory;
mod trace_compare;
mod trace_report;
mod usage;
pub(crate) mod workspace;

pub(crate) use agent_health::get_agent_health;
pub(crate) use automations::{
    create_automation, delete_automation, get_automation, list_automation_runs, list_automations,
    pause_automation, resume_automation, run_automation, update_automation,
};
pub(crate) use blackboards::{get_blackboard, list_blackboards};
pub(crate) use channel_events::post_thread_channel_event;
pub(crate) use kernel_replay::{get_kernel_thread_replay, get_kernel_turn_replay};
pub(crate) use mcp::{
    add_mcp_server, delete_mcp_server, discover_mcp, get_mcp_server, list_mcp_calls,
    list_mcp_servers, list_mcp_tools, merge_mcp_config_json, reload_mcp_config, update_mcp_server,
};
pub(crate) use night_queue::{
    create_night_queue_task, get_night_queue, list_gate_presets, post_night_queue_briefing,
    run_night_queue,
};
pub(crate) use office::get_office_environment;
pub(crate) use sessions::{
    delete_session, get_resume_task, get_session, list_sessions, resume_session_thread,
};
pub(crate) use skills::{create_skill, import_skill_local, install_skill_remote, list_skills};
pub(crate) use symbol_search::search_symbol_index;
pub(crate) use tasks::{cancel_task, clear_tasks, create_task, get_task, list_tasks};
pub(crate) use thread_config::{delete_thread_config_field, get_thread_config, put_thread_config};
pub(crate) use threads::{
    browse_thread_workspace, browse_workspace_by_root, compact_thread, create_thread,
    edit_last_thread_turn, fork_thread, fork_thread_at_user_message, get_thread,
    get_thread_checklist, get_thread_context, get_thread_context_breakdown,
    get_thread_harness_cycles, get_thread_harness_task_graph, get_thread_scratchpad_status,
    init_thread_scratchpad, interrupt_thread_turn, list_thread_snapshots, list_threads,
    list_threads_summary, persist_thread_session, read_thread_workspace_file,
    read_workspace_file_by_root, resolve_approval, restore_thread_snapshot, resume_thread,
    revert_thread_workspace_turn, start_thread_turn, steer_thread_turn, update_thread,
};
pub(crate) use topic_memory::get_topic_memory;
pub(crate) use trace_compare::get_trace_compare;
pub(crate) use trace_report::get_thread_trace_report;
pub(crate) use usage::{get_routing_rules, get_usage, rebuild_symbol_index, set_routing_rules};
pub(crate) use workspace::{
    workspace_changes, workspace_file_diff, workspace_pulls, workspace_snapshot, workspace_status,
};

pub use router::build_router;

pub(crate) use runtime_status::get_runtime_active_turns;
pub(crate) use sessions::ResumeTaskTracker;

#[cfg(test)]
pub(crate) use zagens_runtime_api::cors_layer;

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
    /// Shared MCP connection pool (hot-reload target for all engines).
    shared_mcp_pool: std::sync::Arc<tokio::sync::Mutex<crate::mcp::McpPool>>,
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
        shared_mcp_pool: std::sync::Arc<tokio::sync::Mutex<crate::mcp::McpPool>>,
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
            shared_mcp_pool,
        }
    }

    pub(crate) fn config(&self) -> &Config {
        &self.config
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

pub(crate) use zagens_runtime_api::ApiError;

pub(crate) fn map_thread_err(err: anyhow::Error) -> ApiError {
    let message = err.to_string();
    if message.contains("not found") {
        ApiError::not_found(message)
    } else if message.contains("already has an active turn")
        || message.contains("No active turn")
        || message.contains("no active turn")
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
