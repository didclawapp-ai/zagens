//! Health and internal probe routes (R-003 A4.5).

use axum::extract::State;
use axum::Json;
use serde::Serialize;

use super::RuntimeApiState;

#[derive(Debug, Serialize)]
pub(crate) struct HealthResponse {
    status: &'static str,
    service: &'static str,
    mode: &'static str,
}

#[derive(Debug, Serialize)]
pub(crate) struct InternalProbeResponse {
    status: &'static str,
    pid: u32,
    started_at_ms: u128,
    token_fingerprint: String,
    version: &'static str,
}

pub(crate) async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service: "deepseek-runtime-api",
        mode: "local",
    })
}

pub(crate) async fn internal_probe(State(state): State<RuntimeApiState>) -> Json<InternalProbeResponse> {
    Json(InternalProbeResponse {
        status: "ok",
        pid: std::process::id(),
        started_at_ms: state.process_started_at_ms,
        token_fingerprint: state.token_fingerprint.as_ref().clone(),
        version: env!("CARGO_PKG_VERSION"),
    })
}

