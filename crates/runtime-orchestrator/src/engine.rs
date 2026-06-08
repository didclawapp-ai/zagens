//! Engine host port — orchestrator refers to the generic core handle (D16 E1-b phase 2).
//!
//! Concrete `SandboxPolicy` and `UserInputResponse` types are wired in
//! `zagens-cli` when constructing `RuntimeThreadManager<P, R>`.

pub use zagens_core::engine::handle::EngineHandle;
pub use zagens_core::engine::op::Op;
pub use zagens_core::events::{Event, TurnSummary};
pub use zagens_core::turn::TurnOutcomeStatus;
