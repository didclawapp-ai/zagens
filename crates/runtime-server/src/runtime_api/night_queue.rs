//! Night queue HTTP handlers — desktop enqueue UI (Phase 1a.5).

use std::path::PathBuf;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;

use zagens_runtime_api::night_queue::{
    GatePresetWire, GatePresetsResponse, NewNightQueueTaskRequest, NightQueueBriefingRequest,
    NightQueueBriefingResponse, NightQueueClearFinishedResponse, NightQueueMutateResponse,
    NightQueueResponse, NightQueueStopResponse, QueueTaskStatus as WireQueueTaskStatus,
    QueueTaskWire, RunNightQueueRequest, RunNightQueueResponse,
};

use crate::cli::context::CliContext;
use crate::night_queue::{
    self, EnqueueGateInput, NightQueueDocument, QueueTask, QueueTaskStatus, RunOptions,
    render_briefing, resolve_gate_specs,
};

use super::{ApiError, RuntimeApiState};

pub(crate) async fn get_night_queue(
    State(state): State<RuntimeApiState>,
) -> Result<Json<NightQueueResponse>, ApiError> {
    let doc = night_queue::load(&state.workspace).map_err(map_queue_err)?;
    Ok(Json(to_response(&state.workspace, &doc)))
}

pub(crate) async fn create_night_queue_task(
    State(state): State<RuntimeApiState>,
    Json(req): Json<NewNightQueueTaskRequest>,
) -> Result<(StatusCode, Json<QueueTaskWire>), ApiError> {
    if req.prompt.trim().is_empty() {
        return Err(ApiError::bad_request("prompt is required"));
    }
    let gate_file = resolve_gate_file(&state.workspace, req.gate_file)?;
    let gate = resolve_gate_specs(&EnqueueGateInput {
        gate: req.gate,
        gate_file,
        gate_preset: req.gate_preset,
    })
    .map_err(map_queue_err)?;
    let task = night_queue::enqueue(&state.workspace, req.prompt, gate, req.use_worktree)
        .map_err(map_queue_err)?;
    crate::night_queue::dispatch_enqueue(&state.config, &state.workspace, &task.id, &task.prompt);
    Ok((StatusCode::CREATED, Json(to_task_wire(&task))))
}

pub(crate) async fn run_night_queue(
    State(state): State<RuntimeApiState>,
    Json(req): Json<RunNightQueueRequest>,
) -> Result<Json<RunNightQueueResponse>, ApiError> {
    let ctx = CliContext {
        config: state.config.clone(),
        workspace: state.workspace.clone(),
    };
    let report = night_queue::run_pending(
        &ctx,
        &state.config,
        RunOptions {
            max_parallel: req.max_parallel.max(1),
            use_worktree: req.use_worktree,
            write_briefing: req.write_briefing,
        },
    )
    .await
    .map_err(map_queue_err)?;
    Ok(Json(RunNightQueueResponse {
        ran: report.ran,
        passed: report.passed,
        failed: report.failed,
        canceled: report.canceled,
    }))
}

pub(crate) async fn stop_night_queue(
    State(state): State<RuntimeApiState>,
) -> Result<Json<NightQueueStopResponse>, ApiError> {
    let stopped = night_queue::request_stop(&state.workspace);
    let reclaimed = if stopped {
        0
    } else {
        night_queue::reclaim_stale_running(&state.workspace)
            .map_err(map_queue_err)?
            .len()
    };
    Ok(Json(NightQueueStopResponse { stopped, reclaimed }))
}

pub(crate) async fn cancel_night_queue_task(
    State(state): State<RuntimeApiState>,
    Path(id): Path<String>,
) -> Result<Json<NightQueueMutateResponse>, ApiError> {
    let doc = night_queue::load(&state.workspace).map_err(map_queue_err)?;
    let task = doc
        .tasks
        .iter()
        .find(|t| t.id == id)
        .ok_or_else(|| ApiError::bad_request(format!("queue task not found: {id}")))?;
    if task.status == QueueTaskStatus::Running {
        let _ = night_queue::request_stop(&state.workspace);
    }
    let task = night_queue::cancel_task(&state.workspace, &id).map_err(map_queue_err)?;
    Ok(Json(NightQueueMutateResponse {
        task: to_task_wire(&task),
    }))
}

pub(crate) async fn delete_night_queue_task(
    State(state): State<RuntimeApiState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    night_queue::remove_task(&state.workspace, &id).map_err(map_queue_err)?;
    Ok(StatusCode::NO_CONTENT)
}

pub(crate) async fn retry_night_queue_task(
    State(state): State<RuntimeApiState>,
    Path(id): Path<String>,
) -> Result<(StatusCode, Json<NightQueueMutateResponse>), ApiError> {
    let task = night_queue::retry_task(&state.workspace, &id).map_err(map_queue_err)?;
    crate::night_queue::dispatch_enqueue(&state.config, &state.workspace, &task.id, &task.prompt);
    Ok((
        StatusCode::CREATED,
        Json(NightQueueMutateResponse {
            task: to_task_wire(&task),
        }),
    ))
}

pub(crate) async fn clear_night_queue_finished(
    State(state): State<RuntimeApiState>,
) -> Result<Json<NightQueueClearFinishedResponse>, ApiError> {
    let removed = night_queue::clear_finished(&state.workspace).map_err(map_queue_err)?;
    Ok(Json(NightQueueClearFinishedResponse { removed }))
}

pub(crate) async fn post_night_queue_briefing(
    State(state): State<RuntimeApiState>,
    Json(req): Json<NightQueueBriefingRequest>,
) -> Result<Json<NightQueueBriefingResponse>, ApiError> {
    let doc = night_queue::load(&state.workspace).map_err(map_queue_err)?;
    let markdown = render_briefing(&doc);
    let handoff_path = if req.write_handoff {
        night_queue::write_briefing_to_handoff(&state.workspace, &doc)
            .map_err(|e| ApiError::bad_request(e.to_string()))?;
        Some(zagens_config::workspace_meta_file_write(
            &state.workspace,
            "handoff.md",
        ))
    } else {
        None
    };
    Ok(Json(NightQueueBriefingResponse {
        markdown,
        handoff_path,
    }))
}

pub(crate) async fn list_gate_presets(
    State(_state): State<RuntimeApiState>,
) -> Result<Json<GatePresetsResponse>, ApiError> {
    let presets = crate::cli::handlers::gate::bundled_gate_presets()
        .map(|(id, description)| GatePresetWire {
            id: id.to_string(),
            description: description.to_string(),
        })
        .collect();
    Ok(Json(GatePresetsResponse { presets }))
}

fn resolve_gate_file(
    workspace: &std::path::Path,
    gate_file: Option<PathBuf>,
) -> Result<Option<PathBuf>, ApiError> {
    let Some(path) = gate_file else {
        return Ok(None);
    };
    let resolved = if path.is_absolute() {
        path
    } else {
        workspace.join(path)
    };
    if !resolved.is_file() {
        return Err(ApiError::bad_request(format!(
            "gate_file not found: {}",
            resolved.display()
        )));
    }
    Ok(Some(resolved))
}

fn to_response(workspace: &std::path::Path, doc: &NightQueueDocument) -> NightQueueResponse {
    NightQueueResponse {
        schema_version: doc.schema_version,
        last_run_at: doc.last_run_at,
        tasks: doc.tasks.iter().map(to_task_wire).collect(),
        queue_path: night_queue::queue_path(workspace),
    }
}

fn to_task_wire(task: &QueueTask) -> QueueTaskWire {
    QueueTaskWire {
        id: task.id.clone(),
        prompt: task.prompt.clone(),
        status: match task.status {
            QueueTaskStatus::Pending => WireQueueTaskStatus::Pending,
            QueueTaskStatus::Running => WireQueueTaskStatus::Running,
            QueueTaskStatus::Passed => WireQueueTaskStatus::Passed,
            QueueTaskStatus::Failed => WireQueueTaskStatus::Failed,
            QueueTaskStatus::RolledBack => WireQueueTaskStatus::RolledBack,
            QueueTaskStatus::Canceled => WireQueueTaskStatus::Canceled,
        },
        worktree_path: task.worktree_path.clone(),
        gate: task
            .gate
            .iter()
            .map(|g| zagens_runtime_api::night_queue::GatePredicateWire {
                predicate: g.predicate.clone(),
                args: g.args.clone(),
            })
            .collect(),
        created_at: task.created_at,
        started_at: task.started_at,
        finished_at: task.finished_at,
        pre_snapshot_id: task.pre_snapshot_id.clone(),
        gate_summary: task.gate_summary.clone(),
        error: task.error.clone(),
    }
}

fn map_queue_err(err: anyhow::Error) -> ApiError {
    ApiError::bad_request(err.to_string())
}
