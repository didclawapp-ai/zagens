//! Deferred tool catalog policy and built-in tool-search helpers (P2 PR4).
//!
//! Execution of `code_execution` (subprocess) stays in the runtime-server tool layer.

use std::collections::HashSet;

use serde_json::{Value, json};
use zagens_tools::{ToolError, ToolResult, required_str};

use crate::chat::Tool;
use crate::turn::TurnLoopMode;

pub const MULTI_TOOL_PARALLEL_NAME: &str = "multi_tool_use.parallel";
pub const REQUEST_USER_INPUT_NAME: &str = "request_user_input";
pub const CODE_EXECUTION_TOOL_NAME: &str = "code_execution";
// Reserved for future API-defined tool payloads (OpenAI code_execution).
#[allow(dead_code)]
const CODE_EXECUTION_TOOL_TYPE: &str = "code_execution_20250825";
const TOOL_SEARCH_REGEX_NAME: &str = "tool_search_tool_regex";
const TOOL_SEARCH_REGEX_TYPE: &str = "tool_search_tool_regex_20251119";
pub const TOOL_SEARCH_BM25_NAME: &str = "tool_search_tool_bm25";
const TOOL_SEARCH_BM25_TYPE: &str = "tool_search_tool_bm25_20251119";

#[must_use]
pub fn is_tool_search_tool(name: &str) -> bool {
    matches!(name, TOOL_SEARCH_REGEX_NAME | TOOL_SEARCH_BM25_NAME)
}

#[must_use]
pub fn should_default_defer_tool(name: &str, mode: TurnLoopMode) -> bool {
    if mode == TurnLoopMode::Yolo {
        return false;
    }

    let always_loaded_in_action_modes = matches!(mode, TurnLoopMode::Agent)
        && matches!(
            name,
            "exec_shell"
                | "exec_shell_wait"
                | "exec_shell_interact"
                | "exec_shell_cancel"
                | "exec_wait"
                | "exec_interact"
        );
    if always_loaded_in_action_modes {
        return false;
    }

    !matches!(
        name,
        "read_file"
            | "file_info"
            | "list_dir"
            | "glob_files"
            | "grep_files"
            | "file_search"
            | "load_skill"
            | "diagnostics"
            | "rlm"
            | "recall_archive"
            | MULTI_TOOL_PARALLEL_NAME
            | "update_plan"
            | "checklist_write"
            | "todo_write"
            | "write_office"
            | "read_office"
            | "load_office_payload"
            | "describe_image"
            | "task_list"
            | "task_read"
            | "task_gate_run"
            | "task_shell_start"
            | "task_shell_wait"
            | "github_issue_context"
            | "github_pr_context"
            | "scratchpad_status"
            | "scratchpad_init"
            | "scratchpad_append"
            | "scratchpad_set_area"
            | "scratchpad_list_notes"
            | "investigate"
            | "answer_from_repo"
            | "explore_codebase"
            | "change_and_verify"
            | "edit_and_check"
            | "promote_to_context"
            | REQUEST_USER_INPUT_NAME
    )
}

fn audit_scratchpad_eager_tool(name: &str) -> bool {
    matches!(
        name,
        "agent_spawn"
            | "spawn_agent"
            | "delegate_to_agent"
            | "agent_list"
            | "agent_result"
            | "agent_wait"
    )
}

fn audit_scratchpad_defer_task_create(name: &str, scratchpad_active: bool) -> bool {
    scratchpad_active && name == "task_create"
}

pub fn apply_native_tool_deferral(
    catalog: &mut [Tool],
    mode: TurnLoopMode,
    scratchpad_run_id: Option<&str>,
) {
    let scratchpad_active = scratchpad_run_id.is_some_and(|id| !id.trim().is_empty());
    for tool in catalog {
        let mut defer = should_default_defer_tool(&tool.name, mode);
        if scratchpad_active && audit_scratchpad_eager_tool(&tool.name) {
            defer = false;
        }
        if audit_scratchpad_defer_task_create(&tool.name, scratchpad_active) {
            defer = true;
        }
        tool.defer_loading = Some(defer);
    }
}

/// Block batched `scratchpad_set_area(status=deferred)` — P2 requires append(meta) then
/// set_area for **one** pending area per model step.
#[must_use]
pub fn scratchpad_defer_set_area_batch_error(batch_count: usize) -> Option<ToolError> {
    if batch_count <= 1 {
        return None;
    }
    Some(ToolError::invalid_input(format!(
        "P2 workflow: do not issue {batch_count} scratchpad_set_area(deferred) calls in one step. \
         For each pending area, in separate steps: (1) scratchpad_append with kind=meta and a non-empty \
         claim (defer reason), (2) scratchpad_set_area(deferred) for that single area_id. \
         Call scratchpad_status for pending_area_ids; repeat one area at a time."
    )))
}

/// Scratchpad tools that bind or mutate the active audit run (mid-turn eager-load hook).
#[must_use]
pub fn is_audit_scratchpad_bind_tool(name: &str) -> bool {
    matches!(
        name,
        "scratchpad_init"
            | "scratchpad_append"
            | "scratchpad_set_area"
            | "scratchpad_verify_note"
            | "scratchpad_import_agent"
    )
}

/// After scratchpad bind, refresh deferral and add audit sub-agent tools to the active set.
pub fn activate_audit_subagent_tools(
    catalog: &mut [Tool],
    mode: TurnLoopMode,
    scratchpad_run_id: Option<&str>,
    active: &mut HashSet<String>,
) {
    apply_native_tool_deferral(catalog, mode, scratchpad_run_id);
    for tool in catalog {
        if audit_scratchpad_eager_tool(&tool.name) && !tool.defer_loading.unwrap_or(true) {
            active.insert(tool.name.clone());
        }
    }
}

fn should_keep_mcp_tool_loaded(name: &str) -> bool {
    matches!(
        name,
        "list_mcp_resources"
            | "list_mcp_resource_templates"
            | "mcp_read_resource"
            | "read_mcp_resource"
            | "mcp_get_prompt"
    )
}

pub fn apply_mcp_tool_deferral(catalog: &mut [Tool], mode: TurnLoopMode) {
    for tool in catalog {
        tool.defer_loading =
            Some(mode != TurnLoopMode::Yolo && !should_keep_mcp_tool_loaded(&tool.name));
    }
}

#[must_use]
pub fn build_model_tool_catalog(
    mut native_tools: Vec<Tool>,
    mut mcp_tools: Vec<Tool>,
    mode: TurnLoopMode,
    scratchpad_run_id: Option<&str>,
) -> Vec<Tool> {
    apply_native_tool_deferral(&mut native_tools, mode, scratchpad_run_id);
    apply_mcp_tool_deferral(&mut mcp_tools, mode);
    native_tools.sort_by(|a, b| a.name.cmp(&b.name));
    mcp_tools.sort_by(|a, b| a.name.cmp(&b.name));
    native_tools.extend(mcp_tools);
    native_tools
}

pub fn ensure_advanced_tooling(catalog: &mut Vec<Tool>) {
    if !catalog.iter().any(|t| t.name == TOOL_SEARCH_REGEX_NAME) {
        catalog.push(Tool {
            tool_type: Some(TOOL_SEARCH_REGEX_TYPE.to_string()),
            name: TOOL_SEARCH_REGEX_NAME.to_string(),
            description: "Search deferred tool definitions using a regex query and return matching tool references.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Regex pattern to search tool names/descriptions/schema." }
                },
                "required": ["query"]
            }),
            allowed_callers: Some(vec!["direct".to_string()]),
            defer_loading: Some(false),
            input_examples: None,
            strict: None,
            cache_control: None,
        });
    }

    if !catalog.iter().any(|t| t.name == TOOL_SEARCH_BM25_NAME) {
        catalog.push(Tool {
            tool_type: Some(TOOL_SEARCH_BM25_TYPE.to_string()),
            name: TOOL_SEARCH_BM25_NAME.to_string(),
            description: "Search deferred tool definitions using natural-language matching and return matching tool references.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Natural language query for tool discovery." }
                },
                "required": ["query"]
            }),
            allowed_callers: Some(vec!["direct".to_string()]),
            defer_loading: Some(false),
            input_examples: None,
            strict: None,
            cache_control: None,
        });
    }
}

#[must_use]
pub fn initial_active_tools(catalog: &[Tool]) -> HashSet<String> {
    let mut active = HashSet::new();
    for tool in catalog {
        if !tool.defer_loading.unwrap_or(false) || is_tool_search_tool(&tool.name) {
            active.insert(tool.name.clone());
        }
    }
    if active.is_empty()
        && !catalog.is_empty()
        && let Some(first) = catalog.first()
    {
        active.insert(first.name.clone());
    }
    active
}

fn active_tool_list_from_catalog(catalog: &[Tool], active: &HashSet<String>) -> Vec<Tool> {
    let mut head: Vec<Tool> = Vec::new();
    let mut tail: Vec<Tool> = Vec::new();
    for tool in catalog {
        if !active.contains(&tool.name) {
            continue;
        }
        if tool.defer_loading.unwrap_or(false) {
            tail.push(tool.clone());
        } else {
            head.push(tool.clone());
        }
    }
    head.extend(tail);
    head
}

#[must_use]
pub fn active_tools_for_step(
    catalog: &[Tool],
    active: &HashSet<String>,
    force_update_plan: bool,
) -> Vec<Tool> {
    if force_update_plan {
        let forced: Vec<_> = catalog
            .iter()
            .filter(|tool| tool.name == "update_plan")
            .cloned()
            .collect();
        if !forced.is_empty() {
            return forced;
        }
    }

    active_tool_list_from_catalog(catalog, active)
}

fn tool_search_haystack(tool: &Tool) -> String {
    format!(
        "{}\n{}\n{}",
        tool.name.to_lowercase(),
        tool.description.to_lowercase(),
        tool.input_schema.to_string().to_lowercase()
    )
}

fn discover_tools_with_regex(catalog: &[Tool], query: &str) -> Result<Vec<String>, ToolError> {
    let regex = regex::Regex::new(query)
        .map_err(|err| ToolError::invalid_input(format!("Invalid regex query: {err}")))?;

    let mut matches = Vec::new();
    for tool in catalog {
        if is_tool_search_tool(&tool.name) {
            continue;
        }
        let hay = tool_search_haystack(tool);
        if regex.is_match(&hay) {
            matches.push(tool.name.clone());
        }
        if matches.len() >= 5 {
            break;
        }
    }
    Ok(matches)
}

fn discover_tools_with_bm25_like(catalog: &[Tool], query: &str) -> Vec<String> {
    let terms: Vec<String> = query
        .split_whitespace()
        .map(|term| term.trim().to_lowercase())
        .filter(|term| !term.is_empty())
        .collect();
    if terms.is_empty() {
        return Vec::new();
    }

    let mut scored: Vec<(i64, String)> = Vec::new();
    for tool in catalog {
        if is_tool_search_tool(&tool.name) {
            continue;
        }
        let hay = tool_search_haystack(tool);
        let mut score = 0i64;
        for term in &terms {
            if hay.contains(term) {
                score += 1;
            }
            if tool.name.to_lowercase().contains(term) {
                score += 2;
            }
        }
        if score > 0 {
            scored.push((score, tool.name.clone()));
        }
    }
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    scored.into_iter().take(5).map(|(_, name)| name).collect()
}

fn edit_distance(a: &str, b: &str) -> usize {
    if a == b {
        return 0;
    }
    if a.is_empty() {
        return b.chars().count();
    }
    if b.is_empty() {
        return a.chars().count();
    }

    let b_chars: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b_chars.len()).collect();
    let mut curr = vec![0usize; b_chars.len() + 1];

    for (i, a_ch) in a.chars().enumerate() {
        curr[0] = i + 1;
        for (j, b_ch) in b_chars.iter().enumerate() {
            let cost = if a_ch == *b_ch { 0 } else { 1 };
            let delete = prev[j + 1] + 1;
            let insert = curr[j] + 1;
            let substitute = prev[j] + cost;
            curr[j + 1] = delete.min(insert).min(substitute);
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[b_chars.len()]
}

fn suggest_tool_names(catalog: &[Tool], requested: &str, limit: usize) -> Vec<String> {
    let requested = requested.trim().to_ascii_lowercase();
    if requested.is_empty() || limit == 0 {
        return Vec::new();
    }

    let mut candidates: Vec<(u8, usize, String)> = Vec::new();
    for tool in catalog {
        let candidate = tool.name.to_ascii_lowercase();
        let prefix_match = candidate.starts_with(&requested) || requested.starts_with(&candidate);
        let contains_match = candidate.contains(&requested) || requested.contains(&candidate);
        let distance = edit_distance(&candidate, &requested);
        let close_typo = distance <= 3;

        if !(prefix_match || contains_match || close_typo) {
            continue;
        }

        let rank = if prefix_match {
            0
        } else if contains_match {
            1
        } else {
            2
        };
        candidates.push((rank, distance, tool.name.clone()));
    }

    candidates.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1.cmp(&b.1))
            .then_with(|| a.2.cmp(&b.2))
    });
    candidates.dedup_by(|a, b| a.2 == b.2);
    candidates
        .into_iter()
        .take(limit)
        .map(|(_, _, name)| name)
        .collect()
}

#[must_use]
pub fn missing_tool_error_message(tool_name: &str, catalog: &[Tool]) -> String {
    let suggestions = suggest_tool_names(catalog, tool_name, 3);
    if suggestions.is_empty() {
        return format!(
            "Tool '{tool_name}' is not available in the current tool catalog. \
             Verify mode/feature flags, or use {TOOL_SEARCH_BM25_NAME} with a short query."
        );
    }

    format!(
        "Tool '{tool_name}' is not available in the current tool catalog. \
         Did you mean: {}? You can also use {TOOL_SEARCH_BM25_NAME} to discover tools.",
        suggestions.join(", ")
    )
}

#[must_use]
pub fn maybe_activate_requested_deferred_tool(
    tool_name: &str,
    catalog: &[Tool],
    active_tools: &mut HashSet<String>,
) -> bool {
    let Some(def) = catalog.iter().find(|def| def.name == tool_name) else {
        return false;
    };

    if !def.defer_loading.unwrap_or(false) || active_tools.contains(tool_name) {
        return false;
    }

    active_tools.insert(tool_name.to_string())
}

pub fn execute_tool_search(
    tool_name: &str,
    input: &Value,
    catalog: &[Tool],
    active_tools: &mut HashSet<String>,
) -> Result<ToolResult, ToolError> {
    let query = required_str(input, "query")?;
    let discovered = if tool_name == TOOL_SEARCH_REGEX_NAME {
        discover_tools_with_regex(catalog, query)?
    } else {
        discover_tools_with_bm25_like(catalog, query)
    };

    for name in &discovered {
        active_tools.insert(name.clone());
    }

    let references = discovered
        .iter()
        .map(|name| json!({"type": "tool_reference", "tool_name": name}))
        .collect::<Vec<_>>();

    let payload = json!({
        "type": "tool_search_tool_search_result",
        "tool_references": references,
    });

    Ok(ToolResult {
        content: serde_json::to_string(&payload).unwrap_or_else(|_| payload.to_string()),
        success: true,
        metadata: Some(json!({
            "tool_references": discovered,
        })),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scratchpad_defer_batch_blocked_when_count_gt_one() {
        assert!(scratchpad_defer_set_area_batch_error(1).is_none());
        let err = scratchpad_defer_set_area_batch_error(19).expect("blocked");
        let msg = err.to_string();
        assert!(msg.contains("do not issue 19"));
        assert!(msg.contains("one area at a time"));
    }

    #[test]
    fn deferral_keeps_shell_eager_in_agent_mode() {
        assert!(!should_default_defer_tool(
            "exec_shell",
            TurnLoopMode::Agent
        ));
        assert!(!should_default_defer_tool(
            "exec_shell_wait",
            TurnLoopMode::Agent
        ));
        assert!(!should_default_defer_tool(
            "exec_shell_cancel",
            TurnLoopMode::Agent
        ));
        assert!(should_default_defer_tool("exec_shell", TurnLoopMode::Plan));
    }

    #[test]
    fn deferral_keeps_office_read_tools_eager_in_agent_mode() {
        assert!(!should_default_defer_tool(
            "read_office",
            TurnLoopMode::Agent
        ));
        assert!(!should_default_defer_tool(
            "load_office_payload",
            TurnLoopMode::Agent
        ));
        assert!(!should_default_defer_tool(
            "write_office",
            TurnLoopMode::Agent
        ));
    }

    #[test]
    fn activate_audit_subagent_tools_eager_loads_spawn_after_bind() {
        use crate::chat::Tool;

        let mut catalog = vec![Tool {
            tool_type: None,
            name: "agent_spawn".to_string(),
            description: String::new(),
            input_schema: serde_json::json!({}),
            allowed_callers: None,
            defer_loading: Some(true),
            input_examples: None,
            strict: None,
            cache_control: None,
        }];
        let mut active = HashSet::new();
        activate_audit_subagent_tools(
            &mut catalog,
            TurnLoopMode::Agent,
            Some("audit-run-1"),
            &mut active,
        );
        assert_eq!(catalog[0].defer_loading, Some(false));
        assert!(active.contains("agent_spawn"));
    }
}
