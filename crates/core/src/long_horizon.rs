//! Long-horizon code task (LHT) harness configuration — shared between core and runtime.

mod completion_gate;

use serde::Deserialize;

pub use completion_gate::{
    CompletionGateConfig, CompletionGateConfigToml, CompletionGateDeliverableEntry,
    CompletionGateMode, CompletionGateVerifyEntry, GenericGateMode, ManifestShell, VerifySource,
};

/// LHT enforcement mode (user-facing toggle).
///
/// - `Auto` (default): the harness only engages once the model authors a
///   plan/checklist — an **empty** task graph is skipped, so the model is free
///   to free-style trivial / conversational work without being forced to plan.
/// - `Strict`: a code-surface task may **not** proceed with an empty task graph.
///   The runtime injects a bounded "establish a plan first" nudge, and the
///   completion / stub gates are treated as `enforce`, so the full LHT net
///   cannot be silently bypassed by simply never planning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LhtMode {
    #[default]
    Auto,
    Strict,
}

impl LhtMode {
    #[must_use]
    pub fn is_strict(self) -> bool {
        matches!(self, LhtMode::Strict)
    }

    /// Parse from an optional config/UI string; unknown / absent ⇒ `Auto`.
    #[must_use]
    pub fn from_optional_str(s: Option<&str>) -> Self {
        match s.map(|v| v.trim().to_ascii_lowercase()).as_deref() {
            Some("strict") => LhtMode::Strict,
            _ => LhtMode::Auto,
        }
    }
}

/// Resolved LHT settings for the engine turn loop.
#[derive(Debug, Clone)]
pub struct LongHorizonConfig {
    pub enabled: bool,
    /// Enforcement mode. `Auto` (default) engages only once the model plans;
    /// `Strict` forces a plan to exist and treats completion/stub gates as
    /// enforce. Per-turn UI toggle may override this default (see engine).
    pub mode: LhtMode,
    pub max_nudges_per_item: u32,
    pub blocked_nudges_without_progress: u32,
    /// Re-inject plan/checklist objective summary every N assistant steps (0 = off).
    pub reinject_every_steps: u32,
    /// Phase 2.x (§4.8): treat a changed git working tree (since the last nudge)
    /// as objective, language-agnostic qualified progress. Auto-degrades to the
    /// Phase 1 tool signals when the workspace is not a git repo.
    pub progress_via_git: bool,
    /// "一推到底" (C2): when the in-turn nudge gate has given up (blocked /
    /// max-nudges) but the task graph is still genuinely incomplete, keep the
    /// turn alive by resetting the nudge tracker and re-injecting a forceful
    /// continue message — bounded per turn by [`Self::max_auto_continue_rounds`].
    /// Off by default; opt-in for hands-off multi-phase runs.
    pub auto_continue: bool,
    /// Hard per-turn ceiling on auto-continue rounds (only consulted when
    /// [`Self::auto_continue`] is true). Bounds the give-up override so a model
    /// that truly cannot progress still terminates the turn.
    pub max_auto_continue_rounds: u32,
    /// Composable harness completion gate (§6 — manifest oracle + deliverable audit).
    pub completion_gate: CompletionGateConfig,
}

impl Default for LongHorizonConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: LhtMode::Auto,
            max_nudges_per_item: 5,
            blocked_nudges_without_progress: 3,
            reinject_every_steps: 0,
            progress_via_git: true,
            auto_continue: false,
            max_auto_continue_rounds: 16,
            completion_gate: CompletionGateConfig::default(),
        }
    }
}

/// Deserializable `[long_horizon]` table for TOML.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct LongHorizonConfigToml {
    #[serde(default)]
    pub enabled: Option<bool>,
    /// `"auto"` (default) | `"strict"`.
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub max_nudges_per_item: Option<u32>,
    #[serde(default)]
    pub blocked_nudges_without_progress: Option<u32>,
    #[serde(default)]
    pub reinject_every_steps: Option<u32>,
    #[serde(default)]
    pub progress_via_git: Option<bool>,
    #[serde(default)]
    pub auto_continue: Option<bool>,
    #[serde(default)]
    pub max_auto_continue_rounds: Option<u32>,
    #[serde(default)]
    pub completion_gate: Option<CompletionGateConfigToml>,
}

impl LongHorizonConfigToml {
    #[must_use]
    pub fn into_runtime(self) -> LongHorizonConfig {
        let defaults = LongHorizonConfig::default();
        LongHorizonConfig {
            enabled: self.enabled.unwrap_or(defaults.enabled),
            mode: LhtMode::from_optional_str(self.mode.as_deref()),
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
            auto_continue: self.auto_continue.unwrap_or(defaults.auto_continue),
            max_auto_continue_rounds: self
                .max_auto_continue_rounds
                .unwrap_or(defaults.max_auto_continue_rounds),
            completion_gate: self
                .completion_gate
                .map(CompletionGateConfigToml::into_runtime)
                .unwrap_or_default(),
        }
    }
}
