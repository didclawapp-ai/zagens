//! CRAFT blackboard HTTP handlers (B-L1).

use std::path::PathBuf;

use axum::Json;
use axum::extract::{Path as AxumPath, Query, State};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::tools::subagent::blackboard::{
    list_blackboard_tasks, read_blackboard_raw, validate_task_id,
};

use super::{ApiError, RuntimeApiState};

#[derive(Serialize)]
pub(crate) struct BlackboardListResponse {
    tasks: Vec<String>,
}

/// Optional absolute workspace root (Composer path). Falls back to sidecar default when omitted.
#[derive(Debug, Deserialize)]
pub(crate) struct BlackboardWorkspaceQuery {
    #[serde(default)]
    workspace: Option<String>,
}

fn resolve_blackboard_workspace(
    state: &RuntimeApiState,
    workspace: Option<&str>,
) -> Result<PathBuf, ApiError> {
    let raw = workspace.map(str::trim).unwrap_or("");
    if raw.is_empty() {
        return Ok(state.workspace.clone());
    }
    let path = PathBuf::from(raw);
    let base = path
        .canonicalize()
        .map_err(|e| ApiError::bad_request(format!("workspace: {e}")))?;
    if !base.is_dir() {
        return Err(ApiError::bad_request("workspace is not a directory"));
    }
    Ok(base)
}

pub(crate) async fn list_blackboards(
    State(state): State<RuntimeApiState>,
    Query(q): Query<BlackboardWorkspaceQuery>,
) -> Result<Json<BlackboardListResponse>, ApiError> {
    let ws = resolve_blackboard_workspace(&state, q.workspace.as_deref())?;
    Ok(Json(BlackboardListResponse {
        tasks: list_blackboard_tasks(&ws),
    }))
}

pub(crate) async fn get_blackboard(
    State(state): State<RuntimeApiState>,
    AxumPath(task_id): AxumPath<String>,
    Query(q): Query<BlackboardWorkspaceQuery>,
) -> Result<Json<Value>, ApiError> {
    let task_id = task_id.trim();
    if task_id.is_empty() {
        return Err(ApiError::bad_request("task_id is required"));
    }
    validate_task_id(task_id).map_err(ApiError::bad_request)?;
    let ws = resolve_blackboard_workspace(&state, q.workspace.as_deref())?;
    read_blackboard_raw(&ws, task_id)
        .map(Json)
        .ok_or_else(|| ApiError::not_found(format!("blackboard not found: {task_id}")))
}
