//! Parallel + sequential tool plan execution (P2 PR6b — TUI L2; called from `TurnLoopHost::execute_tool_plans`).

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

use deepseek_core::chat::Tool;
use deepseek_core::engine::dispatch::caller_type_for_tool_use;
use deepseek_core::engine::emit_tool_audit;
use deepseek_core::engine::turn_loop::{ToolExecOutcome, ToolExecutionPlan};
use deepseek_core::turn::TurnLoopMode;
use deepseek_tools::{ToolError, ToolResult};
use futures_util::stream::FuturesUnordered;
use futures_util::StreamExt;
use serde_json::json;
use tokio::sync::{Mutex as AsyncMutex, RwLock};

use super::super::approval::ApprovalResult;
use super::super::dispatch::should_parallelize_tool_batch;
use super::super::tool_catalog::{
    execute_code_execution_tool, execute_tool_search, is_tool_search_tool,
    CODE_EXECUTION_TOOL_NAME, MULTI_TOOL_PARALLEL_NAME, REQUEST_USER_INPUT_NAME,
};
use super::super::tool_execution::{
    apply_tool_spillover_audit, detached_execute_with_lock, execute_plan_on_engine,
};
use deepseek_core::engine::turn_loop::TurnLoopToolExec;
use super::Engine;
use crate::core::events::Event;
use crate::mcp::McpPool;
use crate::tools::user_input::UserInputRequest;

pub(super) async fn execute_tool_plans(
    engine: &mut Engine,
    _mode: TurnLoopMode,
    plans: Vec<ToolExecutionPlan>,
    tool_catalog: &[Tool],
    active_tool_names: &mut HashSet<String>,
    tool_registry: Option<&crate::tools::ToolRegistry>,
    mcp_pool: Option<Arc<AsyncMutex<McpPool>>>,
    tool_exec_lock: Arc<RwLock<()>>,
) -> Vec<ToolExecOutcome> {
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
                        let exec = TurnLoopToolExec {
                            lock,
                            tx_event: tx_event.clone(),
                        };
                        let mut result = detached_execute_with_lock(
                            exec,
                            plan.supports_parallel,
                            plan.interactive,
                            plan.name.clone(),
                            plan.input.clone(),
                            registry,
                            mcp_pool,
                            Some(plan.id.clone()),
                        )
                        .await;

                        if let Ok(ref mut tool_result) = result {
                            apply_tool_spillover_audit(tool_result, &plan.id, &plan.name);
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
                        let exec = TurnLoopToolExec {
                            lock: tool_exec_lock.clone(),
                            tx_event: engine.tx_event.clone(),
                        };
                        execute_plan_on_engine(
                            exec,
                            plan.supports_parallel,
                            plan.interactive,
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
                    if let Ok(ref mut tool_result) = result {
                        apply_tool_spillover_audit(tool_result, &tool_id, &tool_name);
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
    outcomes.into_iter().flatten().collect()
}
