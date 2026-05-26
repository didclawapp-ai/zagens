//! CLI surface retained for args/types and headless helpers (D6 Phase B — no full CLI binary).

pub mod args;
#[cfg(test)]
pub(crate) mod doctor;
pub mod entry;
#[cfg(test)]
pub(crate) mod pr_prompt;
#[cfg(test)]
pub(crate) mod setup;

pub use args::*;
pub use entry::configure_windows_console_utf8;

#[cfg(test)]
pub(crate) use doctor::{
    doctor_api_target, doctor_check_mcp_server, doctor_timeout_recovery_lines,
    McpServerDoctorStatus,
};
#[cfg(test)]
pub(crate) use pr_prompt::{format_pr_prompt, GhPullRequest};
#[cfg(test)]
pub(crate) use setup::{
    ApiKeySource, WriteStatus, collect_clean_targets, dotenv_status_line, execute_clean_plan,
    init_plugins_dir, init_tools_dir, is_command_available, merge_project_config,
    resolve_api_key_source, run_setup_clean, skills_count_for,
};

#[cfg(test)]
mod tests;
