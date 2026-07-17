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
        // Try quality 45 then 28 if payload exceeds the agent-facing budget (C6).
        for quality in [45_u8, 28_u8] {
            let data_url = capture_once(app, mode, host_label, quality).await?;
            if data_url.len() <= MAX_DATA_URL_CHARS {
                return Ok(data_url);
            }
            if quality == 28 {
                return Err(BrowserError::msg(
                    "screenshot_too_large",
                    format!(
                        "截图 data-URL 仍超过 {} 字符（{}）；请关闭 include_screenshot 或缩小视口",
                        MAX_DATA_URL_CHARS,
                        data_url.len()
                    ),
                ));
            }
        }
        Err(BrowserError::msg("screenshot_failed", "截图失败"))
    }

    #[cfg(not(windows))]
    {
        let _ = (app, mode, host_label);
        Err(BrowserError::msg(
            "screenshot_unsupported",
            "本平台尚未接入 Browser 截图（仅 Windows WebView2 CDP）；请用 browser_snapshot 文本/a11y",
        ))
    }
}

#[cfg(windows)]
async fn capture_once(
    app: &AppHandle,
    mode: BrowserMode,
    host_label: &str,
    quality: u8,
) -> Result<String, BrowserError> {
    let (tx, rx) = tokio::sync::oneshot::channel::<Result<String, String>>();
    let tx = Mutex::new(Some(tx));

    let finish = move |result: Result<String, String>| {
        if let Ok(mut g) = tx.lock()
            && let Some(sender) = g.take()
        {
            let _ = sender.send(result);
        }
    };

    match mode {
        BrowserMode::Embedded => {
            let wv = app
                .get_webview(host_label)
                .ok_or_else(BrowserError::missing)?;
            wv.with_webview(move |platform| {
                start_cdp_screenshot(&platform, quality, finish);
            })
            .map_err(|e| BrowserError::msg("screenshot_webview", e.to_string()))?;
        }
        BrowserMode::Windowed => {
            let w = app
                .get_webview_window(host_label)
                .ok_or_else(BrowserError::missing)?;
            w.with_webview(move |platform| {
                start_cdp_screenshot(&platform, quality, finish);
            })
            .map_err(|e| BrowserError::msg("screenshot_webview", e.to_string()))?;
        }
    }

    tokio::time::timeout(std::time::Duration::from_secs(8), rx)
        .await
        .map_err(|_| BrowserError::msg("screenshot_timeout", "截图超时"))?
        .map_err(|_| BrowserError::msg("screenshot_canceled", "截图通道关闭"))?
        .map_err(|e| BrowserError::msg("screenshot_failed", e))
}

#[cfg(windows)]
fn start_cdp_screenshot(
    platform: &tauri::webview::PlatformWebview,
    quality: u8,
    finish: impl FnOnce(Result<String, String>) + Send + 'static,
) {
    use webview2_com::CallDevToolsProtocolMethodCompletedHandler;
    use windows::core::PCWSTR;

    #[allow(clippy::type_complexity)]
    let finish: Arc<Mutex<Option<Box<dyn FnOnce(Result<String, String>) + Send>>>> =
        Arc::new(Mutex::new(Some(Box::new(finish))));

    #[allow(clippy::type_complexity)]
    let send = |finish: &Arc<Mutex<Option<Box<dyn FnOnce(Result<String, String>) + Send>>>>,
                r: Result<String, String>| {
        if let Ok(mut g) = finish.lock()
            && let Some(f) = g.take()
        {
            f(r);
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
    let q = quality.clamp(10, 90);
    let params_json = format!(r#"{{"format":"jpeg","quality":{q},"fromSurface":true}}"#);
    let params: Vec<u16> = params_json
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
