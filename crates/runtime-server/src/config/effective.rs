//! Effective session config: global base ⊕ thread overlay (⊕ turn override).

use zagens_runtime_orchestrator::runtime_threads::{
    CompactionOverlay, LspOverlay, MemoryOverlay, SnapshotsOverlay, ThreadConfigOverlay,
    TopicMemoryOverlay,
};

use super::types::{
    CompactionConfigToml, Config, LspConfigToml, MemoryConfig, SnapshotsConfig, TopicMemoryConfig,
};
use crate::features::FeaturesToml;

/// Resolved LHT composer tri-state for engine spawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EffectiveLhtComposerMode {
    #[default]
    Auto,
    Strict,
    Off,
}

impl EffectiveLhtComposerMode {
    #[must_use]
    pub fn parse(raw: Option<&str>) -> Self {
        match raw.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
            Some("strict") => Self::Strict,
            Some("off") => Self::Off,
            _ => Self::Auto,
        }
    }

    #[must_use]
    pub fn from_settings_fallback(settings_raw: Option<&str>) -> Self {
        if settings_raw.is_some() {
            return Self::parse(settings_raw);
        }
        Self::Auto
    }
}

/// Merge process-global base config with a thread's optional overlay (field-level).
#[must_use]
pub fn resolve_effective_config(base: &Config, overlay: Option<&ThreadConfigOverlay>) -> Config {
    let Some(overlay) = overlay else {
        return base.clone();
    };
    if overlay.is_empty() {
        return base.clone();
    }

    let mut out = base.clone();
    if let Some(lh) = &overlay.long_horizon {
        out.long_horizon = Some(merge_long_horizon(
            out.long_horizon.take().unwrap_or_default(),
            lh.clone(),
        ));
    }
    if let Some(c) = &overlay.compaction {
        out.compaction = Some(merge_compaction(out.compaction.as_ref(), c));
    }
    if let Some(f) = &overlay.features {
        out.features = merge_features_overlay(out.features.clone(), f);
    }
    if let Some(m) = &overlay.memory {
        out.memory = Some(merge_memory(out.memory.as_ref(), m));
    }
    if let Some(tm) = &overlay.topic_memory {
        out.topic_memory = Some(merge_topic_memory(out.topic_memory.as_ref(), tm));
    }
    if let Some(lsp) = &overlay.lsp {
        out.lsp = Some(merge_lsp(out.lsp.take(), lsp));
    }
    if let Some(s) = &overlay.snapshots {
        out.snapshots = Some(merge_snapshots(out.snapshots.as_ref(), s));
    }
    if let Some(policy) = &overlay.approval_policy {
        out.approval_policy = Some(policy.clone());
    }
    out
}

/// Composer mode: overlay wins, else legacy `settings.toml` (until P3 removes it).
#[must_use]
pub fn resolve_lht_composer_mode(
    overlay: Option<&ThreadConfigOverlay>,
    settings_fallback: Option<&str>,
) -> EffectiveLhtComposerMode {
    if let Some(mode) = overlay.and_then(|o| o.lht_composer_mode.as_deref()) {
        return EffectiveLhtComposerMode::parse(Some(mode));
    }
    EffectiveLhtComposerMode::from_settings_fallback(settings_fallback)
}

fn merge_long_horizon(
    base: zagens_core::long_horizon::LongHorizonConfigToml,
    patch: zagens_core::long_horizon::LongHorizonConfigToml,
) -> zagens_core::long_horizon::LongHorizonConfigToml {
    zagens_core::long_horizon::LongHorizonConfigToml {
        enabled: patch.enabled.or(base.enabled),
        mode: patch.mode.or(base.mode),
        max_nudges_per_item: patch.max_nudges_per_item.or(base.max_nudges_per_item),
        blocked_nudges_without_progress: patch
            .blocked_nudges_without_progress
            .or(base.blocked_nudges_without_progress),
        reinject_every_steps: patch.reinject_every_steps.or(base.reinject_every_steps),
        progress_via_git: patch.progress_via_git.or(base.progress_via_git),
        auto_continue: patch.auto_continue.or(base.auto_continue),
        max_auto_continue_rounds: patch
            .max_auto_continue_rounds
            .or(base.max_auto_continue_rounds),
        completion_gate: patch.completion_gate.or(base.completion_gate),
        macro_loop: patch.macro_loop.or(base.macro_loop),
        adversarial_audit: patch.adversarial_audit.or(base.adversarial_audit),
    }
}

fn merge_compaction(
    base: Option<&CompactionConfigToml>,
    patch: &CompactionOverlay,
) -> CompactionConfigToml {
    let base = base.cloned().unwrap_or_default();
    CompactionConfigToml {
        auto_compact: patch.auto_compact.or(base.auto_compact),
        token_threshold: patch.token_threshold.or(base.token_threshold),
    }
}

fn merge_memory(base: Option<&MemoryConfig>, patch: &MemoryOverlay) -> MemoryConfig {
    let base = base.cloned().unwrap_or_default();
    MemoryConfig {
        enabled: patch.enabled.or(base.enabled),
    }
}

fn merge_topic_memory(
    base: Option<&TopicMemoryConfig>,
    patch: &TopicMemoryOverlay,
) -> TopicMemoryConfig {
    let base = base.cloned().unwrap_or_default();
    TopicMemoryConfig {
        enabled: patch.enabled.or(base.enabled),
        graph_path: patch.graph_path.clone().or(base.graph_path),
        inject_interval: patch.inject_interval.or(base.inject_interval),
        attribution: patch.attribution.clone().or(base.attribution),
    }
}

fn merge_features_overlay(
    base: Option<FeaturesToml>,
    patch: &zagens_core::features::FeaturesToml,
) -> Option<FeaturesToml> {
    let mut base = base.unwrap_or_default();
    for (k, v) in &patch.entries {
        base.entries.insert(k.clone(), *v);
    }
    Some(base)
}

fn merge_lsp(base: Option<LspConfigToml>, patch: &LspOverlay) -> LspConfigToml {
    let base = base.unwrap_or(LspConfigToml {
        enabled: None,
        poll_after_edit_ms: None,
        max_diagnostics_per_file: None,
        include_warnings: None,
        servers: None,
    });
    LspConfigToml {
        enabled: patch.enabled.or(base.enabled),
        poll_after_edit_ms: patch.poll_after_edit_ms.or(base.poll_after_edit_ms),
        max_diagnostics_per_file: patch
            .max_diagnostics_per_file
            .or(base.max_diagnostics_per_file),
        include_warnings: patch.include_warnings.or(base.include_warnings),
        servers: base.servers,
    }
}

fn merge_snapshots(base: Option<&SnapshotsConfig>, patch: &SnapshotsOverlay) -> SnapshotsConfig {
    let base = base.cloned().unwrap_or_default();
    SnapshotsConfig {
        enabled: patch.enabled.unwrap_or(base.enabled),
        max_age_days: patch.max_age_days.unwrap_or(base.max_age_days),
        max_workspace_gb: patch.max_workspace_gb.unwrap_or(base.max_workspace_gb),
    }
}

/// Build API-facing effective view from resolved process config.
#[must_use]
pub fn config_effective_view(
    effective: &Config,
    composer_mode: EffectiveLhtComposerMode,
) -> ThreadConfigOverlay {
    let snapshots = effective.snapshots_config();
    ThreadConfigOverlay {
        long_horizon: effective.long_horizon.clone(),
        compaction: effective.compaction.as_ref().map(|c| CompactionOverlay {
            auto_compact: c.auto_compact,
            token_threshold: c.token_threshold,
        }),
        features: effective.features.clone(),
        memory: effective
            .memory
            .as_ref()
            .map(|m| MemoryOverlay { enabled: m.enabled }),
        topic_memory: effective
            .topic_memory
            .as_ref()
            .map(|tm| TopicMemoryOverlay {
                enabled: tm.enabled,
                graph_path: tm.graph_path.clone(),
                inject_interval: tm.inject_interval,
                attribution: tm.attribution.clone(),
            }),
        lsp: effective.lsp.as_ref().map(|l| LspOverlay {
            enabled: l.enabled,
            poll_after_edit_ms: l.poll_after_edit_ms,
            max_diagnostics_per_file: l.max_diagnostics_per_file,
            include_warnings: l.include_warnings,
        }),
        snapshots: Some(SnapshotsOverlay {
            enabled: Some(snapshots.enabled),
            max_age_days: Some(snapshots.max_age_days),
            max_workspace_gb: Some(snapshots.max_workspace_gb),
        }),
        approval_policy: effective.approval_policy.clone(),
        lht_composer_mode: Some(match composer_mode {
            EffectiveLhtComposerMode::Strict => "strict".into(),
            EffectiveLhtComposerMode::Off => "off".into(),
            EffectiveLhtComposerMode::Auto => "auto".into(),
        }),
        extras: serde_json::Map::new(),
    }
}

#[cfg(test)]
mod view_tests {
    use super::*;

    #[test]
    fn effective_view_includes_composer_mode() {
        let cfg = Config::default();
        let view = config_effective_view(&cfg, EffectiveLhtComposerMode::Strict);
        assert_eq!(view.lht_composer_mode.as_deref(), Some("strict"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zagens_core::long_horizon::LongHorizonConfigToml;

    fn base_with_lht_strict() -> Config {
        Config {
            long_horizon: Some(LongHorizonConfigToml {
                enabled: Some(true),
                mode: Some("strict".into()),
                max_nudges_per_item: Some(5),
                ..Default::default()
            }),
            ..Config::default()
        }
    }

    #[test]
    fn resolve_without_overlay_returns_base_clone() {
        let base = base_with_lht_strict();
        let effective = resolve_effective_config(&base, None);
        assert_eq!(
            effective
                .long_horizon
                .as_ref()
                .and_then(|lh| lh.mode.as_deref()),
            Some("strict")
        );
    }

    #[test]
    fn resolve_overlay_overrides_long_horizon_field() {
        let base = base_with_lht_strict();
        let overlay = ThreadConfigOverlay {
            long_horizon: Some(LongHorizonConfigToml {
                mode: Some("auto".into()),
                max_nudges_per_item: Some(9),
                ..Default::default()
            }),
            ..Default::default()
        };
        let effective = resolve_effective_config(&base, Some(&overlay));
        let lh = effective.long_horizon.expect("lh");
        assert_eq!(lh.mode.as_deref(), Some("auto"));
        assert_eq!(lh.max_nudges_per_item, Some(9));
        assert_eq!(lh.enabled, Some(true));
    }

    #[test]
    fn two_threads_different_overlays_resolve_independently() {
        let base = base_with_lht_strict();
        let overlay_a = ThreadConfigOverlay {
            long_horizon: Some(LongHorizonConfigToml {
                mode: Some("auto".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let overlay_b = ThreadConfigOverlay {
            compaction: Some(CompactionOverlay {
                auto_compact: Some(false),
                token_threshold: Some(42_000),
            }),
            ..Default::default()
        };
        let eff_a = resolve_effective_config(&base, Some(&overlay_a));
        let eff_b = resolve_effective_config(&base, Some(&overlay_b));
        assert_eq!(
            eff_a
                .long_horizon
                .as_ref()
                .and_then(|lh| lh.mode.as_deref()),
            Some("auto")
        );
        assert_eq!(
            eff_b
                .long_horizon
                .as_ref()
                .and_then(|lh| lh.mode.as_deref()),
            Some("strict")
        );
        assert_eq!(
            eff_b.compaction.as_ref().and_then(|c| c.auto_compact),
            Some(false)
        );
    }

    #[test]
    fn composer_mode_overlay_beats_settings_fallback() {
        let overlay = ThreadConfigOverlay {
            lht_composer_mode: Some("off".into()),
            ..Default::default()
        };
        assert_eq!(
            resolve_lht_composer_mode(Some(&overlay), Some("strict")),
            EffectiveLhtComposerMode::Off
        );
    }

    #[test]
    fn memory_overlay_merges_enabled_flag() {
        let base = Config {
            memory: Some(MemoryConfig {
                enabled: Some(true),
            }),
            ..Config::default()
        };
        let overlay = ThreadConfigOverlay {
            memory: Some(MemoryOverlay {
                enabled: Some(false),
            }),
            ..Default::default()
        };
        let effective = resolve_effective_config(&base, Some(&overlay));
        assert_eq!(
            effective.memory.as_ref().and_then(|m| m.enabled),
            Some(false)
        );
    }
}
