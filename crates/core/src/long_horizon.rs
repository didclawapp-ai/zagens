//! Long-horizon code task (LHT) harness configuration — shared between core and runtime.

mod completion_gate;

use serde::Deserialize;

pub use completion_gate::{
    CompletionGateConfig, CompletionGateConfigToml, CompletionGateDeliverableEntry,
    CompletionGateMode, CompletionGateVerifyEntry, GenericGateMode, ManifestShell,
    MinLinesGateConfig, VerifySource,
};

/// §6.7 Adversarial read-only auditor configuration (agent-independent grounding signal).
///
/// Designed per §3.1 "gap enumerator" constraint: no release/veto power, output
/// never directly enters `graph_complete` judgment. Widens machine-grounding surface
/// by surfacing gaps that regex-based stub gate cannot catch (e.g. bodies that are
/// `return Ok(())` with no `todo!()` marker).
#[derive(Debug, Clone)]
pub struct AdversarialAuditConfig {
    /// Enable the adversarial auditor (default: false — opt-in).
    pub enabled: bool,
    /// Maximum audit rounds per LHT session (default 1).  Prevents unbounded
    /// token spend in a single session.
    pub max_audit_rounds: u32,
    /// Token budget for the auditor's single `create_message` call (default 1500).
    pub max_tokens: u32,
    /// When `Enforce`: gap candidates are added as pending checklist items and
    /// trigger a reinject nudge.  When `Observe` (default): only emit telemetry.
    pub mode: CompletionGateMode,
}

impl Default for AdversarialAuditConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_audit_rounds: 1,
            max_tokens: 1500,
            mode: CompletionGateMode::Observe,
        }
    }
}

/// Deserializable `[long_horizon.adversarial_audit]` table for TOML.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct AdversarialAuditConfigToml {
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub max_audit_rounds: Option<u32>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    /// `"observe"` (default) | `"enforce"`.
    #[serde(default)]
    pub mode: Option<String>,
}

impl AdversarialAuditConfigToml {
    #[must_use]
    pub fn into_runtime(self) -> AdversarialAuditConfig {
        let defaults = AdversarialAuditConfig::default();
        AdversarialAuditConfig {
            enabled: self.enabled.unwrap_or(defaults.enabled),
            max_audit_rounds: self.max_audit_rounds.unwrap_or(defaults.max_audit_rounds),
            max_tokens: self.max_tokens.unwrap_or(defaults.max_tokens),
            mode: self
                .mode
                .as_deref()
                .map(CompletionGateMode::from_str_lossy)
                .unwrap_or(defaults.mode),
        }
    }
}

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

/// When to enter the CRAFT review segment relative to micro completion gates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AutoEnterCraft {
    #[default]
    UserConfirm,
    /// Only after layer-2/3 micro gates are fully green.
    OnMicroPass,
    /// After checklist graph is complete but micro gates failed or exhausted
    /// (implementation segment done; acceptance may still be red).
    OnGraphComplete,
    /// Narrower: only when manifest gate rounds are honestly exhausted.
    OnManifestExhausted,
    Off,
}

impl AutoEnterCraft {
    #[must_use]
    pub fn from_optional_str(s: Option<&str>) -> Self {
        match s.map(|v| v.trim().to_ascii_lowercase()).as_deref() {
            Some("on_micro_pass" | "auto" | "immediate") => AutoEnterCraft::OnMicroPass,
            Some("on_graph_complete" | "graph_complete") => AutoEnterCraft::OnGraphComplete,
            Some("on_manifest_exhausted" | "manifest_exhausted") => {
                AutoEnterCraft::OnManifestExhausted
            }
            Some("off" | "disabled" | "false") => AutoEnterCraft::Off,
            _ => AutoEnterCraft::UserConfirm,
        }
    }
}

/// Phase 4 macro loop: LHT implement → CRAFT review → LHT remediation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MacroPhase {
    #[default]
    Implement,
    Craft,
    Remediation,
}

impl MacroPhase {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            MacroPhase::Implement => "implement",
            MacroPhase::Craft => "craft",
            MacroPhase::Remediation => "remediation",
        }
    }
}

/// `[long_horizon.macro_loop]` — opt-in LHT↔CRAFT macro cycle (Phase 4).
#[derive(Debug, Clone)]
pub struct MacroLoopConfig {
    pub enabled: bool,
    pub max_macro_cycles: u32,
    pub max_craft_rounds_per_cycle: u32,
    pub auto_enter_craft: AutoEnterCraft,
    /// Skip CRAFT when the checklist has fewer than this many items.
    pub craft_on_small_tasks: bool,
    pub min_checklist_items_for_craft: u32,
}

impl Default for MacroLoopConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_macro_cycles: 3,
            max_craft_rounds_per_cycle: 2,
            auto_enter_craft: AutoEnterCraft::UserConfirm,
            craft_on_small_tasks: false,
            min_checklist_items_for_craft: 3,
        }
    }
}

/// Deserializable `[long_horizon.macro_loop]` table.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct MacroLoopConfigToml {
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub max_macro_cycles: Option<u32>,
    #[serde(default)]
    pub max_craft_rounds_per_cycle: Option<u32>,
    #[serde(default)]
    pub auto_enter_craft: Option<String>,
    #[serde(default)]
    pub craft_on_small_tasks: Option<bool>,
    #[serde(default)]
    pub min_checklist_items_for_craft: Option<u32>,
}

impl MacroLoopConfigToml {
    #[must_use]
    pub fn into_runtime(self) -> MacroLoopConfig {
        let defaults = MacroLoopConfig::default();
        MacroLoopConfig {
            enabled: self.enabled.unwrap_or(defaults.enabled),
            max_macro_cycles: self
                .max_macro_cycles
                .unwrap_or(defaults.max_macro_cycles)
                .clamp(1, 8),
            max_craft_rounds_per_cycle: self
                .max_craft_rounds_per_cycle
                .unwrap_or(defaults.max_craft_rounds_per_cycle)
                .clamp(1, 4),
            auto_enter_craft: AutoEnterCraft::from_optional_str(self.auto_enter_craft.as_deref()),
            craft_on_small_tasks: self
                .craft_on_small_tasks
                .unwrap_or(defaults.craft_on_small_tasks),
            min_checklist_items_for_craft: self
                .min_checklist_items_for_craft
                .unwrap_or(defaults.min_checklist_items_for_craft)
                .max(1),
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
    /// Phase 4: bounded LHT implement ↔ CRAFT review ↔ remediation macro loop.
    pub macro_loop: MacroLoopConfig,
    /// §6.7: Adversarial read-only auditor — agent-independent gap enumerator.
    pub adversarial_audit: AdversarialAuditConfig,
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
            macro_loop: MacroLoopConfig::default(),
            adversarial_audit: AdversarialAuditConfig::default(),
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
    #[serde(default)]
    pub macro_loop: Option<MacroLoopConfigToml>,
    #[serde(default)]
    pub adversarial_audit: Option<AdversarialAuditConfigToml>,
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
            macro_loop: self
                .macro_loop
                .map(MacroLoopConfigToml::into_runtime)
                .unwrap_or_default(),
            adversarial_audit: self
                .adversarial_audit
                .map(AdversarialAuditConfigToml::into_runtime)
                .unwrap_or_default(),
        }
    }
}
