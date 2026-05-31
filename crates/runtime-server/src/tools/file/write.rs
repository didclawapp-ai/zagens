//! write_file tool and shared edit/write helpers.

use super::read::detect_and_decode;
use super::{DIFF_MAX_INPUT_BYTES, MAX_WRITE_SIZE, WRITE_PREVIEW_LINES};
use crate::tools::diff_format::make_unified_diff;
use crate::tools::spec::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec,
    lsp_diagnostics_for_paths, required_str,
};
use async_trait::async_trait;
use serde_json::{Value, json};
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

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
        let requested_content = required_str(&input, "content")?;

        // 3.3: guard against runaway / accidental huge writes (mirror read_file).
        if requested_content.len() > MAX_WRITE_SIZE {
            return Err(ToolError::execution_failed(format!(
                "[TOO_LARGE] 写入内容 {} 字节超过上限 ({}MB)，请分块写入或改用 edit_file 增量补全",
                requested_content.len(),
                MAX_WRITE_SIZE / 1024 / 1024
            )));
        }

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

        // 2.2: snapshot prior contents with encoding-tolerant decode (GB18030 /
        // UTF-16), so the diff/summary doesn't pretend an existing file is empty.
        // Also capture the original encoding so an overwrite preserves it (C8)
        // rather than silently transcoding e.g. GB18030 → UTF-8.
        let existed_before = file_path.exists();
        let (prior_contents, prior_label, prior_had_bom) = if existed_before {
            match fs::read(&file_path) {
                Ok(bytes) => {
                    let (text, label, via) = detect_and_decode(&bytes);
                    (text, label, via == "bom")
                }
                Err(_) => (String::new(), "utf-8".to_string(), false),
            }
        } else {
            (String::new(), "utf-8".to_string(), false)
        };

        // 2.1: preserve the file's original line endings on overwrite. The model
        // almost always sends LF; blindly writing it would flip a CRLF file to LF
        // and produce a whole-file git diff on Windows.
        let file_le = if existed_before && prior_contents.contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        };
        let file_content = if file_le == "\r\n" {
            normalize_line_endings(requested_content, file_le)
        } else {
            requested_content.to_string()
        };

        // 2.7: skip the write entirely when the (normalized) content is identical
        // to what's on disk — avoids touching mtime / triggering file watchers.
        if existed_before && file_content == prior_contents {
            let display = file_path.display().to_string();
            return Ok(ToolResult::success(format!(
                "{display}\n(no changes — content identical, write skipped)"
            )));
        }

        // Create parent directories if needed.
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| map_write_io_error(parent, e))?;
        }

        // 2.4: atomic write — temp file in the same dir + rename, so an
        // interrupted/failed write never leaves a truncated original behind.
        // C8: re-encode in the file's original encoding when it wasn't UTF-8.
        let encoded = encode_text(&file_content, &prior_label, prior_had_bom);
        atomic_write(&file_path, &encoded).map_err(|e| map_write_io_error(&file_path, e))?;

        let display = file_path.display().to_string();
        let byte_len = encoded.len();
        let line_count = file_content.lines().count();

        // 3.1 / 3.2: only LARGE inputs skip the full unified diff (which would
        // echo the whole file back / cost O(N·D)). Small writes — including new
        // files — keep the real diff so the web-ui DiffCard and 「本轮变更」panel
        // render them exactly as before (avoids a frontend regression).
        let large_input =
            file_content.len() > DIFF_MAX_INPUT_BYTES || prior_contents.len() > DIFF_MAX_INPUT_BYTES;
        let verb = if existed_before { "Wrote" } else { "Created" };
        let body = if large_input {
            // 3.4: echo size/lines so the model can self-check truncation.
            format!(
                "{verb} {byte_len} bytes ({line_count} lines) to {display}\n\
                [diff omitted — large file; showing head preview]\n{}",
                preview_block(&file_content, WRITE_PREVIEW_LINES, line_count)
            )
        } else {
            let diff = make_unified_diff(&display, &prior_contents, &file_content);
            let summary = format!("{verb} {byte_len} bytes ({line_count} lines) to {display}");
            if diff.is_empty() {
                format!("{summary}\n(no textual changes)")
            } else {
                format!("{diff}\n{summary}")
            }
        };

        // 2.3 / 3.4: balance check — for .tsx/.jsx flags unbalanced JSX, and for
        // any file a gross brace imbalance is a likely truncation signal.
        let balance_warning = balance_warning(&file_path, &file_content);

        // Append LSP diagnostics for the written file when enabled (#428).
        let diag_block = lsp_diagnostics_for_paths(context, &[file_path]).await;
        let full_body = if diag_block.is_empty() {
            format!("{body}{balance_warning}")
        } else {
            format!("{body}{balance_warning}\n{diag_block}")
        };

        Ok(ToolResult::success(full_body))
    }
}

/// A file decoded for in-place editing, carrying enough metadata to write the
/// result back in the *same* encoding instead of silently transcoding to UTF-8
/// (C8). `label` is the encoding name from [`detect_and_decode`]; `had_bom`
/// records whether a BOM was present so it can be restored.
pub(crate) struct DecodedFile {
    pub text: String,
    pub label: String,
    pub had_bom: bool,
}

/// Read + decode a file for the edit/fim paths. Mirrors the `[NOT_FOUND]` /
/// `[PERMISSION]` diagnostic style used elsewhere and, unlike `read_to_string`,
/// tolerates GB18030 / UTF-16 so those files can be edited rather than rejected.
pub(crate) fn read_decoded_for_edit(path: &Path) -> Result<DecodedFile, ToolError> {
    let bytes = fs::read(path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => {
            ToolError::execution_failed(format!("[NOT_FOUND] 文件 {} 不存在: {e}", path.display()))
        }
        std::io::ErrorKind::PermissionDenied => ToolError::execution_failed(format!(
            "[PERMISSION] 没有权限读取 {}: {e}",
            path.display()
        )),
        _ => ToolError::execution_failed(format!("Failed to read {}: {e}", path.display())),
    })?;
    let (text, label, via) = detect_and_decode(&bytes);
    Ok(DecodedFile {
        text,
        label,
        had_bom: via == "bom",
    })
}

/// Re-encode `text` to bytes using the encoding `label` produced by
/// [`detect_and_decode`], restoring a BOM when `had_bom` is true. Used by the
/// write/edit/fim paths to preserve a file's original encoding (C8). Unknown
/// labels fall back to UTF-8, which is the safe default.
pub(crate) fn encode_text(text: &str, label: &str, had_bom: bool) -> Vec<u8> {
    let norm = label.trim().to_ascii_lowercase();
    match norm.as_str() {
        "utf-8" => {
            if had_bom {
                let mut out = vec![0xEF, 0xBB, 0xBF];
                out.extend_from_slice(text.as_bytes());
                out
            } else {
                text.as_bytes().to_vec()
            }
        }
        "utf-16le" => encode_utf16(text, true, had_bom),
        "utf-16be" => encode_utf16(text, false, had_bom),
        "gb18030" => encoding_rs::GB18030.encode(text).0.into_owned(),
        _ if norm.starts_with("windows-1252") => {
            encoding_rs::WINDOWS_1252.encode(text).0.into_owned()
        }
        // Unknown / unsupported (encoding_rs has no UTF-16 *encoder* beyond the
        // manual path above): default to UTF-8 rather than risk a lossy round-trip.
        _ => text.as_bytes().to_vec(),
    }
}

/// Manually encode `text` as UTF-16 (encoding_rs deliberately has no UTF-16
/// encoder), optionally prefixing the byte-order mark.
fn encode_utf16(text: &str, little_endian: bool, had_bom: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() * 2 + 2);
    if had_bom {
        out.extend_from_slice(if little_endian {
            &[0xFF, 0xFE]
        } else {
            &[0xFE, 0xFF]
        });
    }
    for unit in text.encode_utf16() {
        if little_endian {
            out.extend_from_slice(&unit.to_le_bytes());
        } else {
            out.extend_from_slice(&unit.to_be_bytes());
        }
    }
    out
}

/// Localized IO-error mapping for writes, matching the `[NOT_FOUND]` /
/// `[PERMISSION]` diagnostic style used by `read_file` / `edit_file`.
fn map_write_io_error(path: &Path, e: std::io::Error) -> ToolError {
    match e.kind() {
        std::io::ErrorKind::PermissionDenied => ToolError::execution_failed(format!(
            "[PERMISSION] 没有权限写入 {}: {e}",
            path.display()
        )),
        std::io::ErrorKind::NotFound => ToolError::execution_failed(format!(
            "[NOT_FOUND] 路径不存在，无法写入 {}: {e}",
            path.display()
        )),
        _ => ToolError::execution_failed(format!("写入 {} 失败: {e}", path.display())),
    }
}

static ATOMIC_WRITE_SEQ: AtomicU64 = AtomicU64::new(0);

/// Write `bytes` to `path` atomically: write to a temp file in the same
/// directory, then rename over the target. `fs::rename` replaces an existing
/// destination on all supported platforms (incl. Windows).
pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty());
    let dir = parent.unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "out".to_string());
    let seq = ATOMIC_WRITE_SEQ.fetch_add(1, Ordering::Relaxed);
    let tmp = dir.join(format!(".{file_name}.tmp.{}.{seq}", std::process::id()));

    let result = (|| {
        fs::write(&tmp, bytes)?;
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// Build a head-only preview (first `max_lines` lines) for cases where the full
/// diff is intentionally skipped (large files). `total_lines` is passed in by
/// the caller to avoid a second full O(n) scan of large content.
fn preview_block(content: &str, max_lines: usize, total_lines: usize) -> String {
    // NOTE: must not start with "--- " / "+++ " / "@@" — the web-ui's
    // `looksLikeDiff` would otherwise misrender this preview as a unified diff.
    let mut out = String::from("=== preview (head) ===\n");
    for (i, line) in content.lines().take(max_lines).enumerate() {
        out.push_str(&format!("{:>5} | {}\n", i + 1, line));
    }
    if total_lines > max_lines {
        out.push_str(&format!("... ({} more lines)\n", total_lines - max_lines));
    }
    out
}

/// Balance warning for written content. For `.tsx`/`.jsx` reuses the JSX check;
/// for other text it flags a gross brace imbalance as a likely truncation signal.
fn balance_warning(file_path: &Path, content: &str) -> String {
    let jsx = jsx_balance_warning(file_path, content);
    if !jsx.is_empty() {
        return jsx;
    }
    // Generic truncation heuristic — restricted to languages where the brace
    // counter is reliable. Rust/Python are excluded because `'` (lifetimes,
    // char literals, quotes) confuses the simple string-state scanner.
    let safe_ext = matches!(
        file_path.extension().and_then(|e| e.to_str()),
        Some("json" | "js" | "ts" | "css" | "html" | "scss")
    );
    if safe_ext
        && content.len() >= 200
        && let Some(w) = check_jsx_balance(content)
        && w.contains("unclosed")
    {
        return format!(
            "\n[TRUNCATION_SUSPECTED] {w} — 内容可能在生成时被截断，请核对文件是否完整"
        );
    }
    String::new()
}

/// Normalize text line-endings to match the file's actual format.
/// When the file uses CRLF, converts `\n` → `\r\n` in the provided text.
pub(crate) fn normalize_line_endings(text: &str, file_le: &str) -> String {
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
