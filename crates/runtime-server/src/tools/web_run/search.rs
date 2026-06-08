//! Web search backends (DuckDuckGo, Bing, image query).

use super::USER_AGENT;
use super::html::{
    is_duckduckgo_challenge, parse_bing_results, parse_duckduckgo_results, url_encode,
};
use super::types::{ImageResultEntry, SearchEntry, WebLink, WebPage};
use crate::network_policy::NetworkPolicyDecider;
use crate::tools::spec::{ToolContext, ToolError};
use serde::Deserialize;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use zagens_runtime_adapters::tools::check_host_policy;

const DUCKDUCKGO_HOST: &str = "html.duckduckgo.com";
const BING_HOST: &str = "www.bing.com";
const MAX_SEARCH_RESPONSE_BYTES: usize = 5 * 1024 * 1024;

fn check_policy(decider: Option<&NetworkPolicyDecider>, host: &str) -> Result<(), ToolError> {
    check_host_policy(decider, "web.run", host)
        .map_err(|e| ToolError::permission_denied(e.denial_message()))
}

pub(in crate::tools::web_run) async fn run_search(
    query: &str,
    max_results: usize,
    timeout_ms: u64,
    domains: &[String],
    context: &ToolContext,
) -> Result<(Vec<SearchEntry>, String, Option<String>), ToolError> {
    let decider = context.network_policy.as_ref();
    check_policy(decider, DUCKDUCKGO_HOST)?;
    crate::tools::ssrf::ensure_not_cancelled(context.cancel_token.as_ref())?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(timeout_ms))
        .user_agent(USER_AGENT)
        .build()
        .map_err(|e| ToolError::execution_failed(format!("Failed to build HTTP client: {e}")))?;

    let encoded = url_encode(query);
    let url = format!("https://html.duckduckgo.com/html/?q={encoded}");
    let cancel = context.cancel_token.as_ref();

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
    let mut warnings = Vec::new();

    match ddg_resp {
        Err(err) => {
            check_policy(decider, BING_HOST)?;
            match run_bing_search(&client, query, max_results, cancel).await {
                Ok(fallback) if !fallback.is_empty() => {
                    results = fallback;
                    source = "bing".to_string();
                    warnings.push(format!(
                        "DuckDuckGo request failed ({err}); used Bing fallback"
                    ));
                }
                Ok(_) => {
                    return Err(ToolError::execution_failed(format!(
                        "Web search failed: DuckDuckGo request failed ({err}); Bing returned no results"
                    )));
                }
                Err(bing_err) => {
                    return Err(ToolError::execution_failed(format!(
                        "Web search failed: DuckDuckGo request failed ({err}); Bing fallback: {bing_err}"
                    )));
                }
            }
        }
        Ok(resp) => {
            let status = resp.status();
            if !status.is_success() {
                check_policy(decider, BING_HOST)?;
                let code = status.as_u16();
                match run_bing_search(&client, query, max_results, cancel).await {
                    Ok(fallback) if !fallback.is_empty() => {
                        results = fallback;
                        source = "bing".to_string();
                        warnings.push(format!(
                            "DuckDuckGo returned HTTP {code}; used Bing fallback"
                        ));
                    }
                    Ok(_) => {
                        return Err(ToolError::execution_failed(format!(
                            "Web search failed: DuckDuckGo HTTP {code} and Bing returned no results"
                        )));
                    }
                    Err(bing_err) => {
                        return Err(ToolError::execution_failed(format!(
                            "Web search failed: DuckDuckGo HTTP {code}; Bing fallback: {bing_err}"
                        )));
                    }
                }
            } else {
                match crate::tools::ssrf::read_body_capped(resp, MAX_SEARCH_RESPONSE_BYTES, cancel)
                    .await
                {
                    Ok((bytes, _truncated)) => {
                        let body = String::from_utf8_lossy(&bytes).into_owned();
                        source = "duckduckgo".to_string();
                        results = parse_duckduckgo_results(&body, max_results);
                        if results.is_empty() {
                            let duckduckgo_blocked = is_duckduckgo_challenge(&body);
                            check_policy(decider, BING_HOST)?;
                            match run_bing_search(&client, query, max_results, cancel).await {
                                Ok(fallback_results) if !fallback_results.is_empty() => {
                                    results = fallback_results;
                                    source = "bing".to_string();
                                    warnings.push(if duckduckgo_blocked {
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
                        match run_bing_search(&client, query, max_results, cancel).await {
                            Ok(fallback) if !fallback.is_empty() => {
                                results = fallback;
                                source = "bing".to_string();
                                warnings.push(format!(
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

    if !domains.is_empty() {
        let before = results.len();
        results.retain(|entry| domain_matches(&entry.url, domains));
        if before != results.len() {
            warnings.push("Filtered search results by domain list".to_string());
        }
    }

    Ok((
        results,
        source,
        if warnings.is_empty() {
            None
        } else {
            Some(warnings.join("; "))
        },
    ))
}

pub(in crate::tools::web_run) async fn run_bing_search(
    client: &reqwest::Client,
    query: &str,
    max_results: usize,
    cancel: Option<&CancellationToken>,
) -> Result<Vec<SearchEntry>, ToolError> {
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

pub(in crate::tools::web_run) fn domain_matches(url: &str, domains: &[String]) -> bool {
    if domains.is_empty() {
        return true;
    }
    let Ok(parsed) = reqwest::Url::parse(url) else {
        return false;
    };
    let Some(host) = parsed.host_str() else {
        return false;
    };
    domains.iter().any(|domain| {
        let domain = domain.trim_start_matches("www.");
        host == domain || host.ends_with(&format!(".{domain}"))
    })
}

#[derive(Debug, Clone, Deserialize)]
pub(in crate::tools::web_run) struct DuckDuckGoImageResponse {
    #[serde(default)]
    results: Vec<DuckDuckGoImageResult>,
}

#[derive(Debug, Clone, Deserialize)]
pub(in crate::tools::web_run) struct DuckDuckGoImageResult {
    image: String,
    #[serde(default)]
    thumbnail: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    width: Option<u32>,
    #[serde(default)]
    height: Option<u32>,
}

pub(in crate::tools::web_run) fn extract_duckduckgo_vqd(html: &str) -> Option<String> {
    let html = html.trim();
    if html.is_empty() {
        return None;
    }

    for (prefix, suffix) in [("vqd='", "'"), ("vqd=\"", "\"")] {
        if let Some(start) = html.find(prefix) {
            let rest = &html[start + prefix.len()..];
            if let Some(end) = rest.find(suffix) {
                let token = rest[..end].trim();
                if !token.is_empty() {
                    return Some(token.to_string());
                }
            }
        }
    }

    // Fallback: look for `vqd=` and accept a conservative token charset.
    if let Some(start) = html.find("vqd=") {
        let rest = &html[start + 4..];
        let mut token = String::new();
        for ch in rest.chars() {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                token.push(ch);
            } else {
                break;
            }
        }
        if !token.is_empty() {
            return Some(token);
        }
    }

    None
}

pub(in crate::tools::web_run) async fn run_image_search(
    query: &str,
    max_results: usize,
    timeout_ms: u64,
    domains: &[String],
) -> Result<(Vec<ImageResultEntry>, Option<String>), ToolError> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(timeout_ms))
        .user_agent(USER_AGENT)
        .build()
        .map_err(|e| ToolError::execution_failed(format!("Failed to build HTTP client: {e}")))?;

    // Step 1: fetch the HTML page to obtain the `vqd` token used by the images API.
    let encoded = url_encode(query);
    let seed_url = format!("https://duckduckgo.com/?q={encoded}&iax=images&ia=images");
    let seed_resp = client
        .get(&seed_url)
        .header(
            "Accept",
            "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
        )
        .header("Accept-Language", "en-US,en;q=0.5")
        .send()
        .await
        .map_err(|e| {
            ToolError::execution_failed(format!("Image search seed request failed: {e}"))
        })?;

    let seed_status = seed_resp.status();
    let seed_body = seed_resp.text().await.map_err(|e| {
        ToolError::execution_failed(format!("Failed to read image seed response: {e}"))
    })?;

    if !seed_status.is_success() {
        return Err(ToolError::execution_failed(format!(
            "Image search seed request failed: HTTP {}",
            seed_status.as_u16()
        )));
    }

    let vqd = extract_duckduckgo_vqd(&seed_body).ok_or_else(|| {
        ToolError::execution_failed("Failed to extract DuckDuckGo image token (vqd)")
    })?;

    // Step 2: query the DuckDuckGo images JSON endpoint.
    let api_url = format!("https://duckduckgo.com/i.js?l=us-en&o=json&q={encoded}&vqd={vqd}&p=1");
    let api_resp = client
        .get(&api_url)
        .header("Accept", "application/json")
        .header("Referer", "https://duckduckgo.com/")
        .send()
        .await
        .map_err(|e| ToolError::execution_failed(format!("Image search request failed: {e}")))?;

    let api_status = api_resp.status();
    let api_body = api_resp
        .text()
        .await
        .map_err(|e| ToolError::execution_failed(format!("Failed to read image response: {e}")))?;

    if !api_status.is_success() {
        return Err(ToolError::execution_failed(format!(
            "Image search failed: HTTP {}",
            api_status.as_u16()
        )));
    }

    let parsed: DuckDuckGoImageResponse = serde_json::from_str(&api_body).map_err(|e| {
        ToolError::execution_failed(format!("Failed to parse image search JSON: {e}"))
    })?;

    let mut results = parsed
        .results
        .into_iter()
        .filter(|item| !item.image.trim().is_empty())
        .map(|item| ImageResultEntry {
            image: item.image,
            thumbnail: item.thumbnail,
            title: item.title,
            url: item.url,
            source: item.source,
            width: item.width,
            height: item.height,
        })
        .collect::<Vec<_>>();

    // Domain filter is applied to the source page URL when available.
    let warning = if !domains.is_empty() {
        let before = results.len();
        results.retain(|entry| match entry.url.as_deref() {
            Some(url) => domain_matches(url, domains),
            None => true,
        });
        if before != results.len() {
            Some("Filtered image results by domain list".to_string())
        } else {
            None
        }
    } else {
        None
    };

    results.truncate(max_results);
    Ok((results, warning))
}

pub(in crate::tools::web_run) fn page_from_search(query: &str, results: &[SearchEntry]) -> WebPage {
    let mut lines = Vec::new();
    let mut links = Vec::new();

    lines.push(format!("Search results for: {query}"));
    for (idx, entry) in results.iter().enumerate() {
        let id = idx + 1;
        links.push(WebLink {
            id,
            url: entry.url.clone(),
            text: entry.title.clone(),
        });
        lines.push(format!("{}. [{}] {}", id, id, entry.title));
        if let Some(snippet) = entry.snippet.as_ref()
            && !snippet.trim().is_empty()
        {
            lines.push(format!("    {snippet}"));
        }
        lines.push(format!("    {url}", url = entry.url));
    }

    WebPage {
        url: "https://html.duckduckgo.com/html/".to_string(),
        title: Some("Search Results".to_string()),
        content_type: Some("text/html".to_string()),
        lines,
        links,
        pdf_pages: None,
    }
}
