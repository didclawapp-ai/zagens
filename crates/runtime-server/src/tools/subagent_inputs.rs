//! schemars input types for the sub-agent tool family (kernel-v2 M2).

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::tools::tool_schema::derived_input_schema;

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
struct AgentSpawnInput {
    #[schemars(description = "Task description for the sub-agent")]
    pub prompt: Option<String>,
    #[schemars(description = "Alias for prompt")]
    pub message: Option<String>,
    #[schemars(description = "Alias for prompt")]
    pub objective: Option<String>,
    #[schemars(description = "Structured input items (text, mention, skill, local_image, image)")]
    pub items: Option<Vec<Value>>,
    #[serde(rename = "type")]
    #[schemars(
        description = "Sub-agent type: general, explore, plan, review, implementer, verifier, custom. See docs/SUBAGENTS.md for posture per role."
    )]
    pub spawn_type: Option<String>,
    #[schemars(description = "Alias for type")]
    pub agent_type: Option<String>,
    #[schemars(description = "Alias for type")]
    pub agent_name: Option<String>,
    #[schemars(description = "Role alias: worker, explorer, awaiter, default")]
    pub role: Option<String>,
    #[schemars(description = "Alias for role")]
    pub agent_role: Option<String>,
    #[schemars(
        description = "Explicit tool allowlist (required for custom type). Default behavior is full registry inheritance from the parent."
    )]
    pub allowed_tools: Option<Vec<String>>,
    #[schemars(
        description = "Optional DeepSeek model id for this child. Explicit model wins over role/type defaults; omit to inherit."
    )]
    pub model: Option<String>,
    #[schemars(
        description = "Optional working directory for the child. Must be inside the parent's workspace (use a relative path or an absolute path under the workspace root). Used for the parallel-worktree pattern: parent runs `git worktree add .worktrees/feature-x ...` then spawns the child with `cwd: \".worktrees/feature-x\"`."
    )]
    pub cwd: Option<String>,
    #[schemars(
        description = "Optional file path for cache-aware resident mode. When set, the child's system prefix is augmented with the full contents of this file so DeepSeek's prefix cache stays warm across follow-up send_input calls. Only one agent may hold a resident lease on a given file at a time — a second spawn with the same path receives a conflict warning in the result."
    )]
    pub resident_file: Option<String>,
    #[schemars(
        description = "Optional CRAFT work-package id (blackboard filename under `.deepseek/blackboards/{task_id}.json`). Same string may equal audit `run_id`. This is NOT `task_create` / TaskManager — do not call `task_create` just to set this field."
    )]
    pub task_id: Option<String>,
    #[schemars(
        description = "Optional short display label in the agent panel (e.g. BE-Services). When omitted, runtime derives a label from area_id / audit task title / task_id / cwd."
    )]
    pub nickname: Option<String>,
    #[schemars(description = "Alias for nickname")]
    pub display_name: Option<String>,
    #[schemars(
        description = "Optional audit scratchpad run_id. For type=auditor, runtime builds track A from verified notes and track B from prompt (prose draft). Defaults to the active thread scratchpad_run_id when set."
    )]
    pub scratchpad_run_id: Option<String>,
    #[schemars(
        extend("minimum" = 120000, "maximum" = 1800000),
        description = "Per-step LLM API timeout in ms. Omitted → [subagents] step_timeout_secs from config.toml / Zagens settings (default 600000). Range 120000–1800000. Full-repo audit: set per inventory file count (audit-repo skill tier table). On step timeout the child fails — parent must re-spawn or shrink scope, not treat as done."
    )]
    pub step_timeout_ms: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
struct DelegateToAgentInput {
    #[schemars(
        description = "Name/type alias for the agent (general, explore, plan, review, implementer, verifier, worker, explorer, awaiter, builder, validator, tester)"
    )]
    pub agent_name: Option<String>,
    #[serde(rename = "type")]
    #[schemars(description = "Alias for agent_name")]
    pub delegate_type: Option<String>,
    #[schemars(description = "Alias for agent_name")]
    pub agent_type: Option<String>,
    #[schemars(description = "Role alias: worker, explorer, awaiter, default")]
    pub role: Option<String>,
    #[schemars(description = "Alias for role")]
    pub agent_role: Option<String>,
    #[schemars(description = "The goal or task description for the agent")]
    pub objective: Option<String>,
    #[schemars(description = "Alias for objective")]
    pub prompt: Option<String>,
    #[schemars(description = "Alias for objective")]
    pub message: Option<String>,
    #[schemars(description = "Structured input items (text, mention, skill, local_image, image)")]
    pub items: Option<Vec<Value>>,
    #[schemars(description = "Explicit tool allowlist (required for custom type)")]
    pub allowed_tools: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
struct AgentWaitInput {
    #[schemars(
        description = "Agent IDs to wait on. When omitted, waits on all currently running sub-agents."
    )]
    pub ids: Option<Vec<String>>,
    #[schemars(description = "Alias for ids")]
    pub agent_ids: Option<Vec<String>>,
    #[schemars(description = "Single agent ID")]
    pub agent_id: Option<String>,
    #[schemars(description = "Alias for agent_id")]
    pub id: Option<String>,
    #[schemars(description = "Wait behavior: any (default) or all")]
    pub wait_mode: Option<String>,
    #[schemars(
        description = "Max wait time in milliseconds. When omitted, defaults adaptively from the agent's step_timeout_ms and remaining steps (clamped 10000-3600000). Explicit values override."
    )]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
struct AgentResultInput {
    #[schemars(description = "ID returned by agent_spawn")]
    pub agent_id: Option<String>,
    #[schemars(description = "Alias for agent_id")]
    pub id: Option<String>,
    #[schemars(description = "Wait for completion (default: false)")]
    pub block: Option<bool>,
    #[schemars(
        description = "Max wait time in milliseconds when block=true. When omitted, defaults adaptively from the agent's step_timeout_ms and remaining steps (clamped 1000-3600000). Explicit values override."
    )]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
struct AgentListInput {
    #[schemars(
        description = "When true, include agents from prior sessions in the listing. Default false."
    )]
    pub include_archived: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
struct AgentCancelInput {
    #[schemars(description = "ID returned by agent_spawn")]
    pub agent_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
struct AgentCloseInput {
    #[schemars(description = "Agent id returned by agent_spawn")]
    pub id: Option<String>,
    #[schemars(description = "Alias for id")]
    pub agent_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
struct AgentResumeInput {
    #[schemars(description = "Agent id to resume")]
    pub id: Option<String>,
    #[schemars(description = "Alias for id")]
    pub agent_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
struct AgentAssignInput {
    #[schemars(description = "Agent id returned by agent_spawn")]
    pub agent_id: Option<String>,
    #[schemars(description = "Alias for agent_id")]
    pub id: Option<String>,
    #[schemars(description = "Updated assignment objective")]
    pub objective: Option<String>,
    #[schemars(description = "Updated role alias: worker, explorer, awaiter, default")]
    pub role: Option<String>,
    #[schemars(description = "Alias for role")]
    pub agent_role: Option<String>,
    #[schemars(description = "Optional coordinator note to send to the agent")]
    pub message: Option<String>,
    #[schemars(description = "Alias for message")]
    pub input: Option<String>,
    #[schemars(description = "Structured input items (text, mention, skill, local_image, image)")]
    pub items: Option<Vec<Value>>,
    #[schemars(
        description = "Prioritize this assignment update in the agent inbox (default: true)"
    )]
    pub interrupt: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
struct AgentSendInputInput {
    #[schemars(description = "ID returned by agent_spawn")]
    pub agent_id: Option<String>,
    #[schemars(description = "Alias for agent_id")]
    pub id: Option<String>,
    #[schemars(description = "Message to deliver to the agent")]
    pub message: Option<String>,
    #[schemars(description = "Alias for message")]
    pub input: Option<String>,
    #[schemars(description = "Structured input items (text, mention, skill, local_image, image)")]
    pub items: Option<Vec<Value>>,
    #[schemars(description = "Prioritize this message over pending inputs")]
    pub interrupt: Option<bool>,
}

fn ensure_bare_object_array_items(schema: &mut Value, prop_name: &str) {
    let Some(prop) = schema
        .get_mut("properties")
        .and_then(|v| v.get_mut(prop_name))
    else {
        return;
    };
    if let Some(items) = prop.get_mut("items") {
        *items = json!({ "type": "object" });
    }
}

#[must_use]
pub fn agent_spawn_input_schema() -> Value {
    let mut schema = derived_input_schema::<AgentSpawnInput>();
    ensure_bare_object_array_items(&mut schema, "items");
    schema
}

#[must_use]
pub fn delegate_to_agent_input_schema() -> Value {
    let mut schema = derived_input_schema::<DelegateToAgentInput>();
    ensure_bare_object_array_items(&mut schema, "items");
    schema
}

#[must_use]
pub fn agent_wait_input_schema() -> Value {
    derived_input_schema::<AgentWaitInput>()
}

#[must_use]
pub fn agent_result_input_schema() -> Value {
    derived_input_schema::<AgentResultInput>()
}

#[must_use]
pub fn agent_list_input_schema() -> Value {
    derived_input_schema::<AgentListInput>()
}

#[must_use]
pub fn agent_cancel_input_schema() -> Value {
    derived_input_schema::<AgentCancelInput>()
}

#[must_use]
pub fn agent_close_input_schema() -> Value {
    derived_input_schema::<AgentCloseInput>()
}

#[must_use]
pub fn agent_resume_input_schema() -> Value {
    derived_input_schema::<AgentResumeInput>()
}

#[must_use]
pub fn agent_assign_input_schema() -> Value {
    let mut schema = derived_input_schema::<AgentAssignInput>();
    ensure_bare_object_array_items(&mut schema, "items");
    schema
}

#[must_use]
pub fn agent_send_input_input_schema() -> Value {
    let mut schema = derived_input_schema::<AgentSendInputInput>();
    ensure_bare_object_array_items(&mut schema, "items");
    schema
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::schema_sanitize;
    use crate::tools::spec::ToolSpec;
    use crate::tools::subagent::{
        AgentAssignTool, AgentCancelTool, AgentCloseTool, AgentListTool, AgentResultTool,
        AgentResumeTool, AgentSendInputTool, AgentSpawnTool, AgentWaitTool, DelegateToAgentTool,
        SubAgentRuntime, new_shared_subagent_manager,
    };
    use std::sync::Arc;
    use std::time::Duration;
    use tokio_util::sync::CancellationToken;

    fn model_visible_input_schema(tool: &dyn ToolSpec) -> Value {
        let mut schema = tool.input_schema();
        schema_sanitize::sanitize(&mut schema);
        schema
    }

    fn test_subagent_manager() -> crate::tools::subagent::SharedSubAgentManager {
        new_shared_subagent_manager(std::env::temp_dir(), 1, Duration::from_secs(60))
    }

    fn stub_runtime() -> SubAgentRuntime {
        use crate::client::DeepSeekClient;
        use crate::tools::spec::ToolContext;
        use crate::tools::subagent::{DEFAULT_MAX_SPAWN_DEPTH, STEP_API_TIMEOUT};

        let workspace = std::env::temp_dir().join("zagens-subagent-schema-test");
        let context = ToolContext::new(workspace.clone());
        let config = crate::config::Config {
            api_key: Some("test-key".to_string()),
            ..crate::config::Config::default()
        };
        SubAgentRuntime {
            client: Arc::new(DeepSeekClient::new(&config).expect("stub client")),
            model: "deepseek-v4-flash".to_string(),
            auto_model: false,
            reasoning_effort: None,
            reasoning_effort_auto: false,
            role_models: std::collections::HashMap::new(),
            context,
            allow_shell: true,
            event_tx: None,
            manager: test_subagent_manager(),
            spawn_depth: 0,
            max_spawn_depth: DEFAULT_MAX_SPAWN_DEPTH,
            cancel_token: CancellationToken::new(),
            mailbox: None,
            parent_completion_tx: None,
            step_timeout: STEP_API_TIMEOUT,
            hook_executor: None,
        }
    }

    const SUBAGENT_SCHEMA_SNAPSHOT_DIR: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/harness/kernel-v2-schema-snapshots"
    );

    #[test]
    #[ignore = "bootstrap kernel-v2 subagent-tool schema snapshot fixtures"]
    fn dump_subagent_tool_schemas_for_snapshot_bootstrap() {
        let manager = test_subagent_manager();
        let runtime = stub_runtime();
        let cases: [(&str, Box<dyn ToolSpec>); 14] = [
            (
                "agent_spawn",
                Box::new(AgentSpawnTool::new(manager.clone(), runtime.clone())),
            ),
            (
                "spawn_agent",
                Box::new(AgentSpawnTool::with_name(
                    manager.clone(),
                    runtime.clone(),
                    "spawn_agent",
                )),
            ),
            (
                "delegate_to_agent",
                Box::new(DelegateToAgentTool::new(manager.clone(), runtime.clone())),
            ),
            (
                "agent_result",
                Box::new(AgentResultTool::new(manager.clone())),
            ),
            (
                "send_input",
                Box::new(AgentSendInputTool::new(manager.clone(), "send_input")),
            ),
            (
                "agent_assign",
                Box::new(AgentAssignTool::new(manager.clone(), "agent_assign")),
            ),
            (
                "assign_agent",
                Box::new(AgentAssignTool::new(manager.clone(), "assign_agent")),
            ),
            (
                "wait",
                Box::new(AgentWaitTool::new(manager.clone(), "wait")),
            ),
            (
                "agent_send_input",
                Box::new(AgentSendInputTool::new(manager.clone(), "agent_send_input")),
            ),
            (
                "agent_wait",
                Box::new(AgentWaitTool::new(manager.clone(), "agent_wait")),
            ),
            (
                "resume_agent",
                Box::new(AgentResumeTool::new(manager.clone(), runtime.clone())),
            ),
            (
                "close_agent",
                Box::new(AgentCloseTool::new(manager.clone())),
            ),
            (
                "agent_cancel",
                Box::new(AgentCancelTool::new(manager.clone())),
            ),
            ("agent_list", Box::new(AgentListTool::new(manager))),
        ];
        for (name, tool) in cases {
            let schema = model_visible_input_schema(tool.as_ref());
            let pretty = serde_json::to_string_pretty(&schema).expect("serialize");
            println!("=== {name} ===\n{pretty}\n");
        }
    }

    #[test]
    fn subagent_tool_model_visible_schemas_match_snapshots() {
        let manager = test_subagent_manager();
        let runtime = stub_runtime();
        let cases: [(&str, Box<dyn ToolSpec>, &str); 14] = [
            (
                "agent_spawn",
                Box::new(AgentSpawnTool::new(manager.clone(), runtime.clone())),
                "agent_spawn",
            ),
            (
                "spawn_agent",
                Box::new(AgentSpawnTool::with_name(
                    manager.clone(),
                    runtime.clone(),
                    "spawn_agent",
                )),
                "agent_spawn",
            ),
            (
                "delegate_to_agent",
                Box::new(DelegateToAgentTool::new(manager.clone(), runtime.clone())),
                "delegate_to_agent",
            ),
            (
                "agent_result",
                Box::new(AgentResultTool::new(manager.clone())),
                "agent_result",
            ),
            (
                "send_input",
                Box::new(AgentSendInputTool::new(manager.clone(), "send_input")),
                "agent_send_input",
            ),
            (
                "agent_assign",
                Box::new(AgentAssignTool::new(manager.clone(), "agent_assign")),
                "agent_assign",
            ),
            (
                "assign_agent",
                Box::new(AgentAssignTool::new(manager.clone(), "assign_agent")),
                "agent_assign",
            ),
            (
                "wait",
                Box::new(AgentWaitTool::new(manager.clone(), "wait")),
                "agent_wait",
            ),
            (
                "agent_send_input",
                Box::new(AgentSendInputTool::new(manager.clone(), "agent_send_input")),
                "agent_send_input",
            ),
            (
                "agent_wait",
                Box::new(AgentWaitTool::new(manager.clone(), "agent_wait")),
                "agent_wait",
            ),
            (
                "resume_agent",
                Box::new(AgentResumeTool::new(manager.clone(), runtime.clone())),
                "resume_agent",
            ),
            (
                "close_agent",
                Box::new(AgentCloseTool::new(manager.clone())),
                "close_agent",
            ),
            (
                "agent_cancel",
                Box::new(AgentCancelTool::new(manager.clone())),
                "agent_cancel",
            ),
            (
                "agent_list",
                Box::new(AgentListTool::new(manager)),
                "agent_list",
            ),
        ];
        for (tool_name, tool, snapshot_name) in cases {
            assert_eq!(tool.name(), tool_name);
            let schema = model_visible_input_schema(tool.as_ref());
            let path = format!("{SUBAGENT_SCHEMA_SNAPSHOT_DIR}/subagent-{snapshot_name}.json");
            let expected: Value = serde_json::from_str(
                &std::fs::read_to_string(&path)
                    .unwrap_or_else(|e| panic!("missing snapshot {path}: {e}")),
            )
            .expect("parse snapshot JSON");
            assert_eq!(
                schema, expected,
                "model-visible schema drift for {tool_name} (snapshot {snapshot_name}) — update fixture only after explicit KV-cache review"
            );
        }
    }
}
