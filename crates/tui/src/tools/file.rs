//! File system tools: `read_file`, `write_file`, `edit_file`, `list_dir`
//!
//! These tools provide safe file system operations within the workspace,
//! with path validation to prevent escaping the workspace boundary.

use super::diff_format::make_unified_diff;
use super::spec::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec,
    lsp_diagnostics_for_paths, optional_str, optional_u64, required_str,
};
use async_trait::async_trait;
use regex::Regex;
use serde_json::{Value, json};
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::LazyLock;

const MAX_FILE_SIZE: u64 = 100 * 1024 * 1024;
const FILE_SIZE_LINE_COUNT_LIMIT: u64 = 10 * 1024 * 1024;
const DEFAULT_LIMIT: usize = 2000;
const MAX_LIMIT: usize = 5000;

// === ReadFileTool ===

/// Tool for reading UTF-8 files from the workspace.
pub struct ReadFileTool;

#[async_trait]
impl ToolSpec for ReadFileTool {
    fn name(&self) -> &'static str {
        "read_file"
    }

    fn description(&self) -> &'static str {
        "Read a file from the workspace. Plain text uses line paging (start_line or offset + limit) with streaming newline decode (low memory); files starting with UTF-16/UTF-32 BOM use full-file decode. PDFs: `pdftotext` or `pdf-extract`."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file (relative to workspace or absolute)"
                },
                "start_line": {
                    "type": "integer",
                    "description": "First line to read (1-based, default: 1). Preferred over \"offset\"."
                },
                "offset": {
                    "type": "integer",
                    "description": "Alias for start_line (1-based, default: 1). Ignored when \"start_line\" is also set."
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum lines to read (default: 2000, max: 5000)"
                },
                "pages": {
                    "type": "string",
                    "description": "PDF only: page range to extract, e.g. \"1-5\" or \"10\". Ignored for non-PDF files."
                }
            },
            "required": ["path"]
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::ReadOnly, ToolCapability::Sandboxable]
    }

    fn supports_parallel(&self) -> bool {
        true
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let path_str = required_str(&input, "path")?;
        let file_path = context.resolve_path(path_str)?;
        let pages = optional_str(&input, "pages");

        if is_pdf(&file_path)? {
            return read_pdf(&file_path, pages);
        }

        if is_docx(&file_path)? {
            return read_docx(&file_path);
        }

        let start_line = match (
            input.get("start_line").and_then(Value::as_u64),
            input.get("offset").and_then(Value::as_u64),
        ) {
            (Some(s), _) => s.max(1),
            (None, Some(o)) => o.max(1),
            (None, None) => 1,
        };
        let limit =
            optional_u64(&input, "limit", DEFAULT_LIMIT as u64).clamp(1, MAX_LIMIT as u64) as usize;

        let metadata_result = fs::metadata(&file_path);
        let size_bytes = metadata_result.as_ref().ok().map(|m| m.len());

        if size_bytes.is_some_and(|s| s > MAX_FILE_SIZE) {
            return Err(ToolError::execution_failed(format!(
                "[TOO_LARGE] 文件 {} 大小 {} 超过读取上限 ({}MB)",
                file_path.display(),
                size_bytes.unwrap(),
                MAX_FILE_SIZE / 1024 / 1024
            )));
        }

        let sniff_totals = size_bytes.is_some_and(|s| s <= FILE_SIZE_LINE_COUNT_LIMIT);

        let skip = start_line.saturating_sub(1) as usize;

        let (collected, truncated, total_lines_known, encoding_used, encoding_detected_via) =
            if file_needs_bulk_text_decode(&file_path)? {
                let bytes =
                    fs::read(&file_path).map_err(|e| map_plain_read_io_error(&file_path, e))?;
                let (text, encoding_used, encoding_detected_via) = detect_and_decode(&bytes);

                let all_lines: Vec<&str> = text.lines().collect();
                let total_lines_known = sniff_totals.then_some(all_lines.len());

                let end = (skip + limit).min(all_lines.len());
                let collected: Vec<String> = if skip < all_lines.len() {
                    all_lines[skip..end]
                        .iter()
                        .copied()
                        .map(String::from)
                        .collect()
                } else {
                    Vec::new()
                };

                let truncated = skip + collected.len() < all_lines.len();

                (
                    collected,
                    truncated,
                    total_lines_known,
                    encoding_used,
                    encoding_detected_via,
                )
            } else {
                read_plain_lines_stream(&file_path, skip, limit, sniff_totals)
                    .map_err(|e| map_plain_read_io_error(&file_path, e))?
            };

        let mut content = collected.join("\n");

        if truncated && !collected.is_empty() {
            let line_range = format!(
                "第 {}-{} 行",
                start_line,
                start_line + collected.len() as u64 - 1
            );
            let next = start_line + collected.len() as u64;
            if let Some(t) = total_lines_known {
                content.push_str(&format!(
                    "\n\n... ({} 行，共 {} 行; 下一窗口设 start_line={} 或 offset={} 接续)",
                    line_range, t, next, next,
                ));
            } else {
                content.push_str(&format!(
                    "\n\n... ({} 行; 下一窗口设 start_line={} 或 offset={} 接续 — 文件中还有更多行)",
                    line_range, next, next,
                ));
            }
        }

        let mut metadata = json!({
            "path": file_path.to_string_lossy(),
            "lines_read": collected.len(),
            "truncated": truncated,
            "encoding_used": encoding_used,
            "encoding_detected_via": encoding_detected_via,
        });
        if let Some(s) = size_bytes {
            metadata["size_bytes"] = json!(s);
        }
        if let Some(t) = total_lines_known {
            metadata["total_lines_known"] = json!(t);
        }

        Ok(ToolResult::success(content).with_metadata(metadata))
    }
}

/// Detect a PDF by extension OR by sniffing the `%PDF-` magic bytes.
/// Files without an extension are still recognized as PDFs when the header
/// matches.
fn is_pdf(path: &Path) -> Result<bool, ToolError> {
    if path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"))
    {
        return Ok(true);
    }
    // Sniff first 4 bytes. Don't error if the file doesn't exist — let the
    // caller's `read_to_string` produce the canonical not-found error.
    let mut buf = [0u8; 4];
    let result = match fs::File::open(path) {
        Ok(mut f) => {
            use std::io::Read;
            f.read_exact(&mut buf).map(|_| buf)
        }
        Err(_) => return Ok(false),
    };
    Ok(matches!(result, Ok(b) if &b == b"%PDF"))
}

fn parse_pages_arg(spec: &str) -> Option<(u32, u32)> {
    let trimmed = spec.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some((a, b)) = trimmed.split_once('-') {
        let start: u32 = a.trim().parse().ok()?;
        let end: u32 = b.trim().parse().ok()?;
        if start == 0 || end < start {
            return None;
        }
        Some((start, end))
    } else {
        let n: u32 = trimmed.parse().ok()?;
        if n == 0 {
            return None;
        }
        Some((n, n))
    }
}

fn detect_and_decode(bytes: &[u8]) -> (String, String, String) {
    if bytes.is_empty() {
        return (String::new(), "utf-8".into(), "empty".into());
    }

    // 1. BOM detection via encoding_rs
    if let Some((enc, bom_len)) = encoding_rs::Encoding::for_bom(bytes) {
        let (cow, _encoding, _had_errors) = enc.decode(&bytes[bom_len..]);
        let label = enc.name().to_lowercase();
        return (cow.into_owned(), label, "bom".into());
    }

    // 2. Try UTF-8
    if let Ok(text) = std::str::from_utf8(bytes) {
        return (text.to_string(), "utf-8".into(), "default".into());
    }

    // 3. Try GB18030 (covers GBK, common for Chinese users)
    let (cow, _enc, had_errors) = encoding_rs::GB18030.decode(bytes);
    if !had_errors {
        return (cow.into_owned(), "gb18030".into(), "fallback".into());
    }

    // 4. Fallback to Windows-1252 (Latin-1 superset, never fails)
    let (cow, _enc, _had_errors) = encoding_rs::WINDOWS_1252.decode(bytes);
    let label = "windows-1252 (gb18030 had errors)".to_string();
    (cow.into_owned(), label, "fallback".into())
}

/// Best-effort encoding label from a leading slice (for [`file_info`]). Not a guarantee on full-file decode.
pub(super) fn sniff_encoding_label(sample: &[u8]) -> Option<String> {
    if sample.is_empty() {
        return None;
    }
    if let Some((enc, _bom_len)) = encoding_rs::Encoding::for_bom(sample) {
        return Some(enc.name().to_ascii_lowercase());
    }
    if std::str::from_utf8(sample).is_ok() {
        return Some("utf-8".into());
    }
    let (_cow, _, had_errors) = encoding_rs::GB18030.decode(sample);
    if !had_errors {
        return Some("gb18030".into());
    }
    Some("windows-1252-likely".into())
}

#[derive(Clone, Copy)]
enum PhysicalLineEnc {
    Utf8,
    Gb18030,
    Win1252,
}

fn map_plain_read_io_error(path: &Path, e: std::io::Error) -> ToolError {
    let kind = e.kind();
    if kind == std::io::ErrorKind::NotFound {
        ToolError::execution_failed(format!("[NOT_FOUND] 文件 {} 不存在: {e}", path.display()))
    } else if kind == std::io::ErrorKind::PermissionDenied {
        ToolError::execution_failed(format!("[PERMISSION] 没有权限读取 {}: {e}", path.display()))
    } else {
        ToolError::execution_failed(format!("Failed to read {}: {e}", path.display()))
    }
}

/// Returns true when the file begins with a UTF-16 / UTF-32 BOM. Those encodings need a full
/// buffer decode so newlines are interpreted correctly.
fn file_needs_bulk_text_decode(path: &Path) -> Result<bool, ToolError> {
    let mut file = fs::File::open(path).map_err(|e| map_plain_read_io_error(path, e))?;
    let mut probe = [0u8; 4];
    let read = file
        .read(&mut probe)
        .map_err(|e| map_plain_read_io_error(path, e))?;
    if read < 2 {
        return Ok(false);
    }
    if read >= 4
        && (probe.starts_with(&[0xFF, 0xFE, 0x00, 0x00])
            || probe.starts_with(&[0x00, 0x00, 0xFE, 0xFF]))
    {
        return Ok(true);
    }
    if probe.starts_with(&[0xFF, 0xFE]) || probe.starts_with(&[0xFE, 0xFF]) {
        return Ok(true);
    }
    Ok(false)
}

fn trim_line_terminator(mut b: &[u8]) -> &[u8] {
    if b.ends_with(b"\r\n") {
        return &b[..b.len() - 2];
    }
    if let Some(rest) = b.strip_suffix(b"\n") {
        b = rest;
    }
    b.strip_suffix(b"\r").unwrap_or(b)
}

fn decode_physical_line(bytes: &[u8], strip_utf8_bom: bool) -> (String, PhysicalLineEnc) {
    let mut slice = trim_line_terminator(bytes);
    if strip_utf8_bom && slice.len() >= 3 && slice.starts_with(&[0xEF, 0xBB, 0xBF]) {
        slice = &slice[3..];
    }
    if slice.is_empty() {
        return (String::new(), PhysicalLineEnc::Utf8);
    }
    if std::str::from_utf8(slice).is_ok() {
        // Safety: validated above
        return (
            std::str::from_utf8(slice)
                .expect("utf-8 checked")
                .to_string(),
            PhysicalLineEnc::Utf8,
        );
    }
    let (cow_gbk, _, had_errors) = encoding_rs::GB18030.decode(slice);
    if !had_errors {
        return (cow_gbk.into_owned(), PhysicalLineEnc::Gb18030);
    }
    let (cow, _, _) = encoding_rs::WINDOWS_1252.decode(slice);
    (cow.into_owned(), PhysicalLineEnc::Win1252)
}

fn summarize_physical_line_encoding(utf: u64, gbk: u64, win: u64) -> String {
    let kinds = (utf > 0) as u8 + (gbk > 0) as u8 + (win > 0) as u8;
    if kinds <= 1 {
        if gbk > 0 {
            return "gb18030".into();
        }
        if win > 0 {
            return "windows-1252".into();
        }
        return "utf-8".into();
    }
    format!("mixed(utf8_lines={utf}, gb18030_lines={gbk}, windows1252_lines={win})")
}

fn read_plain_lines_stream(
    path: &Path,
    skip: usize,
    limit: usize,
    sniff_totals: bool,
) -> Result<(Vec<String>, bool, Option<usize>, String, String), std::io::Error> {
    let file = fs::File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut buf = Vec::new();
    let mut lineno: u64 = 0;
    let mut out = Vec::new();
    let mut utf = 0u64;
    let mut gbk = 0u64;
    let mut win = 0u64;
    let skip_u64 = skip as u64;

    loop {
        buf.clear();
        let n = reader.read_until(b'\n', &mut buf)?;
        if n == 0 {
            break;
        }
        lineno += 1;
        let (decoded, enc) = decode_physical_line(&buf, lineno == 1);
        match enc {
            PhysicalLineEnc::Utf8 => utf += 1,
            PhysicalLineEnc::Gb18030 => gbk += 1,
            PhysicalLineEnc::Win1252 => win += 1,
        }
        if lineno <= skip_u64 {
            continue;
        }
        if out.len() < limit {
            out.push(decoded);
        }
    }

    let eligible = lineno.saturating_sub(skip_u64);
    let truncated = eligible > limit as u64;
    let total_lines_known = sniff_totals.then_some(lineno as usize);
    let encoding_used = summarize_physical_line_encoding(utf, gbk, win);
    Ok((
        out,
        truncated,
        total_lines_known,
        encoding_used,
        "streaming-line".into(),
    ))
}

static DOCX_WT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<w:t[^>]*>(.*?)</w:t>").unwrap());

fn is_docx(path: &Path) -> Result<bool, ToolError> {
    // Extension-only: many ZIP formats share the PK header; `.docx` is unambiguous enough.
    Ok(path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("docx")))
}

fn read_docx(path: &Path) -> Result<ToolResult, ToolError> {
    let size_bytes = fs::metadata(path).map(|m| m.len()).ok();
    let file = fs::File::open(path).map_err(|e| {
        ToolError::execution_failed(format!(
            "[NOT_FOUND] 无法打开 DOCX 文件 {}: {e}",
            path.display()
        ))
    })?;

    let mut archive = zip::ZipArchive::new(file).map_err(|e| {
        ToolError::execution_failed(format!(
            "[BINARY] 无法解析 DOCX/ZIP {}: {e}",
            path.display()
        ))
    })?;

    let mut doc_xml = String::new();
    match archive.by_name("word/document.xml") {
        Ok(mut entry) => {
            entry.read_to_string(&mut doc_xml).map_err(|e| {
                ToolError::execution_failed(format!(
                    "Failed to read word/document.xml from {}: {e}",
                    path.display()
                ))
            })?;
        }
        Err(e) => {
            return Err(ToolError::execution_failed(format!(
                "[BINARY] word/document.xml not found in {}: {e}",
                path.display()
            )));
        }
    }

    let mut result = String::new();

    for para in doc_xml.split("</w:p>") {
        let mut line = String::new();
        for cap in DOCX_WT_RE.captures_iter(para) {
            if let Some(m) = cap.get(1) {
                line.push_str(m.as_str());
            }
        }
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            if !result.is_empty() {
                result.push('\n');
            }
            result.push_str(trimmed);
        }
    }

    if result.is_empty() {
        return Ok(
            ToolResult::success("[DOCX] 文件内容为空或仅包含非文本元素。").with_metadata(json!({
                "path": path.to_string_lossy(),
                "kind": "docx",
                "size_bytes": size_bytes,
            })),
        );
    }

    Ok(ToolResult::success(result).with_metadata(json!({
        "path": path.to_string_lossy(),
        "kind": "docx",
        "size_bytes": size_bytes,
    })))
}

fn read_pdf(path: &Path, pages: Option<&str>) -> Result<ToolResult, ToolError> {
    let size_bytes = fs::metadata(path).map(|m| m.len()).ok();

    let mut cmd = Command::new("pdftotext");
    cmd.arg("-layout");

    let valid_pages = if let Some(spec) = pages {
        match parse_pages_arg(spec) {
            Some(range) => {
                cmd.arg("-f").arg(range.0.to_string());
                cmd.arg("-l").arg(range.1.to_string());
                Some(range)
            }
            None => {
                return Err(ToolError::invalid_input(format!(
                    "invalid `pages` value `{spec}` (expected `N` or `N-M`, e.g. `1-5`)"
                )));
            }
        }
    } else {
        None
    };

    cmd.arg(path).arg("-");
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    match cmd.spawn() {
        Ok(child) => {
            let output = child.wait_with_output().map_err(|e| {
                ToolError::execution_failed(format!("pdftotext failed to complete: {e}"))
            })?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                return Err(ToolError::execution_failed(format!(
                    "pdftotext failed (exit {:?}): {stderr}",
                    output.status.code()
                )));
            }

            let text = String::from_utf8_lossy(&output.stdout).to_string();
            let mut metadata = json!({
                "path": path.to_string_lossy(),
                "kind": "pdf",
                "extractor": "pdftotext",
                "size_bytes": size_bytes,
            });
            if let Some(range) = valid_pages {
                metadata["pages"] = json!(format!("{}-{}", range.0, range.1));
            }
            return Ok(ToolResult::success(text).with_metadata(metadata));
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // Fall through to pdf-extract fallback
        }
        Err(e) => {
            return Err(ToolError::execution_failed(format!(
                "failed to launch pdftotext: {e}"
            )));
        }
    }

    // pdf-extract fallback: pure Rust, no system dependency
    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            return ToolResult::json(&json!({
                "type": "binary_unavailable",
                "path": path.display().to_string(),
                "kind": "pdf",
                "reason": "pdftotext not installed and failed to read file for pdf-extract",
                "detail": e.to_string(),
                "hint": "install poppler for better PDF support (macOS: `brew install poppler`; Debian/Ubuntu: `apt install poppler-utils`)"
            }))
            .map_err(|e| ToolError::execution_failed(format!("failed to serialize response: {e}")));
        }
    };

    let text = match pdf_extract::extract_text_from_mem(&bytes) {
        Ok(t) => t,
        Err(e) => {
            return ToolResult::json(&json!({
                "type": "binary_unavailable",
                "path": path.display().to_string(),
                "kind": "pdf",
                "reason": "pdftotext not installed and pdf-extract failed",
                "detail": e.to_string(),
                "hint": "install poppler for better PDF support (macOS: `brew install poppler`; Debian/Ubuntu: `apt install poppler-utils`)"
            }))
            .map_err(|e| ToolError::execution_failed(format!("failed to serialize response: {e}")));
        }
    };

    if text.trim().is_empty() {
        return ToolResult::json(&json!({
            "type": "binary_unavailable",
            "path": path.display().to_string(),
            "kind": "pdf",
            "reason": "pdf-extract returned empty text — the PDF may be scanned, encrypted, or uses unsupported features",
            "hint": "install poppler for better PDF support (macOS: `brew install poppler`; Debian/Ubuntu: `apt install poppler-utils`)"
        }))
        .map_err(|e| ToolError::execution_failed(format!("failed to serialize response: {e}")));
    }

    let note = if valid_pages.is_some() {
        "\n\n[注意: pdf-extract 不支持分页，已返回全文。安装 poppler 可启用 --pages 功能。]\n"
    } else {
        ""
    };

    let mut metadata = json!({
        "path": path.to_string_lossy(),
        "kind": "pdf",
        "extractor": "pdf-extract",
        "fallback_from_missing_pdftotext": true,
        "size_bytes": size_bytes,
    });
    if valid_pages.is_some() {
        metadata["pdf_extract_pages_note"] =
            json!("pages only apply when pdftotext is installed; full document returned")
    }

    Ok(ToolResult::success(format!("{note}{text}")).with_metadata(metadata))
}

// === WriteFileTool ===

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

// === EditFileTool ===

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
                }
            },
            "required": ["path", "search", "replace"]
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
        let search = required_str(&input, "search")?;
        let replace = required_str(&input, "replace")?;

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

        let count = contents.matches(search).count();
        if count == 0 {
            return Err(ToolError::execution_failed(format!(
                "Search string not found in {}",
                file_path.display()
            )));
        }

        let updated = contents.replace(search, replace);

        fs::write(&file_path, &updated).map_err(|e| {
            ToolError::execution_failed(format!("Failed to write {}: {}", file_path.display(), e))
        })?;

        let display = file_path.display().to_string();
        let diff = make_unified_diff(&display, &contents, &updated);
        let summary = format!("Replaced {count} occurrence(s) in {display}");
        let body = if diff.is_empty() {
            format!("{summary}\n(no textual changes)")
        } else {
            format!("{diff}\n{summary}")
        };

        // Append LSP diagnostics for the edited file when enabled (#428).
        let diag_block = lsp_diagnostics_for_paths(context, &[file_path]).await;
        let full_body = if diag_block.is_empty() {
            body
        } else {
            format!("{body}\n{diag_block}")
        };

        Ok(ToolResult::success(full_body))
    }
}

// === ListDirTool ===

/// Tool for listing directory contents.
pub struct ListDirTool;

#[async_trait]
impl ToolSpec for ListDirTool {
    fn name(&self) -> &'static str {
        "list_dir"
    }

    fn description(&self) -> &'static str {
        "List entries in a directory relative to the workspace."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Relative path (default: .)"
                }
            },
            "required": []
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::ReadOnly, ToolCapability::Sandboxable]
    }

    fn supports_parallel(&self) -> bool {
        true
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let path_str = optional_str(&input, "path").unwrap_or(".");
        let dir_path = context.resolve_path(path_str)?;

        let mut entries = Vec::new();

        for entry in fs::read_dir(&dir_path).map_err(|e| {
            ToolError::execution_failed(format!(
                "Failed to read directory {}: {}",
                dir_path.display(),
                e
            ))
        })? {
            let entry = entry.map_err(|e| ToolError::execution_failed(e.to_string()))?;
            let file_type = entry
                .file_type()
                .map_err(|e| ToolError::execution_failed(e.to_string()))?;

            entries.push(json!({
                "name": entry.file_name().to_string_lossy().to_string(),
                "is_dir": file_type.is_dir(),
            }));
        }

        ToolResult::json(&entries).map_err(|e| ToolError::execution_failed(e.to_string()))
    }
}

// === Unit Tests ===

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_read_file_tool() {
        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path().to_path_buf());

        // Create a test file
        let test_file = tmp.path().join("test.txt");
        fs::write(&test_file, "hello world").expect("write");

        let tool = ReadFileTool;
        let result = tool
            .execute(json!({"path": "test.txt"}), &ctx)
            .await
            .expect("execute");

        assert!(result.success);
        assert_eq!(result.content, "hello world");

        let md = result.metadata.as_ref().expect("metadata");
        assert_eq!(md["encoding_detected_via"], "streaming-line");
    }

    #[tokio::test]
    async fn test_read_file_not_found() {
        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path().to_path_buf());

        let tool = ReadFileTool;
        let result = tool.execute(json!({"path": "nonexistent.txt"}), &ctx).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_read_file_missing_path() {
        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path().to_path_buf());

        let tool = ReadFileTool;
        let result = tool.execute(json!({}), &ctx).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string()
                .contains("Failed to validate input: missing required field 'path'")
        );
    }

    #[test]
    fn pdf_detected_by_extension() {
        let tmp = tempdir().expect("tempdir");
        let path = tmp.path().join("paper.PDF");
        fs::write(&path, b"not really a pdf, but extension says yes").unwrap();
        assert!(is_pdf(&path).unwrap());
    }

    #[test]
    fn pdf_detected_by_magic_bytes_without_extension() {
        let tmp = tempdir().expect("tempdir");
        let path = tmp.path().join("blob");
        fs::write(&path, b"%PDF-1.7\nrest of bytes").unwrap();
        assert!(is_pdf(&path).unwrap());
    }

    #[test]
    fn non_pdf_not_detected() {
        let tmp = tempdir().expect("tempdir");
        let path = tmp.path().join("notes.txt");
        fs::write(&path, "hello").unwrap();
        assert!(!is_pdf(&path).unwrap());
    }

    #[test]
    fn pages_arg_parses_single_and_range() {
        assert_eq!(parse_pages_arg("5"), Some((5, 5)));
        assert_eq!(parse_pages_arg("1-10"), Some((1, 10)));
        assert_eq!(parse_pages_arg(" 3 - 7 "), Some((3, 7)));
        assert_eq!(parse_pages_arg("0"), None);
        assert_eq!(parse_pages_arg("10-3"), None);
        assert_eq!(parse_pages_arg(""), None);
        assert_eq!(parse_pages_arg("abc"), None);
    }

    #[tokio::test]
    async fn read_file_offset_alias_matches_start_line_precedence() {
        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path().to_path_buf());

        let test_file = tmp.path().join("lines.txt");
        fs::write(&test_file, "a\nb\nc").expect("write");

        let by_offset = ReadFileTool
            .execute(json!({"path": "lines.txt", "offset": 2}), &ctx)
            .await
            .expect("execute");

        assert_eq!(by_offset.content, "b\nc");

        let by_start_line = ReadFileTool
            .execute(json!({"path": "lines.txt", "start_line": 2}), &ctx)
            .await
            .expect("execute");

        assert_eq!(by_start_line.content, "b\nc");

        let start_line_wins = ReadFileTool
            .execute(
                json!({"path": "lines.txt", "start_line": 1, "offset": 3}),
                &ctx,
            )
            .await
            .expect("execute");

        assert!(
            start_line_wins.content.starts_with('a'),
            "{}",
            start_line_wins.content
        );
    }

    #[tokio::test]
    async fn read_file_exact_window_to_eof_without_trunc_notice_when_total_unknown() {
        // Large files skip total_lines_known — ensure we don't emit a truncation
        // footer when we read exactly to EOF (within the small-file line-count budget).

        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path().to_path_buf());

        let lines: Vec<String> = (1..=800).map(|i| format!("line{i}")).collect();
        let content = lines.join("\n");
        assert!(content.len() < FILE_SIZE_LINE_COUNT_LIMIT as usize);
        let test_file = tmp.path().join("exact.txt");
        fs::write(&test_file, &content).expect("write");

        let result = ReadFileTool
            .execute(json!({"path": "exact.txt", "limit": 800}), &ctx)
            .await
            .expect("execute");

        assert!(result.success);
        assert!(!result.content.contains("接续"), "{}", result.content);
        let metadata = result.metadata.expect("metadata");
        assert!(!metadata["truncated"].as_bool().unwrap());
    }

    #[tokio::test]
    async fn read_file_returns_binary_unavailable_when_pdftotext_missing_and_pdf_extract_fails() {
        // When pdftotext is missing and pdf-extract cannot parse the PDF,
        // a structured binary_unavailable response is returned.
        if Command::new("pdftotext")
            .arg("-v")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok()
        {
            return;
        }
        let tmp = tempdir().expect("tempdir");
        let path = tmp.path().join("doc.pdf");
        fs::write(&path, b"%PDF-1.7\n%%EOF").unwrap();
        let ctx = ToolContext::new(tmp.path().to_path_buf());
        let result = ReadFileTool
            .execute(json!({"path": "doc.pdf"}), &ctx)
            .await
            .expect("structured response, not error");
        assert!(result.success);
        assert!(result.content.contains("binary_unavailable"));
    }

    #[tokio::test]
    async fn test_write_file_tool() {
        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path().to_path_buf());

        let tool = WriteFileTool;
        let result = tool
            .execute(
                json!({"path": "output.txt", "content": "test content"}),
                &ctx,
            )
            .await
            .expect("execute");

        assert!(result.success);
        // New file → "Created …" summary; the unified diff above the summary
        // primes the TUI's diff-aware renderer (#505).
        assert!(result.content.contains("Created"), "{}", result.content);
        assert!(result.content.contains("--- a/"), "{}", result.content);
        assert!(
            result.content.contains("+test content"),
            "{}",
            result.content
        );

        // Verify file was written
        let written = fs::read_to_string(tmp.path().join("output.txt")).expect("read");
        assert_eq!(written, "test content");
    }

    #[tokio::test]
    async fn test_write_file_creates_dirs() {
        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path().to_path_buf());

        let tool = WriteFileTool;
        let result = tool
            .execute(
                json!({"path": "subdir/nested/file.txt", "content": "nested content"}),
                &ctx,
            )
            .await
            .expect("execute");

        assert!(result.success);

        // Verify nested file was created
        let written = fs::read_to_string(tmp.path().join("subdir/nested/file.txt")).expect("read");
        assert_eq!(written, "nested content");
    }

    #[tokio::test]
    async fn test_edit_file_tool() {
        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path().to_path_buf());

        // Create a file to edit
        let test_file = tmp.path().join("edit_me.txt");
        fs::write(&test_file, "hello world hello").expect("write");

        let tool = EditFileTool;
        let result = tool
            .execute(
                json!({"path": "edit_me.txt", "search": "hello", "replace": "hi"}),
                &ctx,
            )
            .await
            .expect("execute");

        assert!(result.success);
        assert!(result.content.contains("2 occurrence(s)"));
        // Inline diff (#505) — the unified diff lands above the summary
        // line so the TUI's diff-aware renderer kicks in.
        assert!(result.content.contains("--- a/"), "{}", result.content);
        assert!(
            result.content.contains("-hello world hello"),
            "{}",
            result.content
        );
        assert!(
            result.content.contains("+hi world hi"),
            "{}",
            result.content
        );

        // Verify edit was applied
        let edited = fs::read_to_string(&test_file).expect("read");
        assert_eq!(edited, "hi world hi");
    }

    #[tokio::test]
    async fn test_edit_file_not_found() {
        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path().to_path_buf());

        // Create a file without the search string
        let test_file = tmp.path().join("no_match.txt");
        fs::write(&test_file, "foo bar baz").expect("write");

        let tool = EditFileTool;
        let result = tool
            .execute(
                json!({"path": "no_match.txt", "search": "hello", "replace": "hi"}),
                &ctx,
            )
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    /// #157 — When the model uses `replacement` instead of `replace`,
    /// the error should name the provided fields so the model can
    /// self-correct without a second round-trip.
    #[tokio::test]
    async fn test_edit_file_wrong_param_name_shows_provided_fields() {
        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path().to_path_buf());

        let test_file = tmp.path().join("test.txt");
        fs::write(&test_file, "hello world").expect("write");

        let tool = EditFileTool;
        // Model uses `replacement` instead of `replace`.
        let result = tool
            .execute(
                json!({"path": "test.txt", "search": "hello", "replacement": "hi"}),
                &ctx,
            )
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        // The error must name both the missing field AND the provided ones.
        assert!(
            err.contains("missing required field 'replace'"),
            "error must name the missing field: {err}"
        );
        assert!(
            err.contains("Input provided:") || err.contains("provided:"),
            "error must list the fields the model did supply: {err}"
        );
    }

    #[tokio::test]
    async fn test_list_dir_tool() {
        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path().to_path_buf());

        // Create some files and directories
        fs::write(tmp.path().join("file1.txt"), "").expect("write");
        fs::write(tmp.path().join("file2.txt"), "").expect("write");
        fs::create_dir(tmp.path().join("subdir")).expect("mkdir");

        let tool = ListDirTool;
        let result = tool.execute(json!({}), &ctx).await.expect("execute");

        assert!(result.success);
        assert!(result.content.contains("file1.txt"));
        assert!(result.content.contains("file2.txt"));
        assert!(result.content.contains("subdir"));
        assert!(result.content.contains("\"is_dir\": true"));
    }

    #[tokio::test]
    async fn test_list_dir_with_path() {
        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path().to_path_buf());

        // Create a subdirectory with files
        let subdir = tmp.path().join("mydir");
        fs::create_dir(&subdir).expect("mkdir");
        fs::write(subdir.join("nested.txt"), "").expect("write");

        let tool = ListDirTool;
        let result = tool
            .execute(json!({"path": "mydir"}), &ctx)
            .await
            .expect("execute");

        assert!(result.success);
        assert!(result.content.contains("nested.txt"));
    }

    #[test]
    fn test_read_file_tool_properties() {
        let tool = ReadFileTool;
        assert_eq!(tool.name(), "read_file");
        assert!(tool.is_read_only());
        assert!(tool.is_sandboxable());
        assert_eq!(tool.approval_requirement(), ApprovalRequirement::Auto);
    }

    #[test]
    fn test_write_file_tool_properties() {
        let tool = WriteFileTool;
        assert_eq!(tool.name(), "write_file");
        assert!(!tool.is_read_only());
        assert!(tool.is_sandboxable());
        assert_eq!(tool.approval_requirement(), ApprovalRequirement::Suggest);
    }

    #[test]
    fn test_edit_file_tool_properties() {
        let tool = EditFileTool;
        assert_eq!(tool.name(), "edit_file");
        assert!(!tool.is_read_only());
        assert!(tool.is_sandboxable());
        assert_eq!(tool.approval_requirement(), ApprovalRequirement::Suggest);
    }

    #[test]
    fn test_list_dir_tool_properties() {
        let tool = ListDirTool;
        assert_eq!(tool.name(), "list_dir");
        assert!(tool.is_read_only());
        assert!(tool.is_sandboxable());
        assert_eq!(tool.approval_requirement(), ApprovalRequirement::Auto);
    }

    #[test]
    fn test_parallel_support_flags() {
        let read_tool = ReadFileTool;
        let list_tool = ListDirTool;
        let write_tool = WriteFileTool;

        assert!(read_tool.supports_parallel());
        assert!(list_tool.supports_parallel());
        assert!(!write_tool.supports_parallel());
    }

    #[test]
    fn test_input_schemas() {
        // Verify all tools have valid JSON schemas
        let read_schema = ReadFileTool.input_schema();
        assert!(read_schema.get("type").is_some());
        let props = read_schema
            .get("properties")
            .and_then(|v| v.as_object())
            .expect("read schema should have properties");
        assert!(props.contains_key("path"));
        assert!(props.contains_key("start_line"));
        assert!(props.contains_key("offset"));
        assert!(props.contains_key("limit"));
        assert!(props.contains_key("pages"));

        let write_schema = WriteFileTool.input_schema();
        let required = write_schema
            .get("required")
            .and_then(|value| value.as_array())
            .expect("write schema should include required array");
        assert!(required.iter().any(|v| v.as_str() == Some("path")));
        assert!(required.iter().any(|v| v.as_str() == Some("content")));

        let edit_schema = EditFileTool.input_schema();
        let required = edit_schema
            .get("required")
            .and_then(|value| value.as_array())
            .expect("edit schema should include required array");
        assert_eq!(required.len(), 3);

        let list_schema = ListDirTool.input_schema();
        let required = list_schema
            .get("required")
            .and_then(|value| value.as_array())
            .expect("list schema should include required array");
        assert!(required.is_empty()); // path is optional
    }

    #[tokio::test]
    async fn read_file_start_line_skips_leading_lines() {
        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path().to_path_buf());

        let test_file = tmp.path().join("multiline.txt");
        fs::write(&test_file, "line1\nline2\nline3\nline4\nline5").expect("write");

        let result = ReadFileTool
            .execute(json!({"path": "multiline.txt", "start_line": 3}), &ctx)
            .await
            .expect("execute");

        assert!(result.success);
        assert_eq!(result.content, "line3\nline4\nline5");

        let metadata = result.metadata.expect("should have metadata");
        assert_eq!(metadata["lines_read"], 3);
        assert_eq!(metadata["total_lines_known"], 5);
        assert!(!metadata["truncated"].as_bool().unwrap());
    }

    #[tokio::test]
    async fn read_file_limit_truncates_with_notice() {
        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path().to_path_buf());

        let lines: Vec<String> = (1..=50).map(|i| format!("line{i}")).collect();
        let content = lines.join("\n");
        let test_file = tmp.path().join("many_lines.txt");
        fs::write(&test_file, &content).expect("write");

        let result = ReadFileTool
            .execute(
                json!({"path": "many_lines.txt", "start_line": 1, "limit": 10}),
                &ctx,
            )
            .await
            .expect("execute");

        assert!(result.success);
        assert!(result.content.contains("line1\nline2"));
        assert!(result.content.contains("line10"));
        assert!(!result.content.contains("line11"));
        assert!(result.content.contains("..."));
        assert!(result.content.contains("共 50 行"));

        let metadata = result.metadata.expect("should have metadata");
        assert_eq!(metadata["lines_read"], 10);
        assert_eq!(metadata["total_lines_known"], 50);
        assert!(metadata["truncated"].as_bool().unwrap());
    }

    #[tokio::test]
    async fn read_file_metadata_includes_path_and_size() {
        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path().to_path_buf());

        let test_file = tmp.path().join("meta_test.txt");
        let body = "hello metadata test";
        fs::write(&test_file, body).expect("write");

        let result = ReadFileTool
            .execute(json!({"path": "meta_test.txt"}), &ctx)
            .await
            .expect("execute");

        assert!(result.success);
        let metadata = result.metadata.expect("should have metadata");
        assert!(metadata["path"].as_str().unwrap().contains("meta_test.txt"));
        assert!(metadata["size_bytes"].as_u64().unwrap() > 0);
        assert_eq!(metadata["lines_read"], 1);
        assert!(!metadata["truncated"].as_bool().unwrap());
    }

    #[tokio::test]
    async fn read_file_start_line_past_end_returns_empty_with_metadata() {
        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path().to_path_buf());

        let test_file = tmp.path().join("short.txt");
        fs::write(&test_file, "only two\nlines here").expect("write");

        let result = ReadFileTool
            .execute(json!({"path": "short.txt", "start_line": 10}), &ctx)
            .await
            .expect("execute");

        assert!(result.success);
        assert!(result.content.is_empty());

        let metadata = result.metadata.expect("should have metadata");
        assert_eq!(metadata["lines_read"], 0);
        assert_eq!(metadata["total_lines_known"], 2);
    }
}
