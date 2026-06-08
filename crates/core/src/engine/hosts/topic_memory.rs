//! [`TopicMemoryHost`] — engine boundary for the topic-memory graph
//! integration (`B2` system-prompt injection).
//!
//! M5 (Engine-struct strangler step) introduces this host trait so the
//! future core-side `Engine` struct (M7) can hold `Box<dyn
//! TopicMemoryHost>` without taking a tui dependency on
//! `crates/tui/src/topic_memory.rs` (307 LOC) or transitively on the
//! `zagens-topic-memory` workspace crate (spike R9 — prefer
//! adapter-tui-side option (a) over crate dep option (b)).
//!
//! ## Call-graph (R1)
//!
//! Method surface derived from the live `Engine`'s direct calls on the
//! `topic_memory_runtime` field:
//!
//! | Method            | Call site                                                                          |
//! |-------------------|------------------------------------------------------------------------------------|
//! | `compose_block`   | `crates/tui/src/core/engine/cycle_hooks.rs:243` (in `refresh_system_prompt_with_arbitration`) |
//! | `on_turn_complete`| `crates/tui/src/core/engine/message_handlers.rs:277`                               |
//!
//! ## Why no `settings` parameter?
//!
//! `TopicMemorySettings` is a tui-side type wrapping
//! `zagens-topic-memory` defaults. Passing `&TopicMemorySettings`
//! through the trait would force this core module to either depend on
//! that crate (spike R9 option (b), rejected) or define a parallel
//! settings struct (premature core-side leakage).
//!
//! M5 instead moves settings **into the implementation**:
//! `TopicMemoryRuntime` gains an owned `TopicMemorySettings` field at
//! construction (`TopicMemoryRuntime::new(settings)`) so the trait
//! methods see settings via `self`. Settings hot-reload is not
//! currently supported (no slash command updates it), so single-shot
//! ownership at engine init is sufficient.

/// Engine-side topic-memory host.
///
/// Implemented by `crates/tui/src/topic_memory.rs`'s
/// `TopicMemoryRuntime`, which owns its `TopicMemorySettings` and
/// runs-since-last-inject counter.
pub trait TopicMemoryHost: Send + Sync {
    /// Build the `<topic_memory>` system-prompt block, if topic
    /// memory is enabled in the configured settings and the inject
    /// cadence is met. `query_hint` is the latest user-message text
    /// (used for k-hop retrieval); pass `None` to fall back to the
    /// full graph.
    ///
    /// Returns the rendered block string (already wrapped in the
    /// `<topic_memory>...</topic_memory>` XML tag and attribution
    /// comment), or `None` when the inject cadence is not met /
    /// memory is disabled.
    fn compose_block(&mut self, query_hint: Option<&str>) -> Option<String>;

    /// Update the topic-memory graph after a turn completes. `user`
    /// and `assistant` are the most recent exchange text — extracted
    /// topics flow into the graph, decay is applied, and the on-disk
    /// metrics file is updated.
    ///
    /// Called from the post-turn bookkeeping path
    /// (`message_handlers.rs:277`).
    fn on_turn_complete(&mut self, user: &str, assistant: &str);
}
