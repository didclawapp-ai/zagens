//! Desktop Browser pane tools (P1 read + P2 write). Via `ToolBrowserHost` bridge.

use super::spec::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec,
    optional_str, optional_u64, required_str,
};
use async_trait::async_trait;
use serde_json::{Value, json};
use zagens_runtime_adapters::tools::ToolBrowserHost;

fn require_browser_host(
    context: &ToolContext,
) -> Result<&std::sync::Arc<dyn ToolBrowserHost>, ToolError> {
    context.runtime.browser_host.as_ref().ok_or_else(|| {
        ToolError::not_available(
            json!({
                "code": "browser_host_missing",
                "message": "Browser tools require the Zagens desktop app with Browser pane open",
                "hint": "请打开 Browser 视图后再试"
            })
            .to_string(),
        )
    })
}

fn thread_id(context: &ToolContext) -> Option<&str> {
    context.runtime.wire.active_thread_id.as_deref()
}

fn map_host_err(e: String) -> ToolError {
    if e.contains("browser_host_missing") {
        return ToolError::not_available(e);
    }
    if e.contains("agent_external_needs_ask") {
        return ToolError::permission_denied(e);
    }
    ToolError::execution_failed(e)
}

fn optional_f64(input: &Value, key: &str) -> Option<f64> {
    input.get(key).and_then(|v| v.as_f64())
}

pub struct BrowserNavigateTool;

#[async_trait]
impl ToolSpec for BrowserNavigateTool {
    fn name(&self) -> &'static str {
        "browser_navigate"
    }

    fn description(&self) -> &'static str {
        "Navigate the Zagens desktop Browser pane to a URL. Loopback http://127.0.0.1|localhost|::1 is allowed without ask. External https prompts an approval card (unless browser_yolo or the host is already on the session allowlist); after approve the host is allowlisted for this profile."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "url": { "type": "string", "description": "Target URL" },
                "window_label": { "type": "string", "description": "Optional desktop window label" }
            },
            "required": ["url"],
            "additionalProperties": false
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::Network, ToolCapability::RequiresApproval]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Auto
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let host = require_browser_host(context)?;
        let url = required_str(&input, "url")?;
        let window_label = optional_str(&input, "window_label");
        // After approval (or yolo / prior allowlist), seed desktop allowlist so url_policy accepts the host.
        if let Some(ext_host) = external_https_host_for_allow(url) {
            host.allow_host(thread_id(context), window_label, &ext_host)
                .await
                .map_err(map_host_err)?;
        }
        let value = host
            .navigate(thread_id(context), window_label, url)
            .await
            .map_err(map_host_err)?;
        Ok(ToolResult::success(value.to_string()))
    }
}

fn external_https_host_for_allow(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    let lower = trimmed.to_ascii_lowercase();
    if !lower.starts_with("https://") {
        return None;
    }
    let rest = trimmed.get(8..)?;
    let hostport = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = hostport
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(':')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    if host.is_empty() || matches!(host.as_str(), "127.0.0.1" | "localhost" | "::1") {
        return None;
    }
    Some(host)
}

pub struct BrowserSnapshotTool;

#[async_trait]
impl ToolSpec for BrowserSnapshotTool {
    fn name(&self) -> &'static str {
        "browser_snapshot"
    }

    fn description(&self) -> &'static str {
        "Read the current Browser pane as visible text plus a simplified a11y tree with stable element refs (`role:slug:nth`, e.g. button:submit:0). Prefer this over screenshots. Use browser_wait before click when the page is still loading."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "window_label": { "type": "string" },
                "include_screenshot": {
                    "type": "boolean",
                    "description": "When true, attach a compact JPEG data-URL of the visible viewport (Windows WebView2). Prefer text/a11y refs; screenshots are large."
                }
            },
            "additionalProperties": false
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::ReadOnly]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Auto
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let host = require_browser_host(context)?;
        let window_label = optional_str(&input, "window_label");
        let include_screenshot = input
            .get("include_screenshot")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let value = host
            .snapshot(thread_id(context), window_label, include_screenshot)
            .await
            .map_err(map_host_err)?;
        Ok(ToolResult::success(value.to_string()))
    }
}

pub struct BrowserGetTextTool;

#[async_trait]
impl ToolSpec for BrowserGetTextTool {
    fn name(&self) -> &'static str {
        "browser_get_text"
    }

    fn description(&self) -> &'static str {
        "Return visible text and title from the Browser pane (subset of browser_snapshot)."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "window_label": { "type": "string" }
            },
            "additionalProperties": false
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::ReadOnly]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Auto
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let host = require_browser_host(context)?;
        let window_label = optional_str(&input, "window_label");
        let value = host
            .get_text(thread_id(context), window_label)
            .await
            .map_err(map_host_err)?;
        Ok(ToolResult::success(value.to_string()))
    }
}

pub struct BrowserConsoleTailTool;

#[async_trait]
impl ToolSpec for BrowserConsoleTailTool {
    fn name(&self) -> &'static str {
        "browser_console_tail"
    }

    fn description(&self) -> &'static str {
        "Return recent console messages from the Browser pane (hook installed at document start; buffer cleared on navigation)."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "limit": { "type": "integer", "minimum": 1, "maximum": 200 },
                "window_label": { "type": "string" }
            },
            "additionalProperties": false
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::ReadOnly]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Auto
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let host = require_browser_host(context)?;
        let window_label = optional_str(&input, "window_label");
        let limit = optional_u64(&input, "limit", 50).min(200) as usize;
        let value = host
            .console_tail(thread_id(context), window_label, limit)
            .await
            .map_err(map_host_err)?;
        Ok(ToolResult::success(value.to_string()))
    }
}

pub struct BrowserClickTool;

#[async_trait]
impl ToolSpec for BrowserClickTool {
    fn name(&self) -> &'static str {
        "browser_click"
    }

    fn description(&self) -> &'static str {
        "Click a Browser pane element by stable ref from browser_snapshot (`role:slug:nth`, e.g. button:submit:0). Never use screen coordinates. Requires approval unless [browser] yolo / ZAGENS_BROWSER_YOLO is enabled."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "ref": { "type": "string", "description": "Stable element ref from browser_snapshot (e.g. button:submit:0)" },
                "window_label": { "type": "string" }
            },
            "required": ["ref"],
            "additionalProperties": false
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::RequiresApproval]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Required
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let host = require_browser_host(context)?;
        let element_ref = required_str(&input, "ref")?;
        let window_label = optional_str(&input, "window_label");
        let value = host
            .click(thread_id(context), window_label, element_ref)
            .await
            .map_err(map_host_err)?;
        Ok(ToolResult::success(value.to_string()))
    }
}

pub struct BrowserTypeTool;

#[async_trait]
impl ToolSpec for BrowserTypeTool {
    fn name(&self) -> &'static str {
        "browser_type"
    }

    fn description(&self) -> &'static str {
        "Type text into a Browser pane element by snapshot ref. Requires approval unless browser_yolo is enabled."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "ref": { "type": "string" },
                "text": { "type": "string" },
                "window_label": { "type": "string" }
            },
            "required": ["ref", "text"],
            "additionalProperties": false
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::RequiresApproval]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Required
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let host = require_browser_host(context)?;
        let element_ref = required_str(&input, "ref")?;
        let text = required_str(&input, "text")?;
        let window_label = optional_str(&input, "window_label");
        let value = host
            .type_text(thread_id(context), window_label, element_ref, text)
            .await
            .map_err(map_host_err)?;
        Ok(ToolResult::success(value.to_string()))
    }
}

pub struct BrowserScrollTool;

#[async_trait]
impl ToolSpec for BrowserScrollTool {
    fn name(&self) -> &'static str {
        "browser_scroll"
    }

    fn description(&self) -> &'static str {
        "Scroll the Browser pane window or a ref container. direction: up|down|left|right. Requires approval unless browser_yolo is enabled."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "direction": { "type": "string", "enum": ["up", "down", "left", "right"] },
                "ref": { "type": "string", "description": "Optional container ref; omit to scroll the window" },
                "amount": { "type": "number", "description": "Pixels (default 400)" },
                "window_label": { "type": "string" }
            },
            "required": ["direction"],
            "additionalProperties": false
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::RequiresApproval]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Required
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let host = require_browser_host(context)?;
        let direction = required_str(&input, "direction")?;
        let element_ref = optional_str(&input, "ref");
        let amount = optional_f64(&input, "amount");
        let window_label = optional_str(&input, "window_label");
        let value = host
            .scroll(
                thread_id(context),
                window_label,
                element_ref,
                direction,
                amount,
            )
            .await
            .map_err(map_host_err)?;
        Ok(ToolResult::success(value.to_string()))
    }
}

pub struct BrowserWaitTool;

#[async_trait]
impl ToolSpec for BrowserWaitTool {
    fn name(&self) -> &'static str {
        "browser_wait"
    }

    fn description(&self) -> &'static str {
        "Wait until a Browser pane condition is true: kind=text (substring in visible text), ref (stable snapshot ref present), selector (CSS), or load (document complete). Default timeout 8000ms (max 30000). Prefer before click/type on slow/SPA pages."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "kind": {
                    "type": "string",
                    "enum": ["text", "ref", "selector", "load"],
                    "description": "Wait condition"
                },
                "value": {
                    "type": "string",
                    "description": "For text/ref/selector: the substring, stable ref, or CSS selector. Omit for load."
                },
                "timeout_ms": {
                    "type": "integer",
                    "minimum": 200,
                    "maximum": 30000,
                    "description": "Default 8000"
                },
                "window_label": { "type": "string" }
            },
            "required": ["kind"],
            "additionalProperties": false
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::ReadOnly]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Auto
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let host = require_browser_host(context)?;
        let kind = required_str(&input, "kind")?;
        let value = optional_str(&input, "value");
        let timeout_ms = input.get("timeout_ms").and_then(|v| v.as_u64());
        let window_label = optional_str(&input, "window_label");
        let value = host
            .wait(thread_id(context), window_label, kind, value, timeout_ms)
            .await
            .map_err(map_host_err)?;
        Ok(ToolResult::success(value.to_string()))
    }
}

pub struct BrowserStartPreviewTool;

#[async_trait]
impl ToolSpec for BrowserStartPreviewTool {
    fn name(&self) -> &'static str {
        "browser_start_preview"
    }

    fn description(&self) -> &'static str {
        "Start the workspace `.zagens/preview.json` command, wait for ready_pattern (substring by default; set ready_regex=true for Rust regex), then browser_navigate to its url. Prefer this for one-click local frontend preview."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "workspace": { "type": "string", "description": "Optional absolute workspace path" },
                "window_label": { "type": "string" }
            },
            "additionalProperties": false
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![
            ToolCapability::ExecutesCode,
            ToolCapability::Network,
            ToolCapability::RequiresApproval,
        ]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Required
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let host = require_browser_host(context)?;
        let workspace = optional_str(&input, "workspace");
        let window_label = optional_str(&input, "window_label");
        let value = host
            .start_preview(thread_id(context), window_label, workspace)
            .await
            .map_err(map_host_err)?;
        Ok(ToolResult::success(value.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::spec::ToolContext;

    #[tokio::test]
    async fn navigate_without_host_is_structured_missing() {
        let ctx = ToolContext::new(std::env::temp_dir());
        let err = BrowserNavigateTool
            .execute(json!({ "url": "http://127.0.0.1:1/" }), &ctx)
            .await
            .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("browser_host_missing"),
            "expected structured missing host, got {msg}"
        );
    }

    #[tokio::test]
    async fn click_without_host_is_structured_missing() {
        let ctx = ToolContext::new(std::env::temp_dir());
        let err = BrowserClickTool
            .execute(json!({ "ref": "button:go:0" }), &ctx)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("browser_host_missing"));
    }

    #[tokio::test]
    async fn wait_without_host_is_structured_missing() {
        let ctx = ToolContext::new(std::env::temp_dir());
        let err = BrowserWaitTool
            .execute(json!({ "kind": "load" }), &ctx)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("browser_host_missing"));
    }
}
