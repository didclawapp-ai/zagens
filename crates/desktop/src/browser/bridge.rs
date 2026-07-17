//! Loopback HTTP bridge: sidecar runtime → desktop BrowserHosts.
//!
//! Auth: `Authorization: Bearer <DEEPSEEK_RUNTIME_TOKEN>`.
//! Sidecar env: `ZAGENS_BROWSER_BRIDGE_URL=http://127.0.0.1:<port>`.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::post;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use tokio::sync::oneshot;

use crate::browser::{
    BrowserError, BrowserHosts, PreviewProcess, agent_console_tail, agent_get_text, agent_navigate,
    agent_snapshot, interact, preview,
};
use crate::window_registry::WindowRegistry;

#[derive(Clone)]
struct BridgeState {
    app: AppHandle,
    token: Arc<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BridgeOpRequest {
    op: String,
    thread_id: Option<String>,
    window_label: Option<String>,
    url: Option<String>,
    limit: Option<usize>,
    #[serde(rename = "ref")]
    element_ref: Option<String>,
    text: Option<String>,
    direction: Option<String>,
    amount: Option<f64>,
    workspace: Option<String>,
    include_screenshot: Option<bool>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BridgeOpResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<BrowserError>,
}

fn extract_bearer(headers: &HeaderMap) -> Option<String> {
    let auth = headers
        .get(axum::http::header::AUTHORIZATION)?
        .to_str()
        .ok()?;
    let rest = auth
        .strip_prefix("Bearer ")
        .or_else(|| auth.strip_prefix("bearer "))?;
    Some(rest.trim().to_string())
}

fn resolve_parent_label(
    app: &AppHandle,
    thread_id: Option<&str>,
    window_label: Option<&str>,
) -> Result<String, BrowserError> {
    if let Some(label) = window_label.map(str::trim).filter(|s| !s.is_empty()) {
        return Ok(label.to_string());
    }
    if let Some(tid) = thread_id.map(str::trim).filter(|s| !s.is_empty()) {
        let registry = app.state::<WindowRegistry>();
        if let Some(label) = registry.thread_owner_label(tid) {
            return Ok(label);
        }
        return Err(BrowserError {
            code: "browser_window_ambiguous".into(),
            message: format!("无法根据 thread_id={tid} 定位桌面窗"),
            hint: Some("请在对应会话窗口打开 Browser 视图".into()),
        });
    }
    // Fall back to last focused agent window.
    let registry = app.state::<WindowRegistry>();
    let label = registry.last_focused_label();
    Ok(label)
}

async fn handle_op(
    State(state): State<BridgeState>,
    headers: HeaderMap,
    Json(req): Json<BridgeOpRequest>,
) -> (StatusCode, Json<BridgeOpResponse>) {
    let Some(bearer) = extract_bearer(&headers) else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(BridgeOpResponse {
                ok: false,
                result: None,
                error: Some(BrowserError {
                    code: "unauthorized".into(),
                    message: "missing bearer token".into(),
                    hint: None,
                }),
            }),
        );
    };
    if bearer != state.token.as_str() {
        return (
            StatusCode::UNAUTHORIZED,
            Json(BridgeOpResponse {
                ok: false,
                result: None,
                error: Some(BrowserError {
                    code: "unauthorized".into(),
                    message: "invalid bearer token".into(),
                    hint: None,
                }),
            }),
        );
    }

    let parent = match resolve_parent_label(
        &state.app,
        req.thread_id.as_deref(),
        req.window_label.as_deref(),
    ) {
        Ok(p) => p,
        Err(e) => {
            return (
                StatusCode::OK,
                Json(BridgeOpResponse {
                    ok: false,
                    result: None,
                    error: Some(e),
                }),
            );
        }
    };

    let hosts = state.app.state::<BrowserHosts>();
    let preview_proc = state.app.state::<PreviewProcess>();
    let result = match req.op.as_str() {
        "navigate" => {
            let url = req.url.unwrap_or_default();
            agent_navigate(&state.app, &hosts, &parent, &url)
                .await
                .map(|s| serde_json::to_value(s).unwrap_or(serde_json::Value::Null))
        }
        "snapshot" => {
            let shot = req.include_screenshot.unwrap_or(false);
            agent_snapshot(&state.app, &hosts, &parent, shot)
                .await
                .map(|s| serde_json::to_value(s).unwrap_or(serde_json::Value::Null))
        }
        "get_text" => agent_get_text(&state.app, &hosts, &parent)
            .await
            .map(|s| serde_json::to_value(s).unwrap_or(serde_json::Value::Null)),
        "console_tail" => {
            let limit = req.limit.unwrap_or(50).clamp(1, 200);
            agent_console_tail(&state.app, &hosts, &parent, limit)
                .await
                .map(|s| serde_json::to_value(s).unwrap_or(serde_json::Value::Null))
        }
        "click" => {
            let r = req.element_ref.unwrap_or_default();
            interact::agent_click(&state.app, &hosts, &parent, &r)
                .await
                .map(|s| serde_json::to_value(s).unwrap_or(serde_json::Value::Null))
        }
        "type" => {
            let r = req.element_ref.unwrap_or_default();
            let text = req.text.unwrap_or_default();
            interact::agent_type(&state.app, &hosts, &parent, &r, &text)
                .await
                .map(|s| serde_json::to_value(s).unwrap_or(serde_json::Value::Null))
        }
        "scroll" => {
            let dir = req.direction.unwrap_or_else(|| "down".into());
            interact::agent_scroll(
                &state.app,
                &hosts,
                &parent,
                req.element_ref.as_deref(),
                &dir,
                req.amount,
            )
            .await
            .map(|s| serde_json::to_value(s).unwrap_or(serde_json::Value::Null))
        }
        "start_preview" => preview::agent_start_preview(
            &state.app,
            &hosts,
            &preview_proc,
            &parent,
            req.workspace.as_deref(),
        )
        .await
        .map(|s| serde_json::to_value(s).unwrap_or(serde_json::Value::Null)),
        other => Err(BrowserError {
            code: "unknown_op".into(),
            message: format!("unknown browser bridge op: {other}"),
            hint: None,
        }),
    };

    match result {
        Ok(value) => (
            StatusCode::OK,
            Json(BridgeOpResponse {
                ok: true,
                result: Some(value),
                error: None,
            }),
        ),
        Err(e) => {
            let status = if e.code == "browser_host_missing" {
                StatusCode::SERVICE_UNAVAILABLE
            } else {
                StatusCode::OK
            };
            (
                status,
                Json(BridgeOpResponse {
                    ok: false,
                    result: None,
                    error: Some(e),
                }),
            )
        }
    }
}

/// Bind `127.0.0.1:0`, spawn axum, return `http://127.0.0.1:<port>`.
pub async fn start_browser_bridge(app: AppHandle, token: String) -> Result<String, String> {
    let listener = tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .map_err(|e| format!("browser bridge bind failed: {e}"))?;
    let addr = listener
        .local_addr()
        .map_err(|e| format!("browser bridge local_addr: {e}"))?;
    let url = format!("http://{addr}");

    let state = BridgeState {
        app,
        token: Arc::new(token),
    };
    let router = Router::new()
        .route("/v1/browser/op", post(handle_op))
        .with_state(state);

    let (ready_tx, ready_rx) = oneshot::channel::<()>();
    tauri::async_runtime::spawn(async move {
        let _ = ready_tx.send(());
        if let Err(e) = axum::serve(listener, router).await {
            tracing::error!(target: "zagens_browser", error = %e, "browser bridge server stopped");
        }
    });
    let _ = ready_rx.await;
    tracing::info!(target: "zagens_browser", %url, "browser bridge listening");
    Ok(url)
}

/// Managed by Tauri; read by sidecar spawn.
#[derive(Clone, Default)]
pub struct BrowserBridgeUrl(pub Arc<std::sync::Mutex<Option<String>>>);

impl BrowserBridgeUrl {
    pub fn set(&self, url: String) {
        if let Ok(mut g) = self.0.lock() {
            *g = Some(url);
        }
    }

    pub fn get(&self) -> Option<String> {
        self.0.lock().ok().and_then(|g| g.clone())
    }
}
