//! Capacity-aware guardrail controller — **re-export shim** (M6
//! 2026-05-25).
//!
//! The full `CapacityController` body now lives in
//! [`zagens_core::capacity`] after the M6 strangler step (spike §3
//! row #20 + R10 — single atomic move, delete tui copy in same PR).
//! This file keeps only:
//!
//!   - `pub use` re-exports of every type previously defined here, so
//!     existing call sites under `crates/tui/src/core/engine/capacity_flow/*`,
//!     `crates/tui/src/runtime_threads/*`, `crates/tui/src/tui/ui*`,
//!     and the engine state (`tui::core::engine::types::EngineConfig`)
//!     keep building unchanged.
//!   - [`capacity_config_from_app`] — the tui-side adapter that
//!     projects the flat `crate::config::Config` (tui-only type) onto
//!     the core `CapacityControllerConfig`. Stays tui-side because
//!     `crate::config::Config` cannot cross the layering boundary.

pub use zagens_core::capacity::{
    CapacityController, CapacityControllerConfig, CapacityDecision, CapacityObservationInput,
    CapacitySnapshot, GuardrailAction, RiskBand,
};

/// Build effective capacity config from app config (free fn — struct lives in zagens-core).
#[must_use]
pub fn capacity_config_from_app(config: &crate::config::Config) -> CapacityControllerConfig {
    let mut out = CapacityControllerConfig::default();
    let Some(capacity) = config.capacity.as_ref() else {
        return out;
    };

    if let Some(v) = capacity.enabled {
        out.enabled = v;
    }
    if let Some(v) = capacity.low_risk_max {
        out.low_risk_max = v;
    }
    if let Some(v) = capacity.medium_risk_max {
        out.medium_risk_max = v;
    }
    if let Some(v) = capacity.severe_min_slack {
        out.severe_min_slack = v;
    }
    if let Some(v) = capacity.severe_violation_ratio {
        out.severe_violation_ratio = v;
    }
    if let Some(v) = capacity.refresh_cooldown_turns {
        out.refresh_cooldown_turns = v;
    }
    if let Some(v) = capacity.replan_cooldown_turns {
        out.replan_cooldown_turns = v;
    }
    if let Some(v) = capacity.max_replay_per_turn {
        out.max_replay_per_turn = v;
    }
    if let Some(v) = capacity.min_turns_before_guardrail {
        out.min_turns_before_guardrail = v;
    }
    if let Some(v) = capacity.profile_window {
        out.profile_window = v.max(2);
    }
    if let Some(v) = capacity.deepseek_v3_2_chat_prior {
        out.deepseek_v3_2_chat_prior = v;
    }
    if let Some(v) = capacity.deepseek_v3_2_reasoner_prior {
        out.deepseek_v3_2_reasoner_prior = v;
    }
    if let Some(v) = capacity.deepseek_v4_pro_prior {
        out.deepseek_v4_pro_prior = v;
    }
    if let Some(v) = capacity.deepseek_v4_flash_prior {
        out.deepseek_v4_flash_prior = v;
    }
    if let Some(v) = capacity.fallback_default_prior {
        out.fallback_default_prior = v;
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_config_without_capacity_uses_default_disabled() {
        let cfg = capacity_config_from_app(&crate::config::Config::default());
        // v0.8.11: default is disabled. No capacity section in config
        // means the controller stays inert; users opt in deliberately.
        assert!(!cfg.enabled);
        assert_eq!(cfg.low_risk_max, 0.50);
        assert_eq!(cfg.refresh_cooldown_turns, 6);
        assert_eq!(cfg.min_turns_before_guardrail, 4);
        assert_eq!(cfg.deepseek_v4_pro_prior, 3.5);
        assert_eq!(cfg.deepseek_v4_flash_prior, 4.2);
    }
}
