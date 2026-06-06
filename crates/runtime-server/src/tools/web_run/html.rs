//! HTML parsing, regex helpers, and URL utilities.

use super::page::looks_like_url;
use super::types::{ResponseLength, SearchEntry, WebLink};
use base64::{Engine as _, engine::general_purpose};
use regex::Regex;
use std::sync::OnceLock;

static ANCHOR_RE: OnceLock<Regex> = OnceLock::new();
static TAG_RE: OnceLock<Regex> = OnceLock::new();
static BLOCK_RE: OnceLock<Regex> = OnceLock::new();
static SCRIPT_RE: OnceLock<Regex> = OnceLock::new();
static STYLE_RE: OnceLock<Regex> = OnceLock::new();
static TITLE_RE: OnceLock<Regex> = OnceLock::new();
static SNIPPET_RE: OnceLock<Regex> = OnceLock::new();
static SEARCH_TITLE_RE: OnceLock<Regex> = OnceLock::new();
static BING_RESULT_RE: OnceLock<Regex> = OnceLock::new();
static BING_TITLE_RE: OnceLock<Regex> = OnceLock::new();
static BING_SNIPPET_RE: OnceLock<Regex> = OnceLock::new();

pub(in crate::tools::web_run) fn get_anchor_re() -> &'static Regex {
    ANCHOR_RE.get_or_init(|| {
        Regex::new(r#"(?is)<a\s+[^>]*href\s*=\s*['\"]([^'\"]+)['\"][^>]*>(.*?)</a>"#)
            .expect("anchor regex")
    })
}

pub(in crate::tools::web_run) fn get_tag_re() -> &'static Regex {
    TAG_RE.get_or_init(|| Regex::new(r"<[^>]+>").expect("tag regex"))
}

pub(in crate::tools::web_run) fn get_block_re() -> &'static Regex {
    BLOCK_RE.get_or_init(|| {
        Regex::new(r"(?is)</?(p|div|li|ul|ol|br|h[1-6]|tr|td|th|table|section|article)[^>]*>")
            .expect("block regex")
    })
}

pub(in crate::tools::web_run) fn get_script_re() -> &'static Regex {
    SCRIPT_RE.get_or_init(|| Regex::new(r"(?is)<script[^>]*>.*?</script>").unwrap())
}

pub(in crate::tools::web_run) fn get_style_re() -> &'static Regex {
    STYLE_RE.get_or_init(|| Regex::new(r"(?is)<style[^>]*>.*?</style>").unwrap())
}

pub(in crate::tools::web_run) fn get_title_re() -> &'static Regex {
    TITLE_RE.get_or_init(|| Regex::new(r"(?is)<title[^>]*>(.*?)</title>").unwrap())
}

pub(in crate::tools::web_run) fn get_search_title_re() -> &'static Regex {
    SEARCH_TITLE_RE.get_or_init(|| {
        Regex::new(r#"<a[^>]*class=\"result__a\"[^>]*href=\"([^\"]+)\"[^>]*>(.*?)</a>"#)
            .expect("title regex pattern is valid")
    })
}

pub(in crate::tools::web_run) fn get_search_snippet_re() -> &'static Regex {
    SNIPPET_RE.get_or_init(|| {
        Regex::new(
            r#"<a[^>]*class=\"result__snippet\"[^>]*>(.*?)</a>|<div[^>]*class=\"result__snippet\"[^>]*>(.*?)</div>"#,
        )
        .expect("snippet regex pattern is valid")
    })
}

pub(in crate::tools::web_run) fn get_bing_result_re() -> &'static Regex {
    BING_RESULT_RE.get_or_init(|| {
        Regex::new(r#"(?is)<li[^>]*class=\"[^\"]*\bb_algo\b[^\"]*\"[^>]*>(.*?)</li>"#)
            .expect("bing result regex pattern is valid")
    })
}

pub(in crate::tools::web_run) fn get_bing_title_re() -> &'static Regex {
    BING_TITLE_RE.get_or_init(|| {
        Regex::new(r#"(?is)<h2[^>]*>.*?<a[^>]*href=\"([^\"]+)\"[^>]*>(.*?)</a>"#)
            .expect("bing title regex pattern is valid")
    })
}

pub(in crate::tools::web_run) fn get_bing_snippet_re() -> &'static Regex {
    BING_SNIPPET_RE.get_or_init(|| {
        Regex::new(r#"(?is)<div[^>]*class=\"[^\"]*\bb_caption\b[^\"]*\"[^>]*>.*?<p[^>]*>(.*?)</p>"#)
            .expect("bing snippet regex pattern is valid")
    })
}

pub(in crate::tools::web_run) fn parse_html(
    html: &str,
    base_url: &str,
) -> (Vec<String>, Vec<WebLink>, Option<String>) {
    let title = extract_title(html);
    let without_scripts = get_script_re().replace_all(html, "").to_string();
    let without_styles = get_style_re().replace_all(&without_scripts, "").to_string();

    let (with_links, links) = replace_links(&without_styles, base_url);
    let with_breaks = get_block_re().replace_all(&with_links, "\n").to_string();
    let without_tags = get_tag_re().replace_all(&with_breaks, "").to_string();
    let decoded = decode_html_entities(&without_tags);

    let mut lines = Vec::new();
    for line in decoded.lines() {
        let trimmed = normalize_whitespace(line);
        if trimmed.is_empty() {
            continue;
        }
        for wrapped in wrap_line(&trimmed, ResponseLength::Medium.wrap_width()) {
            lines.push(wrapped);
        }
    }

    (lines, links, title)
}

pub(in crate::tools::web_run) fn extract_title(html: &str) -> Option<String> {
    let re = get_title_re();
    let cap = re.captures(html)?;
    let raw = cap.get(1)?.as_str();
    let cleaned = normalize_whitespace(&decode_html_entities(raw));
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

pub(in crate::tools::web_run) fn replace_links(
    html: &str,
    base_url: &str,
) -> (String, Vec<WebLink>) {
    let re = get_anchor_re();
    let mut links = Vec::new();
    let mut output = String::with_capacity(html.len());
    let mut last = 0;

    for cap in re.captures_iter(html) {
        let Some(full) = cap.get(0) else { continue };
        let Some(href) = cap.get(1) else { continue };
        let Some(text_match) = cap.get(2) else {
            continue;
        };

        output.push_str(&html[last..full.start()]);
        let text = normalize_whitespace(&strip_tags(text_match.as_str()));
        let resolved = resolve_url(base_url, href.as_str());
        if !text.is_empty() {
            let id = links.len() + 1;
            links.push(WebLink {
                id,
                url: resolved.clone(),
                text: text.clone(),
            });
            output.push_str(&format!("[{}] {}", id, text));
        } else {
            output.push_str(&resolved);
        }
        last = full.end();
    }

    output.push_str(&html[last..]);
    (output, links)
}

pub(in crate::tools::web_run) fn resolve_url(base: &str, href: &str) -> String {
    if href.starts_with("http://") || href.starts_with("https://") {
        return href.to_string();
    }
    if href.starts_with("//") {
        return format!("https:{href}");
    }
    if let Ok(base_url) = reqwest::Url::parse(base)
        && let Ok(joined) = base_url.join(href)
    {
        return joined.to_string();
    }
    href.to_string()
}

pub(in crate::tools::web_run) fn strip_tags(text: &str) -> String {
    get_tag_re().replace_all(text, "").to_string()
}

pub(in crate::tools::web_run) fn normalize_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(in crate::tools::web_run) fn wrap_line(text: &str, width: usize) -> Vec<String> {
    if text.len() <= width {
        return vec![text.to_string()];
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if current.is_empty() {
            current.push_str(word);
        } else if current.len() + word.len() < width {
            current.push(' ');
            current.push_str(word);
        } else {
            lines.push(current);
            current = word.to_string();
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

pub(in crate::tools::web_run) fn decode_html_entities(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
}

pub(in crate::tools::web_run) fn parse_duckduckgo_results(
    html: &str,
    max_results: usize,
) -> Vec<SearchEntry> {
    let title_re = get_search_title_re();
    let snippet_re = get_search_snippet_re();
    let snippets: Vec<String> = snippet_re
        .captures_iter(html)
        .filter_map(|cap| cap.get(1).or_else(|| cap.get(2)))
        .map(|m| normalize_whitespace(&decode_html_entities(&strip_tags(m.as_str()))))
        .collect();

    let mut results = Vec::new();
    for (idx, cap) in title_re.captures_iter(html).enumerate() {
        if results.len() >= max_results {
            break;
        }
        let href = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let title_raw = cap.get(2).map(|m| m.as_str()).unwrap_or("");
        let title = normalize_whitespace(&decode_html_entities(&strip_tags(title_raw)));
        if title.is_empty() {
            continue;
        }
        let url = normalize_search_url(href);
        let snippet = snippets
            .get(idx)
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty());

        results.push(SearchEntry {
            title,
            url,
            snippet,
        });
    }

    results
}

pub(in crate::tools::web_run) fn is_duckduckgo_challenge(html: &str) -> bool {
    html.contains("anomaly-modal") || html.contains("Unfortunately, bots use DuckDuckGo too")
}

pub(in crate::tools::web_run) fn parse_bing_results(
    html: &str,
    max_results: usize,
) -> Vec<SearchEntry> {
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
        let title = normalize_whitespace(&decode_html_entities(&strip_tags(title_raw)));
        if title.is_empty() {
            continue;
        }
        let snippet = get_bing_snippet_re()
            .captures(block)
            .and_then(|snippet_cap| snippet_cap.get(1))
            .map(|m| normalize_whitespace(&decode_html_entities(&strip_tags(m.as_str()))))
            .filter(|s| !s.is_empty());

        results.push(SearchEntry {
            title,
            url: normalize_bing_url(href),
            snippet,
        });
    }

    results
}

pub(in crate::tools::web_run) fn normalize_search_url(href: &str) -> String {
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

pub(in crate::tools::web_run) fn normalize_bing_url(href: &str) -> String {
    if let Some(encoded) = extract_query_param(href, "u") {
        let decoded = percent_decode(&encoded);
        let token = decoded.strip_prefix("a1").unwrap_or(&decoded);
        let mut padded = token.replace('-', "+").replace('_', "/");
        while !padded.len().is_multiple_of(4) {
            padded.push('=');
        }
        if let Ok(bytes) = general_purpose::STANDARD.decode(padded)
            && let Ok(url) = String::from_utf8(bytes)
            && looks_like_url(&url)
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

pub(in crate::tools::web_run) fn extract_query_param(url: &str, key: &str) -> Option<String> {
    let query_start = url.find('?')?;
    let query = &url[query_start + 1..];
    for part in query.split('&') {
        let (k, v) = part.split_once('=')?;
        if k == key {
            return Some(v.to_string());
        }
    }
    None
}

pub(in crate::tools::web_run) fn percent_decode(input: &str) -> String {
    let mut out = Vec::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut idx = 0;
    while idx < bytes.len() {
        if bytes[idx] == b'%'
            && idx + 2 < bytes.len()
            && let Ok(hex) = std::str::from_utf8(&bytes[idx + 1..idx + 3])
            && let Ok(val) = u8::from_str_radix(hex, 16)
        {
            out.push(val);
            idx += 3;
            continue;
        }
        out.push(bytes[idx]);
        idx += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub(in crate::tools::web_run) fn url_encode(input: &str) -> String {
    crate::utils::url_encode(input)
}
