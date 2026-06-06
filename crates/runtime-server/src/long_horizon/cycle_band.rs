//! LHT context pressure band (~75–85%) for early cycle checkpoints (Phase 2).

use crate::models::context_window_for_model;

/// Lower bound of the LHT warning band as a fraction of the model context window.
pub const LHT_WARNING_BAND_LOW: f64 = 0.75;
/// Upper bound (exclusive) — at/above this the standard cycle threshold usually fires.
pub const LHT_WARNING_BAND_HIGH: f64 = 0.85;

/// Estimated fill ratio of the next request input against the model window (0.0–1.0).
#[must_use]
pub fn context_pressure_ratio(
    active_input_tokens: u64,
    reserved_response_headroom_tokens: u64,
    model: &str,
) -> Option<f64> {
    let window = u64::from(context_window_for_model(model)?);
    let denom = window.saturating_sub(reserved_response_headroom_tokens);
    if denom == 0 {
        return None;
    }
    Some((active_input_tokens as f64) / (denom as f64))
}

/// True when context pressure is in the LHT early-cycle band (§10.2).
#[must_use]
pub fn in_lht_warning_band(
    active_input_tokens: u64,
    reserved_response_headroom_tokens: u64,
    model: &str,
) -> bool {
    context_pressure_ratio(
        active_input_tokens,
        reserved_response_headroom_tokens,
        model,
    )
    .is_some_and(|r| (LHT_WARNING_BAND_LOW..LHT_WARNING_BAND_HIGH).contains(&r))
}

/// Whether LHT queued a checkpoint and context is in the warning band.
#[must_use]
pub fn should_lht_early_advance_cycle(
    active_input_tokens: u64,
    reserved_response_headroom_tokens: u64,
    model: &str,
    lht_enabled: bool,
    pending_checkpoint: bool,
) -> bool {
    lht_enabled
        && pending_checkpoint
        && in_lht_warning_band(
            active_input_tokens,
            reserved_response_headroom_tokens,
            model,
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warning_band_mid_window() {
        let model = "deepseek-v4-pro";
        let window = u64::from(context_window_for_model(model).unwrap());
        let headroom = 32_768u64;
        let at_80 = ((window - headroom) as f64 * 0.80) as u64;
        assert!(in_lht_warning_band(at_80, headroom, model));
        let at_50 = ((window - headroom) as f64 * 0.50) as u64;
        assert!(!in_lht_warning_band(at_50, headroom, model));
    }
}
