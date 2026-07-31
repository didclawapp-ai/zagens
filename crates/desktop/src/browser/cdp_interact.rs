//! CDP-backed click / type (Windows WebView2). Falls back to JS inject on other platforms.

use serde::Deserialize;
use tauri::AppHandle;

use super::cdp::call_devtools_protocol;
use super::scripts::{click_point_js, focus_ref_js, normalize_eval_json};
use super::{BrowserError, BrowserHosts, BrowserMode, eval_js_string, lookup_host};

#[derive(Debug, Clone)]
pub struct CdpInteractOk {
    pub r#ref: String,
    pub role: Option<String>,
    pub name: Option<String>,
    pub detail: serde_json::Value,
}

#[cfg(windows)]
pub fn is_available() -> bool {
    true
}

#[cfg(not(windows))]
pub fn is_available() -> bool {
    false
}

#[derive(Deserialize)]
struct PointRaw {
    ok: Option<bool>,
    r#ref: Option<String>,
    role: Option<String>,
    name: Option<String>,
    error: Option<String>,
    x: Option<f64>,
    y: Option<f64>,
}

fn parse_point(raw: &str) -> Result<PointRaw, BrowserError> {
    let normalized = normalize_eval_json(raw);
    serde_json::from_str(&normalized).map_err(|e| {
        BrowserError::msg(
            "cdp_point_parse",
            format!("无法解析 interact 坐标 JSON: {e}; raw={normalized}"),
        )
    })
}

async fn ref_center(
    app: &AppHandle,
    mode: BrowserMode,
    host_label: &str,
    ref_id: &str,
) -> Result<PointRaw, BrowserError> {
    let raw = eval_js_string(app, mode, host_label, &click_point_js(ref_id)).await?;
    let pt = parse_point(&raw)?;
    if !pt.ok.unwrap_or(false) {
        let code = pt.error.as_deref().unwrap_or("ref_not_found");
        return Err(BrowserError::msg(code, format!("找不到 ref {ref_id}")));
    }
    let (Some(x), Some(y)) = (pt.x, pt.y) else {
        return Err(BrowserError::msg(
            "cdp_point_missing",
            "元素无有效 bounding box",
        ));
    };
    if !x.is_finite() || !y.is_finite() {
        return Err(BrowserError::msg("cdp_point_invalid", "坐标无效"));
    }
    Ok(pt)
}

#[cfg(windows)]
async fn dispatch_mouse_click(
    app: &AppHandle,
    mode: BrowserMode,
    host_label: &str,
    x: f64,
    y: f64,
) -> Result<(), BrowserError> {
    let xi = x.round() as i64;
    let yi = y.round() as i64;
    for (event_type, button) in [
        ("mouseMoved", "none"),
        ("mousePressed", "left"),
        ("mouseReleased", "left"),
    ] {
        let params = format!(
            r#"{{"type":"{event_type}","x":{xi},"y":{yi},"button":"{button}","clickCount":1}}"#
        );
        call_devtools_protocol(app, mode, host_label, "Input.dispatchMouseEvent", &params).await?;
    }
    Ok(())
}

#[cfg(windows)]
async fn dispatch_insert_text(
    app: &AppHandle,
    mode: BrowserMode,
    host_label: &str,
    text: &str,
) -> Result<(), BrowserError> {
    let params = serde_json::json!({ "text": text }).to_string();
    call_devtools_protocol(app, mode, host_label, "Input.insertText", &params).await?;
    Ok(())
}

pub async fn cdp_click(
    app: &AppHandle,
    hosts: &BrowserHosts,
    parent_label: &str,
    ref_id: &str,
) -> Result<CdpInteractOk, BrowserError> {
    #[cfg(not(windows))]
    {
        let _ = (app, hosts, parent_label, ref_id);
        return Err(BrowserError::msg("cdp_unsupported", "CDP interact 不可用"));
    }

    #[cfg(windows)]
    {
        let (mode, host_label) = lookup_host(hosts, parent_label)?;
        let pt = ref_center(app, mode, &host_label, ref_id).await?;
        dispatch_mouse_click(app, mode, &host_label, pt.x.unwrap(), pt.y.unwrap()).await?;
        Ok(CdpInteractOk {
            r#ref: pt.r#ref.unwrap_or_else(|| ref_id.to_string()),
            role: pt.role,
            name: pt.name,
            detail: serde_json::json!({ "via": "cdp" }),
        })
    }
}

pub async fn cdp_type(
    app: &AppHandle,
    hosts: &BrowserHosts,
    parent_label: &str,
    ref_id: &str,
    text: &str,
) -> Result<CdpInteractOk, BrowserError> {
    #[cfg(not(windows))]
    {
        let _ = (app, hosts, parent_label, ref_id, text);
        return Err(BrowserError::msg("cdp_unsupported", "CDP interact 不可用"));
    }

    #[cfg(windows)]
    {
        let (mode, host_label) = lookup_host(hosts, parent_label)?;
        let focus_raw = eval_js_string(app, mode, &host_label, &focus_ref_js(ref_id)).await?;
        let pt = parse_point(&focus_raw)?;
        if !pt.ok.unwrap_or(false) {
            let code = pt.error.as_deref().unwrap_or("ref_not_found");
            return Err(BrowserError::msg(code, format!("找不到 ref {ref_id}")));
        }
        dispatch_insert_text(app, mode, &host_label, text).await?;
        Ok(CdpInteractOk {
            r#ref: pt.r#ref.unwrap_or_else(|| ref_id.to_string()),
            role: pt.role,
            name: pt.name,
            detail: serde_json::json!({ "via": "cdp", "typedLen": text.chars().count() }),
        })
    }
}
