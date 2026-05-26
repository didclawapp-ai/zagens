//! CRAFT blackboard HTTP handlers (B-L1).

use axum::extract::{Path as AxumPath, State};
use axum::Json;
use serde::Serialize;
use serde_json::Value;

use crate::tools::subagent::blackboard::{
    list_blackboard_tasks, read_blackboard_raw, validate_task_id,
};

use super::{ApiError, RuntimeApiState};

#[derive(Serialize)]
pub(crate) struct BlackboardListResponse {
    tasks: Vec<String>,
}

pub(crate) async fn list_blackboards(
    State(state): State<RuntimeApiState>,
) -> Result<Json<BlackboardListResponse>, ApiError> {
    Ok(Json(BlackboardListResponse {
        tasks: list_blackboard_tasks(&state.workspace),
    }))
}

pub(crate) async fn get_blackboard(
    State(state): State<RuntimeApiState>,
    AxumPath(task_id): AxumPath<String>,
) -> Result<Json<Value>, ApiError> {
    let task_id = task_id.trim();
    if task_id.is_empty() {
        return Err(ApiError::bad_request("task_id is required"));
    }
    validate_task_id(task_id).map_err(ApiError::bad_request)?;
    read_blackboard_raw(&state.workspace, task_id)
        .map(Json)
        .ok_or_else(|| ApiError::not_found(format!("blackboard not found: {task_id}")))
}
