//! Web search tool with multiple provider backends.
//!
//! Default: DuckDuckGo HTML scrape with automatic Bing fallback — works
//! without any configuration. For users who need API-backed or China-
//! accessible search, configure `[search]` in `~/.deepseek/config.toml`:
//!
//! ```toml
//! [search]
//! provider = "metaso"   # or tavily / bocha / baidu / volcengine / bing
//! api_key  = "..."      # not required for metaso (has a built-in community key)
//! ```
//!
//! Supported providers:
//! - `duckduckgo` (default) — HTML scrape, no key needed, Bing fallback
//! - `bing`                 — HTML scrape, no key needed
//! - `tavily`               — Tavily AI Search API (global)
//! - `bocha`                — 博查 (Bocha) AI Search API (China-friendly)
//! - `metaso`               — 秘塔搜索 API (China-friendly, has free default key)
//! - `baidu`                — 百度 AI Search / Qianfan API (China)
//! - `volcengine`           — 火山引擎 Ark Responses API (China, ByteDance)

use super::spec::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec, optional_u64,
};
use crate::config::SearchProvider;
use crate::network_policy::NetworkPolicyDecider;
use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose};
use regex::Regex;
use serde::Serialize;
use serde_json::{Value, json};
use std::sync::OnceLock;
use std::time::Duration;
use zagens_runtime_adapters::tools::check_host_policy;

const DUCKDUCKGO_HOST: &str = "html.duckduckgo.com";
const BING_HOST: &str = "www.bing.com";
const TAVILY_ENDPOINT: &str = "https://api.tavily.com/search";
const BOCHA_ENDPOINT: &str = "https://api.bochaai.com/v1/ai/search";
const METASO_ENDPOINT: &str = "https://metaso.cn/api/v1";
const BAIDU_ENDPOINT: &str = "https://qianfan.baidubce.com/v2/ai_search/web_search";
const VOLCENGINE_RESPONSES_ENDPOINT: &str = "https://ark.cn-beijing.volces.com/api/v3/responses";

/// Intentionally public default key provided by Metaso for open-source / community use.
/// Last-resort fallback after config and env var. Rate-limited to ~100 searches/day.
const METASO_DEFAULT_API_KEY: &str = "mk-E384C1DD5E8501BB7EFE27C949AFDE5B";
const ERROR_BODY_PREVIEW_BYTES: usize = 512;

/// Cap HTML response size so a huge/broken page can't OOM before parse.
const MAX_SEARCH_RESPONSE_BYTES: usize = 5 * 1024 * 1024;

/// Returns `Ok(())` if the policy allows the call, or a `ToolError` otherwise.
/// Falls through silently when no policy is attached (back-compat).
fn check_policy(decider: Option<&NetworkPolicyDecider>, host: &str) -> Result<(), ToolError> {
    check_host_policy(decider, "web_search", host)
        .map_err(|e| ToolError::permission_denied(e.denial_message()))
}

// Cached regex patterns for HTML parsing
static TITLE_RE: OnceLock<Regex> = OnceLock::new();
static SNIPPET_RE: OnceLock<Regex> = OnceLock::new();
static TAG_RE: OnceLock<Regex> = OnceLock::new();
static BING_RESULT_RE: OnceLock<Regex> = OnceLock::new();
static BING_TITLE_RE: OnceLock<Regex> = OnceLock::new();
static BING_SNIPPET_RE: OnceLock<Regex> = OnceLock::new();
static BEARER_TOKEN_RE: OnceLock<Regex> = OnceLock::new();

fn get_title_re() -> &'static Regex {
    TITLE_RE.get_or_init(|| {
        Regex::new(r#"<a[^>]*class=\"result__a\"[^>]*href=\"([^\"]+)\"[^>]*>(.*?)</a>"#)
            .expect("title regex pattern is valid")
    })
}

fn get_snippet_re() -> &'static Regex {
    SNIPPET_RE.get_or_init(|| {
        Regex::new(
            r#"<a[^>]*class=\"result__snippet\"[^>]*>(.*?)</a>|<div[^>]*class=\"result__snippet\"[^>]*>(.*?)</div>"#,
        )
        .expect("snippet regex pattern is valid")
    })
}

fn get_tag_re() -> &'static Regex {
    TAG_RE.get_or_init(|| Regex::new(r"<[^>]+>").expect("tag regex pattern is valid"))
}

fn get_bing_result_re() -> &'static Regex {
    BING_RESULT_RE.get_or_init(|| {
        Regex::new(r#"(?is)<li[^>]*class=\"[^\"]*\bb_algo\b[^\"]*\"[^>]*>(.*?)</li>"#)
            .expect("bing result regex pattern is valid")
    })
}

fn get_bing_title_re() -> &'static Regex {
    BING_TITLE_RE.get_or_init(|| {
        Regex::new(r#"(?is)<h2[^>]*>.*?<a[^>]*href=\"([^\"]+)\"[^>]*>(.*?)</a>"#)
            .expect("bing title regex pattern is valid")
    })
}

fn get_bing_snippet_re() -> &'static Regex {
    BING_SNIPPET_RE.get_or_init(|| {
        Regex::new(r#"(?is)<div[^>]*class=\"[^\"]*\bb_caption\b[^\"]*\"[^>]*>.*?<p[^>]*>(.*?)</p>"#)
            .expect("bing snippet regex pattern is valid")
    })
}

fn get_bearer_token_re() -> &'static Regex {
    BEARER_TOKEN_RE.get_or_init(|| {
        Regex::new(r"(?i)\bBearer\s+[A-Za-z0-9._~+/=-]+")
            .expect("bearer token regex pattern is valid")
    })
}

const DEFAULT_MAX_RESULTS: usize = 8;
const MAX_RESULTS: usize = 15;
const DEFAULT_TIMEOUT_MS: u64 = 15_000;
const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15";

#[derive(Debug, Clone, Serialize)]
struct WebSearchEntry {
    title: String,
    url: String,
    snippet: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct WebSearchResponse {
    query: String,
    source: String,
    count: usize,
    message: String,
    results: Vec<WebSearchEntry>,
}

pub struct WebSearchTool;

#[async_trait]
impl ToolSpec for WebSearchTool {
    fn name(&self) -> &'static str {
        "web_search"
    }

    fn description(&self) -> &'static str {
        "Search the web and return ranked results with URLs and short snippets only — not full page text. \
        After searching, call `fetch_url` on the 2–3 most relevant URLs before answering research questions. \
        Default backend is DuckDuckGo with Bing fallback; set \
        `[search] provider = \"bing\" | \"tavily\" | \"bocha\" | \"metaso\" | \"baidu\" | \"volcengine\"` \
        in config.toml to switch backends. For a known canonical URL, use `fetch_url` directly."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query. Compatibility aliases: q, or search_query[0].q."
                },
                "q": {
                    "type": "string",
                    "description": "Search query."
                },
                "search_query": {
                    "type": "array",
                    "description": "Array form for advanced queries: [{\"q\":\"...\", \"max_results\": 5}]",
                    "items": {
                        "type": "object",
                        "properties": {
                            "q": { "type": "string" },
                            "query": { "type": "string" },
                            "max_results": { "type": "integer" }
                        }
                    }
                },
                "max_results": {
                    "type": "integer",
                    "description": "Maximum number of results to return (default: 8, max: 15)"
                },
                "timeout_ms": {
                    "type": "integer",
                    "description": "Timeout in milliseconds (default: 15000, max: 60000)"
                }
            }
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::ReadOnly, ToolCapability::Network]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Auto
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let query = extract_search_query(&input)?;
        if query.is_empty() {
            return Err(ToolError::invalid_input("Query cannot be empty"));
        }
        let max_results =
            usize::try_from(optional_search_max_results(&input)).unwrap_or(DEFAULT_MAX_RESULTS);
        let max_results = max_results.clamp(1, MAX_RESULTS);
        let timeout_ms = optional_u64(&input, "timeout_ms", DEFAULT_TIMEOUT_MS).min(60_000);

        // Dispatch to API-backed providers before building the HTML-scraping client.
        let decider = context.network_policy.as_ref();
        match &context.search_provider {
            SearchProvider::Tavily => {
                check_policy(decider, "api.tavily.com")?;
                return self
                    .run_tavily_search(&query, max_results, timeout_ms, context)
                    .await;
            }
            SearchProvider::Bocha => {
                check_policy(decider, "api.bochaai.com")?;
                return self
                    .run_bocha_search(&query, max_results, timeout_ms, context)
                    .await;
            }
            SearchProvider::Metaso => {
                check_policy(decider, "metaso.cn")?;
                return self
                    .run_metaso_search(&query, max_results, timeout_ms, context)
                    .await;
            }
            SearchProvider::Baidu => {
                check_policy(decider, "qianfan.baidubce.com")?;
                return self
                    .run_baidu_search(&query, max_results, timeout_ms, context)
                    .await;
            }
            SearchProvider::Volcengine => {
                check_policy(decider, "ark.cn-beijing.volces.com")?;
                return self
                    .run_volcengine_search(&query, max_results, timeout_ms, context)
                    .await;
            }
            SearchProvider::Bing | SearchProvider::DuckDuckGo => {}
        }

        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(timeout_ms))
            .user_agent(USER_AGENT)
            .build()
            .map_err(|e| {
                ToolError::execution_failed(format!("Failed to build HTTP client: {e}"))
            })?;

        // When Bing is explicitly chosen, try it first and fall through to DDG on empty.
        let mut bing_was_empty = false;
        if matches!(context.search_provider, SearchProvider::Bing) {
            check_policy(decider, BING_HOST)?;
            crate::tools::ssrf::ensure_not_cancelled(context.cancel_token.as_ref())?;
            let results =
                run_bing_search(&client, &query, max_results, context.cancel_token.as_ref())
                    .await?;
            if !results.is_empty() {
                return build_result(query, "bing", results, None);
            }
            bing_was_empty = true;
        }

        // DuckDuckGo path (default) with automatic Bing fallback.
        check_policy(decider, DUCKDUCKGO_HOST)?;
        crate::tools::ssrf::ensure_not_cancelled(context.cancel_token.as_ref())?;

        let encoded = url_encode(&query);
        let url = format!("https://html.duckduckgo.com/html/?q={encoded}");

        let ddg_resp = client
            .get(&url)
            .header(
                "Accept",
                "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
            )
            .header("Accept-Language", "en-US,en;q=0.5")
            .send()
            .await;

        let mut results;
        let mut source;
        let mut message_suffix: Option<String> = None;

        match ddg_resp {
            Err(ddg_err) => {
                check_policy(decider, BING_HOST)?;
                match run_bing_search(&client, &query, max_results, context.cancel_token.as_ref())
                    .await
                {
                    Ok(fallback) if !fallback.is_empty() => {
                        results = fallback;
                        source = "bing".to_string();
                        message_suffix = Some(format!(
                            "DuckDuckGo request failed ({ddg_err}); used Bing fallback"
                        ));
                    }
                    Ok(_) => {
                        return Err(ToolError::execution_failed(format!(
                            "Web search failed: DuckDuckGo error ({ddg_err}); Bing fallback returned no results"
                        )));
                    }
                    Err(bing_err) => {
                        return Err(ToolError::execution_failed(format!(
                            "Web search failed: DuckDuckGo error ({ddg_err}); Bing fallback: {bing_err}"
                        )));
                    }
                }
            }
            Ok(resp) => {
                let status = resp.status();
                if !status.is_success() {
                    check_policy(decider, BING_HOST)?;
                    let code = status.as_u16();
                    match run_bing_search(
                        &client,
                        &query,
                        max_results,
                        context.cancel_token.as_ref(),
                    )
                    .await
                    {
                        Ok(fallback) if !fallback.is_empty() => {
                            results = fallback;
                            source = "bing".to_string();
                            message_suffix = Some(format!(
                                "DuckDuckGo returned HTTP {code}; used Bing fallback"
                            ));
                        }
                        Ok(_) => {
                            return Err(ToolError::execution_failed(format!(
                                "Web search failed: DuckDuckGo HTTP {code} and Bing fallback returned no results"
                            )));
                        }
                        Err(bing_err) => {
                            return Err(ToolError::execution_failed(format!(
                                "Web search failed: DuckDuckGo HTTP {code}; Bing fallback: {bing_err}"
                            )));
                        }
                    }
                } else {
                    match crate::tools::ssrf::read_body_capped(
                        resp,
                        MAX_SEARCH_RESPONSE_BYTES,
                        context.cancel_token.as_ref(),
                    )
                    .await
                    {
                        Ok((bytes, _truncated)) => {
                            let body = String::from_utf8_lossy(&bytes).into_owned();
                            source = "duckduckgo".to_string();
                            results = parse_duckduckgo_results(&body, max_results);

                            if bing_was_empty && !results.is_empty() {
                                message_suffix = Some(
                                    "Bing returned no results; used DuckDuckGo fallback"
                                        .to_string(),
                                );
                            }

                            if results.is_empty() {
                                let duckduckgo_blocked = is_duckduckgo_challenge(&body);
                                check_policy(decider, BING_HOST)?;
                                match run_bing_search(
                                    &client,
                                    &query,
                                    max_results,
                                    context.cancel_token.as_ref(),
                                )
                                .await
                                {
                                    Ok(fallback_results) if !fallback_results.is_empty() => {
                                        results = fallback_results;
                                        source = "bing".to_string();
                                        message_suffix = Some(if duckduckgo_blocked {
                                            "DuckDuckGo returned a bot challenge; used Bing fallback"
                                                .to_string()
                                        } else {
                                            "DuckDuckGo returned no parseable results; used Bing fallback"
                                                .to_string()
                                        });
                                    }
                                    Ok(_) if duckduckgo_blocked => {
                                        return Err(ToolError::execution_failed(
                                            "DuckDuckGo returned a bot challenge and Bing fallback returned no results",
                                        ));
                                    }
                                    Err(err) if duckduckgo_blocked => {
                                        return Err(ToolError::execution_failed(format!(
                                            "DuckDuckGo returned a bot challenge and Bing fallback failed: {err}"
                                        )));
                                    }
                                    Ok(_) | Err(_) => {}
                                }
                            }
                        }
                        Err(read_err) => {
                            check_policy(decider, BING_HOST)?;
                            match run_bing_search(
                                &client,
                                &query,
                                max_results,
                                context.cancel_token.as_ref(),
                            )
                            .await
                            {
                                Ok(fallback) if !fallback.is_empty() => {
                                    results = fallback;
                                    source = "bing".to_string();
                                    message_suffix = Some(format!(
                                        "Failed to read DuckDuckGo response ({read_err}); used Bing fallback"
                                    ));
                                }
                                Ok(_) => {
                                    return Err(ToolError::execution_failed(format!(
                                        "Web search failed: failed to read DuckDuckGo response ({read_err}); Bing returned no results"
                                    )));
                                }
                                Err(bing_err) => {
                                    return Err(ToolError::execution_failed(format!(
                                        "Web search failed: failed to read DuckDuckGo response ({read_err}); Bing fallback: {bing_err}"
                                    )));
                                }
                            }
                        }
                    }
                }
            }
        }

        build_result(query, &source, results, message_suffix.as_deref())
    }
}

fn build_result(
    query: String,
    source: &str,
    results: Vec<WebSearchEntry>,
    message_suffix: Option<&str>,
) -> Result<ToolResult, ToolError> {
    let message = if results.is_empty() {
        if message_suffix.is_some_and(|s| s.contains("bot challenge")) {
            "No results found — search engine returned a bot challenge".to_string()
        } else if let Some(suffix) = message_suffix {
            format!("No results found ({suffix})")
        } else {
            format!(
                "No results found via {source} (request succeeded but no parseable entries — HTML layout may have changed)"
            )
        }
    } else if let Some(suffix) = message_suffix {
        format!("Found {} result(s). {suffix}", results.len())
    } else {
        format!("Found {} result(s)", results.len())
    };

    let response = WebSearchResponse {
        query,
        source: source.to_string(),
        count: results.len(),
        message,
        results,
    };

    ToolResult::json(&response).map_err(|e| ToolError::execution_failed(e.to_string()))
}

// ─── API-backed providers ─────────────────────────────────────────────────────

impl WebSearchTool {
    /// Search via Tavily AI Search API.
    async fn run_tavily_search(
        &self,
        query: &str,
        max_results: usize,
        timeout_ms: u64,
        context: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let api_key = context.search_api_key.as_deref().ok_or_else(|| {
            ToolError::execution_failed(
                "Tavily search requires an API key. Set `[search] api_key = \"tvly-...\"` in config.toml.",
            )
        })?;

        let client = build_simple_client(timeout_ms)?;

        let payload = json!({
            "api_key": api_key,
            "query": query,
            "search_depth": "advanced",
            "max_results": max_results,
        });

        let resp = client
            .post(TAVILY_ENDPOINT)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
            .map_err(|e| {
                ToolError::execution_failed(format!("Tavily search request failed: {e}"))
            })?;

        let status = resp.status();
        let body = resp.text().await.map_err(|e| {
            ToolError::execution_failed(format!("Failed to read Tavily response: {e}"))
        })?;

        if !status.is_success() {
            let truncated = truncate_error_body(&body);
            return Err(ToolError::execution_failed(format!(
                "Tavily search failed: HTTP {} — {truncated}",
                status.as_u16()
            )));
        }

        let parsed: Value = serde_json::from_str(&body).map_err(|e| {
            ToolError::execution_failed(format!("Failed to parse Tavily response: {e}"))
        })?;

        let results: Vec<WebSearchEntry> = parsed
            .get("results")
            .and_then(|v| v.as_array())
            .into_iter()
            .flat_map(|arr| arr.iter())
            .filter_map(|item| {
                let title = item.get("title")?.as_str()?.to_string();
                let url = item.get("url")?.as_str()?.to_string();
                let snippet = item
                    .get("content")
                    .or_else(|| item.get("snippet"))
                    .and_then(|s| s.as_str())
                    .map(|s| s.to_string());
                Some(WebSearchEntry {
                    title,
                    url,
                    snippet,
                })
            })
            .take(max_results)
            .collect();

        build_result(query.to_string(), "tavily", results, None)
    }

    /// Search via Bocha (博查) AI Search API.
    async fn run_bocha_search(
        &self,
        query: &str,
        max_results: usize,
        timeout_ms: u64,
        context: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let api_key = context.search_api_key.as_deref().ok_or_else(|| {
            ToolError::execution_failed(
                "Bocha search requires an API key. Set `[search] api_key = \"sk-...\"` in config.toml.",
            )
        })?;

        let client = build_simple_client(timeout_ms)?;

        let payload = json!({
            "query": query,
            "freshness": "noLimit",
            "count": max_results,
        });

        let resp = client
            .post(BOCHA_ENDPOINT)
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {api_key}"))
            .json(&payload)
            .send()
            .await
            .map_err(|e| {
                ToolError::execution_failed(format!("Bocha search request failed: {e}"))
            })?;

        let status = resp.status();
        let body = resp.text().await.map_err(|e| {
            ToolError::execution_failed(format!("Failed to read Bocha response: {e}"))
        })?;

        if !status.is_success() {
            let truncated = truncate_error_body(&body);
            return Err(ToolError::execution_failed(format!(
                "Bocha search failed: HTTP {} — {truncated}",
                status.as_u16()
            )));
        }

        let parsed: Value = serde_json::from_str(&body).map_err(|e| {
            ToolError::execution_failed(format!("Failed to parse Bocha response: {e}"))
        })?;

        // Bocha returns `{"code": 200, "data": {"pages": [...]}}`
        let results: Vec<WebSearchEntry> = parsed
            .get("data")
            .and_then(|d| d.get("pages"))
            .or_else(|| parsed.get("pages"))
            .and_then(|v| v.as_array())
            .into_iter()
            .flat_map(|arr| arr.iter())
            .filter_map(|item| {
                let title = item
                    .get("name")
                    .or_else(|| item.get("title"))
                    .and_then(|s| s.as_str())?
                    .to_string();
                let url = item
                    .get("url")
                    .or_else(|| item.get("link"))
                    .and_then(|s| s.as_str())?
                    .to_string();
                let snippet = item
                    .get("summary")
                    .or_else(|| item.get("snippet"))
                    .or_else(|| item.get("description"))
                    .and_then(|s| s.as_str())
                    .map(|s| s.to_string());
                Some(WebSearchEntry {
                    title,
                    url,
                    snippet,
                })
            })
            .take(max_results)
            .collect();

        build_result(query.to_string(), "bocha", results, None)
    }

    /// Search via Metaso (秘塔) AI Search API.
    ///
    /// Falls back to `METASO_API_KEY` env var, then a built-in community key
    /// (rate-limited to ~100 searches/day) when no config key is set.
    async fn run_metaso_search(
        &self,
        query: &str,
        max_results: usize,
        timeout_ms: u64,
        context: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let env_key = std::env::var("METASO_API_KEY").ok();
        let api_key = context
            .search_api_key
            .as_deref()
            .or(env_key.as_deref())
            .unwrap_or(METASO_DEFAULT_API_KEY);

        let client = build_simple_client(timeout_ms)?;

        let size = max_results.clamp(1, 100);
        let payload = json!({
            "q": query,
            "scope": "webpage",
            "size": size,
        });

        let resp = client
            .post(format!("{METASO_ENDPOINT}/search"))
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {api_key}"))
            .json(&payload)
            .send()
            .await
            .map_err(|e| {
                ToolError::execution_failed(format!("Metaso search request failed: {e}"))
            })?;

        let status = resp.status();
        let body = resp.text().await.map_err(|e| {
            ToolError::execution_failed(format!("Failed to read Metaso response: {e}"))
        })?;

        if !status.is_success() {
            let msg = match status.as_u16() {
                401 | 403 => "Metaso API key rejected — check METASO_API_KEY or set `[search] api_key` in config.toml, or get one at https://metaso.cn/search-api/playground".to_string(),
                429 => "Metaso rate-limited — wait and retry, or get your own API key at https://metaso.cn/search-api/playground".to_string(),
                _ => {
                    let truncated = truncate_error_body(&body);
                    format!("Metaso server error (HTTP {status}) — {truncated}")
                }
            };
            return Err(ToolError::execution_failed(msg));
        }

        let parsed: Value = serde_json::from_str(&body).map_err(|e| {
            ToolError::execution_failed(format!("Failed to parse Metaso response: {e}"))
        })?;

        // Check business-logic error codes.
        if let Some(code) = parsed.get("code").and_then(|v| v.as_i64())
            && code != 0
        {
            let msg = parsed
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown error");
            return Err(ToolError::execution_failed(match code {
                3003 => "Metaso: daily search limit reached — set METASO_API_KEY or get one at https://metaso.cn/search-api/playground".to_string(),
                2005 => "Metaso API key rejected — check METASO_API_KEY or set `[search] api_key` in config.toml".to_string(),
                _ => format!("Metaso API error (code {code}: {msg})"),
            }));
        }

        let results: Vec<WebSearchEntry> = parsed
            .get("webpages")
            .and_then(|v| v.as_array())
            .into_iter()
            .flat_map(|arr| arr.iter())
            .filter_map(|item| {
                let title = item.get("title")?.as_str()?.to_string();
                let url = item.get("link")?.as_str()?.to_string();
                let snippet = item
                    .get("snippet")
                    .or_else(|| item.get("summary"))
                    .and_then(|s| s.as_str())
                    .map(|s| s.to_string());
                Some(WebSearchEntry {
                    title,
                    url,
                    snippet,
                })
            })
            .take(size)
            .collect();

        build_result(query.to_string(), "metaso", results, None)
    }

    /// Search via Baidu AI Search (百度千帆) API.
    async fn run_baidu_search(
        &self,
        query: &str,
        max_results: usize,
        timeout_ms: u64,
        context: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let env_key = std::env::var("BAIDU_SEARCH_API_KEY").ok();
        let api_key = context
            .search_api_key
            .as_deref()
            .or(env_key.as_deref())
            .ok_or_else(|| {
                ToolError::execution_failed(
                    "Baidu search requires an API key. Set `BAIDU_SEARCH_API_KEY` or `[search] api_key` in config.toml.",
                )
            })?;

        let client = build_simple_client(timeout_ms)?;

        let payload = baidu_search_payload(query, max_results);

        let resp = client
            .post(BAIDU_ENDPOINT)
            .header("Authorization", format!("Bearer {api_key}"))
            .json(&payload)
            .send()
            .await
            .map_err(|e| {
                ToolError::execution_failed(format!("Baidu search request failed: {e}"))
            })?;

        let status = resp.status();
        let body = resp.text().await.map_err(|e| {
            ToolError::execution_failed(format!("Failed to read Baidu response: {e}"))
        })?;

        if !status.is_success() {
            let msg = match status.as_u16() {
                401 | 403 => "Baidu search API key rejected — check BAIDU_SEARCH_API_KEY or `[search] api_key` in config.toml".to_string(),
                429 => "Baidu search rate-limited — wait and retry, or check your Baidu AI Search quota".to_string(),
                _ => {
                    let truncated = truncate_error_body(&body);
                    format!("Baidu search failed: HTTP {} — {truncated}", status.as_u16())
                }
            };
            return Err(ToolError::execution_failed(msg));
        }

        let parsed: Value = serde_json::from_str(&body).map_err(|e| {
            ToolError::execution_failed(format!("Failed to parse Baidu response: {e}"))
        })?;

        if let Some(error) = baidu_error_message(&parsed) {
            return Err(ToolError::execution_failed(error));
        }

        let results = parse_baidu_results(&parsed, max_results);
        build_result(query.to_string(), "baidu", results, None)
    }

    /// Search via Volcengine Ark Responses API (火山引擎).
    ///
    /// Uses the Ark web_search tool with strict JSON output constraints.
    /// Enforces a minimum 90 s timeout because the pipeline (search + model
    /// inference + JSON generation) is inherently slower than simple APIs.
    /// Transient transport errors are retried twice with exponential back-off.
    async fn run_volcengine_search(
        &self,
        query: &str,
        max_results: usize,
        timeout_ms: u64,
        context: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let volc_key = std::env::var("VOLCENGINE_API_KEY").ok();
        let volc_ark_key = std::env::var("VOLCENGINE_ARK_API_KEY").ok();
        let ark_key = std::env::var("ARK_API_KEY").ok();
        let api_key = context
            .search_api_key
            .as_deref()
            .or(volc_key.as_deref())
            .or(volc_ark_key.as_deref())
            .or(ark_key.as_deref())
            .ok_or_else(|| {
                ToolError::execution_failed(
                    "Volcengine search requires an API key. Set `[search] api_key`, \
                     or VOLCENGINE_API_KEY / VOLCENGINE_ARK_API_KEY / ARK_API_KEY env var.",
                )
            })?;

        let effective_timeout = timeout_ms.max(90_000);

        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_millis(effective_timeout))
            .tcp_keepalive(Some(Duration::from_secs(30)))
            .http2_keep_alive_interval(Some(Duration::from_secs(15)))
            .http2_keep_alive_timeout(Duration::from_secs(20))
            .user_agent(USER_AGENT)
            .build()
            .map_err(|e| {
                ToolError::execution_failed(format!("Failed to build HTTP client: {e}"))
            })?;

        let payload = volcengine_search_payload(query, max_results);

        // Retry transient transport errors up to 2 times: backoff 1 s, 2 s.
        let mut last_err: Option<ToolError> = None;
        for attempt in 0..3u32 {
            if attempt > 0 {
                tokio::time::sleep(Duration::from_millis(1000 * (1 << (attempt - 1)))).await;
            }

            match client
                .post(VOLCENGINE_RESPONSES_ENDPOINT)
                .header("Authorization", format!("Bearer {api_key}"))
                .json(&payload)
                .send()
                .await
            {
                Ok(resp) => {
                    let status = resp.status();
                    let body = resp.text().await.map_err(|e| {
                        ToolError::execution_failed(format!(
                            "Failed to read Volcengine response: {e}"
                        ))
                    })?;

                    if !status.is_success() {
                        let msg = match status.as_u16() {
                            401 | 403 => "Volcengine API key rejected — check `[search] api_key` in config.toml or VOLCENGINE_API_KEY / VOLCENGINE_ARK_API_KEY / ARK_API_KEY".to_string(),
                            429 => "Volcengine API rate-limited — wait and retry, or check your quota".to_string(),
                            _ => {
                                let truncated = truncate_error_body(&body);
                                format!("Volcengine search failed: HTTP {} — {truncated}", status.as_u16())
                            }
                        };
                        return Err(ToolError::execution_failed(msg));
                    }

                    let parsed: Value = serde_json::from_str(&body).map_err(|e| {
                        ToolError::execution_failed(format!(
                            "Failed to parse Volcengine response: {e}"
                        ))
                    })?;

                    if let Some(error) = volcengine_error_message(&parsed) {
                        return Err(ToolError::execution_failed(error));
                    }

                    let response_text = volcengine_extract_text(&parsed).ok_or_else(|| {
                        ToolError::execution_failed("Volcengine response contains no output text")
                    })?;

                    let results = parse_volcengine_results(&response_text, max_results);
                    return build_result(query.to_string(), "volcengine", results, None);
                }
                Err(e) => {
                    let is_transient = e.is_timeout() || e.is_connect();
                    if !is_transient || attempt == 2 {
                        return Err(ToolError::execution_failed(format!(
                            "Volcengine search request failed: {e}"
                        )));
                    }
                    last_err = Some(ToolError::execution_failed(format!(
                        "Volcengine search request failed (attempt {}/3): {e}",
                        attempt + 1
                    )));
                }
            }
        }

        Err(last_err.unwrap_or_else(|| {
            ToolError::execution_failed("Volcengine search: unexpected retry exit")
        }))
    }
}

// ─── Shared helpers ──────────────────────────────────────────────────────────

fn build_simple_client(timeout_ms: u64) -> Result<reqwest::Client, ToolError> {
    reqwest::Client::builder()
        .timeout(Duration::from_millis(timeout_ms))
        .build()
        .map_err(|e| ToolError::execution_failed(format!("Failed to build HTTP client: {e}")))
}

fn truncate_error_body(body: &str) -> String {
    let stripped = sanitize_error_body(body);
    if stripped.len() <= ERROR_BODY_PREVIEW_BYTES {
        stripped
    } else {
        let mut end = ERROR_BODY_PREVIEW_BYTES;
        while !stripped.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}...", &stripped[..end])
    }
}

fn sanitize_error_body(body: &str) -> String {
    let stripped = strip_html_tags(body);
    let visible: String = stripped
        .chars()
        .filter(|c| !c.is_control() || c.is_ascii_whitespace())
        .collect();
    get_bearer_token_re()
        .replace_all(&visible, "Bearer [REDACTED]")
        .to_string()
}

fn baidu_search_payload(query: &str, max_results: usize) -> Value {
    json!({
        "messages": [{"role": "user", "content": query}],
        "search_source": "baidu_search_v2",
        "resource_type_filter": [{"type": "web", "top_k": max_results}],
    })
}

fn parse_baidu_results(parsed: &Value, max_results: usize) -> Vec<WebSearchEntry> {
    parsed
        .get("references")
        .and_then(|v| v.as_array())
        .into_iter()
        .flat_map(|arr| arr.iter())
        .filter_map(|item| {
            let title = item
                .get("title")
                .or_else(|| item.get("name"))
                .and_then(|s| s.as_str())?
                .trim();
            let url = item
                .get("url")
                .or_else(|| item.get("link"))
                .and_then(|s| s.as_str())?
                .trim();
            if title.is_empty() || url.is_empty() {
                return None;
            }
            let snippet = item
                .get("content")
                .or_else(|| item.get("snippet"))
                .or_else(|| item.get("summary"))
                .and_then(|s| s.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToString::to_string);
            Some(WebSearchEntry {
                title: title.to_string(),
                url: url.to_string(),
                snippet,
            })
        })
        .take(max_results)
        .collect()
}

fn baidu_error_message(parsed: &Value) -> Option<String> {
    let code = parsed
        .get("error_code")
        .or_else(|| parsed.get("code"))
        .and_then(|v| v.as_i64())?;
    if code == 0 {
        return None;
    }
    let message = parsed
        .get("error_msg")
        .or_else(|| parsed.get("message"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown error");
    Some(format!("Baidu search API error (code {code}: {message})"))
}

fn volcengine_search_payload(query: &str, max_results: usize) -> Value {
    json!({
        "model": "doubao-seed-2-0-lite-260428",
        "stream": false,
        "tools": [{"type": "web_search"}],
        "input": [{
            "role": "user",
            "content": [{
                "type": "input_text",
                "text": format!(
                    "Search the web for: {query}\n\n\
                    CRITICAL: Respond ONLY with a valid JSON object. No markdown, no explanation.\n\
                    Schema: {{\"results\":[{{\"title\":\"...\",\"url\":\"https://...\",\"snippet\":\"...\"}}]}}\n\
                    - results: 1-{max_results} most relevant pages\n\
                    - title: page title (required)\n\
                    - url: full URL starting with https:// (required)\n\
                    - snippet: 1-2 sentence factual summary (required)\n\
                    - If zero results: {{\"results\":[]}}\n\
                    - Your entire response must be valid, parseable JSON."
                )
            }]
        }]
    })
}

fn volcengine_extract_text(parsed: &Value) -> Option<String> {
    parsed
        .get("output")
        .and_then(|v| v.as_array())
        .into_iter()
        .flat_map(|arr| arr.iter().rev())
        .find(|item| item.get("type").and_then(|t| t.as_str()) == Some("message"))
        .and_then(|msg| msg.get("content").and_then(|c| c.as_array()))
        .and_then(|content| {
            content
                .iter()
                .find(|c| c.get("text").and_then(|t| t.as_str()).is_some())
        })
        .and_then(|c| c.get("text").and_then(|t| t.as_str()))
        .map(|s| s.to_string())
}

fn volcengine_error_message(parsed: &Value) -> Option<String> {
    let error = parsed.get("error")?;
    let code = error
        .get("code")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let message = error
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("no details");
    Some(format!("Volcengine API error (code {code}: {message})"))
}

fn parse_volcengine_results(response_text: &str, max_results: usize) -> Vec<WebSearchEntry> {
    let json_text = extract_json_block(response_text).unwrap_or(response_text);
    let parsed: Value = match serde_json::from_str(json_text) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    parsed
        .get("results")
        .and_then(|v| v.as_array())
        .into_iter()
        .flat_map(|arr| arr.iter())
        .filter_map(|item| {
            let title = item.get("title").and_then(|s| s.as_str())?.trim();
            let url = item.get("url").and_then(|s| s.as_str())?.trim();
            if title.is_empty() || url.is_empty() {
                return None;
            }
            let snippet = item
                .get("snippet")
                .and_then(|s| s.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToString::to_string);
            Some(WebSearchEntry {
                title: title.to_string(),
                url: url.to_string(),
                snippet,
            })
        })
        .take(max_results)
        .collect()
}

fn extract_json_block(text: &str) -> Option<&str> {
    if let Some(start) = text.find("```json") {
        let inner = &text[start + 7..];
        if let Some(end) = inner.find("```") {
            return Some(inner[..end].trim());
        }
    }
    if let Some(start) = text.find('{')
        && let Some(end) = text.rfind('}')
    {
        return Some(&text[start..=end]);
    }
    None
}

// ─── HTML scraping (DuckDuckGo / Bing) ───────────────────────────────────────

async fn run_bing_search(
    client: &reqwest::Client,
    query: &str,
    max_results: usize,
    cancel: Option<&tokio_util::sync::CancellationToken>,
) -> Result<Vec<WebSearchEntry>, ToolError> {
    let encoded = url_encode(query);
    let url = format!("https://www.bing.com/search?q={encoded}");
    let resp = client
        .get(&url)
        .header(
            "Accept",
            "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
        )
        .header("Accept-Language", "en-US,en;q=0.9")
        .send()
        .await
        .map_err(|e| ToolError::execution_failed(format!("Bing fallback request failed: {e}")))?;

    let status = resp.status();
    let (bytes, _truncated) =
        crate::tools::ssrf::read_body_capped(resp, MAX_SEARCH_RESPONSE_BYTES, cancel).await?;
    let body = String::from_utf8_lossy(&bytes).into_owned();

    if !status.is_success() {
        return Err(ToolError::execution_failed(format!(
            "Bing fallback failed: HTTP {}",
            status.as_u16()
        )));
    }

    Ok(parse_bing_results(&body, max_results))
}

fn parse_duckduckgo_results(html: &str, max_results: usize) -> Vec<WebSearchEntry> {
    let title_re = get_title_re();
    let snippet_re = get_snippet_re();
    let snippets: Vec<String> = snippet_re
        .captures_iter(html)
        .filter_map(|cap| cap.get(1).or_else(|| cap.get(2)))
        .map(|m| normalize_text(m.as_str()))
        .collect();

    let mut results = Vec::new();
    for (idx, cap) in title_re.captures_iter(html).enumerate() {
        if results.len() >= max_results {
            break;
        }
        let href = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let title_raw = cap.get(2).map(|m| m.as_str()).unwrap_or("");
        let title = normalize_text(title_raw);
        if title.is_empty() {
            continue;
        }
        let url = normalize_url(href);
        let snippet = snippets
            .get(idx)
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty());

        results.push(WebSearchEntry {
            title,
            url,
            snippet,
        });
    }

    results
}

fn is_duckduckgo_challenge(html: &str) -> bool {
    html.contains("anomaly-modal") || html.contains("Unfortunately, bots use DuckDuckGo too")
}

fn parse_bing_results(html: &str, max_results: usize) -> Vec<WebSearchEntry> {
    let mut results = Vec::new();
    for cap in get_bing_result_re().captures_iter(html) {
        if results.len() >= max_results {
            break;
        }
        let Some(block) = cap.get(1).map(|m| m.as_str()) else {
            continue;
        };
        let Some(title_cap) = get_bing_title_re().captures(block) else {
            continue;
        };
        let href = title_cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let title_raw = title_cap.get(2).map(|m| m.as_str()).unwrap_or("");
        let title = normalize_text(title_raw);
        if title.is_empty() {
            continue;
        }
        let snippet = get_bing_snippet_re()
            .captures(block)
            .and_then(|snippet_cap| snippet_cap.get(1))
            .map(|m| normalize_text(m.as_str()))
            .filter(|s| !s.is_empty());

        results.push(WebSearchEntry {
            title,
            url: normalize_bing_url(href),
            snippet,
        });
    }

    results
}

fn normalize_url(href: &str) -> String {
    if let Some(uddg) = extract_query_param(href, "uddg") {
        let decoded = percent_decode(&uddg);
        if !decoded.is_empty() {
            return decoded;
        }
    }
    if href.starts_with("//") {
        return format!("https:{href}");
    }
    if href.starts_with('/') {
        return format!("https://duckduckgo.com{href}");
    }
    href.to_string()
}

fn normalize_bing_url(href: &str) -> String {
    if let Some(encoded) = extract_query_param(href, "u") {
        let decoded = percent_decode(&encoded);
        let token = decoded.strip_prefix("a1").unwrap_or(&decoded);
        let mut padded = token.replace('-', "+").replace('_', "/");
        while !padded.len().is_multiple_of(4) {
            padded.push('=');
        }
        if let Ok(bytes) = general_purpose::STANDARD.decode(padded)
            && let Ok(url) = String::from_utf8(bytes)
            && (url.starts_with("http://") || url.starts_with("https://"))
        {
            return url;
        }
    }
    if href.starts_with("//") {
        return format!("https:{href}");
    }
    if href.starts_with('/') {
        return format!("https://www.bing.com{href}");
    }
    href.to_string()
}

fn normalize_text(text: &str) -> String {
    let stripped = strip_html_tags(text);
    let decoded = decode_html_entities(&stripped);
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn strip_html_tags(text: &str) -> String {
    get_tag_re().replace_all(text, "").to_string()
}

fn decode_html_entities(text: &str) -> String {
    use regex::Regex;
    use std::sync::OnceLock;

    static ENTITY_RE: OnceLock<Regex> = OnceLock::new();
    let re = ENTITY_RE.get_or_init(|| {
        Regex::new(r"&(?:#(\d+)|#x([0-9A-Fa-f]+)|([a-zA-Z]+));").expect("HTML entity regex")
    });

    re.replace_all(text, |caps: &regex::Captures| {
        if let Some(dec) = caps.get(1) {
            return dec
                .as_str()
                .parse::<u32>()
                .ok()
                .and_then(std::char::from_u32)
                .unwrap_or('\u{FFFD}')
                .to_string();
        }
        if let Some(hex) = caps.get(2) {
            return u32::from_str_radix(hex.as_str(), 16)
                .ok()
                .and_then(std::char::from_u32)
                .unwrap_or('\u{FFFD}')
                .to_string();
        }
        let named = caps.get(3).map(|m| m.as_str());
        match named {
            Some("amp") => "&",
            Some("lt") => "<",
            Some("gt") => ">",
            Some("quot") => "\"",
            Some("apos") => "'",
            Some("nbsp") => " ",
            Some("copy") => "\u{00A9}",
            Some("reg") => "\u{00AE}",
            Some("mdash") => "\u{2014}",
            Some("ndash") => "\u{2013}",
            Some("lsquo") => "\u{2018}",
            Some("rsquo") => "\u{2019}",
            Some("ldquo") => "\u{201C}",
            Some("rdquo") => "\u{201D}",
            Some("hellip") => "\u{2026}",
            _ => return caps.get(0).map(|m| m.as_str()).unwrap_or("").to_string(),
        }
        .to_string()
    })
    .to_string()
}

fn url_encode(input: &str) -> String {
    crate::utils::url_encode(input)
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = &input[i + 1..i + 3];
                if let Ok(val) = u8::from_str_radix(hex, 16) {
                    out.push(val);
                    i += 3;
                    continue;
                }
                out.push(bytes[i]);
            }
            b'+' => out.push(b' '),
            _ => out.push(bytes[i]),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn extract_query_param(url: &str, key: &str) -> Option<String> {
    let query = url.split_once('?')?.1;
    for part in query.split('&') {
        let mut iter = part.splitn(2, '=');
        let name = iter.next().unwrap_or("");
        if name == key {
            return iter.next().map(str::to_string);
        }
    }
    None
}

// ─── Input parsing ────────────────────────────────────────────────────────────

fn extract_search_query(input: &Value) -> Result<String, ToolError> {
    for key in ["query", "q"] {
        if let Some(value) = input.get(key) {
            let Some(query) = value.as_str() else {
                return Err(ToolError::invalid_input(format!(
                    "Field '{key}' must be a string"
                )));
            };
            let query = query.trim();
            if !query.is_empty() {
                return Ok(query.to_string());
            }
        }
    }

    for item in search_query_items(input) {
        for key in ["q", "query"] {
            if let Some(value) = item.get(key) {
                let Some(query) = value.as_str() else {
                    return Err(ToolError::invalid_input(format!(
                        "Field 'search_query[].{key}' must be a string"
                    )));
                };
                let query = query.trim();
                if !query.is_empty() {
                    return Ok(query.to_string());
                }
            }
        }
    }

    Err(ToolError::missing_field("query"))
}

fn optional_search_max_results(input: &Value) -> u64 {
    if let Some(value) = input.get("max_results").and_then(Value::as_u64) {
        return value;
    }
    search_query_items(input)
        .filter_map(|item| item.get("max_results").and_then(Value::as_u64))
        .next()
        .unwrap_or(DEFAULT_MAX_RESULTS as u64)
}

fn search_query_items(input: &Value) -> impl Iterator<Item = &Value> {
    input
        .get("search_query")
        .and_then(Value::as_array)
        .into_iter()
        .flat_map(|items| items.iter())
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{decode_html_entities, extract_search_query, optional_search_max_results};
    use serde_json::json;

    #[test]
    fn decode_html_entities_handles_named_entities() {
        assert_eq!(decode_html_entities("&amp;"), "&");
        assert_eq!(decode_html_entities("&lt;"), "<");
        assert_eq!(decode_html_entities("&gt;"), ">");
        assert_eq!(decode_html_entities("&quot;"), "\"");
        assert_eq!(decode_html_entities("&apos;"), "'");
        assert_eq!(decode_html_entities("&nbsp;"), " ");
        assert_eq!(decode_html_entities("&copy;"), "\u{00A9}");
        assert_eq!(decode_html_entities("&mdash;"), "\u{2014}");
    }

    #[test]
    fn decode_html_entities_handles_decimal_numeric_references() {
        assert_eq!(decode_html_entities("&#65;"), "A");
        assert_eq!(decode_html_entities("&#60;"), "<");
        assert_eq!(decode_html_entities("&#8211;"), "\u{2013}");
    }

    #[test]
    fn decode_html_entities_handles_hex_numeric_references() {
        assert_eq!(decode_html_entities("&#x41;"), "A");
        assert_eq!(decode_html_entities("&#x3C;"), "<");
        assert_eq!(decode_html_entities("&#x2014;"), "\u{2014}");
    }

    #[test]
    fn decode_html_entities_passthrough_unknown() {
        assert_eq!(decode_html_entities("&unknown;"), "&unknown;");
    }

    #[test]
    fn decode_html_entities_mixed_content() {
        let input = "Hello &amp; welcome to &quot;Rust&apos;s world&quot; &mdash; enjoy!";
        let expected = "Hello & welcome to \"Rust's world\" \u{2014} enjoy!";
        assert_eq!(decode_html_entities(input), expected);
    }

    #[test]
    fn extract_search_query_accepts_legacy_query() {
        let query =
            extract_search_query(&json!({"query": " deepseek v4 "})).expect("query should parse");
        assert_eq!(query, "deepseek v4");
    }

    #[test]
    fn extract_search_query_accepts_q_alias() {
        let query =
            extract_search_query(&json!({"q": "deepseek v4 pro"})).expect("q alias should parse");
        assert_eq!(query, "deepseek v4 pro");
    }

    #[test]
    fn extract_search_query_accepts_array_form() {
        let input = json!({"search_query": [{"q": "deepseek api", "max_results": 3}]});
        let query = extract_search_query(&input).expect("array form should parse");
        assert_eq!(query, "deepseek api");
        assert_eq!(optional_search_max_results(&input), 3);
    }

    #[test]
    fn extract_search_query_rejects_missing_query() {
        let err = extract_search_query(&json!({"max_results": 2}))
            .expect_err("missing query should fail");
        assert!(format!("{err}").contains("missing required field 'query'"));
    }
}
