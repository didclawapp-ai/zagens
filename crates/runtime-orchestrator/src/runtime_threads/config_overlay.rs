//! Per-thread session config overrides (C scheme / multi-tenant overlay).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Field-level override for `[compaction]`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct CompactionOverlay {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_compact: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_threshold: Option<usize>,
}

/// Field-level override for `[memory]`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct MemoryOverlay {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

/// Field-level override for `[topic_memory]`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct TopicMemoryOverlay {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inject_interval: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attribution: Option<String>,
}

/// Field-level override for `[snapshots]`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct SnapshotsOverlay {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_age_days: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_workspace_gb: Option<f64>,
}

/// Field-level override for `[lsp]`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct LspOverlay {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poll_after_edit_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_diagnostics_per_file: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_warnings: Option<bool>,
}

/// API response for `GET /v1/threads/{id}/config`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadConfigResponse {
    /// Global baseline view (no thread overlay applied) — lets the UI show
    /// "inherited from global" vs "session override" per field.
    pub base: ThreadConfigOverlay,
    pub overlay: Option<ThreadConfigOverlay>,
    pub effective: ThreadConfigOverlay,
}

/// Per-thread session config overlay. `None` at the field level means inherit global base.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ThreadConfigOverlay {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub long_horizon: Option<zagens_core::long_horizon::LongHorizonConfigToml>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compaction: Option<CompactionOverlay>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub features: Option<zagens_core::features::FeaturesToml>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory: Option<MemoryOverlay>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topic_memory: Option<TopicMemoryOverlay>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lsp: Option<LspOverlay>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshots: Option<SnapshotsOverlay>,
    /// Overrides global `approval_policy` for this thread (next turn).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_policy: Option<String>,
    /// Composer LHT tri-state: `auto` | `strict` | `off`. Replaces `settings.toml` path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lht_composer_mode: Option<String>,
    /// Forward-compat catch-all: unknown fields written by newer versions survive a
    /// round-trip through older readers instead of being silently dropped (plan §5.1/§11).
    #[serde(flatten, default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub extras: serde_json::Map<String, serde_json::Value>,
}

impl ThreadConfigOverlay {
    /// Merge `patch` into `self` field-by-field (for partial PUT).
    pub fn merge_from(&mut self, patch: ThreadConfigOverlay) {
        if let Some(lh) = patch.long_horizon {
            self.long_horizon = Some(merge_long_horizon_toml(
                self.long_horizon.take().unwrap_or_default(),
                lh,
            ));
        }
        if let Some(c) = patch.compaction {
            self.compaction = Some(merge_compaction_overlay(self.compaction.take(), c));
        }
        if let Some(f) = patch.features {
            self.features = Some(merge_features_toml(self.features.take(), f));
        }
        if let Some(m) = patch.memory {
            self.memory = Some(merge_memory_overlay(self.memory.take(), m));
        }
        if let Some(tm) = patch.topic_memory {
            self.topic_memory = Some(merge_topic_memory_overlay(self.topic_memory.take(), tm));
        }
        if let Some(lsp) = patch.lsp {
            self.lsp = Some(merge_lsp_overlay(self.lsp.take(), lsp));
        }
        if let Some(s) = patch.snapshots {
            self.snapshots = Some(merge_snapshots_overlay(self.snapshots.take(), s));
        }
        if patch.approval_policy.is_some() {
            self.approval_policy = patch.approval_policy;
        }
        if patch.lht_composer_mode.is_some() {
            self.lht_composer_mode = patch.lht_composer_mode;
        }
        for (k, v) in patch.extras {
            self.extras.insert(k, v);
        }
    }

    /// Clear one top-level overlay section (DELETE /config/{field}).
    pub fn clear_field(&mut self, field: &str) -> bool {
        match field.trim() {
            "long_horizon" => self.long_horizon = None,
            "compaction" => self.compaction = None,
            "features" => self.features = None,
            "memory" => self.memory = None,
            "topic_memory" => self.topic_memory = None,
            "lsp" => self.lsp = None,
            "snapshots" => self.snapshots = None,
            "approval_policy" => self.approval_policy = None,
            "lht_composer_mode" => self.lht_composer_mode = None,
            other => return self.extras.remove(other).is_some(),
        }
        true
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.long_horizon.is_none()
            && self.compaction.is_none()
            && self.features.is_none()
            && self.memory.is_none()
            && self.topic_memory.is_none()
            && self.lsp.is_none()
            && self.snapshots.is_none()
            && self.approval_policy.is_none()
            && self.lht_composer_mode.is_none()
            && self.extras.is_empty()
    }
}

fn merge_long_horizon_toml(
    base: zagens_core::long_horizon::LongHorizonConfigToml,
    patch: zagens_core::long_horizon::LongHorizonConfigToml,
) -> zagens_core::long_horizon::LongHorizonConfigToml {
    use zagens_core::long_horizon::LongHorizonConfigToml;
    LongHorizonConfigToml {
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

fn merge_compaction_overlay(
    base: Option<CompactionOverlay>,
    patch: CompactionOverlay,
) -> CompactionOverlay {
    let base = base.unwrap_or_default();
    CompactionOverlay {
        auto_compact: patch.auto_compact.or(base.auto_compact),
        token_threshold: patch.token_threshold.or(base.token_threshold),
    }
}

fn merge_features_toml(
    base: Option<zagens_core::features::FeaturesToml>,
    patch: zagens_core::features::FeaturesToml,
) -> zagens_core::features::FeaturesToml {
    let mut base = base.unwrap_or_default();
    for (k, v) in patch.entries {
        base.entries.insert(k, v);
    }
    base
}

fn merge_memory_overlay(base: Option<MemoryOverlay>, patch: MemoryOverlay) -> MemoryOverlay {
    let base = base.unwrap_or_default();
    MemoryOverlay {
        enabled: patch.enabled.or(base.enabled),
    }
}

fn merge_topic_memory_overlay(
    base: Option<TopicMemoryOverlay>,
    patch: TopicMemoryOverlay,
) -> TopicMemoryOverlay {
    let base = base.unwrap_or_default();
    TopicMemoryOverlay {
        enabled: patch.enabled.or(base.enabled),
        graph_path: patch.graph_path.or(base.graph_path),
        inject_interval: patch.inject_interval.or(base.inject_interval),
        attribution: patch.attribution.or(base.attribution),
    }
}

fn merge_lsp_overlay(base: Option<LspOverlay>, patch: LspOverlay) -> LspOverlay {
    let base = base.unwrap_or_default();
    LspOverlay {
        enabled: patch.enabled.or(base.enabled),
        poll_after_edit_ms: patch.poll_after_edit_ms.or(base.poll_after_edit_ms),
        max_diagnostics_per_file: patch
            .max_diagnostics_per_file
            .or(base.max_diagnostics_per_file),
        include_warnings: patch.include_warnings.or(base.include_warnings),
    }
}

fn merge_snapshots_overlay(
    base: Option<SnapshotsOverlay>,
    patch: SnapshotsOverlay,
) -> SnapshotsOverlay {
    let base = base.unwrap_or_default();
    SnapshotsOverlay {
        enabled: patch.enabled.or(base.enabled),
        max_age_days: patch.max_age_days.or(base.max_age_days),
        max_workspace_gb: patch.max_workspace_gb.or(base.max_workspace_gb),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_merge_long_horizon_partial() {
        let mut overlay = ThreadConfigOverlay {
            long_horizon: Some(zagens_core::long_horizon::LongHorizonConfigToml {
                enabled: Some(true),
                mode: Some("strict".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        overlay.merge_from(ThreadConfigOverlay {
            long_horizon: Some(zagens_core::long_horizon::LongHorizonConfigToml {
                max_nudges_per_item: Some(9),
                ..Default::default()
            }),
            ..Default::default()
        });
        let lh = overlay.long_horizon.expect("lh");
        assert_eq!(lh.enabled, Some(true));
        assert_eq!(lh.mode.as_deref(), Some("strict"));
        assert_eq!(lh.max_nudges_per_item, Some(9));
    }

    #[test]
    fn overlay_clear_field() {
        let mut overlay = ThreadConfigOverlay {
            lht_composer_mode: Some("strict".into()),
            ..Default::default()
        };
        assert!(overlay.clear_field("lht_composer_mode"));
        assert!(overlay.lht_composer_mode.is_none());
        assert!(!overlay.clear_field("unknown"));
    }

    #[test]
    fn overlay_preserves_unknown_fields_round_trip() {
        // A field written by a newer version must survive an older reader's
        // deserialize → serialize cycle instead of being silently dropped.
        let json = r#"{ "lht_composer_mode": "strict", "future_knob": { "x": 1 } }"#;
        let overlay: ThreadConfigOverlay = serde_json::from_str(json).expect("parse");
        assert!(!overlay.is_empty());
        assert_eq!(
            overlay.extras.get("future_knob"),
            Some(&serde_json::json!({ "x": 1 }))
        );
        let round = serde_json::to_value(&overlay).expect("serialize");
        assert_eq!(
            round.get("future_knob"),
            Some(&serde_json::json!({ "x": 1 }))
        );

        let mut overlay = overlay;
        assert!(overlay.clear_field("future_knob"));
        assert!(overlay.extras.is_empty());
    }

    #[test]
    fn thread_record_deserializes_without_overlay() {
        let json = r#"{
            "schema_version": 2,
            "id": "thr_old",
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:00Z",
            "model": "deepseek-v4-pro",
            "workspace": "/tmp/ws",
            "mode": "agent",
            "allow_shell": true,
            "trust_mode": false,
            "auto_approve": false,
            "task_type": "code"
        }"#;
        let thread: super::super::ThreadRecord = serde_json::from_str(json).expect("parse");
        assert!(thread.config_overlay.is_none());
    }
}
