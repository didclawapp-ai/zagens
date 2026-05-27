use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use serde_json::json;
use tokio::sync::{Mutex, mpsc};

use deepseek_core::events::Event;
use crate::models::{ContentBlock, Message, MessageRequest, SystemPrompt};
use crate::tools::plan::PlanState;
use crate::tools::spec::ToolError;
use crate::tools::todo::TodoList;

use super::blackboard::{read_blackboard_section, write_blackboard_partition};
use deepseek_core::subagent::{
    MailboxMessage, SubAgentAssignment, SubAgentResult, SubAgentStatus,
    SubAgentType,
};
use super::mailbox::Mailbox;

use super::constants::*;
use super::prompts::{build_subagent_system_prompt, parse_structured_verdict};
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
    let result = run_subagent(
        &task.runtime,
        task.agent_id.clone(),
        task.agent_type,
        task.prompt,
        task.assignment,
        task.allowed_tools,
        task.started_at,
        task.max_steps,
        task.input_rx,
        task.task_id.clone(),
    )
    .await;

    let mut manager = task.manager_handle.write().await;
    match &result {
        Ok(res) => manager.update_from_result(&task.agent_id, res.clone()),
        Err(err) => manager.update_failed(&task.agent_id, err.to_string()),
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
            subagent_done_sentinel(&task.agent_id, res),
        ),
        Err(err) => (
            format!("Failed: {err}"),
            subagent_failed_sentinel(&task.agent_id, &err.to_string()),
        ),
    };

    if let Some(mb) = task.runtime.mailbox.as_ref() {
        let envelope = match &result {
            Ok(_) => MailboxMessage::Completed {
                agent_id: task.agent_id.clone(),
                summary: summary.clone(),
            },
            Err(err) => MailboxMessage::Failed {
                agent_id: task.agent_id.clone(),
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
    emit_parent_completion(&task.runtime, &task.agent_id, &payload);

    if let Some(event_tx) = task.runtime.event_tx {
        let _ = event_tx.try_send(Event::AgentComplete {
            id: task.agent_id,
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

    let payload = serde_json::Value::Object(payload);
    format!("<deepseek:subagent.done>{payload}</deepseek:subagent.done>")
}

/// Build a `<deepseek:subagent.done>` sentinel for a failed child.
pub(crate) fn subagent_failed_sentinel(agent_id: &str, err: &str) -> String {
    let payload = json!({
        "agent_id": agent_id,
        "status": "failed",
        "error": err,
    });
    format!("<deepseek:subagent.done>{payload}</deepseek:subagent.done>")
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
async fn run_subagent(
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
    emit_agent_progress(
        runtime.event_tx.as_ref(),
        runtime.mailbox.as_ref(),
        &agent_id,
        format!("started ({})", agent_type.as_str()),
    );

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

    for _step in 0..max_steps {
        // Cooperative cancellation: bail if the parent (or root) cancelled
        // us while we were between steps. Children derive their token from
        // the parent's via `child_token()` so this propagates the whole tree.
        if runtime.cancel_token.is_cancelled() {
            emit_agent_progress(
                runtime.event_tx.as_ref(),
                runtime.mailbox.as_ref(),
                &agent_id,
                format!("step {steps}/{max_steps}: cancelled"),
            );
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
            });
        }

        steps += 1;
        emit_agent_progress(
            runtime.event_tx.as_ref(),
            runtime.mailbox.as_ref(),
            &agent_id,
            format!("step {steps}/{max_steps}: requesting model response"),
        );

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
                emit_agent_progress(
                    runtime.event_tx.as_ref(),
                    runtime.mailbox.as_ref(),
                    &agent_id,
                    format!("step {steps}/{max_steps}: cancelled mid-request"),
                );
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
                emit_agent_progress(
                    runtime.event_tx.as_ref(),
                    runtime.mailbox.as_ref(),
                    &agent_id,
                    format!("step {steps}/{max_steps}: complete"),
                );
                break;
            }
            continue;
        }

        emit_agent_progress(
            runtime.event_tx.as_ref(),
            runtime.mailbox.as_ref(),
            &agent_id,
            format!(
                "step {steps}/{max_steps}: executing {} tool call(s)",
                tool_uses.len()
            ),
        );
        let mut tool_results: Vec<ContentBlock> = Vec::new();
        for (tool_id, tool_name, tool_input) in tool_uses {
            emit_agent_progress(
                runtime.event_tx.as_ref(),
                runtime.mailbox.as_ref(),
                &agent_id,
                format!("step {steps}/{max_steps}: running tool '{tool_name}'"),
            );
            if let Some(mb) = runtime.mailbox.as_ref() {
                let _ = mb.send(MailboxMessage::ToolCallStarted {
                    agent_id: agent_id.clone(),
                    tool_name: tool_name.clone(),
                    step: steps,
                });
            }
            let result = match tokio::time::timeout(TOOL_TIMEOUT, async {
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
            let tool_ok = !result.starts_with("Error:");
            emit_agent_progress(
                runtime.event_tx.as_ref(),
                runtime.mailbox.as_ref(),
                &agent_id,
                format!("step {steps}/{max_steps}: finished tool '{tool_name}'"),
            );
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

    let structured_verdict = final_result
        .as_deref()
        .and_then(parse_structured_verdict);

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
            let manager = manager.read().await;
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
            let manager = manager.read().await;
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
