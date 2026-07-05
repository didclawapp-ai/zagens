//! Night queue orchestration (Phase 1a).

mod briefing;
mod gate;
mod model;
mod runner;
mod store;

pub use briefing::{render_briefing, write_briefing_to_handoff};
pub use model::{GatePredicateSpec, NightQueueDocument, QueueTaskStatus};
pub use runner::{RunOptions, run_pending};
pub use store::{enqueue, load, preview, queue_path};
