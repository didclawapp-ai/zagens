//! Agent health / T1 telemetry for desktop 「Agent 体检」(Phase 3.3).

use axum::Json;
use axum::extract::State;

use crate::harness::telemetry::{
    ToolTelemetryReport, build_tool_telemetry_report, default_sessions_db_path,
};

use super::{ApiError, RuntimeApiState};

pub(crate) async fn get_agent_health(
    State(_state): State<RuntimeApiState>,
) -> Result<Json<ToolTelemetryReport>, ApiError> {
    let db_path = default_sessions_db_path();
    let report =
        build_tool_telemetry_report(&db_path).map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Json(report))
}
