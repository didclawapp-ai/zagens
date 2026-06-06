//! Usage aggregation, routing rules, symbol index rebuild (R-003 A4.5).

use std::path::PathBuf;

use axum::Json;
use axum::extract::{Query, State};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::runtime_threads::UsageGroupBy;

use super::{ApiError, RuntimeApiState};

#[derive(Debug, Deserialize)]
pub(crate) struct UsageQuery {
    /// ISO-8601 lower bound (inclusive). When omitted, no lower bound.
    since: Option<String>,
    /// ISO-8601 upper bound (inclusive). When omitted, no upper bound.
    until: Option<String>,
    /// Bucket key. One of `day` (default), `model`, `provider`, `thread`.
    group_by: Option<String>,
}

pub(crate) fn parse_iso8601(raw: &str, field: &str) -> Result<chrono::DateTime<Utc>, ApiError> {
    chrono::DateTime::parse_from_rfc3339(raw)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| ApiError::bad_request(format!("Invalid {field} (expected RFC 3339): {e}")))
}

pub(crate) async fn get_usage(
    State(state): State<RuntimeApiState>,
    Query(query): Query<UsageQuery>,
) -> Result<Json<Value>, ApiError> {
    let since = match query.since.as_deref() {
        Some(raw) => Some(parse_iso8601(raw, "since")?),
        None => None,
    };
    let until = match query.until.as_deref() {
        Some(raw) => Some(parse_iso8601(raw, "until")?),
        None => None,
    };
    if let (Some(s), Some(u)) = (since, until)
        && s > u
    {
        return Err(ApiError::bad_request("since must be <= until".to_string()));
    }
    let group_by = match query.group_by.as_deref().unwrap_or("day") {
        "day" => UsageGroupBy::Day,
        "model" => UsageGroupBy::Model,
        "provider" => UsageGroupBy::Provider,
        "thread" => UsageGroupBy::Thread,
        other => {
            return Err(ApiError::bad_request(format!(
                "Unsupported group_by '{other}': expected one of day, model, provider, thread"
            )));
        }
    };

    let aggregation = state
        .runtime_threads
        .aggregate_usage(since, until, group_by)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Json(json!(aggregation)))
}

// ============== Routing Rules ==============

pub(crate) async fn get_routing_rules(
    State(state): State<RuntimeApiState>,
) -> Result<Json<Value>, ApiError> {
    let rules = state.runtime_threads.get_routing_rules().await;
    Ok(Json(json!({ "rules": rules })))
}

pub(crate) async fn set_routing_rules(
    State(state): State<RuntimeApiState>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<Value>, ApiError> {
    let rules: Vec<crate::runtime_threads::RoutingRule> = serde_json::from_value(
        body.get("rules")
            .cloned()
            .unwrap_or(serde_json::Value::Array(vec![])),
    )
    .map_err(|e| ApiError::bad_request(format!("Invalid rules: {e}")))?;
    state
        .runtime_threads
        .set_routing_rules(rules)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    let updated = state.runtime_threads.get_routing_rules().await;
    Ok(Json(json!({ "rules": updated })))
}

// ============== Symbol Index Rebuild ==============

/// Query params for symbol index rebuild.
#[derive(Deserialize)]
pub(crate) struct RebuildSymbolIndexQuery {
    workspace: String,
}

pub(crate) async fn rebuild_symbol_index(
    State(_state): State<RuntimeApiState>,
    Query(q): Query<RebuildSymbolIndexQuery>,
) -> Result<Json<Value>, ApiError> {
    let ws = PathBuf::from(&q.workspace);
    if !ws.is_dir() {
        return Err(ApiError::bad_request(format!(
            "Not a directory: {}",
            q.workspace
        )));
    }
    let ws_for_build = ws.clone();
    let path = deepseek_config::workspace_meta_file_write(&ws, "symbols.json");
    let index = tokio::task::spawn_blocking(move || {
        crate::symbol_index::build_index(
            &ws_for_build,
            crate::symbol_index::SymbolVisibility::Public,
        )
    })
    .await
    .map_err(|e| ApiError::internal(format!("build_index panicked: {e}")))?;
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let json = serde_json::to_string_pretty(&index).unwrap_or_default();
    std::fs::write(&path, &json).map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Json(json!({
        "status": "ok",
        "path": path.to_string_lossy(),
        "symbol_count": index.files.len(),
    })))
}
