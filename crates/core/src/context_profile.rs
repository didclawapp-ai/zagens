//! Context window profile helpers — Large / Medium / Small gating and scaled thresholds.

use crate::chat::context_window_for_model;
use crate::cycle::CycleConfig;

/// Window size at or above which a model is treated as the Large profile
/// (V4-scale). Matches compaction floor and tool-result limits elsewhere.
pub const LARGE_CONTEXT_WINDOW_TOKENS: u32 = 500_000;

/// Lower bound for the Medium profile (`128K ≤ window < 500K`).
pub const MEDIUM_CONTEXT_WINDOW_MIN_TOKENS: u32 = 128_000;

/// Baseline seam/cycle level percentages for Large-profile scaling (V4 paper alignment).
pub const L1_THRESHOLD_PERCENT: u32 = 19;
pub const L2_THRESHOLD_PERCENT: u32 = 38;
pub const L3_THRESHOLD_PERCENT: u32 = 57;
pub const CYCLE_THRESHOLD_PERCENT: u32 = 75;

/// Profile bucket used for transition policy and Explorer (P2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextProfile {
    Large,
    Medium,
    Small,
    Unknown,
}

impl ContextProfile {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Large => "large",
            Self::Medium => "medium",
            Self::Small => "small",
            Self::Unknown => "unknown",
        }
    }
}

/// Explicit `[context]` / `[context.per_model]` threshold overrides only.
///
/// `None` at runtime means "use `baseline_threshold(window, percent)`", not the
/// documented 1M example values in `config.example.toml`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ContextThresholdOverrides {
    pub l1: Option<usize>,
    pub l2: Option<usize>,
    pub l3: Option<usize>,
    pub cycle: Option<usize>,
}

/// Resolved soft-seam and cycle thresholds for a model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScaledContextThresholds {
    pub profile: ContextProfile,
    pub window: Option<u32>,
    pub l1: usize,
    pub l2: usize,
    pub l3: usize,
    pub cycle: usize,
}

/// Whether the model's known context window qualifies for the Large profile.
#[must_use]
pub fn is_large_context_profile(model: &str) -> bool {
    matches!(resolve_context_profile(model), ContextProfile::Large)
}

/// Resolve the context profile for a model identifier.
#[must_use]
pub fn resolve_context_profile(model: &str) -> ContextProfile {
    match context_window_for_model(model) {
        None => ContextProfile::Unknown,
        Some(window) if window >= LARGE_CONTEXT_WINDOW_TOKENS => ContextProfile::Large,
        Some(window) if window >= MEDIUM_CONTEXT_WINDOW_MIN_TOKENS => ContextProfile::Medium,
        Some(_) => ContextProfile::Small,
    }
}

/// Pure ratio baseline — no cap against legacy 768K constants.
#[must_use]
pub fn baseline_threshold(window: u32, percent: u32) -> usize {
    ((u64::from(window) * u64::from(percent)) / 100) as usize
}

/// Resolve effective thresholds for a model.
#[must_use]
pub fn scaled_thresholds(
    model: &str,
    overrides: ContextThresholdOverrides,
) -> ScaledContextThresholds {
    let profile = resolve_context_profile(model);
    let window = context_window_for_model(model);

    if let Some(window) = window {
        ScaledContextThresholds {
            profile,
            window: Some(window),
            l1: overrides
                .l1
                .unwrap_or_else(|| baseline_threshold(window, L1_THRESHOLD_PERCENT)),
            l2: overrides
                .l2
                .unwrap_or_else(|| baseline_threshold(window, L2_THRESHOLD_PERCENT)),
            l3: overrides
                .l3
                .unwrap_or_else(|| baseline_threshold(window, L3_THRESHOLD_PERCENT)),
            cycle: overrides
                .cycle
                .unwrap_or_else(|| baseline_threshold(window, CYCLE_THRESHOLD_PERCENT)),
        }
    } else {
        ScaledContextThresholds {
            profile,
            window: None,
            l1: overrides.l1.unwrap_or(192_000),
            l2: overrides.l2.unwrap_or(384_000),
            l3: overrides.l3.unwrap_or(576_000),
            cycle: overrides.cycle.unwrap_or(768_000),
        }
    }
}

/// Cycle trigger floor: `min(effective_cycle, window − response_headroom)`.
#[must_use]
pub fn cycle_trigger_floor(
    model: &str,
    cycle_threshold: usize,
    reserved_response_headroom_tokens: u64,
) -> u64 {
    let threshold = cycle_threshold as u64;
    context_window_for_model(model)
        .map(|window| u64::from(window).saturating_sub(reserved_response_headroom_tokens))
        .map_or(threshold, |window_floor| threshold.min(window_floor))
}

/// LHT early-advance band aligned to scaled L3..cycle (P1-3).
#[must_use]
pub fn in_lht_scaled_warning_band(
    active_input_tokens: u64,
    reserved_response_headroom_tokens: u64,
    model: &str,
    thresholds: &ScaledContextThresholds,
) -> bool {
    let floor = cycle_trigger_floor(model, thresholds.cycle, reserved_response_headroom_tokens);
    let l3 = thresholds.l3 as u64;
    active_input_tokens >= l3 && active_input_tokens < floor
}

/// Default seam enablement when `[context] enabled` is unset: Large profile on.
#[must_use]
pub fn default_seam_enabled_for_model(model: &str) -> bool {
    is_large_context_profile(model)
}

/// Build a [`CycleConfig`] whose threshold matches the scaled resolver (P1-2).
#[must_use]
pub fn cycle_config_from_thresholds(
    model: &str,
    thresholds: &ScaledContextThresholds,
) -> CycleConfig {
    use crate::cycle::ModelCycleConfig;

    let mut cfg = CycleConfig::default();
    cfg.threshold_tokens = thresholds.cycle;
    for m in cfg.per_model.values_mut() {
        m.threshold_tokens = thresholds.cycle;
    }
    cfg.per_model
        .entry(model.to_string())
        .or_insert_with(ModelCycleConfig::default)
        .threshold_tokens = thresholds.cycle;
    cfg
}

#[must_use]
pub fn auto_compaction_allowed(model: &str, cycle: &CycleConfig) -> bool {
    if !cycle.enabled {
        return true;
    }
    let Some(window) = context_window_for_model(model) else {
        return true;
    };
    if window >= LARGE_CONTEXT_WINDOW_TOKENS {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cycle::CycleConfig;

    #[test]
    fn auto_compaction_blocked_for_v4_large_when_cycle_enabled() {
        let cycle = CycleConfig::default();
        assert!(!auto_compaction_allowed("deepseek-v4-pro", &cycle));
        assert!(!auto_compaction_allowed("deepseek-v4-flash", &cycle));
    }

    #[test]
    fn auto_compaction_allowed_for_medium_window_models() {
        let cycle = CycleConfig::default();
        assert!(auto_compaction_allowed("deepseek-chat", &cycle));
    }

    #[test]
    fn auto_compaction_allowed_when_cycle_disabled() {
        let mut cycle = CycleConfig::default();
        cycle.enabled = false;
        assert!(auto_compaction_allowed("deepseek-v4-pro", &cycle));
    }

    #[test]
    fn auto_compaction_allowed_for_unknown_window_models() {
        let cycle = CycleConfig::default();
        assert!(auto_compaction_allowed(
            "some-unknown-openrouter-model",
            &cycle
        ));
    }

    #[test]
    fn is_large_context_profile_matches_window_table() {
        assert!(is_large_context_profile("deepseek-v4-pro"));
        assert!(!is_large_context_profile("deepseek-chat"));
    }

    #[test]
    fn scaled_thresholds_use_baseline_for_1m_v4() {
        let t = scaled_thresholds("deepseek-v4-pro", ContextThresholdOverrides::default());
        assert_eq!(t.profile, ContextProfile::Large);
        assert_eq!(t.l1, 190_000);
        assert_eq!(t.l2, 380_000);
        assert_eq!(t.l3, 570_000);
        assert_eq!(t.cycle, 750_000);
    }

    #[test]
    fn baseline_threshold_scales_to_2m_without_legacy_cap() {
        assert_eq!(
            baseline_threshold(2_000_000, CYCLE_THRESHOLD_PERCENT),
            1_500_000
        );
        assert_eq!(baseline_threshold(2_000_000, L1_THRESHOLD_PERCENT), 380_000);
    }

    #[test]
    fn scaled_thresholds_honors_explicit_cycle_without_768k_cap() {
        let t = scaled_thresholds(
            "deepseek-v4-pro",
            ContextThresholdOverrides {
                cycle: Some(1_500_000),
                ..Default::default()
            },
        );
        assert_eq!(t.cycle, 1_500_000);
    }

    #[test]
    fn explicit_override_wins_over_baseline() {
        let t = scaled_thresholds(
            "deepseek-v4-pro",
            ContextThresholdOverrides {
                cycle: Some(120_000),
                ..Default::default()
            },
        );
        assert_eq!(t.cycle, 120_000);
        assert_eq!(t.l1, 190_000);
    }

    #[test]
    fn medium_profile_cycle_baseline_is_75_percent() {
        let t = scaled_thresholds("deepseek-chat", ContextThresholdOverrides::default());
        assert_eq!(t.profile, ContextProfile::Medium);
        assert_eq!(t.cycle, 96_000);
    }

    #[test]
    fn cycle_trigger_floor_caps_at_window_minus_headroom() {
        let headroom = 263_168u64;
        let floor = cycle_trigger_floor("deepseek-v4-pro", 750_000, headroom);
        assert_eq!(floor, 736_832);
    }

    #[test]
    fn lht_warning_band_uses_l3_to_cycle_gap() {
        let t = scaled_thresholds("deepseek-v4-pro", ContextThresholdOverrides::default());
        let headroom = 263_168u64;
        assert!(!in_lht_scaled_warning_band(
            569_000,
            headroom,
            "deepseek-v4-pro",
            &t
        ));
        assert!(in_lht_scaled_warning_band(
            600_000,
            headroom,
            "deepseek-v4-pro",
            &t
        ));
        assert!(!in_lht_scaled_warning_band(
            750_000,
            headroom,
            "deepseek-v4-pro",
            &t
        ));
    }

    #[test]
    fn default_seam_enabled_only_for_large_profile() {
        assert!(default_seam_enabled_for_model("deepseek-v4-pro"));
        assert!(!default_seam_enabled_for_model("deepseek-chat"));
    }
}
