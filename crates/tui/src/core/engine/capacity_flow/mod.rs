//! Capacity-controller checkpoints and interventions for the engine loop.
//!
//! Extracted from `core/engine.rs` for issue #74. The main turn loop still
//! decides when checkpoints run; this module owns the guardrail policy side
//! effects, replay verification, canonical-state persistence, and event
//! emission helpers.

mod checkpoints;
mod events;
mod interventions;
mod observation;
mod persistence;
mod replay;
