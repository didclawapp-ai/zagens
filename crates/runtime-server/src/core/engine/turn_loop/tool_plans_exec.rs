//! Parallel + sequential tool plan execution (P2 PR6b — TUI L2; called from `TurnLoopHost::execute_tool_plans`).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use futures_util::StreamExt;
use futures_util::stream::FuturesUnordered;
use serde_json::json;
use tokio::sync::{Mutex as AsyncMutex, RwLock};
use zagens_core::chat::Tool;
use zagens_core::engine::dispatch::caller_type_for_tool_use;
use zagens_core::engine::emit_tool_audit;
use zagens_core::engine::turn_loop::{ToolExecOutcome, ToolExecutionPlan};
use zagens_core::turn::TurnLoopMode;
use zagens_tools::{ToolError, ToolResult};

use super::super::approval::ApprovalResult;
use super::super::dispatch::should_parallelize_tool_batch;
use super::super::hook_dispatch::fire_tool_call_after_with_executor;
use super::super::tool_catalog::{
    CODE_EXECUTION_TOOL_NAME, MULTI_TOOL_PARALLEL_NAME, REQUEST_USER_INPUT_NAME,
    execute_code_execution_tool, execute_tool_search, is_tool_search_tool,
};
use super::super::tool_execution::{
    apply_tool_spillover_audit, detached_execute_with_lock, execute_plan_on_engine,
};
use super::Engine;
use crate::agent_surface::AppMode;
use crate::core::events::Event;
use crate::mcp::McpPool;
use crate::tools::resource_locks::FineGrainedLockContext;
use crate::tools::schedule_bridge::{self, ScheduleContext};
use crate::tools::user_input::UserInputRequest;
use zagens_core::engine::turn_loop::TurnLoopToolExec;

fn schedule_context(engine: &Engine) -> ScheduleContext {
    let sandbox_enforced = engine
        .runtime_ext()
        .shell_manager
        .lock()
        .map(|m| m.probe_sandbox_enforced())
        .unwrap_or(false);
    ScheduleContext { sandbox_enforced }
}

fn fine_grained_lock_ctx(
    registry: Arc<crate::tools::resource_locks::ResourceLockRegistry>,
    plan: &ToolExecutionPlan,
    ctx: &ScheduleContext,
) -> FineGrainedLockContext {
    let view = schedule_bridge::dag_plan_view(plan, ctx);
    FineGrainedLockContext {
        registry,
        reads: view.reads,
        writes: view.writes,
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn execute_tool_plans(
    engine: &mut Engine,
    mode: TurnLoopMode,
    plans: Vec<ToolExecutionPlan>,
    tool_catalog: &[Tool],
    active_tool_names: &mut HashSet<String>,
    tool_registry: Option<&crate::tools::ToolRegistry>,
    mcp_pool: Option<Arc<AsyncMutex<McpPool>>>,
    tool_exec_lock: Arc<RwLock<()>>,
) -> Vec<ToolExecOutcome> {
    let scheduler = engine.runtime_ext().tools_scheduler;
    let ctx = schedule_context(engine);
    let fine_grained_locks = scheduler.uses_dag_groups();
    let lock_registry = if fine_grained_locks {
        Some(Arc::clone(&engine.runtime_ext().resource_lock_registry))
    } else {
        None
    };
    let groups = schedule_bridge::resolve_execution_groups(scheduler, &plans, &ctx);
    if scheduler.uses_dag_groups() && groups.len() > 1 {
        let _ = engine
            .tx_event
            .send(Event::status(format!(
                "DAG scheduler: {} execution wave(s) for {} tool(s)",
                groups.len(),
                plans.len()
            )))
            .await;
    }

    let mut outcomes: Vec<Option<ToolExecOutcome>> = Vec::with_capacity(plans.len());
    outcomes.resize_with(plans.len(), || None);

    for group in groups {
        let subgroups = if scheduler.uses_dag_groups() {
            schedule_bridge::split_wave_execution_subgroups(&plans, &group, &ctx)
        } else {
            vec![group.clone()]
        };
        for subgroup in subgroups {
            let batch: Vec<ToolExecutionPlan> =
                subgroup.iter().map(|&i| plans[i].clone()).collect();
            let parallel_override = if scheduler.uses_dag_groups() {
                Some(schedule_bridge::wave_parallel_allowed(
                    &plans, &subgroup, &ctx,
                ))
            } else {
                None
            };
            let batch_outcomes = execute_tool_plans_batch(
                engine,
                mode,
                batch,
                tool_catalog,
                active_tool_names,
                tool_registry,
                mcp_pool.clone(),
                tool_exec_lock.clone(),
                parallel_override,
                fine_grained_locks,
                lock_registry.clone(),
                ctx,
            )
            .await;
            for outcome in batch_outcomes {
                let idx = outcome.index;
                outcomes[idx] = Some(outcome);
            }
        }
    }
    outcomes.into_iter().flatten().collect()
}

#[allow(clippy::too_many_arguments)]
async fn execute_tool_plans_batch(
    engine: &mut Engine,
    _mode: TurnLoopMode,
    plans: Vec<ToolExecutionPlan>,
    tool_catalog: &[Tool],
    active_tool_names: &mut HashSet<String>,
    tool_registry: Option<&crate::tools::ToolRegistry>,
    mcp_pool: Option<Arc<AsyncMutex<McpPool>>>,
    tool_exec_lock: Arc<RwLock<()>>,
    parallel_override: Option<bool>,
    fine_grained_locks: bool,
    lock_registry: Option<Arc<crate::tools::resource_locks::ResourceLockRegistry>>,
    schedule_ctx: ScheduleContext,
) -> Vec<ToolExecOutcome> {
    let app_mode = engine.runtime_ext().turn_app_mode;
    let parallel_allowed =
        parallel_override.unwrap_or_else(|| should_parallelize_tool_batch(&plans));
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
    // `plan.index` is global within the full tool batch; this vec is batch-local.
    let batch_local_index: HashMap<usize, usize> = plans
        .iter()
        .enumerate()
        .map(|(local, plan)| (plan.index, local))
        .collect();
    let local_slot =
        |global: usize| -> usize { batch_local_index.get(&global).copied().unwrap_or(global) };

    if parallel_allowed {
        let mut tool_tasks = FuturesUnordered::new();
        let wave_parallel = parallel_override == Some(true);
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
                outcomes[local_slot(plan.index)] = Some(ToolExecOutcome {
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
                outcomes[local_slot(plan.index)] = Some(ToolExecOutcome {
                    index: plan.index,
                    id: plan.id,
                    name: plan.name,
                    input: plan.input,
                    started_at: Instant::now(),
                    result: Err(err),
                });
                continue;
            }
            let mut effective_input = plan.input.clone();
            match engine.fire_tool_call_before(app_mode, &plan.name, &effective_input) {
                Err(blocked) => {
                    let result = Err(ToolError::execution_failed(blocked));
                    let _ = engine
                        .tx_event
                        .send(Event::ToolCallComplete {
                            id: plan.id.clone(),
                            name: plan.name.clone(),
                            result: result.clone(),
                        })
                        .await;
                    outcomes[local_slot(plan.index)] = Some(ToolExecOutcome {
                        index: plan.index,
                        id: plan.id,
                        name: plan.name,
                        input: effective_input,
                        started_at: Instant::now(),
                        result,
                    });
                    continue;
                }
                Ok(Some(updated)) => effective_input = updated,
                Ok(None) => {}
            }
            let registry = tool_registry;
            let lock = tool_exec_lock.clone();
            let mcp_pool = mcp_pool.clone();
            let tx_event = engine.tx_event.clone();
            let hook_executor = Arc::clone(&engine.runtime_ext().hook_executor);
            let hook_ctx = engine.hook_context(app_mode);
            let started_at = Instant::now();
            let plan_name = plan.name.clone();
            let plan_id = plan.id.clone();
            let use_parallel_lock = plan.supports_parallel || wave_parallel;
            let lock_ctx = if fine_grained_locks {
                lock_registry.as_ref().map(|registry| {
                    fine_grained_lock_ctx(Arc::clone(registry), &plan, &schedule_ctx)
                })
            } else {
                None
            };

            tool_tasks.push(async move {
                let exec = TurnLoopToolExec {
                    lock,
                    tx_event: tx_event.clone(),
                };
                let mut result = detached_execute_with_lock(
                    exec,
                    use_parallel_lock,
                    plan.interactive,
                    plan_name.clone(),
                    effective_input.clone(),
                    registry,
                    mcp_pool,
                    Some(plan_id.clone()),
                    lock_ctx,
                )
                .await;

                if let Ok(ref mut tool_result) = result {
                    apply_tool_spillover_audit(tool_result, &plan_id, &plan_name);
                }

                fire_tool_call_after_with_executor(
                    &hook_executor,
                    hook_ctx,
                    &plan_name,
                    &effective_input,
                    &result,
                );

                let _ = tx_event
                    .send(Event::ToolCallComplete {
                        id: plan_id.clone(),
                        name: plan_name.clone(),
                        result: result.clone(),
                    })
                    .await;

                ToolExecOutcome {
                    index: plan.index,
                    id: plan_id,
                    name: plan_name,
                    input: effective_input,
                    started_at,
                    result,
                }
            });
        }

        while let Some(outcome) = tool_tasks.next().await {
            let slot = local_slot(outcome.index);
            outcomes[slot] = Some(outcome);
        }
    } else {
        for plan in plans {
            let tool_id = plan.id.clone();
            let tool_name = plan.name.clone();
            let mut tool_input = plan.input.clone();
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
                outcomes[local_slot(plan.index)] = Some(ToolExecOutcome {
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
                outcomes[local_slot(plan.index)] = Some(ToolExecOutcome {
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

                outcomes[local_slot(plan.index)] = Some(ToolExecOutcome {
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

                outcomes[local_slot(plan.index)] = Some(ToolExecOutcome {
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
                let result =
                    execute_tool_search(&tool_name, &tool_input, tool_catalog, active_tool_names);

                let _ = engine
                    .tx_event
                    .send(Event::ToolCallComplete {
                        id: tool_id.clone(),
                        name: tool_name.clone(),
                        result: result.clone(),
                    })
                    .await;

                outcomes[local_slot(plan.index)] = Some(ToolExecOutcome {
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
                    Ok(request) => {
                        engine
                            .await_user_input(&tool_id, request)
                            .await
                            .and_then(|response| {
                                ToolResult::json(&response)
                                    .map_err(|e| ToolError::execution_failed(e.to_string()))
                            })
                    }
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

                outcomes[local_slot(plan.index)] = Some(ToolExecOutcome {
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
                if engine.approval_cache_hit(&tool_name, &tool_input) {
                    emit_tool_audit(json!({
                        "event": "tool.approval_cache_hit",
                        "tool_id": tool_id.clone(),
                        "tool_name": tool_name.clone(),
                    }));
                    (None, None)
                } else {
                    emit_tool_audit(json!({
                        "event": "tool.approval_required",
                        "tool_id": tool_id.clone(),
                        "tool_name": tool_name.clone(),
                    }));
                    let approval_key =
                        crate::tools::approval_cache::build_approval_key(&tool_name, &tool_input).0;
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
                        Ok(ApprovalResult::Approved { .. }) => {
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
                            let elevated_context = tool_registry
                                .map(|r| r.context().clone().with_elevated_sandbox_policy(policy));
                            (None, elevated_context)
                        }
                        Err(err) => (Some(Err(err)), None),
                    }
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
                let max_gb = engine.config.snapshots_max_workspace_gb;
                let _ = tokio::task::spawn_blocking(move || {
                    crate::core::turn::pre_tool_snapshot(&ws, &tid, max_gb)
                })
                .await;
            }

            let started_at = Instant::now();
            match engine.fire_tool_call_before(app_mode, &tool_name, &tool_input) {
                Err(blocked) => {
                    let result = Err(ToolError::execution_failed(blocked));
                    let _ = engine
                        .tx_event
                        .send(Event::ToolCallComplete {
                            id: tool_id.clone(),
                            name: tool_name.clone(),
                            result: result.clone(),
                        })
                        .await;
                    engine.fire_tool_call_after(app_mode, &tool_name, &tool_input, &result);
                    outcomes[local_slot(plan.index)] = Some(ToolExecOutcome {
                        index: plan.index,
                        id: tool_id,
                        name: tool_name,
                        input: tool_input,
                        started_at,
                        result,
                    });
                    continue;
                }
                Ok(Some(updated)) => tool_input = updated,
                Ok(None) => {}
            }
            let mut result = if let Some(result_override) = result_override {
                result_override
            } else {
                let lock_ctx = if fine_grained_locks {
                    lock_registry.as_ref().map(|registry| {
                        fine_grained_lock_ctx(Arc::clone(registry), &plan, &schedule_ctx)
                    })
                } else {
                    None
                };
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
                    lock_ctx,
                )
                .await
            };

            engine.fire_tool_call_after(app_mode, &tool_name, &tool_input, &result);

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

            outcomes[local_slot(plan.index)] = Some(ToolExecOutcome {
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
