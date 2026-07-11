//! Night queue orchestration (Phase 1a).

mod briefing;
mod gate;
mod gate_parse;
mod hooks;
mod model;
mod runner;
mod store;

pub use briefing::{render_briefing, write_briefing_to_handoff};
pub use gate_parse::{EnqueueGateInput, parse_gate_spec, resolve_gate_specs};
pub use model::{GatePredicateSpec, NightQueueDocument, QUEUE_FILE, QueueTask, QueueTaskStatus};
pub use runner::{RunOptions, RunReport, run_pending};
pub use store::{enqueue, load, preview, queue_path};

pub use hooks::dispatch_enqueue;
