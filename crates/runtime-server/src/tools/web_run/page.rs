//! Fetch pages, render views, find text, screenshots.

use super::html::parse_html;
use super::state::get_page;
use super::types::{
    FindMatch, FindResult, PageViewResult, ResponseLength, ScreenshotResult, WebPage,
};
use super::USER_AGENT;
use crate::tools::spec::{ToolContext, ToolError};
use deepseek_runtime_adapters::tools::check_url_policy;

/// Hard cap on a fetched page/PDF body (C6). `web_run` has no per-call
/// `max_bytes`, so this bounds memory for an unbounded / huge response while
/// staying generous enough for typical HTML and moderate PDFs.
const MAX_PAGE_BYTES: usize = 25 * 1024 * 1024;

pub(in crate::tools::web_run) async fn resolve_or_fetch_page(
    ref_id: &str,
    timeout_ms: u64,
    context: &ToolContext,
) -> Result<WebPage, ToolError> {
    if let Some(page) = get_page(ref_id) {
        return Ok(page);
    }
    if looks_like_url(ref_id) {
        // SSRF (C3): fetch_page validates network policy + restricted IPs on
        // every redirect hop. (check_network_policy alone left web_run with no
        // IP blocking at all — only policy host matching.)
        return fetch_page(ref_id, timeout_ms, context).await;
    }
    Err(ToolError::invalid_input(format!(
        "Unknown ref_id '{ref_id}'"
    )))
}

pub(in crate::tools::web_run) fn looks_like_url(value: &str) -> bool {
    value.starts_with("http://") || value.starts_with("https://")
}
pub(in crate::tools::web_run) fn check_network_policy(url: &str, context: &ToolContext) -> Result<(), ToolError> {
    check_url_policy(context.network_policy.as_ref(), "web_run", url)
        .map_err(|e| ToolError::permission_denied(e.denial_message()))?;
    Ok(())
}

pub(in crate::tools::web_run) async fn fetch_page(
    url: &str,
    timeout_ms: u64,
    context: &ToolContext,
) -> Result<WebPage, ToolError> {
    // SSRF (C3): validate every hop's host (policy + restricted IPs), follow
    // redirects manually, pin the validated IP. Shared with fetch_url.
    let resp = crate::tools::ssrf::fetch_with_ssrf_guard(
        context,
        "web_run",
        url,
        USER_AGENT,
        "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
        timeout_ms,
    )
    .await?;

    let status = resp.status();
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    // C6: cap the buffered body so an unbounded response can't OOM us.
    let (bytes, _truncated) = crate::tools::ssrf::read_body_capped(
        resp,
        MAX_PAGE_BYTES,
        context.cancel_token.as_ref(),
    )
    .await?;

    if !status.is_success() {
        return Err(ToolError::execution_failed(format!(
            "Web request failed: HTTP {}",
            status.as_u16()
        )));
    }

    if is_pdf(&content_type, url) {
        return parse_pdf_page(url, content_type, &bytes);
    }

    let body = String::from_utf8_lossy(&bytes).to_string();
    let (lines, links, title) = parse_html(&body, url);

    Ok(WebPage {
        url: url.to_string(),
        title,
        content_type,
        lines,
        links,
        pdf_pages: None,
    })
}

pub(in crate::tools::web_run) fn is_pdf(content_type: &Option<String>, url: &str) -> bool {
    if let Some(ct) = content_type
        && ct.to_lowercase().contains("application/pdf")
    {
        return true;
    }
    url.to_lowercase().ends_with(".pdf")
}

pub(in crate::tools::web_run) fn parse_pdf_page(
    url: &str,
    content_type: Option<String>,
    bytes: &[u8],
) -> Result<WebPage, ToolError> {
    let text = pdf_extract_text(bytes)?;
    let pages = split_pdf_pages(&text);
    let lines = pages.first().cloned().unwrap_or_default();

    Ok(WebPage {
        url: url.to_string(),
        title: Some("PDF Document".to_string()),
        content_type,
        lines,
        links: Vec::new(),
        pdf_pages: Some(pages),
    })
}

pub(in crate::tools::web_run) fn pdf_extract_text(bytes: &[u8]) -> Result<String, ToolError> {
    pdf_extract::extract_text_from_mem(bytes)
        .map_err(|e| ToolError::execution_failed(format!("PDF extract failed: {e}")))
}

pub(in crate::tools::web_run) fn split_pdf_pages(text: &str) -> Vec<Vec<String>> {
    let raw_pages: Vec<&str> = text.split('\x0C').collect();
    raw_pages
        .iter()
        .map(|page| {
            page.lines()
                .map(|line| line.trim())
                .filter(|line| !line.is_empty())
                .map(|line| line.to_string())
                .collect::<Vec<_>>()
        })
        .collect()
}

pub(in crate::tools::web_run) fn render_view(
    ref_id: &str,
    page: &WebPage,
    lineno: usize,
    response: ResponseLength,
) -> PageViewResult {
    let total = page.lines.len();
    let view_lines = response.view_lines();
    let start = if total == 0 {
        1
    } else if lineno > total {
        total.saturating_sub(view_lines.saturating_sub(1)).max(1)
    } else {
        lineno
    };
    let end = if total == 0 {
        0
    } else {
        (start + view_lines - 1).min(total)
    };

    let content = if total == 0 {
        "(no content)".to_string()
    } else {
        render_lines(&page.lines, start, end)
    };

    PageViewResult {
        ref_id: ref_id.to_string(),
        url: page.url.clone(),
        title: page.title.clone(),
        content_type: page.content_type.clone(),
        line_start: start,
        line_end: end,
        total_lines: total,
        content,
        links: page.links.clone(),
    }
}

pub(in crate::tools::web_run) fn render_lines(lines: &[String], start: usize, end: usize) -> String {
    lines
        .iter()
        .enumerate()
        .filter_map(|(idx, line)| {
            let line_no = idx + 1;
            if line_no < start || line_no > end {
                return None;
            }
            Some(format!("{:>4} {}", line_no, line))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(in crate::tools::web_run) fn find_in_page(
    ref_id: &str,
    pattern: &str,
    page: &WebPage,
    response: ResponseLength,
) -> FindResult {
    let needle = pattern.to_lowercase();
    let mut matches = Vec::new();
    for (idx, line) in page.lines.iter().enumerate() {
        if line.to_lowercase().contains(&needle) {
            matches.push(FindMatch {
                line: idx + 1,
                text: line.clone(),
            });
        }
        if matches.len() >= response.max_find_matches() {
            break;
        }
    }

    FindResult {
        ref_id: ref_id.to_string(),
        pattern: pattern.to_string(),
        count: matches.len(),
        matches,
    }
}

pub(in crate::tools::web_run) fn screenshot_page(
    ref_id: &str,
    pageno: usize,
    page: &WebPage,
) -> Result<ScreenshotResult, ToolError> {
    let pages = page
        .pdf_pages
        .as_ref()
        .ok_or_else(|| ToolError::invalid_input("screenshot is only supported for PDF pages"))?;
    if pages.is_empty() {
        return Err(ToolError::execution_failed("PDF has no pages"));
    }
    if pageno >= pages.len() {
        return Err(ToolError::invalid_input(format!(
            "pageno {pageno} out of range (0..{max})",
            max = pages.len().saturating_sub(1)
        )));
    }
    let content = pages[pageno].join("\n");
    Ok(ScreenshotResult {
        ref_id: ref_id.to_string(),
        pageno,
        total_pages: pages.len(),
        content,
    })
}
