#![cfg_attr(
    test,
    allow(
        clippy::cloned_ref_to_slice_refs,
        clippy::collapsible_if,
        clippy::field_reassign_with_default,
        clippy::items_after_test_module,
        clippy::needless_borrows_for_generic_args,
    )
)]

//! DeepSeek runtime library + HTTP sidecar (`deepseek-runtime`) — D6 Phase B single crate.

mod agent_surface;
mod audit;
mod auto_reasoning;
mod auto_route;
mod automation_manager;
pub mod cli;
mod client;
pub mod command_safety;
mod compaction;
mod config;
mod context_snapshot;
mod core;
mod cost_status;
mod cycle_manager;
mod error_taxonomy;
mod execpolicy;
mod features;
mod hooks;
mod hooks_load;
mod llm_client;
mod localization;
mod logging;
mod long_horizon;
mod lsp;
mod mcp_shared;
mod memory;
mod models;
mod office_env;
mod path_guard;
mod project_context;
mod project_doc;
mod prompts;
mod python_env;
pub mod repl;
mod request_fingerprint;
mod retry_status;
pub mod rlm;
pub mod runtime_api;
pub mod runtime_serve;
mod runtime_threads;
mod sandbox;
mod schema_migration;
mod scratchpad;
mod seam_manager;
mod settings;
mod shell_environment;
pub mod skills;
mod symbol_index;
mod task_manager;
mod task_type;
#[cfg(test)]
mod test_support;
mod tools;
mod topic_memory;
mod transcript_isomorphism;
#[cfg(feature = "tui")]
pub mod tui;
mod utils;
mod working_set;
mod workspace_trust;

// D16 E1-a — adapters crate (MCP / persist / snapshot); re-export for stable `crate::` paths.
pub use zagens_runtime_adapters::persist::{
    ContextReference, SavedSession, SessionContextReference, SessionManager, SessionMetadata,
    context_reference, session_manager, session_store_sqlite,
};
pub use zagens_runtime_adapters::{json_schema_util, mcp, network_policy, persist, snapshot};
pub use zagens_runtime_orchestrator::pricing;
// D16 E1-d — stable lib entry for in-proc / test hosts (see RUNTIME_ARCHITECTURE §1).
pub use runtime_serve::{RuntimeApiOptions, run_http_server};
