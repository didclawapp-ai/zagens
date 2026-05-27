//! Runtime adapters — MCP, session persist, snapshots (D16 E1-a).
//!
//! Extracted from `deepseek-runtime-server` to begin the sidecar crate split.
//! `tools/` migration follows once host-boundary refactors land (see D16 §1.4).

pub mod json_schema_util;
pub mod mcp;
pub mod models;
pub mod network_policy;
pub mod persist;
pub mod snapshot;
pub mod util;

#[cfg(test)]
mod test_support;
