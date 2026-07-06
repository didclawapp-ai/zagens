//! schemars input types for the search tool family (kernel-v2 M2).

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

use crate::tools::tool_schema::derived_input_schema;

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
pub enum GrepOutputModeInput {
    #[serde(rename = "content")]
    Content,
    #[serde(rename = "files_with_matches")]
    FilesWithMatches,
    #[serde(rename = "count")]
    Count,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
pub struct GrepFilesInput {
    #[schemars(description = "Regular expression pattern to search for")]
    pub pattern: String,
    #[schemars(description = "Directory or file to search (relative to workspace, default: .)")]
    pub path: Option<String>,
    #[schemars(description = "Glob patterns for files to include (e.g., ['*.rs', '*.ts'])")]
    pub include: Option<Vec<String>>,
    #[schemars(
        description = "Glob patterns for files to exclude (e.g., ['*.min.js', 'node_modules/*'])"
    )]
    pub exclude: Option<Vec<String>>,
    #[schemars(description = "Number of context lines before and after each match (default: 2)")]
    pub context_lines: Option<u64>,
    #[schemars(description = "Whether to perform case-insensitive matching (default: false)")]
    pub case_insensitive: Option<bool>,
    #[schemars(description = "Maximum number of results to return (default: 100)")]
    pub max_results: Option<u64>,
    #[schemars(
        description = "Also query the symbol index for definitions matching the pattern (default: false). Symbol line numbers may drift for macro-expanded code."
    )]
    pub symbol_index: Option<bool>,
    #[schemars(
        description = "Filter symbol hits by kind, e.g. \"fn\", \"struct\", \"interface\", \"type\", \"enum\", \"const\", \"trait\", \"trait_fn\", \"impl_fn\", \"class\", \"method\"."
    )]
    pub symbol_kind: Option<String>,
    #[schemars(
        description = "When true (default), honor .gitignore / .ignore and parent ignore files; also skips target/, node_modules/, etc. Set false to search ignored paths (like rg -uuu)."
    )]
    pub respect_gitignore: Option<bool>,
    #[schemars(
        description = "content: matching lines with context (default). files_with_matches: file paths only (saves tokens). count: per-file match counts."
    )]
    pub output_mode: Option<GrepOutputModeInput>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
pub struct GlobFilesInput {
    #[schemars(
        description = "Glob pattern relative to path (e.g. '**/*.rs', 'src/**/*login*.tsx'). Use forward slashes."
    )]
    pub pattern: String,
    #[schemars(description = "Base directory (relative to workspace, default: .)")]
    pub path: Option<String>,
    #[schemars(description = "Maximum files to return (default: 100, max: 100)")]
    pub limit: Option<u64>,
    #[schemars(description = "Honor .gitignore when true (default: true)")]
    pub respect_gitignore: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
pub struct FileSearchInput {
    #[schemars(description = "Search query (file name or path fragment).")]
    pub query: String,
    #[schemars(description = "Optional base path to search (relative to workspace).")]
    pub path: Option<String>,
    #[schemars(description = "Maximum number of results to return (default: 20).")]
    pub limit: Option<u64>,
    #[schemars(
        description = "Optional list of file extensions to include (e.g. [\"rs\", \"md\"])."
    )]
    pub extensions: Option<Vec<String>>,
    #[schemars(
        description = "Also search the symbol index for definitions matching the query (default: false)."
    )]
    pub symbol_index: Option<bool>,
    #[schemars(description = "Optional symbol kind filter when symbol_index is true.")]
    pub symbol_kind: Option<String>,
    #[schemars(description = "Maximum symbol hits when symbol_index is true (default: 15).")]
    pub symbol_limit: Option<u64>,
    #[schemars(
        description = "Honor .gitignore when true (default: true). Set false to search ignored paths (like grep_files)."
    )]
    pub respect_gitignore: Option<bool>,
}

#[must_use]
pub fn grep_files_input_schema() -> Value {
    derived_input_schema::<GrepFilesInput>()
}

#[must_use]
pub fn glob_files_input_schema() -> Value {
    derived_input_schema::<GlobFilesInput>()
}

#[must_use]
pub fn file_search_input_schema() -> Value {
    derived_input_schema::<FileSearchInput>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::file_search::FileSearchTool;
    use crate::tools::glob_files::GlobFilesTool;
    use crate::tools::schema_sanitize;
    use crate::tools::search::GrepFilesTool;
    use crate::tools::spec::ToolSpec;

    fn model_visible_input_schema(tool: &dyn ToolSpec) -> Value {
        let mut schema = tool.input_schema();
        schema_sanitize::sanitize(&mut schema);
        schema
    }

    const SEARCH_SCHEMA_SNAPSHOT_DIR: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/harness/kernel-v2-schema-snapshots"
    );

    #[test]
    #[ignore = "bootstrap kernel-v2 search-tool schema snapshot fixtures"]
    fn dump_search_tool_schemas_for_snapshot_bootstrap() {
        let tools: [(&str, &dyn ToolSpec); 3] = [
            ("grep_files", &GrepFilesTool),
            ("glob_files", &GlobFilesTool),
            ("file_search", &FileSearchTool),
        ];
        for (name, tool) in tools {
            let schema = model_visible_input_schema(tool);
            let pretty = serde_json::to_string_pretty(&schema).expect("serialize");
            println!("=== {name} ===\n{pretty}\n");
        }
    }

    #[test]
    fn search_tool_model_visible_schemas_match_snapshots() {
        let tools: [(&str, &dyn ToolSpec); 3] = [
            ("grep_files", &GrepFilesTool),
            ("glob_files", &GlobFilesTool),
            ("file_search", &FileSearchTool),
        ];
        for (name, tool) in tools {
            assert_eq!(tool.name(), name);
            let schema = model_visible_input_schema(tool);
            let path = format!("{SEARCH_SCHEMA_SNAPSHOT_DIR}/search-{name}.json");
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
