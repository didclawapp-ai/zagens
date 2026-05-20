//! Local runtime HTTP proxy (H06) — Bearer token stays in the Rust shell, not the WebView.

use std::time::Duration;

use futures_util::StreamExt;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use crate::commands::AppContext;

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

fn validate_runtime_path(path: &str) -> Result<(), String> {
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
    app: AppHandle,
    body: String,
    ctx: tauri::State<'_, AppContext>,
) -> Result<(), String> {
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
        let _ = app.emit("runtime://stream-error", format!("HTTP {status}: {text}"));
        return Err(format!("HTTP {status}: {text}"));
    }

    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(bytes) => {
                let payload = String::from_utf8_lossy(&bytes).into_owned();
                app.emit("runtime://stream-chunk", payload)
                    .map_err(|e| e.to_string())?;
            }
            Err(e) => {
                let msg = format!("读取流失败: {e}");
                let _ = app.emit("runtime://stream-error", msg.clone());
                return Err(msg);
            }
        }
    }

    app.emit("runtime://stream-done", ())
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn runtime_get_sse(
    app: AppHandle,
    path: String,
    ctx: tauri::State<'_, AppContext>,
) -> Result<(), String> {
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
        let _ = app.emit("runtime://events-error", format!("HTTP {status}: {text}"));
        return Err(format!("HTTP {status}: {text}"));
    }

    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(bytes) => {
                let payload = String::from_utf8_lossy(&bytes).into_owned();
                app.emit("runtime://events-chunk", payload)
                    .map_err(|e| e.to_string())?;
            }
            Err(e) => {
                let msg = format!("读取 SSE 失败: {e}");
                let _ = app.emit("runtime://events-error", msg.clone());
                return Err(msg);
            }
        }
    }

    app.emit("runtime://events-done", ())
        .map_err(|e| e.to_string())?;
    Ok(())
}
