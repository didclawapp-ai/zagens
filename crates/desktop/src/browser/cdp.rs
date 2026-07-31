//! WebView2 CDP helper (Windows). Shared by screenshot and CDP interact.

use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Manager};

use super::{BrowserError, BrowserMode};

/// Invoke a CDP method and await the JSON result string (Windows WebView2 only).
#[cfg(windows)]
pub async fn call_devtools_protocol(
    app: &AppHandle,
    mode: BrowserMode,
    host_label: &str,
    method: &str,
    params_json: &str,
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

    let method = method.to_string();
    let params_json = params_json.to_string();

    match mode {
        BrowserMode::Embedded => {
            let wv = app
                .get_webview(host_label)
                .ok_or_else(BrowserError::missing)?;
            wv.with_webview(move |platform| {
                start_cdp_call(&platform, &method, &params_json, finish);
            })
            .map_err(|e| BrowserError::msg("cdp_webview", e.to_string()))?;
        }
        BrowserMode::Windowed => {
            let w = app
                .get_webview_window(host_label)
                .ok_or_else(BrowserError::missing)?;
            w.with_webview(move |platform| {
                start_cdp_call(&platform, &method, &params_json, finish);
            })
            .map_err(|e| BrowserError::msg("cdp_webview", e.to_string()))?;
        }
    }

    tokio::time::timeout(std::time::Duration::from_secs(8), rx)
        .await
        .map_err(|_| BrowserError::msg("cdp_timeout", "CDP 调用超时"))?
        .map_err(|_| BrowserError::msg("cdp_canceled", "CDP 通道关闭"))?
        .map_err(|e| BrowserError::msg("cdp_failed", e))
}

#[cfg(not(windows))]
pub async fn call_devtools_protocol(
    _app: &AppHandle,
    _mode: BrowserMode,
    _host_label: &str,
    _method: &str,
    _params_json: &str,
) -> Result<String, BrowserError> {
    Err(BrowserError::msg(
        "cdp_unsupported",
        "CDP 仅 Windows WebView2 可用",
    ))
}

#[cfg(windows)]
fn start_cdp_call(
    platform: &tauri::webview::PlatformWebview,
    method: &str,
    params_json: &str,
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

    let method_w: Vec<u16> = format!("{method}\0").encode_utf16().collect();
    let params_w: Vec<u16> = params_json
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
            send(&finish_cb, Ok(result_json));
            Ok(())
        },
    ));

    unsafe {
        if let Err(e) = webview.CallDevToolsProtocolMethod(
            PCWSTR(method_w.as_ptr()),
            PCWSTR(params_w.as_ptr()),
            &handler,
        ) {
            send(&finish, Err(format!("CallDevToolsProtocolMethod: {e}")));
        }
    }
}
