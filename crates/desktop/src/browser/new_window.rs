//! `window.open` / `target=_blank` — navigate in-place under the same url_policy (P1-7).

use tauri::webview::{NewWindowFeatures, NewWindowResponse};
use tauri::{AppHandle, Manager};
use url::Url;

use super::{BLANK_URL, BrowserHosts, gate_navigation, navigate_host_url};

/// Handle [`window.open`] by validating the URL and navigating the current Browser host
/// (never spawning a second webview). Returns [`NewWindowResponse::Deny`] always.
pub fn attach_new_window(
    app: AppHandle,
    parent_label: String,
) -> impl Fn(Url, NewWindowFeatures) -> NewWindowResponse<tauri::Wry> + Send + 'static {
    move |url, _features| {
        let url_str = url.to_string();
        if url_str.eq_ignore_ascii_case(BLANK_URL) {
            return NewWindowResponse::Deny;
        }
        if !gate_navigation(&app, &parent_label, &url_str) {
            return NewWindowResponse::Deny;
        }
        let hosts = app.state::<BrowserHosts>();
        if let Err(e) = navigate_host_url(&app, &hosts, &parent_label, url) {
            tracing::warn!(
                target: "zagens_browser",
                parent = %parent_label,
                url = %url_str,
                code = %e.code,
                "window.open in-place navigate failed"
            );
        }
        NewWindowResponse::Deny
    }
}
