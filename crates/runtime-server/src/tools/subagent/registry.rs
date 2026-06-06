use anyhow::{Result, anyhow};
use serde_json::Value;

use crate::models::Tool;
use crate::tools::plan::SharedPlanState;
use crate::tools::registry::{ToolRegistry, ToolRegistryBuilder};
use crate::tools::todo::SharedTodoList;

use deepseek_core::subagent::{SubAgentResult, SubAgentStatus, SubAgentType};

use super::runtime::SubAgentRuntime;

// === Tool Registry Helpers ===

/// Per-sub-agent tool registry.
///
/// Two modes:
/// - **Full inheritance** (`allowed_tools = None`): the child sees the same
///   tool surface as the parent's Agent mode — every tool family including
///   `with_subagent_tools` (so it can recurse). This is the v0.6.6 default.
/// - **Explicit narrow** (`allowed_tools = Some(list)`): legacy / Custom
///   path. The registry still builds the full surface, but only the listed
///   tool names are visible to the model and callable.
pub(crate) struct SubAgentToolRegistry {
    /// `None` → full inheritance (no filter applied). `Some(list)` →
    /// only the listed tools are visible to the model and callable.
    allowed_tools: Option<Vec<String>>,
    registry: ToolRegistry,
}

impl SubAgentToolRegistry {
    pub(crate) fn new(
        runtime: SubAgentRuntime,
        explicit_allowed_tools: Option<Vec<String>>,
        todo_list: SharedTodoList,
        plan_state: SharedPlanState,
    ) -> Self {
        // Build the full agent surface — same as the parent's Agent mode.
        // Children inherit shell, file, patch, search, web, git, diagnostics,
        // review, RLM, sub-agent management (so grandchildren can spawn),
        // plus per-child fresh todo/plan state.
        let context = runtime.context.clone();
        let registry = ToolRegistryBuilder::new()
            .with_full_agent_surface(
                Some(runtime.client.clone()),
                runtime.model.clone(),
                runtime.manager.clone(),
                runtime.clone(),
                runtime.allow_shell,
                todo_list,
                plan_state,
            )
            .build(context);

        Self {
            allowed_tools: explicit_allowed_tools,
            registry,
        }
    }

    /// Whether a given tool name is permitted under this child's filter.
    /// `None` filter = everything permitted.
    pub(crate) fn is_tool_allowed(&self, name: &str) -> bool {
        match &self.allowed_tools {
            None => true,
            Some(list) => list.iter().any(|t| t == name),
        }
    }

    pub(crate) fn tools_for_model(&self) -> Vec<Tool> {
        let api_tools = self.registry.to_api_tools();
        match &self.allowed_tools {
            None => api_tools,
            Some(list) => api_tools
                .into_iter()
                .filter(|tool| list.contains(&tool.name))
                .collect(),
        }
    }

    pub(crate) fn unavailable_allowed_tools(&self) -> Vec<String> {
        match &self.allowed_tools {
            None => Vec::new(),
            Some(list) => list
                .iter()
                .filter(|name| !self.registry.contains(name))
                .cloned()
                .collect(),
        }
    }

    pub(crate) async fn execute(
        &self,
        _agent_id: &str,
        name: &str,
        input: Value,
    ) -> Result<String> {
        if !self.is_tool_allowed(name) {
            return Err(anyhow!("Tool {name} not allowed for this sub-agent"));
        }
        self.registry
            .execute(name, input)
            .await
            .map_err(|e| anyhow!(e))
    }
}

/// Resolve the effective allowed-tools list for a child.
///
/// **v0.6.6 default: full inheritance.** Returning `Ok(None)` means the
/// child sees the same tool surface as the parent's Agent mode — every
/// family including `with_subagent_tools` so it can recurse. The narrowing
/// path (`Ok(Some(list))`) is only used by:
/// - `Custom` agent types (which require an explicit list).
/// - Callers that pass `explicit_tools` (advanced / legacy use).
///
/// `allow_shell = false` no longer narrows the tool LIST — the child's
/// registry simply doesn't register shell tools, which has the same
/// effect without papering over the parent's choice with a deny-list.
pub(crate) fn build_allowed_tools(
    agent_type: &SubAgentType,
    explicit_tools: Option<Vec<String>>,
    _allow_shell: bool,
) -> Result<Option<Vec<String>>> {
    if let Some(tools) = explicit_tools {
        let mut deduped = Vec::new();
        for tool in tools {
            let name = tool.trim();
            if !name.is_empty() && !deduped.iter().any(|existing: &String| existing == name) {
                deduped.push(name.to_string());
            }
        }
        if matches!(agent_type, SubAgentType::Custom) && deduped.is_empty() {
            return Err(anyhow!(
                "Custom sub-agent requires a non-empty allowed_tools list"
            ));
        }
        let narrowed = match agent_type {
            SubAgentType::Explore | SubAgentType::Review => {
                let cap = read_only_tool_cap(agent_type);
                deduped
                    .into_iter()
                    .filter(|t| cap.iter().any(|c| *c == t.as_str()))
                    .collect::<Vec<_>>()
            }
            _ => deduped,
        };
        if matches!(agent_type, SubAgentType::Custom) && narrowed.is_empty() {
            return Err(anyhow!(
                "Custom sub-agent requires a non-empty allowed_tools list"
            ));
        }
        return Ok(Some(narrowed));
    }

    if matches!(agent_type, SubAgentType::Custom) {
        return Err(anyhow!(
            "Custom sub-agent requires a non-empty allowed_tools list"
        ));
    }

    // CRAFT P3: hard tool clipping for read-only roles.
    // Explore and Review return explicit narrow lists — no write files,
    // no shell. Other types keep full inheritance.
    match agent_type {
        SubAgentType::Explore | SubAgentType::Review => Ok(Some(
            read_only_tool_cap(agent_type)
                .iter()
                .map(|s| (*s).to_string())
                .collect(),
        )),
        _ => Ok(None),
    }
}

pub(crate) fn read_only_tool_cap(agent_type: &SubAgentType) -> &'static [&'static str] {
    match agent_type {
        SubAgentType::Explore => &[
            "list_dir",
            "read_file",
            "grep_files",
            "glob_files",
            "file_search",
            "web.run",
            "web_search",
            "note",
        ],
        SubAgentType::Review => &[
            "list_dir",
            "read_file",
            "grep_files",
            "glob_files",
            "file_search",
            "exec_shell",
            "note",
        ],
        _ => &[],
    }
}

pub(crate) fn summarize_subagent_result(result: &SubAgentResult) -> String {
    match (&result.status, result.result.as_ref()) {
        (SubAgentStatus::Completed, Some(text)) => truncate_preview(text),
        (SubAgentStatus::Completed, None) => "Completed (no output)".to_string(),
        (SubAgentStatus::Interrupted(error), _) => format!("Interrupted: {error}"),
        (SubAgentStatus::Cancelled, _) => "Cancelled".to_string(),
        (SubAgentStatus::Failed(error), _) => format!("Failed: {error}"),
        (SubAgentStatus::Running, _) => "Running".to_string(),
    }
}

pub(crate) fn subagent_status_name(status: &SubAgentStatus) -> &'static str {
    match status {
        SubAgentStatus::Running => "running",
        SubAgentStatus::Completed => "completed",
        SubAgentStatus::Interrupted(_) => "interrupted",
        SubAgentStatus::Failed(_) => "failed",
        SubAgentStatus::Cancelled => "cancelled",
    }
}

pub(crate) fn truncate_preview(text: &str) -> String {
    const MAX_LEN: usize = 240;
    if text.len() <= MAX_LEN {
        text.to_string()
    } else {
        format!("{}...", text.chars().take(MAX_LEN).collect::<String>())
    }
}
