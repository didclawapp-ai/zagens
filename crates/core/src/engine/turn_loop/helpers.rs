//! Pure turn-loop helpers (P2 PR4 → `zagens-core`).

use crate::chat::{ContentBlock, Message};
use crate::session::Session;
use serde_json::Value;
use std::path::Path;

/// Inject `<turn_meta>` (date + working-set summary) into the last real user message.
#[must_use]
pub fn messages_with_turn_metadata(
    session: &Session,
    workspace_for_summary: &Path,
) -> Vec<Message> {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let working_set_summary = session
        .working_set
        .summary_block(workspace_for_summary)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let summary = if let Some(working_set_summary) = working_set_summary {
        format!("Current local date: {today}\n{working_set_summary}")
    } else {
        format!("Current local date: {today}")
    };

    let mut messages = session.messages.clone();
    let Some(last_user) = messages.iter_mut().rev().find(|message| {
        message.role == "user"
            && message
                .content
                .iter()
                .all(|block| !matches!(block, ContentBlock::ToolResult { .. }))
            && message
                .content
                .iter()
                .any(|block| matches!(block, ContentBlock::Text { .. }))
    }) else {
        return messages;
    };

    let turn_meta = format!("<turn_meta>\n{summary}\n</turn_meta>");
    last_user.content.insert(
        0,
        ContentBlock::Text {
            text: turn_meta,
            cache_control: None,
        },
    );
    messages
}

/// Resolve `"auto"` reasoning effort using the host-supplied tier selector.
#[must_use]
pub fn resolve_auto_effort(
    reasoning_effort: Option<&str>,
    messages: &[Message],
    select_tier: impl FnOnce(bool, &str) -> String,
) -> Option<String> {
    match reasoning_effort {
        Some("auto") => {
            let last_msg = messages
                .iter()
                .rev()
                .find(|m| m.role == "user")
                .map(|m| {
                    m.content
                        .iter()
                        .filter_map(|block| {
                            if let ContentBlock::Text { text, .. } = block {
                                Some(text.as_str())
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<&str>>()
                        .join(" ")
                })
                .unwrap_or_default();

            Some(select_tier(false, &last_msg))
        }
        Some(other) => Some(other.to_string()),
        None => None,
    }
}

/// Human-readable approval description for `edit_file` tool calls.
#[must_use]
pub fn build_edit_file_approval_desc(input: &Value) -> String {
    let path = input.get("path").and_then(|v| v.as_str()).unwrap_or("?");
    let op = input
        .get("operation")
        .and_then(|v| v.as_str())
        .unwrap_or("search_replace");

    match op {
        "delete_lines" => {
            let start = input
                .get("start_line")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let end = input.get("end_line").and_then(|v| v.as_u64()).unwrap_or(0);
            let count = if start > 0 && end >= start {
                end - start + 1
            } else {
                0
            };
            format!("⚠️ DELETE {count} line(s) (lines {start}–{end}) in {path}")
        }
        "insert_after" => {
            let after = input
                .get("after_line")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let text = input.get("text").and_then(|v| v.as_str()).unwrap_or("");
            let preview: String = text
                .lines()
                .take(3)
                .map(|l| l.trim())
                .collect::<Vec<_>>()
                .join(" | ");
            let extra = if text.lines().count() > 3 { "…" } else { "" };
            let pos = if after == 0 {
                "beginning".to_string()
            } else {
                format!("after line {after}")
            };
            format!("Insert text at {pos} in {path}: «{preview}{extra}»")
        }
        "replace_line" => {
            let line = input.get("line").and_then(|v| v.as_u64()).unwrap_or(0);
            let text = input.get("text").and_then(|v| v.as_str()).unwrap_or("");
            let preview: String = text
                .lines()
                .take(2)
                .map(|l| l.trim())
                .collect::<Vec<_>>()
                .join(" | ");
            format!("Replace line {line} in {path} with: «{preview}»")
        }
        _ => {
            let search = input.get("search").and_then(|v| v.as_str()).unwrap_or("?");
            let replace = input.get("replace").and_then(|v| v.as_str()).unwrap_or("?");
            let search_preview: String = search
                .lines()
                .take(2)
                .map(|l| l.trim())
                .collect::<Vec<_>>()
                .join(" | ");
            let replace_preview: String = replace
                .lines()
                .take(2)
                .map(|l| l.trim())
                .collect::<Vec<_>>()
                .join(" | ");
            format!("Search/replace in {path}: «{search_preview}» → «{replace_preview}»")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn build_edit_file_approval_desc_delete_lines() {
        let desc = build_edit_file_approval_desc(&json!({
            "path": "a.rs",
            "operation": "delete_lines",
            "start_line": 2,
            "end_line": 4,
        }));
        assert!(desc.contains("DELETE 3"));
        assert!(desc.contains("a.rs"));
    }
}
