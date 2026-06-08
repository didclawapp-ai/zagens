//! Message-only transcript isomorphism checks (A1.4).
//!
//! Headless runtime paths (persist, compaction trim, tests) must not depend on
//! ratatui [`HistoryCell`] rendering. This module rebuilds a minimal transcript
//! view from [`Message`] alone.

use crate::models::{ContentBlock, Message};

/// Minimal transcript cell for user/assistant/system/thinking/archived blocks.
#[derive(Debug, Clone, PartialEq, Eq)]
enum TranscriptCell {
    User {
        content: String,
    },
    Assistant {
        content: String,
    },
    System {
        content: String,
    },
    Thinking {
        content: String,
    },
    ArchivedContext {
        level: u8,
        range: String,
        tokens: String,
        density: String,
        model: String,
        timestamp: String,
        summary: String,
    },
}

/// Convert API messages into transcript cells (no tool cells — those need live turn state).
#[must_use]
fn transcript_cells_from_message(msg: &Message) -> Vec<TranscriptCell> {
    let mut cells = Vec::new();

    for block in &msg.content {
        match block {
            ContentBlock::Text { text, .. } => {
                if msg.role == "assistant"
                    && let Some(archived) = parse_archived_context(text)
                {
                    cells.push(archived);
                    continue;
                }
                match msg.role.as_str() {
                    "user" => merge_text_cell(&mut cells, text, TranscriptCellKind::User),
                    "assistant" => merge_text_cell(&mut cells, text, TranscriptCellKind::Assistant),
                    "system" => merge_text_cell(&mut cells, text, TranscriptCellKind::System),
                    _ => {}
                }
            }
            ContentBlock::Thinking { thinking } => {
                if let Some(TranscriptCell::Thinking { content }) = cells.last_mut() {
                    if !content.is_empty() {
                        content.push('\n');
                    }
                    content.push_str(thinking);
                } else {
                    cells.push(TranscriptCell::Thinking {
                        content: thinking.clone(),
                    });
                }
            }
            _ => {}
        }
    }

    cells
}

#[derive(Clone, Copy)]
enum TranscriptCellKind {
    User,
    Assistant,
    System,
}

fn merge_text_cell(cells: &mut Vec<TranscriptCell>, text: &str, kind: TranscriptCellKind) {
    let merge = |content: &mut String| {
        if !content.is_empty() {
            content.push('\n');
        }
        content.push_str(text);
    };
    match kind {
        TranscriptCellKind::User => {
            if let Some(TranscriptCell::User { content }) = cells.last_mut() {
                merge(content);
            } else {
                cells.push(TranscriptCell::User {
                    content: text.to_string(),
                });
            }
        }
        TranscriptCellKind::Assistant => {
            if let Some(TranscriptCell::Assistant { content }) = cells.last_mut() {
                merge(content);
            } else {
                cells.push(TranscriptCell::Assistant {
                    content: text.to_string(),
                });
            }
        }
        TranscriptCellKind::System => {
            if let Some(TranscriptCell::System { content }) = cells.last_mut() {
                merge(content);
            } else {
                cells.push(TranscriptCell::System {
                    content: text.to_string(),
                });
            }
        }
    }
}

fn parse_archived_context(text: &str) -> Option<TranscriptCell> {
    let text = text.trim();
    if !text.starts_with("<archived_context") || !text.ends_with("</archived_context>") {
        return None;
    }

    let tag_end = text.find('>')?;
    let tag = &text[..tag_end];

    let level = archived_context_attr(tag, "level")
        .and_then(|v| v.parse::<u8>().ok())
        .unwrap_or(0);
    let range = archived_context_attr(tag, "range").unwrap_or_default();
    let tokens = archived_context_attr(tag, "tokens").unwrap_or_default();
    let density = archived_context_attr(tag, "density").unwrap_or_default();
    let model = archived_context_attr(tag, "model").unwrap_or_default();
    let timestamp = archived_context_attr(tag, "timestamp").unwrap_or_default();

    let close_tag = text.rfind("</archived_context>")?;
    let summary_start = tag_end + 1;
    let summary = text[summary_start..close_tag].trim().to_string();

    Some(TranscriptCell::ArchivedContext {
        level,
        range,
        tokens,
        density,
        model,
        timestamp,
        summary,
    })
}

fn archived_context_attr(tag: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=\"");
    let start = tag.find(&needle)? + needle.len();
    let rest = &tag[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

#[must_use]
fn rebuild_transcript_cells(messages: &[Message]) -> Vec<TranscriptCell> {
    messages
        .iter()
        .flat_map(transcript_cells_from_message)
        .collect()
}

#[must_use]
fn user_assistant_texts_from_messages(messages: &[Message]) -> Vec<String> {
    messages
        .iter()
        .flat_map(|msg| {
            msg.content.iter().filter_map(|block| match block {
                ContentBlock::Text { text, .. } => match msg.role.as_str() {
                    "user" | "assistant" => Some(text.clone()),
                    _ => None,
                },
                _ => None,
            })
        })
        .collect()
}

#[must_use]
fn user_assistant_texts_from_cells(cells: &[TranscriptCell]) -> Vec<String> {
    cells
        .iter()
        .filter_map(|cell| match cell {
            TranscriptCell::User { content } | TranscriptCell::Assistant { content } => {
                Some(content.clone())
            }
            _ => None,
        })
        .collect()
}

#[must_use]
fn thinking_texts_from_messages(messages: &[Message]) -> Vec<String> {
    messages
        .iter()
        .flat_map(|msg| {
            msg.content.iter().filter_map(|block| match block {
                ContentBlock::Thinking { thinking } => Some(thinking.clone()),
                _ => None,
            })
        })
        .collect()
}

#[must_use]
fn thinking_texts_from_cells(cells: &[TranscriptCell]) -> Vec<String> {
    cells
        .iter()
        .filter_map(|cell| match cell {
            TranscriptCell::Thinking { content } => Some(content.clone()),
            _ => None,
        })
        .collect()
}

/// Core transcript isomorphism: user/assistant + thinking (A1.4).
///
/// Tool cells require live turn state; check tool-result bodies separately when needed.
#[must_use]
pub fn history_transcript_core_matches_messages(messages: &[Message]) -> bool {
    let cells = rebuild_transcript_cells(messages);
    user_assistant_texts_from_messages(messages) == user_assistant_texts_from_cells(&cells)
        && thinking_texts_from_messages(messages) == thinking_texts_from_cells(&cells)
}

/// Tool-result bodies from API messages (includes routed `[workshop-ref: …]` synthesis).
#[must_use]
pub fn tool_result_bodies_from_messages(messages: &[Message]) -> Vec<String> {
    messages
        .iter()
        .flat_map(|msg| {
            msg.content.iter().filter_map(|block| match block {
                ContentBlock::ToolResult { content, .. } => Some(content.clone()),
                _ => None,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use zagens_core::context_partition::message_has_external_ref;

    fn msg(role: &str, text: &str) -> Message {
        Message {
            role: role.to_string(),
            content: vec![ContentBlock::Text {
                text: text.to_string(),
                cache_control: None,
            }],
        }
    }

    fn msg_with_thinking(role: &str, thinking: &str) -> Message {
        Message {
            role: role.to_string(),
            content: vec![ContentBlock::Thinking {
                thinking: thinking.to_string(),
            }],
        }
    }

    fn tool_result(content: &str) -> Message {
        Message {
            role: "tool".to_string(),
            content: vec![ContentBlock::ToolResult {
                tool_use_id: "toolu_test".to_string(),
                content: content.to_string(),
                is_error: None,
                content_blocks: None,
            }],
        }
    }

    #[test]
    fn transcript_core_matches_user_assistant_and_thinking() {
        let messages = vec![
            msg("user", "hi"),
            msg_with_thinking("assistant", "reasoning"),
            msg("assistant", "done"),
        ];
        assert!(history_transcript_core_matches_messages(&messages));
    }

    #[test]
    fn tool_result_bodies_extract_workshop_ref() {
        let ref_body = "[workshop-ref: {\"ref_id\":\"lout_hist_iso\"}]\n\nsummary body";
        let bodies = tool_result_bodies_from_messages(&[tool_result(ref_body)]);
        assert_eq!(bodies.len(), 1);
        assert!(message_has_external_ref(&bodies[0]));
    }
}
