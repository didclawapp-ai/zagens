//! Bridge legacy tool-plan metadata to `PolicyEngine` (kernel-v2 M3).

use serde_json::Value;
use zagens_core::engine::dispatch::{
    is_mcp_tool_name, mcp_tool_approval_description, mcp_tool_is_parallel_safe,
    mcp_tool_is_read_only,
};
use zagens_core::engine::tool_catalog::{CODE_EXECUTION_TOOL_NAME, is_tool_search_tool};
use zagens_core::engine::turn_loop::{ToolPlanApprovalMeta, build_edit_file_approval_desc};
use zagens_core::turn::TurnLoopMode;
use zagens_tools::{
    ApprovalRequirement, FootprintProvenance, PolicyEngine, PolicyInput, PolicyPlanMeta,
    PolicySessionMode, ToolCapability, ToolManifest, record_policy_shadow_diff,
};

use crate::config::ToolsPolicyMode;
use crate::tools::ToolRegistry;
use crate::tools::spec::ApprovalRequirement as SpecApprovalRequirement;

fn is_first_party_mcp_surface_tool(name: &str) -> bool {
    matches!(
        name,
        "list_mcp_resources"
            | "list_mcp_resource_templates"
            | "read_mcp_resource"
            | "mcp_read_resource"
            | "mcp_get_prompt"
    )
}

fn session_mode_from_turn(mode: TurnLoopMode) -> PolicySessionMode {
    match mode {
        TurnLoopMode::Agent => PolicySessionMode::Agent,
        TurnLoopMode::Plan => PolicySessionMode::Plan,
        TurnLoopMode::Yolo => PolicySessionMode::Yolo,
    }
}

fn legacy_plan_meta(meta: &ToolPlanApprovalMeta) -> PolicyPlanMeta {
    PolicyPlanMeta {
        approval_required: meta.approval_required,
        read_only: meta.read_only,
        supports_parallel: meta.supports_parallel,
    }
}

/// Resolve approval / parallelism metadata using the legacy heuristic path.
#[must_use]
pub fn legacy_tool_plan_approval_meta(
    tool_name: &str,
    tool_input: &Value,
    registry: Option<&ToolRegistry>,
) -> ToolPlanApprovalMeta {
    if is_mcp_tool_name(tool_name) {
        return ToolPlanApprovalMeta {
            read_only: mcp_tool_is_read_only(tool_name),
            supports_parallel: mcp_tool_is_parallel_safe(tool_name),
            approval_required: !mcp_tool_is_read_only(tool_name),
            approval_description: mcp_tool_approval_description(tool_name),
        };
    }
    if let Some(registry) = registry
        && let Some(spec) = registry.get(tool_name)
    {
        return ToolPlanApprovalMeta {
            approval_required: spec.approval_requirement() != SpecApprovalRequirement::Auto,
            approval_description: if tool_name == "edit_file" {
                build_edit_file_approval_desc(tool_input)
            } else {
                spec.description().to_string()
            },
            supports_parallel: spec.supports_parallel(),
            read_only: spec.is_read_only(),
        };
    }
    if tool_name == CODE_EXECUTION_TOOL_NAME {
        return ToolPlanApprovalMeta {
            approval_required: true,
            approval_description: "Run model-provided Python code in local execution sandbox"
                .to_string(),
            supports_parallel: false,
            read_only: false,
        };
    }
    if is_tool_search_tool(tool_name) {
        return ToolPlanApprovalMeta {
            approval_required: false,
            approval_description: "Search tool catalog".to_string(),
            supports_parallel: false,
            read_only: true,
        };
    }
    ToolPlanApprovalMeta {
        approval_required: false,
        approval_description: String::new(),
        supports_parallel: false,
        read_only: false,
    }
}

fn build_policy_input(
    tool_name: &str,
    registry: Option<&ToolRegistry>,
    session_mode: PolicySessionMode,
    trust_mode: bool,
) -> PolicyInput {
    if is_mcp_tool_name(tool_name) {
        let (manifest, legacy_approval, supports_parallel_hint) =
            if is_first_party_mcp_surface_tool(tool_name) {
                let manifest = ToolManifest::derive_conservative(
                    tool_name,
                    &[ToolCapability::ReadOnly],
                    false,
                    FootprintProvenance::BuiltIn,
                );
                (
                    manifest,
                    ApprovalRequirement::Auto,
                    mcp_tool_is_parallel_safe(tool_name),
                )
            } else {
                let manifest = registry
                    .and_then(|r| r.get(tool_name))
                    .map(|spec| spec.manifest())
                    .unwrap_or_else(|| {
                        ToolManifest::derive_conservative(
                            tool_name,
                            &[ToolCapability::Network, ToolCapability::RequiresApproval],
                            false,
                            FootprintProvenance::McpSelfDeclared,
                        )
                    });
                (manifest, ApprovalRequirement::Required, false)
            };
        return PolicyInput {
            session_mode,
            manifest,
            legacy_approval,
            supports_parallel_hint,
            trust_mode,
        };
    }

    if let Some(registry) = registry
        && let Some(spec) = registry.get(tool_name)
    {
        return PolicyInput {
            session_mode,
            manifest: spec.manifest(),
            legacy_approval: map_spec_approval(spec.approval_requirement()),
            supports_parallel_hint: spec.supports_parallel(),
            trust_mode,
        };
    }

    if tool_name == CODE_EXECUTION_TOOL_NAME {
        return PolicyInput {
            session_mode,
            manifest: ToolManifest::derive_conservative(
                tool_name,
                &[ToolCapability::ExecutesCode],
                true,
                FootprintProvenance::BuiltIn,
            ),
            legacy_approval: ApprovalRequirement::Required,
            supports_parallel_hint: false,
            trust_mode,
        };
    }

    if is_tool_search_tool(tool_name) {
        return PolicyInput {
            session_mode,
            manifest: ToolManifest::derive_conservative(
                tool_name,
                &[ToolCapability::ReadOnly],
                false,
                FootprintProvenance::BuiltIn,
            ),
            legacy_approval: ApprovalRequirement::Auto,
            supports_parallel_hint: false,
            trust_mode,
        };
    }

    PolicyInput {
        session_mode,
        manifest: ToolManifest::derive_conservative(
            tool_name,
            &[],
            false,
            FootprintProvenance::BuiltIn,
        ),
        legacy_approval: ApprovalRequirement::Auto,
        supports_parallel_hint: false,
        trust_mode,
    }
}

fn map_spec_approval(level: SpecApprovalRequirement) -> ApprovalRequirement {
    match level {
        SpecApprovalRequirement::Auto => ApprovalRequirement::Auto,
        SpecApprovalRequirement::Suggest => ApprovalRequirement::Suggest,
        SpecApprovalRequirement::Required => ApprovalRequirement::Required,
    }
}

fn engine_plan_meta(
    tool_name: &str,
    registry: Option<&ToolRegistry>,
    session_mode: PolicySessionMode,
    trust_mode: bool,
) -> PolicyPlanMeta {
    let input = build_policy_input(tool_name, registry, session_mode, trust_mode);
    PolicyEngine::decide(&input).plan_meta()
}

fn apply_engine_meta(base: &ToolPlanApprovalMeta, engine: PolicyPlanMeta) -> ToolPlanApprovalMeta {
    ToolPlanApprovalMeta {
        approval_required: engine.approval_required,
        read_only: engine.read_only,
        supports_parallel: engine.supports_parallel,
        approval_description: base.approval_description.clone(),
    }
}

/// Resolve tool-plan metadata, optionally shadowing or switching to `PolicyEngine`.
#[must_use]
pub fn resolve_tool_plan_approval_meta(
    policy_mode: ToolsPolicyMode,
    turn_mode: TurnLoopMode,
    trust_mode: bool,
    tool_name: &str,
    tool_input: &Value,
    registry: Option<&ToolRegistry>,
) -> ToolPlanApprovalMeta {
    let legacy = legacy_tool_plan_approval_meta(tool_name, tool_input, registry);
    let session_mode = session_mode_from_turn(turn_mode);

    match policy_mode {
        ToolsPolicyMode::Legacy => legacy,
        ToolsPolicyMode::Shadow => {
            let engine = engine_plan_meta(tool_name, registry, session_mode, trust_mode);
            record_policy_shadow_diff(tool_name, &legacy_plan_meta(&legacy), &engine);
            legacy
        }
        ToolsPolicyMode::Engine => {
            let engine = engine_plan_meta(tool_name, registry, session_mode, trust_mode);
            apply_engine_meta(&legacy, engine)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zagens_tools::policy_shadow_stats;

    #[test]
    fn engine_mode_denies_mcp_self_declared_parallel() {
        let meta = resolve_tool_plan_approval_meta(
            ToolsPolicyMode::Engine,
            TurnLoopMode::Agent,
            false,
            "mcp_server_evil",
            &serde_json::json!({}),
            None,
        );
        assert!(meta.approval_required);
        assert!(!meta.supports_parallel);
        assert!(!meta.read_only);
    }

    #[test]
    fn first_party_mcp_discovery_matches_legacy_in_engine_mode() {
        let meta = resolve_tool_plan_approval_meta(
            ToolsPolicyMode::Engine,
            TurnLoopMode::Agent,
            false,
            "list_mcp_resources",
            &serde_json::json!({}),
            None,
        );
        assert!(!meta.approval_required);
        assert!(meta.read_only);
    }

    #[test]
    fn shadow_mode_records_comparison_without_changing_legacy() {
        let before = policy_shadow_stats();
        let meta = resolve_tool_plan_approval_meta(
            ToolsPolicyMode::Shadow,
            TurnLoopMode::Agent,
            false,
            "unknown_tool_xyz",
            &serde_json::json!({}),
            None,
        );
        assert!(!meta.approval_required);
        assert!(!meta.read_only);
        assert!(!meta.supports_parallel);
        let after = policy_shadow_stats();
        assert!(after.comparisons > before.comparisons);
    }
}
