


use deepseek_core::subagent::{
    StructuredVerdict, SubAgentAssignment,
    SubAgentType,
};

use super::prompt_text::*;


pub fn subagent_system_prompt(agent_type: &SubAgentType) -> String {
    match *agent_type {
        SubAgentType::General => GENERAL_AGENT_PROMPT.to_string(),
        SubAgentType::Explore => EXPLORE_AGENT_PROMPT.to_string(),
        SubAgentType::Plan => PLAN_AGENT_PROMPT.to_string(),
        SubAgentType::Review => REVIEW_AGENT_PROMPT.to_string(),
        SubAgentType::Implementer => IMPLEMENTER_AGENT_PROMPT.to_string(),
        SubAgentType::Verifier => VERIFIER_AGENT_PROMPT.to_string(),
        SubAgentType::Custom => CUSTOM_AGENT_PROMPT.to_string(),
        SubAgentType::Auditor => AUDITOR_AGENT_PROMPT.to_string(),
    }
}

/// Default allowed tools for a sub-agent type (deprecated advisory list).
#[must_use]
#[deprecated(
    since = "0.6.6",
    note = "Default sub-agents inherit the full parent registry; pass an explicit allowed_tools list only for narrow Custom roles."
)]
pub fn subagent_allowed_tools(agent_type: &SubAgentType) -> Vec<&'static str> {
    match *agent_type {
        SubAgentType::General => vec![
                "list_dir",
                "read_file",
                "write_file",
                "edit_file",
                "apply_patch",
                "grep_files",
                "file_search",
                "web.run",
                "web_search",
                "exec_shell",
                "exec_shell_wait",
                "exec_shell_interact",
                "exec_wait",
                "exec_interact",
                "note",
                "checklist_write",
                "checklist_add",
                "checklist_update",
                "checklist_list",
                "todo_write",
                "todo_add",
                "todo_update",
                "todo_list",
                "update_plan",
            ],
        SubAgentType::Explore => vec![
                "list_dir",
                "read_file",
                "grep_files",
                "glob_files",
                "file_search",
                "web.run",
                "web_search",
                "exec_shell",
                "exec_shell_wait",
                "exec_shell_interact",
                "exec_wait",
                "exec_interact",
            ],
        SubAgentType::Plan => vec![
                "list_dir",
                "read_file",
                "grep_files",
                "glob_files",
                "file_search",
                "web.run",
                "note",
                "update_plan",
                "checklist_write",
                "checklist_add",
                "checklist_update",
                "checklist_list",
                "todo_write",
                "todo_add",
                "todo_update",
                "todo_list",
            ],
        SubAgentType::Review => vec!["list_dir", "read_file", "grep_files", "glob_files", "file_search", "note"],
        SubAgentType::Implementer => vec![
                "list_dir",
                "read_file",
                "write_file",
                "edit_file",
                "apply_patch",
                "grep_files",
                "file_search",
                "exec_shell",
                "exec_shell_wait",
                "exec_shell_interact",
                "exec_wait",
                "exec_interact",
                "note",
                "checklist_write",
                "checklist_add",
                "checklist_update",
                "checklist_list",
                "todo_write",
                "todo_add",
                "todo_update",
                "todo_list",
                "update_plan",
            ],
        SubAgentType::Verifier => vec![
                "list_dir",
                "read_file",
                "grep_files",
                "file_search",
                "exec_shell",
                "exec_shell_wait",
                "exec_shell_interact",
                "exec_wait",
                "exec_interact",
                "run_tests",
                "diagnostics",
                "note",
            ],
        SubAgentType::Custom => vec![],
        SubAgentType::Auditor => vec![
                "list_dir",
                "read_file",
                "grep_files",
                "glob_files",
                "file_search",
                "note",
            ],
    }
}
pub(crate) fn build_subagent_system_prompt(
    agent_type: &SubAgentType,
    assignment: &SubAgentAssignment,
) -> String {
    let base = subagent_system_prompt(agent_type);
    match assignment.role.as_deref() {
        Some(role) if !role.trim().is_empty() => {
            format!(
                "{base}\n\nYou are operating in the role of `{}`.",
                role.trim()
            )
        }
        _ => base,
    }
}
// === Structured Verdict (CRAFT P0) ===

/// Parse a `<!-- craft-verdict -->` JSON fence from the agent's final text output.
///
/// Strategy: search for the marker, extract the first `{…}` JSON block
/// that follows, and deserialize. Returns `None` if the marker is absent
/// or the JSON is unparseable — the caller falls back to natural-language
/// processing (graceful degradation).
pub(crate) fn parse_structured_verdict(text: &str) -> Option<StructuredVerdict> {
    let marker = "<!-- craft-verdict -->";
    let Some(after_marker) = text.find(marker).map(|idx| &text[idx + marker.len()..]) else {
        tracing::debug!(
            "parse_structured_verdict: no fence marker found, falling back to natural-language"
        );
        return None;
    };

    // Find the first '{' and matching '}'
    let brace_start = after_marker.find('{')?;
    let slice = &after_marker[brace_start..];

    // Naive brace matching: find the final '}' that balances
    let mut depth = 0i32;
    let mut end = None;
    for (i, ch) in slice.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(i + ch.len_utf8());
                    break;
                }
            }
            _ => {}
        }
    }

    let json_str = end.map(|e| &slice[..e])?;
    match serde_json::from_str::<StructuredVerdict>(json_str) {
        Ok(v) => {
            tracing::info!(
                "parse_structured_verdict: success (verdict={}, items={})",
                super::craft::verdict_level_str(&v.verdict),
                v.items.len(),
            );
            Some(v)
        }
        Err(e) => {
            tracing::warn!("parse_structured_verdict: JSON parse failed: {e}");
            None
        }
    }
}

