//! Turn-loop hooks for skill stage gate (Phase 2a.2).

use zagens_core::engine::turn_machine::emit_kernel_event;
use zagens_tools::ToolError;

use super::Engine;
use crate::long_horizon::predicate::CompletionGateExec;
use crate::long_horizon::stage_gate::{
    StageGateBlocked, blocked_to_kernel_event, manifest_path_for_skill,
};
use crate::skills::discover_in_workspace;

pub fn activate_stage_gate_from_config(engine: &mut Engine) {
    let manifest = engine.config.long_horizon.stage_gate.manifest.clone();
    let enforce = engine.config.long_horizon.stage_gate.enforce;
    let Some(rel) = manifest else {
        return;
    };
    let path = engine.config.workspace.join(rel);
    let _ = engine
        .runtime_ext_mut()
        .long_horizon_state
        .stage_gate
        .load_manifest_file(&path, enforce);
}

pub fn maybe_block_tool(engine: &mut Engine, tool_name: &str) -> Option<ToolError> {
    let blocked = engine
        .runtime_ext()
        .long_horizon_state
        .stage_gate
        .check_tool(tool_name)
        .err()?;
    emit_stage_gate_blocked(engine, &blocked);
    Some(ToolError::execution_failed(blocked.message()))
}

pub fn emit_stage_gate_blocked(engine: &mut Engine, blocked: &StageGateBlocked) {
    let turn_id = engine
        .runtime_ext()
        .kernel_active_turn_id
        .clone()
        .unwrap_or_else(|| "unknown".into());
    let step_idx = engine.runtime_ext().kernel_active_step;
    emit_kernel_event(engine, blocked_to_kernel_event(turn_id, step_idx, blocked));
}

pub fn activate_stage_gate_for_skill(engine: &mut Engine, skill_name: &str) {
    let registry = discover_in_workspace(&engine.config.workspace);
    let Some(skill) = registry.get(skill_name) else {
        return;
    };
    let manifest = manifest_path_for_skill(&skill.path);
    if !manifest.exists() {
        return;
    }
    let _ = engine
        .runtime_ext_mut()
        .long_horizon_state
        .stage_gate
        .load_manifest_file(&manifest, true);
}

pub async fn after_tool_success(engine: &mut Engine, tool_name: &str, success: bool) {
    if !success {
        return;
    }
    if !engine
        .runtime_ext()
        .long_horizon_state
        .stage_gate
        .is_active()
    {
        return;
    }
    let stage_id = match engine
        .runtime_ext()
        .long_horizon_state
        .stage_gate
        .current_stage_id()
    {
        Some(id) => id,
        None => return,
    };
    let is_stage_tool = engine
        .runtime_ext()
        .long_horizon_state
        .stage_gate
        .contract
        .as_ref()
        .and_then(|c| c.stage_by_id(&stage_id))
        .is_some_and(|s| s.tools.iter().any(|t| t == tool_name));
    if !is_stage_tool {
        return;
    }
    let workspace = engine.config.workspace.clone();
    let shell_manager = engine.runtime_ext().shell_manager.clone();
    let cancel = engine.cancel_token.clone();
    let exec = CompletionGateExec {
        shell_manager: &shell_manager,
        cancel_token: Some(&cancel),
    };
    let _ = engine
        .runtime_ext_mut()
        .long_horizon_state
        .stage_gate
        .try_pass_stage(&workspace, &stage_id, Some(&exec))
        .await;
}

pub async fn after_harness_assert_tool(
    engine: &mut Engine,
    tool_name: &str,
    tool_input: &serde_json::Value,
    success: bool,
) {
    if !success || !tool_name.starts_with("assert_") {
        return;
    }
    let Some(stage) = tool_input
        .get("stage")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    else {
        return;
    };
    let workspace = engine.config.workspace.clone();
    let shell_manager = engine.runtime_ext().shell_manager.clone();
    let cancel = engine.cancel_token.clone();
    let exec = CompletionGateExec {
        shell_manager: &shell_manager,
        cancel_token: Some(&cancel),
    };
    let _ = engine
        .runtime_ext_mut()
        .long_horizon_state
        .stage_gate
        .try_pass_stage(&workspace, stage, Some(&exec))
        .await;
}
