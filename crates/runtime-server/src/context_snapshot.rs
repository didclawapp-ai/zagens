//! Runtime context usage snapshots (TUI-aligned estimates for Zagens / HTTP API).
//!
//! The wire-shape `ThreadContextSnapshot` lives in
//! [`deepseek_core::engine::context_snapshot`] since M1; this module keeps
//! the tui-side `build_*` helper because it depends on the tui-only
//! [`crate::compaction::should_compact`] working-set logic.

use std::path::Path;

use crate::compaction::{CompactionConfig, estimate_input_tokens_conservative, should_compact};
use crate::models::{
    LEGACY_DEEPSEEK_CONTEXT_WINDOW_TOKENS, Message, SystemPrompt, context_window_for_model,
};

pub use deepseek_core::engine::context_snapshot::ThreadContextSnapshot;

fn usage_percent_for(used: u32, window: u32) -> f64 {
    if window == 0 {
        return 0.0;
    }
    ((f64::from(used) / f64::from(window)) * 100.0).clamp(0.0, 100.0)
}

/// Build a snapshot using the same conservative estimator and compaction gate as the TUI engine.
#[must_use]
pub fn build_thread_context_snapshot(
    model: &str,
    messages: &[Message],
    system: Option<&SystemPrompt>,
    compaction: &CompactionConfig,
    workspace: Option<&Path>,
    last_api_input_tokens: Option<u32>,
    last_reported_input_tokens: Option<u32>,
    source: &str,
) -> ThreadContextSnapshot {
    let estimated = estimate_input_tokens_conservative(messages, system);
    let window = context_window_for_model(model).unwrap_or(LEGACY_DEEPSEEK_CONTEXT_WINDOW_TOKENS);
    let window_f64 = f64::from(window);
    let used_f64 = (estimated as f64).min(window_f64);
    let percent = ((used_f64 / window_f64) * 100.0).clamp(0.0, 100.0);
    let last_api_usage_percent = last_api_input_tokens.map(|t| usage_percent_for(t, window));
    ThreadContextSnapshot {
        estimated_input_tokens: estimated,
        context_window_tokens: window,
        usage_percent: percent,
        message_count: messages.len(),
        compaction_enabled: compaction.enabled,
        compaction_threshold_tokens: compaction.token_threshold,
        compaction_floor_tokens: compaction.auto_floor_tokens,
        should_compact: should_compact(messages, compaction, workspace, None, None),
        last_api_input_tokens,
        last_api_usage_percent,
        last_reported_input_tokens,
        source: source.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compaction::CompactionConfig;
    use crate::models::{ContentBlock, Message};

    #[test]
    fn snapshot_uses_conservative_estimate() {
        let messages = vec![Message {
            role: "user".to_string(),
            content: vec![ContentBlock::Text {
                text: "hello world".repeat(100),
                cache_control: None,
            }],
        }];
        let compaction = CompactionConfig {
            enabled: false,
            token_threshold: 800_000,
            ..Default::default()
        };
        let snap = build_thread_context_snapshot(
            "deepseek-v4-pro",
            &messages,
            None,
            &compaction,
            None,
            Some(12_345),
            None,
            "store",
        );
        assert!(snap.estimated_input_tokens > 0);
        assert_eq!(snap.context_window_tokens, 1_000_000);
        assert!(!snap.should_compact);
        assert_eq!(snap.last_api_input_tokens, Some(12_345));
        assert!(snap.last_api_usage_percent.is_some());
    }
}
