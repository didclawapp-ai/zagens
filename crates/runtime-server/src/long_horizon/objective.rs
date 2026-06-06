//! Objective derivation for LHT nudge messages (§4.5).

use deepseek_core::chat::{ContentBlock, Message};
use deepseek_core::engine::context::summarize_text;

use crate::tools::plan::{PlanSnapshot, StepStatus};
use crate::tools::todo::{TodoListSnapshot, TodoStatus};

const MAX_OBJECTIVE_CHARS: usize = 120;
const MAX_CHECKLIST_OBJECTIVE_CHARS: usize = 80;

/// Returns `(objective_one_line, source_tag)`.
#[must_use]
pub fn derive_objective(
    plan: &PlanSnapshot,
    checklist: &TodoListSnapshot,
    messages: &[Message],
    lang: &str,
) -> (String, &'static str) {
    if let Some(explanation) = plan.explanation.as_deref().filter(|s| !s.trim().is_empty()) {
        let first = first_sentence(explanation);
        return (
            truncate_chars(&first, MAX_OBJECTIVE_CHARS),
            "plan_explanation",
        );
    }

    if let Some((_, item)) = plan
        .items
        .iter()
        .enumerate()
        .find(|(_, item)| item.status == StepStatus::InProgress)
    {
        return (
            truncate_chars(&item.step, MAX_OBJECTIVE_CHARS),
            "plan_in_progress",
        );
    }

    if let Some(item) = plan
        .items
        .iter()
        .find(|item| item.status == StepStatus::Pending)
    {
        return (
            truncate_chars(&item.step, MAX_OBJECTIVE_CHARS),
            "plan_pending",
        );
    }

    if let Some(item) = checklist
        .items
        .iter()
        .find(|item| item.status == TodoStatus::InProgress)
    {
        let content = super::verify::strip_verify_prefix(&item.content);
        return (
            truncate_chars(&content, MAX_CHECKLIST_OBJECTIVE_CHARS),
            "checklist_in_progress",
        );
    }

    if let Some(text) = latest_user_message(messages) {
        return (summarize_text(&text, 280), "latest_user");
    }

    let fallback = if is_zh(lang) {
        "长程代码任务"
    } else {
        "Long-horizon code task"
    };
    (fallback.to_string(), "fallback")
}

fn first_sentence(text: &str) -> String {
    let mut end = text.len();
    for (i, ch) in text.char_indices() {
        if ch == '.' || ch == '。' || ch == '\n' {
            end = i;
            break;
        }
    }
    text[..end].trim().to_string()
}

fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    text.chars().take(max).collect::<String>() + "…"
}

fn latest_user_message(messages: &[Message]) -> Option<String> {
    messages.iter().rev().find_map(|m| {
        if m.role != "user" {
            return None;
        }
        let text: String = m
            .content
            .iter()
            .filter_map(|block| {
                if let ContentBlock::Text { text, .. } = block {
                    Some(text.as_str())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        let trimmed = text.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn is_zh(lang: &str) -> bool {
    let l = lang.trim().to_ascii_lowercase();
    l.starts_with("zh")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::plan::{PlanItemArg, PlanSnapshot};
    use crate::tools::todo::TodoListSnapshot;

    #[test]
    fn explanation_first_sentence() {
        let plan = PlanSnapshot {
            explanation: Some("Refactor auth. Then ship.".into()),
            items: vec![],
        };
        let (obj, src) = derive_objective(&plan, &empty_checklist(), &[], "en");
        assert_eq!(obj, "Refactor auth");
        assert_eq!(src, "plan_explanation");
    }

    #[test]
    fn plan_in_progress_beats_pending() {
        let plan = PlanSnapshot {
            explanation: None,
            items: vec![
                PlanItemArg {
                    step: "Pending step".into(),
                    status: StepStatus::Pending,
                },
                PlanItemArg {
                    step: "Active step".into(),
                    status: StepStatus::InProgress,
                },
            ],
        };
        let (obj, src) = derive_objective(&plan, &empty_checklist(), &[], "en");
        assert_eq!(obj, "Active step");
        assert_eq!(src, "plan_in_progress");
    }

    #[test]
    fn zh_fallback() {
        let plan = PlanSnapshot {
            explanation: None,
            items: vec![],
        };
        let (obj, src) = derive_objective(&plan, &empty_checklist(), &[], "zh-Hans");
        assert_eq!(obj, "长程代码任务");
        assert_eq!(src, "fallback");
    }

    fn empty_checklist() -> TodoListSnapshot {
        TodoListSnapshot {
            items: vec![],
            completion_pct: 0,
            in_progress_id: None,
        }
    }
}
