//! write_file tool and shared edit/write helpers.

use crate::tools::diff_format::make_unified_diff;
use crate::tools::spec::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec,
    lsp_diagnostics_for_paths, required_str,
};
use async_trait::async_trait;
use serde_json::{Value, json};
use std::fs;
use std::path::Path;

/// Tool for writing UTF-8 files to the workspace.
pub struct WriteFileTool;

#[async_trait]
impl ToolSpec for WriteFileTool {
    fn name(&self) -> &'static str {
        "write_file"
    }

    fn description(&self) -> &'static str {
        "Write content to a UTF-8 file in the workspace."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file"
                },
                "content": {
                    "type": "string",
                    "description": "Content to write"
                }
            },
            "required": ["path", "content"]
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
        let path_str = required_str(&input, "path")?;
        let file_content = required_str(&input, "content")?;

        let scratchpad_cfg = context
            .runtime
            .wire
            .scratchpad_config
            .clone()
            .unwrap_or_default();
        let bound_run = context
            .runtime
            .wire
            .scratchpad_run_id
            .lock()
            .ok()
            .and_then(|g| g.clone());
        if let Some(block_msg) = deepseek_runtime_adapters::scratchpad_gates::check_write_file_audit_report_gate(
            &context.workspace,
            bound_run.as_deref(),
            &scratchpad_cfg,
            path_str,
        ) {
            return Err(ToolError::execution_failed(block_msg));
        }

        let file_path = context.resolve_path(path_str)?;

        // Snapshot the existing contents (if any) before we overwrite — used
        // to render an inline diff in the tool result.
        let existed_before = file_path.exists();
        let prior_contents = if existed_before {
            fs::read_to_string(&file_path).unwrap_or_default()
        } else {
            String::new()
        };

        // Create parent directories if needed
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                ToolError::execution_failed(format!(
                    "Failed to create directory {}: {}",
                    parent.display(),
                    e
                ))
            })?;
        }

        fs::write(&file_path, file_content).map_err(|e| {
            ToolError::execution_failed(format!("Failed to write {}: {}", file_path.display(), e))
        })?;

        let display = file_path.display().to_string();
        let diff = make_unified_diff(&display, &prior_contents, file_content);
        let summary = if existed_before {
            format!("Wrote {} bytes to {}", file_content.len(), display)
        } else {
            format!("Created {} ({} bytes)", display, file_content.len())
        };
        let body = if diff.is_empty() {
            format!("{summary}\n(no changes)")
        } else {
            format!("{diff}\n{summary}")
        };

        // Append LSP diagnostics for the written file when enabled (#428).
        let diag_block = lsp_diagnostics_for_paths(context, &[file_path]).await;
        let full_body = if diag_block.is_empty() {
            body
        } else {
            format!("{body}\n{diag_block}")
        };

        Ok(ToolResult::success(full_body))
    }
}

/// Normalize text line-endings to match the file's actual format.
/// When the file uses CRLF, converts `\n` → `\r\n` in the provided text.
pub(in crate::tools::file) fn normalize_line_endings(text: &str, file_le: &str) -> String {
    if file_le == "\r\n" {
        let s = text.replace("\r\n", "\n");
        s.replace('\n', "\r\n")
    } else {
        text.to_string()
    }
}

/// Build a compact before/after snippet for small changes.
pub(in crate::tools::file) fn make_compact_change(old: &str, new: &str) -> String {
    let mut out = String::new();
    for line in old.lines() {
        out.push_str(&format!("  - {line}\n"));
    }
    for line in new.lines() {
        out.push_str(&format!("  + {line}\n"));
    }
    out
}

/// Return the 1-based line numbers where `search` occurs in `contents`,
/// up to `max_results`.  Used by edit_file diagnostic messages.
pub(in crate::tools::file) fn find_match_line_numbers(
    contents: &str,
    search: &str,
    max_results: usize,
) -> Vec<usize> {
    let mut result = Vec::new();
    let mut byte_pos = 0;
    let mut line_num = 1;
    let content_bytes = contents.as_bytes();
    let search_bytes = search.as_bytes();
    while byte_pos <= content_bytes.len().saturating_sub(search_bytes.len()) {
        if result.len() >= max_results {
            break;
        }
        if content_bytes[byte_pos..].starts_with(search_bytes) {
            result.push(line_num);
            byte_pos += search_bytes.len();
        } else if content_bytes[byte_pos] == b'\n' {
            line_num += 1;
            byte_pos += 1;
        } else {
            byte_pos += 1;
        }
    }
    result
}

pub(in crate::tools::file) fn check_jsx_balance(content: &str) -> Option<String> {
    let mut brace_depth: i32 = 0;
    let mut paren_depth: i32 = 0;
    let mut in_string = false;
    let mut string_char = ' ';
    let mut warnings = Vec::new();

    for ch in content.chars() {
        if in_string {
            if ch == string_char {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' | '\'' | '`' => {
                in_string = true;
                string_char = ch;
            }
            '{' => brace_depth += 1,
            '}' => {
                brace_depth -= 1;
                if brace_depth < 0 {
                    warnings.push("unmatched closing brace '}'".to_string());
                    brace_depth = 0;
                }
            }
            '(' => paren_depth += 1,
            ')' => {
                paren_depth -= 1;
                if paren_depth < 0 {
                    warnings.push("unmatched closing paren ')'".to_string());
                    paren_depth = 0;
                }
            }
            _ => {}
        }
    }

    if brace_depth != 0 {
        warnings.push(format!(
            "unbalanced braces: {} unclosed '{{'",
            brace_depth.abs()
        ));
    }
    if paren_depth != 0 {
        warnings.push(format!(
            "unbalanced parens: {} unclosed '('",
            paren_depth.abs()
        ));
    }

    if warnings.is_empty() {
        None
    } else {
        Some(warnings.join("; "))
    }
}

pub(in crate::tools::file) fn jsx_balance_warning(file_path: &std::path::Path, content: &str) -> String {
    if matches!(
        file_path.extension().and_then(|e| e.to_str()),
        Some("tsx") | Some("jsx")
    ) {
        check_jsx_balance(content)
            .map(|w| format!("\n[JSX_WARNING] {w} — run tsc to verify"))
            .unwrap_or_default()
    } else {
        String::new()
    }
}
