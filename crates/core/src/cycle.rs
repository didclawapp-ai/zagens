//! Checkpoint-restart cycle configuration — shared between core and TUI.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Default token threshold at which a cycle boundary fires (~75% of 1M window).
pub const DEFAULT_CYCLE_THRESHOLD_TOKENS: usize = 768_000;

/// Default cap on the model-curated briefing block.
pub const DEFAULT_BRIEFING_MAX_TOKENS: usize = 3_000;

/// Per-model cycle tuning. Loaded from `[cycle.per_model.<model>]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelCycleConfig {
    /// Token threshold above which a cycle boundary fires.
    pub threshold_tokens: usize,
    /// Cap on the model-curated `<carry_forward>` briefing.
    pub briefing_max_tokens: usize,
}

impl Default for ModelCycleConfig {
    fn default() -> Self {
        Self {
            threshold_tokens: DEFAULT_CYCLE_THRESHOLD_TOKENS,
            briefing_max_tokens: DEFAULT_BRIEFING_MAX_TOKENS,
        }
    }
}

/// Top-level cycle configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CycleConfig {
    /// Whether checkpoint-restart cycles are enabled. Defaults to true.
    pub enabled: bool,
    /// Default token threshold; per-model overrides take precedence when present.
    pub threshold_tokens: usize,
    /// Default briefing cap; per-model overrides take precedence when present.
    pub briefing_max_tokens: usize,
    /// Per-model overrides keyed by model identifier (e.g. `deepseek-v4-pro`).
    pub per_model: HashMap<String, ModelCycleConfig>,
}

impl Default for CycleConfig {
    fn default() -> Self {
        let mut per_model: HashMap<String, ModelCycleConfig> = HashMap::new();
        per_model.insert("deepseek-v4-pro".to_string(), ModelCycleConfig::default());
        per_model.insert("deepseek-v4-flash".to_string(), ModelCycleConfig::default());
        Self {
            enabled: true,
            threshold_tokens: DEFAULT_CYCLE_THRESHOLD_TOKENS,
            briefing_max_tokens: DEFAULT_BRIEFING_MAX_TOKENS,
            per_model,
        }
    }
}

impl CycleConfig {
    /// Resolve the threshold for a given model (per-model override > default).
    #[must_use]
    pub fn threshold_for(&self, model: &str) -> usize {
        self.per_model
            .get(model)
            .map(|m| m.threshold_tokens)
            .unwrap_or(self.threshold_tokens)
    }

    /// Resolve the briefing-token cap for a given model.
    #[must_use]
    pub fn briefing_max_for(&self, model: &str) -> usize {
        self.per_model
            .get(model)
            .map(|m| m.briefing_max_tokens)
            .unwrap_or(self.briefing_max_tokens)
    }
}

/// Snapshot of a model-curated briefing produced at cycle handoff.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CycleBriefing {
    /// 1-based cycle number this briefing closes (i.e. the cycle being archived).
    pub cycle: u32,
    /// UTC timestamp when the briefing turn completed.
    pub timestamp: DateTime<Utc>,
    /// Extracted contents of the `<carry_forward>` block.
    pub briefing_text: String,
    /// Approximate token count of `briefing_text`.
    pub token_estimate: usize,
}
