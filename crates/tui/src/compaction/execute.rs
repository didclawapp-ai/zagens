use anyhow::Result;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::config::DEFAULT_TEXT_MODEL;
use crate::llm_client::LlmClient;
use crate::logging;
use crate::models::{
    CacheControl, ContentBlock, Message, MessageRequest, SystemBlock, SystemPrompt,
    context_window_for_model,
};
use deepseek_core::compaction::CompactionConfig;

use super::plan::plan_compaction;
use super::prune::{prune_tool_results, tail_chars, truncate_chars};
use super::tokens::{estimate_tokens, should_compact};
use super::{
    CACHE_ALIGNED_SUMMARY_CONTEXT_BUDGET_PERCENT, KEEP_RECENT_MESSAGES,
    LARGE_CONTEXT_SUMMARY_INPUT_HEAD_CHARS, LARGE_CONTEXT_SUMMARY_INPUT_MAX_CHARS,
    LARGE_CONTEXT_SUMMARY_INPUT_TAIL_CHARS, LARGE_CONTEXT_SUMMARY_MAX_TOKENS,
    LARGE_CONTEXT_SUMMARY_TEXT_SNIPPET_CHARS, LARGE_CONTEXT_SUMMARY_TOOL_RESULT_SNIPPET_CHARS,
    LARGE_CONTEXT_WINDOW_TOKENS, SUMMARY_INPUT_HEAD_CHARS, SUMMARY_INPUT_MAX_CHARS,
    SUMMARY_INPUT_TAIL_CHARS, SUMMARY_TEXT_SNIPPET_CHARS, SUMMARY_TOOL_RESULT_SNIPPET_CHARS,
};

#[derive(Debug, Clone, Copy)]
pub(crate) struct SummaryInputLimits {
    pub(crate) text_snippet_chars: usize,
    pub(crate) tool_result_snippet_chars: usize,
    pub(crate) input_max_chars: usize,
    pub(crate) input_head_chars: usize,
    pub(crate) input_tail_chars: usize,
    pub(crate) max_tokens: u32,
    pub(crate) word_limit: usize,
}

pub(crate) fn summary_input_limits_for_model(model: &str) -> SummaryInputLimits {
    let is_large_context =
        context_window_for_model(model).is_some_and(|window| window >= LARGE_CONTEXT_WINDOW_TOKENS);
    if is_large_context {
        SummaryInputLimits {
            text_snippet_chars: LARGE_CONTEXT_SUMMARY_TEXT_SNIPPET_CHARS,
            tool_result_snippet_chars: LARGE_CONTEXT_SUMMARY_TOOL_RESULT_SNIPPET_CHARS,
            input_max_chars: LARGE_CONTEXT_SUMMARY_INPUT_MAX_CHARS,
            input_head_chars: LARGE_CONTEXT_SUMMARY_INPUT_HEAD_CHARS,
            input_tail_chars: LARGE_CONTEXT_SUMMARY_INPUT_TAIL_CHARS,
            max_tokens: LARGE_CONTEXT_SUMMARY_MAX_TOKENS,
            word_limit: 900,
        }
    } else {
        SummaryInputLimits {
            text_snippet_chars: SUMMARY_TEXT_SNIPPET_CHARS,
            tool_result_snippet_chars: SUMMARY_TOOL_RESULT_SNIPPET_CHARS,
            input_max_chars: SUMMARY_INPUT_MAX_CHARS,
            input_head_chars: SUMMARY_INPUT_HEAD_CHARS,
            input_tail_chars: SUMMARY_INPUT_TAIL_CHARS,
            max_tokens: 1_024,
            word_limit: 500,
        }
    }
}

pub struct CompactionResult {
    /// Compacted messages
    pub messages: Vec<Message>,
    /// Summary system prompt
    pub summary_prompt: Option<SystemPrompt>,
    /// Messages that were removed from the active window
    #[allow(dead_code)]
    pub removed_messages: Vec<Message>,
    /// Number of retries used before success
    pub retries_used: u32,
}

/// Check if an error is transient and worth retrying. Categories that map to
/// transient retry: Network, RateLimit, Timeout. Anything else (auth, parse,
/// invalid request, etc.) is permanent and propagates.
pub(crate) fn is_transient_error(e: &anyhow::Error) -> bool {
    let category = crate::error_taxonomy::classify_error_message(&e.to_string());
    matches!(
        category,
        crate::error_taxonomy::ErrorCategory::Network
            | crate::error_taxonomy::ErrorCategory::RateLimit
            | crate::error_taxonomy::ErrorCategory::Timeout
    )
}

/// Compact messages with retry and backoff for transient errors.
///
/// This function wraps `compact_messages` with retry logic to handle
/// transient network errors and rate limits. It uses exponential backoff
/// with delays of 1s, 2s, 4s between retries.
///
/// # Safety
/// - Never panics
/// - Never corrupts the original messages (returns error instead)
/// - Only retries on transient errors (network, rate limit, etc.)
pub async fn compact_messages_safe(
    client: &(dyn crate::llm_client::LlmClient),
    messages: &[Message],
    config: &CompactionConfig,
    workspace: Option<&Path>,
    external_pins: Option<&[usize]>,
    external_working_set_paths: Option<&[String]>,
) -> Result<CompactionResult> {
    const MAX_RETRIES: u32 = 3;
    const BASE_DELAY_MS: u64 = 1000;

    let mut pruned_messages = messages.to_vec();
    let pruned_bytes = prune_tool_results(&mut pruned_messages, KEEP_RECENT_MESSAGES);
    let compaction_input: &[Message] = if pruned_bytes > 0 {
        logging::info(format!(
            "Local tool-result prune saved {pruned_bytes} bytes before LLM compaction"
        ));
        let was_over_threshold = should_compact(
            messages,
            config,
            workspace,
            external_pins,
            external_working_set_paths,
        );
        let now_under_threshold = !should_compact(
            &pruned_messages,
            config,
            workspace,
            external_pins,
            external_working_set_paths,
        );
        if was_over_threshold && now_under_threshold {
            return Ok(CompactionResult {
                messages: pruned_messages,
                summary_prompt: None,
                removed_messages: Vec::new(),
                retries_used: 0,
            });
        }
        &pruned_messages
    } else {
        messages
    };

    let mut last_error: Option<anyhow::Error> = None;

    for attempt in 0..MAX_RETRIES {
        if attempt > 0 {
            // Exponential backoff: 1s, 2s, 4s
            let delay = Duration::from_millis(BASE_DELAY_MS * (1 << (attempt - 1)));
            tokio::time::sleep(delay).await;
        }

        match compact_messages(
            client,
            compaction_input,
            config,
            workspace,
            external_pins,
            external_working_set_paths,
        )
        .await
        {
            Ok((msgs, prompt, removed)) => {
                return Ok(CompactionResult {
                    messages: msgs,
                    summary_prompt: prompt,
                    removed_messages: removed,
                    retries_used: attempt,
                });
            }
            Err(e) => {
                // Only retry on transient errors
                if !is_transient_error(&e) {
                    return Err(e);
                }
                last_error = Some(e);
            }
        }
    }

    Err(last_error
        .unwrap_or_else(|| anyhow::anyhow!("Compaction failed after {MAX_RETRIES} retries")))
}

pub(crate) fn read_workspace_anchors(workspace: Option<&Path>) -> Vec<String> {
    let Some(ws) = workspace else {
        return Vec::new();
    };

    let anchors_path = ws.join(".deepseek").join("anchors.md");
    let Ok(content) = std::fs::read_to_string(anchors_path) else {
        return Vec::new();
    };

    content
        .split("\n---\n")
        .map(str::trim)
        .filter(|anchor| !anchor.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

pub(crate) fn anchor_summary_section(workspace: Option<&Path>) -> String {
    let anchors = read_workspace_anchors(workspace);
    if anchors.is_empty() {
        return String::new();
    }

    let mut section = String::from(
        "## Pinned Facts (User Anchors)\n\n\
         The following facts were explicitly anchored by the user with `/anchor`. \
         Preserve them across compaction cycles.\n\n",
    );

    for anchor in anchors {
        let _ = writeln!(section, "- {anchor}");
    }

    section.push_str("\n---\n\n");
    section
}

pub async fn compact_messages(
    client: &(dyn crate::llm_client::LlmClient),
    messages: &[Message],
    config: &CompactionConfig,
    workspace: Option<&Path>,
    external_pins: Option<&[usize]>,
    external_working_set_paths: Option<&[String]>,
) -> Result<(Vec<Message>, Option<SystemPrompt>, Vec<Message>)> {
    if messages.is_empty() {
        return Ok((Vec::new(), None, Vec::new()));
    }

    let plan = plan_compaction(
        messages,
        workspace,
        KEEP_RECENT_MESSAGES,
        external_pins,
        external_working_set_paths,
    );
    if plan.summarize_indices.is_empty() {
        return Ok((messages.to_vec(), None, Vec::new()));
    }

    let to_summarize: Vec<Message> = plan
        .summarize_indices
        .iter()
        .map(|&idx| messages[idx].clone())
        .collect();

    // Create a summary of the unpinned portion of the conversation
    let summary = create_summary(client, &to_summarize, &config.model).await?;

    // Extract workflow context (files touched, tasks in progress, etc.)
    let workflow_context = extract_workflow_context(&to_summarize, workspace);

    let anchors_section = anchor_summary_section(workspace);

    // Build new message list with enhanced summary as system block
    let summary_block = SystemBlock {
        block_type: "text".to_string(),
        text: format!(
            "{anchors_section}\
             ## 📋 Conversation Summary (Auto-Generated)\n\n\
             {summary}\n\n\
             ---\n\n\
             ## 🔍 Workflow Context\n\n\
             {workflow_context}\n\n\
             ---\n\n\
             ## 💡 What to Do Next\n\n\
             You have just resumed from a context compaction. The conversation above was summarized to save space. \
             Review the summary and workflow context, then continue helping the user with their task. \
             If you need more details about the summarized portion, ask the user to clarify.\n\n\
             ---\n\n\
             Pinned messages follow:"
        ),
        cache_control: if config.cache_summary {
            Some(CacheControl {
                cache_type: "ephemeral".to_string(),
            })
        } else {
            None
        },
    };

    let pinned_messages = messages
        .iter()
        .enumerate()
        .filter_map(|(idx, msg)| plan.pinned_indices.contains(&idx).then_some(msg.clone()))
        .collect();

    Ok((
        pinned_messages,
        Some(SystemPrompt::Blocks(vec![summary_block])),
        to_summarize,
    ))
}

pub(crate) async fn create_summary(
    client: &(dyn crate::llm_client::LlmClient),
    messages: &[Message],
    model: &str,
) -> Result<String> {
    let limits = summary_input_limits_for_model(model);
    let used_cache_aligned = should_use_cache_aligned_summary(model, messages);
    let request = if used_cache_aligned {
        build_cache_aligned_summary_request(model, messages, limits)
    } else {
        build_formatted_summary_request(model, messages, limits)
    };

    let response = client.create_message(request).await?;
    // Compaction summary calls are billed by DeepSeek; route the
    // tokens through the side-channel so the dashboard total
    // matches the website (#526).
    crate::cost_status::report(&response.model, &response.usage);

    // #584: emit one debug-level event per summary call so the
    // V4 cache-aligned win is observable post-deploy without
    // adding UI surface. The event is emitted with
    // `target = "compaction"`, so the filter is
    // `RUST_LOG=compaction=debug` (the module-path form
    // `deepseek_tui::compaction=debug` does NOT match — `EnvFilter`
    // matches the explicit target string when one is set).
    log_summary_cache_telemetry(used_cache_aligned, &response.usage);

    // Extract text from response
    let summary = response
        .content
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");

    Ok(summary)
}

/// Cache-hit percentage for a compaction summary call.
///
/// Denominator is `input_tokens` (the total prompt size), not
/// `cache_hit + cache_miss`. Some providers populate
/// `prompt_cache_hit_tokens` but not `prompt_cache_miss_tokens` — using
/// the sum as the denominator there reports an inflated 100% even when
/// most of the prompt was uncached. Anchoring on `input_tokens` matches
/// how the rest of the codebase (cost reporting, `/cache`) infers
/// missing miss counts. (#584)
pub(crate) fn summary_cache_hit_percent(cache_hit: u32, input_tokens: u32) -> f64 {
    if input_tokens > 0 {
        (f64::from(cache_hit) * 100.0) / f64::from(input_tokens)
    } else {
        0.0
    }
}

/// Emit one `tracing::debug!` event per compaction summary call so the
/// path choice (cache-aligned vs fallback) and the resulting cache-hit
/// rate are observable. Both raw token counts and the percentage are
/// included; on providers that don't return cache-token fields the
/// counts are reported as `0` and the percentage as `0.0`. (#584)
pub(crate) fn log_summary_cache_telemetry(used_cache_aligned: bool, usage: &crate::models::Usage) {
    let path = if used_cache_aligned {
        "cache_aligned"
    } else {
        "fallback"
    };
    let cache_hit = usage.prompt_cache_hit_tokens.unwrap_or(0);
    let cache_miss = usage.prompt_cache_miss_tokens.unwrap_or(0);
    let cache_hit_pct = summary_cache_hit_percent(cache_hit, usage.input_tokens);
    tracing::debug!(
        target: "compaction",
        "compaction summary call: path={} prompt_tokens={} cache_hit_tokens={} cache_miss_tokens={} cache_hit_pct={:.1}",
        path,
        usage.input_tokens,
        cache_hit,
        cache_miss,
        cache_hit_pct,
    );
}

/// Decide whether to use the cache-aligned summary path
/// ([`build_cache_aligned_summary_request`]) or the fallback
/// ([`build_formatted_summary_request`]). Returns `true` when both
/// gates hold:
///
/// 1. The model has a known large context window
///    (≥ `LARGE_CONTEXT_WINDOW_TOKENS`, currently V4-scale).
/// 2. Replaying the message prefix plus a ~512-token instruction
///    still fits within `CACHE_ALIGNED_SUMMARY_CONTEXT_BUDGET_PERCENT`
///    of that budget.
///
/// ## Why the two paths produce slightly different prompts (#584)
///
/// The two summary requests are *intentionally* framed differently:
///
/// - **Cache-aligned** replays the original `messages` verbatim
///   with `system: None` and appends the summary instruction as
///   the final `user` turn. The model sees the conversation as if
///   it were its own history. This is what lets the V4 prefix cache
///   hit on the bulk of the request (#572).
/// - **Fallback** reformats the conversation into a flat
///   `User:/Assistant:` transcript inside a single `user` message
///   and adds a "You are a helpful assistant that creates concise
///   conversation summaries." system prompt. The model sees a
///   transcript of someone else's conversation.
///
/// The empirical bar is that V4 produces equivalent summaries
/// either way; the post-#572 review noted this fork is worth
/// documenting but not yet worth unifying. The fallback's
/// external-transcript framing is also more conservative for the
/// older / smaller models the cache-aligned path explicitly
/// excludes, so dropping the system prompt would risk regressing
/// those models without a corresponding gain. If we ever want to
/// unify, land it in a separate PR backed by an A/B summary-quality
/// evaluation rather than as a drive-by cleanup.
///
/// `create_summary` emits a `tracing::debug!` event under
/// `target = "compaction"` after each call so the path choice and
/// cache-hit rate are observable post-deploy without UI surface.
pub(crate) fn should_use_cache_aligned_summary(model: &str, messages: &[Message]) -> bool {
    let Some(window) = context_window_for_model(model) else {
        return false;
    };
    if window < LARGE_CONTEXT_WINDOW_TOKENS {
        return false;
    }

    let budget = usize::try_from(window).unwrap_or(usize::MAX)
        * CACHE_ALIGNED_SUMMARY_CONTEXT_BUDGET_PERCENT
        / 100;
    let summary_prompt_tokens = 512usize;
    estimate_tokens(messages).saturating_add(summary_prompt_tokens) <= budget
}

pub(crate) fn summary_instruction(word_limit: usize) -> String {
    format!(
        "Summarize the conversation above in a concise but comprehensive way. \
         Preserve key information, decisions made, exact file paths, commands, \
         errors, and tool-result facts needed to continue the work. \
         Tool outputs may be abbreviated only when they are repetitive. \
         Keep it under {word_limit} words."
    )
}

pub(crate) fn build_cache_aligned_summary_request(
    model: &str,
    messages: &[Message],
    limits: SummaryInputLimits,
) -> MessageRequest {
    let mut request_messages = messages.to_vec();
    request_messages.push(Message {
        role: "user".to_string(),
        content: vec![ContentBlock::Text {
            text: summary_instruction(limits.word_limit),
            cache_control: None,
        }],
    });

    MessageRequest {
        model: model.to_string(),
        messages: request_messages,
        max_tokens: limits.max_tokens,
        system: None,
        tools: None,
        tool_choice: None,
        metadata: None,
        thinking: None,
        reasoning_effort: None,
        stream: Some(false),
        temperature: Some(0.3),
        top_p: None,
    }
}

pub(crate) fn build_formatted_summary_request(
    model: &str,
    messages: &[Message],
    limits: SummaryInputLimits,
) -> MessageRequest {
    // Format messages for summarization
    let mut conversation_text = String::new();
    for msg in messages {
        let role = if msg.role == "user" {
            "User"
        } else {
            "Assistant"
        };
        for block in &msg.content {
            match block {
                ContentBlock::Text { text, .. } => {
                    let snippet = truncate_chars(text, limits.text_snippet_chars);
                    let _ = write!(conversation_text, "{role}: {snippet}\n\n");
                }
                ContentBlock::ToolUse { name, .. } => {
                    let _ = write!(conversation_text, "{role}: [Used tool: {name}]\n\n");
                }
                ContentBlock::ToolResult { content, .. } => {
                    let snippet = truncate_chars(content, limits.tool_result_snippet_chars);
                    let _ = write!(conversation_text, "Tool result: {}\n\n", snippet);
                }
                ContentBlock::Thinking { .. } => {
                    // Skip thinking blocks in summary
                }
                ContentBlock::ServerToolUse { .. }
                | ContentBlock::ToolSearchToolResult { .. }
                | ContentBlock::CodeExecutionToolResult { .. } => {}
            }
        }
    }

    let conversation_chars = conversation_text.chars().count();
    if conversation_chars > limits.input_max_chars {
        let head = truncate_chars(&conversation_text, limits.input_head_chars).to_string();
        let tail = tail_chars(&conversation_text, limits.input_tail_chars);
        let omitted = conversation_chars
            .saturating_sub(head.chars().count())
            .saturating_sub(tail.chars().count());
        conversation_text =
            format!("{head}\n\n[... {omitted} characters omitted before summary ...]\n\n{tail}");
    }

    MessageRequest {
        model: model.to_string(),
        messages: vec![Message {
            role: "user".to_string(),
            content: vec![ContentBlock::Text {
                text: format!(
                    "{}\n\n---\n\n{conversation_text}",
                    summary_instruction(limits.word_limit)
                ),
                cache_control: None,
            }],
        }],
        max_tokens: limits.max_tokens,
        system: Some(SystemPrompt::Text(
            "You are a helpful assistant that creates concise conversation summaries.".to_string(),
        )),
        tools: None,
        tool_choice: None,
        metadata: None,
        thinking: None,
        reasoning_effort: None,
        stream: Some(false),
        temperature: Some(0.3),
        top_p: None,
    }
}

/// Extract workflow context from messages (files touched, tasks, etc.)
pub(crate) fn extract_workflow_context(messages: &[Message], workspace: Option<&Path>) -> String {
    let mut files_touched: Vec<String> = Vec::new();
    let mut tools_used: Vec<String> = Vec::new();
    let mut tasks_identified: Vec<String> = Vec::new();

    for msg in messages {
        for block in &msg.content {
            match block {
                ContentBlock::ToolUse { name, input, .. } => {
                    tools_used.push(name.clone());

                    // Extract file paths from tool inputs
                    if let Some(path) = extract_path_from_input(input)
                        && !files_touched.contains(&path)
                    {
                        files_touched.push(path);
                    }
                }
                ContentBlock::Text { text, .. }
                    // Look for task/todo mentions
                    if (text.contains("TODO") || text.contains("task") || text.contains("need to")) => {
                        let task = truncate_chars(text, 200).to_string();
                        if !tasks_identified.contains(&task) {
                            tasks_identified.push(task);
                        }
                    }
                _ => {}
            }
        }
    }

    let mut context = String::new();

    if !files_touched.is_empty() {
        context.push_str("**Files Modified/Read:**\n");
        for file in &files_touched {
            if let Some(ws) = workspace {
                let relative = Path::new(file)
                    .strip_prefix(ws)
                    .unwrap_or(Path::new(file))
                    .display();
                context.push_str(&format!("- `{}`\n", relative));
            } else {
                context.push_str(&format!("- `{}`\n", file));
            }
        }
        context.push('\n');
    }

    if !tools_used.is_empty() {
        context.push_str("**Tools Used:** ");
        context.push_str(&tools_used.join(", "));
        context.push_str("\n\n");
    }

    if !tasks_identified.is_empty() {
        context.push_str("**Tasks/TODOs Identified:**\n");
        for task in &tasks_identified {
            context.push_str(&format!("- {}\n", task));
        }
        context.push('\n');
    }

    if context.is_empty() {
        context.push_str("No specific workflow context detected. Continue assisting the user with their current task.\n");
    }

    context
}

/// Extract file path from tool input JSON
pub(crate) fn extract_path_from_input(input: &serde_json::Value) -> Option<String> {
    // Try common path field names
    for key in ["path", "file", "file_path", "filename"] {
        if let Some(path) = input.get(key).and_then(|v| v.as_str()) {
            return Some(path.to_string());
        }
    }

    // Try to find path in nested objects
    if let Some(obj) = input.as_object() {
        for (_, value) in obj {
            if let Some(path) = value.as_str()
                && (path.contains('/') || path.contains('\\') || path.contains('.'))
            {
                return Some(path.to_string());
            }
        }
    }

    None
}
