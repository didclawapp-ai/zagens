//! read_file tool and format-specific readers.

use super::path_input::required_path_field;
use super::schemas::read_file_input_schema;
use super::{DEFAULT_LIMIT, FILE_SIZE_LINE_COUNT_LIMIT, MAX_FILE_SIZE, MAX_LIMIT};
use crate::tools::spec::{
    ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec, optional_str, optional_u64,
};
use async_trait::async_trait;
use regex::Regex;
use serde_json::{Value, json};
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::LazyLock;
use zagens_config::workspace_meta_file_read;
use zagens_tools::{EvidenceCitation, EvidenceEnvelope, UncertaintyKind};

// === ReadFileTool ===

/// Tool for reading UTF-8 files from the workspace.
pub struct ReadFileTool;

#[async_trait]
impl ToolSpec for ReadFileTool {
    fn name(&self) -> &'static str {
        "read_file"
    }

    fn description(&self) -> &'static str {
        "Read a file from the workspace. Plain text uses line paging (start_line or offset + limit) with streaming newline decode (low memory); files starting with UTF-16/UTF-32 BOM use full-file decode. Prefer around_line/symbol/around_last_edit/since_tool_use_id for differential windows instead of re-reading whole files. PDFs: `pdftotext` or `pdf-extract`. DOCX/XLSX/PPTX: extracts text from OOXML ZIP."
    }

    fn input_schema(&self) -> Value {
        read_file_input_schema()
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::ReadOnly, ToolCapability::Sandboxable]
    }

    fn supports_parallel(&self) -> bool {
        true
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let path_str = required_path_field(&input, "read_file")?;
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

        let start_line =
            resolve_read_window(&input, &file_path, &context.workspace, context).await?;
        let limit = if input.get("around_line").and_then(Value::as_u64).is_some()
            || input.get("symbol").and_then(Value::as_str).is_some()
            || input
                .get("around_last_edit")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            || input
                .get("since_tool_use_id")
                .and_then(Value::as_str)
                .is_some_and(|s| !s.trim().is_empty())
        {
            let radius = optional_u64(&input, "context_radius", 40).clamp(1, 250);
            ((radius * 2) + 1).min(MAX_LIMIT as u64) as usize
        } else {
            optional_u64(&input, "limit", DEFAULT_LIMIT as u64).clamp(1, MAX_LIMIT as u64) as usize
        };

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
            let index_path = workspace_meta_file_read(&context.workspace, "symbols.json");
            if let Ok(raw) = std::fs::read_to_string(&index_path)
                && let Ok(index) = serde_json::from_str::<crate::symbol_index::SymbolIndex>(&raw)
                && let Some(summary) = crate::symbol_index::format_file_summary(&index, &rel, total)
            {
                content = format!("{summary}\n\n---\n\n{content}");
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
            "start_line": start_line,
        });
        if let Some(s) = size_bytes {
            metadata["size_bytes"] = json!(s);
        }
        if let Some(t) = total_lines_known {
            metadata["total_lines"] = json!(t);
        }

        let rel = super::workspace_relative_posix(&context.workspace, &file_path);
        let end_line = if collected.is_empty() {
            start_line
        } else {
            start_line + collected.len() as u64 - 1
        };
        let uncertainty = if collected.is_empty() && start_line > 1 {
            UncertaintyKind::NotFound
        } else if truncated {
            UncertaintyKind::Truncated
        } else {
            UncertaintyKind::None
        };
        let mut evidence = EvidenceEnvelope::new()
            .with_fact("path", &rel)
            .with_fact("lines_read", collected.len().to_string())
            .with_fact("start_line", start_line.to_string())
            .with_uncertainty(uncertainty);
        if !collected.is_empty() {
            evidence = evidence.with_citation(EvidenceCitation::lines(&rel, start_line, end_line));
        }
        if let Some(t) = total_lines_known {
            evidence = evidence.with_fact("total_lines", t.to_string());
        }

        Ok(ToolResult::success(content)
            .with_metadata(metadata)
            .with_evidence(evidence))
    }
}

/// Resolve 1-based start_line for plain-text paging, honoring differential windows.
async fn resolve_read_window(
    input: &Value,
    file_path: &Path,
    workspace: &Path,
    context: &ToolContext,
) -> Result<u64, ToolError> {
    let rel = super::workspace_relative_posix(workspace, file_path);
    let radius = optional_u64(input, "context_radius", 40).clamp(1, 250);

    if let Some(tool_use_id) = input
        .get("since_tool_use_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        if let Some(anchors) = context.diff_read_anchors.as_ref() {
            let guard = anchors.lock().await;
            if let Some((anchor_path, window)) = guard.window_for_tool_use(tool_use_id) {
                let path_ok = zagens_core::engine::repo_paths_match(&anchor_path, &rel);
                if path_ok {
                    return Ok(window.with_radius(radius).start_line);
                }
                return Err(ToolError::execution_failed(format!(
                    "since_tool_use_id `{tool_use_id}` anchors `{anchor_path}`, not `{rel}`"
                )));
            }
        }
        return Err(ToolError::execution_failed(format!(
            "since_tool_use_id `{tool_use_id}` not found in session differential-read anchors"
        )));
    }

    if input
        .get("around_last_edit")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        if let Some(anchors) = context.diff_read_anchors.as_ref() {
            let guard = anchors.lock().await;
            if let Some(window) = guard.window_for_last_edit(&rel) {
                return Ok(window.with_radius(radius).start_line);
            }
        }
        return Err(ToolError::execution_failed(format!(
            "around_last_edit: no prior edit window for `{rel}` — edit the file first or use around_line"
        )));
    }

    if let Some(symbol) = input.get("symbol").and_then(Value::as_str) {
        let symbol = symbol.trim();
        if !symbol.is_empty() {
            let index_path = workspace_meta_file_read(workspace, "symbols.json");
            if let Ok(raw) = std::fs::read_to_string(&index_path)
                && let Ok(index) = serde_json::from_str::<crate::symbol_index::SymbolIndex>(&raw)
                && let Some(line) = find_symbol_line_in_file(&index, &rel, symbol)
            {
                return Ok(line.saturating_sub(radius).max(1));
            }
            return Err(ToolError::execution_failed(format!(
                "symbol `{symbol}` not found in symbol index for `{rel}` — try around_line or start_line"
            )));
        }
    }

    if let Some(around) = input.get("around_line").and_then(Value::as_u64) {
        let around = around.max(1);
        return Ok(around.saturating_sub(radius).max(1));
    }

    Ok(
        match (
            input.get("start_line").and_then(Value::as_u64),
            input.get("offset").and_then(Value::as_u64),
        ) {
            (Some(s), _) => s.max(1),
            (None, Some(o)) => o.max(1),
            (None, None) => 1,
        },
    )
}

fn find_symbol_line_in_file(
    index: &crate::symbol_index::SymbolIndex,
    file_rel: &str,
    symbol: &str,
) -> Option<u64> {
    let hits = crate::symbol_index::query_symbol(index, symbol);
    let exact = hits
        .iter()
        .find(|(path, _, name, _)| *path == file_rel && name.eq_ignore_ascii_case(symbol));
    if let Some((_, line, _, _)) = exact {
        return Some(*line as u64);
    }
    hits.iter()
        .find(|(path, _, _, _)| *path == file_rel)
        .map(|(_, line, _, _)| *line as u64)
}

/// Detect a PDF by extension OR by sniffing the `%PDF-` magic bytes.
/// Files without an extension are still recognized as PDFs when the header
/// matches.
pub(in crate::tools::file) fn is_pdf(path: &Path) -> Result<bool, ToolError> {
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

pub(in crate::tools::file) fn parse_pages_arg(spec: &str) -> Option<(u32, u32)> {
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

pub(crate) fn detect_and_decode(bytes: &[u8]) -> (String, String, String) {
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
pub fn sniff_encoding_label(sample: &[u8]) -> Option<String> {
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

type PlainLinesStreamResult =
    Result<(Vec<String>, bool, Option<usize>, String, String), std::io::Error>;

fn read_plain_lines_stream(
    path: &Path,
    skip: usize,
    limit: usize,
    sniff_totals: bool,
) -> PlainLinesStreamResult {
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

pub(crate) fn read_docx(path: &Path) -> Result<ToolResult, ToolError> {
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
    let inline_re = regex::Regex::new(
        r#"<c r="([A-Z]+)(\d+)"[^>]*t="inlineStr"[^>]*>.*?<t[^>]*>(.*?)</t>.*?</c>"#,
    )
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

        for cells in rows.values() {
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

pub(crate) fn read_pptx(path: &Path) -> Result<ToolResult, ToolError> {
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

pub(crate) fn read_pdf(path: &Path, pages: Option<&str>) -> Result<ToolResult, ToolError> {
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
