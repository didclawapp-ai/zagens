//! Background task queue HTTP handlers (R-003 A4.5).

use axum::extract::{Path as AxumPath, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::config::DEFAULT_TEXT_MODEL;
use crate::task_manager::{NewTaskRequest, TaskRecord, TaskSummary};

use super::{ApiError, RuntimeApiState};

#[derive(Debug, Serialize)]
pub(crate) struct TasksResponse {
    tasks: Vec<TaskSummary>,
    counts: crate::task_manager::TaskCounts,
}

#[derive(Debug, Deserialize)]
pub(crate) struct TasksQuery {
    limit: Option<usize>,
}

#[derive(Serialize)]
pub(crate) struct ClearTasksResponse {
    removed: usize,
}

pub(crate) async fn create_task(
    State(state): State<RuntimeApiState>,
    Json(mut req): Json<NewTaskRequest>,
) -> Result<(StatusCode, Json<TaskRecord>), ApiError> {
    if req.prompt.trim().is_empty() {
        return Err(ApiError::bad_request("prompt is required"));
    }
    if req.workspace.is_none() {
        req.workspace = Some(state.workspace.clone());
    }
    if req.model.is_none() {
        req.model = Some(
            state
                .config
                .default_text_model
                .clone()
                .unwrap_or_else(|| DEFAULT_TEXT_MODEL.to_string()),
        );
    }
    let task = state
        .task_manager
        .add_task(req)
        .await
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    Ok((StatusCode::CREATED, Json(task)))
}

pub(crate) async fn list_tasks(
    State(state): State<RuntimeApiState>,
    Query(query): Query<TasksQuery>,
) -> Result<Json<TasksResponse>, ApiError> {
    let tasks = state.task_manager.list_tasks(query.limit).await;
    let counts = state.task_manager.counts().await;
    Ok(Json(TasksResponse { tasks, counts }))
}

pub(crate) async fn get_task(
    State(state): State<RuntimeApiState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<TaskRecord>, ApiError> {
    let task = state
        .task_manager
        .get_task(&id)
        .await
        .map_err(map_task_err)?;
    Ok(Json(task))
}

pub(crate) async fn cancel_task(
    State(state): State<RuntimeApiState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<TaskRecord>, ApiError> {
    let task = state
        .task_manager
        .cancel_task(&id)
        .await
        .map_err(map_task_err)?;
    Ok(Json(task))
}

pub(crate) async fn clear_tasks(
    State(state): State<RuntimeApiState>,
) -> Result<Json<ClearTasksResponse>, ApiError> {
    let removed = state
        .task_manager
        .clear_terminal_tasks()
        .await
        .map_err(map_task_err)?;
    Ok(Json(ClearTasksResponse { removed }))
}

pub(crate) fn map_task_err(err: anyhow::Error) -> ApiError {
    let message = err.to_string();
    if message.contains("not found") {
        ApiError::not_found(message)
    } else {
        ApiError::bad_request(message)
    }
}
