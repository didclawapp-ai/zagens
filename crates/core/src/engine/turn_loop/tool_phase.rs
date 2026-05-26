//! Tool planning + outcome aggregation for one turn step (P2 PR6b — `deepseek-core`).

use std::collections::HashSet;
use deepseek_tools::{ToolError, ToolResult};
use serde_json::json;

use crate::chat::{ContentBlock, Message, Tool};
use crate::engine::context::compact_tool_result_for_context;
use crate::engine::dispatch::{
    caller_allowed_for_tool, caller_type_for_tool_use, format_tool_error,
    should_stop_after_plan_tool,
};
use crate::engine::emit_tool_audit;
use crate::engine::loop_guard::{AttemptDecision, LoopGuard, OutcomeDecision};
use crate::engine::streaming::ToolUseState;
use crate::engine::tool_catalog::{
    CODE_EXECUTION_TOOL_NAME, is_tool_search_tool, missing_tool_error_message,
    REQUEST_USER_INPUT_NAME,
};
use crate::engine::turn_loop::exec::ToolExecutionPlan;
use crate::engine::turn_loop::host::TurnLoopHost;
use crate::engine::turn_loop::control::TurnLoopToolPhaseOutcome;
use crate::error_taxonomy::ErrorEnvelope;
use crate::turn::{TurnContext, TurnLoopMode, TurnToolCall};

pub async fn run_tool_execution_phase<H: TurnLoopHost>(
    host: &mut H,
    turn: &mut TurnContext,
    mode: TurnLoopMode,
    tool_uses: &mut [ToolUseState],
    tool_catalog: &[Tool],
    active_tool_names: &mut HashSet<String>,
    loop_guard: &mut LoopGuard,
    consecutive_tool_error_steps: u32,
    tool_registry: Option<&H::ToolRegistry>,
) -> TurnLoopToolPhaseOutcome {
    let tool_exec_lock = host.tool_exec_lock();
    let mcp_pool = host.ensure_mcp_pool_for_tools(tool_uses).await;

    let mut plans: Vec<ToolExecutionPlan> = Vec::with_capacity(tool_uses.len());
    for (index, tool) in tool_uses.iter_mut().enumerate() {
        let tool_id = tool.id.clone();
        let mut tool_name = tool.name.clone();
        let tool_input = tool.input.clone();
        let tool_caller = tool.caller.clone();

        tracing::info!(
            "Planning tool '{}' with input: {:?}",
            tool_name,
            tool_input
        );

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

        if host.maybe_activate_deferred_tool(&tool_name, tool_catalog, active_tool_names) {
            let _ = host
                .tx_event()
                .send(crate::events::Event::status(format!(
                    "Auto-loaded deferred tool '{tool_name}' after model request."
                )))
                .await;
        }

        let mut tool_def = tool_catalog.iter().find(|def| def.name == tool_name);

        if tool_def.is_none()
            && let Some(canonical) =
                host.resolve_hallucinated_tool_name(&tool_name, tool_catalog, tool_registry)
        {
            tracing::info!(
                "Resolved hallucinated tool name '{}' -> '{}'",
                tool_name,
                canonical
            );
            tool_def = tool_catalog.iter().find(|d| d.name == canonical);
            if tool_def.is_some() {
                tool_name = canonical;
                tool.name = tool_name.clone();
                if host.maybe_activate_deferred_tool(&tool_name, tool_catalog, active_tool_names)
                {
                    let _ = host
                        .tx_event()
                        .send(crate::events::Event::status(format!(
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
            && !host.is_mcp_tool_name(&tool_name)
            && tool_name != CODE_EXECUTION_TOOL_NAME
            && !is_tool_search_tool(&tool_name)
        {
            blocked_error = Some(ToolError::not_available(missing_tool_error_message(
                &tool_name,
                tool_catalog,
            )));
        }

        if blocked_error.is_none() {
            let meta = host.tool_plan_approval_meta(&tool_name, &tool_input, tool_registry);
            approval_required = meta.approval_required;
            approval_description = meta.approval_description;
            supports_parallel = meta.supports_parallel;
            read_only = meta.read_only;
        }

        if blocked_error.is_none()
            && let AttemptDecision::Block(message) =
                loop_guard.record_attempt(&tool_name, &tool_input)
        {
            tracing::warn!("{}", message);
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

    let outcomes = host
        .execute_tool_plans(
            mode,
            plans,
            tool_catalog,
            active_tool_names,
            tool_registry,
            mcp_pool.clone(),
            tool_exec_lock.clone(),
        )
        .await;

    let mut step_error_count = 0usize;
    let mut step_error_categories = Vec::new();
    let mut stop_after_plan_tool = false;
    let mut loop_guard_halt: Option<String> = None;

    for outcome in outcomes {
        let duration = outcome.started_at.elapsed();
        let tool_input = outcome.input.clone();
        let tool_name_for_ws = outcome.name.clone();
        let mut tool_call =
            TurnToolCall::new(outcome.id.clone(), outcome.name.clone(), outcome.input);
        let should_stop_this_turn = should_stop_after_plan_tool(
            mode == TurnLoopMode::Plan,
            &outcome.name,
            &outcome.result,
        );

        match outcome.result {
            Ok(output) => {
                host.record_scratchpad_tool_outcome(&outcome.name, output.success);
                match loop_guard.record_outcome(&outcome.name, output.success) {
                    OutcomeDecision::Continue => {}
                    OutcomeDecision::Warn(message) => {
                        tracing::warn!("{}", message);
                        let _ = host
                            .tx_event()
                            .send(crate::events::Event::status(message))
                            .await;
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
                let workspace = host.workspace().to_path_buf();
                let session_model = host.session_mut().model.clone();
                let output_for_context =
                    compact_tool_result_for_context(&session_model, &outcome.name, &output);
                let output_content = output.content;

                tool_call.set_result(output_content.clone(), duration);
                host.session_mut().working_set.observe_tool_call(
                    &tool_name_for_ws,
                    &tool_input,
                    Some(&output_for_context),
                    &workspace,
                );

                if output.success {
                    host.run_post_edit_lsp_hook(&outcome.name, &tool_input)
                        .await;
                }

                host.add_session_message(Message {
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
                        tracing::warn!("{}", message);
                        let _ = host
                            .tx_event()
                            .send(crate::events::Event::status(message))
                            .await;
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
                let workspace = host.workspace().to_path_buf();
                host.session_mut().working_set.observe_tool_call(
                    &tool_name_for_ws,
                    &tool_input,
                    Some(&error),
                    &workspace,
                );
                host.add_session_message(Message {
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
        tracing::warn!("{}", message);
        let _ = host
            .tx_event()
            .send(crate::events::Event::status(message))
            .await;
    }
    outcome.continue_outer_loop = host
        .run_capacity_post_tool_checkpoint(
            turn,
            mode,
            tool_registry,
            tool_exec_lock,
            mcp_pool,
            step_error_count,
            consecutive_tool_error_steps,
        )
        .await;
    outcome
}
