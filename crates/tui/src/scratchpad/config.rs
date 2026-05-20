//! Scratchpad runtime configuration (`[scratchpad]` in `~/.deepseek/config.toml`).

use serde::Deserialize;

/// Resolved scratchpad settings for engine + tools.
#[derive(Debug, Clone)]
pub struct ScratchpadConfig {
    pub enabled: bool,
    pub max_notes_per_run: usize,
    pub remind_after_readonly_tools: usize,
    pub remind_enabled: bool,
    pub inject_summary_max_chars: usize,
    pub inject_on_report_keywords: Vec<String>,
    pub retention_days: u32,
    /// Phase C1: soft warn when `accounted_ratio` is below this (default 0.85).
    pub coverage_soft_ratio: f64,
    /// Phase C1: hard block P2 summary when below this (default 0.60).
    pub coverage_hard_ratio: f64,
    pub coverage_hard_block_enabled: bool,
    /// When true, `deferred` counts toward accounted only with `kind=meta` reason (§6.12.4).
    pub coverage_count_deferred_as_accounted: bool,
    /// Phase C1: `set_area(deferred)` requires `kind=meta` with non-empty claim.
    pub require_deferred_meta: bool,
    /// L0 lists deferred areas when `reviewed_ratio` is below this (default 0.70).
    pub coverage_reviewed_warn_ratio: f64,
    /// Phase C2: prepend scratchpad verified `note_id` table to Auditor spawn.
    pub auditor_from_scratchpad: bool,
    /// Phase C2: include all MEDIUM in track A when count ≥ this (default 3).
    pub auditor_include_medium_min: usize,
}

impl Default for ScratchpadConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_notes_per_run: 2000,
            remind_after_readonly_tools: 8,
            remind_enabled: true,
            inject_summary_max_chars: 6000,
            inject_on_report_keywords: vec![
                "审查报告".into(),
                "final report".into(),
                "synthesize".into(),
                "write the report".into(),
                "写报告".into(),
                // L7b / §14 E1 — common full-repo audit prompts that bypassed C1
                "全库".into(),
                "全仓".into(),
                "代码级审核".into(),
                "代码级审查".into(),
                "repo-wide".into(),
                "code-level audit".into(),
                "deliverables".into(),
                "audit report".into(),
                "输出md".into(),
                "md格式".into(),
                "md 报告".into(),
                "code_review".into(),
            ],
            retention_days: 30,
            coverage_soft_ratio: 0.85,
            coverage_hard_ratio: 0.60,
            coverage_hard_block_enabled: true,
            coverage_count_deferred_as_accounted: true,
            require_deferred_meta: true,
            coverage_reviewed_warn_ratio: 0.70,
            auditor_from_scratchpad: true,
            auditor_include_medium_min: 3,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ScratchpadConfigToml {
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub max_notes_per_run: Option<usize>,
    #[serde(default)]
    pub remind_after_readonly_tools: Option<usize>,
    #[serde(default)]
    pub remind_enabled: Option<bool>,
    #[serde(default)]
    pub inject_summary_max_chars: Option<usize>,
    #[serde(default)]
    pub inject_on_report_keywords: Option<Vec<String>>,
    #[serde(default)]
    pub retention_days: Option<u32>,
    #[serde(default)]
    pub coverage_soft_ratio: Option<f64>,
    #[serde(default)]
    pub coverage_hard_ratio: Option<f64>,
    #[serde(default)]
    pub coverage_hard_block_enabled: Option<bool>,
    #[serde(default)]
    pub coverage_count_deferred_as_accounted: Option<bool>,
    #[serde(default)]
    pub require_deferred_meta: Option<bool>,
    #[serde(default)]
    pub coverage_reviewed_warn_ratio: Option<f64>,
    #[serde(default)]
    pub auditor_from_scratchpad: Option<bool>,
    #[serde(default)]
    pub auditor_include_medium_min: Option<usize>,
}

impl ScratchpadConfigToml {
    #[must_use]
    pub fn into_runtime(self) -> ScratchpadConfig {
        let defaults = ScratchpadConfig::default();
        ScratchpadConfig {
            enabled: self.enabled.unwrap_or(defaults.enabled),
            max_notes_per_run: self.max_notes_per_run.unwrap_or(defaults.max_notes_per_run),
            remind_after_readonly_tools: self
                .remind_after_readonly_tools
                .unwrap_or(defaults.remind_after_readonly_tools),
            remind_enabled: self.remind_enabled.unwrap_or(defaults.remind_enabled),
            inject_summary_max_chars: self
                .inject_summary_max_chars
                .unwrap_or(defaults.inject_summary_max_chars),
            inject_on_report_keywords: self
                .inject_on_report_keywords
                .unwrap_or(defaults.inject_on_report_keywords),
            retention_days: self.retention_days.unwrap_or(defaults.retention_days),
            coverage_soft_ratio: self
                .coverage_soft_ratio
                .unwrap_or(defaults.coverage_soft_ratio),
            coverage_hard_ratio: self
                .coverage_hard_ratio
                .unwrap_or(defaults.coverage_hard_ratio),
            coverage_hard_block_enabled: self
                .coverage_hard_block_enabled
                .unwrap_or(defaults.coverage_hard_block_enabled),
            coverage_count_deferred_as_accounted: self
                .coverage_count_deferred_as_accounted
                .unwrap_or(defaults.coverage_count_deferred_as_accounted),
            require_deferred_meta: self
                .require_deferred_meta
                .unwrap_or(defaults.require_deferred_meta),
            coverage_reviewed_warn_ratio: self
                .coverage_reviewed_warn_ratio
                .unwrap_or(defaults.coverage_reviewed_warn_ratio),
            auditor_from_scratchpad: self
                .auditor_from_scratchpad
                .unwrap_or(defaults.auditor_from_scratchpad),
            auditor_include_medium_min: self
                .auditor_include_medium_min
                .unwrap_or(defaults.auditor_include_medium_min),
        }
    }
}
