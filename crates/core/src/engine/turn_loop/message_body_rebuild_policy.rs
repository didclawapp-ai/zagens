//! Log-driven transcript rebuild — Phase 3b batch 5c.
//!
//! Reconstructs session `Message` rows from kernel events. Preview-only rows remain
//! for legacy logs; full session JSON byte parity uses `assistant_text`,
//! `ToolCallPlanned`, and `session_content` closure fields.

use std::collections::HashMap;

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
    if has_session_message_rebuild_substrate(events) {
        return rebuild_session_messages_from_events(events);
    }
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
    let combined = flatten_thread_events(turn_events);
    if !has_session_message_rebuild_substrate(&combined) {
        let index = replay_thread_transcript_preview_index(turn_events);
        if index.preview_body_event_count == 0 {
            return false;
        }
        return verify_session_transcript_preview_bodies(messages, turn_events).is_some();
    }
    verify_session_messages_structural_parity(messages, &combined).is_some()
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

#[derive(Debug, Clone)]
struct PlannedToolCall {
    call_id: String,
    tool_name: String,
    input_json: String,
}

/// Whether the log carries enough substrate for full session message rebuild.
#[must_use]
pub fn has_session_message_rebuild_substrate(events: &[KernelEvent]) -> bool {
    events.iter().any(|event| match event {
        KernelEvent::ModelMessage {
            assistant_text,
            text_preview,
            block_count,
            ..
        } if !assistant_text.is_empty() || !text_preview.is_empty() || *block_count > 0 => true,
        KernelEvent::ToolCallPlanned { .. } => true,
        KernelEvent::ToolCallFinished {
            session_content,
            result_preview,
            ..
        } if !session_content.is_empty() || !result_preview.is_empty() => true,
        KernelEvent::TurnStarted { input_text, .. } if !input_text.is_empty() => true,
        KernelEvent::SteerInjected { text, .. } if !text.is_empty() => true,
        KernelEvent::LayeredContextSeamInjected { text_preview, .. }
            if !text_preview.is_empty() =>
        {
            true
        }
        _ => false,
    })
}

fn index_tool_plans_by_step(events: &[KernelEvent]) -> HashMap<u32, Vec<PlannedToolCall>> {
    let mut out: HashMap<u32, Vec<PlannedToolCall>> = HashMap::new();
    for event in events {
        if let KernelEvent::ToolCallPlanned {
            step_idx,
            call_id,
            tool_name,
            input_json,
            ..
        } = event
        {
            out.entry(*step_idx).or_default().push(PlannedToolCall {
                call_id: call_id.clone(),
                tool_name: tool_name.clone(),
                input_json: input_json.clone(),
            });
        }
    }
    out
}

fn assistant_text_for_model_message(text_preview: &str, assistant_text: &str) -> String {
    if !assistant_text.is_empty() {
        assistant_text.to_string()
    } else {
        text_preview.to_string()
    }
}

fn rebuild_assistant_message(
    block_count: u32,
    text_preview: &str,
    assistant_text: &str,
    planned: &[PlannedToolCall],
) -> Message {
    let text = assistant_text_for_model_message(text_preview, assistant_text);
    let mut content = Vec::new();
    if !text.is_empty() {
        content.push(ContentBlock::Text {
            text: text.clone(),
            cache_control: None,
        });
    }
    let text_blocks = u32::from(!text.is_empty());
    let tool_slots = block_count.saturating_sub(text_blocks) as usize;
    for plan in planned.iter().take(tool_slots) {
        let input =
            serde_json::from_str(&plan.input_json).unwrap_or_else(|_| serde_json::json!({}));
        content.push(ContentBlock::ToolUse {
            id: plan.call_id.clone(),
            name: plan.tool_name.clone(),
            input,
            caller: None,
        });
    }
    Message {
        role: "assistant".into(),
        content,
    }
}

fn tool_result_content(
    session_content: &str,
    result_preview: &str,
    outcome: &ToolOutcome,
) -> (String, Option<bool>) {
    if !session_content.is_empty() {
        let is_error = matches!(outcome, ToolOutcome::ToolError { .. });
        return (
            session_content.to_string(),
            if is_error { Some(true) } else { None },
        );
    }
    if let ToolOutcome::ToolError { message } = outcome
        && !message.is_empty()
    {
        return (format!("Error: {message}"), Some(true));
    }
    if !result_preview.is_empty() {
        return (result_preview.to_string(), None);
    }
    (String::new(), None)
}

fn rebuild_tool_result_message(
    call_id: &str,
    session_content: &str,
    result_preview: &str,
    outcome: &ToolOutcome,
) -> Message {
    let (content, is_error) = tool_result_content(session_content, result_preview, outcome);
    Message {
        role: "user".into(),
        content: vec![ContentBlock::ToolResult {
            tool_use_id: call_id.to_string(),
            content,
            is_error,
            content_blocks: None,
        }],
    }
}

/// Rebuild full session rows from a turn/thread event log.
#[must_use]
pub fn rebuild_session_messages_from_events(events: &[KernelEvent]) -> Vec<Message> {
    let tool_plans = index_tool_plans_by_step(events);
    let mut out = Vec::new();
    let mut turn_input_emitted = false;

    for event in events {
        match event {
            KernelEvent::TurnStarted { input_text, .. }
                if !input_text.is_empty() && !turn_input_emitted =>
            {
                turn_input_emitted = true;
                out.push(Message {
                    role: "user".into(),
                    content: vec![ContentBlock::Text {
                        text: input_text.clone(),
                        cache_control: None,
                    }],
                });
            }
            KernelEvent::LayeredContextSeamInjected { text_preview, .. }
                if !text_preview.is_empty() =>
            {
                out.push(Message {
                    role: "assistant".into(),
                    content: vec![ContentBlock::Text {
                        text: text_preview.clone(),
                        cache_control: None,
                    }],
                });
            }
            KernelEvent::SteerInjected { text, .. } if !text.is_empty() => {
                out.push(Message {
                    role: "user".into(),
                    content: vec![ContentBlock::Text {
                        text: text.clone(),
                        cache_control: None,
                    }],
                });
            }
            KernelEvent::ModelMessage {
                step_idx,
                block_count,
                text_preview,
                assistant_text,
                ..
            } => {
                let planned = tool_plans.get(step_idx).map_or(&[][..], Vec::as_slice);
                out.push(rebuild_assistant_message(
                    *block_count,
                    text_preview,
                    assistant_text,
                    planned,
                ));
            }
            KernelEvent::ToolCallFinished {
                call_id,
                outcome,
                result_preview,
                session_content,
                ..
            } => {
                out.push(rebuild_tool_result_message(
                    call_id,
                    session_content,
                    result_preview,
                    outcome,
                ));
            }
            _ => {}
        }
    }
    out
}

/// Rebuild full session rows from flattened thread turn logs.
#[must_use]
pub fn rebuild_session_messages_from_thread_events(
    turn_events: &[(String, Vec<KernelEvent>)],
) -> Vec<Message> {
    rebuild_session_messages_from_events(&flatten_thread_events(turn_events))
}

/// Canonical session JSON for byte-level parity checks.
#[must_use]
pub fn canonical_session_messages_json(messages: &[Message]) -> Option<String> {
    serde_json::to_string(messages).ok()
}

/// Verify rebuilt session rows match expected session JSON bytes.
#[must_use]
pub fn verify_session_messages_byte_parity(
    rebuilt: &[Message],
    expected: &[Message],
) -> Option<String> {
    if rebuilt != expected {
        return Some(format!(
            "session message struct mismatch: rebuilt={} expected={}",
            rebuilt.len(),
            expected.len()
        ));
    }
    let rebuilt_json = canonical_session_messages_json(rebuilt)?;
    let expected_json = canonical_session_messages_json(expected)?;
    if rebuilt_json != expected_json {
        return Some(format!(
            "session JSON byte mismatch: rebuilt_bytes={} expected_bytes={}",
            rebuilt_json.len(),
            expected_json.len()
        ));
    }
    None
}

/// Verify session rows match log-rebuilt full session messages.
#[must_use]
pub fn verify_session_messages_structural_parity(
    session: &[Message],
    events: &[KernelEvent],
) -> Option<String> {
    if !has_session_message_rebuild_substrate(events) {
        return None;
    }
    let rebuilt = rebuild_session_messages_from_events(events);
    verify_session_messages_byte_parity(&rebuilt, session)
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
                assistant_text: String::new(),
            },
            KernelEvent::ToolCallFinished {
                turn_id: "t1".into(),
                call_id: "c1".into(),
                tool_name: "list_dir".into(),
                outcome: ToolOutcome::Success,
                duration_ms: 1,
                wrote_state: false,
                result_preview: "src/auth".into(),
                session_content: String::new(),
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
                    assistant_text: String::new(),
                },
                KernelEvent::ToolCallFinished {
                    turn_id: "t1".into(),
                    call_id: "c1".into(),
                    tool_name: "list_dir".into(),
                    outcome: ToolOutcome::Success,
                    duration_ms: 1,
                    wrote_state: false,
                    result_preview: "src/auth".into(),
                    session_content: String::new(),
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
                    assistant_text: String::new(),
                },
                KernelEvent::ToolCallFinished {
                    turn_id: "t1".into(),
                    call_id: "c1".into(),
                    tool_name: "list_dir".into(),
                    outcome: ToolOutcome::Success,
                    duration_ms: 1,
                    wrote_state: false,
                    result_preview: "src/auth".into(),
                    session_content: String::new(),
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
    fn rebuild_session_messages_byte_parity_from_fixture_fields() {
        let events = vec![
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
                block_count: 2,
                text_preview: "I'll inspect auth".into(),
                assistant_text: "I'll inspect auth".into(),
            },
            KernelEvent::ToolCallPlanned {
                turn_id: "t1".into(),
                step_idx: 1,
                call_id: "c1".into(),
                tool_name: "list_dir".into(),
                input_json: r#"{"path":"src/"}"#.into(),
                decision: crate::engine::kernel_event::PolicyDecision::new(false, true, true),
            },
            KernelEvent::ToolCallFinished {
                turn_id: "t1".into(),
                call_id: "c1".into(),
                tool_name: "list_dir".into(),
                outcome: ToolOutcome::Success,
                duration_ms: 1,
                wrote_state: false,
                result_preview: "src/auth".into(),
                session_content: "src/auth".into(),
            },
        ];
        let rebuilt = rebuild_session_messages_from_events(&events);
        let expected = vec![
            Message {
                role: "user".into(),
                content: vec![ContentBlock::Text {
                    text: "Fix auth".into(),
                    cache_control: None,
                }],
            },
            Message {
                role: "assistant".into(),
                content: vec![
                    ContentBlock::Text {
                        text: "I'll inspect auth".into(),
                        cache_control: None,
                    },
                    ContentBlock::ToolUse {
                        id: "c1".into(),
                        name: "list_dir".into(),
                        input: serde_json::json!({"path":"src/"}),
                        caller: None,
                    },
                ],
            },
            Message {
                role: "user".into(),
                content: vec![ContentBlock::ToolResult {
                    tool_use_id: "c1".into(),
                    content: "src/auth".into(),
                    is_error: None,
                    content_blocks: None,
                }],
            },
        ];
        assert!(verify_session_messages_byte_parity(&rebuilt, &expected).is_none());
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
                assistant_text: String::new(),
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
