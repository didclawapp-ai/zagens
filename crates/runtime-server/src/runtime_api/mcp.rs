//! MCP server registry HTTP handlers (R-003 A4.5).

use std::collections::HashSet;
use std::fs;

use axum::body::Bytes;
use axum::extract::{Path as AxumPath, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::mcp::{
    manager_snapshot_from_pool, recent_mcp_calls, McpConfig, McpManagerSnapshot,
    McpReloadReport, McpServerConfig,
};

use super::{ApiError, RuntimeApiState};

#[derive(Debug, Serialize)]
pub(crate) struct McpServerEntry {
    name: String,
    enabled: bool,
    required: bool,
    command: Option<String>,
    url: Option<String>,
    args: Vec<String>,
    /// Resolved transport label (`stdio` | `sse` | `http`), or `null` when the
    /// server config is too incomplete to resolve one.
    transport: Option<String>,
    connected: bool,
    enabled_tools: Vec<String>,
    disabled_tools: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct McpServersResponse {
    servers: Vec<McpServerEntry>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct McpToolsQuery {
    server: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct McpToolEntry {
    server: String,
    name: String,
    prefixed_name: String,
    description: Option<String>,
    input_schema: Value,
}

#[derive(Debug, Serialize)]
pub(crate) struct McpToolsResponse {
    tools: Vec<McpToolEntry>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct McpAddServerRequest {
    name: String,
    command: Option<String>,
    url: Option<String>,
    #[serde(default)]
    args: Vec<String>,
}

pub(crate) async fn list_mcp_servers(
    State(state): State<RuntimeApiState>,
) -> Result<Json<McpServersResponse>, ApiError> {
    let config = load_mcp_config_or_default(&state.mcp_config_path)?;
    // Report live connection state from the sidecar shared pool (same pool agents use).
    // Do not spawn a throwaway pool here — a 2s connect race falsely showed "disconnected"
    // while tool calls against the shared pool were succeeding.
    let pool = state.shared_mcp_pool.lock().await;
    let connected: HashSet<String> = pool
        .connected_servers()
        .into_iter()
        .map(str::to_string)
        .collect();

    let mut servers = Vec::new();
    for (name, server_cfg) in config.servers {
        servers.push(McpServerEntry {
            name: name.clone(),
            enabled: server_cfg.is_enabled(),
            required: server_cfg.required,
            command: server_cfg.command.clone(),
            url: server_cfg.url.clone(),
            args: server_cfg.args.clone(),
            transport: server_cfg
                .transport_kind()
                .ok()
                .map(|kind| kind.as_str().to_string()),
            connected: connected.contains(&name),
            enabled_tools: server_cfg.enabled_tools.clone(),
            disabled_tools: server_cfg.disabled_tools.clone(),
        });
    }
    servers.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(Json(McpServersResponse { servers }))
}

#[derive(Debug, serde::Serialize)]
pub(crate) struct McpDiscoverResponse {
    snapshot: McpManagerSnapshot,
    recent_calls: Vec<crate::mcp::McpCallRecord>,
}

pub(crate) async fn discover_mcp(
    State(state): State<RuntimeApiState>,
) -> Result<Json<McpDiscoverResponse>, ApiError> {
    let mut pool = state.shared_mcp_pool.lock().await;
    let snapshot = tokio::time::timeout(
        std::time::Duration::from_secs(15),
        manager_snapshot_from_pool(&state.mcp_config_path, &mut pool),
    )
    .await
    .map_err(|_| ApiError::internal("MCP discover timed out after 15s".to_string()))?;
    Ok(Json(McpDiscoverResponse {
        snapshot,
        recent_calls: recent_mcp_calls(),
    }))
}

pub(crate) async fn list_mcp_calls(
    State(_state): State<RuntimeApiState>,
) -> Result<Json<Vec<crate::mcp::McpCallRecord>>, ApiError> {
    Ok(Json(recent_mcp_calls()))
}

pub(crate) async fn reload_mcp_config(
    State(state): State<RuntimeApiState>,
) -> Result<Json<McpReloadReport>, ApiError> {
    let mut pool = state.shared_mcp_pool.lock().await;
    pool.reload_from_path(&state.mcp_config_path)
        .await
        .map_err(|e| ApiError::internal(format!("MCP reload failed: {e}")))
        .map(Json)
}

pub(crate) async fn merge_mcp_config_json(
    State(state): State<RuntimeApiState>,
    body: Bytes,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let s = std::str::from_utf8(&body).map_err(|_| ApiError::bad_request("请求体须为 UTF-8"))?;
    let merged = crate::mcp::merge_mcp_json_fragment(&state.mcp_config_path, s)
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    Ok((
        StatusCode::OK,
        Json(json!({
            "merged_servers": merged,
        })),
    ))
}

pub(crate) async fn add_mcp_server(
    State(state): State<RuntimeApiState>,
    Json(req): Json<McpAddServerRequest>,
) -> Result<StatusCode, ApiError> {
    crate::mcp::add_server_config(
        &state.mcp_config_path,
        req.name,
        req.command,
        req.url,
        req.args,
    )
    .map_err(|e| ApiError::bad_request(format!("添加 MCP 服务器失败：{e}")))?;
    Ok(StatusCode::CREATED)
}

pub(crate) async fn get_mcp_server(
    State(state): State<RuntimeApiState>,
    AxumPath(name): AxumPath<String>,
) -> Result<Json<McpServerConfig>, ApiError> {
    let entry = crate::mcp::get_server_entry(&state.mcp_config_path, &name)
        .map_err(|e| ApiError::internal(e.to_string()))?;
    let Some(cfg) = entry else {
        return Err(ApiError::not_found(format!(
            "MCP server '{name}' not found"
        )));
    };
    Ok(Json(cfg.redacted_for_display()))
}

pub(crate) async fn update_mcp_server(
    State(state): State<RuntimeApiState>,
    AxumPath(name): AxumPath<String>,
    Json(cfg): Json<McpServerConfig>,
) -> Result<Json<Value>, ApiError> {
    crate::mcp::replace_server_in_config(&state.mcp_config_path, &name, cfg)
        .map_err(|e| ApiError::bad_request(format!("更新 MCP 服务器失败：{e}")))?;
    Ok(Json(json!({ "ok": true })))
}

pub(crate) async fn delete_mcp_server(
    State(state): State<RuntimeApiState>,
    AxumPath(name): AxumPath<String>,
) -> Result<StatusCode, ApiError> {
    crate::mcp::remove_server_from_config(&state.mcp_config_path, &name)
        .map_err(|e| ApiError::bad_request(format!("删除 MCP 服务器失败：{e}")))?;
    let mut pool = state.shared_mcp_pool.lock().await;
    pool.reload_from_path(&state.mcp_config_path)
        .await
        .map_err(|e| ApiError::internal(format!("MCP reload after delete failed: {e}")))?;
    Ok(StatusCode::NO_CONTENT)
}

pub(crate) async fn list_mcp_tools(
    State(state): State<RuntimeApiState>,
    Query(query): Query<McpToolsQuery>,
) -> Result<Json<McpToolsResponse>, ApiError> {
    let pool = state.shared_mcp_pool.lock().await;

    let mut tools = Vec::new();
    for (prefixed_name, tool) in pool.all_tools() {
        // The bare tool name is known (`tool.name`); derive the server by
        // stripping the `mcp_` prefix and the `_{tool.name}` suffix. This is
        // robust to server names that themselves contain underscores, unlike
        // a naive `split_once('_')`.
        let server = prefixed_name
            .strip_prefix("mcp_")
            .and_then(|rest| rest.strip_suffix(&tool.name))
            .map(|s| s.trim_end_matches('_'));
        let Some(server) = server else {
            continue;
        };

        if let Some(filter) = query.server.as_deref()
            && server != filter
        {
            continue;
        }

        tools.push(McpToolEntry {
            server: server.to_string(),
            name: tool.name.clone(),
            prefixed_name,
            description: tool.description.clone(),
            input_schema: tool.input_schema.clone(),
        });
    }

    tools.sort_by(|a, b| a.server.cmp(&b.server).then_with(|| a.name.cmp(&b.name)));

    Ok(Json(McpToolsResponse { tools }))
}

fn load_mcp_config_or_default(path: &std::path::Path) -> Result<McpConfig, ApiError> {
    if !path.exists() {
        return Ok(McpConfig::default());
    }
    let raw = fs::read_to_string(path).map_err(|e| {
        ApiError::internal(format!("Failed to read MCP config {}: {e}", path.display()))
    })?;
    serde_json::from_str::<McpConfig>(&raw).map_err(|e| {
        ApiError::internal(format!(
            "Failed to parse MCP config {}: {e}",
            path.display()
        ))
    })
}
