//! LHT harness presets (§5.2 LONG_HORIZON_CODE_TASKS) — config.toml overlays.

use crate::lht_config::{LongHorizonConfigToml, MacroLoopConfigToml};

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

/// Apply a harness preset onto `[long_horizon]`, preserving operator completion_gate rows.
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
                completion_gate: gate,
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
                auto_continue: Some(false),
                max_auto_continue_rounds: Some(16),
                completion_gate: gate,
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
                completion_gate: gate,
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
                completion_gate: gate,
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
}
