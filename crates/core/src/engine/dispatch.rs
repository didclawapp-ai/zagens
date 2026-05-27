//! Tool-input parsing and batch policy helpers (P2 PR4).
//!
//! Types that tie to the live engine batch driver (`ToolExecutionPlan`, lock
//! guards) remain in `deepseek-runtime` engine dispatch glue.

use serde_json::{Value, json};

use deepseek_tools::{ToolError, ToolResult};

use super::streaming::ToolUseState;
use crate::chat::{Tool, ToolCaller};

/// Parallel-batch eligibility for one planned tool invocation.
#[derive(Debug, Clone, Copy)]
pub struct ToolParallelPlanFlags {
    pub read_only: bool,
    pub supports_parallel: bool,
    pub approval_required: bool,
    pub interactive: bool,
}

/// Promote a streaming `ToolUseState` to a finalized JSON input.
#[must_use]
pub fn final_tool_input(state: &ToolUseState) -> Value {
    if !state.input_buffer.trim().is_empty()
        && let Some(parsed) = parse_tool_input_json(&state.input_buffer)
    {
        return parsed;
    }
    state.input.clone()
}

/// Parse streamed tool arguments (fences, segments, double-encoded JSON).
///
/// Callers that need TUI `arg_repair` should try that first, then fall back to
/// this function.
#[must_use]
pub fn parse_tool_input_json(buffer: &str) -> Option<Value> {
    let trimmed = buffer.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(stripped) = strip_code_fences(trimmed)
        && let Ok(value) = serde_json::from_str::<Value>(&stripped)
    {
        return Some(value);
    }
    if let Ok(Value::String(inner)) = serde_json::from_str::<Value>(trimmed)
        && let Ok(value) = serde_json::from_str::<Value>(&inner)
    {
        return Some(value);
    }
    extract_json_segment(trimmed).and_then(|segment| serde_json::from_str::<Value>(&segment).ok())
}

#[must_use]
pub fn caller_type_for_tool_use(caller: Option<&ToolCaller>) -> &str {
    caller.map_or("direct", |c| c.caller_type.as_str())
}

#[must_use]
pub fn caller_allowed_for_tool(caller: Option<&ToolCaller>, tool_def: Option<&Tool>) -> bool {
    let requested = caller_type_for_tool_use(caller);
    if let Some(def) = tool_def
        && let Some(allowed) = &def.allowed_callers
    {
        if allowed.is_empty() {
            return requested == "direct";
        }
        return allowed.iter().any(|item| item == requested);
    }
    requested == "direct"
}

#[must_use]
pub fn format_tool_error(err: &ToolError, tool_name: &str) -> String {
    match err {
        ToolError::InvalidInput { message } => {
            format!("Invalid input for tool '{tool_name}': {message}")
        }
        ToolError::MissingField { field } => {
            format!("Tool '{tool_name}' is missing required field '{field}'")
        }
        ToolError::PathEscape { path } => format!(
            "Path escapes workspace: {}. Use a workspace-relative path or enable trust mode.",
            path.display()
        ),
        ToolError::ExecutionFailed { message } => message.clone(),
        ToolError::Timeout { seconds } => format!(
            "Tool '{tool_name}' timed out after {seconds}s. Try a narrower scope or a longer timeout."
        ),
        ToolError::NotAvailable { message } => {
            let lower = message.to_ascii_lowercase();
            if lower.contains("current tool catalog") || lower.contains("did you mean:") {
                message.clone()
            } else {
                format!(
                    "Tool '{tool_name}' is not available: {message}. Check mode, feature flags, or tool name."
                )
            }
        }
        ToolError::PermissionDenied { message } => format!(
            "Tool '{tool_name}' was denied: {message}. Adjust approval mode or request permission."
        ),
    }
}

pub fn parse_parallel_tool_calls(
    input: &Value,
) -> Result<Vec<(String, Value)>, ToolError> {
    let tool_uses = input
        .get("tool_uses")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ToolError::missing_field("tool_uses"))?;
    if tool_uses.is_empty() {
        return Err(ToolError::invalid_input(
            "multi_tool_use.parallel requires at least one tool call",
        ));
    }

    let mut calls = Vec::with_capacity(tool_uses.len());
    for item in tool_uses {
        let name = item
            .get("recipient_name")
            .or_else(|| item.get("tool_name"))
            .or_else(|| item.get("name"))
            .or_else(|| item.get("tool"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::missing_field("recipient_name"))?;
        let params = item
            .get("parameters")
            .or_else(|| item.get("input"))
            .or_else(|| item.get("args"))
            .or_else(|| item.get("arguments"))
            .cloned()
            .unwrap_or_else(|| json!({}));
        calls.push((normalize_parallel_tool_name(name), params));
    }

    Ok(calls)
}

#[must_use]
pub fn should_parallelize_tool_batch(plans: &[ToolParallelPlanFlags]) -> bool {
    !plans.is_empty()
        && plans.iter().all(|plan| {
            plan.read_only
                && plan.supports_parallel
                && !plan.approval_required
                && !plan.interactive
        })
}

#[must_use]
pub fn should_stop_after_plan_tool(
    is_plan_mode: bool,
    tool_name: &str,
    result: &Result<ToolResult, ToolError>,
) -> bool {
    is_plan_mode && tool_name == "update_plan" && result.is_ok()
}

#[must_use]
pub fn should_force_update_plan_first(is_plan_mode: bool, content: &str) -> bool {
    if !is_plan_mode {
        return false;
    }

    let lower = content.to_ascii_lowercase();
    let asks_for_direct_plan = [
        "quick plan",
        "short plan",
        "simple plan",
        "3-step plan",
        "3 step plan",
        "three-step plan",
        "three step plan",
        "high-level plan",
        "high level plan",
        "give me a plan",
        "make a plan",
        "outline a plan",
        "draft a plan",
    ]
    .iter()
    .any(|needle| lower.contains(needle));

    if !asks_for_direct_plan {
        return false;
    }

    let asks_for_repo_exploration = [
        "inspect the repo",
        "inspect the code",
        "explore the repo",
        "search the repo",
        "read the code",
        "review the code",
        "analyze the code",
        "investigate",
        "look through",
        "understand the current",
        "ground it in the codebase",
        "based on the codebase",
    ]
    .iter()
    .any(|needle| lower.contains(needle));

    !asks_for_repo_exploration
}

/// Whether `name` is dispatched through the MCP pool (vs. the native
/// `ToolRegistry`). Mirrors the body of
/// `tui::mcp::McpPool::is_mcp_tool` so the core turn loop and
/// [`McpHost`](crate::engine::hosts::McpHost) implementations can
/// answer the same question without depending on the tui crate.
///
/// **Drift guard**: the
/// `is_mcp_tool_name_matches_tui_mcp_pool` test in `tui::mcp` asserts
/// this function and `McpPool::is_mcp_tool` produce identical output
/// on a curated name set.
#[must_use]
pub fn is_mcp_tool_name(name: &str) -> bool {
    name.starts_with("mcp_")
        || matches!(
            name,
            "list_mcp_resources" | "list_mcp_resource_templates" | "read_mcp_resource"
        )
}

#[must_use]
pub fn mcp_tool_is_parallel_safe(name: &str) -> bool {
    matches!(
        name,
        "list_mcp_resources"
            | "list_mcp_resource_templates"
            | "mcp_read_resource"
            | "read_mcp_resource"
            | "mcp_get_prompt"
    )
}

#[must_use]
pub fn mcp_tool_is_read_only(name: &str) -> bool {
    matches!(
        name,
        "list_mcp_resources"
            | "list_mcp_resource_templates"
            | "mcp_read_resource"
            | "read_mcp_resource"
            | "mcp_get_prompt"
    )
}

#[must_use]
pub fn mcp_tool_approval_description(name: &str) -> String {
    if mcp_tool_is_read_only(name) {
        format!("Read-only MCP tool '{name}'")
    } else {
        format!("MCP tool '{name}' may have side effects")
    }
}

fn strip_code_fences(text: &str) -> Option<String> {
    if !text.contains("```") {
        return None;
    }
    let mut lines = Vec::new();
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            continue;
        }
        lines.push(line);
    }
    let stripped = lines.join("\n");
    let stripped = stripped.trim();
    if stripped.is_empty() {
        None
    } else {
        Some(stripped.to_string())
    }
}

fn extract_json_segment(text: &str) -> Option<String> {
    extract_balanced_segment(text, '{', '}').or_else(|| extract_balanced_segment(text, '[', ']'))
}

fn extract_balanced_segment(text: &str, open: char, close: char) -> Option<String> {
    let start = text.find(open)?;
    let mut depth = 0i32;
    let mut end = None;
    for (offset, ch) in text[start..].char_indices() {
        if ch == open {
            depth += 1;
        } else if ch == close {
            depth -= 1;
            if depth == 0 {
                end = Some(start + offset + ch.len_utf8());
                break;
            }
        }
    }
    end.map(|end_idx| text[start..end_idx].to_string())
}

fn normalize_parallel_tool_name(raw: &str) -> String {
    let mut name = raw.trim();
    for prefix in ["functions.", "tools.", "tool."] {
        if let Some(stripped) = name.strip_prefix(prefix) {
            name = stripped;
            break;
        }
    }
    name.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parallel_batch_requires_read_only_parallel_tools() {
        let ok = ToolParallelPlanFlags {
            read_only: true,
            supports_parallel: true,
            approval_required: false,
            interactive: false,
        };
        assert!(should_parallelize_tool_batch(&[ok, ok]));
        assert!(!should_parallelize_tool_batch(&[ToolParallelPlanFlags {
            read_only: false,
            ..ok
        }]));
    }

    #[test]
    fn plan_mode_stops_after_update_plan() {
        assert!(should_stop_after_plan_tool(
            true,
            "update_plan",
            &Ok(ToolResult::success("ok"))
        ));
        assert!(!should_stop_after_plan_tool(
            false,
            "update_plan",
            &Ok(ToolResult::success("ok"))
        ));
    }

    #[test]
    fn is_mcp_tool_name_covers_prefix_and_resource_helpers() {
        assert!(is_mcp_tool_name("mcp_filesystem_read"));
        assert!(is_mcp_tool_name("mcp_git_status"));
        assert!(is_mcp_tool_name("list_mcp_resources"));
        assert!(is_mcp_tool_name("list_mcp_resource_templates"));
        assert!(is_mcp_tool_name("read_mcp_resource"));
        assert!(!is_mcp_tool_name("read_file"));
        assert!(!is_mcp_tool_name("exec_shell"));
        assert!(!is_mcp_tool_name(""));
    }

    #[test]
    fn quick_plan_forces_update_plan_first() {
        assert!(should_force_update_plan_first(
            true,
            "Give me a quick 3-step plan."
        ));
        assert!(!should_force_update_plan_first(
            true,
            "Inspect the repo and give me a quick plan."
        ));
    }
}
