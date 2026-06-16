//! schemars input types for the file tool family (kernel-v2 M2).

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

use crate::tools::tool_schema::derived_input_schema;

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
pub struct ReadFileInput {
    #[schemars(description = "Path to the file (relative to workspace or absolute)")]
    pub path: String,
    #[schemars(
        description = "First line to read (1-based, default: 1). Preferred over \"offset\"."
    )]
    pub start_line: Option<u64>,
    #[schemars(
        description = "Alias for start_line (1-based, default: 1). Ignored when \"start_line\" is also set."
    )]
    pub offset: Option<u64>,
    #[schemars(description = "Maximum lines to read (default: 2000, max: 5000)")]
    pub limit: Option<u64>,
    #[schemars(
        description = "PDF only: page range to extract, e.g. \"1-5\" or \"10\". Ignored for non-PDF files."
    )]
    pub pages: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
pub struct WriteFileInput {
    #[schemars(description = "Path to the file")]
    pub path: String,
    #[schemars(description = "Content to write")]
    pub content: String,
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EditReplaceMode {
    First,
    All,
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EditOperation {
    SearchReplace,
    InsertAfter,
    DeleteLines,
    ReplaceLine,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
pub struct EditFileInput {
    #[schemars(description = "Path to the file")]
    pub path: String,
    #[schemars(description = "Text to search for")]
    pub search: Option<String>,
    #[schemars(
        description = "Replacement text (this field is named 'replace' — NOT 'new_str'/'new_string'/'replacement')."
    )]
    pub replace: Option<String>,
    #[schemars(
        description = "Limit the search range to start at this 1-based line (inclusive). Use with end_line for precision."
    )]
    pub start_line: Option<u64>,
    #[schemars(description = "Limit the search range to end at this 1-based line (inclusive).")]
    pub end_line: Option<u64>,
    #[schemars(
        description = "[search_replace mode] When there are multiple matches: 'first' replaces only the first, 'all' replaces all (requires explicit choice)."
    )]
    pub replace_mode: Option<EditReplaceMode>,
    #[schemars(
        description = "Edit operation. Default 'search_replace'. Other modes use line numbers instead of search strings."
    )]
    pub operation: Option<EditOperation>,
    #[schemars(
        description = "[insert_after / replace_line mode] The text to insert or use as replacement."
    )]
    pub text: Option<String>,
    #[schemars(
        description = "[insert_after mode] Insert text after this line number (1-based). 0 = at the beginning of the file."
    )]
    pub after_line: Option<u64>,
    #[schemars(description = "[replace_line mode] The line number to replace (1-based).")]
    pub line: Option<u64>,
    #[schemars(
        description = "[delete_lines mode] If true, preview what would be deleted without modifying the file. Returns the lines that would be removed."
    )]
    pub dry_run: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
pub struct ListDirInput {
    #[schemars(description = "Relative path (default: .)")]
    pub path: Option<String>,
    #[schemars(
        description = "Maximum entries to return (default: 1000, max: 10000). Directories are listed first, then files, each sorted by name."
    )]
    pub limit: Option<u64>,
    #[schemars(
        description = "Skip this many entries after sorting (default: 0). Use with limit to paginate large directories."
    )]
    pub offset: Option<u64>,
}

#[must_use]
pub fn read_file_input_schema() -> Value {
    derived_input_schema::<ReadFileInput>()
}

#[must_use]
pub fn write_file_input_schema() -> Value {
    derived_input_schema::<WriteFileInput>()
}

#[must_use]
pub fn edit_file_input_schema() -> Value {
    derived_input_schema::<EditFileInput>()
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
pub struct BatchEditInput {
    #[schemars(description = "Exact text to find in each matched file")]
    pub search: String,
    #[schemars(description = "Replacement text")]
    pub replace: String,
    #[schemars(
        description = "Glob pattern relative to path (e.g. '**/*.ts', 'src/**/*.go'). Max 32 files."
    )]
    pub glob: String,
    #[schemars(description = "Base directory for glob (default: workspace root '.')")]
    pub path: Option<String>,
    #[schemars(
        description = "Preview only — default true. Set false to write changes after reviewing dry-run output."
    )]
    pub dry_run: Option<bool>,
    #[schemars(
        description = "When multiple matches exist in one file: 'all' (default) or 'first'"
    )]
    pub replace_mode: Option<EditReplaceMode>,
    #[schemars(description = "Skip gitignored paths when walking (default: true)")]
    pub respect_gitignore: Option<bool>,
}

#[must_use]
pub fn batch_edit_input_schema() -> Value {
    derived_input_schema::<BatchEditInput>()
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
pub struct RefactorImportsInput {
    #[schemars(
        description = "Workspace-relative module path imports currently resolve to (e.g. 'shared/old')"
    )]
    pub from_target: String,
    #[schemars(description = "New workspace-relative module path to import instead")]
    pub to_target: String,
    #[schemars(description = "Glob for source files (e.g. '**/*.{ts,tsx,go}'). Max 32 files.")]
    pub glob: String,
    #[schemars(description = "Base directory for glob (default: workspace root '.')")]
    pub path: Option<String>,
    #[schemars(description = "Preview only — default true")]
    pub dry_run: Option<bool>,
    #[schemars(description = "Skip gitignored paths when walking (default: true)")]
    pub respect_gitignore: Option<bool>,
}

#[must_use]
pub fn refactor_imports_input_schema() -> Value {
    derived_input_schema::<RefactorImportsInput>()
}

#[must_use]
pub fn list_dir_input_schema() -> Value {
    derived_input_schema::<ListDirInput>()
}
