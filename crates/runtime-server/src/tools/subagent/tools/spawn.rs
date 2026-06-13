use std::sync::Arc;
use std::time::Duration;

use crate::tools::subagent_inputs::agent_spawn_input_schema;
use anyhow::Result;
use async_trait::async_trait;
use serde_json::{Value, json};

use crate::config::{MAX_SUBAGENT_STEP_TIMEOUT_SECS, MIN_SUBAGENT_STEP_TIMEOUT_SECS};
use crate::tools::spec::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec, optional_u64,
};
use zagens_core::subagent::{SubAgentStatus, SubAgentType};

use super::super::deprecation::wrap_with_deprecation_notice;
use super::super::factory::SharedSubAgentManager;
use super::super::parse::configured_model_for_role_or_type;
use super::super::parse::parse_spawn_request;
use super::super::resident::{
    release_resident_file_lease, try_claim_resident_file_lease, upgrade_pending_resident_lease,
};
use super::super::router::resolve_subagent_assignment_route;
use super::super::runtime::SubAgentRuntime;
use super::super::types::SubAgentSpawnOptions;

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
         `[subagents] step_timeout_secs` from config (Zagens system settings, default 600 s); for \
         audit/review workloads set explicit `step_timeout_ms` per audit-repo inventory tier \
         (600000–1800000) or raise the config default — do not assume unlimited time. For parallel \
         read-only tool calls in *this* turn, batch tools instead of spawning."
    }

    fn input_schema(&self) -> Value {
        agent_spawn_input_schema()
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
        let cwd_label = validated_cwd.as_ref().and_then(|p| {
            p.file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .filter(|s| !s.trim().is_empty())
        });
        let mut child_runtime = self.runtime.background_runtime();
        if let Some(cwd) = validated_cwd {
            child_runtime.context.workspace = cwd;
        }
        let min_step_ms = MIN_SUBAGENT_STEP_TIMEOUT_SECS * 1000;
        let max_step_ms = MAX_SUBAGENT_STEP_TIMEOUT_SECS * 1000;
        let default_step_ms = context
            .subagent_default_step_timeout_ms
            .clamp(min_step_ms, max_step_ms);
        let step_timeout_ms = optional_u64(&input, "step_timeout_ms", default_step_ms)
            .clamp(min_step_ms, max_step_ms);
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
            try_claim_resident_file_lease(file_path, "pending")
                .map_err(ToolError::execution_failed)?;

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
                .wire
                .scratchpad_config
                .clone()
                .unwrap_or_default();
            let explicit_run = input.get("scratchpad_run_id").and_then(|v| v.as_str());
            let slot_run = self
                .runtime
                .context
                .runtime
                .wire
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

        let explicit_run = input.get("scratchpad_run_id").and_then(|v| v.as_str());
        let scratchpad_run_id =
            crate::scratchpad::try_resolve_run_id(&self.runtime.context, explicit_run);

        let craft_agent_type = spawn_request.agent_type.clone();
        let craft_task_id = spawn_request.task_id.clone();

        let spawn_result = manager.spawn_background_with_assignment_options(
            Arc::clone(&self.manager),
            child_runtime,
            spawn_request.agent_type,
            effective_prompt,
            spawn_request.assignment,
            spawn_request.allowed_tools,
            SubAgentSpawnOptions {
                model: Some(effective_model),
                nickname: spawn_request.nickname.clone(),
                task_id: spawn_request.task_id.clone(),
                scratchpad_run_id,
                cwd_label,
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
        if craft_blackboard_recommends_task_id(&craft_agent_type) && craft_task_id.is_none() {
            let notice = "CRAFT: pass `task_id` on agent_spawn for Explore/Implementer/Review/Verifier \
                          so the blackboard and fix-loop sentinels work; without it fix-loop hints are suppressed.";
            tool_result.metadata = Some(match tool_result.metadata.take() {
                Some(mut meta) => {
                    if let Some(obj) = meta.as_object_mut() {
                        obj.insert("craft_notice".into(), json!(notice));
                    }
                    meta
                }
                None => json!({ "craft_notice": notice }),
            });
        }
        Ok(tool_result)
    }
}

fn craft_blackboard_recommends_task_id(agent_type: &SubAgentType) -> bool {
    matches!(
        agent_type,
        SubAgentType::Explore
            | SubAgentType::Implementer
            | SubAgentType::Review
            | SubAgentType::Verifier
    )
}
