//! Turn loop (P2 PR4/PR6): [`handle_deepseek_turn`], streaming + tool planning/outcomes in core; tool execution L2 in tui (`tool_plans_exec`).

pub mod capacity_policy;
pub mod capacity_replay_policy;
pub mod control;
pub mod exec;
pub mod guard_projection_policy;
pub mod helpers;
pub mod host;
pub mod loop_guard_replay_policy;
pub mod memory_plane_archival_policy;
pub mod memory_plane_compiler_policy;
pub mod memory_plane_episodic_policy;
pub mod memory_plane_projection_policy;
pub mod memory_plane_query_policy;
pub mod memory_plane_query_replay_policy;
pub mod memory_plane_working_policy;
pub mod memory_plane_wrapup_policy;
pub mod run;
pub mod streaming_phase;
pub mod tool_exec;
pub mod tool_phase;
pub mod v3_driver;
pub mod v3_step;

pub use capacity_policy::should_run_capacity_error_escalation;
pub use control::{TurnLoopControl, TurnLoopStreamingPhaseOutcome, TurnLoopToolPhaseOutcome};
pub use exec::{ToolExecOutcome, ToolExecutionPlan, ToolPlanApprovalMeta};
pub use helpers::{
    build_edit_file_approval_desc, messages_with_turn_metadata, resolve_auto_effort,
};
#[allow(deprecated)]
pub use host::TurnLoopMcpPool;
pub use host::TurnLoopToolRegistry;
pub use host::{CompilerRequestContext, TurnLoopConfigView, TurnLoopHost};
pub use run::handle_deepseek_turn;
pub use tool_exec::{McpPoolPort, TurnLoopToolExec, TurnLoopToolExecutor};
