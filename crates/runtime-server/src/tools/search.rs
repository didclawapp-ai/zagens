//! Search tools: `grep_files` for code search
//!
//! Workspace traversal uses the same `ignore` stack as ripgrep (`WalkBuilder`):
//! `.gitignore` / `.ignore` by default, skips common vendor/build dirs, and
//! skips binary files (NUL sniff). Results are BM25-ranked for agent consumption.

use super::spec::{
    ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec, optional_bool, optional_str,
    optional_u64, required_str,
};
use super::workspace_walk::{collect_workspace_files, is_probably_binary};
use async_trait::async_trait;
use deepseek_config::workspace_meta_file_read;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// Wall-clock cap for the blocking file scan (C6 / audit §3 P0).
const GREP_TIMEOUT_SECS: u64 = 120;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GrepOutputMode {
    Content,
    FilesWithMatches,
    Count,
}

impl GrepOutputMode {
    fn parse(raw: &str) -> Result<Self, ToolError> {
        match raw {
            "content" => Ok(Self::Content),
            "files_with_matches" => Ok(Self::FilesWithMatches),
            "count" => Ok(Self::Count),
            other => Err(ToolError::invalid_input(format!(
                "output_mode must be content, files_with_matches, or count (got {other:?})"
            ))),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Content => "content",
            Self::FilesWithMatches => "files_with_matches",
            Self::Count => "count",
        }
    }
}
/// Maximum number of results to return to avoid overwhelming output
const MAX_RESULTS: usize = 100;

/// Maximum file size to search (skip very large files)
const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024; // 10MB

/// Fallback excludes when `respect_gitignore` is false (ripgrep `-uuu` style).
const DEFAULT_EXCLUDE_PATTERNS: &[&str] = &[
    "node_modules/**",
    ".git/**",
    "target/**",
    "*.min.js",
    "*.min.css",
    "dist/**",
    "build/**",
    "__pycache__/**",
    ".venv/**",
    "venv/**",
];

/// Result of a grep match
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrepMatch {
    pub file: String,
    pub line_number: usize,
    pub line: String,
    pub context_before: Vec<String>,
    pub context_after: Vec<String>,
}

/// Tool for searching files using regex patterns
pub struct GrepFilesTool;

#[async_trait]
impl ToolSpec for GrepFilesTool {
    fn name(&self) -> &'static str {
        "grep_files"
    }

    fn description(&self) -> &'static str {
        "Search file contents by regex (ripgrep-style walk, respects .gitignore). output_mode: content (lines+context), files_with_matches (paths only), count (per-file hits). NEVER use exec_shell with grep/rg — use this tool."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Regular expression pattern to search for"
                },
                "path": {
                    "type": "string",
                    "description": "Directory or file to search (relative to workspace, default: .)"
                },
                "include": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Glob patterns for files to include (e.g., ['*.rs', '*.ts'])"
                },
                "exclude": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Glob patterns for files to exclude (e.g., ['*.min.js', 'node_modules/*'])"
                },
                "context_lines": {
                    "type": "integer",
                    "description": "Number of context lines before and after each match (default: 2)"
                },
                "case_insensitive": {
                    "type": "boolean",
                    "description": "Whether to perform case-insensitive matching (default: false)"
                },
                "max_results": {
                    "type": "integer",
                    "description": "Maximum number of results to return (default: 100)"
                },
                "symbol_index": {
                    "type": "boolean",
                    "description": "Also query the symbol index for definitions matching the pattern (default: false). Symbol line numbers may drift for macro-expanded code."
                },
                "symbol_kind": {
                    "type": "string",
                    "description": "Filter symbol hits by kind, e.g. \"fn\", \"struct\", \"interface\", \"type\", \"enum\", \"const\", \"trait\", \"trait_fn\", \"impl_fn\", \"class\", \"method\"."
                },
                "respect_gitignore": {
                    "type": "boolean",
                    "description": "When true (default), honor .gitignore / .ignore and parent ignore files; also skips target/, node_modules/, etc. Set false to search ignored paths (like rg -uuu)."
                },
                "output_mode": {
                    "type": "string",
                    "enum": ["content", "files_with_matches", "count"],
                    "description": "content: matching lines with context (default). files_with_matches: file paths only (saves tokens). count: per-file match counts."
                }
            },
            "required": ["pattern"]
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::ReadOnly, ToolCapability::Sandboxable]
    }

    fn supports_parallel(&self) -> bool {
        true
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let pattern_str = required_str(&input, "pattern")?;
        let path_str = optional_str(&input, "path").unwrap_or(".");
        let context_lines = usize::try_from(optional_u64(&input, "context_lines", 2))
            .unwrap_or(2)
            .min(20);
        let case_insensitive = optional_bool(&input, "case_insensitive", false);
        let symbol_index_enabled = optional_bool(&input, "symbol_index", false);
        let symbol_kind = optional_str(&input, "symbol_kind").map(|s| s.to_string());
        let respect_gitignore = optional_bool(&input, "respect_gitignore", true);
        let output_mode = match optional_str(&input, "output_mode") {
            Some(mode) => GrepOutputMode::parse(mode)?,
            None => GrepOutputMode::Content,
        };
        let max_results = usize::try_from(optional_u64(&input, "max_results", MAX_RESULTS as u64))
            .unwrap_or(MAX_RESULTS);

        // Parse include patterns
        let include_patterns: Vec<String> = input
            .get("include")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        // Parse exclude patterns (supplement gitignore when respect_gitignore is true).
        let exclude_patterns: Vec<String> = input
            .get("exclude")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_else(|| {
                if respect_gitignore {
                    Vec::new()
                } else {
                    DEFAULT_EXCLUDE_PATTERNS
                        .iter()
                        .map(|s| (*s).to_string())
                        .collect()
                }
            });

        // Build regex
        let regex_pattern = if case_insensitive {
            format!("(?i){pattern_str}")
        } else {
            pattern_str.to_string()
        };

        let regex = Regex::new(&regex_pattern)
            .map_err(|e| ToolError::invalid_input(format!("Invalid regex pattern: {e}")))?;

        // Resolve search path
        let search_path = context.resolve_path(path_str)?;

        let scan_params = GrepScanParams {
            search_path,
            workspace: context.workspace.clone(),
            include_patterns,
            exclude_patterns,
            respect_gitignore,
            context_lines,
            max_results,
            output_mode,
            regex,
            cancel: context.cancel_token.clone(),
        };

        let scan = match tokio::time::timeout(
            Duration::from_secs(GREP_TIMEOUT_SECS),
            tokio::task::spawn_blocking(move || grep_file_scan(scan_params)),
        )
        .await
        {
            Ok(Ok(Ok(out))) => out,
            Ok(Ok(Err(e))) => return Err(e),
            Ok(Err(e)) => {
                return Err(ToolError::execution_failed(format!(
                    "grep_files worker failed: {e}"
                )));
            }
            Err(_) => {
                return Err(ToolError::execution_failed(format!(
                    "grep_files exceeded the {GREP_TIMEOUT_SECS}s timeout"
                )));
            }
        };

        let mut results = scan.results;
        let file_match_counts = scan.file_match_counts;
        let files_searched = scan.files_searched;
        let files_skipped_binary = scan.files_skipped_binary;
        let files_skipped_io = scan.files_skipped_io;
        let files_skipped_size = scan.files_skipped_size;
        let total_matches = scan.total_matches;

        if output_mode == GrepOutputMode::Content {
            // BM25 re-rank: order files by relevance so the model sees the
            // most likely definitions / usages first, not filesystem order.
            bm25_rank(&mut results, &pattern_str);
        }

        // Symbol index maintenance: trigger a background rebuild when the
        // index is missing or stale (schema bump, new TS support, etc.).
        // This runs unconditionally — `symbol_index` only controls whether
        // query results are included in the response.
        crate::symbol_index::ensure_symbol_index(&context.workspace);

        // Symbol index lookup: if enabled and the pattern matches a known
        // symbol, prepend file:line references so the model can jump to
        // definitions. Disabled by default — enable with `symbol_index: true`
        // when grep is used for definition search rather than call-site audit.
        let (symbol_hits, symbol_status) = if symbol_index_enabled {
            let hits = lookup_symbol_hits(&context.workspace, &pattern_str, symbol_kind.as_deref());
            let status = crate::symbol_index::index_status(&context.workspace);

            // Boost index hits: lines that match the symbol index float to the top
            // of `matches` while preserving BM25 order among peers (stable).
            if output_mode == GrepOutputMode::Content {
                boost_index_hits(&mut results, &hits);
            }
            (hits, status)
        } else {
            (
                Vec::new(),
                crate::symbol_index::index_status(&context.workspace),
            )
        };

        // Build result
        let mut extra = serde_json::Map::new();
        if !symbol_hits.is_empty() {
            extra.insert(
                "symbol_index_note".into(),
                serde_json::Value::String(
                    "Line numbers from syn spans; may drift for macro-expanded code.".into(),
                ),
            );
            // V3-4: human-readable summary (max 3 entries)
            let summary_parts: Vec<String> = symbol_hits
                .iter()
                .take(3)
                .filter_map(|h| {
                    let sym = h.get("symbol")?.as_str()?;
                    let file = h.get("file")?.as_str()?;
                    let line = h.get("line")?.as_u64()?;
                    Some(format!("{sym} -> {file}:{line}"))
                })
                .collect();
            if !summary_parts.is_empty() {
                let mut s = format!("Symbol index: {}", summary_parts.join(", "));
                if symbol_hits.len() > 3 {
                    s.push_str(&format!(" ... and {} more", symbol_hits.len() - 3));
                }
                extra.insert("symbol_index_summary".into(), serde_json::Value::String(s));
            }
            // When symbol_index_hits include impl_fn (derive-expanded code),
            // warn that line numbers may be off by 5-20 lines.
            if symbol_hits
                .iter()
                .any(|h| h.get("symbol") == Some(&json!("impl_fn")))
            {
                extra.insert(
                    "symbol_index_warning".into(),
                    json!("Some hits are 'impl_fn' — these come from #[derive] expansions and line numbers may be off by 5-20 lines. Use read_file with a wider range.")
                );
            }

            // Attach per-file symbol summaries for the hit files (max 3)
            // so the model doesn't need a follow-up read_file for context.
            let hit_files: std::collections::BTreeSet<&str> = symbol_hits
                .iter()
                .filter_map(|h| h.get("file")?.as_str())
                .collect();
            if hit_files.len() <= 3 {
                // Load the index once (already called ensure_symbol_index above).
                let idx_path = workspace_meta_file_read(&context.workspace, "symbols.json");
                if let Ok(raw) = std::fs::read_to_string(&idx_path) {
                    if let Ok(idx) = serde_json::from_str::<crate::symbol_index::SymbolIndex>(&raw)
                    {
                        let mut file_summaries = serde_json::Map::new();
                        for file in hit_files {
                            if let Some(fs) = idx.files.get(file) {
                                let syms: Vec<serde_json::Value> = fs
                                    .symbols
                                    .iter()
                                    .map(
                                        |s| json!({"name": s.name, "kind": s.kind, "line": s.line}),
                                    )
                                    .collect();
                                file_summaries.insert(file.to_string(), json!({"symbols": syms}));
                            }
                        }
                        if !file_summaries.is_empty() {
                            extra.insert(
                                "symbol_index_file_summaries".into(),
                                serde_json::Value::Object(file_summaries),
                            );
                        }
                    }
                }
            }
        }
        // Attach Tauri bridge pairs when the pattern matches a command name.
        if symbol_index_enabled {
            let idx_path = workspace_meta_file_read(&context.workspace, "symbols.json");
            if let Ok(raw) = std::fs::read_to_string(&idx_path) {
                if let Ok(idx) = serde_json::from_str::<crate::symbol_index::SymbolIndex>(&raw) {
                    let cleaned = pattern_str.replace(
                        [
                            '.', '*', '+', '?', '(', ')', '[', ']', '{', '}', '^', '$', '|', '\\',
                        ],
                        " ",
                    );
                    let terms: Vec<&str> = cleaned.split_whitespace().collect();
                    let matched_pairs: Vec<serde_json::Value> = idx
                        .bridge_pairs
                        .iter()
                        .filter(|bp| {
                            terms
                                .iter()
                                .any(|t| bp.command.to_lowercase().contains(&t.to_lowercase()))
                        })
                        .map(|bp| {
                            json!({
                                "command": bp.command,
                                "rust": {"file": bp.rust_file, "line": bp.rust_line},
                                "ts": {"file": bp.ts_file, "line": bp.ts_line},
                            })
                        })
                        .collect();
                    if !matched_pairs.is_empty() {
                        extra.insert(
                            "symbol_index_bridge_pairs".into(),
                            serde_json::Value::Array(matched_pairs),
                        );
                    }

                    // Reverse call graph: who calls the queried symbol?
                    for term in &terms {
                        let callers: Vec<serde_json::Value> =
                            crate::symbol_index::query_callers(&idx, term)
                                .into_iter()
                                .map(|c| {
                                    json!({
                                        "name": c.name,
                                        "file": c.file,
                                        "line": c.line,
                                        "kind": c.kind,
                                    })
                                })
                                .collect();
                        if !callers.is_empty() {
                            extra.insert(
                                "symbol_index_callers".into(),
                                serde_json::Value::Array(callers),
                            );
                            break;
                        }
                    }

                    // C/C++ hits may drift due to macros/templates.
                    let has_cpp = symbol_hits.iter().any(|h| {
                        h.get("file").and_then(|f| f.as_str()).is_some_and(|f| {
                            f.ends_with(".c")
                                || f.ends_with(".h")
                                || f.ends_with(".cpp")
                                || f.ends_with(".cc")
                                || f.ends_with(".cxx")
                                || f.ends_with(".hpp")
                                || f.ends_with(".hxx")
                                || f.ends_with(".hh")
                        })
                    });
                    if has_cpp {
                        extra.insert(
                            "symbol_index_cpp_note".into(),
                            json!("C/C++ line numbers are regex-based and may drift for macro-expanded or templated code. Prefer read_file with a wider range when lines look off."),
                        );
                    }
                }
            }
        }
        let mut file_counts_json: Vec<Value> = file_match_counts
            .iter()
            .map(|(file, count)| json!({ "file": file, "match_count": count }))
            .collect();
        file_counts_json.sort_by(|a, b| {
            let ca = a["match_count"].as_u64().unwrap_or(0);
            let cb = b["match_count"].as_u64().unwrap_or(0);
            cb.cmp(&ca).then_with(|| {
                a["file"]
                    .as_str()
                    .unwrap_or_default()
                    .cmp(b["file"].as_str().unwrap_or_default())
            })
        });
        let file_counts_truncated = file_counts_json.len() > max_results;
        if file_counts_truncated {
            file_counts_json.truncate(max_results);
        }

        let mut files_with_matches: Vec<String> = file_match_counts.keys().cloned().collect();
        files_with_matches.sort_by(|a, b| {
            let ca = file_match_counts.get(a).copied().unwrap_or(0);
            let cb = file_match_counts.get(b).copied().unwrap_or(0);
            cb.cmp(&ca).then_with(|| a.cmp(b))
        });
        let files_truncated = files_with_matches.len() > max_results;
        if files_truncated {
            files_with_matches.truncate(max_results);
        }

        let truncated = match output_mode {
            GrepOutputMode::Content => total_matches > max_results || results.len() >= max_results,
            GrepOutputMode::FilesWithMatches => {
                files_truncated || file_match_counts.len() > max_results
            }
            GrepOutputMode::Count => file_counts_truncated || file_match_counts.len() > max_results,
        };

        let mut result_map = serde_json::Map::from_iter(vec![
            ("output_mode".into(), json!(output_mode.as_str())),
            ("total_matches".into(), serde_json::json!(total_matches)),
            ("files_searched".into(), serde_json::json!(files_searched)),
            (
                "files_skipped_binary".into(),
                serde_json::json!(files_skipped_binary),
            ),
            (
                "files_skipped_io".into(),
                serde_json::json!(files_skipped_io),
            ),
            (
                "files_skipped_size".into(),
                serde_json::json!(files_skipped_size),
            ),
            (
                "respect_gitignore".into(),
                serde_json::json!(respect_gitignore),
            ),
            ("truncated".into(), serde_json::json!(truncated)),
            ("symbol_index_hits".into(), serde_json::json!(symbol_hits)),
            (
                "symbol_index_status".into(),
                serde_json::json!(symbol_status),
            ),
        ]);

        match output_mode {
            GrepOutputMode::Content => {
                result_map.insert("matches".into(), serde_json::json!(results));
            }
            GrepOutputMode::FilesWithMatches => {
                result_map.insert("files".into(), serde_json::json!(files_with_matches));
            }
            GrepOutputMode::Count => {
                result_map.insert("file_counts".into(), Value::Array(file_counts_json));
            }
        }
        result_map.extend(extra);
        let result = serde_json::Value::Object(result_map);

        ToolResult::json(&result).map_err(|e| ToolError::execution_failed(e.to_string()))
    }
}

struct GrepScanParams {
    search_path: PathBuf,
    workspace: PathBuf,
    include_patterns: Vec<String>,
    exclude_patterns: Vec<String>,
    respect_gitignore: bool,
    context_lines: usize,
    max_results: usize,
    output_mode: GrepOutputMode,
    regex: Regex,
    cancel: Option<CancellationToken>,
}

struct GrepFileScanOutput {
    results: Vec<GrepMatch>,
    file_match_counts: HashMap<String, usize>,
    files_searched: u64,
    files_skipped_binary: u64,
    files_skipped_io: u64,
    files_skipped_size: u64,
    total_matches: usize,
}

/// Blocking file walk + scan — runs on the blocking pool so large monorepos
/// don't stall tokio workers (C4/C6).
fn grep_file_scan(params: GrepScanParams) -> Result<GrepFileScanOutput, ToolError> {
    let GrepScanParams {
        search_path,
        workspace,
        include_patterns,
        exclude_patterns,
        respect_gitignore,
        context_lines,
        max_results,
        output_mode,
        regex,
        cancel,
    } = params;

    let files = collect_files(
        &search_path,
        &include_patterns,
        &exclude_patterns,
        respect_gitignore,
    )?;

    let mut results: Vec<GrepMatch> = Vec::new();
    let mut file_match_counts: HashMap<String, usize> = HashMap::new();
    let mut files_searched = 0;
    let mut files_skipped_binary = 0u64;
    let mut files_skipped_io = 0u64;
    let mut files_skipped_size = 0u64;
    let mut total_matches = 0;

    'files: for file_path in files {
        if cancel.as_ref().is_some_and(CancellationToken::is_cancelled) {
            break 'files;
        }
        if output_mode == GrepOutputMode::Content && results.len() >= max_results {
            break;
        }
        if output_mode == GrepOutputMode::FilesWithMatches && file_match_counts.len() >= max_results
        {
            break;
        }

        if let Ok(metadata) = fs::metadata(&file_path) {
            if metadata.len() > MAX_FILE_SIZE {
                files_skipped_size += 1;
                continue;
            }
        }

        if is_probably_binary(&file_path) {
            files_skipped_binary += 1;
            continue;
        }

        let Ok(raw_bytes) = fs::read(&file_path) else {
            files_skipped_io += 1;
            continue;
        };
        let (file_content, _enc, _via) = super::file::detect_and_decode(&raw_bytes);

        files_searched += 1;
        // Strip trailing CR so CRLF files match patterns written for LF (Windows).
        let lines: Vec<String> = file_content
            .lines()
            .map(|line| line.trim_end_matches('\r').to_string())
            .collect();
        let relative_path = file_path
            .strip_prefix(&workspace)
            .unwrap_or(&file_path)
            .to_string_lossy()
            .to_string();
        let mut hits_in_file = 0usize;

        for (line_idx, line) in lines.iter().enumerate() {
            if !regex.is_match(line) {
                continue;
            }
            total_matches += 1;
            hits_in_file += 1;

            if output_mode == GrepOutputMode::FilesWithMatches {
                break;
            }

            if output_mode == GrepOutputMode::Content {
                let context_before: Vec<String> = (line_idx.saturating_sub(context_lines)
                    ..line_idx)
                    .filter_map(|i| lines.get(i).cloned())
                    .collect();

                let context_after: Vec<String> = ((line_idx + 1)
                    ..=(line_idx + context_lines).min(lines.len() - 1))
                    .filter_map(|i| lines.get(i).cloned())
                    .collect();

                results.push(GrepMatch {
                    file: relative_path.clone(),
                    line_number: line_idx + 1,
                    line: line.clone(),
                    context_before,
                    context_after,
                });

                if results.len() >= max_results {
                    break;
                }
            }
        }

        if hits_in_file > 0 {
            file_match_counts.insert(relative_path, hits_in_file);
            if output_mode == GrepOutputMode::FilesWithMatches
                && file_match_counts.len() >= max_results
            {
                break 'files;
            }
        }
    }

    Ok(GrepFileScanOutput {
        results,
        file_match_counts,
        files_searched,
        files_skipped_binary,
        files_skipped_io,
        files_skipped_size,
        total_matches,
    })
}

/// Collect files to search: workspace walk + optional include/exclude globs.
fn collect_files(
    search_path: &Path,
    include_patterns: &[String],
    exclude_patterns: &[String],
    respect_gitignore: bool,
) -> Result<Vec<PathBuf>, ToolError> {
    if search_path.is_file() {
        return Ok(vec![search_path.to_path_buf()]);
    }

    if !search_path.exists() {
        return Err(ToolError::invalid_input(format!(
            "Search path does not exist: {}",
            search_path.display()
        )));
    }

    if !search_path.is_dir() {
        return Err(ToolError::invalid_input(format!(
            "Search path is not a file or directory: {}",
            search_path.display()
        )));
    }

    let mut files = collect_workspace_files(search_path, respect_gitignore);
    files.retain(|path| {
        let relative = path.strip_prefix(search_path).unwrap_or(path);
        // Normalize Windows `\` to `/` so include/exclude globs (written with
        // `/`, e.g. `src/**/*.rs`) match on Windows too — matches_glob splits
        // on `/`. (glob_files already does this.)
        let relative_str = relative.to_string_lossy().replace('\\', "/");
        if should_exclude(&relative_str, exclude_patterns) {
            return false;
        }
        include_patterns.is_empty() || should_include(&relative_str, include_patterns)
    });

    Ok(files)
}

/// Check if a path matches any of the exclude patterns
fn should_exclude(path: &str, patterns: &[String]) -> bool {
    for pattern in patterns {
        if matches_glob(path, pattern) {
            return true;
        }
    }
    false
}

/// Check if a path matches any of the include patterns
fn should_include(path: &str, patterns: &[String]) -> bool {
    for pattern in patterns {
        if matches_glob(path, pattern) {
            return true;
        }
    }
    false
}

/// Simple glob pattern matching
/// Supports: * (any chars), ** (any path), ? (single char)
fn matches_glob(path: &str, pattern: &str) -> bool {
    // Handle ** for any path
    if pattern.contains("**") {
        let parts: Vec<&str> = pattern.split("**").collect();
        if parts.len() == 2 {
            let prefix = parts[0].trim_end_matches('/');
            let suffix = parts[1].trim_start_matches('/');

            if !prefix.is_empty() && !path.starts_with(prefix) {
                return false;
            }
            if !suffix.is_empty() {
                return path.ends_with(suffix)
                    || path
                        .split('/')
                        .any(|part| matches_simple_glob(part, suffix));
            }
            return path.starts_with(prefix) || prefix.is_empty();
        }
    }

    // Handle patterns like "*.rs" - match against filename only
    if pattern.starts_with('*') && !pattern.contains('/') {
        let filename = path.rsplit('/').next().unwrap_or(path);
        return matches_simple_glob(filename, pattern);
    }

    // Handle patterns with path components
    if pattern.contains('/') {
        return matches_simple_glob(path, pattern);
    }

    // Match against filename
    let filename = path.rsplit('/').next().unwrap_or(path);
    matches_simple_glob(filename, pattern)
}

/// Simple glob matching for single path component
fn matches_simple_glob(text: &str, pattern: &str) -> bool {
    let mut text_chars = text.chars().peekable();
    let mut pattern_chars = pattern.chars().peekable();

    while let Some(p) = pattern_chars.next() {
        match p {
            '*' => {
                // Match zero or more characters
                let next_pattern: String = pattern_chars.collect();
                if next_pattern.is_empty() {
                    return true;
                }

                // Try matching at each position (use char-indices to stay on
                // UTF-8 boundaries — byte-index slicing panics on multi-byte
                // characters like 冰糖, see #249).
                let remaining: String = text_chars.collect();
                for (i, _) in remaining.char_indices() {
                    if matches_simple_glob(&remaining[i..], &next_pattern) {
                        return true;
                    }
                }
                // Also try the empty suffix at end of string
                if matches_simple_glob("", &next_pattern) {
                    return true;
                }
                return false;
            }
            '?' => {
                // Match exactly one character
                if text_chars.next().is_none() {
                    return false;
                }
            }
            c => {
                // Match literal character
                if text_chars.next() != Some(c) {
                    return false;
                }
            }
        }
    }

    text_chars.next().is_none()
}

/// Reorder `matches` by BM25 relevance score so files with more and rarer
/// term matches appear first. Stable for files with equal scores.
fn bm25_rank(matches: &mut Vec<GrepMatch>, pattern: &str) {
    if matches.is_empty() {
        return;
    }

    // 1. Extract query terms from the regex pattern.
    let terms: Vec<String> = pattern
        .replace(
            [
                '.', '*', '+', '?', '(', ')', '[', ']', '{', '}', '^', '$', '|', '\\',
            ],
            " ",
        )
        .split_whitespace()
        .map(|s| s.to_lowercase())
        .filter(|s| s.len() >= 2)
        .collect();
    if terms.is_empty() {
        return;
    }

    // 2. Group matches by file, count term occurrences per file.
    let mut file_term_counts: std::collections::HashMap<
        String,
        std::collections::HashMap<String, usize>,
    > = std::collections::HashMap::new();
    for m in matches.iter() {
        let entry = file_term_counts.entry(m.file.clone()).or_default();
        let line_lower = m.line.to_lowercase();
        for term in &terms {
            if line_lower.contains(term.as_str()) {
                *entry.entry(term.clone()).or_insert(0) += 1;
            }
        }
    }

    let total_files = file_term_counts.len() as f64;

    // 3. Compute IDF per term.
    let mut term_idf: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
    for term in &terms {
        let n = file_term_counts
            .values()
            .filter(|counts| counts.contains_key(term.as_str()))
            .count() as f64;
        let idf = ((total_files - n + 0.5) / (n + 0.5) + 1.0).ln();
        term_idf.insert(term.clone(), idf);
    }

    // 4. Compute BM25 score per file.
    let k1: f64 = 1.2;
    let b: f64 = 0.75;
    let avgdl: f64 = total_files; // use file count as avgdl proxy

    // Precompute matches-per-file once (was an O(files × matches) inner scan).
    let mut file_match_total: std::collections::HashMap<&str, usize> =
        std::collections::HashMap::new();
    for m in matches.iter() {
        *file_match_total.entry(m.file.as_str()).or_insert(0) += 1;
    }

    let mut file_scores: Vec<(String, f64)> = file_term_counts
        .iter()
        .map(|(file, counts)| {
            let dl = file_match_total.get(file.as_str()).copied().unwrap_or(0) as f64;
            let score: f64 = terms
                .iter()
                .filter_map(|term| {
                    let tf = *counts.get(term)? as f64;
                    let idf = term_idf.get(term)?;
                    Some(idf * (tf * (k1 + 1.0)) / (tf + k1 * (1.0 - b + b * dl / avgdl.max(1.0))))
                })
                .sum();
            (file.clone(), score)
        })
        .collect();

    // 5. Sort by score descending, stable for equal scores.
    file_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let rank: std::collections::HashMap<String, usize> = file_scores
        .iter()
        .enumerate()
        .map(|(i, (f, _))| (f.clone(), i))
        .collect();

    matches.sort_by_key(|m| rank.get(&m.file).copied().unwrap_or(usize::MAX));
}

/// Boost matches that also appear in symbol index hits to the top.
/// Uses a stable sort so existing BM25 order is preserved among peers.
fn boost_index_hits(matches: &mut Vec<GrepMatch>, symbol_hits: &[serde_json::Value]) {
    if symbol_hits.is_empty() || matches.is_empty() {
        return;
    }
    use std::collections::HashSet;
    let hit_set: HashSet<(String, usize)> = symbol_hits
        .iter()
        .filter_map(|h| {
            let file = h.get("file")?.as_str()?;
            let line = h.get("line")?.as_u64()?;
            Some((file.to_string(), line as usize))
        })
        .collect();

    matches.sort_by(|a, b| {
        let a_boost = hit_set.contains(&(a.file.clone(), a.line_number));
        let b_boost = hit_set.contains(&(b.file.clone(), b.line_number));
        match (a_boost, b_boost) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => std::cmp::Ordering::Equal,
        }
    });
}

/// Ensure the symbol index exists and is up-to-date.

/// Query the symbol index for definitions matching `pattern`.
/// Returns file:line pairs with match_score. Assumes `ensure_symbol_index()`
/// has been called recently — uses whatever index is on disk (may be stale).
fn lookup_symbol_hits(
    workspace: &Path,
    pattern: &str,
    kind_filter: Option<&str>,
) -> Vec<serde_json::Value> {
    let index_path = workspace_meta_file_read(workspace, "symbols.json");

    let index: Option<crate::symbol_index::SymbolIndex> = std::fs::read_to_string(&index_path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok());

    let index = match index {
        Some(idx) => idx,
        None => return Vec::new(),
    };

    // Strip regex metacharacters to extract a plain symbol name.
    let cleaned = pattern.replace(
        [
            '.', '*', '+', '?', '(', ')', '[', ']', '{', '}', '^', '$', '|', '\\',
        ],
        " ",
    );
    let terms: Vec<&str> = cleaned.split_whitespace().collect();
    if terms.is_empty() {
        return Vec::new();
    }

    // Try each term as a symbol query. Stop at the first term that yields hits.
    for term in &terms {
        let hits = if let Some(kf) = kind_filter {
            crate::symbol_index::query_symbol_with_mode(
                &index,
                term,
                crate::symbol_index::MatchMode::Substring,
                Some(kf),
            )
        } else {
            crate::symbol_index::query_symbol(&index, term)
        };
        if !hits.is_empty() {
            return hits
                .into_iter()
                .map(|(file, line, kind, prio)| {
                    let match_score = match prio {
                        0 => 1.0,
                        1 => 0.8,
                        4 => 0.4,
                        _ => 0.5,
                    };
                    let calls = index
                        .files
                        .get(file)
                        .and_then(|fs| fs.symbols.iter().find(|s| s.line == line && s.kind == kind))
                        .map(|s| s.calls.clone())
                        .filter(|c| !c.is_empty());
                    let mut hit = json!({
                        "symbol": term,
                        "file": file,
                        "line": line,
                        "kind": kind,
                        "match_score": match_score,
                    });
                    if let Some(c) = calls {
                        hit.as_object_mut()
                            .expect("hit object")
                            .insert("calls".into(), json!(c));
                    }
                    hit
                })
                .collect();
        }
    }

    Vec::new()
}

// === Unit Tests ===

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::{Value, json};
    use tempfile::tempdir;

    use crate::tools::spec::{ApprovalRequirement, ToolContext, ToolSpec};

    use super::{GrepFilesTool, matches_glob};

    #[test]
    fn test_matches_glob_star() {
        assert!(matches_glob("test.rs", "*.rs"));
        assert!(matches_glob("foo.rs", "*.rs"));
        assert!(!matches_glob("test.ts", "*.rs"));
        assert!(!matches_glob("test.rs.bak", "*.rs"));
    }

    #[test]
    fn test_matches_glob_question() {
        assert!(matches_glob("test.rs", "test.??"));
        assert!(!matches_glob("test.rs", "test.?"));
    }

    #[test]
    fn test_matches_glob_double_star() {
        assert!(matches_glob("src/main.rs", "src/**"));
        assert!(matches_glob("src/lib/mod.rs", "src/**"));
        assert!(matches_glob("node_modules/pkg/index.js", "node_modules/*"));
    }

    #[test]
    fn test_matches_glob_path() {
        assert!(matches_glob("src/main.rs", "src/*.rs"));
        assert!(!matches_glob("lib/main.rs", "src/*.rs"));
    }

    /// Regression for #249: byte-index slicing panics on multi-byte
    /// characters inside filenames like `dialogue_line__冰糖.mp3`.
    #[test]
    fn test_matches_glob_unicode_filename() {
        let filename = "dialogue_line__冰糖.mp3";
        // The filename should match *.mp3 without panicking.
        assert!(matches_glob(filename, "*.mp3"));
        // Asterisk matching against multi-byte characters must succeed.
        assert!(matches_glob(filename, "dialogue_line__*"));
        // Literal multi-byte characters inside the pattern must match.
        assert!(matches_glob(filename, "*冰糖*"));
        // Non-matching pattern must not panic either.
        assert!(!matches_glob(filename, "nonexistent*"));
    }

    #[tokio::test]
    async fn test_grep_files_basic() {
        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path().to_path_buf());

        // Create test files
        fs::write(
            tmp.path().join("test.rs"),
            "fn main() {\n    println!(\"hello\");\n}\n",
        )
        .expect("write");
        fs::write(
            tmp.path().join("lib.rs"),
            "pub fn hello() {}\npub fn world() {}\n",
        )
        .expect("write");

        let tool = GrepFilesTool;
        let result = tool
            .execute(json!({"pattern": "fn"}), &ctx)
            .await
            .expect("execute");

        assert!(result.success);
        assert!(result.content.contains("main"));
        assert!(result.content.contains("hello"));
    }

    #[tokio::test]
    async fn test_grep_files_with_context() {
        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path().to_path_buf());

        fs::write(
            tmp.path().join("test.txt"),
            "line1\nline2\nMATCH\nline4\nline5\n",
        )
        .expect("write");

        let tool = GrepFilesTool;
        let result = tool
            .execute(json!({"pattern": "MATCH", "context_lines": 1}), &ctx)
            .await
            .expect("execute");

        assert!(result.success);
        assert!(result.content.contains("line2")); // context before
        assert!(result.content.contains("line4")); // context after
    }

    #[tokio::test]
    async fn test_grep_files_case_insensitive() {
        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path().to_path_buf());

        fs::write(
            tmp.path().join("test.txt"),
            "Hello World\nHELLO WORLD\nhello world\n",
        )
        .expect("write");

        let tool = GrepFilesTool;
        let result = tool
            .execute(json!({"pattern": "hello", "case_insensitive": true}), &ctx)
            .await
            .expect("execute");

        assert!(result.success);
        // Should find all 3 lines
        let parsed: Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(parsed["total_matches"].as_u64().unwrap(), 3);
    }

    #[tokio::test]
    async fn test_grep_files_include_filter() {
        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path().to_path_buf());

        fs::write(tmp.path().join("test.rs"), "fn test() {}\n").expect("write");
        fs::write(tmp.path().join("test.js"), "function test() {}\n").expect("write");

        let tool = GrepFilesTool;
        let result = tool
            .execute(json!({"pattern": "test", "include": ["*.rs"]}), &ctx)
            .await
            .expect("execute");

        assert!(result.success);
        // Should only match .rs file
        let parsed: Value = serde_json::from_str(&result.content).unwrap();
        let matches = parsed["matches"].as_array().unwrap();
        assert_eq!(matches.len(), 1);
        let file = matches[0]["file"].as_str().unwrap();
        assert!(
            file.rsplit('.')
                .next()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("rs"))
        );
    }

    #[tokio::test]
    async fn test_grep_files_invalid_regex() {
        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path().to_path_buf());

        let tool = GrepFilesTool;
        let result = tool.execute(json!({"pattern": "[invalid"}), &ctx).await;

        assert!(result.is_err());
    }

    #[test]
    fn test_grep_files_tool_properties() {
        let tool = GrepFilesTool;
        assert_eq!(tool.name(), "grep_files");
        assert!(tool.is_read_only());
        assert!(tool.is_sandboxable());
        assert_eq!(tool.approval_requirement(), ApprovalRequirement::Auto);
    }

    #[test]
    fn test_parallel_support_flags() {
        let tool = GrepFilesTool;
        assert!(tool.supports_parallel());
    }

    #[tokio::test]
    async fn test_grep_files_respects_gitignore() {
        let tmp = tempdir().expect("tempdir");
        fs::write(tmp.path().join(".gitignore"), "ignored.txt\n").expect("write");
        fs::write(tmp.path().join("ignored.txt"), "SECRET\n").expect("write");
        fs::write(tmp.path().join("visible.txt"), "VISIBLE\n").expect("write");

        let ctx = ToolContext::new(tmp.path().to_path_buf());
        let tool = GrepFilesTool;
        let result = tool
            .execute(json!({"pattern": "SECRET"}), &ctx)
            .await
            .expect("execute");

        assert!(result.success);
        let parsed: Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(parsed["total_matches"].as_u64().unwrap(), 0);

        let all = tool
            .execute(
                json!({"pattern": "SECRET", "respect_gitignore": false}),
                &ctx,
            )
            .await
            .expect("execute");
        let parsed_all: Value = serde_json::from_str(&all.content).unwrap();
        assert!(parsed_all["total_matches"].as_u64().unwrap() >= 1);
    }

    #[tokio::test]
    async fn test_grep_files_decodes_gb18030() {
        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path().to_path_buf());

        // ASCII "needle" followed by GBK-encoded 你好 (invalid UTF-8). Before the
        // fix `read_to_string` failed and the file was skipped as binary.
        let mut bytes = b"fn needle() {}\n".to_vec();
        bytes.extend_from_slice(&[0xC4, 0xE3, 0xBA, 0xC3, b'\n']);
        fs::write(tmp.path().join("zh.rs"), bytes).expect("write");

        let tool = GrepFilesTool;
        let result = tool
            .execute(json!({"pattern": "needle"}), &ctx)
            .await
            .expect("execute");
        let parsed: Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(parsed["total_matches"].as_u64().unwrap(), 1);
    }

    #[tokio::test]
    async fn test_grep_files_skips_binary() {
        let tmp = tempdir().expect("tempdir");
        let mut bytes = b"before\0after".to_vec();
        bytes.extend_from_slice(b"\nplain line\n");
        fs::write(tmp.path().join("binary.dat"), bytes).expect("write");
        fs::write(tmp.path().join("plain.txt"), "findme\n").expect("write");

        let ctx = ToolContext::new(tmp.path().to_path_buf());
        let tool = GrepFilesTool;
        let result = tool
            .execute(json!({"pattern": "findme"}), &ctx)
            .await
            .expect("execute");

        let parsed: Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(parsed["total_matches"].as_u64().unwrap(), 1);
        assert!(parsed["files_skipped_binary"].as_u64().unwrap() >= 1);
    }

    #[tokio::test]
    async fn test_grep_files_output_mode_files_with_matches() {
        let tmp = tempdir().expect("tempdir");
        fs::write(tmp.path().join("a.rs"), "fn alpha() {}\n").expect("write");
        fs::write(tmp.path().join("b.rs"), "fn beta() {}\n").expect("write");

        let ctx = ToolContext::new(tmp.path().to_path_buf());
        let tool = GrepFilesTool;
        let result = tool
            .execute(
                json!({"pattern": "fn", "output_mode": "files_with_matches"}),
                &ctx,
            )
            .await
            .expect("execute");

        let parsed: Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(parsed["output_mode"], "files_with_matches");
        let files = parsed["files"].as_array().unwrap();
        assert_eq!(files.len(), 2);
        assert!(parsed.get("matches").is_none());
    }

    #[tokio::test]
    async fn test_grep_files_output_mode_count() {
        let tmp = tempdir().expect("tempdir");
        fs::write(tmp.path().join("a.rs"), "x\nx\n").expect("write");

        let ctx = ToolContext::new(tmp.path().to_path_buf());
        let tool = GrepFilesTool;
        let result = tool
            .execute(json!({"pattern": "x", "output_mode": "count"}), &ctx)
            .await
            .expect("execute");

        let parsed: Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(parsed["output_mode"], "count");
        let counts = parsed["file_counts"].as_array().unwrap();
        assert_eq!(counts[0]["match_count"].as_u64().unwrap(), 2);
    }

    #[tokio::test]
    async fn test_grep_files_matches_crlf_lines() {
        let tmp = tempdir().expect("tempdir");
        fs::write(tmp.path().join("crlf.txt"), "fn foo()\r\n").expect("write");

        let ctx = ToolContext::new(tmp.path().to_path_buf());
        let tool = GrepFilesTool;
        let result = tool
            .execute(json!({"pattern": "fn foo"}), &ctx)
            .await
            .expect("execute");

        let parsed: Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(parsed["total_matches"].as_u64().unwrap(), 1);
    }

    #[test]
    fn test_is_probably_binary() {
        use crate::tools::workspace_walk::is_probably_binary;

        let tmp = tempdir().expect("tempdir");
        let bin = tmp.path().join("x.bin");
        fs::write(&bin, b"a\0b").expect("write");
        assert!(is_probably_binary(&bin));

        let txt = tmp.path().join("x.txt");
        fs::write(&txt, b"hello").expect("write");
        assert!(!is_probably_binary(&txt));
    }
}
