//! Turn-loop control-flow results returned by host hooks (P2 PR4).

use crate::engine::streaming::ToolUseState;
use crate::error_taxonomy::ErrorCategory;
use crate::turn::TurnOutcomeStatus;

/// Control return from `handle_no_tool_uses` / subagent wait paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurnLoopControl {
    /// Keep iterating the outer turn loop (`continue`).
    Continue,
    /// Exit the outer loop (`break`).
    Break,
    /// End the turn immediately.
    Return(TurnOutcomeStatus, Option<String>),
}

/// Result of [`super::tool_phase::run_tool_execution_phase`].
#[derive(Debug, Default)]
pub struct TurnLoopToolPhaseOutcome {
    pub step_error_count: usize,
    pub step_error_categories: Vec<ErrorCategory>,
    /// `stop_after_plan_tool` or loop-guard halt — exit the outer turn loop.
    pub break_outer_loop: bool,
    /// Capacity post-tool checkpoint requested another model step.
    pub continue_outer_loop: bool,
}

/// Result of [`super::streaming_phase::run_streaming_phase`].
#[derive(Debug, Default)]
pub struct TurnLoopStreamingPhaseOutcome {
    /// Parsed tool calls from the model stream (empty → no tool phase this step).
    pub tool_uses: Vec<ToolUseState>,
    /// Steer messages queued during streaming (drained after tool phase).
    pub pending_steers: Vec<String>,
    /// Retry the outer turn loop (stream died with nothing / `handle_no_tool_uses` continue).
    pub continue_outer_loop: bool,
    /// Exit the outer turn loop (`handle_no_tool_uses` break).
    pub break_outer_loop: bool,
    /// End the turn immediately (`handle_no_tool_uses` return or fatal stream error).
    pub return_early: Option<(TurnOutcomeStatus, Option<String>)>,
}
