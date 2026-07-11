//! LHT harness presets (§5.2 LONG_HORIZON_CODE_TASKS) — config.toml overlays.

use crate::lht_config::{CompletionGateConfigToml, LongHorizonConfigToml, MacroLoopConfigToml};

/// Known harness preset ids (Desktop LHT settings panel).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LhtPresetId {
    CodeDefault,
    LongRefactor,
    LongFix,
    CraftAudit,
}

impl LhtPresetId {
    #[must_use]
    pub fn from_str_id(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "code-default" | "code_default" => Some(Self::CodeDefault),
            "long-refactor" | "long_refactor" => Some(Self::LongRefactor),
            "long-fix" | "long_fix" => Some(Self::LongFix),
            "craft-audit" | "craft_audit" => Some(Self::CraftAudit),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CodeDefault => "code-default",
            Self::LongRefactor => "long-refactor",
            Self::LongFix => "long-fix",
            Self::CraftAudit => "craft-audit",
        }
    }
}

/// Overlay the three product gates while preserving operator `verify` /
/// `deliverable` / `min_lines` rows (users should not hand-edit gate modes).
fn with_product_gate_modes(
    base: Option<CompletionGateConfigToml>,
    auto_verify_replay: &str,
    toolchain_gate: &str,
    stub_gate: &str,
) -> Option<CompletionGateConfigToml> {
    let mut gate = base.unwrap_or_default();
    gate.auto_verify_replay = Some(auto_verify_replay.into());
    gate.toolchain_gate = Some(toolchain_gate.into());
    gate.stub_gate = Some(stub_gate.into());
    Some(gate)
}

/// Apply a harness preset onto `[long_horizon]`, preserving operator completion_gate rows
/// while setting product gate modes for the preset (observe vs hard enforce).
pub fn apply_lht_preset(base: &mut LongHorizonConfigToml, preset: LhtPresetId) {
    let gate = base.completion_gate.clone();
    match preset {
        LhtPresetId::CodeDefault => {
            *base = LongHorizonConfigToml {
                enabled: Some(true),
                mode: Some("auto".into()),
                max_nudges_per_item: Some(5),
                blocked_nudges_without_progress: Some(3),
                reinject_every_steps: Some(0),
                progress_via_git: Some(true),
                auto_continue: Some(false),
                max_auto_continue_rounds: Some(16),
                // Soft: observe toolchain / stub / verify-replay (no hard stop).
                completion_gate: with_product_gate_modes(gate, "observe", "observe", "observe"),
                macro_loop: Some(MacroLoopConfigToml {
                    enabled: Some(false),
                    ..MacroLoopConfigToml::default()
                }),
            };
        }
        LhtPresetId::LongRefactor => {
            *base = LongHorizonConfigToml {
                enabled: Some(true),
                mode: Some("strict".into()),
                max_nudges_per_item: Some(5),
                blocked_nudges_without_progress: Some(3),
                reinject_every_steps: Some(5),
                progress_via_git: Some(true),
                // auto_continue must be true for macro loop: the remediation
                // segment re-injects when the model stalls on open checklist
                // items. Without this, LHT→CRAFT→LHT macro cycles can stall
                // silently on the remediation leg.
                auto_continue: Some(true),
                max_auto_continue_rounds: Some(16),
                // Hard acceptance without per-task verify TOML: toolchain +
                // stub + model `[verify:]` replay all enforce.
                completion_gate: with_product_gate_modes(gate, "enforce", "enforce", "enforce"),
                macro_loop: Some(MacroLoopConfigToml {
                    enabled: Some(true),
                    max_macro_cycles: Some(3),
                    max_craft_rounds_per_cycle: Some(2),
                    auto_enter_craft: Some("on_graph_complete".into()),
                    craft_on_small_tasks: Some(false),
                    min_checklist_items_for_craft: Some(3),
                }),
            };
        }
        LhtPresetId::LongFix => {
            *base = LongHorizonConfigToml {
                enabled: Some(true),
                mode: Some("auto".into()),
                max_nudges_per_item: Some(5),
                blocked_nudges_without_progress: Some(3),
                reinject_every_steps: Some(0),
                progress_via_git: Some(true),
                auto_continue: Some(false),
                max_auto_continue_rounds: Some(16),
                // Verification-focused without macro CRAFT: hard product gates.
                completion_gate: with_product_gate_modes(gate, "enforce", "enforce", "enforce"),
                macro_loop: Some(MacroLoopConfigToml {
                    enabled: Some(false),
                    ..MacroLoopConfigToml::default()
                }),
            };
        }
        LhtPresetId::CraftAudit => {
            *base = LongHorizonConfigToml {
                enabled: Some(false),
                mode: Some("auto".into()),
                max_nudges_per_item: Some(5),
                blocked_nudges_without_progress: Some(3),
                reinject_every_steps: Some(0),
                progress_via_git: Some(true),
                auto_continue: Some(false),
                max_auto_continue_rounds: Some(16),
                // LHT off — leave gate modes soft so re-enabling is not surprising.
                completion_gate: with_product_gate_modes(gate, "observe", "observe", "observe"),
                macro_loop: Some(MacroLoopConfigToml {
                    enabled: Some(false),
                    ..MacroLoopConfigToml::default()
                }),
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lht_config::CompletionGateVerifyToml;

    #[test]
    fn long_refactor_enables_macro_loop() {
        let mut lh = LongHorizonConfigToml::default();
        apply_lht_preset(&mut lh, LhtPresetId::LongRefactor);
        assert_eq!(lh.enabled, Some(true));
        assert_eq!(lh.mode.as_deref(), Some("strict"));
        assert_eq!(lh.reinject_every_steps, Some(5));
        let ml = lh.macro_loop.unwrap();
        assert_eq!(ml.enabled, Some(true));
        assert_eq!(ml.auto_enter_craft.as_deref(), Some("on_graph_complete"));
    }

    #[test]
    fn long_refactor_and_long_fix_enforce_product_gates() {
        let mut lh = LongHorizonConfigToml::default();
        apply_lht_preset(&mut lh, LhtPresetId::LongRefactor);
        let gate = lh.completion_gate.as_ref().expect("gate");
        assert_eq!(gate.auto_verify_replay.as_deref(), Some("enforce"));
        assert_eq!(gate.toolchain_gate.as_deref(), Some("enforce"));
        assert_eq!(gate.stub_gate.as_deref(), Some("enforce"));

        apply_lht_preset(&mut lh, LhtPresetId::LongFix);
        let gate = lh.completion_gate.as_ref().expect("gate");
        assert_eq!(gate.auto_verify_replay.as_deref(), Some("enforce"));
        assert_eq!(gate.toolchain_gate.as_deref(), Some("enforce"));
        assert_eq!(gate.stub_gate.as_deref(), Some("enforce"));
        assert_eq!(lh.mode.as_deref(), Some("auto"));
        assert_eq!(lh.macro_loop.as_ref().and_then(|m| m.enabled), Some(false));
    }

    #[test]
    fn code_default_observes_and_preserves_verify_rows() {
        let mut lh = LongHorizonConfigToml {
            completion_gate: Some(CompletionGateConfigToml {
                verify: vec![CompletionGateVerifyToml {
                    id: "custom".into(),
                    cmd: Some("npm test".into()),
                    ..Default::default()
                }],
                auto_verify_replay: Some("off".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        apply_lht_preset(&mut lh, LhtPresetId::CodeDefault);
        let gate = lh.completion_gate.as_ref().expect("gate");
        assert_eq!(gate.verify.len(), 1);
        assert_eq!(gate.verify[0].id, "custom");
        assert_eq!(gate.auto_verify_replay.as_deref(), Some("observe"));
        assert_eq!(gate.toolchain_gate.as_deref(), Some("observe"));
        assert_eq!(gate.stub_gate.as_deref(), Some("observe"));
    }
}
