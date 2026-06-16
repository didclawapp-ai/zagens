//! Log-driven transcript rebuild — Phase 3b batch 5c skeleton.
//!
//! Reconstructs a best-effort message sequence from kernel events that carry
//! text previews. Full byte parity with session JSON is a later closure step.

use crate::chat::{ContentBlock, Message};
use crate::engine::kernel_event::{KernelEvent, ToolOutcome};

/// Role of a rebuilt transcript row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RebuiltMessageRole {
    User,
    Assistant,
    ToolResult,
}

/// One row in a log-rebuilt transcript.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RebuiltTranscriptEntry {
    pub role: RebuiltMessageRole,
    pub text: String,
    pub source: &'static str,
}

/// Aggregated preview transcript counters rebuildable from kernel logs.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ThreadTranscriptPreviewIndex {
    pub preview_row_count: u32,
    pub user_preview_count: u32,
    pub assistant_preview_count: u32,
    pub tool_result_preview_count: u32,
    /// Events with non-empty `text_preview` / `result_preview` (5c gate substrate).
    pub preview_body_event_count: u32,
}

/// Flatten thread turn logs and build preview transcript index.
#[must_use]
pub fn replay_thread_transcript_preview_index(
    turn_events: &[(String, Vec<KernelEvent>)],
) -> ThreadTranscriptPreviewIndex {
    let combined: Vec<KernelEvent> = turn_events
        .iter()
        .flat_map(|(_, events)| events.iter().cloned())
        .collect();
    index_from_events(&combined)
}

fn index_from_events(events: &[KernelEvent]) -> ThreadTranscriptPreviewIndex {
    let rows = rebuild_transcript_from_events(events);
    let mut index = ThreadTranscriptPreviewIndex {
        preview_row_count: rows.len() as u32,
        ..ThreadTranscriptPreviewIndex::default()
    };
    for row in rows {
        match row.role {
            RebuiltMessageRole::User => index.user_preview_count += 1,
            RebuiltMessageRole::Assistant => index.assistant_preview_count += 1,
            RebuiltMessageRole::ToolResult => index.tool_result_preview_count += 1,
        }
    }
    index.preview_body_event_count = count_preview_body_events(events);
    index
}

fn count_preview_body_events(events: &[KernelEvent]) -> u32 {
    events
        .iter()
        .filter(|event| match event {
            KernelEvent::ModelMessage { text_preview, .. } if !text_preview.is_empty() => true,
            KernelEvent::ToolCallFinished { result_preview, .. } if !result_preview.is_empty() => {
                true
            }
            KernelEvent::LayeredContextSeamInjected { text_preview, .. }
                if !text_preview.is_empty() =>
            {
                true
            }
            _ => false,
        })
        .count() as u32
}

/// When preview bodies exist, session row count should match rebuilt preview rows.
#[must_use]
pub fn verify_session_transcript_preview_count(
    session_message_count: usize,
    index: &ThreadTranscriptPreviewIndex,
) -> Option<String> {
    if index.preview_body_event_count == 0 {
        return None;
    }
    if session_message_count as u32 != index.preview_row_count {
        return Some(format!(
            "transcript preview row mismatch: session={session_message_count} rebuilt={}",
            index.preview_row_count
        ));
    }
    None
}

/// Flatten thread turn logs for transcript rebuild.
fn flatten_thread_events(turn_events: &[(String, Vec<KernelEvent>)]) -> Vec<KernelEvent> {
    turn_events
        .iter()
        .flat_map(|(_, events)| events.iter().cloned())
        .collect()
}

/// Build minimal preview-only session rows from a rebuilt transcript.
#[must_use]
pub fn rebuild_preview_messages_from_events(events: &[KernelEvent]) -> Vec<Message> {
    rebuild_transcript_from_events(events)
        .iter()
        .map(preview_message_from_entry)
        .collect()
}

/// Build minimal preview-only session rows from a thread event log.
#[must_use]
pub fn rebuild_preview_messages_from_thread_events(
    turn_events: &[(String, Vec<KernelEvent>)],
) -> Vec<Message> {
    rebuild_preview_messages_from_events(&flatten_thread_events(turn_events))
}

fn preview_message_from_entry(entry: &RebuiltTranscriptEntry) -> Message {
    match entry.role {
        RebuiltMessageRole::User => Message {
            role: "user".into(),
            content: vec![ContentBlock::Text {
                text: entry.text.clone(),
                cache_control: None,
            }],
        },
        RebuiltMessageRole::Assistant => Message {
            role: "assistant".into(),
            content: vec![ContentBlock::Text {
                text: entry.text.clone(),
                cache_control: None,
            }],
        },
        RebuiltMessageRole::ToolResult => Message {
            role: "user".into(),
            content: vec![ContentBlock::ToolResult {
                tool_use_id: "kernel-log-preview".into(),
                content: entry.text.clone(),
                is_error: Some(entry.source == "tool_error"),
                content_blocks: None,
            }],
        },
    }
}

/// Extract preview role + text from a session row (ignores tool ids / metadata).
#[must_use]
pub fn session_message_preview(msg: &Message) -> Option<(RebuiltMessageRole, String)> {
    match msg.role.as_str() {
        "assistant" => msg.content.iter().find_map(|block| {
            if let ContentBlock::Text { text, .. } = block {
                Some((RebuiltMessageRole::Assistant, text.clone()))
            } else {
                None
            }
        }),
        "user" => {
            for block in &msg.content {
                if let ContentBlock::ToolResult { content, .. } = block {
                    return Some((RebuiltMessageRole::ToolResult, content.clone()));
                }
            }
            msg.content.iter().find_map(|block| {
                if let ContentBlock::Text { text, .. } = block {
                    Some((RebuiltMessageRole::User, text.clone()))
                } else {
                    None
                }
            })
        }
        _ => None,
    }
}

/// Whether log-driven preview repair should replace the current session transcript.
#[must_use]
pub fn should_repair_session_from_kernel_log(
    messages: &[Message],
    turn_events: &[(String, Vec<KernelEvent>)],
) -> bool {
    let index = replay_thread_transcript_preview_index(turn_events);
    if index.preview_body_event_count == 0 {
        return false;
    }
    verify_session_transcript_preview_bodies(messages, turn_events).is_some()
}

/// When preview bodies exist, verify session rows match rebuilt preview text (role + text).
#[must_use]
pub fn verify_session_transcript_preview_bodies(
    messages: &[Message],
    turn_events: &[(String, Vec<KernelEvent>)],
) -> Option<String> {
    let combined = flatten_thread_events(turn_events);
    let index = index_from_events(&combined);
    if index.preview_body_event_count == 0 {
        return None;
    }
    let rebuilt = rebuild_transcript_from_events(&combined);
    if messages.len() != rebuilt.len() {
        return Some(format!(
            "transcript preview body length mismatch: session={} rebuilt={}",
            messages.len(),
            rebuilt.len()
        ));
    }
    for (idx, (entry, msg)) in rebuilt.iter().zip(messages.iter()).enumerate() {
        let Some((role, text)) = session_message_preview(msg) else {
            return Some(format!("session[{idx}] has no preview body"));
        };
        if role != entry.role {
            return Some(format!(
                "transcript preview body[{idx}] role mismatch: session={role:?} rebuilt={:?}",
                entry.role
            ));
        }
        if text != entry.text {
            return Some(format!(
                "transcript preview body[{idx}] text mismatch: session={text:?} rebuilt={:?}",
                entry.text
            ));
        }
    }
    None
}

/// Rebuild transcript rows from a turn/thread event log (preview text only).
#[must_use]
pub fn rebuild_transcript_from_events(events: &[KernelEvent]) -> Vec<RebuiltTranscriptEntry> {
    let mut out = Vec::new();
    let mut turn_input_emitted = false;

    for event in events {
        match event {
            KernelEvent::TurnStarted { input_text, .. }
                if !input_text.is_empty() && !turn_input_emitted =>
            {
                turn_input_emitted = true;
                out.push(RebuiltTranscriptEntry {
                    role: RebuiltMessageRole::User,
                    text: input_text.clone(),
                    source: "turn_input",
                });
            }
            KernelEvent::LayeredContextSeamInjected { text_preview, .. }
                if !text_preview.is_empty() =>
            {
                out.push(RebuiltTranscriptEntry {
                    role: RebuiltMessageRole::Assistant,
                    text: text_preview.clone(),
                    source: "layered_context_seam",
                });
            }
            KernelEvent::SteerInjected { text, .. } if !text.is_empty() => {
                out.push(RebuiltTranscriptEntry {
                    role: RebuiltMessageRole::User,
                    text: text.clone(),
                    source: "steer",
                });
            }
            KernelEvent::ModelMessage { text_preview, .. } if !text_preview.is_empty() => {
                out.push(RebuiltTranscriptEntry {
                    role: RebuiltMessageRole::Assistant,
                    text: text_preview.clone(),
                    source: "model_message",
                });
            }
            KernelEvent::ToolCallFinished { result_preview, .. } if !result_preview.is_empty() => {
                out.push(RebuiltTranscriptEntry {
                    role: RebuiltMessageRole::ToolResult,
                    text: result_preview.clone(),
                    source: "tool_result",
                });
            }
            KernelEvent::ToolCallFinished {
                outcome: ToolOutcome::ToolError { message },
                ..
            } if !message.is_empty() => {
                out.push(RebuiltTranscriptEntry {
                    role: RebuiltMessageRole::ToolResult,
                    text: message.clone(),
                    source: "tool_error",
                });
            }
            _ => {}
        }
    }
    out
}

/// Verify rebuilt transcript matches documented session-direct expectations.
#[must_use]
pub fn verify_log_transcript_rebuild(
    events: &[KernelEvent],
    expected: &[(RebuiltMessageRole, &str)],
) -> Option<String> {
    let rebuilt = rebuild_transcript_from_events(events);
    if rebuilt.len() != expected.len() {
        return Some(format!(
            "transcript length mismatch: rebuilt={} expected={}",
            rebuilt.len(),
            expected.len()
        ));
    }
    for (idx, (entry, (role, text))) in rebuilt.iter().zip(expected.iter()).enumerate() {
        if entry.role != *role {
            return Some(format!(
                "transcript[{idx}] role mismatch: rebuilt={:?} expected={role:?}",
                entry.role
            ));
        }
        if entry.text != *text {
            return Some(format!(
                "transcript[{idx}] text mismatch: rebuilt={:?} expected={text:?}",
                entry.text
            ));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::kernel_event::KernelEvent;
    use crate::turn::TurnLoopMode;

    #[test]
    fn rebuild_transcript_from_preview_events() {
        let events = vec![
            KernelEvent::TurnStarted {
                turn_id: "t1".into(),
                mode: TurnLoopMode::Agent,
                input_text: "Fix auth".into(),
                max_steps: 5,
            },
            KernelEvent::SteerInjected {
                turn_id: "t1".into(),
                step_idx: 1,
                text: "check tokens".into(),
            },
            KernelEvent::ModelMessage {
                turn_id: "t1".into(),
                step_idx: 1,
                usage: crate::models::Usage::default(),
                block_count: 1,
                text_preview: "I'll inspect auth".into(),
            },
            KernelEvent::ToolCallFinished {
                turn_id: "t1".into(),
                call_id: "c1".into(),
                tool_name: "list_dir".into(),
                outcome: ToolOutcome::Success,
                duration_ms: 1,
                wrote_state: false,
                result_preview: "src/auth".into(),
            },
        ];
        let expected = [
            (RebuiltMessageRole::User, "Fix auth"),
            (RebuiltMessageRole::User, "check tokens"),
            (RebuiltMessageRole::Assistant, "I'll inspect auth"),
            (RebuiltMessageRole::ToolResult, "src/auth"),
        ];
        assert!(verify_log_transcript_rebuild(&events, &expected).is_none());
    }

    #[test]
    fn replay_thread_transcript_preview_index_counts_roles() {
        let events = vec![(
            "t1".into(),
            vec![
                KernelEvent::TurnStarted {
                    turn_id: "t1".into(),
                    mode: TurnLoopMode::Agent,
                    input_text: "Fix auth".into(),
                    max_steps: 5,
                },
                KernelEvent::ModelMessage {
                    turn_id: "t1".into(),
                    step_idx: 1,
                    usage: crate::models::Usage::default(),
                    block_count: 1,
                    text_preview: "I'll inspect auth".into(),
                },
                KernelEvent::ToolCallFinished {
                    turn_id: "t1".into(),
                    call_id: "c1".into(),
                    tool_name: "list_dir".into(),
                    outcome: ToolOutcome::Success,
                    duration_ms: 1,
                    wrote_state: false,
                    result_preview: "src/auth".into(),
                },
            ],
        )];
        let index = replay_thread_transcript_preview_index(&events);
        assert_eq!(index.preview_row_count, 3);
        assert_eq!(index.user_preview_count, 1);
        assert_eq!(index.assistant_preview_count, 1);
        assert_eq!(index.tool_result_preview_count, 1);
        assert_eq!(index.preview_body_event_count, 2);
        assert!(verify_session_transcript_preview_count(3, &index).is_none());
        assert!(verify_session_transcript_preview_count(2, &index).is_some());
    }

    #[test]
    fn rebuild_preview_messages_and_verify_session_bodies() {
        let events = vec![(
            "t1".into(),
            vec![
                KernelEvent::TurnStarted {
                    turn_id: "t1".into(),
                    mode: TurnLoopMode::Agent,
                    input_text: "Fix auth".into(),
                    max_steps: 5,
                },
                KernelEvent::SteerInjected {
                    turn_id: "t1".into(),
                    step_idx: 1,
                    text: "check tokens".into(),
                },
                KernelEvent::ModelMessage {
                    turn_id: "t1".into(),
                    step_idx: 1,
                    usage: crate::models::Usage::default(),
                    block_count: 1,
                    text_preview: "I'll inspect auth".into(),
                },
                KernelEvent::ToolCallFinished {
                    turn_id: "t1".into(),
                    call_id: "c1".into(),
                    tool_name: "list_dir".into(),
                    outcome: ToolOutcome::Success,
                    duration_ms: 1,
                    wrote_state: false,
                    result_preview: "src/auth".into(),
                },
            ],
        )];
        let mut messages = rebuild_preview_messages_from_thread_events(&events);
        assert_eq!(messages.len(), 4);
        assert!(verify_session_transcript_preview_bodies(&messages, &events).is_none());
        messages[0].content = vec![ContentBlock::Text {
            text: "wrong".into(),
            cache_control: None,
        }];
        assert!(verify_session_transcript_preview_bodies(&messages, &events).is_some());
    }

    #[test]
    fn should_repair_session_when_preview_bodies_diverge() {
        let events = vec![(
            "t1".into(),
            vec![KernelEvent::ModelMessage {
                turn_id: "t1".into(),
                step_idx: 1,
                usage: crate::models::Usage::default(),
                block_count: 1,
                text_preview: "hello".into(),
            }],
        )];
        let aligned = rebuild_preview_messages_from_thread_events(&events);
        assert!(!should_repair_session_from_kernel_log(&aligned, &events));
        let mut wrong = aligned;
        wrong[0].content = vec![ContentBlock::Text {
            text: "other".into(),
            cache_control: None,
        }];
        assert!(should_repair_session_from_kernel_log(&wrong, &events));
    }
}
