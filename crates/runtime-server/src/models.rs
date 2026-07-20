//! API request/response models for `DeepSeek` and OpenAI-compatible endpoints.
//!
//! Most types are now re-exported from `zagens-core` (P2 PR3b).
//! Only tui-specific helpers and tests remain in this file.

// ── Re-exports from zagens-core ─────────────────────────────────────

#[allow(unused_imports)] // public re-exports for downstream crates / tests
pub use zagens_core::chat::{
    CacheControl, ContentBlock, ContentBlockStart, DEEPSEEK_V4_CONTEXT_WINDOW_TOKENS,
    DEFAULT_MAX_OUTPUT_TOKENS, Delta, KIMI_K3_CONTEXT_WINDOW_TOKENS, KIMI_K3_MAX_OUTPUT_TOKENS,
    LEGACY_DEEPSEEK_CONTEXT_WINDOW_TOKENS, Message, MessageDelta, MessageRequest, MessageResponse,
    StreamEvent, SystemBlock, SystemPrompt, Tool, ToolCaller, compaction_threshold_for_model,
    context_window_for_model,
};
pub use zagens_core::models::{ServerToolUsage, Usage};

// ── TUI-specific helpers ──────────────────────────────────────────────

/// Compaction threshold keyed by model and caller-supplied effort tier.
#[must_use]
pub fn compaction_threshold_for_model_and_effort(
    model: &str,
    _reasoning_effort: Option<&str>,
) -> usize {
    compaction_threshold_for_model(model)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v4_snapshots_preserve_context_window() {
        // v-series snapshots get 1M context since they contain "v4"
        assert_eq!(
            context_window_for_model("deepseek-v4-flash-20260423"),
            Some(DEEPSEEK_V4_CONTEXT_WINDOW_TOKENS)
        );
        assert_eq!(
            context_window_for_model("deepseek-v4-pro-20260423"),
            Some(DEEPSEEK_V4_CONTEXT_WINDOW_TOKENS)
        );
    }

    #[test]
    fn unknown_legacy_deepseek_models_map_to_128k_context_window() {
        assert_eq!(
            context_window_for_model("deepseek-coder"),
            Some(LEGACY_DEEPSEEK_CONTEXT_WINDOW_TOKENS)
        );
        assert_eq!(
            context_window_for_model("deepseek-v3.2-0324"),
            Some(LEGACY_DEEPSEEK_CONTEXT_WINDOW_TOKENS)
        );
    }

    #[test]
    fn deepseek_v4_models_map_to_1m_context_window() {
        assert_eq!(
            context_window_for_model("deepseek-v4-pro"),
            Some(DEEPSEEK_V4_CONTEXT_WINDOW_TOKENS)
        );
        assert_eq!(
            context_window_for_model("deepseek-v4-flash"),
            Some(DEEPSEEK_V4_CONTEXT_WINDOW_TOKENS)
        );
        assert_eq!(
            context_window_for_model("deepseek-ai/deepseek-v4-pro"),
            Some(DEEPSEEK_V4_CONTEXT_WINDOW_TOKENS)
        );
    }

    #[test]
    fn deepseek_models_with_k_suffix_use_hint() {
        assert_eq!(context_window_for_model("deepseek-v3.2-32k"), Some(32_000));
        assert_eq!(
            context_window_for_model("deepseek-v3.2-256k-preview"),
            Some(256_000)
        );
        assert_eq!(
            context_window_for_model("deepseek-v3.2-2k-preview"),
            Some(LEGACY_DEEPSEEK_CONTEXT_WINDOW_TOKENS)
        );
    }

    #[test]
    fn compaction_threshold_scales_with_context_window() {
        assert_eq!(
            compaction_threshold_for_model("deepseek-v3.2-128k"),
            102_400
        );
        // v0.8.11 (#664): unknown-model fallback also resolves to 80% of
        // `LEGACY_DEEPSEEK_CONTEXT_WINDOW_TOKENS` (128K legacy DeepSeek
        // fallback) — same late-trigger discipline as the V4 path. Was
        // `50_000` pre-v0.8.11; that hardcoded value compacted at ~5% of a
        // 1M window when model detection silently fell through, which is
        // exactly the prefix-cache-burning behaviour we're getting away from.
        assert_eq!(compaction_threshold_for_model("unknown-model"), 102_400);
    }

    #[test]
    fn compaction_scales_for_deepseek_v4_1m_context() {
        assert_eq!(compaction_threshold_for_model("deepseek-v4-pro"), 800_000);
    }

    #[test]
    fn v4_replacement_compaction_ignores_reasoning_effort() {
        assert_eq!(
            compaction_threshold_for_model_and_effort("deepseek-v4-pro", Some("off")),
            800_000
        );
        assert_eq!(
            compaction_threshold_for_model_and_effort("deepseek-v4-pro", Some("high")),
            800_000
        );
        assert_eq!(
            compaction_threshold_for_model_and_effort("deepseek-v4-pro", Some("max")),
            800_000
        );
    }

    #[test]
    fn v4_soft_caps_only_apply_to_v4_models() {
        assert_eq!(
            compaction_threshold_for_model_and_effort("deepseek-v3.2-128k", Some("max")),
            102_400
        );
        // v0.8.11 (#664): unknown-model fallback also lands on the
        // 80%-of-128K legacy DeepSeek fallback instead of the legacy
        // hardcoded 50K, so model-detection-fall-through doesn't quietly
        // burn V4 prefix cache at 5%-of-window.
        assert_eq!(
            compaction_threshold_for_model_and_effort("unknown-model", Some("max")),
            102_400
        );
    }

    #[test]
    fn v4_replacement_compaction_defaults_to_late_guard_when_effort_unknown() {
        assert_eq!(
            compaction_threshold_for_model_and_effort("deepseek-v4-pro", None),
            800_000
        );
        assert_eq!(
            compaction_threshold_for_model_and_effort("deepseek-v4-pro", Some("unknown")),
            800_000
        );
    }
}
