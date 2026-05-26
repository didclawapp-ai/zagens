use std::time::Duration;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{Value, json};

use deepseek_core::subagent::{SubAgentAssignment, SubAgentResult, SubAgentStatus, SubAgentType};
use crate::tools::spec::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec,
    optional_bool, optional_u64, required_str,
};

use super::super::constants::*;
use super::super::deprecation::wrap_with_deprecation_notice;
use super::super::factory::SharedSubAgentManager;
use super::super::parse::parse_spawn_request;
use super::super::registry::summarize_subagent_result;
use super::super::parse::configured_model_for_role_or_type;
use super::super::resident::{release_resident_file_lease, try_claim_resident_file_lease, upgrade_pending_resident_lease};
use super::super::router::resolve_subagent_assignment_route;
use super::super::runtime::SubAgentRuntime;
use super::super::types::SubAgentSpawnOptions;
use super::super::constants::whale_nickname_for_index;

/// Tool to spawn a background sub-agent.
pub struct AgentSpawnTool {
    manager: SharedSubAgentManager,
    runtime: SubAgentRuntime,
    name: &'static str,
}

impl AgentSpawnTool {
    /// Create a new spawn tool.
    #[must_use]
    pub fn new(manager: SharedSubAgentManager, runtime: SubAgentRuntime) -> Self {
        Self::with_name(manager, runtime, "agent_spawn")
    }

    /// Create a new spawn tool with a custom tool name alias.
    #[must_use]
    pub fn with_name(
        manager: SharedSubAgentManager,
        runtime: SubAgentRuntime,
        name: &'static str,
    ) -> Self {
        Self {
            manager,
            runtime,
            name,
        }
    }
}

#[async_trait]
impl ToolSpec for AgentSpawnTool {
    fn name(&self) -> &'static str {
        self.name
    }

    fn description(&self) -> &'static str {
        "Dispatch a **sub-agent** (child of the current agent): hierarchical, tools narrowed by \
         `type`, parent may receive `<deepseek:subagent.done>`. Returns `agent_id` immediately; \
         join with `agent_result` / `agent_wait` / `agent_list`. **Not** `task_create` — Tasks are \
         peer background jobs (TaskManager) and require `task_read`. Optional `task_id` is only a \
         CRAFT **work-package / blackboard key** (e.g. scratchpad `run_id`), not a TaskManager id. \
         Cap: `[subagents].max_concurrent` (default 10). Omitting `step_timeout_ms` uses \
         `[subagents] step_timeout_secs` from config (Zagens system settings); for audit/review \
         workloads prefer 240000–360000 ms or raise the config default — do not assume the child \
         can run many minutes unless you set it. For parallel read-only tool calls in *this* turn, \
         batch tools instead of spawning."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "prompt": {
                    "type": "string",
                    "description": "Task description for the sub-agent"
                },
                "message": {
                    "type": "string",
                    "description": "Alias for prompt"
                },
                "objective": {
                    "type": "string",
                    "description": "Alias for prompt"
                },
                "items": {
                    "type": "array",
                    "description": "Structured input items (text, mention, skill, local_image, image)",
                    "items": {
                        "type": "object"
                    }
                },
                "type": {
                    "type": "string",
                    "description": "Sub-agent type: general, explore, plan, review, implementer, verifier, custom. See docs/SUBAGENTS.md for posture per role."
                },
                "agent_type": {
                    "type": "string",
                    "description": "Alias for type"
                },
                "agent_name": {
                    "type": "string",
                    "description": "Alias for type"
                },
                "role": {
                    "type": "string",
                    "description": "Role alias: worker, explorer, awaiter, default"
                },
                "agent_role": {
                    "type": "string",
                    "description": "Alias for role"
                },
                "allowed_tools": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Explicit tool allowlist (required for custom type). Default behavior is full registry inheritance from the parent."
                },
                "model": {
                    "type": "string",
                    "description": "Optional DeepSeek model id for this child. Explicit model wins over role/type defaults; omit to inherit."
                },
                "cwd": {
                    "type": "string",
                    "description": "Optional working directory for the child. Must be inside the parent's workspace (use a relative path or an absolute path under the workspace root). Used for the parallel-worktree pattern: parent runs `git worktree add .worktrees/feature-x ...` then spawns the child with `cwd: \".worktrees/feature-x\"`."
                },
                "resident_file": {
                    "type": "string",
                    "description": "Optional file path for cache-aware resident mode. When set, the child's system prefix is augmented with the full contents of this file so DeepSeek's prefix cache stays warm across follow-up send_input calls. Only one agent may hold a resident lease on a given file at a time — a second spawn with the same path receives a conflict warning in the result."
                },
                "task_id": {
                    "type": "string",
                    "description": "Optional CRAFT work-package id (blackboard filename under `.deepseek/blackboards/{task_id}.json`). Same string may equal audit `run_id`. This is NOT `task_create` / TaskManager — do not call `task_create` just to set this field."
                },
                "scratchpad_run_id": {
                    "type": "string",
                    "description": "Optional audit scratchpad run_id. For type=auditor, runtime builds track A from verified notes and track B from prompt (prose draft). Defaults to the active thread scratchpad_run_id when set."
                },
                "step_timeout_ms": {
                    "type": "integer",
                    "description": "Per-step LLM API timeout in ms. Omitted → [subagents] step_timeout_secs from config.toml / Zagens settings (else 120000). Full-repo audit: use 240000–360000 unless config default is already high. On step timeout the child fails — parent must re-spawn or shrink scope, not treat as done.",
                    "minimum": 10000,
                    "maximum": 600000
                }
            }
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![
            ToolCapability::ExecutesCode,
            ToolCapability::RequiresApproval,
        ]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Required
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let spawn_request = parse_spawn_request(&input)?;

        // Depth cap: reject before locking the manager so we don't introduce
        // unnecessary contention. Mirrors codex's pattern (allow-equal at the
        // boundary; reject when `next > max`).
        if self.runtime.would_exceed_depth() {
            return Err(ToolError::execution_failed(format!(
                "Sub-agent depth limit reached (current depth {}, max {}). \
                 Increase via [runtime] max_spawn_depth in config.toml.",
                self.runtime.spawn_depth, self.runtime.max_spawn_depth
            )));
        }

        // Validate cwd if supplied: must canonicalize inside the parent
        // workspace. Catches accidents like `cwd: "/etc"`.
        let validated_cwd = if let Some(requested_cwd) = spawn_request.cwd.as_ref() {
            let parent_workspace = &self.runtime.context.workspace;
            let resolved = if requested_cwd.is_absolute() {
                requested_cwd.clone()
            } else {
                parent_workspace.join(requested_cwd)
            };
            let canonical = resolved.canonicalize().map_err(|e| {
                ToolError::invalid_input(format!(
                    "Invalid cwd '{}': {e} (path may not exist yet — create the worktree first)",
                    requested_cwd.display()
                ))
            })?;
            let workspace_canonical = parent_workspace
                .canonicalize()
                .unwrap_or_else(|_| parent_workspace.clone());
            if !canonical.starts_with(&workspace_canonical) {
                return Err(ToolError::invalid_input(format!(
                    "cwd must be inside the parent workspace: {} is not under {}",
                    canonical.display(),
                    workspace_canonical.display()
                )));
            }
            Some(canonical)
        } else {
            None
        };

        // Derive the child's runtime as a durable background job: it keeps
        // its own cancellation token, forces auto_approve, and optionally
        // overrides cwd if the caller passed one (used for the parallel-
        // worktree pattern).
        let mut child_runtime = self.runtime.background_runtime();
        if let Some(cwd) = validated_cwd {
            child_runtime.context.workspace = cwd;
        }
        let default_step_ms = context.subagent_default_step_timeout_ms.clamp(10_000, 600_000);
        let step_timeout_ms =
            optional_u64(&input, "step_timeout_ms", default_step_ms).clamp(10_000, 600_000);
        child_runtime = child_runtime.with_step_timeout(Duration::from_millis(step_timeout_ms));
        let configured_model = match spawn_request.model.clone() {
            Some(model) => Some(model),
            None => configured_model_for_role_or_type(
                &self.runtime,
                spawn_request.assignment.role.as_deref(),
                &spawn_request.agent_type,
            )?,
        };

        // Cache-aware resident mode (#529): prepend file contents to the prompt
        // so the child's prefix is byte-stable for DeepSeek prefix caching.
        let effective_prompt = if let Some(ref file_path) = spawn_request.resident_file {
            try_claim_resident_file_lease(file_path, "pending").map_err(ToolError::execution_failed)?;

            let abs_path = if std::path::Path::new(file_path).is_absolute() {
                std::path::PathBuf::from(file_path)
            } else {
                self.runtime.context.workspace.join(file_path)
            };
            let file_contents = std::fs::read_to_string(&abs_path)
                .unwrap_or_else(|e| format!("<!-- resident_file read error: {e} -->"));
            format!(
                "<!-- resident_file: {file_path} -->\n```\n{file_contents}\n```\n\n{}",
                spawn_request.prompt
            )
        } else {
            spawn_request.prompt
        };

        let mut effective_prompt = effective_prompt;
        if spawn_request.agent_type == SubAgentType::Auditor {
            let scratchpad_cfg = self
                .runtime
                .context
                .runtime
                .scratchpad_config
                .clone()
                .unwrap_or_default();
            let explicit_run = input
                .get("scratchpad_run_id")
                .and_then(|v| v.as_str());
            let slot_run = self
                .runtime
                .context
                .runtime
                .scratchpad_run_id
                .lock()
                .ok()
                .and_then(|g| g.clone());
            if let Some(run_id) =
                crate::scratchpad::resolve_auditor_run_id(explicit_run, slot_run.as_deref())
            {
                if let Some(tid) = spawn_request.task_id.as_deref() {
                    crate::tools::subagent::blackboard::write_scratchpad_mirror(
                        tid,
                        &self.runtime.context.workspace,
                        &run_id,
                        &scratchpad_cfg,
                    );
                }
                if scratchpad_cfg.auditor_from_scratchpad
                    && let Some(section) = crate::scratchpad::build_auditor_assignment_sections(
                        &self.runtime.context.workspace,
                        &run_id,
                        &effective_prompt,
                        &scratchpad_cfg,
                    )
                {
                    effective_prompt = format!("{section}\n\n---\n\n{effective_prompt}");
                }
            }
        }

        let route =
            resolve_subagent_assignment_route(&self.runtime, configured_model, &effective_prompt)
                .await;
        child_runtime.model = route.model.clone();
        child_runtime.reasoning_effort = route.reasoning_effort.clone();
        child_runtime.reasoning_effort_auto = false;
        let effective_model = route.model;

        let mut manager = self.manager.write().await;

        let spawn_result = manager
            .spawn_background_with_assignment_options(
                Arc::clone(&self.manager),
                child_runtime,
                spawn_request.agent_type,
                effective_prompt,
                spawn_request.assignment,
                spawn_request.allowed_tools,
                SubAgentSpawnOptions {
                    model: Some(effective_model),
                    nickname: None,
                    task_id: spawn_request.task_id.clone(),
                },
            );
        if spawn_result.is_err()
            && let Some(ref file_path) = spawn_request.resident_file
        {
            release_resident_file_lease(file_path);
        }
        let result = spawn_result
            .map_err(|e| ToolError::execution_failed(format!("Failed to spawn sub-agent: {e}")))?;

        // Replace the "pending" lease placeholder with the real agent id now that
        // the manager has assigned one. Without this, `release_resident_leases_for`
        // (which matches by agent id at terminal-state transitions) can never find
        // the entry — leases would stay stamped as "pending" forever, defeating the
        // release machinery added in #660.
        if let Some(ref file_path) = spawn_request.resident_file {
            upgrade_pending_resident_lease(file_path, &result.agent_id);
        }

        let mut tool_result = if self.name == "spawn_agent" {
            let payload = json!({
                "agent_id": result.agent_id.clone(),
                "nickname": result.nickname.clone(),
                "model": result.model.clone()
            });
            ToolResult::json(&payload).map_err(|e| ToolError::execution_failed(e.to_string()))?
        } else {
            ToolResult::json(&result).map_err(|e| ToolError::execution_failed(e.to_string()))?
        };
        if result.status == SubAgentStatus::Running {
            if self.name == "spawn_agent" {
                tool_result.metadata = Some(json!({
                    "status": "Running",
                    "snapshot": result
                }));
            } else {
                tool_result.metadata = Some(json!({ "status": "Running" }));
            }
        }
        // Annotate alias invocations with a deprecation notice so the model
        // can migrate to the canonical name before removal in v0.8.0.
        if self.name == "spawn_agent" {
            tool_result = wrap_with_deprecation_notice(tool_result, "spawn_agent", "agent_spawn");
        }
        Ok(tool_result)
    }
}


