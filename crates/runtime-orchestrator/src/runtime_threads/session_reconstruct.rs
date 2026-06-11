//! Turn items → session `Message` export (tools included, output truncated).

use anyhow::Result;
use serde_json::{Value, json};

use crate::models::{ContentBlock, Message};

use super::persist::RuntimeThreadStore;
use super::types::{TurnItemKind, TurnItemLifecycleStatus, TurnItemRecord, TurnRecord};
use super::{summarize_text, tool_kind_for_name};

/// Cap tool output embedded in session JSON to keep files bounded.
pub const SESSION_TOOL_OUTPUT_LIMIT: usize = 2000;

fn is_tool_item_kind(kind: TurnItemKind) -> bool {
    matches!(
        kind,
        TurnItemKind::ToolCall | TurnItemKind::FileChange | TurnItemKind::CommandExecution
    )
}

fn tool_name_from_item(item: &TurnItemRecord) -> String {
    if let Some(name) = item
        .metadata
        .as_ref()
        .and_then(|m| m.get("tool_name"))
        .and_then(|v| v.as_str())
    {
        return name.to_string();
    }
    let summary = item.summary.trim();
    if summary.ends_with(" started") {
        return summary.trim_end_matches(" started").trim().to_string();
    }
    if let Some((name, _)) = summary.split_once(':') {
        return name.trim().to_string();
    }
    match item.kind {
        TurnItemKind::FileChange => "file_change".to_string(),
        TurnItemKind::CommandExecution => "command_execution".to_string(),
        _ => "tool_call".to_string(),
    }
}

fn tool_input_from_item(item: &TurnItemRecord) -> Value {
    if let Some(input) = item.metadata.as_ref().and_then(|m| m.get("tool_input")) {
        return input.clone();
    }
    if let Some(detail) = item.detail.as_deref()
        && let Ok(v) = serde_json::from_str::<Value>(detail)
        && v.is_object()
    {
        return v;
    }
    json!({})
}

fn truncate_tool_output(text: &str) -> String {
    summarize_text(text, SESSION_TOOL_OUTPUT_LIMIT)
}

fn ensure_assistant(slot: &mut Option<Message>) {
    if slot.is_none() {
        *slot = Some(Message {
            role: "assistant".to_string(),
            content: Vec::new(),
        });
    }
}

fn flush_assistant(messages: &mut Vec<Message>, slot: &mut Option<Message>) {
    if let Some(msg) = slot.take()
        && !msg.content.is_empty()
    {
        messages.push(msg);
    }
}

fn append_text_block(msg: &mut Message, text: &str) {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return;
    }
    if let Some(ContentBlock::Text { text: existing, .. }) = msg.content.last_mut() {
        if !existing.is_empty() {
            existing.push_str("\n\n");
        }
        existing.push_str(trimmed);
        return;
    }
    msg.content.push(ContentBlock::Text {
        text: trimmed.to_string(),
        cache_control: None,
    });
}

fn append_tool_blocks(msg: &mut Message, item: &TurnItemRecord) {
    let name = tool_name_from_item(item);
    let input = tool_input_from_item(item);
    let raw_output = item.detail.as_deref().unwrap_or("");
    let output = truncate_tool_output(raw_output);
    let is_error = item.status == TurnItemLifecycleStatus::Failed;
    let tool_id = item.id.clone();

    msg.content.push(ContentBlock::ToolUse {
        id: tool_id.clone(),
        name,
        input,
        caller: None,
    });
    msg.content.push(ContentBlock::ToolResult {
        tool_use_id: tool_id,
        content: output,
        is_error: Some(is_error),
        content_blocks: None,
    });
}

/// Reconstruct chat messages for session JSON export — includes tool blocks per assistant turn.
pub fn reconstruct_messages_for_store(
    store: &RuntimeThreadStore,
    turns: &[TurnRecord],
) -> Result<Vec<Message>> {
    let mut messages = Vec::new();
    for turn in turns {
        let items = store.list_items_for_turn(&turn.id)?;
        let mut current_assistant: Option<Message> = None;

        for item in items {
            match item.kind {
                TurnItemKind::UserMessage => {
                    flush_assistant(&mut messages, &mut current_assistant);
                    let text = item.detail.unwrap_or(item.summary);
                    messages.push(Message {
                        role: "user".to_string(),
                        content: vec![ContentBlock::Text {
                            text,
                            cache_control: None,
                        }],
                    });
                }
                TurnItemKind::AgentMessage => {
                    ensure_assistant(&mut current_assistant);
                    let text = item.detail.unwrap_or(item.summary);
                    if let Some(msg) = current_assistant.as_mut() {
                        append_text_block(msg, &text);
                    }
                }
                kind if is_tool_item_kind(kind) => {
                    ensure_assistant(&mut current_assistant);
                    if let Some(msg) = current_assistant.as_mut() {
                        append_tool_blocks(msg, &item);
                    }
                }
                _ => {}
            }
        }
        flush_assistant(&mut messages, &mut current_assistant);
    }
    Ok(messages)
}

/// Map persisted tool item kind + tool name to `TurnItemKind` when seeding from session JSON.
pub fn tool_item_kind_for_seed(name: &str) -> TurnItemKind {
    tool_kind_for_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_threads::RuntimeTurnStatus;
    use crate::runtime_threads::types::TurnItemLifecycleStatus;
    use chrono::Utc;

    fn temp_store() -> (tempfile::TempDir, RuntimeThreadStore) {
        let dir = tempfile::tempdir().expect("tempdir");
        let store =
            RuntimeThreadStore::open_json_only(dir.path().to_path_buf()).expect("open store");
        (dir, store)
    }

    fn tool_item(turn_id: &str, item_id: &str, name: &str, output: &str) -> TurnItemRecord {
        TurnItemRecord {
            schema_version: 1,
            id: item_id.to_string(),
            turn_id: turn_id.to_string(),
            kind: TurnItemKind::ToolCall,
            status: TurnItemLifecycleStatus::Completed,
            summary: format!("{name}: {output}"),
            detail: Some(output.to_string()),
            metadata: Some(json!({
                "tool_name": name,
                "tool_input": {"path": "lib.rs"},
            })),
            artifact_refs: Vec::new(),
            started_at: Some(Utc::now()),
            ended_at: Some(Utc::now()),
        }
    }

    #[test]
    fn reconstruct_includes_tool_blocks_in_assistant_message() {
        let (_dir, store) = temp_store();
        let thread_id = "thr_tools";
        let turn_id = "turn_tools";

        let user = TurnItemRecord {
            schema_version: 1,
            id: "item_user".to_string(),
            turn_id: turn_id.to_string(),
            kind: TurnItemKind::UserMessage,
            status: TurnItemLifecycleStatus::Completed,
            summary: "hi".to_string(),
            detail: Some("hi".to_string()),
            metadata: None,
            artifact_refs: Vec::new(),
            started_at: Some(Utc::now()),
            ended_at: Some(Utc::now()),
        };
        let agent = TurnItemRecord {
            schema_version: 1,
            id: "item_agent".to_string(),
            turn_id: turn_id.to_string(),
            kind: TurnItemKind::AgentMessage,
            status: TurnItemLifecycleStatus::Completed,
            summary: "done".to_string(),
            detail: Some("done".to_string()),
            metadata: None,
            artifact_refs: Vec::new(),
            started_at: Some(Utc::now()),
            ended_at: Some(Utc::now()),
        };
        let tool = tool_item(turn_id, "item_tool", "read_file", "file contents");

        for item in [&user, &tool, &agent] {
            store.save_item(item).expect("save item");
        }

        let turn = TurnRecord {
            schema_version: 1,
            id: turn_id.to_string(),
            thread_id: thread_id.to_string(),
            status: RuntimeTurnStatus::Completed,
            input_summary: "hi".to_string(),
            created_at: Utc::now(),
            started_at: Some(Utc::now()),
            ended_at: Some(Utc::now()),
            duration_ms: Some(1),
            usage: None,
            last_request_input_tokens: None,
            error: None,
            item_ids: vec![user.id.clone(), tool.id.clone(), agent.id.clone()],
            steer_count: 0,
        };
        store.save_turn(&turn).expect("save turn");

        let turns = store.list_turns_for_thread(thread_id).expect("list turns");
        let messages = reconstruct_messages_for_store(&store, &turns).expect("reconstruct");
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[1].role, "assistant");
        let blocks = &messages[1].content;
        assert!(
            blocks
                .iter()
                .any(|b| matches!(b, ContentBlock::ToolUse { name, .. } if name == "read_file")),
            "expected ToolUse block"
        );
        assert!(
            blocks
                .iter()
                .any(|b| matches!(b, ContentBlock::ToolResult { content, .. } if content == "file contents")),
            "expected ToolResult block"
        );
        assert!(
            blocks
                .iter()
                .any(|b| matches!(b, ContentBlock::Text { text, .. } if text == "done")),
            "expected trailing agent text"
        );
    }
}
