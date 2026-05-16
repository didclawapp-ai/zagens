//! Search tools: `grep_files` for code search
//!
//! These tools provide powerful code search capabilities within the workspace,
//! similar to ripgrep/grep functionality.

use super::spec::{
    ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec, optional_bool, optional_str,
    optional_u64, required_str,
};
use async_trait::async_trait;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

static SYMBOL_INDEX_BUILDING: std::sync::LazyLock<Mutex<HashSet<PathBuf>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashSet::new()));

/// Maximum number of results to return to avoid overwhelming output
const MAX_RESULTS: usize = 100;

/// Maximum file size to search (skip large binaries)
const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024; // 10MB

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
        "Search for a regex pattern in files within the workspace. Returns matching lines with context."
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
        let context_lines =
            usize::try_from(optional_u64(&input, "context_lines", 2)).unwrap_or(usize::MAX);
        let case_insensitive = optional_bool(&input, "case_insensitive", false);
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

        // Parse exclude patterns
        let exclude_patterns: Vec<String> =
            input.get("exclude").and_then(|v| v.as_array()).map_or_else(
                || {
                    // Default exclusions for common non-code directories
                    vec![
                        "node_modules/*".to_string(),
                        ".git/*".to_string(),
                        "target/*".to_string(),
                        "*.min.js".to_string(),
                        "*.min.css".to_string(),
                        "dist/*".to_string(),
                        "build/*".to_string(),
                        "__pycache__/*".to_string(),
                        ".venv/*".to_string(),
                        "venv/*".to_string(),
                    ]
                },
                |arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                },
            );

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

        // Collect files to search
        let files = collect_files(&search_path, &include_patterns, &exclude_patterns)?;

        // Search files
        let mut results: Vec<GrepMatch> = Vec::new();
        let mut files_searched = 0;
        let mut total_matches = 0;

        for file_path in files {
            if results.len() >= max_results {
                break;
            }

            // Skip files that are too large
            if let Ok(metadata) = fs::metadata(&file_path)
                && metadata.len() > MAX_FILE_SIZE
            {
                continue;
            }

            // Read file content
            let Ok(file_content) = fs::read_to_string(&file_path) else {
                continue; // Skip binary or unreadable files
            };

            files_searched += 1;
            let lines: Vec<&str> = file_content.lines().collect();

            for (line_idx, line) in lines.iter().enumerate() {
                if regex.is_match(line) {
                    total_matches += 1;

                    // Get context lines
                    let context_before: Vec<String> = (line_idx.saturating_sub(context_lines)
                        ..line_idx)
                        .filter_map(|i| lines.get(i).map(|s| (*s).to_string()))
                        .collect();

                    let context_after: Vec<String> = ((line_idx + 1)
                        ..=(line_idx + context_lines).min(lines.len() - 1))
                        .filter_map(|i| lines.get(i).map(|s| (*s).to_string()))
                        .collect();

                    // Get relative path from workspace
                    let relative_path = file_path
                        .strip_prefix(&context.workspace)
                        .unwrap_or(&file_path)
                        .to_string_lossy()
                        .to_string();

                    results.push(GrepMatch {
                        file: relative_path,
                        line_number: line_idx + 1,
                        line: (*line).to_string(),
                        context_before,
                        context_after,
                    });

                    if results.len() >= max_results {
                        break;
                    }
                }
            }
        }

        // BM25 re-rank: order files by relevance so the model sees the
        // most likely definitions / usages first, not filesystem order.
        bm25_rank(&mut results, &pattern_str);

        // Symbol index lookup: if the pattern matches a known Rust symbol,
        // prepend file:line references so the model can jump to definitions.
        let symbol_hits = lookup_symbol_hits(&context.workspace, &pattern_str);
        let symbol_status = crate::symbol_index::index_status(&context.workspace);

        // Build result
        let result = json!({
            "matches": results,
            "total_matches": total_matches,
            "files_searched": files_searched,
            "truncated": total_matches > max_results,
            "symbol_index_hits": symbol_hits,
            "symbol_index_status": symbol_status,
        });

        ToolResult::json(&result).map_err(|e| ToolError::execution_failed(e.to_string()))
    }
}

/// Collect files to search based on include/exclude patterns
fn collect_files(
    root: &Path,
    include_patterns: &[String],
    exclude_patterns: &[String],
) -> Result<Vec<PathBuf>, ToolError> {
    let mut files = Vec::new();

    if root.is_file() {
        files.push(root.to_path_buf());
        return Ok(files);
    }

    collect_files_recursive(root, root, include_patterns, exclude_patterns, &mut files)?;
    Ok(files)
}

fn collect_files_recursive(
    root: &Path,
    current: &Path,
    include_patterns: &[String],
    exclude_patterns: &[String],
    files: &mut Vec<PathBuf>,
) -> Result<(), ToolError> {
    let entries = fs::read_dir(current).map_err(|e| {
        ToolError::execution_failed(format!(
            "Failed to read directory {}: {}",
            current.display(),
            e
        ))
    })?;

    for entry in entries {
        let entry = entry.map_err(|e| ToolError::execution_failed(e.to_string()))?;
        let path = entry.path();

        // Get relative path for pattern matching
        let relative = path.strip_prefix(root).unwrap_or(&path);
        let relative_str = relative.to_string_lossy();

        // Check exclusions
        if should_exclude(&relative_str, exclude_patterns) {
            continue;
        }

        if path.is_dir() {
            collect_files_recursive(root, &path, include_patterns, exclude_patterns, files)?;
        } else if path.is_file() {
            // Check inclusions (if any specified)
            if include_patterns.is_empty() || should_include(&relative_str, include_patterns) {
                files.push(path);
            }
        }
    }

    Ok(())
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
            ['.', '*', '+', '?', '(', ')', '[', ']', '{', '}', '^', '$', '|', '\\'],
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
    let mut file_term_counts: std::collections::HashMap<String, std::collections::HashMap<String, usize>> =
        std::collections::HashMap::new();
    for m in matches.iter() {
        let entry = file_term_counts
            .entry(m.file.clone())
            .or_default();
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

    let mut file_scores: Vec<(String, f64)> = file_term_counts
        .iter()
        .map(|(file, counts)| {
            let dl = matches.iter().filter(|m| &m.file == file).count() as f64;
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

/// Try loading the symbol index and query it for symbols matching `pattern`.
/// Returns file:line pairs for definition-side hits.
///
/// When the index is missing (first use per workspace) or stale (source files
/// changed), a background thread rebuilds it. The current query uses whatever
/// is immediately available — an empty result for missing, the stale index
/// for stale — so the model is never blocked waiting for index build.
fn lookup_symbol_hits(workspace: &Path, pattern: &str) -> Vec<serde_json::Value> {
    let index_dir = workspace.join(".deepseek");
    let index_path = index_dir.join("symbols.json");

    // Try to load existing index
    let index: Option<crate::symbol_index::SymbolIndex> = std::fs::read_to_string(&index_path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok());

    // Determine build status
    let needs_build = match &index {
        Some(_idx) => {
            // Stale check: skip if we already know it needs rebuild
            crate::symbol_index::index_status(workspace) != crate::symbol_index::IndexStatus::Fresh
        }
        None => true,
    };

    // Schedule background build if needed and not already building this workspace
    if needs_build {
        let mut building = SYMBOL_INDEX_BUILDING.lock().unwrap();
        let ws_canonical = workspace
            .canonicalize()
            .unwrap_or_else(|_| workspace.to_path_buf());
        if building.insert(ws_canonical.clone()) {
            drop(building);
            let ws = ws_canonical;
            std::thread::Builder::new()
                .name("symbol-index".into())
                .stack_size(8 * 1024 * 1024)
                .spawn(move || {
                    let index = crate::symbol_index::build_index(&ws, crate::symbol_index::SymbolVisibility::Public);
                    let _ = std::fs::create_dir_all(ws.join(".deepseek"));
                    let _ = std::fs::write(
                        ws.join(".deepseek").join("symbols.json"),
                        serde_json::to_string_pretty(&index).unwrap_or_default(),
                    );
                    SYMBOL_INDEX_BUILDING.lock().unwrap().remove(&ws);
                })
                .ok();
        }
    }

    let index = match index {
        Some(idx) => idx,
        None => return Vec::new(),
    };

    // Strip regex metacharacters to extract a plain symbol name.
    let cleaned = pattern
        .replace(['.', '*', '+', '?', '(', ')', '[', ']', '{', '}', '^', '$', '|', '\\'], " ");
    let terms: Vec<&str> = cleaned.split_whitespace().collect();
    if terms.is_empty() {
        return Vec::new();
    }

    // Try each term as a symbol query. Stop at the first term that yields hits.
    for term in &terms {
        let hits = crate::symbol_index::query_symbol(&index, term);
        if !hits.is_empty() {
            return hits
                .into_iter()
                .map(|(file, line)| {
                    json!({"symbol": term, "file": file, "line": line})
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
}
