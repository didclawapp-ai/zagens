//! Thread-level context usage snapshot — pure data type (M1 → `zagens-core`).
//!
//! The companion `build_thread_context_snapshot` helper still lives in
//! `crates/tui/src/context_snapshot.rs` because it depends on the tui-only
//! `compaction::should_compact` (which is ~1k LOC of tui-specific working-set
//! / pinning logic). Moving the helper would require dragging that whole
//! chain into core — out of scope for M1 (see `PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE`
//! §1.2). Only the wire-shape struct moves here so `Op::QueryContext.reply`
//! can be a core type.

use serde::{Deserialize, Serialize};

/// Context fill + compaction policy snapshot for a thread.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ThreadContextSnapshot {
    /// Conservative estimate (`estimate_input_tokens_conservative`) for
    /// compaction / overflow.
    pub estimated_input_tokens: usize,
    pub context_window_tokens: u32,
    /// Percent from conservative estimate (primary UI ring).
    pub usage_percent: f64,
    pub message_count: usize,
    pub compaction_enabled: bool,
    pub compaction_threshold_tokens: usize,
    pub compaction_floor_tokens: usize,
    pub should_compact: bool,
    /// Provider `usage.input_tokens` from the last API round (authoritative
    /// per DeepSeek docs).
    pub last_api_input_tokens: Option<u32>,
    /// Percent from `last_api_input_tokens` when present.
    pub last_api_usage_percent: Option<f64>,
    /// Deprecated: last turn's **summed** `usage.input_tokens` (multi-round
    /// turns inflate).
    pub last_reported_input_tokens: Option<u32>,
    /// `engine` when read from a loaded engine; `store` when reconstructed
    /// from persisted turns.
    pub source: String,
}
