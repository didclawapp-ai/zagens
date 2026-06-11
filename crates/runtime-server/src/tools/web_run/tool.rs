//! web.run ToolSpec implementation.

use super::page::{find_in_page, render_view, resolve_or_fetch_page, screenshot_page};
use super::search::{page_from_search, run_image_search, run_search};
use super::state::{get_page, scoped_ref_prefix, store_page, with_state};
use super::types::{ImageQueryResult, ResponseLength, SearchResult, WebRunOutput};
use super::{DEFAULT_OPEN_TIMEOUT_MS, DEFAULT_TIMEOUT_MS, MAX_RESULTS};
use crate::tools::spec::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec,
    optional_u64, required_str,
};
use async_trait::async_trait;
use serde_json::{Value, json};

pub struct WebRunTool;

#[async_trait]
impl ToolSpec for WebRunTool {
    fn name(&self) -> &'static str {
        "web.run"
    }

    fn description(&self) -> &'static str {
        "Browse the web (search/open/click/find/screenshot/image_query) and return structured results with ref_ids for citations. \
        Use `open` after `search_query` to read full page text — search snippets alone are not enough for research answers."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "search_query": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "q": { "type": "string" },
                            "recency": { "type": "integer" },
                            "max_results": { "type": "integer" },
                            "timeout_ms": { "type": "integer" },
                            "domains": { "type": "array", "items": { "type": "string" } }
                        },
                        "required": ["q"]
                    }
                },
                "image_query": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "q": { "type": "string" },
                            "recency": { "type": "integer" },
                            "max_results": { "type": "integer" },
                            "timeout_ms": { "type": "integer" },
                            "domains": { "type": "array", "items": { "type": "string" } }
                        },
                        "required": ["q"]
                    }
                },
                "open": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "ref_id": { "type": "string" },
                            "lineno": { "type": "integer" }
                        },
                        "required": ["ref_id"]
                    }
                },
                "click": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "ref_id": { "type": "string" },
                            "id": { "type": "integer" }
                        },
                        "required": ["ref_id", "id"]
                    }
                },
                "find": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "ref_id": { "type": "string" },
                            "pattern": { "type": "string" }
                        },
                        "required": ["ref_id", "pattern"]
                    }
                },
                "screenshot": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "ref_id": { "type": "string" },
                            "pageno": { "type": "integer" }
                        },
                        "required": ["ref_id", "pageno"]
                    }
                },
                "response_length": {
                    "type": "string",
                    "enum": ["short", "medium", "long"],
                    "description": "Controls result verbosity"
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
        let response_length = ResponseLength::from_input(input.get("response_length"));
        let mut output = WebRunOutput::default();
        let scope = scoped_ref_prefix(&context.state_namespace);
        let turn = with_state(|state| state.next_turn(&context.state_namespace));

        let mut search_counter = 0usize;
        let mut view_counter = 0usize;
        let mut click_counter = 0usize;

        if let Some(searches) = input.get("search_query").and_then(|v| v.as_array()) {
            let mut results = Vec::new();
            for search in searches {
                let query = required_str(search, "q")?.trim().to_string();
                if query.is_empty() {
                    continue;
                }
                let recency = optional_u64(search, "recency", 0);
                let max_results = usize::try_from(optional_u64(
                    search,
                    "max_results",
                    response_length.max_results() as u64,
                ))
                .unwrap_or(response_length.max_results())
                .clamp(1, MAX_RESULTS);
                let timeout_ms = optional_u64(search, "timeout_ms", DEFAULT_TIMEOUT_MS).min(60_000);

                let domains = search
                    .get("domains")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();

                let (entries, source, warning) =
                    run_search(&query, max_results, timeout_ms, &domains, context).await?;
                let mut warnings = Vec::new();
                if recency > 0 {
                    warnings.push(format!(
                        "Recency filter not enforced (requested last {recency} days)"
                    ));
                }
                if let Some(w) = warning {
                    warnings.push(w);
                }
                search_counter += 1;
                let ref_id = format!("{scope}turn{turn}search{search_counter}");

                let page = page_from_search(&query, &entries);
                store_page(&context.state_namespace, &ref_id, page);

                results.push(SearchResult {
                    ref_id,
                    query,
                    source,
                    count: entries.len(),
                    results: entries,
                    warning: if warnings.is_empty() {
                        None
                    } else {
                        Some(warnings.join("; "))
                    },
                });
            }
            if !results.is_empty() {
                output.search_query = Some(results);
            }
        }

        if let Some(images) = input.get("image_query").and_then(|v| v.as_array()) {
            let mut results = Vec::new();
            for image in images {
                let query = required_str(image, "q")?.trim().to_string();
                if query.is_empty() {
                    continue;
                }
                let recency = optional_u64(image, "recency", 0);
                let max_results = usize::try_from(optional_u64(
                    image,
                    "max_results",
                    response_length.max_results() as u64,
                ))
                .unwrap_or(response_length.max_results())
                .clamp(1, MAX_RESULTS);
                let timeout_ms = optional_u64(image, "timeout_ms", DEFAULT_TIMEOUT_MS).min(60_000);

                let domains = image
                    .get("domains")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();

                let (entries, warning) =
                    run_image_search(&query, max_results, timeout_ms, &domains).await?;

                let mut warnings = Vec::new();
                if recency > 0 {
                    warnings.push(format!(
                        "Recency filter not enforced (requested last {recency} days)"
                    ));
                }
                if let Some(w) = warning {
                    warnings.push(w);
                }

                results.push(ImageQueryResult {
                    query,
                    source: "duckduckgo_images".to_string(),
                    count: entries.len(),
                    results: entries,
                    warning: if warnings.is_empty() {
                        None
                    } else {
                        Some(warnings.join("; "))
                    },
                });
            }
            if !results.is_empty() {
                output.image_query = Some(results);
            }
        }

        if let Some(opens) = input.get("open").and_then(|v| v.as_array()) {
            let mut views = Vec::new();
            for open in opens {
                let ref_id = required_str(open, "ref_id")?.to_string();
                let lineno = optional_u64(open, "lineno", 1).max(1) as usize;

                let page = resolve_or_fetch_page(&ref_id, DEFAULT_OPEN_TIMEOUT_MS, context).await?;
                view_counter += 1;
                let view_ref = format!("{scope}turn{turn}view{view_counter}");
                store_page(&context.state_namespace, &view_ref, page.clone());

                let view = render_view(&view_ref, &page, lineno, response_length);
                views.push(view);
            }
            if !views.is_empty() {
                output.open = Some(views);
            }
        }

        if let Some(clicks) = input.get("click").and_then(|v| v.as_array()) {
            let mut views = Vec::new();
            for click in clicks {
                let ref_id = required_str(click, "ref_id")?.to_string();
                let link_id = optional_u64(click, "id", 0) as usize;
                if link_id == 0 {
                    return Err(ToolError::invalid_input("click.id must be >= 1"));
                }
                let page = get_page(&ref_id).ok_or_else(|| {
                    ToolError::invalid_input(format!("Unknown ref_id '{ref_id}'"))
                })?;
                let link = page.links.iter().find(|l| l.id == link_id).ok_or_else(|| {
                    ToolError::invalid_input(format!(
                        "Link id {link_id} not found for ref_id '{ref_id}'"
                    ))
                })?;
                let target = link.url.clone();
                let fetched =
                    resolve_or_fetch_page(&target, DEFAULT_OPEN_TIMEOUT_MS, context).await?;
                click_counter += 1;
                let click_ref = format!("{scope}turn{turn}click{click_counter}");
                store_page(&context.state_namespace, &click_ref, fetched.clone());
                let view = render_view(&click_ref, &fetched, 1, response_length);
                views.push(view);
            }
            if !views.is_empty() {
                output.click = Some(views);
            }
        }

        if let Some(find_requests) = input.get("find").and_then(|v| v.as_array()) {
            let mut finds = Vec::new();
            for find_req in find_requests {
                let ref_id = required_str(find_req, "ref_id")?.to_string();
                let pattern = required_str(find_req, "pattern")?.to_string();
                let page = get_page(&ref_id).ok_or_else(|| {
                    ToolError::invalid_input(format!("Unknown ref_id '{ref_id}'"))
                })?;
                let find_result = find_in_page(&ref_id, &pattern, &page, response_length);
                finds.push(find_result);
            }
            if !finds.is_empty() {
                output.find = Some(finds);
            }
        }

        if let Some(shots) = input.get("screenshot").and_then(|v| v.as_array()) {
            let mut screenshots = Vec::new();
            for shot in shots {
                let ref_id = required_str(shot, "ref_id")?.to_string();
                let pageno = optional_u64(shot, "pageno", 0) as usize;
                let page = get_page(&ref_id).ok_or_else(|| {
                    ToolError::invalid_input(format!("Unknown ref_id '{ref_id}'"))
                })?;
                let screenshot = screenshot_page(&ref_id, pageno, &page)?;
                screenshots.push(screenshot);
            }
            if !screenshots.is_empty() {
                output.screenshot = Some(screenshots);
            }
        }

        ToolResult::json(&output).map_err(|e| ToolError::execution_failed(e.to_string()))
    }
}
