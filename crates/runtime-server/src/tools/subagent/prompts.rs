


use deepseek_core::subagent::{
    ParseFailureReason, StructuredFindings, StructuredVerdict, SubAgentAssignment, SubAgentType,
    VerdictItem, VerdictLevel,
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

/// Parse a `<!-- audit-findings -->` JSON fence from Explore/Review final output.
pub(crate) fn parse_structured_findings(text: &str) -> Option<StructuredFindings> {
    parse_structured_findings_result(text).ok()
}

/// Parse audit findings with an explicit failure reason for diagnostics.
pub(crate) fn parse_structured_findings_result(
    text: &str,
) -> Result<StructuredFindings, ParseFailureReason> {
    parse_json_fence_after_marker(text, "<!-- audit-findings -->")
}

/// Parse a `<!-- craft-verdict -->` JSON fence from the agent's final text output.
pub(crate) fn parse_structured_verdict(text: &str) -> Option<StructuredVerdict> {
    parse_json_fence_after_marker(text, "<!-- craft-verdict -->").ok()
}

fn parse_json_fence_after_marker<T: serde::de::DeserializeOwned>(
    text: &str,
    marker: &str,
) -> Result<T, ParseFailureReason> {
    let Some(after_marker) = text.find(marker).map(|idx| &text[idx + marker.len()..]) else {
        tracing::debug!(
            "parse_json_fence_after_marker: no marker '{marker}' found"
        );
        return Err(ParseFailureReason::NoMarker);
    };

    let Some(brace_start) = after_marker.find('{') else {
        return Err(ParseFailureReason::Truncated);
    };
    let slice = &after_marker[brace_start..];

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

    let Some(json_str) = end.map(|e| &slice[..e]) else {
        return Err(ParseFailureReason::Truncated);
    };
    serde_json::from_str::<T>(json_str).map_err(|e| {
        tracing::warn!("parse_json_fence_after_marker({marker}): JSON parse failed: {e}");
        ParseFailureReason::InvalidJson(e.to_string())
    })
}

/// Map audit findings to CRAFT verdict for blackboard compatibility.
pub(crate) fn findings_to_verdict(findings: &StructuredFindings) -> StructuredVerdict {
    let verdict = if findings.items.is_empty() {
        VerdictLevel::Pass
    } else if findings.items.iter().any(|i| {
        matches!(
            i.severity.to_ascii_uppercase().as_str(),
            "HIGH" | "BLOCKER"
        )
    }) {
        VerdictLevel::Blocker
    } else {
        VerdictLevel::Major
    };
    let items: Vec<VerdictItem> = findings
        .items
        .iter()
        .map(|i| VerdictItem {
            severity: i.severity.to_ascii_uppercase(),
            file: i.file.clone().unwrap_or_default(),
            line: i.line,
            description: i.claim.clone(),
            rule: None,
            suggestion: i.evidence.clone(),
        })
        .collect();
    StructuredVerdict {
        verdict,
        items,
        summary: findings.summary.clone(),
    }
}

#[cfg(test)]
mod parse_tests {
    use super::*;

    #[test]
    fn parse_structured_findings_reads_fence() {
        let text = r#"done
<!-- audit-findings -->
{
  "area_id": "area-core",
  "items": [{
    "severity": "HIGH",
    "file": "src/lib.rs",
    "line": 10,
    "claim": "test"
  }]
}"#;
        let f = parse_structured_findings_result(text).expect("findings");
        assert_eq!(f.area_id, "area-core");
        assert_eq!(f.items.len(), 1);
        assert_eq!(f.items[0].claim, "test");
    }

    #[test]
    fn parse_structured_findings_reports_no_marker() {
        assert_eq!(
            parse_structured_findings_result("plain prose only"),
            Err(ParseFailureReason::NoMarker)
        );
    }

    #[test]
    fn parse_structured_findings_reports_truncated_fence() {
        let text = "<!-- audit-findings -->\n{\"area_id\":\"a\",\"items\":[";
        assert_eq!(
            parse_structured_findings_result(text),
            Err(ParseFailureReason::Truncated)
        );
    }

    #[test]
    fn parse_structured_findings_reports_invalid_json() {
        let text = "<!-- audit-findings -->\n{not-json}";
        assert!(matches!(
            parse_structured_findings_result(text),
            Err(ParseFailureReason::InvalidJson(_))
        ));
    }
}

