//! Health and internal probe routes (R-003 A4.5).

use axum::extract::State;
use axum::Json;
use serde::Serialize;

use deepseek_runtime_orchestrator::runtime_threads::CURRENT_EVENT_SCHEMA_VERSION;

use crate::state::RuntimeApiProbeState;

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    status: &'static str,
    service: &'static str,
    mode: &'static str,
    event_schema_version: u32,
}

#[derive(Debug, Serialize)]
pub struct InternalProbeResponse {
    status: &'static str,
    pid: u32,
    started_at_ms: u128,
    token_fingerprint: String,
    version: &'static str,
}

pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service: "deepseek-runtime-api",
        mode: "local",
        event_schema_version: CURRENT_EVENT_SCHEMA_VERSION,
    })
}

pub async fn internal_probe<S>(State(state): State<S>) -> Json<InternalProbeResponse>
where
    S: RuntimeApiProbeState,
{
    Json(InternalProbeResponse {
        status: "ok",
        pid: std::process::id(),
        started_at_ms: state.process_started_at_ms(),
        token_fingerprint: state.token_fingerprint().to_string(),
        version: state.service_version(),
    })
}
