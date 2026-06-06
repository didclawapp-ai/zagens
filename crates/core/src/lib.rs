//! Core runtime boundaries for the embedded agent engine (turn loop, hosts, pure logic).
//!
//! Production turns run via `RuntimeThreadManager` in the sidecar — not legacy CLI `Runtime`.

pub mod approval;
pub mod capacity;
pub mod chat;
pub mod coherence;
pub mod compaction;
pub mod context_partition;
pub mod cycle;
pub mod engine;
pub mod error_taxonomy;
pub mod events;
pub mod features;
pub mod long_horizon;
pub mod lsp;
pub mod models;
pub mod project_context;
pub mod sandbox;
pub mod scratchpad;
pub mod session;
pub mod subagent;
pub mod task_type;
pub mod turn;
pub mod user_input;
pub mod working_set;
pub mod workshop;

#[cfg(test)]
mod test_support;
