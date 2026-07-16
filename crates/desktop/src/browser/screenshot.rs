//! Capture visible Browser host content as a compact JPEG data-URL (opt-in).
//!
//! Windows: WebView2 CDP `Page.captureScreenshot`. Must not block inside
//! `with_webview` (UI thread) — the CDP completion handler posts back to that
//! thread, so we only *start* the call there and await outside.

use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Manager};

use super::{BrowserError, BrowserMode};

/// Max data-URL chars returned to agent (~150KB decoded JPEG).
const MAX_DATA_URL_CHARS: usize = 200_000;

pub async fn capture_screenshot_data_url(
    app: &AppHandle,
    mode: BrowserMode,
    host_label: &str,
) -> Result<String, BrowserError> {
    #[cfg(windows)]
    {
        let (tx, rx) = tokio::sync::oneshot::channel::<Result<String, String>>();
        let tx = Mutex::new(Some(tx));

        let finish = move |result: Result<String, String>| {
            if let Ok(mut g) = tx.lock() {
                if let Some(sender) = g.take() {
                    let _ = sender.send(result);
                }
            }
        };

        match mode {
            BrowserMode::Embedded => {
                let wv = app
                    .get_webview(host_label)
                    .ok_or_else(BrowserError::missing)?;
                wv.with_webview(move |platform| {
                    start_cdp_screenshot(&platform, finish);
                })
                .map_err(|e| BrowserError::msg("screenshot_webview", e.to_string()))?;
            }
            BrowserMode::Windowed => {
                let w = app
                    .get_webview_window(host_label)
                    .ok_or_else(BrowserError::missing)?;
                w.with_webview(move |platform| {
                    start_cdp_screenshot(&platform, finish);
                })
                .map_err(|e| BrowserError::msg("screenshot_webview", e.to_string()))?;
            }
        }

        let raw = tokio::time::timeout(std::time::Duration::from_secs(8), rx)
            .await
            .map_err(|_| BrowserError::msg("screenshot_timeout", "截图超时"))?
            .map_err(|_| BrowserError::msg("screenshot_canceled", "截图通道关闭"))?
            .map_err(|e| BrowserError::msg("screenshot_failed", e))?;

        Ok(truncate_data_url(raw))
    }

    #[cfg(not(windows))]
    {
        let _ = (app, mode, host_label);
        Err(BrowserError::msg(
            "screenshot_unsupported",
            "本平台尚未接入 Browser 截图（Windows WebView2 CDP 可用）",
        ))
    }
}

fn truncate_data_url(s: String) -> String {
    if s.len() <= MAX_DATA_URL_CHARS {
        return s;
    }
    format!(
        "{}…[truncated {} chars]",
        &s[..MAX_DATA_URL_CHARS.min(s.len())],
        s.len().saturating_sub(MAX_DATA_URL_CHARS)
    )
}

#[cfg(windows)]
fn start_cdp_screenshot(
    platform: &tauri::webview::PlatformWebview,
    finish: impl FnOnce(Result<String, String>) + Send + 'static,
) {
    use webview2_com::CallDevToolsProtocolMethodCompletedHandler;
    use windows::core::PCWSTR;

    let finish: Arc<Mutex<Option<Box<dyn FnOnce(Result<String, String>) + Send>>>> =
        Arc::new(Mutex::new(Some(Box::new(finish))));

    let send = |finish: &Arc<Mutex<Option<Box<dyn FnOnce(Result<String, String>) + Send>>>>,
                r: Result<String, String>| {
        if let Ok(mut g) = finish.lock() {
            if let Some(f) = g.take() {
                f(r);
            }
        }
    };

    let controller = platform.controller();
    let webview = match unsafe { controller.CoreWebView2() } {
        Ok(w) => w,
        Err(e) => {
            send(&finish, Err(format!("CoreWebView2: {e}")));
            return;
        }
    };

    let method: Vec<u16> = "Page.captureScreenshot\0".encode_utf16().collect();
    let params: Vec<u16> = r#"{"format":"jpeg","quality":45,"fromSurface":true}"#
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    let finish_cb = Arc::clone(&finish);
    let handler = CallDevToolsProtocolMethodCompletedHandler::create(Box::new(
        move |error_code, result_json| {
            if error_code.is_err() {
                send(&finish_cb, Err(format!("CDP error: {error_code:?}")));
                return Ok(());
            }
            // Callback macro converts PCWSTR → String.
            let json: String = result_json;
            match parse_cdp_screenshot_json(&json) {
                Ok(data_url) => send(&finish_cb, Ok(data_url)),
                Err(e) => send(&finish_cb, Err(e)),
            }
            Ok(())
        },
    ));

    unsafe {
        if let Err(e) = webview.CallDevToolsProtocolMethod(
            PCWSTR(method.as_ptr()),
            PCWSTR(params.as_ptr()),
            &handler,
        ) {
            send(&finish, Err(format!("CallDevToolsProtocolMethod: {e}")));
        }
    }
}

#[cfg(windows)]
fn parse_cdp_screenshot_json(json: &str) -> Result<String, String> {
    use serde_json::Value;
    let parsed: Value =
        serde_json::from_str(json).map_err(|e| format!("CDP JSON parse: {e}; raw={json}"))?;
    let b64 = parsed
        .get("data")
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("CDP missing data field: {json}"))?;
    Ok(format!("data:image/jpeg;base64,{b64}"))
}
