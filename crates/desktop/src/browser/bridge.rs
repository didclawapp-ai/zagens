//! Loopback HTTP bridge: sidecar runtime → desktop BrowserHosts.
//!
//! Auth: `Authorization: Bearer <DEEPSEEK_RUNTIME_TOKEN>`.
//! Sidecar env: `ZAGENS_BROWSER_BRIDGE_URL=http://127.0.0.1:<port>`.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use tokio::sync::oneshot;

use crate::browser::{
    BrowserError, BrowserHosts, PreviewProcess, agent_console_tail, agent_get_text, agent_navigate,
    agent_snapshot, agent_wait, interact, preview,
};
use crate::window_registry::WindowRegistry;

/// Short retries while `browser_create` placeholder is visible (§11 A4).
const CREATING_RETRY_ATTEMPTS: u32 = 3;
const CREATING_RETRY_BASE_MS: u64 = 120;

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
    /// Wait kind: `text` | `ref` | `selector` | `load`.
    kind: Option<String>,
    timeout_ms: Option<u64>,
    selector: Option<String>,
    host: Option<String>,
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

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BridgePrefsDto {
    yolo: bool,
    allow_private_lan: bool,
    allowlist: Vec<String>,
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

/// HTTP status for bridge error codes (T3 contract).
fn status_for_bridge_error(code: &str) -> StatusCode {
    if matches!(code, "browser_host_missing" | "browser_creating") {
        StatusCode::SERVICE_UNAVAILABLE
    } else {
        StatusCode::OK
    }
}

fn unauthorized(msg: &str) -> (StatusCode, Json<BridgeOpResponse>) {
    (
        StatusCode::UNAUTHORIZED,
        Json(BridgeOpResponse {
            ok: false,
            result: None,
            error: Some(BrowserError {
                code: "unauthorized".into(),
                message: msg.into(),
                hint: None,
                detail: None,
            }),
        }),
    )
}

#[allow(clippy::result_large_err)] // axum handler Err carries (StatusCode, Json<…>)
fn require_auth(
    state: &BridgeState,
    headers: &HeaderMap,
) -> Result<(), (StatusCode, Json<BridgeOpResponse>)> {
    let Some(bearer) = extract_bearer(headers) else {
        return Err(unauthorized("missing bearer token"));
    };
    if bearer != state.token.as_str() {
        return Err(unauthorized("invalid bearer token"));
    }
    Ok(())
}

fn window_candidates(app: &AppHandle) -> (Vec<String>, Option<String>) {
    let registry = app.state::<WindowRegistry>();
    let labels = registry
        .list_summaries()
        .unwrap_or_default()
        .into_iter()
        .map(|s| s.label)
        .collect::<Vec<_>>();
    let focused = registry.last_focused_label();
    (labels, Some(focused).filter(|s| !s.is_empty()))
}

fn ambiguous_error(message: String, app: &AppHandle) -> BrowserError {
    let (candidates, focused) = window_candidates(app);
    let hint = if candidates.is_empty() {
        Some("请打开桌面会话窗并传入 window_label".into())
    } else {
        Some(format!(
            "候选窗: {}；请传 window_label{}",
            candidates.join(", "),
            focused
                .as_ref()
                .map(|f| format!("（最近焦点: {f}）"))
                .unwrap_or_default()
        ))
    };
    BrowserError {
        code: "browser_window_ambiguous".into(),
        message,
        hint,
        detail: Some(Box::new(serde_json::json!({
            "candidates": candidates,
            "lastFocused": focused,
        }))),
    }
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
        return Err(ambiguous_error(
            format!("无法根据 thread_id={tid} 定位桌面窗"),
            app,
        ));
    }

    let (candidates, focused) = window_candidates(app);
    if candidates.is_empty() {
        let label = focused.unwrap_or_default();
        if label.is_empty() {
            return Err(ambiguous_error("没有可用的桌面会话窗".into(), app));
        }
        return Ok(label);
    }
    if candidates.len() == 1 {
        return Ok(candidates[0].clone());
    }

    // Multiple windows: prefer last-focused when it already has a Browser host;
    // if exactly one window has a host, use that; otherwise ask for window_label.
    let hosts = app.state::<BrowserHosts>();
    let ready = hosts.ready_host_parents();
    let with_host: Vec<String> = candidates
        .iter()
        .filter(|label| ready.iter().any(|r| r == *label))
        .cloned()
        .collect();
    if let Some(f) = focused.as_ref()
        && with_host.iter().any(|l| l == f)
    {
        return Ok(f.clone());
    }
    if with_host.len() == 1 {
        return Ok(with_host[0].clone());
    }
    Err(ambiguous_error(
        "多个桌面窗，无法唯一确定 Browser 宿主".into(),
        app,
    ))
}

async fn handle_prefs(
    State(state): State<BridgeState>,
    headers: HeaderMap,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err((status, Json(err))) = require_auth(&state, &headers) {
        return (
            status,
            Json(serde_json::json!({
                "ok": false,
                "error": err.error,
            })),
        );
    }
    let hosts = state.app.state::<BrowserHosts>();
    match hosts.nav_opts() {
        Ok((allowlist, lan)) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "ok": true,
                "yolo": hosts.browser_yolo(),
                "allowPrivateLan": lan,
                "allowlist": allowlist,
            })),
        ),
        Err(e) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "ok": false,
                "error": e,
            })),
        ),
    }
}

async fn handle_op(
    State(state): State<BridgeState>,
    headers: HeaderMap,
    Json(req): Json<BridgeOpRequest>,
) -> (StatusCode, Json<BridgeOpResponse>) {
    if let Err(resp) = require_auth(&state, &headers) {
        return resp;
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
    let mut result = execute_bridge_op(&state.app, &hosts, &preview_proc, &parent, &req).await;
    for attempt in 1..CREATING_RETRY_ATTEMPTS {
        match &result {
            Err(e) if e.code == "browser_creating" => {
                let delay = Duration::from_millis(CREATING_RETRY_BASE_MS * u64::from(attempt));
                tokio::time::sleep(delay).await;
                result = execute_bridge_op(&state.app, &hosts, &preview_proc, &parent, &req).await;
            }
            _ => break,
        }
    }

    match result {
        Ok(value) => (
            StatusCode::OK,
            Json(BridgeOpResponse {
                ok: true,
                result: Some(value),
                error: None,
            }),
        ),
        Err(e) => (
            status_for_bridge_error(&e.code),
            Json(BridgeOpResponse {
                ok: false,
                result: None,
                error: Some(e),
            }),
        ),
    }
}

async fn execute_bridge_op(
    app: &AppHandle,
    hosts: &BrowserHosts,
    preview_proc: &PreviewProcess,
    parent: &str,
    req: &BridgeOpRequest,
) -> Result<serde_json::Value, BrowserError> {
    match req.op.as_str() {
        "navigate" => {
            let url = req.url.clone().unwrap_or_default();
            agent_navigate(app, hosts, parent, &url)
                .await
                .map(|s| serde_json::to_value(s).unwrap_or(serde_json::Value::Null))
        }
        "snapshot" => {
            let shot = req.include_screenshot.unwrap_or(false);
            agent_snapshot(app, hosts, parent, shot)
                .await
                .map(|s| serde_json::to_value(s).unwrap_or(serde_json::Value::Null))
        }
        "get_text" => agent_get_text(app, hosts, parent)
            .await
            .map(|s| serde_json::to_value(s).unwrap_or(serde_json::Value::Null)),
        "console_tail" => {
            let limit = req.limit.unwrap_or(50).clamp(1, 200);
            agent_console_tail(app, hosts, parent, limit)
                .await
                .map(|s| serde_json::to_value(s).unwrap_or(serde_json::Value::Null))
        }
        "click" => {
            let r = req.element_ref.clone().unwrap_or_default();
            interact::agent_click(app, hosts, parent, &r)
                .await
                .map(|s| serde_json::to_value(s).unwrap_or(serde_json::Value::Null))
        }
        "type" => {
            let r = req.element_ref.clone().unwrap_or_default();
            let text = req.text.clone().unwrap_or_default();
            interact::agent_type(app, hosts, parent, &r, &text)
                .await
                .map(|s| serde_json::to_value(s).unwrap_or(serde_json::Value::Null))
        }
        "scroll" => {
            let dir = req.direction.clone().unwrap_or_else(|| "down".into());
            interact::agent_scroll(
                app,
                hosts,
                parent,
                req.element_ref.as_deref(),
                &dir,
                req.amount,
            )
            .await
            .map(|s| serde_json::to_value(s).unwrap_or(serde_json::Value::Null))
        }
        "start_preview" => {
            preview::agent_start_preview(app, hosts, preview_proc, parent, req.workspace.as_deref())
                .await
                .map(|s| serde_json::to_value(s).unwrap_or(serde_json::Value::Null))
        }
        "wait" => {
            let kind = req.kind.clone().unwrap_or_else(|| "load".into());
            let value = match kind.to_ascii_lowercase().as_str() {
                "ref" => req.element_ref.clone(),
                "selector" => req.selector.clone().or_else(|| req.text.clone()),
                "text" => req.text.clone(),
                _ => None,
            };
            agent_wait(app, hosts, parent, &kind, value.as_deref(), req.timeout_ms)
                .await
                .map(|s| serde_json::to_value(s).unwrap_or(serde_json::Value::Null))
        }
        "allow_host" => {
            let host = req.host.clone().unwrap_or_default();
            let allowed = hosts.allow_host_name(&host)?;
            let (allowlist, lan) = hosts.nav_opts()?;
            Ok(serde_json::to_value(BridgePrefsDto {
                yolo: hosts.browser_yolo(),
                allow_private_lan: lan,
                allowlist: {
                    let mut a = allowlist;
                    if !a.iter().any(|h| h == &allowed) {
                        a.push(allowed);
                    }
                    a
                },
            })
            .unwrap_or(serde_json::Value::Null))
        }
        "prefs" => {
            let (allowlist, lan) = hosts.nav_opts()?;
            Ok(serde_json::to_value(BridgePrefsDto {
                yolo: hosts.browser_yolo(),
                allow_private_lan: lan,
                allowlist,
            })
            .unwrap_or(serde_json::Value::Null))
        }
        other => Err(BrowserError {
            code: "unknown_op".into(),
            message: format!("unknown browser bridge op: {other}"),
            hint: None,
            detail: None,
        }),
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
        .route("/v1/browser/prefs", get(handle_prefs))
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn extract_bearer_accepts_bearer_and_lowercase() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            HeaderValue::from_static("Bearer secret-token"),
        );
        assert_eq!(extract_bearer(&headers).as_deref(), Some("secret-token"));

        headers.insert(
            axum::http::header::AUTHORIZATION,
            HeaderValue::from_static("bearer other"),
        );
        assert_eq!(extract_bearer(&headers).as_deref(), Some("other"));
    }

    #[test]
    fn extract_bearer_rejects_missing_or_bad_scheme() {
        let headers = HeaderMap::new();
        assert!(extract_bearer(&headers).is_none());

        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            HeaderValue::from_static("Basic abc"),
        );
        assert!(extract_bearer(&headers).is_none());
    }

    #[test]
    fn status_for_missing_and_creating_is_503() {
        assert_eq!(
            status_for_bridge_error("browser_host_missing"),
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert_eq!(
            status_for_bridge_error("browser_creating"),
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert_eq!(status_for_bridge_error("unknown_op"), StatusCode::OK);
        assert_eq!(status_for_bridge_error("unauthorized"), StatusCode::OK);
    }

    #[test]
    fn unauthorized_response_shape() {
        let (status, Json(body)) = unauthorized("missing bearer token");
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert!(!body.ok);
        assert_eq!(body.error.as_ref().unwrap().code, "unauthorized");
        assert!(body.result.is_none());
    }

    #[test]
    fn unknown_op_error_code_stable() {
        let err = BrowserError {
            code: "unknown_op".into(),
            message: "unknown browser bridge op: nope".into(),
            hint: None,
            detail: None,
        };
        assert_eq!(status_for_bridge_error(&err.code), StatusCode::OK);
        assert!(err.message.contains("nope"));
    }

    #[test]
    fn bridge_op_request_deserializes_camel_case() {
        let raw = r#"{
          "op": "wait",
          "threadId": "t1",
          "windowLabel": "main",
          "kind": "text",
          "timeoutMs": 1000,
          "ref": "button:go:0",
          "includeScreenshot": true
        }"#;
        let req: BridgeOpRequest = serde_json::from_str(raw).unwrap();
        assert_eq!(req.op, "wait");
        assert_eq!(req.thread_id.as_deref(), Some("t1"));
        assert_eq!(req.window_label.as_deref(), Some("main"));
        assert_eq!(req.kind.as_deref(), Some("text"));
        assert_eq!(req.timeout_ms, Some(1000));
        assert_eq!(req.element_ref.as_deref(), Some("button:go:0"));
        assert_eq!(req.include_screenshot, Some(true));
    }
}
