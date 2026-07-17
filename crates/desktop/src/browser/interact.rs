//! P2 write interactions: click / type / scroll by snapshot element ref.

use serde::Deserialize;
use serde_json::json;
use tauri::{AppHandle, State};

use super::scripts::{click_js, scroll_js, type_js};
use super::{BrowserError, BrowserHosts, BrowserMode, NavActor, eval_js_string, lookup_host};

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserInteractDto {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<serde_json::Value>,
}

fn parse_interact(raw: &str) -> BrowserInteractDto {
    #[derive(Deserialize)]
    struct Raw {
        ok: Option<bool>,
        r#ref: Option<String>,
        role: Option<String>,
        name: Option<String>,
        error: Option<String>,
        #[serde(flatten)]
        rest: serde_json::Value,
    }
    let normalized = super::scripts::normalize_eval_json(raw);
    let parsed: Raw = serde_json::from_str(&normalized).unwrap_or(Raw {
        ok: Some(false),
        r#ref: None,
        role: None,
        name: None,
        error: Some(normalized),
        rest: serde_json::Value::Null,
    });
    BrowserInteractDto {
        ok: parsed.ok.unwrap_or(false),
        r#ref: parsed.r#ref,
        role: parsed.role,
        name: parsed.name,
        error: parsed.error,
        detail: if parsed.rest.is_null() {
            None
        } else {
            Some(parsed.rest)
        },
    }
}

fn map_interact_result(dto: BrowserInteractDto) -> Result<BrowserInteractDto, BrowserError> {
    if dto.ok {
        return Ok(dto);
    }
    let code = dto.error.as_deref().unwrap_or("interact_failed");
    let message = match code {
        "ref_not_found" => format!("找不到 element ref {}", dto.r#ref.as_deref().unwrap_or("?")),
        "not_editable" => "目标元素不可输入".into(),
        "bad_direction" => "scroll direction 无效（up|down|left|right）".into(),
        other => other.to_string(),
    };
    Err(BrowserError {
        code: code.into(),
        message,
        hint: Some(
            "先 browser_snapshot 获取稳定 ref（role:slug:nth），必要时 browser_wait 后再操作"
                .into(),
        ),
        detail: None,
    })
}

pub async fn agent_click(
    app: &AppHandle,
    hosts: &BrowserHosts,
    parent_label: &str,
    ref_id: &str,
) -> Result<BrowserInteractDto, BrowserError> {
    let ref_id = ref_id.trim();
    if ref_id.is_empty() {
        return Err(BrowserError::msg("missing_ref", "element ref 为空"));
    }
    let (mode, host_label) = lookup_host(hosts, parent_label)?;
    // Agent-triggered navigations from click follow agent URL policy (§6.1).
    hosts.set_nav_policy_actor(parent_label, NavActor::Agent);
    let raw = eval_js_string(app, mode, &host_label, &click_js(ref_id)).await?;
    map_interact_result(parse_interact(&raw))
}

pub async fn agent_type(
    app: &AppHandle,
    hosts: &BrowserHosts,
    parent_label: &str,
    ref_id: &str,
    text: &str,
) -> Result<BrowserInteractDto, BrowserError> {
    let ref_id = ref_id.trim();
    if ref_id.is_empty() {
        return Err(BrowserError::msg("missing_ref", "element ref 为空"));
    }
    let (mode, host_label) = lookup_host(hosts, parent_label)?;
    hosts.set_nav_policy_actor(parent_label, NavActor::Agent);
    let raw = eval_js_string(app, mode, &host_label, &type_js(ref_id, text)).await?;
    map_interact_result(parse_interact(&raw))
}

pub async fn agent_scroll(
    app: &AppHandle,
    hosts: &BrowserHosts,
    parent_label: &str,
    ref_id: Option<&str>,
    direction: &str,
    amount: Option<f64>,
) -> Result<BrowserInteractDto, BrowserError> {
    let direction = direction.trim().to_ascii_lowercase();
    if !matches!(direction.as_str(), "up" | "down" | "left" | "right") {
        return Err(BrowserError::msg(
            "bad_direction",
            "scroll direction 须为 up|down|left|right",
        ));
    }
    let (mode, host_label) = lookup_host(hosts, parent_label)?;
    let raw = eval_js_string(
        app,
        mode,
        &host_label,
        &scroll_js(
            ref_id.map(str::trim).filter(|s| !s.is_empty()),
            &direction,
            amount.unwrap_or(400.0),
        ),
    )
    .await?;
    map_interact_result(parse_interact(&raw))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserClickArgs {
    pub r#ref: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserTypeArgs {
    pub r#ref: String,
    pub text: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserScrollArgs {
    pub direction: String,
    pub r#ref: Option<String>,
    pub amount: Option<f64>,
}

#[tauri::command]
pub async fn browser_click(
    app: AppHandle,
    webview: tauri::Webview,
    hosts: State<'_, BrowserHosts>,
    args: BrowserClickArgs,
) -> Result<BrowserInteractDto, BrowserError> {
    agent_click(&app, &hosts, webview.window().label(), &args.r#ref).await
}

#[tauri::command]
pub async fn browser_type(
    app: AppHandle,
    webview: tauri::Webview,
    hosts: State<'_, BrowserHosts>,
    args: BrowserTypeArgs,
) -> Result<BrowserInteractDto, BrowserError> {
    agent_type(
        &app,
        &hosts,
        webview.window().label(),
        &args.r#ref,
        &args.text,
    )
    .await
}

#[tauri::command]
pub async fn browser_scroll(
    app: AppHandle,
    webview: tauri::Webview,
    hosts: State<'_, BrowserHosts>,
    args: BrowserScrollArgs,
) -> Result<BrowserInteractDto, BrowserError> {
    agent_scroll(
        &app,
        &hosts,
        webview.window().label(),
        args.r#ref.as_deref(),
        &args.direction,
        args.amount,
    )
    .await
}

/// Describe an interact action for approval UI (used by runtime via bridge result).
#[allow(dead_code)]
pub fn interact_approval_blurb(op: &str, input: &serde_json::Value) -> String {
    let r = input
        .get("ref")
        .or_else(|| input.get("r#ref"))
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    match op {
        "browser_click" => format!("Browser click ref={r}"),
        "browser_type" => {
            let n = input
                .get("text")
                .and_then(|v| v.as_str())
                .map(|s| s.chars().count())
                .unwrap_or(0);
            format!("Browser type into ref={r} ({n} chars)")
        }
        "browser_scroll" => {
            let dir = input
                .get("direction")
                .and_then(|v| v.as_str())
                .unwrap_or("down");
            format!("Browser scroll {dir} (ref={r})")
        }
        _ => format!("Browser {op}"),
    }
}

#[allow(dead_code)]
pub fn interact_ok_json(dto: &BrowserInteractDto) -> serde_json::Value {
    json!({
        "ok": dto.ok,
        "ref": dto.r#ref,
        "role": dto.role,
        "name": dto.name,
        "error": dto.error,
        "detail": dto.detail,
    })
}

#[allow(dead_code)]
fn _mode_hint(_m: BrowserMode) {}
