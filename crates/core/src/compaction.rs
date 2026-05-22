//! Compaction configuration — shared between core and TUI.

/// Default model used when no model is specified.
pub const DEFAULT_COMPACTION_MODEL: &str = "deepseek-chat";

/// Hard floor for automatic compaction in v0.8.11+.
///
/// Below this token count, `should_compact` returns `false` regardless of
/// `enabled` or `token_threshold`. The point of the floor is V4 prefix-cache
/// economics: compaction rewrites the stable prefix, which destroys the KV
/// cache.
///
/// Manual `/compact` slash command bypasses this floor with explicit user
/// agency.
pub const MINIMUM_AUTO_COMPACTION_TOKENS: usize = 500_000;

/// Configuration for conversation compaction behavior.
#[derive(Debug, Clone, PartialEq)]
pub struct CompactionConfig {
    pub enabled: bool,
    pub token_threshold: usize,
    pub model: String,
    pub cache_summary: bool,
    /// Hard floor — `should_compact` returns `false` when total session
    /// tokens fall below this number, regardless of `enabled` or
    /// `token_threshold`.
    pub auto_floor_tokens: usize,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            token_threshold: 800_000,
            model: DEFAULT_COMPACTION_MODEL.to_string(),
            cache_summary: true,
            auto_floor_tokens: MINIMUM_AUTO_COMPACTION_TOKENS,
        }
    }
}
