//! Direct-fetch HTTP tool. Complements `web_search` for cases where the user
//! already knows the URL — a known repo, a blog post, a spec page — and
//! search is overkill or actively unhelpful.
//!
//! Returns a structured `{url, status, content_type, content, truncated}`
//! payload. HTML responses are stripped to readable text by default
//! (`format = "markdown"`); pass `format = "raw"` to keep the bytes intact
//! when the model wants to do its own parsing.

use super::spec::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec, optional_u64,
};
use super::web_inputs::fetch_url_input_schema;
use async_trait::async_trait;
use serde::Serialize;
use serde_json::Value;
use zagens_runtime_adapters::tools::is_http_url;

const DEFAULT_MAX_BYTES: u64 = 1_000_000;
const HARD_MAX_BYTES: u64 = 10 * 1024 * 1024;
const DEFAULT_TIMEOUT_MS: u64 = 15_000;
const HARD_MAX_TIMEOUT_MS: u64 = 60_000;
const USER_AGENT: &str = "Mozilla/5.0 (compatible; ds-pick-runtime/0.8)";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    Text,
    Markdown,
    Raw,
}

impl Format {
    fn parse(value: Option<&str>) -> Result<Self, ToolError> {
        match value
            .unwrap_or("markdown")
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "text" | "txt" | "plain" => Ok(Self::Text),
            "markdown" | "md" => Ok(Self::Markdown),
            "raw" | "html" | "bytes" => Ok(Self::Raw),
            other => Err(ToolError::invalid_input(format!(
                "unknown format `{other}` (allowed: text, markdown, raw)"
            ))),
        }
    }
}

#[derive(Debug, Serialize)]
struct FetchResponse {
    url: String,
    status: u16,
    content_type: String,
    content: String,
    truncated: bool,
}

pub struct FetchUrlTool;

#[async_trait]
impl ToolSpec for FetchUrlTool {
    fn name(&self) -> &'static str {
        "fetch_url"
    }

    fn description(&self) -> &'static str {
        "Fetch a URL directly (HTTP GET) and return readable page content. Use after `web_search` on the 2–3 most relevant result URLs to read full text — `web_search` snippets alone are not enough for research answers. Also use when the user supplies a known link."
    }

    fn input_schema(&self) -> Value {
        fetch_url_input_schema()
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::ReadOnly, ToolCapability::Network]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Auto
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let url = input
            .get("url")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::invalid_input("`url` is required"))?
            .trim()
            .to_string();

        if url.is_empty() {
            return Err(ToolError::invalid_input("`url` cannot be empty"));
        }
        if !is_http_url(&url) {
            return Err(ToolError::invalid_input(
                "only http:// and https:// URLs are supported",
            ));
        }

        let format = Format::parse(input.get("format").and_then(Value::as_str))?;
        let max_bytes = optional_u64(&input, "max_bytes", DEFAULT_MAX_BYTES).min(HARD_MAX_BYTES);
        let timeout_ms =
            optional_u64(&input, "timeout_ms", DEFAULT_TIMEOUT_MS).min(HARD_MAX_TIMEOUT_MS);

        // SSRF protection (C3): the host of EVERY hop is validated, not just the
        // initial URL. `fetch_with_ssrf_guard` follows redirects manually with
        // `Policy::none()` so each `Location` target is re-resolved + checked
        // against `is_restricted_ip` (a bare `Policy::limited()` would follow a
        // public → 302 → 169.254.169.254 hop with no re-check). DNS failures
        // fail closed instead of being let through to reqwest's resolver.
        let resp = crate::tools::ssrf::fetch_with_ssrf_guard(
            context,
            "fetch_url",
            &url,
            USER_AGENT,
            "text/html,text/plain,application/json,*/*;q=0.5",
            timeout_ms,
        )
        .await?;

        let final_url = resp.url().to_string();
        let status = resp.status();
        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_string();

        // C6: stream the body with a hard byte cap instead of buffering the
        // whole response first — a multi-GB / unbounded response would OOM us
        // before the post-hoc `[..max_bytes]` slice ever ran.
        let (bytes, truncated) = crate::tools::ssrf::read_body_capped(
            resp,
            max_bytes as usize,
            context.cancel_token.as_ref(),
        )
        .await?;

        let body_text = String::from_utf8_lossy(&bytes).to_string();
        let processed = match format {
            Format::Raw => body_text,
            Format::Text | Format::Markdown => {
                if content_type.contains("text/html") || body_text.contains("<html") {
                    crate::tools::html_page_text::html_to_readable_text(&body_text)
                } else {
                    body_text
                }
            }
        };

        let response = FetchResponse {
            url: final_url,
            status: status.as_u16(),
            content_type,
            content: processed,
            truncated,
        };

        use zagens_tools::{EvidenceCitation, EvidenceEnvelope, UncertaintyKind};
        let uncertainty = if truncated {
            UncertaintyKind::Truncated
        } else if !status.is_success() || response.content.trim().is_empty() {
            if response.content.trim().is_empty() {
                UncertaintyKind::NotFound
            } else {
                UncertaintyKind::Partial
            }
        } else {
            UncertaintyKind::None
        };
        let evidence = EvidenceEnvelope::new()
            .with_fact("url", &response.url)
            .with_fact("status", response.status.to_string())
            .with_fact("truncated", truncated.to_string())
            .with_fact("empty", response.content.trim().is_empty().to_string())
            .with_citation(EvidenceCitation::path(&response.url))
            .with_uncertainty(uncertainty);

        if !status.is_success() {
            // Don't `Err` on 4xx/5xx — the caller often wants to see the body
            // (e.g. a JSON error envelope). Mark the result as a failure so the
            // engine renders it as such.
            return Ok(ToolResult {
                content: serde_json::to_string_pretty(&response).map_err(|e| {
                    ToolError::execution_failed(format!("failed to serialize response: {e}"))
                })?,
                success: false,
                metadata: None,
            }
            .with_evidence(evidence));
        }

        ToolResult::json(&response)
            .map_err(|e| ToolError::execution_failed(format!("failed to serialize response: {e}")))
            .map(|r| r.with_evidence(evidence))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::spec::ToolContext;
    use serde_json::json;
    use std::path::PathBuf;

    fn ctx() -> ToolContext {
        ToolContext::new(PathBuf::from("."))
    }

    #[test]
    fn format_parse_accepts_aliases_and_rejects_unknown() {
        assert_eq!(Format::parse(Some("markdown")).unwrap(), Format::Markdown);
        assert_eq!(Format::parse(Some("MD")).unwrap(), Format::Markdown);
        assert_eq!(Format::parse(Some("text")).unwrap(), Format::Text);
        assert_eq!(Format::parse(Some("raw")).unwrap(), Format::Raw);
        assert_eq!(Format::parse(None).unwrap(), Format::Markdown);
        assert!(Format::parse(Some("yaml")).is_err());
    }

    #[tokio::test]
    async fn rejects_non_http_schemes() {
        let tool = FetchUrlTool;
        let res = tool
            .execute(json!({"url": "file:///etc/passwd"}), &ctx())
            .await;
        let err = res.unwrap_err();
        assert!(format!("{err:?}").contains("http"));
    }

    #[tokio::test]
    async fn rejects_empty_url() {
        let tool = FetchUrlTool;
        let res = tool.execute(json!({"url": "   "}), &ctx()).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn rejects_missing_url() {
        let tool = FetchUrlTool;
        let res = tool.execute(json!({}), &ctx()).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn rejects_localhost_hostname() {
        let tool = FetchUrlTool;
        let res = tool
            .execute(json!({"url": "http://localhost:8080/admin"}), &ctx())
            .await;
        let err = res.unwrap_err();
        assert!(format!("{err}").contains("localhost"));
    }

    #[tokio::test]
    async fn network_policy_denies_blocked_host() {
        use crate::network_policy::{Decision, NetworkPolicy, NetworkPolicyDecider};
        let policy = NetworkPolicy {
            default: Decision::Deny.into(),
            allow: vec!["api.deepseek.com".to_string()],
            deny: vec![],
            audit: false,
        };
        let decider = NetworkPolicyDecider::new(policy, None);
        let ctx = ToolContext::new(PathBuf::from(".")).with_network_policy(decider);
        let tool = FetchUrlTool;
        let res = tool
            .execute(json!({"url": "https://example.com/foo"}), &ctx)
            .await;
        let err = res.expect_err("blocked host should fail");
        assert!(format!("{err}").contains("blocked"));
    }
}
