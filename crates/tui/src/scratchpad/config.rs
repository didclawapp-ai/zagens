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
            ],
            retention_days: 30,
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
        }
    }
}
