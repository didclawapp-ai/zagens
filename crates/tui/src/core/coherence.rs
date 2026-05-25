//! Plain-language session coherence state — **re-export shim** (M6
//! 2026-05-25).
//!
//! The full `CoherenceState` enum + `CoherenceSignal` + reducer body
//! now live in [`deepseek_core::coherence`] after the M6 strangler
//! step (spike §3 row #22 + the M1-deferred reducer work — the
//! reducer depends on `GuardrailAction` / `RiskBand` which only
//! landed in core under M6, so it could not ride M1). This file
//! keeps only `pub use` re-exports so existing call sites under
//! `crates/tui/src/core/engine/capacity_flow/*`,
//! `crates/tui/src/runtime_threads/*`, and engine state
//! (`tui::core::engine::types::EngineConfig`) keep building unchanged.

pub use deepseek_core::coherence::{CoherenceSignal, CoherenceState, next_coherence_state};
