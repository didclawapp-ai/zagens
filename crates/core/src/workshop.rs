//! Workshop / large-tool-output routing configuration — shared between core and TUI.

use std::collections::HashMap;

use serde::Deserialize;

/// Default token threshold above which a tool result is routed through the
/// workshop. Matches the issue spec of 4 096 tokens.
pub const DEFAULT_LARGE_OUTPUT_THRESHOLD_TOKENS: usize = 4_096;

/// `[workshop]` section in `config.toml`.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct WorkshopConfig {
    /// Token threshold above which tool results are routed through the workshop
    /// synthesis sub-agent. Default: [`DEFAULT_LARGE_OUTPUT_THRESHOLD_TOKENS`].
    #[serde(default)]
    pub large_output_threshold_tokens: Option<usize>,

    /// Per-tool threshold overrides (tool name → token limit). A tool whose
    /// name appears here uses this limit instead of
    /// `large_output_threshold_tokens`.
    #[serde(default)]
    pub per_tool_thresholds: Option<HashMap<String, usize>>,
}

impl WorkshopConfig {
    /// Resolve the effective threshold for the given tool name.
    #[must_use]
    pub fn threshold_for(&self, tool_name: &str) -> usize {
        if let Some(per_tool) = self.per_tool_thresholds.as_ref()
            && let Some(&limit) = per_tool.get(tool_name)
        {
            return limit;
        }
        self.large_output_threshold_tokens
            .unwrap_or(DEFAULT_LARGE_OUTPUT_THRESHOLD_TOKENS)
    }
}
