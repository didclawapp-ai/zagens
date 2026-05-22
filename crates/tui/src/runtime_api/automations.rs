//! Scheduled automations HTTP handlers (R-003 A4.5).

use axum::extract::{Path as AxumPath, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;

use crate::automation_manager::{
    AutomationRecord, AutomationRunRecord, CreateAutomationRequest, UpdateAutomationRequest,
};

use super::{ApiError, RuntimeApiState};

#[derive(Debug, Deserialize)]
pub(crate) struct AutomationRunsQuery {
    limit: Option<usize>,
}

pub(crate) async fn list_automations(
    State(state): State<RuntimeApiState>,
) -> Result<Json<Vec<AutomationRecord>>, ApiError> {
    let manager = state.automations.lock().await;
    let automations = manager
        .list_automations()
        .map_err(|e| ApiError::internal(format!("Failed to list automations: {e}")))?;
    Ok(Json(automations))
}

pub(crate) async fn create_automation(
    State(state): State<RuntimeApiState>,
    Json(req): Json<CreateAutomationRequest>,
) -> Result<(StatusCode, Json<AutomationRecord>), ApiError> {
    let manager = state.automations.lock().await;
    let automation = manager
        .create_automation(req)
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    Ok((StatusCode::CREATED, Json(automation)))
}

pub(crate) async fn get_automation(
    State(state): State<RuntimeApiState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<AutomationRecord>, ApiError> {
    let manager = state.automations.lock().await;
    let automation = manager.get_automation(&id).map_err(map_automation_err)?;
    Ok(Json(automation))
}

pub(crate) async fn update_automation(
    State(state): State<RuntimeApiState>,
    AxumPath(id): AxumPath<String>,
    Json(req): Json<UpdateAutomationRequest>,
) -> Result<Json<AutomationRecord>, ApiError> {
    let manager = state.automations.lock().await;
    let automation = manager
        .update_automation(&id, req)
        .map_err(map_automation_err)?;
    Ok(Json(automation))
}

pub(crate) async fn delete_automation(
    State(state): State<RuntimeApiState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<AutomationRecord>, ApiError> {
    let manager = state.automations.lock().await;
    let automation = manager.delete_automation(&id).map_err(map_automation_err)?;
    Ok(Json(automation))
}

pub(crate) async fn run_automation(
    State(state): State<RuntimeApiState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<AutomationRunRecord>, ApiError> {
    let manager = state.automations.lock().await;
    let run = manager
        .run_now(&id, &state.task_manager)
        .await
        .map_err(map_automation_err)?;
    Ok(Json(run))
}

pub(crate) async fn pause_automation(
    State(state): State<RuntimeApiState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<AutomationRecord>, ApiError> {
    let manager = state.automations.lock().await;
    let automation = manager.pause_automation(&id).map_err(map_automation_err)?;
    Ok(Json(automation))
}

pub(crate) async fn resume_automation(
    State(state): State<RuntimeApiState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<AutomationRecord>, ApiError> {
    let manager = state.automations.lock().await;
    let automation = manager.resume_automation(&id).map_err(map_automation_err)?;
    Ok(Json(automation))
}

pub(crate) async fn list_automation_runs(
    State(state): State<RuntimeApiState>,
    AxumPath(id): AxumPath<String>,
    Query(query): Query<AutomationRunsQuery>,
) -> Result<Json<Vec<AutomationRunRecord>>, ApiError> {
    let manager = state.automations.lock().await;
    let runs = manager
        .list_runs(&id, query.limit)
        .map_err(map_automation_err)?;
    Ok(Json(runs))
}

fn map_automation_err(err: anyhow::Error) -> ApiError {
    let message = err.to_string();
    if message.contains("Failed to read automation")
        || message.contains("No such file or directory")
    {
        ApiError::not_found(message)
    } else {
        ApiError::bad_request(message)
    }
}

