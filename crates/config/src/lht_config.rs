//! `[long_horizon]` / `[long_horizon.completion_gate]` on-disk schema for Zagens config.toml.
//!
//! Mirrors `deepseek_core::long_horizon` for serde I/O without a core dependency cycle.

use serde::{Deserialize, Serialize};

/// `[long_horizon]` table.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LongHorizonConfigToml {
    #[serde(default)]
    pub enabled: Option<bool>,
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
}

/// `[long_horizon.macro_loop]` table (Phase 4 macro review cycle).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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

/// `[long_horizon.completion_gate]` table.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CompletionGateConfigToml {
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub max_manifest_rounds: Option<u32>,
    #[serde(default)]
    pub max_audit_rounds: Option<u32>,
    #[serde(default)]
    pub max_infra_strikes: Option<u32>,
    #[serde(default)]
    pub verify: Vec<CompletionGateVerifyToml>,
    #[serde(default)]
    pub deliverable: Vec<CompletionGateDeliverableToml>,
    #[serde(default)]
    pub auto_verify_replay: Option<String>,
    #[serde(default)]
    pub toolchain_gate: Option<String>,
    #[serde(default)]
    pub stub_gate: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CompletionGateVerifyToml {
    pub id: String,
    #[serde(default)]
    pub cmd: Option<String>,
    #[serde(default)]
    pub argv: Option<Vec<String>>,
    #[serde(default)]
    pub shell: Option<String>,
    #[serde(default)]
    pub timeout_secs: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CompletionGateDeliverableToml {
    pub id: String,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub glob: Option<String>,
    #[serde(default)]
    pub optional_verify_cmd: Option<String>,
    #[serde(default)]
    pub tracked: Option<bool>,
}

/// Product defaults for first-run `config.toml` and UI fallbacks.
#[must_use]
pub fn product_defaults() -> LongHorizonConfigToml {
    LongHorizonConfigToml {
        enabled: Some(true),
        mode: Some("auto".into()),
        max_nudges_per_item: Some(5),
        blocked_nudges_without_progress: Some(3),
        reinject_every_steps: Some(0),
        progress_via_git: Some(true),
        auto_continue: Some(false),
        max_auto_continue_rounds: Some(16),
        completion_gate: Some(CompletionGateConfigToml {
            auto_verify_replay: Some("observe".into()),
            toolchain_gate: Some("observe".into()),
            stub_gate: Some("observe".into()),
            max_manifest_rounds: Some(5),
            max_audit_rounds: Some(5),
            max_infra_strikes: Some(3),
            ..CompletionGateConfigToml::default()
        }),
        macro_loop: Some(MacroLoopConfigToml {
            enabled: Some(false),
            max_macro_cycles: Some(3),
            max_craft_rounds_per_cycle: Some(2),
            auto_enter_craft: Some("user_confirm".into()),
            craft_on_small_tasks: Some(false),
            min_checklist_items_for_craft: Some(3),
        }),
    }
}

#[must_use]
pub fn resolve_lht(cfg: &Option<LongHorizonConfigToml>) -> LongHorizonConfigToml {
    cfg.clone().unwrap_or_else(product_defaults)
}

#[must_use]
pub fn normalize_gate_mode(raw: &str) -> String {
    match raw.trim().to_ascii_lowercase().as_str() {
        "enforce" => "enforce".into(),
        "observe" => "observe".into(),
        _ => "off".into(),
    }
}

#[must_use]
pub fn normalize_lht_mode(raw: &str) -> String {
    if raw.trim().eq_ignore_ascii_case("strict") {
        "strict".into()
    } else {
        "auto".into()
    }
}
