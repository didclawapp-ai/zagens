//! schemars input types for the web tool family (kernel-v2 M2).

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::tools::tool_schema::derived_input_schema;

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
struct WebSearchQueryEntry {
    pub q: Option<String>,
    pub query: Option<String>,
    pub max_results: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
struct WebSearchInput {
    #[schemars(description = "Search query. Compatibility aliases: q, or search_query[0].q.")]
    pub query: Option<String>,
    #[schemars(description = "Search query.")]
    pub q: Option<String>,
    #[schemars(
        description = "Array form for advanced queries: [{\"q\":\"...\", \"max_results\": 5}]"
    )]
    pub search_query: Option<Vec<WebSearchQueryEntry>>,
    #[schemars(description = "Maximum number of results to return (default: 8, max: 15)")]
    pub max_results: Option<u64>,
    #[schemars(description = "Timeout in milliseconds (default: 15000, max: 60000)")]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
enum FetchUrlFormatInput {
    #[serde(rename = "text")]
    Text,
    #[serde(rename = "markdown")]
    Markdown,
    #[serde(rename = "raw")]
    Raw,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
struct FetchUrlInput {
    #[schemars(description = "Absolute HTTP/HTTPS URL to fetch.")]
    pub url: String,
    #[schemars(
        description = "Post-processing for the response body. `markdown` (default) and `text` strip HTML tags to readable text; `raw` returns the body bytes as-is."
    )]
    pub format: Option<FetchUrlFormatInput>,
    #[schemars(
        description = "Truncate response body after this many bytes (default 1,000,000; hard max 10,485,760)."
    )]
    pub max_bytes: Option<u64>,
    #[schemars(description = "Request timeout in milliseconds (default 15,000; max 60,000).")]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
#[serde(deny_unknown_fields)]
struct FinanceInput {
    #[schemars(description = "Ticker symbol to look up (for example: AAPL, SPY, BTC).")]
    pub ticker: Option<String>,
    #[schemars(description = "Alias for ticker.")]
    pub symbol: Option<String>,
    #[schemars(description = "Optional asset type hint such as equity, fund, crypto, or index.")]
    #[serde(rename = "type")]
    pub asset_type: Option<String>,
    #[schemars(
        description = "Optional market hint retained for compatibility with finance-style tool calls."
    )]
    pub market: Option<String>,
    #[schemars(description = "Request timeout in milliseconds (default: 10000, max: 60000).")]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
struct WebRunQueryEntry {
    pub q: String,
    pub recency: Option<u64>,
    pub max_results: Option<u64>,
    pub timeout_ms: Option<u64>,
    pub domains: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
struct WebRunOpenEntry {
    pub ref_id: String,
    pub lineno: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
struct WebRunClickEntry {
    pub ref_id: String,
    pub id: u64,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
struct WebRunFindEntry {
    pub ref_id: String,
    pub pattern: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
struct WebRunScreenshotEntry {
    pub ref_id: String,
    pub pageno: u64,
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
enum WebRunResponseLengthInput {
    #[serde(rename = "short")]
    Short,
    #[serde(rename = "medium")]
    Medium,
    #[serde(rename = "long")]
    Long,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
struct WebRunInput {
    pub search_query: Option<Vec<WebRunQueryEntry>>,
    pub image_query: Option<Vec<WebRunQueryEntry>>,
    pub open: Option<Vec<WebRunOpenEntry>>,
    pub click: Option<Vec<WebRunClickEntry>>,
    pub find: Option<Vec<WebRunFindEntry>>,
    pub screenshot: Option<Vec<WebRunScreenshotEntry>>,
    #[schemars(description = "Controls result verbosity")]
    pub response_length: Option<WebRunResponseLengthInput>,
}

#[must_use]
pub fn web_search_input_schema() -> Value {
    derived_input_schema::<WebSearchInput>()
}

#[must_use]
pub fn fetch_url_input_schema() -> Value {
    derived_input_schema::<FetchUrlInput>()
}

#[must_use]
pub fn finance_input_schema() -> Value {
    let mut schema = derived_input_schema::<FinanceInput>();
    if let Value::Object(obj) = &mut schema {
        obj.insert(
            "anyOf".into(),
            json!([
                { "required": ["ticker"] },
                { "required": ["symbol"] }
            ]),
        );
        obj.insert("additionalProperties".into(), Value::Bool(false));
        reorder_finance_schema_root(obj);
    }
    schema
}

fn reorder_finance_schema_root(obj: &mut Map<String, Value>) {
    let old = std::mem::take(obj);
    for key in ["type", "properties", "anyOf", "additionalProperties"] {
        if let Some(val) = old.get(key) {
            obj.insert(key.to_string(), val.clone());
        }
    }
    for (key, val) in old {
        if !obj.contains_key(&key) {
            obj.insert(key, val);
        }
    }
}

#[must_use]
pub fn web_run_input_schema() -> Value {
    derived_input_schema::<WebRunInput>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::fetch_url::FetchUrlTool;
    use crate::tools::finance::FinanceTool;
    use crate::tools::schema_sanitize;
    use crate::tools::spec::ToolSpec;
    use crate::tools::web_run::WebRunTool;
    use crate::tools::web_search::WebSearchTool;

    fn model_visible_input_schema(tool: &dyn ToolSpec) -> Value {
        let mut schema = tool.input_schema();
        schema_sanitize::sanitize(&mut schema);
        schema
    }

    const WEB_SCHEMA_SNAPSHOT_DIR: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/harness/kernel-v2-schema-snapshots"
    );

    #[test]
    #[ignore = "bootstrap kernel-v2 web-tool schema snapshot fixtures"]
    fn dump_web_tool_schemas_for_snapshot_bootstrap() {
        let tools: [(&str, Box<dyn ToolSpec>); 4] = [
            ("web_search", Box::new(WebSearchTool)),
            ("fetch_url", Box::new(FetchUrlTool)),
            ("finance", Box::new(FinanceTool::new())),
            ("web.run", Box::new(WebRunTool)),
        ];
        for (name, tool) in tools {
            let schema = model_visible_input_schema(tool.as_ref());
            let pretty = serde_json::to_string_pretty(&schema).expect("serialize");
            println!("=== {name} ===\n{pretty}\n");
        }
    }

    #[test]
    fn web_tool_model_visible_schemas_match_snapshots() {
        let tools: [(&str, Box<dyn ToolSpec>); 4] = [
            ("web_search", Box::new(WebSearchTool)),
            ("fetch_url", Box::new(FetchUrlTool)),
            ("finance", Box::new(FinanceTool::new())),
            ("web.run", Box::new(WebRunTool)),
        ];
        for (name, tool) in tools {
            assert_eq!(tool.name(), name);
            let schema = model_visible_input_schema(tool.as_ref());
            let path = format!("{WEB_SCHEMA_SNAPSHOT_DIR}/web-{name}.json");
            let expected: Value = serde_json::from_str(
                &std::fs::read_to_string(&path)
                    .unwrap_or_else(|e| panic!("missing snapshot {path}: {e}")),
            )
            .expect("parse snapshot JSON");
            assert_eq!(
                schema, expected,
                "model-visible schema drift for {name} — update fixture only after explicit KV-cache review"
            );
        }
    }
}
