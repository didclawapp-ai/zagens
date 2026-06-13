//! Tool policy engine (kernel-v2 M3).
//!
//! Single decision point for approval, sandbox class, and parallel scheduling
//! keys. Replaces scattered string heuristics once `tools.policy = "engine"`.

use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use crate::ApprovalRequirement;
use crate::tool_manifest::{Footprint, FootprintProvenance, ResourceSet, SpawnClass, ToolManifest};

static POLICY_SHADOW_COMPARISONS: AtomicU64 = AtomicU64::new(0);
static POLICY_SHADOW_DIFFS: AtomicU64 = AtomicU64::new(0);

/// Session mode slice consumed by policy rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicySessionMode {
    Agent,
    Plan,
    Yolo,
}

/// Whether a tool call needs user approval before execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalNeed {
    Auto,
    Ask,
    Deny,
}

/// Sandbox enforcement tier derived from footprint (proposal §8.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxClass {
    None,
    Sandboxed,
    Strict,
}

/// Resource bucket for DAG / batch parallel scheduling (M4 consumer).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParallelResourceKey {
    ReadOnlyBatch,
}

/// Policy output consumed by the turn loop and future DAG scheduler.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyDecision {
    pub approval: ApprovalNeed,
    pub sandbox: SandboxClass,
    pub parallel_key: Option<ParallelResourceKey>,
    /// Scheduling / UI read-only flag (distinct from parallel eligibility).
    pub read_only: bool,
}

/// Legacy-compatible plan flags derived from a policy decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PolicyPlanMeta {
    pub approval_required: bool,
    pub read_only: bool,
    pub supports_parallel: bool,
}

/// Inputs for a single tool policy evaluation.
#[derive(Debug, Clone)]
pub struct PolicyInput {
    pub session_mode: PolicySessionMode,
    pub manifest: ToolManifest,
    /// Registry hint until footprints fully replace capability metadata.
    pub legacy_approval: ApprovalRequirement,
    /// Built-in registry hint for parallel eligibility.
    pub supports_parallel_hint: bool,
    /// Session trust / YOLO bypasses approval prompts.
    pub trust_mode: bool,
}

/// Shadow-mode counters (`tools.policy = "shadow"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PolicyShadowStats {
    pub comparisons: u64,
    pub diffs: u64,
}

impl PolicyDecision {
    #[must_use]
    pub fn plan_meta(&self) -> PolicyPlanMeta {
        PolicyPlanMeta {
            approval_required: !matches!(self.approval, ApprovalNeed::Auto),
            read_only: self.read_only,
            supports_parallel: self.parallel_key.is_some(),
        }
    }
}

impl PolicyPlanMeta {
    #[must_use]
    pub fn differs_from(&self, other: &Self) -> bool {
        self != other
    }
}

/// Policy engine — stateless rule set (proposal §8.1, §8.1.2).
#[derive(Debug, Clone, Copy, Default)]
pub struct PolicyEngine;

impl PolicyEngine {
    #[must_use]
    pub fn decide(input: &PolicyInput) -> PolicyDecision {
        let footprint = effective_footprint(&input.manifest);
        let approval = decide_approval(input, &footprint);
        let sandbox = decide_sandbox(&footprint, input.manifest.provenance);
        let parallel_key = decide_parallel_key(input, &footprint, approval);
        let read_only = decide_read_only(&footprint, input.manifest.provenance);
        PolicyDecision {
            approval,
            sandbox,
            parallel_key,
            read_only,
        }
    }
}

/// Record a shadow comparison between legacy and engine plan flags.
pub fn record_policy_shadow_diff(
    tool_name: &str,
    legacy: &PolicyPlanMeta,
    engine: &PolicyPlanMeta,
) {
    POLICY_SHADOW_COMPARISONS.fetch_add(1, Ordering::Relaxed);
    if legacy.differs_from(engine) {
        POLICY_SHADOW_DIFFS.fetch_add(1, Ordering::Relaxed);
        tracing::debug!(
            tool = tool_name,
            legacy_approval = legacy.approval_required,
            engine_approval = engine.approval_required,
            legacy_read_only = legacy.read_only,
            engine_read_only = engine.read_only,
            legacy_parallel = legacy.supports_parallel,
            engine_parallel = engine.supports_parallel,
            "policy_engine shadow diff"
        );
    }
}

#[must_use]
pub fn policy_shadow_stats() -> PolicyShadowStats {
    PolicyShadowStats {
        comparisons: POLICY_SHADOW_COMPARISONS.load(Ordering::Relaxed),
        diffs: POLICY_SHADOW_DIFFS.load(Ordering::Relaxed),
    }
}

fn effective_footprint(manifest: &ToolManifest) -> Footprint {
    match manifest.provenance {
        FootprintProvenance::McpSelfDeclared => {
            let mut footprint = manifest.footprint.clone();
            if footprint.writes.is_empty() {
                footprint.writes = ResourceSet {
                    workspace_write: true,
                    network_write: true,
                    ..ResourceSet::default()
                };
            }
            footprint
        }
        _ => manifest.footprint.clone(),
    }
}

fn decide_approval(input: &PolicyInput, footprint: &Footprint) -> ApprovalNeed {
    if input.trust_mode || matches!(input.session_mode, PolicySessionMode::Yolo) {
        return ApprovalNeed::Auto;
    }

    match input.manifest.provenance {
        FootprintProvenance::McpSelfDeclared => ApprovalNeed::Ask,
        _ => match input.legacy_approval {
            ApprovalRequirement::Auto => {
                if footprint.writes.is_empty() && footprint.spawns == SpawnClass::None {
                    ApprovalNeed::Auto
                } else {
                    ApprovalNeed::Ask
                }
            }
            ApprovalRequirement::Suggest | ApprovalRequirement::Required => ApprovalNeed::Ask,
        },
    }
}

fn decide_sandbox(footprint: &Footprint, provenance: FootprintProvenance) -> SandboxClass {
    if provenance == FootprintProvenance::McpSelfDeclared {
        return SandboxClass::Strict;
    }
    match footprint.spawns {
        SpawnClass::Privileged => SandboxClass::Strict,
        SpawnClass::Sandboxed => SandboxClass::Sandboxed,
        SpawnClass::None if !footprint.writes.is_empty() => SandboxClass::Strict,
        SpawnClass::None => SandboxClass::None,
    }
}

fn decide_read_only(footprint: &Footprint, provenance: FootprintProvenance) -> bool {
    if provenance == FootprintProvenance::McpSelfDeclared {
        return false;
    }
    footprint.writes.is_empty() && footprint.spawns == SpawnClass::None
}

fn decide_parallel_key(
    input: &PolicyInput,
    footprint: &Footprint,
    approval: ApprovalNeed,
) -> Option<ParallelResourceKey> {
    if input.manifest.provenance == FootprintProvenance::McpSelfDeclared {
        return None;
    }
    if approval != ApprovalNeed::Auto {
        return None;
    }
    if !footprint.writes.is_empty() || footprint.spawns != SpawnClass::None {
        return None;
    }
    if !input.supports_parallel_hint {
        return None;
    }
    Some(ParallelResourceKey::ReadOnlyBatch)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ToolCapability;

    fn read_only_manifest(name: &str, provenance: FootprintProvenance) -> ToolManifest {
        ToolManifest::derive_conservative(name, &[ToolCapability::ReadOnly], false, provenance)
    }

    fn default_input(manifest: ToolManifest) -> PolicyInput {
        PolicyInput {
            session_mode: PolicySessionMode::Agent,
            manifest,
            legacy_approval: ApprovalRequirement::Auto,
            supports_parallel_hint: true,
            trust_mode: false,
        }
    }

    #[test]
    fn builtin_read_only_tool_can_parallelize() {
        let decision = PolicyEngine::decide(&default_input(read_only_manifest(
            "read_file",
            FootprintProvenance::BuiltIn,
        )));
        assert_eq!(decision.approval, ApprovalNeed::Auto);
        assert_eq!(
            decision.parallel_key,
            Some(ParallelResourceKey::ReadOnlyBatch)
        );
        let meta = decision.plan_meta();
        assert!(meta.read_only);
        assert!(meta.supports_parallel);
    }

    #[test]
    fn mcp_self_declared_read_only_still_requires_approval() {
        let manifest = read_only_manifest("mcp_evil_read", FootprintProvenance::McpSelfDeclared);
        let decision = PolicyEngine::decide(&PolicyInput {
            legacy_approval: ApprovalRequirement::Auto,
            supports_parallel_hint: true,
            ..default_input(manifest)
        });
        assert_eq!(decision.approval, ApprovalNeed::Ask);
        assert_eq!(decision.sandbox, SandboxClass::Strict);
        assert!(decision.parallel_key.is_none());
        let meta = decision.plan_meta();
        assert!(meta.approval_required);
        assert!(!meta.read_only);
        assert!(!meta.supports_parallel);
    }

    #[test]
    fn mcp_self_declared_write_is_not_relaxed() {
        let manifest = ToolManifest::derive_conservative(
            "mcp_writer",
            &[ToolCapability::Network, ToolCapability::RequiresApproval],
            false,
            FootprintProvenance::McpSelfDeclared,
        );
        let decision = PolicyEngine::decide(&default_input(manifest));
        assert_eq!(decision.approval, ApprovalNeed::Ask);
        assert!(decision.parallel_key.is_none());
    }

    #[test]
    fn trust_mode_skips_approval() {
        let manifest = read_only_manifest("mcp_evil", FootprintProvenance::McpSelfDeclared);
        let decision = PolicyEngine::decide(&PolicyInput {
            trust_mode: true,
            ..default_input(manifest)
        });
        assert_eq!(decision.approval, ApprovalNeed::Auto);
    }

    #[test]
    fn write_footprint_never_parallelizes() {
        let manifest = ToolManifest::derive_conservative(
            "write_file",
            &[ToolCapability::WritesFiles],
            true,
            FootprintProvenance::BuiltIn,
        );
        let decision = PolicyEngine::decide(&PolicyInput {
            legacy_approval: ApprovalRequirement::Suggest,
            supports_parallel_hint: false,
            ..default_input(manifest)
        });
        assert_eq!(decision.approval, ApprovalNeed::Ask);
        assert!(decision.parallel_key.is_none());
    }

    #[test]
    fn shadow_stats_increment_on_diff() {
        let before = policy_shadow_stats();
        record_policy_shadow_diff(
            "read_file",
            &PolicyPlanMeta {
                approval_required: false,
                read_only: true,
                supports_parallel: true,
            },
            &PolicyPlanMeta {
                approval_required: true,
                read_only: false,
                supports_parallel: false,
            },
        );
        let after = policy_shadow_stats();
        assert_eq!(after.comparisons, before.comparisons + 1);
        assert_eq!(after.diffs, before.diffs + 1);
    }
}
