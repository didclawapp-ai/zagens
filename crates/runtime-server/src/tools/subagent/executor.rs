use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use futures_util::FutureExt;
use serde_json::json;
use tokio::sync::{Mutex, mpsc};

use deepseek_core::events::Event;
use crate::models::{ContentBlock, Message, MessageRequest, SystemPrompt};
use crate::utils::write_panic_dump;
use crate::tools::plan::PlanState;
use crate::tools::spec::ToolError;
use crate::tools::todo::TodoList;

use super::blackboard::{read_blackboard_section, write_blackboard_partition};
use deepseek_core::subagent::{
    CompletionReason, MailboxMessage, SubAgentAssignment, SubAgentResult, SubAgentStatus,
    SubAgentType,
};
use super::mailbox::Mailbox;

use super::constants::*;
use super::prompts::{
    build_subagent_system_prompt, findings_to_verdict, parse_structured_findings_result,
    parse_structured_verdict,
};
use super::registry::{SubAgentToolRegistry, summarize_subagent_result};
use super::resident::release_resident_leases_for;
use super::runtime::SubAgentRuntime;
use super::parse::build_assignment_prompt;
use super::factory::SharedSubAgentManager;
use super::runtime::SubAgentCompletion;
use super::types::{SubAgentInput, WaitMode};
use super::registry::subagent_status_name;
use super::craft;

pub(crate) struct SubAgentTask {
    pub(crate) manager_handle: SharedSubAgentManager,
    pub(crate) runtime: SubAgentRuntime,
    pub(crate) agent_id: String,
    pub(crate) agent_type: SubAgentType,
    pub(crate) prompt: String,
    pub(crate) assignment: SubAgentAssignment,
    /// `None` = full registry inheritance. `Some(list)` = explicit narrow.
    pub(crate) allowed_tools: Option<Vec<String>>,
    pub(crate) started_at: Instant,
    pub(crate) max_steps: u32,
    pub(crate) input_rx: mpsc::UnboundedReceiver<SubAgentInput>,
    /// Optional task id for blackboard association (CRAFT P1).
    pub(crate) task_id: Option<String>,
}

#[allow(clippy::too_many_lines)]
pub(crate) async fn run_subagent_task(task: SubAgentTask) {
    // CRAFT P4-light: create a git stash safety net before
    // the Implementer modifies files. Non-fatal — if git
    // isn't available or there's nothing to stash, we
    // proceed anyway. The stash is a manual recovery aid,
    // not an automated revert mechanism.
    if task.agent_type == SubAgentType::Implementer {
        let workspace = &task.runtime.context.workspace;
        let stash_msg = format!("craft-auto-{}", &task.agent_id[..8.min(task.agent_id.len())]);
        let _ = std::process::Command::new("git")
            .args(["stash", "push", "--include-untracked", "-m", &stash_msg])
            .current_dir(workspace)
            .output();
    }

    let agent_type_for_blackboard = task.agent_type.clone();
    let agent_id = task.agent_id.clone();
    let run_result = std::panic::AssertUnwindSafe(run_subagent(
        &task.manager_handle,
        &task.runtime,
        agent_id.clone(),
        task.agent_type,
        task.prompt,
        task.assignment,
        task.allowed_tools,
        task.started_at,
        task.max_steps,
        task.input_rx,
        task.task_id.clone(),
    ))
    .catch_unwind()
    .await;

    let result: Result<SubAgentResult> = match run_result {
        Ok(inner) => inner,
        Err(panic_info) => {
            let panic_msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic_info.downcast_ref::<String>() {
                s.clone()
            } else {
                "unknown panic".to_string()
            };
            tracing::error!(
                target: "panic",
                "Sub-agent task '{agent_id}' panicked: {panic_msg}",
            );
            let location = std::panic::Location::caller();
            let _ = write_panic_dump("subagent-task", location, &panic_msg);
            Err(anyhow!("panic: {panic_msg}"))
        }
    };

    let mut manager = task.manager_handle.write().await;
    match &result {
        Ok(res) => manager.update_from_result(&agent_id, res.clone()),
        Err(err) => {
            let reason = completion_reason_for_error(err);
            manager.update_failed_with_reason(&agent_id, err.to_string(), reason);
        }
    }

    // CRAFT P1: write structured output to blackboard
    let partition = craft::blackboard_partition_key(&agent_type_for_blackboard);
    if let (Some(tid), Ok(res)) = (task.task_id.as_deref(), &result) {
        write_blackboard_partition(
            &task.runtime.context.workspace,
            tid,
            &agent_type_for_blackboard,
            res,
        );
        craft::emit_craft_events(
            &task.runtime.event_tx,
            &task.agent_id,
            res,
            Some(tid),
            partition,
        );
    }

    // Emit BOTH a human-friendly summary (rendered in the parent's
    // sidebar / cell) AND a structured sentinel the model can recognize
    // on its next turn. Format: human summary on the first line,
    // sentinel on the second. The sentinel uses an opaque tag
    // (`deepseek:subagent.done`) to avoid collision with normal user
    // text.
    let (summary, sentinel) = match &result {
        Ok(res) => (
            summarize_subagent_result(res),
            subagent_done_sentinel(&agent_id, res),
        ),
        Err(err) => (
            format!("Failed: {err}"),
            subagent_failed_sentinel(&agent_id, &err.to_string()),
        ),
    };

    if let Some(mb) = task.runtime.mailbox.as_ref() {
        let envelope = match &result {
            Ok(_) => MailboxMessage::Completed {
                agent_id: agent_id.clone(),
                summary: summary.clone(),
            },
            Err(err) => MailboxMessage::Failed {
                agent_id: agent_id.clone(),
                error: err.to_string(),
            },
        };
        let _ = mb.send(envelope);
    }

    let payload = match &result {
        Ok(res) => {
            let mut payload = format!("{summary}\n{sentinel}");
            if let Some(hint) = craft::craft_fix_loop_hint(res, task.task_id.as_deref()) {
                payload.push('\n');
                payload.push_str(&hint);
            }
            payload
        }
        Err(_) => format!("{summary}\n{sentinel}"),
    };

    // Wake the engine's parent turn loop if this is one of its direct
    // children (issue #756). Gating by `spawn_depth == 1` means the parent
    // only sees completions for agents it directly orchestrated, not for
    // grandchildren spawned recursively inside its children.
    emit_parent_completion(&task.runtime, &agent_id, &payload);

    if let Some(event_tx) = task.runtime.event_tx {
        let _ = event_tx.try_send(Event::AgentComplete {
            id: agent_id,
            result: payload,
        });
    }
}

/// Notify the engine's parent turn loop that a direct child finished
/// (issue #756). Returns `true` if a send was attempted, `false` if the
/// notification was skipped because this isn't a direct child or no channel
/// is wired. Skips silently when the channel sender has no receiver — the
/// engine outlives the runtime, so a dropped receiver means we're shutting
/// down anyway.
pub(crate) fn emit_parent_completion(
    runtime: &SubAgentRuntime,
    agent_id: &str,
    payload: &str,
) -> bool {
    if runtime.spawn_depth != 1 {
        return false;
    }
    let Some(tx) = runtime.parent_completion_tx.as_ref() else {
        return false;
    };
    let _ = tx.send(SubAgentCompletion {
        agent_id: agent_id.to_string(),
        payload: payload.to_string(),
    });
    true
}

/// Build a `<deepseek:subagent.done>` JSON sentinel for a successful child.
/// Intended to surface in the parent's transcript so the model recognizes
/// child completion and can decide whether to read the full result via
/// `agent_result`.
pub(crate) fn subagent_done_sentinel(agent_id: &str, res: &SubAgentResult) -> String {
    let mut payload = serde_json::Map::new();
    payload.insert("agent_id".into(), json!(agent_id));
    payload.insert("agent_type".into(), json!(res.agent_type.as_str()));
    payload.insert("status".into(), json!(subagent_status_name(&res.status)));
    payload.insert("duration_ms".into(), json!(res.duration_ms));
    payload.insert("steps".into(), json!(res.steps_taken));
    payload.insert("summary".into(), json!(summarize_subagent_result(res)));

    if let Some(ref v) = res.structured_verdict {
        if let Ok(val) = serde_json::to_value(v) {
            payload.insert("structured_verdict".into(), val);
        }
    }
    if let Some(ref f) = res.structured_findings {
        if let Ok(val) = serde_json::to_value(f) {
            payload.insert("structured_findings".into(), val);
        }
    }
    if let Some(ref reason) = res.completion_reason {
        if let Ok(val) = serde_json::to_value(reason) {
            payload.insert("completion_reason".into(), val);
        }
    }
    if let Some(ref reason) = res.structured_findings_parse_failure {
        if let Ok(val) = serde_json::to_value(reason) {
            payload.insert("structured_findings_parse_failure".into(), val);
        }
    }

    let payload = serde_json::Value::Object(payload);
    format!("<deepseek:subagent.done>{payload}</deepseek:subagent.done>")
}

/// Build a `<deepseek:subagent.done>` sentinel for a failed child.
pub(crate) fn subagent_failed_sentinel(agent_id: &str, err: &str) -> String {
    subagent_failed_sentinel_with_reason(agent_id, err, completion_reason_for_error_str(err))
}

pub(crate) fn subagent_failed_sentinel_with_reason(
    agent_id: &str,
    err: &str,
    completion_reason: Option<CompletionReason>,
) -> String {
    let mut payload = serde_json::Map::new();
    payload.insert("agent_id".into(), json!(agent_id));
    payload.insert("status".into(), json!("failed"));
    payload.insert("error".into(), json!(err));
    if let Some(reason) = completion_reason {
        if let Ok(val) = serde_json::to_value(reason) {
            payload.insert("completion_reason".into(), val);
        }
    }
    let payload = serde_json::Value::Object(payload);
    format!("<deepseek:subagent.done>{payload}</deepseek:subagent.done>")
}

pub(crate) fn completion_reason_for_successful_exit(natural_break: bool) -> CompletionReason {
    if natural_break {
        CompletionReason::NaturalBreak
    } else {
        CompletionReason::StepLimitReached
    }
}

fn completion_reason_for_error(err: &anyhow::Error) -> Option<CompletionReason> {
    completion_reason_for_error_str(&err.to_string())
}

pub(crate) fn completion_reason_for_error_str(err: &str) -> Option<CompletionReason> {
    if err.starts_with("panic:") {
        Some(CompletionReason::Panic(
            err.strip_prefix("panic:").unwrap_or(err).trim().to_string(),
        ))
    } else if err.contains("API call timed out") {
        Some(CompletionReason::StepApiTimeout)
    } else {
        None
    }
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
async fn run_subagent(
    manager_handle: &SharedSubAgentManager,
    runtime: &SubAgentRuntime,
    agent_id: String,
    agent_type: SubAgentType,
    prompt: String,
    assignment: SubAgentAssignment,
    allowed_tools: Option<Vec<String>>,
    started_at: Instant,
    max_steps: u32,
    mut input_rx: mpsc::UnboundedReceiver<SubAgentInput>,
    task_id: Option<String>,
) -> Result<SubAgentResult> {
    // CRAFT P1: read blackboard at spawn time (snapshot — no live reload)
    let blackboard_section = task_id
        .as_deref()
        .and_then(|tid| read_blackboard_section(&runtime.context.workspace, tid, &agent_type));

    let system_prompt = build_subagent_system_prompt(&agent_type, &assignment);
    let tool_registry = SubAgentToolRegistry::new(
        runtime.clone(),
        allowed_tools.clone(),
        Arc::new(Mutex::new(TodoList::new())),
        Arc::new(Mutex::new(PlanState::default())),
    );
    let unavailable_tools = tool_registry.unavailable_allowed_tools();
    if !unavailable_tools.is_empty() {
        return Err(anyhow!(
            "Sub-agent requested unavailable tools: {}",
            unavailable_tools.join(", ")
        ));
    }
    let tools = tool_registry.tools_for_model();
    if let Some(mb) = runtime.mailbox.as_ref() {
        let _ = mb.send(MailboxMessage::started(&agent_id, agent_type.as_str()));
    }
    record_and_emit_progress(
        manager_handle,
        runtime,
        &agent_id,
        0,
        format!("started ({})", agent_type.as_str()),
    )
    .await;

    let mut messages = vec![Message {
        role: "user".to_string(),
        content: vec![ContentBlock::Text {
            text: build_assignment_prompt(
                &prompt,
                &assignment,
                &agent_type,
                blackboard_section.as_deref(),
            ),
            cache_control: None,
        }],
    }];

    let mut steps = 0;
    let mut final_result: Option<String> = None;
    let mut pending_inputs: VecDeque<SubAgentInput> = VecDeque::new();
    let mut natural_break = false;

    for _step in 0..max_steps {
        // Cooperative cancellation: bail if the parent (or root) cancelled
        // us while we were between steps. Children derive their token from
        // the parent's via `child_token()` so this propagates the whole tree.
        if runtime.cancel_token.is_cancelled() {
            record_and_emit_progress(
                manager_handle,
                runtime,
                &agent_id,
                steps,
                format!("step {steps}/{max_steps}: cancelled"),
            )
            .await;
            if let Some(mb) = runtime.mailbox.as_ref() {
                let _ = mb.send(MailboxMessage::Cancelled {
                    agent_id: agent_id.clone(),
                });
            }
            return Ok(SubAgentResult {
                agent_id: agent_id.clone(),
                agent_type: agent_type.clone(),
                assignment: assignment.clone(),
                model: runtime.model.clone(),
                nickname: None,
                status: SubAgentStatus::Cancelled,
                result: None,
                steps_taken: steps,
                duration_ms: u64::try_from(started_at.elapsed().as_millis()).unwrap_or(u64::MAX),
                from_prior_session: false,
                structured_verdict: None,
                structured_findings: None,
                completion_reason: Some(CompletionReason::Cancelled),
                max_steps,
                step_timeout_ms: u64::try_from(runtime.step_timeout.as_millis()).unwrap_or(u64::MAX),
                structured_findings_parse_failure: None,
                scratchpad_run_id: None,
                progress_status: None,
                stuck_suspected: false,
                idle_ms: 0,
            });
        }

        steps += 1;
        record_and_emit_progress(
            manager_handle,
            runtime,
            &agent_id,
            steps,
            format!("step {steps}/{max_steps}: requesting model response"),
        )
        .await;

        while let Ok(input) = input_rx.try_recv() {
            if input.interrupt {
                pending_inputs.clear();
            }
            pending_inputs.push_back(input);
        }

        while let Some(input) = pending_inputs.pop_front() {
            if !input.text.trim().is_empty() {
                messages.push(Message {
                    role: "user".to_string(),
                    content: vec![ContentBlock::Text {
                        text: input.text,
                        cache_control: None,
                    }],
                });
            }
        }

        let request = MessageRequest {
            model: runtime.model.clone(),
            messages: messages.clone(),
            max_tokens: 4096,
            system: Some(SystemPrompt::Text(system_prompt.clone())),
            tools: Some(tools.clone()),
            tool_choice: Some(json!({ "type": "auto" })),
            metadata: None,
            thinking: None,
            reasoning_effort: runtime.reasoning_effort.clone(),
            stream: Some(false),
            temperature: None,
            top_p: None,
        };

        // Race the API call against the cancellation token so a parent
        // cancel during a long thinking turn doesn't have to wait for the
        // step timeout.
        let response = tokio::select! {
            biased;
            () = runtime.cancel_token.cancelled() => {
                record_and_emit_progress(
                    manager_handle,
                    runtime,
                    &agent_id,
                    steps,
                    format!("step {steps}/{max_steps}: cancelled mid-request"),
                )
                .await;
                if let Some(mb) = runtime.mailbox.as_ref() {
                    let _ = mb.send(MailboxMessage::Cancelled {
                        agent_id: agent_id.clone(),
                    });
                }
                return Ok(SubAgentResult {
                    agent_id: agent_id.clone(),
                    agent_type: agent_type.clone(),
                    assignment: assignment.clone(),
                    model: runtime.model.clone(),
                    nickname: None,
                    status: SubAgentStatus::Cancelled,
                    result: None,
                    steps_taken: steps,
                    duration_ms: u64::try_from(started_at.elapsed().as_millis())
                        .unwrap_or(u64::MAX),
                    from_prior_session: false,
                    structured_verdict: None,
                    structured_findings: None,
                    completion_reason: Some(CompletionReason::Cancelled),
                    max_steps,
                    step_timeout_ms: u64::try_from(runtime.step_timeout.as_millis())
                        .unwrap_or(u64::MAX),
                    structured_findings_parse_failure: None,
                    scratchpad_run_id: None,
                    progress_status: None,
                    stuck_suspected: false,
                    idle_ms: 0,
                });
            }
            api = tokio::time::timeout(runtime.step_timeout, runtime.client.create_message(request)) => {
                api.map_err(|_| step_api_timeout_error(runtime.step_timeout.as_secs()))??
            }
        };

        let mut tool_uses = Vec::new();

        // Report token usage so the parent's cost counter updates live.
        if let Some(mb) = runtime.mailbox.as_ref() {
            let _ = mb.send(MailboxMessage::token_usage(
                &agent_id,
                response.model.clone(),
                response.usage.clone(),
            ));
        }

        for block in &response.content {
            match block {
                ContentBlock::Text { text, .. } if !text.trim().is_empty() => {
                    final_result = Some(text.clone());
                }
                ContentBlock::ToolUse {
                    id, name, input, ..
                } => {
                    tool_uses.push((id.clone(), name.clone(), input.clone()));
                }
                _ => {}
            }
        }

        messages.push(Message {
            role: "assistant".to_string(),
            content: response.content.clone(),
        });

        if tool_uses.is_empty() {
            while let Ok(input) = input_rx.try_recv() {
                if input.interrupt {
                    pending_inputs.clear();
                }
                pending_inputs.push_back(input);
            }
            if pending_inputs.is_empty() {
                record_and_emit_progress(
                    manager_handle,
                    runtime,
                    &agent_id,
                    steps,
                    format!("step {steps}/{max_steps}: complete"),
                )
                .await;
                natural_break = true;
                break;
            }
            continue;
        }

        record_and_emit_progress(
            manager_handle,
            runtime,
            &agent_id,
            steps,
            format!(
                "step {steps}/{max_steps}: executing {} tool call(s)",
                tool_uses.len()
            ),
        )
        .await;
        let mut tool_results: Vec<ContentBlock> = Vec::new();
        let step_tool_budget = step_tool_budget(runtime.step_timeout);
        let mut step_tool_spent = Duration::ZERO;
        for (tool_id, tool_name, tool_input) in tool_uses {
            record_and_emit_progress(
                manager_handle,
                runtime,
                &agent_id,
                steps,
                format!("step {steps}/{max_steps}: running tool '{tool_name}'"),
            )
            .await;
            if let Some(mb) = runtime.mailbox.as_ref() {
                let _ = mb.send(MailboxMessage::ToolCallStarted {
                    agent_id: agent_id.clone(),
                    tool_name: tool_name.clone(),
                    step: steps,
                });
            }
            let result = {
                let remaining_budget = step_tool_budget.saturating_sub(step_tool_spent);
                if remaining_budget.is_zero() {
                    format!(
                        "Error: Step tool time budget exhausted ({:.0}s cap for this step)",
                        step_tool_budget.as_secs_f64()
                    )
                } else {
                    let per_call_timeout = TOOL_TIMEOUT.min(remaining_budget);
                    let tool_start = Instant::now();
                    let out = match tokio::time::timeout(per_call_timeout, async {
                        tool_registry
                            .execute(&agent_id, &tool_name, tool_input)
                            .await
                    })
                    .await
                    {
                        Ok(Ok(output)) => output,
                        Ok(Err(e)) => format!("Error: {e}"),
                        Err(_) => format!("Error: Tool {tool_name} timed out"),
                    };
                    step_tool_spent += tool_start.elapsed().min(per_call_timeout);
                    out
                }
            };
            let tool_ok = !result.starts_with("Error:");
            record_and_emit_progress(
                manager_handle,
                runtime,
                &agent_id,
                steps,
                format!("step {steps}/{max_steps}: finished tool '{tool_name}'"),
            )
            .await;
            if let Some(mb) = runtime.mailbox.as_ref() {
                let _ = mb.send(MailboxMessage::ToolCallCompleted {
                    agent_id: agent_id.clone(),
                    tool_name: tool_name.clone(),
                    step: steps,
                    ok: tool_ok,
                });
            }

            tool_results.push(ContentBlock::ToolResult {
                tool_use_id: tool_id,
                content: result,
                is_error: None,
                content_blocks: None,
            });
        }

        if !tool_results.is_empty() {
            messages.push(Message {
                role: "user".to_string(),
                content: tool_results,
            });
        }
    }

    release_resident_leases_for(&agent_id);

    let (structured_findings, structured_findings_parse_failure) =
        match final_result.as_deref() {
            Some(text) => match parse_structured_findings_result(text) {
                Ok(findings) => (Some(findings), None),
                Err(reason) => (None, Some(reason)),
            },
            None => (None, None),
        };
    let structured_verdict = final_result
        .as_deref()
        .and_then(parse_structured_verdict)
        .or_else(|| structured_findings.as_ref().map(findings_to_verdict));

    Ok(SubAgentResult {
        agent_id,
        agent_type,
        assignment,
        model: runtime.model.clone(),
        nickname: None,
        status: SubAgentStatus::Completed,
        result: final_result,
        steps_taken: steps,
        duration_ms: u64::try_from(started_at.elapsed().as_millis()).unwrap_or(u64::MAX),
        from_prior_session: false,
        structured_verdict,
        structured_findings,
        completion_reason: Some(completion_reason_for_successful_exit(natural_break)),
        max_steps,
        step_timeout_ms: u64::try_from(runtime.step_timeout.as_millis()).unwrap_or(u64::MAX),
        structured_findings_parse_failure,
        scratchpad_run_id: None,
        progress_status: None,
        stuck_suspected: false,
        idle_ms: 0,
    })
}

pub(crate) async fn wait_for_result(
    manager: &SharedSubAgentManager,
    agent_id: &str,
    timeout: Duration,
) -> Result<(SubAgentResult, bool), ToolError> {
    let deadline = Instant::now() + timeout;

    loop {
        let snapshot = {
            let mut manager = manager.write().await;
            manager
                .get_result(agent_id)
                .map_err(|e| ToolError::execution_failed(e.to_string()))?
        };

        if snapshot.status != SubAgentStatus::Running {
            return Ok((snapshot, false));
        }
        if Instant::now() >= deadline {
            return Ok((snapshot, true));
        }

        tokio::time::sleep(RESULT_POLL_INTERVAL).await;
    }
}

pub(crate) async fn wait_for_agents(
    manager: &SharedSubAgentManager,
    ids: &[String],
    wait_mode: WaitMode,
    timeout: Duration,
) -> Result<(Vec<SubAgentResult>, bool), ToolError> {
    let deadline = Instant::now() + timeout;

    loop {
        let snapshots = {
            let mut manager = manager.write().await;
            ids.iter()
                .map(|id| {
                    manager
                        .get_result(id)
                        .map_err(|e| ToolError::execution_failed(e.to_string()))
                })
                .collect::<Result<Vec<_>, _>>()?
        };

        if wait_mode.condition_met(&snapshots) {
            return Ok((snapshots, false));
        }
        if Instant::now() >= deadline {
            return Ok((snapshots, true));
        }

        tokio::time::sleep(RESULT_POLL_INTERVAL).await;
    }
}


pub(crate) async fn record_and_emit_progress(
    manager_handle: &SharedSubAgentManager,
    runtime: &SubAgentRuntime,
    agent_id: &str,
    steps_taken: u32,
    status: String,
) {
    {
        let mut mgr = manager_handle.write().await;
        mgr.record_execution_progress(agent_id, steps_taken, &status);
    }
    emit_agent_progress(
        runtime.event_tx.as_ref(),
        runtime.mailbox.as_ref(),
        agent_id,
        status,
    );
}

pub(crate) fn emit_agent_progress(
    event_tx: Option<&mpsc::Sender<Event>>,
    mailbox: Option<&Mailbox>,
    agent_id: &str,
    status: String,
) {
    if let Some(mb) = mailbox {
        let _ = mb.send(MailboxMessage::progress(agent_id, status.clone()));
    }
    if let Some(event_tx) = event_tx {
        let _ = event_tx.try_send(Event::AgentProgress {
            id: agent_id.to_string(),
            status,
        });
    }
}
