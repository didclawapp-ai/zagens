//! File system tools: `read_file`, `write_file`, `edit_file`, `list_dir`
//!
//! These tools provide safe file system operations within the workspace,
//! with path validation to prevent escaping the workspace boundary.

use super::diff_format::make_unified_diff;
use super::spec::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec,
    lsp_diagnostics_for_paths, optional_bool, optional_str, optional_u64, required_str,
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
        "Read a file from the workspace. Plain text uses line paging (start_line or offset + limit) with streaming newline decode (low memory); files starting with UTF-16/UTF-32 BOM use full-file decode. PDFs: `pdftotext` or `pdf-extract`. DOCX/XLSX/PPTX: extracts text from OOXML ZIP."
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

        if is_xlsx(&file_path)? {
            return read_xlsx(&file_path);
        }

        if is_pptx(&file_path)? {
            return read_pptx(&file_path);
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

        // CRAFT P3: prepend file structure summary for large files (>500 lines).
        if let Some(total) = total_lines_known
            && total >= 500
        {
            let rel = file_path
                .strip_prefix(&context.workspace)
                .unwrap_or(&file_path)
                .to_string_lossy()
                .replace('\\', "/");
            let index_path = context.workspace.join(".deepseek").join("symbols.json");
            if let Ok(raw) = std::fs::read_to_string(&index_path) {
                if let Ok(index) = serde_json::from_str::<crate::symbol_index::SymbolIndex>(&raw) {
                    if let Some(summary) =
                        crate::symbol_index::format_file_summary(&index, &rel, total)
                    {
                        content = format!("{summary}\n\n---\n\n{content}");
                    }
                }
            }
        }

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
            metadata["total_lines"] = json!(t);
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

static XLSX_SI_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<si>(.*?)</si>").unwrap());
static XLSX_T_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<t[^>]*>(.*?)</t>").unwrap());

static PPTX_AT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<a:t[^>]*>(.*?)</a:t>").unwrap());

fn is_docx(path: &Path) -> Result<bool, ToolError> {
    // Extension-only: many ZIP formats share the PK header; `.docx` is unambiguous enough.
    Ok(path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("docx")))
}

fn is_xlsx(path: &Path) -> Result<bool, ToolError> {
    Ok(path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("xlsx")))
}

fn is_pptx(path: &Path) -> Result<bool, ToolError> {
    Ok(path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("pptx")))
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

fn read_xlsx(path: &Path) -> Result<ToolResult, ToolError> {
    let size_bytes = fs::metadata(path).map(|m| m.len()).ok();
    let file = fs::File::open(path).map_err(|e| {
        ToolError::execution_failed(format!(
            "[NOT_FOUND] 无法打开 XLSX 文件 {}: {e}",
            path.display()
        ))
    })?;

    let mut archive = zip::ZipArchive::new(file).map_err(|e| {
        ToolError::execution_failed(format!(
            "[BINARY] 无法解析 XLSX/ZIP {}: {e}",
            path.display()
        ))
    })?;

    // 1. 读取共享字符串表
    let mut shared_strings: Vec<String> = Vec::new();
    if let Ok(mut entry) = archive.by_name("xl/sharedStrings.xml") {
        let mut ss_xml = String::new();
        entry.read_to_string(&mut ss_xml).ok();
        for si_cap in XLSX_SI_RE.captures_iter(&ss_xml) {
            let si_text = si_cap.get(1).map(|m| m.as_str()).unwrap_or("");
            let mut merged = String::new();
            for t_cap in XLSX_T_RE.captures_iter(si_text) {
                if let Some(tm) = t_cap.get(1) {
                    merged.push_str(tm.as_str());
                }
            }
            shared_strings.push(merged);
        }
    }

    // 2. 读取 workbook.xml 获取 sheet 名称
    let mut sheet_names: Vec<String> = Vec::new();
    if let Ok(mut entry) = archive.by_name("xl/workbook.xml") {
        let mut wb_xml = String::new();
        entry.read_to_string(&mut wb_xml).ok();
        let name_re = regex::Regex::new(r#"name="([^"]*)""#).unwrap();
        for cap in name_re.captures_iter(&wb_xml) {
            sheet_names.push(cap[1].to_string());
        }
    }

    // 3. 枚举并解析所有 sheet
    let sheet_re =
        regex::Regex::new(r#"<c r="([A-Z]+)(\d+)"(?:\s+t="([^"]*)")?>(?:<v>([^<]*)</v>)?</c>"#)
            .unwrap();
    let mut result = String::new();

    for i in 1.. {
        let sheet_path = format!("xl/worksheets/sheet{i}.xml");
        let sheet_xml = match archive.by_name(&sheet_path) {
            Ok(mut entry) => {
                let mut s = String::new();
                entry.read_to_string(&mut s).ok();
                s
            }
            Err(_) => break,
        };

        // Replace XML-escaped characters in values
        let name = sheet_names
            .get(i - 1)
            .cloned()
            .unwrap_or_else(|| format!("Sheet{i}"));
        if !result.is_empty() {
            result.push('\n');
        }
        result.push_str(&format!("=== Sheet: {name} ===\n"));

        // Group cells by row for cleaner output
        let mut rows: std::collections::BTreeMap<u64, Vec<(String, String)>> =
            std::collections::BTreeMap::new();

        // Pass 1: inlineStr cells — XML layout: <c r="A1" t="inlineStr"><is><t>text</t></is></c>
        // These have no <v> tag so the main sheet_re does not match them.
        let inline_re = regex::Regex::new(
            r#"<c r="([A-Z]+)(\d+)"[^>]*t="inlineStr"[^>]*>.*?<t[^>]*>(.*?)</t>.*?</c>"#,
        )
        .unwrap();
        for cap in inline_re.captures_iter(&sheet_xml) {
            let col = cap.get(1).map(|m| m.as_str()).unwrap_or("").to_string();
            let row: u64 = cap
                .get(2)
                .and_then(|m| m.as_str().parse().ok())
                .unwrap_or(0);
            let text = cap.get(3).map(|m| m.as_str()).unwrap_or("");
            rows.entry(row).or_default().push((col, text.to_string()));
        }

        // Pass 2: regular cells (t="s" SSI ref, t="str", no type)
        for cap in sheet_re.captures_iter(&sheet_xml) {
            let col = cap.get(1).map(|m| m.as_str()).unwrap_or("").to_string();
            let row: u64 = cap
                .get(2)
                .and_then(|m| m.as_str().parse().ok())
                .unwrap_or(0);
            let t_type = cap.get(3).map(|m| m.as_str()).unwrap_or("");
            let val = cap.get(4).map(|m| m.as_str()).unwrap_or("");

            if t_type == "inlineStr" {
                continue; // handled by pass 1
            }

            let cell_text = if t_type == "s" {
                let idx: usize = val.parse().unwrap_or(0);
                shared_strings.get(idx).cloned().unwrap_or_default()
            } else {
                val.to_string()
            };

            rows.entry(row).or_default().push((col, cell_text));
        }

        for (_row_idx, cells) in &rows {
            let line: Vec<String> = cells
                .iter()
                .map(|(col, txt)| format!("[{col}] {txt}"))
                .collect();
            result.push_str(&line.join("  "));
            result.push('\n');
        }
    }

    if result.is_empty() {
        return Ok(
            ToolResult::success("[XLSX] 文件内容为空或无有效数据。").with_metadata(json!({
                "path": path.to_string_lossy(),
                "kind": "xlsx",
                "size_bytes": size_bytes,
            })),
        );
    }

    Ok(
        ToolResult::success(result.trim_end().to_string()).with_metadata(json!({
            "path": path.to_string_lossy(),
            "kind": "xlsx",
            "size_bytes": size_bytes,
        })),
    )
}

fn read_pptx(path: &Path) -> Result<ToolResult, ToolError> {
    let size_bytes = fs::metadata(path).map(|m| m.len()).ok();
    let file = fs::File::open(path).map_err(|e| {
        ToolError::execution_failed(format!(
            "[NOT_FOUND] 无法打开 PPTX 文件 {}: {e}",
            path.display()
        ))
    })?;

    let mut archive = zip::ZipArchive::new(file).map_err(|e| {
        ToolError::execution_failed(format!(
            "[BINARY] 无法解析 PPTX/ZIP {}: {e}",
            path.display()
        ))
    })?;

    let mut result = String::new();

    for i in 1.. {
        let slide_path = format!("ppt/slides/slide{i}.xml");
        let slide_xml = match archive.by_name(&slide_path) {
            Ok(mut entry) => {
                let mut s = String::new();
                entry.read_to_string(&mut s).ok();
                s
            }
            Err(_) => break,
        };

        let mut slide_text = String::new();
        for cap in PPTX_AT_RE.captures_iter(&slide_xml) {
            if let Some(m) = cap.get(1) {
                slide_text.push_str(m.as_str());
            }
        }
        let trimmed = slide_text.trim();
        if !trimmed.is_empty() {
            if !result.is_empty() {
                result.push('\n');
            }
            result.push_str(&format!("=== Slide {i} ===\n"));
            result.push_str(trimmed);
        }
    }

    if result.is_empty() {
        return Ok(
            ToolResult::success("[PPTX] 文件内容为空或仅包含非文本元素。").with_metadata(json!({
                "path": path.to_string_lossy(),
                "kind": "pptx",
                "size_bytes": size_bytes,
            })),
        );
    }

    Ok(ToolResult::success(result).with_metadata(json!({
        "path": path.to_string_lossy(),
        "kind": "pptx",
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

        let scratchpad_cfg = context
            .runtime
            .scratchpad_config
            .clone()
            .unwrap_or_default();
        let bound_run = context
            .runtime
            .scratchpad_run_id
            .lock()
            .ok()
            .and_then(|g| g.clone());
        if let Some(block_msg) = crate::core::engine::scratchpad_flow::check_write_file_audit_report_gate(
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
fn normalize_line_endings(text: &str, file_le: &str) -> String {
    if file_le == "\r\n" {
        let s = text.replace("\r\n", "\n");
        s.replace('\n', "\r\n")
    } else {
        text.to_string()
    }
}

/// Build a compact before/after snippet for small changes.
fn make_compact_change(old: &str, new: &str) -> String {
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
fn find_match_line_numbers(contents: &str, search: &str, max_results: usize) -> Vec<usize> {
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

fn check_jsx_balance(content: &str) -> Option<String> {
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

fn jsx_balance_warning(file_path: &std::path::Path, content: &str) -> String {
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
                json!({"path": "edit_me.txt", "search": "hello", "replace": "hi", "replace_mode": "all"}),
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
        assert_eq!(required.len(), 1); // only 'path' is required (operation defaults to search_replace)

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
