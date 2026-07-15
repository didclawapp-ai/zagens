//! Readonly `browser_*` tools (P1). Desktop-only via `ToolBrowserHost` bridge.

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
    // Prefer structured JSON when bridge returns BrowserError JSON-ish payloads.
    if e.contains("browser_host_missing") {
        return ToolError::not_available(e);
    }
    if e.contains("agent_external_needs_ask") {
        return ToolError::permission_denied(e);
    }
    ToolError::execution_failed(e)
}

pub struct BrowserNavigateTool;

#[async_trait]
impl ToolSpec for BrowserNavigateTool {
    fn name(&self) -> &'static str {
        "browser_navigate"
    }

    fn description(&self) -> &'static str {
        "Navigate the Zagens desktop Browser pane to a URL. Loopback http://127.0.0.1|localhost|::1 is allowed. External https requires human approval (do not use for silent browsing)."
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
        // Loopback is free at host policy; external triggers permission_denied / ask path.
        ApprovalRequirement::Auto
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let host = require_browser_host(context)?;
        let url = required_str(&input, "url")?;
        let window_label = optional_str(&input, "window_label");
        let value = host
            .navigate(thread_id(context), window_label, url)
            .await
            .map_err(map_host_err)?;
        Ok(ToolResult::success(value.to_string()))
    }
}

pub struct BrowserSnapshotTool;

#[async_trait]
impl ToolSpec for BrowserSnapshotTool {
    fn name(&self) -> &'static str {
        "browser_snapshot"
    }

    fn description(&self) -> &'static str {
        "Read the current Browser pane as visible text plus a simplified a11y tree with stable element refs. Prefer this over screenshots."
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
            .snapshot(thread_id(context), window_label)
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
        "Return recent console messages from the Browser pane when the host supports it (may be empty in early builds)."
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
}
