//! CLI surface for headless `zagens` binary and shared args/types.

pub mod args;
pub mod auto_route_cli;
pub mod context;
pub mod dispatch;
pub mod doctor;
pub mod doctor_context;
pub mod doctor_tools;
pub mod entry;
pub mod failure_hint_registry;
pub mod handlers;
pub mod mcp_config;
pub mod pr_prompt;
pub mod runner;
pub mod setup;
pub(crate) mod trace_harness;
pub(crate) mod trace_thread;

pub use args::*;
pub use entry::configure_windows_console_utf8;

#[allow(unused_imports)]
pub(crate) use doctor::{
    McpServerDoctorStatus, doctor_api_target, doctor_check_mcp_server,
    doctor_timeout_recovery_lines,
};
#[allow(unused_imports)]
pub(crate) use pr_prompt::{GhPullRequest, format_pr_prompt};
#[allow(unused_imports)]
pub(crate) use setup::{
    ApiKeySource, WriteStatus, collect_clean_targets, default_checkpoints_dir, dotenv_status_line,
    execute_clean_plan, init_mcp_config, init_plugins_dir, init_tools_dir, is_command_available,
    merge_project_config, resolve_api_key_source, run_setup, run_setup_clean, run_setup_status,
    skills_count_for,
};

#[cfg(test)]
mod tests;
