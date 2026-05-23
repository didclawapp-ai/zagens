//! Tool execution phase for the turn loop (P2 PR4 — `TurnLoopHost::run_tool_execution_phase`).

use std::collections::HashSet;
use std::time::Instant;

use deepseek_core::engine::context::compact_tool_result_for_context;
use deepseek_core::engine::dispatch::{
    caller_type_for_tool_use, format_tool_error, mcp_tool_approval_description,
    mcp_tool_is_parallel_safe, mcp_tool_is_read_only,
};
use deepseek_core::engine::emit_tool_audit;
use deepseek_core::engine::loop_guard::{AttemptDecision, LoopGuard, OutcomeDecision};
use deepseek_core::engine::streaming::ToolUseState;
use deepseek_core::chat::{ContentBlock, Message, Tool};
use deepseek_core::engine::turn_loop::{
    build_edit_file_approval_desc, ToolExecOutcome, ToolExecutionPlan, TurnLoopToolPhaseOutcome,
};
use deepseek_core::error_taxonomy::{ErrorCategory, ErrorEnvelope};
use deepseek_core::turn::{TurnContext, TurnLoopMode, TurnToolCall};
use deepseek_tools::{ToolError, ToolResult};
use futures_util::stream::FuturesUnordered;
use futures_util::StreamExt;
use serde_json::json;

use super::host_impl::turn_loop_to_app_mode;
use super::super::dispatch::{
    caller_allowed_for_tool, should_parallelize_tool_batch, should_stop_after_plan_tool,
};
use super::super::scratchpad_flow;
use crate::core::events::Event;
use super::super::approval::ApprovalResult;
use crate::tools::spec::{ApprovalRequirement, ToolContext};
use super::super::tool_catalog::{
    execute_code_execution_tool, execute_tool_search, is_tool_search_tool,
    maybe_activate_requested_deferred_tool, missing_tool_error_message,
    CODE_EXECUTION_TOOL_NAME, MULTI_TOOL_PARALLEL_NAME, REQUEST_USER_INPUT_NAME,
};
use super::Engine;
use crate::mcp::McpPool;
use crate::tools::user_input::UserInputRequest;

pub(super) async fn run_tool_execution_phase(
    engine: &mut Engine,
    turn: &mut TurnContext,
    mode: TurnLoopMode,
    tool_uses: &mut [ToolUseState],
    tool_catalog: &[Tool],
    active_tool_names: &mut HashSet<String>,
    loop_guard: &mut LoopGuard,
    consecutive_tool_error_steps: u32,
    tool_registry: Option<&crate::tools::ToolRegistry>,
) -> TurnLoopToolPhaseOutcome {
    let tool_exec_lock = engine.tool_exec_lock.clone();
            let mcp_pool = if tool_uses
                .iter()
                .any(|tool| McpPool::is_mcp_tool(&tool.name))
            {
                match engine.ensure_mcp_pool().await {
                    Ok(pool) => Some(pool),
                    Err(err) => {
                        let _ = engine.tx_event.send(Event::status(err.to_string())).await;
                        None
                    }
                }
            } else {
                None
            };

            let mut plans: Vec<ToolExecutionPlan> = Vec::with_capacity(tool_uses.len());
            for (index, tool) in tool_uses.iter_mut().enumerate() {
                let tool_id = tool.id.clone();
                let mut tool_name = tool.name.clone();
                let tool_input = tool.input.clone();
                let tool_caller = tool.caller.clone();
                crate::logging::info(format!(
                    "Planning tool '{}' with input: {:?}",
                    tool_name, tool_input
                ));

                let interactive = (tool_name == "exec_shell"
                    && tool_input
                        .get("interactive")
                        .and_then(serde_json::Value::as_bool)
                        == Some(true))
                    || tool_name == REQUEST_USER_INPUT_NAME;

                let mut approval_required = false;
                let mut approval_description = "Tool execution requires approval".to_string();
                let mut supports_parallel = false;
                let mut read_only = false;
                let mut blocked_error: Option<ToolError> = None;
                let mut guard_result: Option<ToolResult> = None;
                if maybe_activate_requested_deferred_tool(
                    &tool_name,
                    &tool_catalog,
                    active_tool_names,
                ) {
                    let _ = engine
                        .tx_event
                        .send(Event::status(format!(
                            "Auto-loaded deferred tool '{tool_name}' after model request."
                        )))
                        .await;
                }
                let mut tool_def = tool_catalog.iter().find(|def| def.name == tool_name);

                // Resolve hallucinated tool names when the model emits a
                // non-canonical variant (Read_file, readFile, read-file, etc.).
                if tool_def.is_none()
                    && let Some(registry) = tool_registry
                    && let Some(canonical) = registry.resolve(&tool_name)
                {
                    crate::logging::info(format!(
                        "Resolved hallucinated tool name '{}' -> '{}'",
                        tool_name, canonical
                    ));
                    tool_def = tool_catalog.iter().find(|d| d.name == canonical);
                    if tool_def.is_some() {
                        tool_name = canonical.to_string();
                        // Update the tool_uses entry so the result is
                        // attributed to the canonical name.
                        tool.name = tool_name.clone();
                        // Re-run the deferred-activation check with the
                        // canonical name.
                        if maybe_activate_requested_deferred_tool(
                            &tool_name,
                            &tool_catalog,
                            active_tool_names,
                        ) {
                            let _ = engine
                                .tx_event
                                .send(Event::status(format!(
                                    "Auto-loaded deferred tool '{}' after resolving '{}'.",
                                    tool_name, tool_name
                                )))
                                .await;
                        }
                    }
                }

                if !caller_allowed_for_tool(tool_caller.as_ref(), tool_def) {
                    blocked_error = Some(ToolError::permission_denied(format!(
                        "Tool '{tool_name}' does not allow caller '{}'",
                        caller_type_for_tool_use(tool_caller.as_ref())
                    )));
                }

                if blocked_error.is_none()
                    && tool_def.is_none()
                    && !McpPool::is_mcp_tool(&tool_name)
                    && tool_name != CODE_EXECUTION_TOOL_NAME
                    && !is_tool_search_tool(&tool_name)
                {
                    blocked_error = Some(ToolError::not_available(missing_tool_error_message(
                        &tool_name,
                        &tool_catalog,
                    )));
                }

                if McpPool::is_mcp_tool(&tool_name) {
                    read_only = mcp_tool_is_read_only(&tool_name);
                    supports_parallel = mcp_tool_is_parallel_safe(&tool_name);
                    approval_required = !read_only;
                    approval_description = mcp_tool_approval_description(&tool_name);
                } else if let Some(registry) = tool_registry
                    && let Some(spec) = registry.get(&tool_name)
                {
                    approval_required = spec.approval_requirement() != ApprovalRequirement::Auto;
                    approval_description = if tool_name == "edit_file" {
                        build_edit_file_approval_desc(&tool_input)
                    } else {
                        spec.description().to_string()
                    };
                    supports_parallel = spec.supports_parallel();
                    read_only = spec.is_read_only();
                } else if tool_name == CODE_EXECUTION_TOOL_NAME {
                    approval_required = true;
                    approval_description =
                        "Run model-provided Python code in local execution sandbox".to_string();
                    supports_parallel = false;
                    read_only = false;
                } else if is_tool_search_tool(&tool_name) {
                    approval_required = false;
                    approval_description = "Search tool catalog".to_string();
                    supports_parallel = false;
                    read_only = true;
                }

                if blocked_error.is_none()
                    && let AttemptDecision::Block(message) =
                        loop_guard.record_attempt(&tool_name, &tool_input)
                {
                    crate::logging::warn(message.clone());
                    guard_result = Some(
                        ToolResult::success(message)
                            .with_metadata(json!({"loop_guard": "identical_tool_call"})),
                    );
                }

                plans.push(ToolExecutionPlan {
                    index,
                    id: tool_id,
                    name: tool_name,
                    input: tool_input,
                    caller: tool_caller,
                    interactive,
                    approval_required,
                    approval_description,
                    supports_parallel,
                    read_only,
                    blocked_error,
                    guard_result,
                });
            }

            let parallel_allowed = should_parallelize_tool_batch(&plans);
            if parallel_allowed && plans.len() > 1 {
                let _ = engine
                    .tx_event
                    .send(Event::status(format!(
                        "Executing {} read-only tools in parallel",
                        plans.len()
                    )))
                    .await;
            } else if plans.len() > 1 {
                let _ = engine
                    .tx_event
                    .send(Event::status(
                        "Executing tools sequentially (writes, approvals, or non-parallel tools detected)",
                    ))
                    .await;
            }

            let mut outcomes: Vec<Option<ToolExecOutcome>> = Vec::with_capacity(plans.len());
            outcomes.resize_with(plans.len(), || None);

            if parallel_allowed {
                let mut tool_tasks = FuturesUnordered::new();
                for plan in plans {
                    if let Some(result) = plan.guard_result.clone() {
                        let result = Ok(result);
                        let _ = engine
                            .tx_event
                            .send(Event::ToolCallComplete {
                                id: plan.id.clone(),
                                name: plan.name.clone(),
                                result: result.clone(),
                            })
                            .await;
                        outcomes[plan.index] = Some(ToolExecOutcome {
                            index: plan.index,
                            id: plan.id,
                            name: plan.name,
                            input: plan.input,
                            started_at: Instant::now(),
                            result,
                        });
                        continue;
                    }
                    if let Some(err) = plan.blocked_error.clone() {
                        outcomes[plan.index] = Some(ToolExecOutcome {
                            index: plan.index,
                            id: plan.id,
                            name: plan.name,
                            input: plan.input,
                            started_at: Instant::now(),
                            result: Err(err),
                        });
                        continue;
                    }
                    let registry = tool_registry;
                    let lock = tool_exec_lock.clone();
                    let mcp_pool = mcp_pool.clone();
                    let tx_event = engine.tx_event.clone();
                    let started_at = Instant::now();

                    tool_tasks.push(async move {
                        let mut result = Engine::execute_tool_with_lock(
                            lock,
                            plan.supports_parallel,
                            plan.interactive,
                            tx_event.clone(),
                            plan.name.clone(),
                            plan.input.clone(),
                            registry,
                            mcp_pool,
                            None,
                            Some(plan.id.clone()),
                        )
                        .await;

                        // #500: spill outsized output before fanout (mirror
                        // of the sequential path below). Emit a
                        // `tool.spillover` audit event so operators can
                        // correlate large-output episodes with disk usage.
                        if let Ok(tool_result) = result.as_mut()
                            && let Some(path) =
                                crate::tools::truncate::apply_spillover(tool_result, &plan.id)
                        {
                            emit_tool_audit(json!({
                                "event": "tool.spillover",
                                "tool_id": plan.id.clone(),
                                "tool_name": plan.name.clone(),
                                "path": path.display().to_string(),
                            }));
                        }

                        let _ = tx_event
                            .send(Event::ToolCallComplete {
                                id: plan.id.clone(),
                                name: plan.name.clone(),
                                result: result.clone(),
                            })
                            .await;

                        ToolExecOutcome {
                            index: plan.index,
                            id: plan.id,
                            name: plan.name,
                            input: plan.input,
                            started_at,
                            result,
                        }
                    });
                }

                while let Some(outcome) = tool_tasks.next().await {
                    let index = outcome.index;
                    outcomes[index] = Some(outcome);
                }
            } else {
                for plan in plans {
                    let tool_id = plan.id.clone();
                    let tool_name = plan.name.clone();
                    let tool_input = plan.input.clone();
                    let tool_caller = plan.caller.clone();

                    if let Some(result) = plan.guard_result.clone() {
                        let result = Ok(result);
                        let _ = engine
                            .tx_event
                            .send(Event::ToolCallComplete {
                                id: tool_id.clone(),
                                name: tool_name.clone(),
                                result: result.clone(),
                            })
                            .await;
                        outcomes[plan.index] = Some(ToolExecOutcome {
                            index: plan.index,
                            id: tool_id,
                            name: tool_name,
                            input: tool_input,
                            started_at: Instant::now(),
                            result,
                        });
                        continue;
                    }

                    if let Some(err) = plan.blocked_error.clone() {
                        let result = Err(err);
                        let _ = engine
                            .tx_event
                            .send(Event::ToolCallComplete {
                                id: tool_id.clone(),
                                name: tool_name.clone(),
                                result: result.clone(),
                            })
                            .await;
                        outcomes[plan.index] = Some(ToolExecOutcome {
                            index: plan.index,
                            id: tool_id,
                            name: tool_name,
                            input: tool_input,
                            started_at: Instant::now(),
                            result,
                        });
                        continue;
                    }

                    if tool_name == MULTI_TOOL_PARALLEL_NAME {
                        let started_at = Instant::now();
                        let result = engine
                            .execute_parallel_tool(
                                tool_input.clone(),
                                tool_registry,
                                tool_exec_lock.clone(),
                            )
                            .await;

                        let _ = engine
                            .tx_event
                            .send(Event::ToolCallComplete {
                                id: tool_id.clone(),
                                name: tool_name.clone(),
                                result: result.clone(),
                            })
                            .await;

                        outcomes[plan.index] = Some(ToolExecOutcome {
                            index: plan.index,
                            id: tool_id,
                            name: tool_name,
                            input: tool_input,
                            started_at,
                            result,
                        });
                        continue;
                    }

                    if tool_name == CODE_EXECUTION_TOOL_NAME {
                        let started_at = Instant::now();
                        let result =
                            execute_code_execution_tool(&tool_input, &engine.session.workspace).await;

                        let _ = engine
                            .tx_event
                            .send(Event::ToolCallComplete {
                                id: tool_id.clone(),
                                name: tool_name.clone(),
                                result: result.clone(),
                            })
                            .await;

                        outcomes[plan.index] = Some(ToolExecOutcome {
                            index: plan.index,
                            id: tool_id,
                            name: tool_name,
                            input: tool_input,
                            started_at,
                            result,
                        });
                        continue;
                    }

                    if is_tool_search_tool(&tool_name) {
                        let started_at = Instant::now();
                        let result = execute_tool_search(
                            &tool_name,
                            &tool_input,
                            &tool_catalog,
                            active_tool_names,
                        );

                        let _ = engine
                            .tx_event
                            .send(Event::ToolCallComplete {
                                id: tool_id.clone(),
                                name: tool_name.clone(),
                                result: result.clone(),
                            })
                            .await;

                        outcomes[plan.index] = Some(ToolExecOutcome {
                            index: plan.index,
                            id: tool_id,
                            name: tool_name,
                            input: tool_input,
                            started_at,
                            result,
                        });
                        continue;
                    }

                    if tool_name == REQUEST_USER_INPUT_NAME {
                        let started_at = Instant::now();
                        let result = match UserInputRequest::from_value(&tool_input) {
                            Ok(request) => engine.await_user_input(&tool_id, request).await.and_then(
                                |response| {
                                    ToolResult::json(&response)
                                        .map_err(|e| ToolError::execution_failed(e.to_string()))
                                },
                            ),
                            Err(err) => Err(err),
                        };

                        let _ = engine
                            .tx_event
                            .send(Event::ToolCallComplete {
                                id: tool_id.clone(),
                                name: tool_name.clone(),
                                result: result.clone(),
                            })
                            .await;

                        outcomes[plan.index] = Some(ToolExecOutcome {
                            index: plan.index,
                            id: tool_id,
                            name: tool_name,
                            input: tool_input,
                            started_at,
                            result,
                        });
                        continue;
                    }

                    // Handle approval flow: returns (result_override, context_override)
                    let (result_override, context_override): (
                        Option<Result<ToolResult, ToolError>>,
                        Option<crate::tools::ToolContext>,
                    ) = if plan.approval_required {
                        emit_tool_audit(json!({
                            "event": "tool.approval_required",
                            "tool_id": tool_id.clone(),
                            "tool_name": tool_name.clone(),
                        }));
                        let approval_key = crate::tools::approval_cache::build_approval_key(
                            &tool_name,
                            &tool_input,
                        )
                        .0;
                        let _ = engine
                            .tx_event
                            .send(Event::ApprovalRequired {
                                id: tool_id.clone(),
                                tool_name: tool_name.clone(),
                                description: plan.approval_description.clone(),
                                approval_key,
                            })
                            .await;

                        match engine.await_tool_approval(&tool_id).await {
                            Ok(ApprovalResult::Approved) => {
                                emit_tool_audit(json!({
                                    "event": "tool.approval_decision",
                                    "tool_id": tool_id.clone(),
                                    "tool_name": tool_name.clone(),
                                    "decision": "approved",
                                    "caller": caller_type_for_tool_use(tool_caller.as_ref()),
                                }));
                                (None, None)
                            }
                            Ok(ApprovalResult::Denied) => {
                                emit_tool_audit(json!({
                                    "event": "tool.approval_decision",
                                    "tool_id": tool_id.clone(),
                                    "tool_name": tool_name.clone(),
                                    "decision": "denied",
                                    "caller": caller_type_for_tool_use(tool_caller.as_ref()),
                                }));
                                (
                                    Some(Err(ToolError::permission_denied(format!(
                                        "Tool '{tool_name}' denied by user"
                                    )))),
                                    None,
                                )
                            }
                            Ok(ApprovalResult::RetryWithPolicy(policy)) => {
                                emit_tool_audit(json!({
                                    "event": "tool.approval_decision",
                                    "tool_id": tool_id.clone(),
                                    "tool_name": tool_name.clone(),
                                    "decision": "retry_with_policy",
                                    "policy": format!("{policy:?}"),
                                    "caller": caller_type_for_tool_use(tool_caller.as_ref()),
                                }));
                                let elevated_context = tool_registry.map(|r| {
                                    r.context().clone().with_elevated_sandbox_policy(policy)
                                });
                                (None, elevated_context)
                            }
                            Err(err) => (Some(Err(err)), None),
                        }
                    } else {
                        (None, None)
                    };

                    // Per-tool snapshot for surgical undo (#384): capture workspace
                    // state before file-modifying tools execute so `/undo` can
                    // revert the most recent write_file/edit_file/apply_patch.
                    if result_override.is_none()
                        && matches!(
                            tool_name.as_str(),
                            "write_file" | "edit_file" | "apply_patch"
                        )
                    {
                        let ws = engine.session.workspace.clone();
                        let tid = tool_id.clone();
                        let _ = tokio::task::spawn_blocking(move || {
                            crate::core::turn::pre_tool_snapshot(&ws, &tid)
                        })
                        .await;
                    }

                    let started_at = Instant::now();
                    let mut result = if let Some(result_override) = result_override {
                        result_override
                    } else {
                        Engine::execute_tool_with_lock(
                            tool_exec_lock.clone(),
                            plan.supports_parallel,
                            plan.interactive,
                            engine.tx_event.clone(),
                            tool_name.clone(),
                            tool_input.clone(),
                            tool_registry,
                            mcp_pool.clone(),
                            context_override,
                            Some(tool_id.clone()),
                        )
                        .await
                    };

                    // #500: spill outsized tool outputs to disk before the
                    // result fans out to the model context and the UI cell.
                    // Both consumers see the same truncated content + the
                    // `spillover_path` metadata pointing at the full file.
                    // Emit a discrete `tool.spillover` audit event so
                    // operators can correlate large-output episodes with
                    // disk-usage growth in `~/.deepseek/tool_outputs/`.
                    if let Ok(tool_result) = result.as_mut()
                        && let Some(path) =
                            crate::tools::truncate::apply_spillover(tool_result, &tool_id)
                    {
                        emit_tool_audit(json!({
                            "event": "tool.spillover",
                            "tool_id": tool_id.clone(),
                            "tool_name": tool_name.clone(),
                            "path": path.display().to_string(),
                        }));
                    }

                    let _ = engine
                        .tx_event
                        .send(Event::ToolCallComplete {
                            id: tool_id.clone(),
                            name: tool_name.clone(),
                            result: result.clone(),
                        })
                        .await;

                    outcomes[plan.index] = Some(ToolExecOutcome {
                        index: plan.index,
                        id: tool_id,
                        name: tool_name,
                        input: tool_input,
                        started_at,
                        result,
                    });
                }
            }

            let mut step_error_count = 0usize;
            // Categorized tool errors collected this step. Feeds the capacity
            // controller's error-escalation checkpoint so it can distinguish
            // (e.g.) a Tool failure that should escalate from a permission
            // denial that should not.
            let mut step_error_categories: Vec<ErrorCategory> = Vec::new();
            let mut stop_after_plan_tool = false;
            let mut loop_guard_halt: Option<String> = None;

            for outcome in outcomes.into_iter().flatten() {
                let duration = outcome.started_at.elapsed();
                let tool_input = outcome.input.clone();
                let tool_name_for_ws = outcome.name.clone();
                let mut tool_call =
                    TurnToolCall::new(outcome.id.clone(), outcome.name.clone(), outcome.input);
                let should_stop_this_turn = should_stop_after_plan_tool(
                    turn_loop_to_app_mode(mode),
                    &outcome.name,
                    &outcome.result,
                );

                match outcome.result {
                    Ok(output) => {
                        scratchpad_flow::record_tool_outcome(
                            &mut engine.scratchpad_step,
                            &outcome.name,
                            output.success,
                        );
                        match loop_guard.record_outcome(&outcome.name, output.success) {
                            OutcomeDecision::Continue => {}
                            OutcomeDecision::Warn(message) => {
                                crate::logging::warn(message.clone());
                                let _ = engine.tx_event.send(Event::status(message)).await;
                            }
                            OutcomeDecision::Halt(message) => {
                                loop_guard_halt.get_or_insert(message);
                            }
                        }
                        emit_tool_audit(json!({
                            "event": "tool.result",
                            "tool_id": outcome.id.clone(),
                            "tool_name": outcome.name.clone(),
                            "success": output.success,
                        }));
                        let output_for_context = compact_tool_result_for_context(
                            &engine.session.model,
                            &outcome.name,
                            &output,
                        );
                        let output_content = output.content;

                        tool_call.set_result(output_content.clone(), duration);
                        engine.session.working_set.observe_tool_call(
                            &tool_name_for_ws,
                            &tool_input,
                            Some(&output_for_context),
                            &engine.session.workspace,
                        );

                        // #136: post-edit LSP diagnostics hook. We only run
                        // this on success — failed edits leave the file
                        // untouched, so polling for diagnostics would just
                        // surface stale state.
                        if output.success {
                            engine.run_post_edit_lsp_hook(&outcome.name, &tool_input)
                                .await;
                        }

                        engine.add_session_message(Message {
                            role: "user".to_string(),
                            content: vec![ContentBlock::ToolResult {
                                tool_use_id: outcome.id,
                                content: output_for_context,
                                is_error: None,
                                content_blocks: None,
                            }],
                        })
                        .await;
                    }
                    Err(e) => {
                        match loop_guard.record_outcome(&outcome.name, false) {
                            OutcomeDecision::Continue => {}
                            OutcomeDecision::Warn(message) => {
                                crate::logging::warn(message.clone());
                                let _ = engine.tx_event.send(Event::status(message)).await;
                            }
                            OutcomeDecision::Halt(message) => {
                                loop_guard_halt.get_or_insert(message);
                            }
                        }
                        let envelope: ErrorEnvelope = e.clone().into();
                        emit_tool_audit(json!({
                            "event": "tool.result",
                            "tool_id": outcome.id.clone(),
                            "tool_name": outcome.name.clone(),
                            "success": false,
                            "error": e.to_string(),
                            "category": envelope.category.to_string(),
                            "severity": envelope.severity.to_string(),
                        }));
                        step_error_count += 1;
                        step_error_categories.push(envelope.category);
                        let error = format_tool_error(&e, &outcome.name);
                        tool_call.set_error(error.clone(), duration);
                        engine.session.working_set.observe_tool_call(
                            &tool_name_for_ws,
                            &tool_input,
                            Some(&error),
                            &engine.session.workspace,
                        );
                        engine.add_session_message(Message {
                            role: "user".to_string(),
                            content: vec![ContentBlock::ToolResult {
                                tool_use_id: outcome.id,
                                content: format!("Error: {error}"),
                                is_error: Some(true),
                                content_blocks: None,
                            }],
                        })
                        .await;
                    }
                }

                turn.record_tool_call(tool_call);
                stop_after_plan_tool |= should_stop_this_turn;
            }

    let mut outcome = TurnLoopToolPhaseOutcome {
        step_error_count,
        step_error_categories,
        break_outer_loop: stop_after_plan_tool || loop_guard_halt.is_some(),
        continue_outer_loop: false,
    };
    if let Some(message) = loop_guard_halt {
        crate::logging::warn(message.clone());
        let _ = engine.tx_event.send(Event::status(message)).await;
    }
    outcome.continue_outer_loop = engine
        .run_capacity_post_tool_checkpoint(
            turn,
            turn_loop_to_app_mode(mode),
            tool_registry,
            tool_exec_lock,
            mcp_pool,
            step_error_count,
            consecutive_tool_error_steps,
        )
        .await;
    outcome
}
