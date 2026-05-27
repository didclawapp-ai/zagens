//! Engine host port — orchestrator refers to the generic core handle (D16 E1-b phase 2).
//!
//! Concrete `SandboxPolicy` and `UserInputResponse` types are wired in
//! `deepseek-runtime-server` when constructing `RuntimeThreadManager<P, R>`.

pub use deepseek_core::engine::handle::EngineHandle;
pub use deepseek_core::engine::op::Op;
pub use deepseek_core::events::{Event, TurnSummary};
pub use deepseek_core::turn::TurnOutcomeStatus;
