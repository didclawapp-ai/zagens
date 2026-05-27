//! DeepSeek runtime library + HTTP sidecar (`deepseek-runtime`) — D6 Phase B single crate.

mod agent_surface;
mod audit;
mod auto_reasoning;
mod auto_route;
mod automation_manager;
mod client;
mod command_safety;
mod compaction;
mod context_snapshot;
mod config;
mod core;
mod cost_status;
mod cycle_manager;
mod error_taxonomy;
mod execpolicy;
mod features;
mod hooks;
mod llm_client;
mod localization;
mod logging;
mod lsp;
mod memory;
mod models;
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
mod thread_store_sqlite;
mod settings;
mod skills;
mod task_type;
mod topic_memory;
pub mod cli;
mod symbol_index;
mod task_manager;
#[cfg(test)]
mod test_support;
mod tools;
mod transcript_isomorphism;
mod utils;
mod working_set;
mod workspace_trust;

// D16 E1-a — adapters crate (MCP / persist / snapshot); re-export for stable `crate::` paths.
pub use deepseek_runtime_adapters::{
    json_schema_util, mcp, network_policy, persist, snapshot,
};
pub use deepseek_runtime_adapters::persist::{
    context_reference, session_manager, session_store_sqlite, ContextReference,
    SavedSession, SessionContextReference, SessionManager, SessionMetadata,
};
