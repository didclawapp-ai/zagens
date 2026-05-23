//! Streaming request + SSE processing for one turn step (P2 PR4 — `TurnLoopHost::run_streaming_phase`).

use std::collections::HashSet;
use std::time::{Duration, Instant};

use deepseek_core::chat::{ContentBlock, LlmClient, Message, Tool};
use deepseek_core::engine::context::summarize_text;
use deepseek_core::engine::streaming::{
    contains_fake_tool_wrapper, filter_tool_call_delta, should_transparently_retry_stream,
    ContentBlockKind, ToolUseState, FAKE_WRAPPER_NOTICE, MAX_STREAM_ERRORS_BEFORE_FAIL,
    MAX_STREAM_RETRIES, MAX_TRANSPARENT_STREAM_RETRIES, STREAM_CHUNK_TIMEOUT_SECS,
    STREAM_MAX_CONTENT_BYTES, STREAM_MAX_DURATION_SECS,
};
use deepseek_core::engine::turn_loop::{
    messages_with_turn_metadata, resolve_auto_effort, TurnLoopControl, TurnLoopHost,
    TurnLoopStreamingPhaseOutcome,
};
use deepseek_core::error_taxonomy::{ErrorEnvelope, StreamError};
use deepseek_core::turn::{TurnContext, TurnLoopMode, TurnOutcomeStatus};
use futures_util::StreamExt;
use serde_json::json;

use super::super::dispatch::{final_tool_input, parse_tool_input};
use super::super::*;
use super::Engine;
use crate::core::events::Event;
use crate::models::{ContentBlockStart, Delta, MessageRequest, StreamEvent, Usage};

pub(super) async fn run_streaming_phase(
    engine: &mut Engine,
    turn: &mut TurnContext,
    client: &dyn LlmClient,
    mode: TurnLoopMode,
    tool_catalog: &[Tool],
    active_tool_names: &HashSet<String>,
    force_update_plan_first: bool,
    stream_retry_attempts: &mut u32,
    context_recovery_attempts: &mut u8,
    turn_error: &mut Option<String>,
) -> TurnLoopStreamingPhaseOutcome {
// Build the request
let force_update_plan_this_step = force_update_plan_first && turn.tool_calls.is_empty();
let active_tools = if tool_catalog.is_empty() {
    None
} else {
    Some(TurnLoopHost::active_tools_for_step(engine, 
        &tool_catalog,
        &active_tool_names,
        force_update_plan_this_step,
    ))
};

// Resolve `auto` reasoning_effort to a concrete tier (#663).
let effective_reasoning_effort = resolve_auto_effort(
    engine.session.reasoning_effort.as_deref(),
    &engine.session.messages,
    |is_subagent, last_msg| {
        crate::auto_reasoning::select(is_subagent, last_msg)
            .as_setting()
            .to_string()
    },
);

let request = MessageRequest {
    model: engine.session.model.clone(),
    messages: messages_with_turn_metadata(
        &engine.session,
        &engine.config.workspace,
    ),
    max_tokens: effective_max_output_tokens(&engine.session.model),
    system: engine.session.system_prompt.clone(),
    tools: active_tools.clone(),
    tool_choice: if active_tools.is_some() {
        if engine.config.strict_tool_mode {
            Some(json!("required"))
        } else {
            Some(json!({ "type": "auto" }))
        }
    } else {
        None
    },
    metadata: None,
    thinking: None,
    reasoning_effort: effective_reasoning_effort,
    stream: Some(true),
    temperature: None,
    top_p: None,
};

// Stream the response. Keep the request around (cloned into the
// first call) so we can resend it on a transparent retry below
// when the wire dies before any content was streamed (#103).
let stream_request = request;
let stream_result = client.create_message_stream(stream_request.clone()).await;
let stream = match stream_result {
    Ok(s) => {
        *context_recovery_attempts = 0;
        s
    }
    Err(e) => {
        let message = engine.decorate_auth_error_message(e.to_string());
        if is_context_length_error_message(&message)
            && *context_recovery_attempts < MAX_CONTEXT_RECOVERY_ATTEMPTS
            && engine
                .recover_context_overflow(
                    client,
                    "provider context-length rejection",
                    TURN_MAX_OUTPUT_TOKENS,
                )
                .await
        {
            *context_recovery_attempts = context_recovery_attempts.saturating_add(1);
            return TurnLoopStreamingPhaseOutcome {
                continue_outer_loop: true,
                ..Default::default()
            };
        }
        *turn_error = Some(message.clone());
        let _ = engine
            .tx_event
            .send(Event::error(ErrorEnvelope::classify(message, true)))
            .await;
        return TurnLoopStreamingPhaseOutcome {
            return_early: Some((TurnOutcomeStatus::Failed, turn_error.clone())),
            ..Default::default()
        };
    }
};
// The stream value is itself `Pin<Box<dyn Stream + Send>>`, which
// is `Unpin`, so we can rebind it on a transparent retry without
// breaking the existing pin invariants.
let mut stream = stream;

// Track content blocks
let mut content_blocks: Vec<ContentBlock> = Vec::new();
let mut current_text_raw = String::new();
let mut current_text_visible = String::new();
let mut current_thinking = String::new();
let mut tool_uses: Vec<ToolUseState> = Vec::new();
let mut usage = Usage {
    input_tokens: 0,
    output_tokens: 0,
    ..Usage::default()
};
let mut current_block_kind: Option<ContentBlockKind> = None;
let mut current_tool_index: Option<usize> = None;
let mut in_tool_call_block = false;
let mut fake_wrapper_notice_emitted = false;
let mut pending_message_complete = false;
let mut last_text_index: Option<usize> = None;
let mut stream_errors = 0u32;
// #103 transparent retry bookkeeping. `any_content_received` flips
// on the first non-MessageStart event so we know whether DeepSeek
// billed us / the user has seen any output for this turn yet.
// This is distinct from the outer `stream_retry_attempts` (which
// restarts the whole turn-step when a stream died with no
// content-block delta delivered to the consumer).
let mut any_content_received = false;
let mut transparent_stream_retries = 0u32;
let mut pending_steers: Vec<String> = Vec::new();
// `stream_start` is reset on a transparent retry so the wall-clock
// budget restarts with the fresh stream.
let mut stream_start = Instant::now();
let mut stream_content_bytes: usize = 0;
let chunk_timeout = Duration::from_secs(STREAM_CHUNK_TIMEOUT_SECS);
let max_duration = Duration::from_secs(STREAM_MAX_DURATION_SECS);

// Process stream events
loop {
    let poll_outcome = tokio::select! {
        _ = engine.cancel_token.cancelled() => None,
        result = tokio::time::timeout(chunk_timeout, stream.next()) => {
            match result {
                Ok(Some(event_result)) => Some(event_result),
                Ok(None) => None, // stream ended normally
                Err(_) => {
                    let envelope = StreamError::Stall {
                        timeout_secs: STREAM_CHUNK_TIMEOUT_SECS,
                    }
                    .into_envelope();
                    crate::logging::warn(&envelope.message);
                    let _ = engine.tx_event.send(Event::error(envelope)).await;
                    None
                }
            }
        }
    };
    let Some(event_result) = poll_outcome else {
        break;
    };
    while let Ok(steer) = engine.rx_steer.try_recv() {
        let steer = steer.trim().to_string();
        if steer.is_empty() {
            continue;
        }
        pending_steers.push(steer.clone());
        let _ = engine
            .tx_event
            .send(Event::status(format!(
                "Steer input queued: {}",
                summarize_text(&steer, 120)
            )))
            .await;
    }

    if engine.cancel_token.is_cancelled() {
        break;
    }

    // Guard: max wall-clock duration
    if stream_start.elapsed() > max_duration {
        let envelope = StreamError::DurationLimit {
            limit_secs: STREAM_MAX_DURATION_SECS,
        }
        .into_envelope();
        crate::logging::warn(&envelope.message);
        turn_error.get_or_insert(envelope.message.clone());
        let _ = engine.tx_event.send(Event::error(envelope)).await;
        break;
    }

    // Guard: max accumulated content bytes
    if stream_content_bytes > STREAM_MAX_CONTENT_BYTES {
        let envelope = StreamError::Overflow {
            limit_bytes: STREAM_MAX_CONTENT_BYTES,
        }
        .into_envelope();
        crate::logging::warn(&envelope.message);
        turn_error.get_or_insert(envelope.message.clone());
        let _ = engine.tx_event.send(Event::error(envelope)).await;
        break;
    }

    let event = match event_result {
        Ok(e) => {
            // Flip on the first non-MessageStart event — that's
            // the moment we cross from "stream not yet productive"
            // (eligible for transparent retry) into "DeepSeek has
            // billed us / user has seen output" (must surface).
            if !any_content_received && !matches!(e, StreamEvent::MessageStart { .. }) {
                any_content_received = true;
            }
            e
        }
        Err(e) => {
            stream_errors = stream_errors.saturating_add(1);
            let message = engine.decorate_auth_error_message(e.to_string());
            // #103: when the stream errors before any content was
            // streamed AND we still have retry budget, transparently
            // resend the request. DeepSeek has not billed for any
            // output and the user has seen nothing — re-trying is
            // the right user-visible behavior.
            if should_transparently_retry_stream(
                any_content_received,
                transparent_stream_retries,
                engine.cancel_token.is_cancelled(),
            ) {
                transparent_stream_retries =
                    transparent_stream_retries.saturating_add(1);
                crate::logging::info(format!(
                    "Transparent stream retry {}/{} (no content received yet): {}",
                    transparent_stream_retries, MAX_TRANSPARENT_STREAM_RETRIES, message,
                ));
                // Drop the failed stream before issuing the new
                // request to release the underlying connection.
                drop(stream);
                match client.create_message_stream(stream_request.clone()).await {
                    Ok(fresh) => {
                        stream = fresh;
                        stream_start = Instant::now();
                        // Roll back the error counter — this one
                        // didn't surface to the user.
                        stream_errors = stream_errors.saturating_sub(1);
                        continue;
                    }
                    Err(retry_err) => {
                        let retry_msg = engine.decorate_auth_error_message(format!(
                            "Stream retry failed: {retry_err}"
                        ));
                        turn_error.get_or_insert(retry_msg.clone());
                        let _ = engine
                            .tx_event
                            .send(Event::error(ErrorEnvelope::classify(
                                retry_msg, true,
                            )))
                            .await;
                        break;
                    }
                }
            }
            turn_error.get_or_insert(message.clone());
            let _ = engine
                .tx_event
                .send(Event::error(ErrorEnvelope::classify(message, true)))
                .await;
            if stream_errors >= MAX_STREAM_ERRORS_BEFORE_FAIL {
                break;
            }
            continue;
        }
    };

    match event {
        StreamEvent::MessageStart { message } => {
            usage = message.usage;
        }
        StreamEvent::ContentBlockStart {
            index,
            content_block,
        } => match content_block {
            ContentBlockStart::Text { text } => {
                current_text_raw = text;
                current_text_visible.clear();
                in_tool_call_block = false;
                let filtered =
                    filter_tool_call_delta(&current_text_raw, &mut in_tool_call_block);
                if !fake_wrapper_notice_emitted
                    && filtered.len() < current_text_raw.len()
                    && contains_fake_tool_wrapper(&current_text_raw)
                {
                    let _ =
                        engine.tx_event.send(Event::status(FAKE_WRAPPER_NOTICE)).await;
                    fake_wrapper_notice_emitted = true;
                }
                current_text_visible.push_str(&filtered);
                current_block_kind = Some(ContentBlockKind::Text);
                last_text_index = Some(index as usize);
                let _ = engine
                    .tx_event
                    .send(Event::MessageStarted {
                        index: index as usize,
                    })
                    .await;
            }
            ContentBlockStart::Thinking { thinking } => {
                current_thinking = thinking;
                current_block_kind = Some(ContentBlockKind::Thinking);
                let _ = engine
                    .tx_event
                    .send(Event::ThinkingStarted {
                        index: index as usize,
                    })
                    .await;
            }
            ContentBlockStart::ToolUse {
                id,
                name,
                input,
                caller,
            } => {
                crate::logging::info(format!(
                    "Tool '{}' block start. Initial input: {:?}",
                    name, input
                ));
                current_block_kind = Some(ContentBlockKind::ToolUse);
                current_tool_index = Some(tool_uses.len());
                // ToolCallStarted is deferred to ContentBlockStop —
                // see `final_tool_input`. Emitting here would ship
                // the placeholder `{}` and the cell would render
                // `<command>` / `<file>` literals to the user.
                tool_uses.push(ToolUseState {
                    id,
                    name,
                    input,
                    caller,
                    input_buffer: String::new(),
                });
            }
            ContentBlockStart::ServerToolUse { id, name, input } => {
                crate::logging::info(format!(
                    "Server tool '{}' block start. Initial input: {:?}",
                    name, input
                ));
                current_block_kind = Some(ContentBlockKind::ToolUse);
                current_tool_index = Some(tool_uses.len());
                tool_uses.push(ToolUseState {
                    id,
                    name,
                    input,
                    caller: None,
                    input_buffer: String::new(),
                });
            }
        },
        StreamEvent::ContentBlockDelta { index, delta } => match delta {
            Delta::TextDelta { text } => {
                stream_content_bytes = stream_content_bytes.saturating_add(text.len());
                current_text_raw.push_str(&text);
                let filtered = filter_tool_call_delta(&text, &mut in_tool_call_block);
                if !fake_wrapper_notice_emitted
                    && filtered.len() < text.len()
                    && contains_fake_tool_wrapper(&text)
                {
                    let _ =
                        engine.tx_event.send(Event::status(FAKE_WRAPPER_NOTICE)).await;
                    fake_wrapper_notice_emitted = true;
                }
                if !filtered.is_empty() {
                    current_text_visible.push_str(&filtered);
                    let _ = engine
                        .tx_event
                        .send(Event::MessageDelta {
                            index: index as usize,
                            content: filtered,
                        })
                        .await;
                }
            }
            Delta::ThinkingDelta { thinking } => {
                stream_content_bytes =
                    stream_content_bytes.saturating_add(thinking.len());
                current_thinking.push_str(&thinking);
                if !thinking.is_empty() {
                    let _ = engine
                        .tx_event
                        .send(Event::ThinkingDelta {
                            index: index as usize,
                            content: thinking,
                        })
                        .await;
                }
            }
            Delta::InputJsonDelta { partial_json } => {
                if let Some(index) = current_tool_index
                    && let Some(tool_state) = tool_uses.get_mut(index)
                {
                    tool_state.input_buffer.push_str(&partial_json);
                    crate::logging::info(format!(
                        "Tool '{}' input delta: {} (buffer now: {})",
                        tool_state.name, partial_json, tool_state.input_buffer
                    ));
                    if let Some(value) = parse_tool_input(&tool_state.input_buffer) {
                        tool_state.input = value.clone();
                        crate::logging::info(format!(
                            "Tool '{}' input parsed: {:?}",
                            tool_state.name, value
                        ));
                    }
                }
            }
        },
        StreamEvent::ContentBlockStop { index } => {
            let stopped_kind = current_block_kind.take();
            match stopped_kind {
                Some(ContentBlockKind::Text) => {
                    pending_message_complete = true;
                    last_text_index = Some(index as usize);
                }
                Some(ContentBlockKind::Thinking) => {
                    let _ = engine
                        .tx_event
                        .send(Event::ThinkingComplete {
                            index: index as usize,
                        })
                        .await;
                }
                Some(ContentBlockKind::ToolUse) | None => {}
            }
            if matches!(stopped_kind, Some(ContentBlockKind::ToolUse))
                && let Some(index) = current_tool_index.take()
                && let Some(tool_state) = tool_uses.get_mut(index)
            {
                crate::logging::info(format!(
                    "Tool '{}' block stop. Buffer: '{}', Current input: {:?}",
                    tool_state.name, tool_state.input_buffer, tool_state.input
                ));
                if !tool_state.input_buffer.trim().is_empty() {
                    if let Some(value) = parse_tool_input(&tool_state.input_buffer) {
                        tool_state.input = value;
                        crate::logging::info(format!(
                            "Tool '{}' final input: {:?}",
                            tool_state.name, tool_state.input
                        ));
                    } else {
                        crate::logging::warn(format!(
                            "Tool '{}' failed to parse final input buffer: '{}'",
                            tool_state.name, tool_state.input_buffer
                        ));
                        let _ = engine
                            .tx_event
                            .send(Event::status(format!(
                                "⚠ Tool '{}' received malformed arguments from model",
                                tool_state.name
                            )))
                            .await;
                    }
                } else {
                    crate::logging::warn(format!(
                        "Tool '{}' input buffer is empty, using initial input: {:?}",
                        tool_state.name, tool_state.input
                    ));
                }

                // Now that the input is finalized, announce the
                // tool call to the UI. Deferring to here is what
                // keeps the cell from rendering `<command>` /
                // `<file>` placeholders during the brief window
                // between block start and the last InputJsonDelta.
                let _ = engine
                    .tx_event
                    .send(Event::ToolCallStarted {
                        id: tool_state.id.clone(),
                        name: tool_state.name.clone(),
                        input: final_tool_input(tool_state),
                    })
                    .await;
            }
        }
        StreamEvent::MessageDelta {
            usage: delta_usage, ..
        } => {
            if let Some(u) = delta_usage {
                usage = u;
            }
        }
        StreamEvent::MessageStop | StreamEvent::Ping => {}
    }
}

// #103 Phase 3 — transparent retry. The inner loop above bails
// when reqwest yields chunk decode errors three times in a row;
// most of the time those are recoverable proxy / HTTP/2 issues
// and the request can simply be re-issued. Re-issue silently up
// to MAX_STREAM_RETRIES, but only when the stream produced
// nothing actionable — if any tool call landed or text was
// streamed, ship the partial state to the rest of the turn
// pipeline so we don't double-bill the user by re-running it.
let stream_died_with_nothing = stream_errors > 0
    && tool_uses.is_empty()
    && current_text_visible.trim().is_empty()
    && current_thinking.trim().is_empty()
    && !pending_message_complete;
if stream_died_with_nothing {
    if *stream_retry_attempts < MAX_STREAM_RETRIES {
        *stream_retry_attempts = stream_retry_attempts.saturating_add(1);
        crate::logging::warn(format!(
            "Stream died with no content (attempt {}/{}); retrying request",
            stream_retry_attempts, MAX_STREAM_RETRIES
        ));
        let _ = engine
            .tx_event
            .send(Event::status(format!(
                "Connection interrupted; retrying ({}/{})",
                stream_retry_attempts, MAX_STREAM_RETRIES
            )))
            .await;
        // Don't preserve the per-stream `turn_error` — we're
        // about to retry, and a successful retry should not
        // surface the transient error as the turn outcome.
        *turn_error = None;
        return TurnLoopStreamingPhaseOutcome {
            continue_outer_loop: true,
            ..Default::default()
        };
    }
    crate::logging::warn(format!(
        "Stream retry budget exhausted ({} attempts); failing turn",
        stream_retry_attempts
    ));
} else if stream_errors == 0 {
    // Healthy round → reset retry budget so we don't carry over
    // state from a previous bad round.
    *stream_retry_attempts = 0;
}

// Update turn usage
turn.add_usage(&usage);
engine.session.record_api_round_usage(&usage);

// Build content blocks. If this assistant turn produced tool
// calls, ensure a Thinking block is present even when the model
// didn't stream any reasoning text — DeepSeek's thinking-mode
// API requires `reasoning_content` to accompany every tool-call
// assistant message in the conversation history. Saving a
// placeholder here keeps the on-disk session structurally
// correct so subsequent requests won't 400.
let needs_thinking_block =
    !tool_uses.is_empty() || tool_parser::has_tool_call_markers(&current_text_raw);
let thinking_to_persist = if !current_thinking.is_empty() {
    Some(current_thinking.clone())
} else if needs_thinking_block {
    Some(String::from("(reasoning omitted)"))
} else {
    None
};
if let Some(thinking) = thinking_to_persist {
    content_blocks.push(ContentBlock::Thinking { thinking });
}
let mut final_text = current_text_visible.clone();
if tool_uses.is_empty() && tool_parser::has_tool_call_markers(&current_text_raw) {
    let parsed = tool_parser::parse_tool_calls(&current_text_raw);
    final_text = parsed.clean_text;
    for call in parsed.tool_calls {
        let _ = engine
            .tx_event
            .send(Event::ToolCallStarted {
                id: call.id.clone(),
                name: call.name.clone(),
                input: call.args.clone(),
            })
            .await;
        tool_uses.push(ToolUseState {
            id: call.id,
            name: call.name,
            input: call.args,
            caller: None,
            input_buffer: String::new(),
        });
    }
}

if !final_text.is_empty() {
    content_blocks.push(ContentBlock::Text {
        text: final_text,
        cache_control: None,
    });
}
for tool in &tool_uses {
    content_blocks.push(ContentBlock::ToolUse {
        id: tool.id.clone(),
        name: tool.name.clone(),
        input: tool.input.clone(),
        caller: tool.caller.clone(),
    });
}

if pending_message_complete {
    let index = last_text_index.unwrap_or(0);
    let _ = engine.tx_event.send(Event::MessageComplete { index }).await;
}

// RLM is a structured tool call (`rlm_query`) handled by the
// normal tool dispatch path; inline ```repl blocks (paper §2)
// are executed below when tool_uses is empty.
// DeepSeek chat API rejects assistant messages that contain only
// Keep thinking for UI stream events, but persist only sendable
// assistant turns in the conversation state.
let has_sendable_assistant_content = content_blocks.iter().any(|block| {
    matches!(
        block,
        ContentBlock::Text { .. } | ContentBlock::ToolUse { .. }
    )
});

// Add assistant message to session
if has_sendable_assistant_content {
    engine.add_session_message(Message {
        role: "assistant".to_string(),
        content: content_blocks,
    })
    .await;
}

if tool_uses.is_empty() {
    match TurnLoopHost::handle_no_tool_uses(
        engine,
        turn,
        &mut pending_steers,
        &current_text_visible,
        has_sendable_assistant_content,
    )
    .await
    {
        TurnLoopControl::Continue => {
            return TurnLoopStreamingPhaseOutcome {
                pending_steers,
                continue_outer_loop: true,
                ..Default::default()
            };
        }
        TurnLoopControl::Break => {
            return TurnLoopStreamingPhaseOutcome {
                pending_steers,
                break_outer_loop: true,
                ..Default::default()
            };
        }
        TurnLoopControl::Return(status, err) => {
            return TurnLoopStreamingPhaseOutcome {
                pending_steers,
                return_early: Some((status, err)),
                ..Default::default()
            };
        }
    }
}

    TurnLoopStreamingPhaseOutcome {
        tool_uses,
        pending_steers,
        ..Default::default()
    }
}
