//! Runtime adapters — MCP, session persist, snapshots (D16 E1-a, D17 frozen).
//!
//! Extracted from `zagens-cli` as part of the D16 sidecar crate
//! split.  Further migration of engine, tools, and route handlers is **deferred
//! by design** (D17 Architecture Freeze) — those remain internal co-located
//! units in `zagens-cli`, not candidates for crate extraction.
//! This crate is the stable boundary for MCP connectors, persistence adapters,
//! scratchpad gates, snapshots, and network policy.

pub mod http_client;
pub mod json_schema_util;
pub mod mcp;
pub mod models;
pub mod network_policy;
pub mod persist;
pub mod scratchpad;
pub mod scratchpad_gates;
pub mod snapshot;
pub mod tools;
pub mod util;

#[cfg(test)]
mod test_support;
