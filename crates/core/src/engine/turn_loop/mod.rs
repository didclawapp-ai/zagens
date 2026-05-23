//! Turn loop (P2 PR4): [`handle_deepseek_turn`] in core; L2 phases in `deepseek-tui::turn_loop`.

pub mod control;
pub mod exec;
pub mod helpers;
pub mod host;
pub mod run;
pub mod tool_exec;

pub use control::{TurnLoopControl, TurnLoopStreamingPhaseOutcome, TurnLoopToolPhaseOutcome};
pub use host::{TurnLoopMcpPool, TurnLoopToolRegistry};
pub use exec::{ToolExecOutcome, ToolExecutionPlan};
pub use helpers::{
    build_edit_file_approval_desc, messages_with_turn_metadata, resolve_auto_effort,
};
pub use host::{TurnLoopConfigView, TurnLoopHost};
pub use run::handle_deepseek_turn;
pub use tool_exec::TurnLoopToolExec;
