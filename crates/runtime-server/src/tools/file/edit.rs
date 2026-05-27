//! edit_file tool (search/replace and line operations).

use crate::tools::diff_format::make_unified_diff;
use crate::tools::spec::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec,
    lsp_diagnostics_for_paths, optional_bool, optional_str, optional_u64, required_str,
};
use super::write::{
    find_match_line_numbers, jsx_balance_warning, make_compact_change,
    normalize_line_endings,
};
use async_trait::async_trait;
use serde_json::{Value, json};
use std::fs;

/// Tool for search/replace editing of files.
pub struct EditFileTool;

#[async_trait]
impl ToolSpec for EditFileTool {
    fn name(&self) -> &'static str {
        "edit_file"
    }

    fn description(&self) -> &'static str {
        "Replace text in a file using search/replace. Required: 'path' (file to edit), 'search' (exact text to find), 'replace' (text to substitute)."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file"
                },
                "search": {
                    "type": "string",
                    "description": "Text to search for"
                },
                "replace": {
                    "type": "string",
                    "description": "Text to replace with"
                },
                "start_line": {
                    "type": "integer",
                    "description": "Limit the search range to start at this 1-based line (inclusive). Use with end_line for precision."
                },
                "end_line": {
                    "type": "integer",
                    "description": "Limit the search range to end at this 1-based line (inclusive)."
                },
                "replace_mode": {
                    "type": "string",
                    "enum": ["first", "all"],
                    "description": "[search_replace mode] When there are multiple matches: 'first' replaces only the first, 'all' replaces all (requires explicit choice)."
                },
                "operation": {
                    "type": "string",
                    "enum": ["search_replace", "insert_after", "delete_lines", "replace_line"],
                    "description": "Edit operation. Default 'search_replace'. Other modes use line numbers instead of search strings."
                },
                "text": {
                    "type": "string",
                    "description": "[insert_after / replace_line mode] The text to insert or use as replacement."
                },
                "after_line": {
                    "type": "integer",
                    "description": "[insert_after mode] Insert text after this line number (1-based). 0 = at the beginning of the file."
                },
                "line": {
                    "type": "integer",
                    "description": "[replace_line mode] The line number to replace (1-based)."
                },
                "dry_run": {
                    "type": "boolean",
                    "description": "[delete_lines mode] If true, preview what would be deleted without modifying the file. Returns the lines that would be removed."
                }
            },
            "required": ["path"]
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![
            ToolCapability::WritesFiles,
            ToolCapability::Sandboxable,
            ToolCapability::RequiresApproval,
        ]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Suggest
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let _path_str = required_str(&input, "path")?;
        let operation = optional_str(&input, "operation").unwrap_or("search_replace");
        match operation {
            "search_replace" => self.execute_search_replace(&input, context).await,
            "insert_after" => self.execute_insert_after(&input, context).await,
            "delete_lines" => self.execute_delete_lines(&input, context).await,
            "replace_line" => self.execute_replace_line(&input, context).await,
            other => Err(ToolError::invalid_input(format!(
                "Unknown operation '{}'. Valid operations: search_replace, insert_after, delete_lines, replace_line.",
                other
            ))),
        }
    }
}

impl EditFileTool {
    /// search_replace operation — the original V0 behaviour.
    async fn execute_search_replace(
        &self,
        input: &Value,
        context: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let path_str = required_str(input, "path")?;
        let search = required_str(input, "search")?;
        let replace = required_str(input, "replace")?;
        let start_line = optional_u64(input, "start_line", 0) as usize;
        let end_line = optional_u64(input, "end_line", 0) as usize;
        let replace_mode = optional_str(input, "replace_mode");

        let file_path = context.resolve_path(path_str)?;

        let contents = fs::read_to_string(&file_path).map_err(|e| {
            let kind = e.kind();
            if kind == std::io::ErrorKind::NotFound {
                ToolError::execution_failed(format!(
                    "[NOT_FOUND] 文件 {} 不存在: {e}",
                    file_path.display()
                ))
            } else if kind == std::io::ErrorKind::PermissionDenied {
                ToolError::execution_failed(format!(
                    "[PERMISSION] 没有权限读取 {}: {e}",
                    file_path.display()
                ))
            } else {
                ToolError::execution_failed(format!("Failed to read {}: {e}", file_path.display()))
            }
        })?;

        // E1: Normalize line endings — `fs::read_to_string` preserves platform
        // CRLF on Windows, but the model's search string uses LF (\n).
        let file_le = if contents.contains("\r\n") { "\r\n" } else { "\n" };
        let search_norm = if file_le == "\r\n" {
            let s = search.replace("\r\n", "\n");
            s.replace('\n', "\r\n")
        } else {
            search.to_string()
        };
        let replace_norm = if file_le == "\r\n" {
            let r = replace.replace("\r\n", "\n");
            r.replace('\n', "\r\n")
        } else {
            replace.to_string()
        };

        // E2: If start_line/end_line are specified, narrow the search to that
        // line range to avoid false matches in unrelated parts of the file.
        let (search_target, _range_offset, range_prefix, range_suffix) =
            if start_line > 0 {
                let lines: Vec<&str> = contents.lines().collect();
                let s = start_line.saturating_sub(1);
                let e = if end_line > 0 { end_line.min(lines.len()) } else { lines.len() };
                let slice = lines[s..e].join(file_le);
                let byte_offset: usize = lines[..s]
                    .iter()
                    .map(|l| l.len() + file_le.len())
                    .sum();
                let prefix = &contents[..byte_offset];
                let suffix = &contents[byte_offset + slice.len()..];
                (slice, byte_offset, prefix.to_string(), suffix.to_string())
            } else {
                (contents.clone(), 0, String::new(), String::new())
            };

        let count = search_target.matches(&search_norm).count();
        if count == 0 {
            // E3: Diagnostic error — tell the model *why* matching failed.
            let lines_in_search = search.lines().count();
            let has_crlf = contents.contains("\r\n");
            let search_is_lf_only = search.contains('\n') && !search.contains("\r\n");

            let hint = if has_crlf && search_is_lf_only && lines_in_search > 1 {
                "[HINT: file uses CRLF (\\r\\n) but search uses LF (\\n) — the tool normalises this automatically; if it still fails the search content itself does not match the file]"
            } else if lines_in_search > 1 {
                "[HINT: multi-line search — verify that every character including indentation (tabs/spaces) matches the file exactly. Use read_file on the target region first.]"
            } else {
                "[HINT: single-line search — use grep_files with the same pattern to locate the exact text in the file, then copy it verbatim.]"
            };

            let alt = if start_line > 0 {
                "\nIf you know the exact line number, retry with operation: \"replace_line\" and line: <number> to bypass search entirely."
            } else {
                ""
            };

            return Err(ToolError::execution_failed(format!(
                "[NOT_FOUND] search string not found in {}. {hint}{alt}",
                file_path.display(),
            )));
        }

        // E4: When there are multiple matches, require an explicit choice to
        // avoid accidental sweeping replacements.
        if count > 1 && replace_mode.is_none() {
            let match_lines = find_match_line_numbers(&search_target, &search_norm, 3);
            let line_list: Vec<String> = match_lines
                .iter()
                .map(|n| {
                    let adjusted = n + if start_line > 0 { start_line.saturating_sub(1) } else { 0 };
                    format!("line {adjusted}")
                })
                .collect();

            return Err(ToolError::execution_failed(format!(
                "[AMBIGUOUS] search matched {count} times in {}. \
                Please specify replace_mode: \
                'first' to replace only the first occurrence, \
                or 'all' to replace all {count}. \
                Match locations (first {}): {}",
                file_path.display(),
                match_lines.len().min(3),
                line_list.join(", ")
            )));
        }

        let updated_target = if replace_mode == Some("first") {
            search_target.replacen(&search_norm, &replace_norm, 1)
        } else {
            search_target.replace(&search_norm, &replace_norm)
        };

        let updated = if start_line > 0 {
            format!("{range_prefix}{updated_target}{range_suffix}")
        } else {
            updated_target
        };

        fs::write(&file_path, &updated).map_err(|e| {
            ToolError::execution_failed(format!("Failed to write {}: {}", file_path.display(), e))
        })?;

        let display = file_path.display().to_string();
        // E5: Include hit line numbers so the model can verify without a
        // follow-up read_file call.
        let match_lines = find_match_line_numbers(&contents, &search_norm, 5);
        let line_list: Vec<String> = match_lines
            .iter()
            .map(|n| format!("line {n}"))
            .collect();
        let diff = make_unified_diff(&display, &contents, &updated);
        let total_lines = updated.lines().count();
        let summary = if line_list.is_empty() {
            format!("Replaced {count} occurrence(s) in {display} — file now {total_lines} lines")
        } else {
            format!(
                "Replaced {count} occurrence(s) in {display} ({}) — file now {total_lines} lines",
                line_list.join(", ")
            )
        };
        let body = if diff.is_empty() {
            format!("{summary}\n(no textual changes)")
        } else {
            format!("{diff}\n{summary}")
        };

        let jsx_warning = jsx_balance_warning(&file_path, &updated);

        // Append LSP diagnostics for the edited file when enabled (#428).
        // V1-4: Append compact before/after for small changes (≤5 lines total).
        let compact = if search.lines().count() + replace.lines().count() <= 5 {
            format!("\n--- compact ---\n{}", make_compact_change(search, replace))
        } else {
            String::new()
        };

        let diag_block = lsp_diagnostics_for_paths(context, &[file_path]).await;
        let full_body = if diag_block.is_empty() {
            format!("{body}{compact}{jsx_warning}")
        } else {
            format!("{body}{compact}{jsx_warning}\n{diag_block}")
        };

        Ok(ToolResult::success(full_body))
    }

    /// insert_after operation — insert `text` after `after_line` (1-based).
    /// after_line: 0 = insert at beginning of file.
    async fn execute_insert_after(
        &self,
        input: &Value,
        context: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let path_str = required_str(input, "path")?;
        let text = required_str(input, "text")?;
        let after_line = optional_u64(input, "after_line", 0) as usize;

        let file_path = context.resolve_path(path_str)?;
        let contents = fs::read_to_string(&file_path).map_err(|e| {
            let kind = e.kind();
            if kind == std::io::ErrorKind::NotFound {
                ToolError::execution_failed(format!(
                    "[NOT_FOUND] file {} does not exist: {e}",
                    file_path.display()
                ))
            } else {
                ToolError::execution_failed(format!("Failed to read {}: {e}", file_path.display()))
            }
        })?;

        let file_le = if contents.contains("\r\n") { "\r\n" } else { "\n" };
        let text_normalized = normalize_line_endings(text, file_le);
        let lines: Vec<&str> = contents.lines().collect();

        // V1-2: allow after_line == 0 (beginning) and after_line == lines.len() (end)
        if after_line > lines.len() {
            return Err(ToolError::execution_failed(format!(
                "[OUT_OF_RANGE] after_line={after_line} exceeds file line count {} ({})",
                lines.len(),
                file_path.display()
            )));
        }

        let mut new_lines: Vec<String> =
            Vec::with_capacity(lines.len() + text_normalized.lines().count());
        for l in &lines[..after_line] {
            new_lines.push(l.to_string());
        }
        for t in text_normalized.lines() {
            new_lines.push(t.to_string());
        }
        for l in &lines[after_line..] {
            new_lines.push(l.to_string());
        }
        let updated = new_lines.join(file_le);

        fs::write(&file_path, &updated).map_err(|e| {
            ToolError::execution_failed(format!("Failed to write {}: {e}", file_path.display()))
        })?;

        let display = file_path.display().to_string();
        let diff = make_unified_diff(&display, &contents, &updated);
        let inserted_count = text_normalized.lines().count();
        let total_lines = updated.lines().count();
        let position = if after_line == 0 {
            "beginning of file".to_string()
        } else if after_line == lines.len() {
            "end of file".to_string()
        } else {
            format!("after line {after_line}")
        };
        let summary = format!(
            "Inserted {inserted_count} line(s) at {position} in {display} — file now {total_lines} lines"
        );
        let body = format!("{diff}\n{summary}");

        let jsx_warning = jsx_balance_warning(&file_path, &updated);

        let diag_block = lsp_diagnostics_for_paths(context, &[file_path]).await;
        let full_body = if diag_block.is_empty() {
            format!("{body}{jsx_warning}")
        } else {
            format!("{body}{jsx_warning}\n{diag_block}")
        };
        Ok(ToolResult::success(full_body))
    }

    /// delete_lines operation — remove lines [start_line, end_line] inclusive (1-based).
    async fn execute_delete_lines(
        &self,
        input: &Value,
        context: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let path_str = required_str(input, "path")?;
        let start = optional_u64(input, "start_line", 0) as usize;
        let end = optional_u64(input, "end_line", 0) as usize;

        if start == 0 || end == 0 {
            return Err(ToolError::invalid_input(
                "delete_lines requires both start_line and end_line (1-based, inclusive)",
            ));
        }
        if start > end {
            return Err(ToolError::invalid_input(format!(
                "start_line ({start}) must be ≤ end_line ({end})"
            )));
        }

        let file_path = context.resolve_path(path_str)?;
        let contents = fs::read_to_string(&file_path).map_err(|e| {
            let kind = e.kind();
            if kind == std::io::ErrorKind::NotFound {
                ToolError::execution_failed(format!(
                    "[NOT_FOUND] file {} does not exist: {e}",
                    file_path.display()
                ))
            } else {
                ToolError::execution_failed(format!("Failed to read {}: {e}", file_path.display()))
            }
        })?;

        let file_le = if contents.contains("\r\n") { "\r\n" } else { "\n" };
        let lines: Vec<&str> = contents.lines().collect();

        if start > lines.len() {
            return Err(ToolError::execution_failed(format!(
                "[OUT_OF_RANGE] start_line={start} exceeds file line count {} ({})",
                lines.len(),
                file_path.display()
            )));
        }
        let e = end.min(lines.len());
        let dry_run = optional_bool(input, "dry_run", false);

        let deleted_lines: Vec<&str> = lines[start.saturating_sub(1)..e].to_vec();
        let deleted_count = e.saturating_sub(start) + 1;
        let range = if start == e {
            format!("line {start}")
        } else {
            format!("lines {start}–{e}")
        };

        if dry_run {
            let deleted_preview = deleted_lines
                .iter()
                .enumerate()
                .map(|(i, l)| format!("  [{:>4}] {}", start + i, l))
                .collect::<Vec<_>>()
                .join("\n");
            return Ok(ToolResult::success(format!(
                "[DRY_RUN] Would delete {deleted_count} line(s) ({range}) in {}:\n{deleted_preview}\n\
                To confirm, call delete_lines again with dry_run: false.",
                file_path.display()
            )));
        }

        let mut new_lines: Vec<String> = Vec::with_capacity(lines.len() - deleted_count);
        for l in &lines[..start.saturating_sub(1)] {
            new_lines.push(l.to_string());
        }
        for l in &lines[e..] {
            new_lines.push(l.to_string());
        }
        let updated = new_lines.join(file_le);

        fs::write(&file_path, &updated).map_err(|e| {
            ToolError::execution_failed(format!("Failed to write {}: {e}", file_path.display()))
        })?;

        let display = file_path.display().to_string();
        let diff = make_unified_diff(&display, &contents, &updated);
        let total_lines = updated.lines().count();
        let summary = format!(
            "Deleted {deleted_count} line(s) ({range}) in {display} — file now {total_lines} lines"
        );
        let body = format!("{diff}\n{summary}");

        let jsx_warning = jsx_balance_warning(&file_path, &updated);

        let diag_block = lsp_diagnostics_for_paths(context, &[file_path]).await;
        let full_body = if diag_block.is_empty() {
            format!("{body}{jsx_warning}")
        } else {
            format!("{body}{jsx_warning}\n{diag_block}")
        };
        Ok(ToolResult::success(full_body))
    }

    /// replace_line operation — replace a single line at `line` (1-based) with `text`.
    async fn execute_replace_line(
        &self,
        input: &Value,
        context: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let path_str = required_str(input, "path")?;
        let text = required_str(input, "text")?;
        let line = optional_u64(input, "line", 0) as usize;

        if line == 0 {
            return Err(ToolError::invalid_input(
                "replace_line requires the 'line' parameter (1-based)",
            ));
        }

        let file_path = context.resolve_path(path_str)?;
        let contents = fs::read_to_string(&file_path).map_err(|e| {
            let kind = e.kind();
            if kind == std::io::ErrorKind::NotFound {
                ToolError::execution_failed(format!(
                    "[NOT_FOUND] file {} does not exist: {e}",
                    file_path.display()
                ))
            } else {
                ToolError::execution_failed(format!("Failed to read {}: {e}", file_path.display()))
            }
        })?;

        let file_le = if contents.contains("\r\n") { "\r\n" } else { "\n" };
        let text_normalized = normalize_line_endings(text, file_le);
        let lines: Vec<&str> = contents.lines().collect();

        if line > lines.len() {
            return Err(ToolError::execution_failed(format!(
                "[OUT_OF_RANGE] line={line} exceeds file line count {} ({})",
                lines.len(),
                file_path.display()
            )));
        }

        let old_line = lines[line.saturating_sub(1)];
        let mut new_lines: Vec<String> = Vec::with_capacity(
            lines.len() + text_normalized.lines().count().saturating_sub(1),
        );
        for (i, l) in lines.iter().enumerate() {
            if i + 1 == line {
                for t in text_normalized.lines() {
                    new_lines.push(t.to_string());
                }
            } else {
                new_lines.push(l.to_string());
            }
        }
        let updated = new_lines.join(file_le);

        fs::write(&file_path, &updated).map_err(|e| {
            ToolError::execution_failed(format!("Failed to write {}: {e}", file_path.display()))
        })?;

        let display = file_path.display().to_string();
        let diff = make_unified_diff(&display, &contents, &updated);
        let compact = make_compact_change(old_line, &text_normalized);
        let total_lines = updated.lines().count();
        let summary = format!("Replaced line {line} in {display} — file now {total_lines} lines");
        let body = format!("{diff}\n--- compact ---\n{compact}{summary}");

        let jsx_warning = jsx_balance_warning(&file_path, &updated);

        let diag_block = lsp_diagnostics_for_paths(context, &[file_path]).await;
        let full_body = if diag_block.is_empty() {
            format!("{body}{jsx_warning}")
        } else {
            format!("{body}{jsx_warning}\n{diag_block}")
        };
        Ok(ToolResult::success(full_body))
    }
}
