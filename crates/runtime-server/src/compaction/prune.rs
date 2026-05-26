use std::collections::HashMap;

use crate::models::{ContentBlock, Message};

use super::SUMMARY_TOOL_RESULT_SNIPPET_CHARS;

pub(crate) fn truncate_chars(text: &str, max_chars: usize) -> &str {
    if max_chars == 0 {
        return "";
    }
    match text.char_indices().nth(max_chars) {
        Some((idx, _)) => &text[..idx],
        None => text,
    }
}

pub(crate) fn tail_chars(text: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let total_chars = text.chars().count();
    if total_chars <= max_chars {
        return text.to_string();
    }
    let start_char = total_chars.saturating_sub(max_chars);
    let start_idx = text
        .char_indices()
        .nth(start_char)
        .map_or(0, |(idx, _)| idx);
    text[start_idx..].to_string()
}

#[derive(Debug, Clone)]
pub(crate) struct ToolUseInfo {
    name: String,
    key: String,
    args_preview: String,
}

pub(crate) fn tool_use_key(name: &str, input: &serde_json::Value) -> String {
    format!(
        "{name}:{}",
        serde_json::to_string(input).unwrap_or_else(|_| input.to_string())
    )
}

pub(crate) fn tool_args_preview(input: &serde_json::Value) -> String {
    let raw = serde_json::to_string(input).unwrap_or_else(|_| input.to_string());
    truncate_chars(&raw, 120).to_string()
}

pub(crate) fn collect_tool_uses(messages: &[Message]) -> HashMap<String, ToolUseInfo> {
    let mut tool_uses = HashMap::new();
    for message in messages {
        for block in &message.content {
            if let ContentBlock::ToolUse {
                id, name, input, ..
            } = block
            {
                tool_uses.insert(
                    id.clone(),
                    ToolUseInfo {
                        name: name.clone(),
                        key: tool_use_key(name, input),
                        args_preview: tool_args_preview(input),
                    },
                );
            }
        }
    }
    tool_uses
}

struct ToolResultPruneCandidate {
    message_idx: usize,
    block_idx: usize,
    key: String,
    tool_name: String,
    args_preview: String,
    original_len: usize,
}

/// Mechanically prune old verbose tool results before paying for an LLM summary.
///
/// The most recent `protected_window` messages stay byte-for-byte intact. Older
/// duplicate tool results keep the freshest full body and replace earlier
/// copies with one-line summaries; non-duplicate old results are summarized only
/// when they exceed the normal summary snippet size.
pub fn prune_tool_results(messages: &mut [Message], protected_window: usize) -> usize {
    let cutoff = messages.len().saturating_sub(protected_window);
    if cutoff == 0 {
        return 0;
    }

    let tool_uses = collect_tool_uses(messages);
    let mut candidates = Vec::new();
    let mut latest_by_key: HashMap<String, usize> = HashMap::new();
    let mut count_by_key: HashMap<String, usize> = HashMap::new();

    for (message_idx, message) in messages.iter().take(cutoff).enumerate() {
        for (block_idx, block) in message.content.iter().enumerate() {
            let ContentBlock::ToolResult {
                tool_use_id,
                content,
                ..
            } = block
            else {
                continue;
            };
            let Some(info) = tool_uses.get(tool_use_id) else {
                continue;
            };
            latest_by_key.insert(info.key.clone(), message_idx);
            *count_by_key.entry(info.key.clone()).or_insert(0) += 1;
            candidates.push(ToolResultPruneCandidate {
                message_idx,
                block_idx,
                key: info.key.clone(),
                tool_name: info.name.clone(),
                args_preview: info.args_preview.clone(),
                original_len: content.len(),
            });
        }
    }

    let mut bytes_saved = 0usize;
    for candidate in candidates {
        let duplicate_count = count_by_key.get(&candidate.key).copied().unwrap_or(0);
        let is_latest_duplicate = duplicate_count > 1
            && latest_by_key.get(&candidate.key) == Some(&candidate.message_idx);
        if is_latest_duplicate {
            continue;
        }
        if duplicate_count <= 1 && candidate.original_len <= SUMMARY_TOOL_RESULT_SNIPPET_CHARS {
            continue;
        }

        let summary = format!(
            "[{}] tool result pruned ({} bytes; args: {})",
            candidate.tool_name, candidate.original_len, candidate.args_preview
        );
        if summary.len() >= candidate.original_len {
            continue;
        }

        if let ContentBlock::ToolResult {
            content,
            content_blocks,
            ..
        } = &mut messages[candidate.message_idx].content[candidate.block_idx]
        {
            bytes_saved = bytes_saved.saturating_add(content.len().saturating_sub(summary.len()));
            *content = summary;
            *content_blocks = None;
        }
    }

    bytes_saved
}
