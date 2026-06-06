//! Task-oriented display labels for sub-agents (replaces decorative whale nicknames).

use deepseek_core::subagent::{SubAgentAssignment, SubAgentType};

const MAX_NICKNAME_LEN: usize = 40;

#[derive(Debug, Clone)]
pub(crate) struct DeriveSubagentNicknameInput<'a> {
    pub agent_type: &'a SubAgentType,
    pub prompt: &'a str,
    pub assignment: &'a SubAgentAssignment,
    pub task_id: Option<&'a str>,
    pub cwd_label: Option<&'a str>,
    /// 1-based index among agents of the same type already tracked by the manager.
    pub type_index: usize,
}

/// Derive a short, task-meaningful label for the agent panel title.
#[must_use]
pub fn derive_subagent_nickname(input: DeriveSubagentNicknameInput<'_>) -> String {
    if let Some(scope) = extract_area_id_from_text(input.prompt)
        .or_else(|| extract_area_id_from_text(&input.assignment.objective))
        .or_else(|| extract_audit_task_title(input.prompt))
        .or_else(|| extract_audit_task_title(&input.assignment.objective))
    {
        return scope;
    }

    if let Some(task_id) = input
        .task_id
        .map(str::trim)
        .filter(|s| !s.is_empty() && s.len() <= MAX_NICKNAME_LEN)
    {
        return sanitize_label(task_id);
    }

    if let Some(cwd) = input.cwd_label.map(str::trim).filter(|s| !s.is_empty()) {
        return sanitize_label(cwd);
    }

    if let Some(role) = input
        .assignment
        .role
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return format!("{} #{}", sanitize_label(role), input.type_index);
    }

    format!("{} #{}", input.agent_type.as_str(), input.type_index)
}

fn sanitize_label(raw: &str) -> String {
    let collapsed: String = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.len() <= MAX_NICKNAME_LEN {
        return collapsed;
    }
    collapsed
        .chars()
        .take(MAX_NICKNAME_LEN.saturating_sub(1))
        .collect::<String>()
        + "…"
}

fn extract_audit_task_title(text: &str) -> Option<String> {
    for line in text.lines().take(24) {
        let trimmed = line.trim();
        for prefix in [
            "## Audit Task:",
            "# Audit Task:",
            "## audit task:",
            "Audit Task:",
            "audit task:",
        ] {
            if let Some(rest) = trimmed.strip_prefix(prefix) {
                let title = rest.trim().trim_matches(|c| c == '*' || c == '`');
                if !title.is_empty() {
                    return Some(sanitize_label(title));
                }
            }
        }
    }
    None
}

fn extract_area_id_from_text(text: &str) -> Option<String> {
    if let Some(value) = extract_json_string_field(text, "area_id") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Some(sanitize_label(trimmed));
        }
    }

    for line in text.lines().take(32) {
        let trimmed = line.trim().trim_start_matches('-').trim();
        let rest = trimmed
            .strip_prefix("area_id:")
            .or_else(|| trimmed.strip_prefix("**area_id**:"))
            .or_else(|| trimmed.strip_prefix("* area_id:"));
        if let Some(value) = rest {
            let cleaned = value
                .trim()
                .trim_matches(|c| c == '"' || c == '\'' || c == '`' || c == ',');
            if !cleaned.is_empty() {
                return Some(sanitize_label(cleaned));
            }
        }
    }
    None
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
                'r' => out.push('\r'),
                't' => out.push('\t'),
                '"' => out.push('"'),
                '\\' => out.push('\\'),
                other => {
                    out.push('\\');
                    out.push(other);
                }
            }
            escape = false;
        } else if ch == '\\' {
            escape = true;
        } else if ch == '"' {
            return Some((out, i + 1));
        } else {
            out.push(ch);
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use deepseek_core::subagent::SubAgentAssignment;

    fn derive_for_prompt(agent_type: &SubAgentType, prompt: &str, type_index: usize) -> String {
        let assignment = SubAgentAssignment::new(prompt.to_string(), None);
        derive_subagent_nickname(DeriveSubagentNicknameInput {
            agent_type,
            prompt,
            assignment: &assignment,
            task_id: None,
            cwd_label: None,
            type_index,
        })
    }

    #[test]
    fn derives_from_area_id_kv_line() {
        let prompt = "area_id: BE-Services\narea_path: backend/src/services";
        let nick = derive_for_prompt(&SubAgentType::Explore, prompt, 1);
        assert_eq!(nick, "BE-Services");
    }

    #[test]
    fn derives_from_json_area_id() {
        let prompt = r#"{"area_id": "BE-Middleware", "area_path": "backend/src/middleware"}"#;
        let nick = derive_for_prompt(&SubAgentType::Explore, prompt, 1);
        assert_eq!(nick, "BE-Middleware");
    }

    #[test]
    fn derives_from_audit_task_heading() {
        let prompt = "## Audit Task: FE-Components\n\nReview React components.";
        let nick = derive_for_prompt(&SubAgentType::Explore, prompt, 2);
        assert_eq!(nick, "FE-Components");
    }

    #[test]
    fn prefers_area_id_over_type_fallback() {
        let prompt = "area_id: area-core\nDo the work.";
        let nick = derive_for_prompt(&SubAgentType::General, prompt, 5);
        assert_eq!(nick, "area-core");
    }

    #[test]
    fn falls_back_to_type_index() {
        let nick = derive_for_prompt(&SubAgentType::Explore, "Read the README and summarize.", 3);
        assert_eq!(nick, "explore #3");
    }

    #[test]
    fn uses_task_id_when_no_scope_in_prompt() {
        let prompt = "Summarize docs.";
        let assignment = SubAgentAssignment::new(prompt.to_string(), None);
        let nick = derive_subagent_nickname(DeriveSubagentNicknameInput {
            agent_type: &SubAgentType::General,
            prompt,
            assignment: &assignment,
            task_id: Some("craft-wp-auth"),
            cwd_label: None,
            type_index: 1,
        });
        assert_eq!(nick, "craft-wp-auth");
    }

    #[test]
    fn uses_cwd_label_before_type_fallback() {
        let prompt = "Implement feature X.";
        let assignment = SubAgentAssignment::new(prompt.to_string(), None);
        let nick = derive_subagent_nickname(DeriveSubagentNicknameInput {
            agent_type: &SubAgentType::Implementer,
            prompt,
            assignment: &assignment,
            task_id: None,
            cwd_label: Some("feature-auth"),
            type_index: 2,
        });
        assert_eq!(nick, "feature-auth");
    }

    #[test]
    fn truncates_overlong_labels() {
        let long = "a".repeat(60);
        let prompt = format!("area_id: {long}");
        let nick = derive_for_prompt(&SubAgentType::Explore, &prompt, 1);
        assert!(nick.chars().count() <= MAX_NICKNAME_LEN);
        assert!(nick.ends_with('…'));
    }
}
