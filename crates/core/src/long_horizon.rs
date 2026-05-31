//! Long-horizon code task (LHT) harness configuration — shared between core and runtime.

mod completion_gate;

use serde::Deserialize;

pub use completion_gate::{
    CompletionGateConfig, CompletionGateConfigToml, CompletionGateDeliverableEntry,
    CompletionGateMode, CompletionGateVerifyEntry, GenericGateMode, ManifestShell, VerifySource,
};

/// Resolved LHT settings for the engine turn loop.
#[derive(Debug, Clone)]
pub struct LongHorizonConfig {
    pub enabled: bool,
    pub max_nudges_per_item: u32,
    pub blocked_nudges_without_progress: u32,
    /// Re-inject plan/checklist objective summary every N assistant steps (0 = off).
    pub reinject_every_steps: u32,
    /// Phase 2.x (§4.8): treat a changed git working tree (since the last nudge)
    /// as objective, language-agnostic qualified progress. Auto-degrades to the
    /// Phase 1 tool signals when the workspace is not a git repo.
    pub progress_via_git: bool,
    /// Composable harness completion gate (§6 — manifest oracle + deliverable audit).
    pub completion_gate: CompletionGateConfig,
}

impl Default for LongHorizonConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_nudges_per_item: 5,
            blocked_nudges_without_progress: 3,
            reinject_every_steps: 0,
            progress_via_git: true,
            completion_gate: CompletionGateConfig::default(),
        }
    }
}

/// Deserializable `[long_horizon]` table for TOML.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct LongHorizonConfigToml {
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub max_nudges_per_item: Option<u32>,
    #[serde(default)]
    pub blocked_nudges_without_progress: Option<u32>,
    #[serde(default)]
    pub reinject_every_steps: Option<u32>,
    #[serde(default)]
    pub progress_via_git: Option<bool>,
    #[serde(default)]
    pub completion_gate: Option<CompletionGateConfigToml>,
}

impl LongHorizonConfigToml {
    #[must_use]
    pub fn into_runtime(self) -> LongHorizonConfig {
        let defaults = LongHorizonConfig::default();
        LongHorizonConfig {
            enabled: self.enabled.unwrap_or(defaults.enabled),
            max_nudges_per_item: self
                .max_nudges_per_item
                .unwrap_or(defaults.max_nudges_per_item),
            blocked_nudges_without_progress: self
                .blocked_nudges_without_progress
                .unwrap_or(defaults.blocked_nudges_without_progress),
            reinject_every_steps: self
                .reinject_every_steps
                .unwrap_or(defaults.reinject_every_steps),
            progress_via_git: self.progress_via_git.unwrap_or(defaults.progress_via_git),
            completion_gate: self
                .completion_gate
                .map(CompletionGateConfigToml::into_runtime)
                .unwrap_or_default(),
        }
    }
}
