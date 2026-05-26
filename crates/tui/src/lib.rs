//! DeepSeek TUI — shared library crate.
//!
//! This is the library target for `deepseek-tui`.  The binary entry point
//! lives in `main.rs` and delegates to this crate via `deepseek_tui::*`.

// acp_server is a bin-only module (depends on main.rs-local items).
// mod acp_server;
mod agent_surface;
mod audit;
mod auto_reasoning;
mod auto_route;
mod automation_manager;
mod client;
mod command_safety;
#[cfg(feature = "tui-ui")]
mod commands;
mod compaction;
mod context_snapshot;
mod composer_history;
mod composer_stash;
mod config;
mod context_reference;
#[cfg(feature = "tui-ui")]
mod config_ui;
mod core;
mod cost_status;
mod cycle_manager;
#[cfg(feature = "tui-ui")]
mod deepseek_theme;
mod error_taxonomy;
mod eval;
mod execpolicy;
mod features;
mod handoff;
mod hooks;
mod json_schema_util;
mod llm_client;
mod localization;
mod logging;
mod lsp;
mod mcp;
mod mcp_server;
mod memory;
mod models;
mod network_policy;
#[cfg(feature = "tui-ui")]
mod palette;
mod path_guard;
mod pricing;
mod project_context;
mod project_doc;
mod prompts;
mod python_env;
pub mod repl;
mod retry_status;
pub mod rlm;
pub mod runtime_api;
pub mod runtime_serve;
mod runtime_threads;
mod sandbox;
mod schema_migration;
mod scratchpad;
mod seam_manager;
mod session_manager;
mod session_store_sqlite;
mod thread_store_sqlite;
mod settings;
mod skills;
mod task_type;
mod topic_memory;
mod snapshot;
pub mod cli;
mod symbol_index;
mod task_manager;
#[cfg(test)]
mod test_support;
mod tools;
#[cfg(feature = "tui-ui")]
mod tui;
mod utils;
mod working_set;
mod workspace_trust;
