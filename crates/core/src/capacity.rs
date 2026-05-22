//! Capacity controller configuration — shared between core and TUI.

/// Controller settings.
#[derive(Debug, Clone, PartialEq)]
pub struct CapacityControllerConfig {
    pub enabled: bool,
    pub low_risk_max: f64,
    pub medium_risk_max: f64,
    pub severe_min_slack: f64,
    pub severe_violation_ratio: f64,
    pub refresh_cooldown_turns: u64,
    pub replan_cooldown_turns: u64,
    pub max_replay_per_turn: usize,
    pub min_turns_before_guardrail: u64,
    pub profile_window: usize,
    pub deepseek_v3_2_chat_prior: f64,
    pub deepseek_v3_2_reasoner_prior: f64,
    pub deepseek_v4_pro_prior: f64,
    pub deepseek_v4_flash_prior: f64,
    pub fallback_default_prior: f64,
}

impl Default for CapacityControllerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            low_risk_max: 0.50,
            medium_risk_max: 0.62,
            severe_min_slack: -0.25,
            severe_violation_ratio: 0.40,
            refresh_cooldown_turns: 6,
            replan_cooldown_turns: 5,
            max_replay_per_turn: 1,
            min_turns_before_guardrail: 4,
            profile_window: 8,
            deepseek_v3_2_chat_prior: 3.9,
            deepseek_v3_2_reasoner_prior: 4.1,
            deepseek_v4_pro_prior: 3.5,
            deepseek_v4_flash_prior: 4.2,
            fallback_default_prior: 3.8,
        }
    }
}
