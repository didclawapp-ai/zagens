//! Per-step scratchpad nudge state (B3/B4/B3b/C0/C1 audit scratchpad
//! engine hooks).
//!
//! M5 (Engine-struct strangler step) moves this small state struct
//! into core per spike §3 row #28 + §5 R12: the heavy
//! `crates/tui/src/core/engine/scratchpad_flow.rs` (484 LOC of UI /
//! auditor / coverage helpers) **stays in tui**; only this state
//! struct migrates so future core-side turn-loop code can reset /
//! inspect it without a tui dependency.
//!
//! The tui-side `crates/tui/src/core/engine/scratchpad_flow.rs` keeps
//! a `pub use deepseek_core::engine::ScratchpadStepState;` re-export
//! shim so every existing
//! `use crate::core::engine::scratchpad_flow::ScratchpadStepState`
//! caller (engine state, `host_impl/mod.rs` turn-loop bookkeeping,
//! `message_handlers.rs` reset, tests) continues to compile
//! unchanged.

/// Per-step scratchpad nudge state.
///
/// Reset at the start of every engine step
/// (`message_handlers.rs:37`, `host_impl/mod.rs:148`); mutated by
/// `scratchpad_flow::record_tool_outcome` after each tool result; read
/// by reminder / summary-injection flow helpers that decide whether
/// to nudge the model.
#[derive(Debug, Default, Clone)]
pub struct ScratchpadStepState {
    /// Count of consecutive read-only tool calls (no writes / mutations
    /// observed) this step. Used by the reminder flow to decide when
    /// to prompt the model to record findings in the scratchpad.
    pub readonly_tool_successes: usize,

    /// Count of scratchpad-write tool calls this step
    /// (`scratchpad_append`, `scratchpad_set_area`). Used to short-
    /// circuit the read-only nudge once the model is already writing.
    pub scratchpad_writes_this_step: usize,
}

impl ScratchpadStepState {
    /// Reset to default (zeroed counters). Called at the top of every
    /// engine step before the streaming turn body runs.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_state_has_zero_counters() {
        let state = ScratchpadStepState::default();
        assert_eq!(state.readonly_tool_successes, 0);
        assert_eq!(state.scratchpad_writes_this_step, 0);
    }

    #[test]
    fn reset_zeros_all_fields() {
        let mut state = ScratchpadStepState {
            readonly_tool_successes: 7,
            scratchpad_writes_this_step: 3,
        };
        state.reset();
        assert_eq!(state.readonly_tool_successes, 0);
        assert_eq!(state.scratchpad_writes_this_step, 0);
    }
}
