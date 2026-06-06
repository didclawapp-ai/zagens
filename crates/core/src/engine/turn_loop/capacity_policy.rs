//! Pure capacity guardrail policy for the turn loop (P2 PR6d).

use crate::error_taxonomy::ErrorCategory;

/// Whether the post-tool error-escalation checkpoint should run (mirrors tui `capacity_flow`).
#[must_use]
pub fn should_run_capacity_error_escalation(
    step_error_count: usize,
    consecutive_tool_error_steps: u32,
    error_categories: &[ErrorCategory],
) -> bool {
    if step_error_count == 0 && consecutive_tool_error_steps < 2 {
        return false;
    }

    let has_context_overflow = error_categories.contains(&ErrorCategory::InvalidInput);
    let only_transient = !error_categories.is_empty()
        && error_categories.iter().all(|c| {
            matches!(
                c,
                ErrorCategory::Network | ErrorCategory::RateLimit | ErrorCategory::Timeout
            )
        });
    if only_transient && !has_context_overflow {
        return false;
    }
    if !has_context_overflow && consecutive_tool_error_steps < 2 {
        return false;
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_when_no_errors() {
        assert!(!should_run_capacity_error_escalation(0, 0, &[]));
    }

    #[test]
    fn skips_transient_only_without_overflow() {
        assert!(!should_run_capacity_error_escalation(
            1,
            0,
            &[ErrorCategory::Network],
        ));
    }

    #[test]
    fn runs_on_context_overflow() {
        assert!(should_run_capacity_error_escalation(
            1,
            0,
            &[ErrorCategory::InvalidInput],
        ));
    }

    #[test]
    fn runs_on_consecutive_tool_failures() {
        assert!(should_run_capacity_error_escalation(
            0,
            2,
            &[ErrorCategory::Tool]
        ));
    }
}
