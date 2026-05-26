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
use crate::agent_surface::AppMode;

use super::Engine;

use crate::topic_memory::PromptInjectionArbitration;

fn turn_loop_mode_to_app_mode(mode: TurnLoopMode) -> AppMode {
    match mode {
        TurnLoopMode::Agent => AppMode::Agent,
        TurnLoopMode::Yolo => AppMode::Yolo,
        TurnLoopMode::Plan => AppMode::Plan,
    }
}

/// Bridge `TurnLoopMode` (core turn loop) to TUI system-prompt refresh.
pub(super) fn refresh_system_prompt_for_turn_mode(engine: &mut Engine, mode: TurnLoopMode) {
    engine.refresh_system_prompt_with_arbitration(
        turn_loop_mode_to_app_mode(mode),
        PromptInjectionArbitration::none(),
    );
}

/// After capacity trim/refresh: drop topic memory before auxiliary blocks (B2.1).
pub(super) fn refresh_system_prompt_for_turn_mode_under_capacity(
    engine: &mut Engine,
    mode: TurnLoopMode,
) {
    engine.refresh_system_prompt_with_arbitration(
        turn_loop_mode_to_app_mode(mode),
        PromptInjectionArbitration::capacity_pressure(),
    );
}
