//! `GET /v1/office/environment` — Office Python / dependency diagnostics.

use axum::Json;
use serde_json::Value;

pub async fn get_office_environment() -> Json<Value> {
    Json(crate::office_env::office_environment_status())
}
