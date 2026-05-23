//! Local runtime HTTP proxy (H06) — Bearer token stays in the Rust shell, not the WebView.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use futures_util::StreamExt;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use crate::commands::AppContext;

/// Per-webview cancel flag for in-flight `runtime_get_sse` (abort does not stop reqwest otherwise).
static SSE_CANCEL_FLAGS: LazyLock<Mutex<HashMap<String, Arc<AtomicBool>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn arm_sse_cancel(window_label: &str) -> Arc<AtomicBool> {
    let flag = Arc::new(AtomicBool::new(false));
    let mut guard = SSE_CANCEL_FLAGS.lock().expect("sse cancel map");
    if let Some(prev) = guard.insert(window_label.to_string(), Arc::clone(&flag)) {
        prev.store(true, Ordering::Relaxed);
    }
    flag
}

fn disarm_sse_cancel(window_label: &str) {
    let mut guard = SSE_CANCEL_FLAGS.lock().expect("sse cancel map");
    guard.remove(window_label);
}

#[tauri::command]
pub async fn runtime_cancel_sse(window: tauri::WebviewWindow) -> Result<(), String> {
    let label = window.label().to_string();
    let guard = SSE_CANCEL_FLAGS.lock().map_err(|e| e.to_string())?;
    if let Some(flag) = guard.get(&label) {
        flag.store(true, Ordering::Relaxed);
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct RuntimeHttpRequest {
    pub method: String,
    pub path: String,
    pub body: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RuntimeHttpResponse {
    pub status: u16,
    pub body: String,
}

pub(crate) fn validate_runtime_path(path: &str) -> Result<(), String> {
    let p = path.trim();
    if p.is_empty() || !p.starts_with('/') {
        return Err("path 必须以 / 开头".to_string());
    }
    if p.contains("..") {
        return Err("path 不能包含 ..".to_string());
    }
    if !(p.starts_with("/v1/") || p == "/health") {
        return Err("仅允许 /health 与 /v1/* 路径".to_string());
    }
    Ok(())
}

#[tauri::command]
pub async fn runtime_http(
    request: RuntimeHttpRequest,
    ctx: tauri::State<'_, AppContext>,
) -> Result<RuntimeHttpResponse, String> {
    validate_runtime_path(&request.path)?;
    let method = request.method.trim().to_uppercase();
    let url = format!("http://127.0.0.1:{}{}", ctx.runtime_port, request.path.trim());

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| format!("HTTP 客户端: {e}"))?;

    let mut rb = match method.as_str() {
        "GET" => client.get(&url),
        "POST" => client.post(&url),
        "PATCH" => client.patch(&url),
        "PUT" => client.put(&url),
        "DELETE" => client.delete(&url),
        other => return Err(format!("不支持的 HTTP 方法: {other}")),
    };

    rb = rb.header(AUTHORIZATION, format!("Bearer {}", ctx.runtime_token));
    if let Some(body) = request.body {
        rb = rb
            .header(CONTENT_TYPE, "application/json")
            .body(body);
    }

    let resp = rb
        .send()
        .await
        .map_err(|e| format!("运行时请求失败: {e}"))?;

    let status = resp.status().as_u16();
    let body = resp
        .text()
        .await
        .map_err(|e| format!("读取响应失败: {e}"))?;

    Ok(RuntimeHttpResponse { status, body })
}

#[tauri::command]
pub async fn runtime_post_stream(
    window: tauri::WebviewWindow,
    app: AppHandle,
    body: String,
    ctx: tauri::State<'_, AppContext>,
) -> Result<(), String> {
    let window_label = window.label().to_string();
    let url = format!("http://127.0.0.1:{}/v1/stream", ctx.runtime_port);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3600))
        .build()
        .map_err(|e| format!("HTTP 客户端: {e}"))?;

    let resp = client
        .post(&url)
        .header(AUTHORIZATION, format!("Bearer {}", ctx.runtime_token))
        .header(CONTENT_TYPE, "application/json")
        .body(body)
        .send()
        .await
        .map_err(|e| format!("流式请求失败: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let text = resp.text().await.unwrap_or_default();
        let _ = app.emit_to(
            &window_label,
            "runtime://stream-error",
            format!("HTTP {status}: {text}"),
        );
        return Err(format!("HTTP {status}: {text}"));
    }

    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(bytes) => {
                let payload = String::from_utf8_lossy(&bytes).into_owned();
                app.emit_to(&window_label, "runtime://stream-chunk", payload)
                    .map_err(|e| e.to_string())?;
            }
            Err(e) => {
                let msg = format!("读取流失败: {e}");
                let _ = app.emit_to(&window_label, "runtime://stream-error", msg.clone());
                return Err(msg);
            }
        }
    }

    app.emit_to(&window_label, "runtime://stream-done", ())
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn runtime_get_sse(
    window: tauri::WebviewWindow,
    app: AppHandle,
    path: String,
    ctx: tauri::State<'_, AppContext>,
) -> Result<(), String> {
    let window_label = window.label().to_string();
    validate_runtime_path(&path)?;
    let url = format!("http://127.0.0.1:{}{}", ctx.runtime_port, path.trim());
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3600))
        .build()
        .map_err(|e| format!("HTTP 客户端: {e}"))?;

    let resp = client
        .get(&url)
        .header(AUTHORIZATION, format!("Bearer {}", ctx.runtime_token))
        .send()
        .await
        .map_err(|e| format!("SSE 请求失败: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let text = resp.text().await.unwrap_or_default();
        let _ = app.emit_to(
            &window_label,
            "runtime://events-error",
            format!("HTTP {status}: {text}"),
        );
        return Err(format!("HTTP {status}: {text}"));
    }

    let cancel = arm_sse_cancel(&window_label);
    let mut stream = resp.bytes_stream();
    loop {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let Some(chunk) = stream.next().await else {
            break;
        };
        match chunk {
            Ok(bytes) => {
                let payload = String::from_utf8_lossy(&bytes).into_owned();
                app.emit_to(&window_label, "runtime://events-chunk", payload)
                    .map_err(|e| e.to_string())?;
            }
            Err(e) => {
                let msg = format!("读取 SSE 失败: {e}");
                let _ = app.emit_to(&window_label, "runtime://events-error", msg.clone());
                disarm_sse_cancel(&window_label);
                return Err(msg);
            }
        }
    }

    disarm_sse_cancel(&window_label);
    if !cancel.load(Ordering::Relaxed) {
        app.emit_to(&window_label, "runtime://events-done", ())
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_runtime_path;

    #[test]
    fn allows_health_and_v1_prefix_paths() {
        for path in [
            "/health",
            "/v1/sessions",
            "/v1/threads/thr_1",
            "/v1/threads/thr_1/turns",
            "/v1/stream",
            "/v1/usage",
            "/v1/apps/mcp/servers",
            "/v1/symbol-index/rebuild",
        ] {
            validate_runtime_path(path)
                .unwrap_or_else(|e| panic!("expected allow {path:?}, got {e}"));
        }
    }

    #[test]
    fn rejects_non_v1_and_traversal_paths() {
        let cases = [
            ("", "empty"),
            ("v1/sessions", "missing leading slash"),
            ("/api/v1/sessions", "wrong prefix"),
            ("/v2/sessions", "v2"),
            ("/admin", "admin"),
            ("/v1/../etc/passwd", "dot-dot"),
            ("/v1/foo/../../../bar", "embedded dot-dot"),
        ];
        for (path, label) in cases {
            assert!(
                validate_runtime_path(path).is_err(),
                "expected reject ({label}): {path:?}"
            );
        }
    }

    #[test]
    fn trims_whitespace_before_validation() {
        validate_runtime_path("  /health  ").expect("trimmed /health");
        assert!(validate_runtime_path("  /evil  ").is_err());
    }
}
