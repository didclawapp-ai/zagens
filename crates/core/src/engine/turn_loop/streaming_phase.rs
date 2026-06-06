//! Streaming request + SSE processing for one turn step (P2 PR6 — `deepseek-core::engine::turn_loop`).

use std::collections::HashSet;
use std::time::{Duration, Instant};

use super::control::{TurnLoopControl, TurnLoopStreamingPhaseOutcome};
use super::helpers::messages_with_turn_metadata;
use super::host::TurnLoopHost;
use crate::chat::{ContentBlock, LlmClient, Message, Tool};
use crate::engine::context::{
    MAX_CONTEXT_RECOVERY_ATTEMPTS, TURN_MAX_OUTPUT_TOKENS, effective_max_output_tokens,
    is_context_length_error_message, summarize_text,
};
use crate::engine::streaming::{
    ContentBlockKind, FAKE_WRAPPER_NOTICE, MAX_LENGTH_CONTINUATIONS, MAX_STREAM_ERRORS_BEFORE_FAIL,
    MAX_STREAM_RETRIES, MAX_TRANSPARENT_STREAM_RETRIES, STREAM_CHUNK_TIMEOUT_SECS,
    STREAM_MAX_CONTENT_BYTES, STREAM_MAX_DURATION_SECS, ToolUseState, contains_fake_tool_wrapper,
    filter_tool_call_delta, should_transparently_retry_stream,
};
use crate::engine::tool_parser;
use crate::error_taxonomy::{ErrorEnvelope, StreamError, is_stream_failure_retryable};
use crate::events::Event;
use crate::turn::{TurnContext, TurnLoopMode, TurnOutcomeStatus};
use futures_util::StreamExt;
use serde_json::json;

use crate::chat::{ContentBlockStart, Delta, MessageRequest, StreamEvent};
use crate::models::Usage;

pub async fn run_streaming_phase<H: TurnLoopHost>(
    host: &mut H,
    turn: &mut TurnContext,
    client: &dyn LlmClient,
    _mode: TurnLoopMode,
    tool_catalog: &[Tool],
    active_tool_names: &HashSet<String>,
    force_update_plan_first: bool,
    stream_retry_attempts: &mut u32,
    context_recovery_attempts: &mut u8,
    length_continuations: &mut u32,
    turn_error: &mut Option<String>,
) -> TurnLoopStreamingPhaseOutcome {
    // Build the request
    let force_update_plan_this_step = force_update_plan_first && turn.tool_calls.is_empty();
    let active_tools = if tool_catalog.is_empty() {
        None
    } else {
        Some(host.active_tools_for_step(
            &tool_catalog,
            &active_tool_names,
            force_update_plan_this_step,
        ))
    };

    // Resolve `auto` reasoning_effort to a concrete tier (#663) via L2 host hook.
    let effective_reasoning_effort = host.effective_reasoning_effort_for_request();
    let workspace = host.workspace().to_path_buf();
    let strict_tool_mode = host.strict_tool_mode();
    let request = {
        let session = host.session_mut();
        MessageRequest {
            model: session.model.clone(),
            messages: messages_with_turn_metadata(session, &workspace),
            max_tokens: session
                .max_output_tokens
                .unwrap_or_else(|| effective_max_output_tokens(&session.model)),
            system: session.system_prompt.clone(),
            tools: active_tools.clone(),
            tool_choice: if active_tools.is_some() {
                if strict_tool_mode {
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
            temperature: session.temperature,
            top_p: session.top_p,
        }
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
            let message = host.decorate_auth_error_message(e.to_string());
            if is_context_length_error_message(&message)
                && *context_recovery_attempts < MAX_CONTEXT_RECOVERY_ATTEMPTS
                && host
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
            let _ = host
                .tx_event()
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
    // Stream-truncation probe: why did this stream loop end? Written to stderr
    // (→ sidecar.log) on loop exit. `upstream_eof` = provider closed the
    // connection (the suspected idle-close after backpressure); `chunk_timeout`
    // = 90s with no chunk; `cancelled` = client/turn cancel.
    let mut stream_end_reason = "stream_event_break";
    // Last `finish_reason` the provider sent before closing (e.g. "length" =
    // hit token cap, "stop" = model decided done, "tool_calls"). `None` at EOF
    // means the connection closed with no finish marker (infra/duration cut).
    let mut last_stop_reason: Option<String> = None;
    let loop_t0 = Instant::now();

    // Process stream events
    loop {
        let poll_outcome = tokio::select! {
            _ = host.cancel_token().cancelled() => {
                stream_end_reason = "cancelled";
                None
            }
            result = tokio::time::timeout(chunk_timeout, stream.next()) => {
                match result {
                    Ok(Some(event_result)) => Some(event_result),
                    Ok(None) => {
                        // Stream ended: provider closed the connection. This is the
                        // signature of the idle-close truncation (no error, no body).
                        stream_end_reason = "upstream_eof";
                        None
                    }
                    Err(_) => {
                        let envelope = StreamError::Stall {
                            timeout_secs: STREAM_CHUNK_TIMEOUT_SECS,
                        }
                        .into_envelope();
                        stream_end_reason = "chunk_timeout";
                        eprintln!(
                            "[stream-probe] chunk_timeout after {}s idle: {}",
                            STREAM_CHUNK_TIMEOUT_SECS, envelope.message
                        );
                        let _ = host.tx_event().send(Event::error(envelope)).await;
                        None
                    }
                }
            }
        };
        let Some(event_result) = poll_outcome else {
            break;
        };
        while let Ok(steer) = host.rx_steer_mut().try_recv() {
            let steer = steer.trim().to_string();
            if steer.is_empty() {
                continue;
            }
            pending_steers.push(steer.clone());
            let _ = host
                .tx_event()
                .send(Event::status(format!(
                    "Steer input queued: {}",
                    summarize_text(&steer, 120)
                )))
                .await;
        }

        if host.cancel_token().is_cancelled() {
            break;
        }

        // Guard: max wall-clock duration
        if stream_start.elapsed() > max_duration {
            let envelope = StreamError::DurationLimit {
                limit_secs: STREAM_MAX_DURATION_SECS,
            }
            .into_envelope();
            tracing::warn!("{}", envelope.message);
            turn_error.get_or_insert(envelope.message.clone());
            let _ = host.tx_event().send(Event::error(envelope)).await;
            break;
        }

        // Guard: max accumulated content bytes
        if stream_content_bytes > STREAM_MAX_CONTENT_BYTES {
            let envelope = StreamError::Overflow {
                limit_bytes: STREAM_MAX_CONTENT_BYTES,
            }
            .into_envelope();
            tracing::warn!("{}", envelope.message);
            turn_error.get_or_insert(envelope.message.clone());
            let _ = host.tx_event().send(Event::error(envelope)).await;
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
                let message = host.decorate_auth_error_message(e.to_string());
                // #103: when the stream errors before any content was
                // streamed AND we still have retry budget, transparently
                // resend the request. DeepSeek has not billed for any
                // output and the user has seen nothing — re-trying is
                // the right user-visible behavior.
                if should_transparently_retry_stream(
                    any_content_received,
                    transparent_stream_retries,
                    host.cancel_token().is_cancelled(),
                ) && is_stream_failure_retryable(&message)
                {
                    transparent_stream_retries = transparent_stream_retries.saturating_add(1);
                    tracing::info!(
                        "Transparent stream retry {}/{} (no content received yet): {}",
                        transparent_stream_retries,
                        MAX_TRANSPARENT_STREAM_RETRIES,
                        message,
                    );
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
                            let retry_msg = host.decorate_auth_error_message(format!(
                                "Stream retry failed: {retry_err}"
                            ));
                            turn_error.get_or_insert(retry_msg.clone());
                            let _ = host
                                .tx_event()
                                .send(Event::error(ErrorEnvelope::classify(retry_msg, true)))
                                .await;
                            break;
                        }
                    }
                }
                turn_error.get_or_insert(message.clone());
                let _ = host
                    .tx_event()
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
                        let _ = host
                            .tx_event()
                            .send(Event::status(FAKE_WRAPPER_NOTICE))
                            .await;
                        fake_wrapper_notice_emitted = true;
                    }
                    current_text_visible.push_str(&filtered);
                    current_block_kind = Some(ContentBlockKind::Text);
                    last_text_index = Some(index as usize);
                    let _ = host
                        .tx_event()
                        .send(Event::MessageStarted {
                            index: index as usize,
                        })
                        .await;
                }
                ContentBlockStart::Thinking { thinking } => {
                    current_thinking = thinking;
                    current_block_kind = Some(ContentBlockKind::Thinking);
                    let _ = host
                        .tx_event()
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
                    tracing::info!("Tool '{}' block start. Initial input: {:?}", name, input);
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
                    tracing::info!(
                        "Server tool '{}' block start. Initial input: {:?}",
                        name,
                        input
                    );
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
                        let _ = host
                            .tx_event()
                            .send(Event::status(FAKE_WRAPPER_NOTICE))
                            .await;
                        fake_wrapper_notice_emitted = true;
                    }
                    if !filtered.is_empty() {
                        current_text_visible.push_str(&filtered);
                        let _ = host
                            .tx_event()
                            .send(Event::MessageDelta {
                                index: index as usize,
                                content: filtered,
                            })
                            .await;
                    }
                }
                Delta::ThinkingDelta { thinking } => {
                    stream_content_bytes = stream_content_bytes.saturating_add(thinking.len());
                    current_thinking.push_str(&thinking);
                    if !thinking.is_empty() {
                        // Backpressure probe (stream-truncation investigation): the
                        // event channel is bounded (256). When the monitor drains
                        // slower than the model streams reasoning (per-delta DB
                        // write under a global lock), this `.await` blocks — and
                        // while blocked the loop stops polling the upstream HTTP
                        // stream, which can let the provider idle-close the socket.
                        let send_t0 = Instant::now();
                        let _ = host
                            .tx_event()
                            .send(Event::ThinkingDelta {
                                index: index as usize,
                                content: thinking,
                            })
                            .await;
                        let send_ms = send_t0.elapsed().as_millis() as u64;
                        if send_ms >= 50 {
                            eprintln!(
                                "[stream-probe] engine ThinkingDelta send blocked {send_ms}ms on bounded event channel (backpressure stalls upstream read)"
                            );
                        }
                    }
                }
                Delta::InputJsonDelta { partial_json } => {
                    if let Some(index) = current_tool_index
                        && let Some(tool_state) = tool_uses.get_mut(index)
                    {
                        tool_state.input_buffer.push_str(&partial_json);
                        tracing::info!(
                            "Tool '{}' input delta: {} (buffer now: {})",
                            tool_state.name,
                            partial_json,
                            tool_state.input_buffer
                        );
                        if let Some(value) =
                            host.parse_streaming_tool_input(&tool_state.input_buffer)
                        {
                            tool_state.input = value.clone();
                            tracing::info!("Tool '{}' input parsed: {:?}", tool_state.name, value);
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
                        let _ = host
                            .tx_event()
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
                    tracing::info!(
                        "Tool '{}' block stop. Buffer: '{}', Current input: {:?}",
                        tool_state.name,
                        tool_state.input_buffer,
                        tool_state.input
                    );
                    if !tool_state.input_buffer.trim().is_empty() {
                        if let Some(value) =
                            host.parse_streaming_tool_input(&tool_state.input_buffer)
                        {
                            tool_state.input = value;
                            tracing::info!(
                                "Tool '{}' final input: {:?}",
                                tool_state.name,
                                tool_state.input
                            );
                        } else {
                            tracing::warn!(
                                "Tool '{}' failed to parse final input buffer: '{}'",
                                tool_state.name,
                                tool_state.input_buffer
                            );
                            let _ = host
                                .tx_event()
                                .send(Event::status(format!(
                                    "⚠ Tool '{}' received malformed arguments from model",
                                    tool_state.name
                                )))
                                .await;
                        }
                    } else {
                        tracing::warn!(
                            "Tool '{}' input buffer is empty, using initial input: {:?}",
                            tool_state.name,
                            tool_state.input
                        );
                    }

                    // Now that the input is finalized, announce the
                    // tool call to the UI. Deferring to here is what
                    // keeps the cell from rendering `<command>` /
                    // `<file>` placeholders during the brief window
                    // between block start and the last InputJsonDelta.
                    let _ = host
                        .tx_event()
                        .send(Event::ToolCallStarted {
                            id: tool_state.id.clone(),
                            name: tool_state.name.clone(),
                            input: host.final_streaming_tool_input(tool_state),
                        })
                        .await;
                }
            }
            StreamEvent::MessageDelta {
                delta: msg_delta,
                usage: delta_usage,
            } => {
                if let Some(reason) = &msg_delta.stop_reason {
                    last_stop_reason = Some(reason.clone());
                }
                if let Some(u) = delta_usage {
                    usage = u;
                }
            }
            StreamEvent::MessageStop | StreamEvent::Ping => {}
        }
    }

    // Stream-truncation probe: summarize how this stream ended and what it
    // produced. `upstream_eof` with non-empty thinking + empty text/tools is the
    // empty-body `Completed` truncation signature.
    eprintln!(
        "[stream-probe] stream ended reason={stream_end_reason} stop_reason={last_stop_reason:?} elapsed_ms={} thinking_bytes={} text_bytes={} tool_uses={} out_tokens={} max_tokens={} stream_errors={} pending_msg={} transparent_retries={}",
        loop_t0.elapsed().as_millis(),
        current_thinking.len(),
        current_text_visible.len(),
        tool_uses.len(),
        usage.output_tokens,
        stream_request.max_tokens,
        stream_errors,
        pending_message_complete,
        transparent_stream_retries,
    );

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
        let outer_retry_ok = turn_error
            .as_deref()
            .map(is_stream_failure_retryable)
            .unwrap_or(true);
        if outer_retry_ok && *stream_retry_attempts < MAX_STREAM_RETRIES {
            *stream_retry_attempts = stream_retry_attempts.saturating_add(1);
            tracing::warn!(
                "Stream died with no content (attempt {}/{}); retrying request",
                stream_retry_attempts,
                MAX_STREAM_RETRIES
            );
            let _ = host
                .tx_event()
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
        tracing::warn!(
            "Stream retry budget exhausted ({} attempts); failing turn",
            stream_retry_attempts
        );
    } else if stream_errors == 0 {
        // Healthy round → reset retry budget so we don't carry over
        // state from a previous bad round.
        *stream_retry_attempts = 0;
    }

    // Update turn usage
    turn.add_usage(&usage);
    host.session_mut().record_api_round_usage(&usage);

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
            let _ = host
                .tx_event()
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
        let _ = host.tx_event().send(Event::MessageComplete { index }).await;
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
        host.add_session_message(Message {
            role: "assistant".to_string(),
            content: content_blocks,
        })
        .await;
    }

    // Length-truncation auto-recovery. The provider sets `finish_reason=length`
    // when the model hit the output `max_tokens` cap mid-output (a very long
    // answer, or — before the 384K default — reasoning that ate the whole
    // budget). Reset the consecutive-continuation counter on any other ending so
    // occasional cuts across a long task don't accumulate toward the cap.
    let truncated_by_length = last_stop_reason.as_deref() == Some("length");
    if !truncated_by_length {
        *length_continuations = 0;
    }

    if tool_uses.is_empty() {
        // When length-truncated with NO tool call to carry the turn forward,
        // ending here would surface as a truncated / empty-body `Completed` turn —
        // the worst failure for a user mid–large task. Instead persist what we
        // have, inject a continuation hint, and re-issue, bounded by
        // MAX_LENGTH_CONTINUATIONS so a pathological cut→continue loop can't run
        // away. (A length cut WITH tool calls self-recovers: the tools execute and
        // the next step continues, so we only special-case the empty-tool path.)
        if truncated_by_length && *length_continuations < MAX_LENGTH_CONTINUATIONS {
            *length_continuations = length_continuations.saturating_add(1);
            if !has_sendable_assistant_content {
                // No assistant turn was persisted for a reasoning-only truncation;
                // add a short placeholder so role alternation stays valid and the
                // model gets a breadcrumb that its previous attempt was cut.
                host.add_session_message(Message {
                    role: "assistant".to_string(),
                    content: vec![ContentBlock::Text {
                        text: "(上一轮回复因达到输出长度上限被中断)".to_string(),
                        cache_control: None,
                    }],
                })
                .await;
            }
            let hint = if has_sendable_assistant_content {
                "[系统] 你上一条回复因达到输出长度上限被截断。请从中断处继续输出剩余内容，不要重复或重写已经输出的部分。"
            } else {
                "[系统] 你上一轮思考因达到输出长度上限被截断，尚未产出任何回复。请基于已有分析直接给出结论或下一步操作，并精简思考过程。"
            };
            host.add_session_message(Message {
                role: "user".to_string(),
                content: vec![ContentBlock::Text {
                    text: hint.to_string(),
                    cache_control: None,
                }],
            })
            .await;
            let _ = host
                .tx_event()
                .send(Event::status(format!(
                    "Output hit the length limit; continuing automatically ({}/{})",
                    *length_continuations, MAX_LENGTH_CONTINUATIONS
                )))
                .await;
            eprintln!(
                "[stream-probe] length-truncation auto-continue {}/{} had_text={}",
                *length_continuations, MAX_LENGTH_CONTINUATIONS, has_sendable_assistant_content
            );
            turn.next_step();
            return TurnLoopStreamingPhaseOutcome {
                pending_steers,
                continue_outer_loop: true,
                ..Default::default()
            };
        }

        match host
            .handle_no_tool_uses(
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
