//! Turn loop (P2 PR4/PR6): [`handle_deepseek_turn`], streaming + tool planning/outcomes in core; tool execution L2 in tui (`tool_plans_exec`).

pub mod capacity_policy;
pub mod capacity_replay_policy;
pub mod continuation_boundary_policy;
pub mod control;
pub mod exec;
pub mod guard_projection_policy;
pub mod helpers;
pub mod host;
pub mod inner_step_host;
pub mod inner_step_replay_policy;
pub mod kernel_resume_parity_policy;
pub mod layered_context_replay_policy;
pub mod live_outer_loop_policy;
pub mod live_turn_inner_driver;
pub mod live_turn_inner_planner;
pub mod live_turn_machine;
pub mod live_turn_outer_driver;
pub mod live_turn_outer_planner;
pub mod loop_guard_replay_policy;
pub mod memory_plane_archival_policy;
pub mod memory_plane_compiler_policy;
pub mod memory_plane_episodic_policy;
pub mod memory_plane_projection_policy;
pub mod memory_plane_query_policy;
pub mod memory_plane_query_replay_policy;
pub mod memory_plane_working_policy;
pub mod memory_plane_wrapup_policy;
pub mod message_body_rebuild_policy;
pub mod outer_boundary_replay_policy;
pub mod run;
pub mod streaming_phase;
pub mod system_prompt_refresh_policy;
pub mod system_prompt_refresh_replay_policy;
pub mod tool_exec;
pub mod tool_phase;
pub mod turn_loop_outer_host;
pub mod turn_loop_session_host;
pub mod v3_driver;
pub mod v3_step;

pub use capacity_policy::should_run_capacity_error_escalation;
pub use control::{TurnLoopControl, TurnLoopStreamingPhaseOutcome, TurnLoopToolPhaseOutcome};
pub use exec::{ToolExecOutcome, ToolExecutionPlan, ToolPlanApprovalMeta};
pub use helpers::{
    build_edit_file_approval_desc, messages_with_turn_metadata, resolve_auto_effort,
};
#[allow(deprecated)]
pub use host::TurnLoopHost;
#[allow(deprecated)]
pub use host::TurnLoopMcpPool;
pub use host::TurnLoopToolRegistry;
pub use host::{CompilerRequestContext, TurnLoopConfigView, V3TurnHost};
pub use inner_step_host::InnerStepHost;
pub use live_turn_machine::{
    LiveOuterLoopState, LiveTurnMachine, OuterPostInnerStepOutcome, OuterPreInnerStepOutcome,
    OuterStepFrameOutcome,
};
pub use run::handle_deepseek_turn;
pub use tool_exec::{McpPoolPort, TurnLoopToolExec, TurnLoopToolExecutor};
pub use turn_loop_outer_host::{OuterLoopHost, TurnLoopOuterHost};
pub use turn_loop_session_host::TurnLoopSessionHost;
