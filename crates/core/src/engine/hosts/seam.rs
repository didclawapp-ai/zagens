//! [`SeamHost`] — engine boundary for the Flash seam manager (issue
//! #159 layered-context pipeline).
//!
//! M5 (Engine-struct strangler step) introduces this host trait so the
//! future core-side `Engine` struct (M7) can hold `Option<Box<dyn
//! SeamHost>>` without taking a tui dependency on
//! `crates/tui/src/seam_manager.rs` (712 LOC).
//!
//! ## Call-graph (R1)
//!
//! Method surface derived from the live `Engine`'s direct calls on the
//! `seam_manager` field (M5 R1 mitigation — strictly call-graph
//! driven, **not** a dump of `SeamManager`'s public API):
//!
//! | Method                    | Call site                                                   |
//! |---------------------------|-------------------------------------------------------------|
//! | `config_enabled`          | `crates/tui/src/core/engine/layered_context.rs:15`          |
//! | `highest_level`           | `crates/tui/src/core/engine/layered_context.rs:19`          |
//! | `seam_level_for`          | `crates/tui/src/core/engine/layered_context.rs:20`          |
//! | `verbatim_window_start`   | `crates/tui/src/core/engine/layered_context.rs:28`          |
//! | `collect_seam_texts`      | `cycle_hooks.rs:56` + `layered_context.rs:47`               |
//! | `produce_soft_seam`       | `layered_context.rs:49-57`                                  |
//! | `recompact`               | `layered_context.rs:70-72`                                  |
//! | `seam_count`              | `layered_context.rs:87`                                     |
//! | `produce_flash_briefing`  | `cycle_hooks.rs:70-72`                                      |
//! | `reset`                   | `cycle_hooks.rs:195`                                        |
//!
//! Inherent `SeamManager` methods *not* on this trait (because Engine
//! does not call them directly):
//!   - `new(client, config)` — construction is a tui-side concern
//!     (depends on `LlmClient` factory).
//!   - `should_cycle(active_input_tokens)` — currently dead code, kept
//!     in `SeamManager` for future wiring.
//!   - `summarize_messages(...)` — private helper used by
//!     `produce_soft_seam` / `recompact` internally.
//!
//! ## Why `config_enabled` instead of `config(&self) -> &SeamConfig`?
//!
//! `SeamConfig` is a tui-only type (`crates/tui/src/seam_manager.rs:64`)
//! and lifting it into core would create a brand-new config dependency
//! for **one** boolean Engine reads (`.enabled`). The trait surface
//! only exposes the single bit Engine actually consumes; richer config
//! introspection (`l1_threshold`, `seam_model`, …) stays inside the
//! tui-side `SeamManager` and feeds the other trait methods (e.g.
//! `seam_level_for`) directly.

use std::path::Path;

use async_trait::async_trait;

use crate::chat::Message;

/// Opaque error from a seam operation.
///
/// Boxed `dyn Error` keeps the trait surface free of tui-side error
/// hierarchies (`anyhow::Error`, `reqwest::Error`, `LlmClientError`,
/// `serde_json::Error`). Call sites already format errors via
/// `format!("{err}")` for status events (`cycle_hooks.rs:75-100`,
/// `layered_context.rs:62, 76`); the `Display` blanket of `dyn Error`
/// preserves the existing log shape.
pub type SeamError = Box<dyn std::error::Error + Send + Sync + 'static>;

/// Engine-side Flash seam host.
///
/// Implemented by `crates/tui/src/seam_manager.rs`'s `SeamManager`
/// (M5 inline `impl SeamHost`). All ten methods are thin UFCS
/// delegations to the existing inherent methods on `SeamManager` — M5
/// adds zero behavior, only the trait surface.
#[async_trait]
pub trait SeamHost: Send + Sync {
    /// Whether the layered-context seam pipeline is enabled in config.
    fn config_enabled(&self) -> bool;

    /// Highest seam level the manager has emitted so far this session
    /// (`None` when no seams have been produced yet).
    async fn highest_level(&self) -> Option<u8>;

    /// Decide which soft-seam level (1/2/3) to produce for the given
    /// active-request input-token estimate and the manager's current
    /// highest level. Returns `None` when no seam is due.
    fn seam_level_for(
        &self,
        active_input_tokens: usize,
        highest_existing_level: Option<u8>,
    ) -> Option<u8>;

    /// First message index of the verbatim "recent" window —
    /// everything before this index is candidate for soft-seam
    /// summarization. Returns `0` when the session has fewer messages
    /// than the verbatim window.
    fn verbatim_window_start(&self, message_count: usize) -> usize;

    /// Collect already-produced `<archived_context>` texts embedded in
    /// the assistant messages (for downstream recompaction or briefing).
    async fn collect_seam_texts(&self, messages: &[Message]) -> Vec<String>;

    /// Produce a fresh soft seam at the given level over the message
    /// range `[start_idx, end_idx)`. `pinned_indices` are excluded
    /// from summarization (always kept verbatim).
    #[allow(clippy::too_many_arguments)]
    async fn produce_soft_seam(
        &self,
        messages: &[Message],
        level: u8,
        start_idx: usize,
        end_idx: usize,
        workspace: Option<&Path>,
        pinned_indices: &[usize],
    ) -> Result<String, SeamError>;

    /// Re-compact existing seam blocks with the recent message slice
    /// into a single updated seam at `level`. Consumes prior
    /// `<archived_context>` content and fuses it with the recent range.
    async fn recompact(
        &self,
        existing_seams: &[String],
        recent: &[&Message],
        level: u8,
        start_idx: usize,
        end_idx: usize,
    ) -> Result<String, SeamError>;

    /// Total active seam blocks currently held. Used in the post-seam
    /// status line emitted after a checkpoint.
    async fn seam_count(&self) -> usize;

    /// Produce the Flash briefing used at cycle-advance boundaries.
    /// `state_text` is the `StructuredState` system-block snapshot
    /// (mode label, workspace, working set, todos, plan, sub-agents).
    async fn produce_flash_briefing(
        &self,
        existing_seams: &[String],
        state_text: Option<&str>,
    ) -> Result<String, SeamError>;

    /// Clear seam-tracking state at the cycle boundary. Called from
    /// `Engine::advance_cycle` after the new seed messages have been
    /// swapped in.
    async fn reset(&self);
}
