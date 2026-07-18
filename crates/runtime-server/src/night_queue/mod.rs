//! Night queue orchestration (Phase 1a).

mod briefing;
mod gate;
pub(crate) mod gate_parse;
mod hooks;
mod model;
mod run_control;
mod runner;
mod store;

pub use briefing::{render_briefing, write_briefing_to_handoff};
pub use gate_parse::{EnqueueGateInput, resolve_gate_specs};
pub use model::{GatePredicateSpec, NightQueueDocument, QUEUE_FILE, QueueTask, QueueTaskStatus};
pub use run_control::request_stop;
pub use runner::{RunOptions, RunReport, run_pending};
pub use store::{
    cancel_task, clear_finished, enqueue, load, preview, queue_path, reclaim_stale_running,
    remove_task, retry_task,
};

pub use hooks::dispatch_enqueue;
