


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
    match parse_json_fence_after_marker(text, "<!-- audit-findings -->") {
        Ok(findings) => Ok(findings),
        Err(ParseFailureReason::Truncated) => {
            let Some(marker_idx) = text.find("<!-- audit-findings -->") else {
                return Err(ParseFailureReason::Truncated);
            };
            let after_marker = &text[marker_idx + "<!-- audit-findings -->".len()..];
            let Some(brace_start) = after_marker.find('{') else {
                return Err(ParseFailureReason::Truncated);
            };
            salvage_structured_findings(&after_marker[brace_start..])
                .ok_or(ParseFailureReason::Truncated)
        }
        Err(other) => Err(other),
    }
}

/// Best-effort recovery when the outer JSON object was cut off mid-stream.
fn salvage_structured_findings(partial_json: &str) -> Option<StructuredFindings> {
    let area_id = extract_json_string_field(partial_json, "area_id")?;
    if area_id.trim().is_empty() {
        return None;
    }
    let area_path = extract_json_string_field(partial_json, "area_path");
    let items = extract_items_from_truncated(partial_json);
    let summary = extract_json_string_field(partial_json, "summary");
    Some(StructuredFindings {
        area_id,
        area_path,
        items,
        summary,
    })
}

fn extract_json_string_field(json: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let start = json.find(&needle)?;
    let rest = json[start + needle.len()..].trim_start();
    if !rest.starts_with(':') {
        return None;
    }
    let rest = rest[1..].trim_start();
    parse_json_string_at(rest).map(|(value, _)| value)
}

fn parse_json_string_at(s: &str) -> Option<(String, usize)> {
    if !s.starts_with('"') {
        return None;
    }
    let mut out = String::new();
    let mut escape = false;
    let mut i = 1usize;
    let chars: Vec<char> = s.chars().collect();
    while i < chars.len() {
        let ch = chars[i];
        if escape {
            match ch {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                '"' => out.push('"'),
                '\\' => out.push('\\'),
                other => {
                    out.push('\\');
                    out.push(other);
                }
            }
            escape = false;
            i += 1;
            continue;
        }
        if ch == '\\' {
            escape = true;
            i += 1;
            continue;
        }
        if ch == '"' {
            return Some((out, i + 1));
        }
        out.push(ch);
        i += 1;
    }
    None
}

fn extract_balanced_object(s: &str) -> Option<(&str, usize)> {
    if !s.starts_with('{') {
        return None;
    }
    let mut depth = 0i32;
    let mut in_str = false;
    let mut escape = false;
    for (i, ch) in s.char_indices() {
        if in_str {
            if escape {
                escape = false;
                continue;
            }
            if ch == '\\' {
                escape = true;
                continue;
            }
            if ch == '"' {
                in_str = false;
            }
            continue;
        }
        match ch {
            '"' => in_str = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some((&s[..i + ch.len_utf8()], i + ch.len_utf8()));
                }
            }
            _ => {}
        }
    }
    None
}

fn extract_items_from_truncated(json: &str) -> Vec<deepseek_core::subagent::AuditFindingItem> {
    let items_key = "\"items\"";
    let Some(idx) = json.find(items_key) else {
        return Vec::new();
    };
    let after = &json[idx + items_key.len()..];
    let Some(bracket) = after.find('[') else {
        return Vec::new();
    };
    let mut slice = &after[bracket + 1..];
    let mut items = Vec::new();
    loop {
        slice = slice.trim_start();
        if slice.starts_with(']') || slice.is_empty() {
            break;
        }
        if !slice.starts_with('{') {
            break;
        }
        let Some((obj_str, consumed)) = extract_balanced_object(slice) else {
            break;
        };
        if let Ok(item) =
            serde_json::from_str::<deepseek_core::subagent::AuditFindingItem>(obj_str)
        {
            items.push(item);
        }
        slice = &slice[consumed..];
        slice = slice.trim_start();
        if slice.starts_with(',') {
            slice = &slice[1..];
        }
    }
    items
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
    fn parse_structured_findings_salvages_truncated_items() {
        let text = r#"<!-- audit-findings -->
{
  "area_id": "area-tools",
  "area_path": "crates/tools",
  "items": [{
    "severity": "HIGH",
    "file": "src/lib.rs",
    "line": 10,
    "claim": "first"
  }, {
    "severity": "MEDIUM",
    "file": "src/other.rs",
    "line": 20,
    "claim": "truncated mid-
"#;
        let f = parse_structured_findings_result(text).expect("salvaged");
        assert_eq!(f.area_id, "area-tools");
        assert_eq!(f.items.len(), 1);
        assert_eq!(f.items[0].claim, "first");
    }

    #[test]
    fn parse_structured_findings_reports_truncated_without_area_id() {
        let text = "<!-- audit-findings -->\n{\"items\":[";
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

