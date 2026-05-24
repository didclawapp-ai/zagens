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

use deepseek_core::turn::TurnLoopMode;
use crate::tui::app::AppMode;

use super::Engine;

/// Bridge `TurnLoopMode` (core turn loop) to TUI system-prompt refresh.
pub(super) fn refresh_system_prompt_for_turn_mode(engine: &mut Engine, mode: TurnLoopMode) {
    let app_mode = match mode {
        TurnLoopMode::Agent => AppMode::Agent,
        TurnLoopMode::Yolo => AppMode::Yolo,
        TurnLoopMode::Plan => AppMode::Plan,
    };
    Engine::refresh_system_prompt(engine, app_mode);
}
