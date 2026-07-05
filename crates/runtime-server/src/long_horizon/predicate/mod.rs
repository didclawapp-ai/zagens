//! Harness predicate library v0 (Phase 1b.1).
//!
//! Single implementation for machine success判定 consumed by layer-2 gate, future
//! verify-loop, queue gate, and `assert_*` tools.

mod command_output_matches;
mod evaluate;
mod exit_code;
mod file_exists;
mod layer2;
mod manifest_exec;
mod shell_exec;
mod tests_pass;
mod types;
pub mod verify_result;

pub use evaluate::{evaluate, evaluate_sync};
pub use layer2::run_manifest_verify_entry;
pub use manifest_exec::CompletionGateExec;
pub use tests_pass::resolve_for_cli as resolve_tests_pass_command;
pub use types::{PredicateContext, PredicateError, PredicateResult, names};
pub use verify_result::{VerifyExitClass, VerifyRunResult};
