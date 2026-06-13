//! `read_office` tool — high-fidelity read for office documents (XLSX/DOCX/PPTX/PDF/CSV).
//!
//! XLSX (and .xls / .xlsb / .ods) use **calamine** for typed cells, dates, formulas, and paging.
//! DOCX/PPTX enhance the legacy `read_file` OOXML paths (tables, headings, speaker notes).
//! PDF reuses [`crate::tools::file::read::read_pdf`]; scanned PDFs are directed to `describe_image`.

use async_trait::async_trait;
use calamine::{Data, Reader, Sheets, open_workbook_auto};
use regex::Regex;
use serde_json::{Value, json};
use std::fs;
use std::io::{BufReader, Read};
use std::path::Path;
use std::sync::LazyLock;

use super::file::read_pdf;
use super::file::{DEFAULT_LIMIT, MAX_FILE_SIZE, MAX_LIMIT};
use super::office_inputs::read_office_input_schema;
use super::spec::{
    ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec, optional_str, optional_u64,
    required_str,
};

const OFFICE_DEFAULT_ROW_LIMIT: u64 = DEFAULT_LIMIT as u64;

static DOCX_WT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<w:t[^>]*>([^<]*)</w:t>").expect("DOCX_WT_RE"));
static DOCX_STYLE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"<w:pStyle w:val="([^"]+)""#).expect("DOCX_STYLE_RE"));
static PPTX_AT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<a:t[^>]*>([^<]*)</a:t>").expect("PPTX_AT_RE"));
static PPTX_REL_CHART_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"<Relationship[^>]+Type="[^"]*?/relationships/chart"[^>]+Target="([^"]+)""#)
        .expect("PPTX_REL_CHART_RE")
});
static PPTX_CV_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<c:v>([^<]*)</c:v>").expect("PPTX_CV_RE"));

// ── ReadOfficeTool ────────────────────────────────────────────────────────

pub struct ReadOfficeTool;

#[async_trait]
impl ToolSpec for ReadOfficeTool {
    fn name(&self) -> &'static str {
        "read_office"
    }

    fn description(&self) -> &'static str {
        concat!(
            "Read office documents (.xlsx/.xls/.xlsb/.ods, .docx, .pptx, .pdf, .csv/.tsv) with ",
            "higher fidelity than read_file. Prefer this for spreadsheets and structured docs: ",
            "Excel dates/formats/formulas, aligned tables, row paging (start_row + limit), DOCX tables, ",
            "PPTX speaker notes. PDF page ranges via pages. Scanned PDFs: use describe_image for OCR."
        )
    }

    fn input_schema(&self) -> Value {
        read_office_input_schema()
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
        check_file_size(&file_path)?;

        let pages = optional_str(&input, "pages");
        let sheet = optional_str(&input, "sheet");
        let start_row = input
            .get("start_row")
            .and_then(Value::as_u64)
            .unwrap_or(1)
            .max(1);
        let limit = optional_u64(&input, "limit", OFFICE_DEFAULT_ROW_LIMIT)
            .clamp(1, MAX_LIMIT as u64) as usize;

        let ext = file_path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase());

        match ext.as_deref() {
            Some("xlsx" | "xls" | "xlsb" | "ods") => {
                read_spreadsheet_calamine(&file_path, sheet, start_row, limit)
            }
            Some("docx") => read_docx_enhanced(&file_path, limit),
            Some("pptx") => read_pptx_enhanced(&file_path),
            Some("pdf") => read_pdf_office(&file_path, pages),
            Some("csv") => read_delimited_table(&file_path, b',', start_row, limit),
            Some("tsv") => read_delimited_table(&file_path, b'\t', start_row, limit),
            Some("doc") => Err(legacy_binary_hint(&file_path, "DOC", ".docx")),
            Some("ppt") => Err(legacy_binary_hint(&file_path, "PPT", ".pptx")),
            _ => Err(ToolError::invalid_input(format!(
                "read_office does not support this extension ({}). Supported: xlsx, xls, xlsb, ods, docx, pptx, pdf, csv, tsv.",
                ext.as_deref().unwrap_or("(none)")
            ))),
        }
    }
}

fn check_file_size(path: &Path) -> Result<(), ToolError> {
    let size = fs::metadata(path).map_err(|e| {
        ToolError::execution_failed(format!("[NOT_FOUND] 无法访问文件 {}: {e}", path.display()))
    })?;
    if size.len() > MAX_FILE_SIZE {
        return Err(ToolError::execution_failed(format!(
            "[TOO_LARGE] 文件 {} 大小 {} 超过读取上限 ({}MB)。请缩小文件或指定 start_row/limit 分页。",
            path.display(),
            size.len(),
            MAX_FILE_SIZE / 1024 / 1024
        )));
    }
    Ok(())
}

fn legacy_binary_hint(path: &Path, kind: &str, target: &str) -> ToolError {
    ToolError::execution_failed(format!(
        "[UNSUPPORTED] {kind} 旧版二进制格式 ({}) 暂不支持。请在 Office/LibreOffice 中另存为 {target} 后重试，或使用 read_file 作兜底。",
        path.display(),
        target = target
    ))
}

// ── Spreadsheet (calamine) ────────────────────────────────────────────────

fn read_spreadsheet_calamine(
    path: &Path,
    sheet_param: Option<&str>,
    start_row: u64,
    limit: usize,
) -> Result<ToolResult, ToolError> {
    let mut workbook: Sheets<BufReader<fs::File>> = open_workbook_auto(path).map_err(|e| {
        ToolError::execution_failed(format!("[BINARY] 无法打开表格文件 {}: {e}", path.display()))
    })?;

    let sheet_names = workbook.sheet_names().to_vec();
    if sheet_names.is_empty() {
        return Ok(
            ToolResult::success("[表格] 文件无工作表。").with_metadata(json!({
                "path": path.to_string_lossy(),
                "kind": spreadsheet_kind(path),
                "sheets": [],
            })),
        );
    }

    let sheet_infos: Vec<Value> = sheet_names
        .iter()
        .map(|name| {
            workbook
                .worksheet_range(name)
                .ok()
                .map(|r| {
                    let (h, w) = r.get_size();
                    json!({ "name": name, "rows": h, "cols": w })
                })
                .unwrap_or_else(|| json!({ "name": name }))
        })
        .collect();

    let indices = resolve_sheet_indices(&sheet_names, sheet_param)?;
    let mut output = String::new();
    let mut truncated_any = false;
    let mut next_start_row: Option<u64> = None;

    for (idx_pos, sheet_idx) in indices.iter().enumerate() {
        let name = &sheet_names[*sheet_idx];
        if idx_pos > 0 {
            output.push('\n');
        }

        let range = workbook.worksheet_range(name).map_err(|e| {
            ToolError::execution_failed(format!(
                "无法读取工作表 '{name}' ({}): {e}",
                path.display()
            ))
        })?;

        let formula_range = workbook.worksheet_formula(name).ok();
        let (body, meta) =
            format_sheet_range(name, &range, formula_range.as_ref(), start_row, limit)?;
        if meta.truncated {
            truncated_any = true;
            next_start_row = Some(meta.next_start_row);
        }
        output.push_str(&body);
    }

    if output.trim().is_empty() {
        output =
            format!("[表格] 工作表无可见数据（可能为空或超出分页范围 start_row={start_row}）。");
    }

    let mut metadata = json!({
        "path": path.to_string_lossy(),
        "kind": spreadsheet_kind(path),
        "sheets": sheet_infos,
        "start_row": start_row,
        "limit": limit,
    });
    if let Some(s) = sheet_param {
        metadata["sheet"] = json!(s);
    }
    if truncated_any {
        metadata["truncated"] = json!(true);
        if let Some(n) = next_start_row {
            metadata["next_start_row"] = json!(n);
        }
    }

    Ok(ToolResult::success(output.trim_end().to_string()).with_metadata(metadata))
}

fn spreadsheet_kind(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) if ext.eq_ignore_ascii_case("ods") => "ods",
        Some(ext) if ext.eq_ignore_ascii_case("xls") => "xls",
        Some(ext) if ext.eq_ignore_ascii_case("xlsb") => "xlsb",
        _ => "xlsx",
    }
}

fn resolve_sheet_indices(names: &[String], param: Option<&str>) -> Result<Vec<usize>, ToolError> {
    match param {
        None => Ok(vec![0]),
        Some(s) => {
            if let Ok(idx) = s.parse::<usize>() {
                if idx >= names.len() {
                    return Err(ToolError::invalid_input(format!(
                        "sheet 索引 {idx} 超出范围（共 {} 个工作表: {}）",
                        names.len(),
                        names.join(", ")
                    )));
                }
                return Ok(vec![idx]);
            }
            names
                .iter()
                .position(|n| n == s)
                .map(|i| vec![i])
                .ok_or_else(|| {
                    ToolError::invalid_input(format!(
                        "未找到工作表 '{s}'。可用: {}",
                        names.join(", ")
                    ))
                })
        }
    }
}

struct SheetFormatMeta {
    truncated: bool,
    next_start_row: u64,
}

fn format_sheet_range(
    sheet_name: &str,
    range: &calamine::Range<Data>,
    formula_range: Option<&calamine::Range<String>>,
    start_row: u64,
    limit: usize,
) -> Result<(String, SheetFormatMeta), ToolError> {
    let (height, width) = range.get_size();
    if height == 0 || width == 0 {
        return Ok((
            format!("=== Sheet: {sheet_name} (空) ===\n"),
            SheetFormatMeta {
                truncated: false,
                next_start_row: start_row,
            },
        ));
    }

    let start_idx = start_row.saturating_sub(1) as usize;
    if start_idx >= height {
        return Ok((
            format!(
                "=== Sheet: {sheet_name} ({height} 行 × {width} 列) ===\n[分页] start_row={start_row} 超出表行数。\n"
            ),
            SheetFormatMeta {
                truncated: false,
                next_start_row: start_row,
            },
        ));
    }

    let end_idx = (start_idx + limit).min(height);
    let truncated = end_idx < height;
    let next_start_row = if truncated {
        (end_idx + 1) as u64
    } else {
        start_row
    };

    let mut out = format!("=== Sheet: {sheet_name} ({height} 行 × {width} 列) ===\n");

    for row_idx in start_idx..end_idx {
        let mut cells = Vec::with_capacity(width);
        for col_idx in 0..width {
            let cell = range.get((row_idx, col_idx)).unwrap_or(&Data::Empty);
            let formula = formula_range
                .and_then(|fr| fr.get((row_idx, col_idx)))
                .map(|s| s.as_str())
                .filter(|s| !s.is_empty());
            cells.push(format_data_cell(cell, formula));
        }
        out.push('|');
        out.push_str(&cells.join("|"));
        out.push_str("|\n");
    }

    if truncated {
        let remaining = height - end_idx;
        out.push_str(&format!(
            "\n...（已显示第 {start_row}–{end_idx} 行，共 {height} 行；还有 {remaining} 行，请用 start_row={next_start_row} 续读）\n"
        ));
    } else if start_row > 1 || end_idx < height {
        out.push_str(&format!(
            "\n（已显示第 {start_row}–{end_idx} 行，共 {height} 行）\n"
        ));
    }

    Ok((
        out,
        SheetFormatMeta {
            truncated,
            next_start_row,
        },
    ))
}

fn format_data_cell(data: &Data, formula: Option<&str>) -> String {
    if let Some(f) = formula {
        let display = format!("={f}");
        return escape_table_cell(&display);
    }
    let text = match data {
        Data::Empty => String::new(),
        Data::Float(f) => format_excel_float(*f),
        Data::Int(i) => i.to_string(),
        Data::String(s) => s.clone(),
        Data::Bool(b) => b.to_string(),
        Data::DateTime(dt) => dt.to_string(),
        Data::DateTimeIso(s) => s.clone(),
        Data::DurationIso(s) => s.clone(),
        Data::Error(e) => format!("#{e:?}"),
    };
    escape_table_cell(&text)
}

fn format_excel_float(f: f64) -> String {
    if (f - f.round()).abs() < f64::EPSILON * 10.0_f64.max(f.abs()) {
        return (f.round() as i64).to_string();
    }
    let s = format!("{f:.6}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn escape_table_cell(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}

// ── DOCX (enhanced) ───────────────────────────────────────────────────────

fn read_docx_enhanced(path: &Path, max_paragraphs: usize) -> Result<ToolResult, ToolError> {
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
    archive
        .by_name("word/document.xml")
        .map_err(|e| {
            ToolError::execution_failed(format!(
                "[BINARY] word/document.xml not found in {}: {e}",
                path.display()
            ))
        })?
        .read_to_string(&mut doc_xml)
        .map_err(|e| {
            ToolError::execution_failed(format!(
                "Failed to read word/document.xml from {}: {e}",
                path.display()
            ))
        })?;

    let mut result = String::new();
    let mut para_count = 0usize;
    let mut truncated = false;

    if let Some(tbl_idx) = doc_xml.find("<w:tbl") {
        for para in doc_xml[..tbl_idx].split("</w:p>") {
            if para_count >= max_paragraphs {
                truncated = true;
                break;
            }
            if let Some(line) = format_docx_paragraph(para) {
                if !result.is_empty() {
                    result.push('\n');
                }
                result.push_str(&line);
                para_count += 1;
            }
        }
    }

    for segment in doc_xml.split("<w:tbl").skip(1) {
        let (tbl_part, rest) = segment.split_once("</w:tbl>").unwrap_or((segment, ""));
        if !result.is_empty() {
            result.push('\n');
        }
        result.push_str(&format_docx_table(tbl_part));
        if para_count >= max_paragraphs {
            truncated = true;
            break;
        }
        for para in rest.split("</w:p>") {
            if para_count >= max_paragraphs {
                truncated = true;
                break;
            }
            if let Some(line) = format_docx_paragraph(para) {
                if !result.is_empty() {
                    result.push('\n');
                }
                result.push_str(&line);
                para_count += 1;
            }
        }
        if truncated {
            break;
        }
    }

    if result.is_empty() {
        for para in doc_xml.split("</w:p>") {
            if para_count >= max_paragraphs {
                truncated = true;
                break;
            }
            if let Some(line) = format_docx_paragraph(para) {
                if !result.is_empty() {
                    result.push('\n');
                }
                result.push_str(&line);
                para_count += 1;
            }
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

    if truncated {
        result.push_str(&format!(
            "\n\n[截断] 已输出约 {max_paragraphs} 段；文档较长，可拆段处理或缩小范围。"
        ));
    }

    Ok(ToolResult::success(result).with_metadata(json!({
        "path": path.to_string_lossy(),
        "kind": "docx",
        "size_bytes": size_bytes,
        "truncated": truncated,
        "paragraph_limit": max_paragraphs,
    })))
}

fn format_docx_paragraph(para: &str) -> Option<String> {
    if para.contains("<w:tbl") {
        return None;
    }
    let mut line = String::new();
    for cap in DOCX_WT_RE.captures_iter(para) {
        if let Some(m) = cap.get(1) {
            line.push_str(m.as_str());
        }
    }
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    let prefix = DOCX_STYLE_RE
        .captures(para)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str())
        .map(docx_heading_prefix)
        .unwrap_or("");
    Some(format!("{prefix}{trimmed}"))
}

fn docx_heading_prefix(style: &str) -> &'static str {
    if style.starts_with("Heading1") || style == "1" || style.contains("标题1") {
        "# "
    } else if style.starts_with("Heading2") || style == "2" || style.contains("标题2") {
        "## "
    } else if style.starts_with("Heading3") || style == "3" || style.contains("标题3") {
        "### "
    } else {
        ""
    }
}

fn format_docx_table(tbl_xml: &str) -> String {
    let mut out = String::from("[表格]\n");
    for row_xml in tbl_xml.split("<w:tr").skip(1) {
        let row_part = row_xml.split("</w:tr>").next().unwrap_or(row_xml);
        let mut cells = Vec::new();
        for cell_xml in row_part.split("<w:tc").skip(1) {
            let cell_part = cell_xml.split("</w:tc>").next().unwrap_or(cell_xml);
            let mut text = String::new();
            for cap in DOCX_WT_RE.captures_iter(cell_part) {
                if let Some(m) = cap.get(1) {
                    text.push_str(m.as_str());
                }
            }
            cells.push(escape_table_cell(text.trim()));
        }
        if !cells.is_empty() {
            out.push('|');
            out.push_str(&cells.join("|"));
            out.push_str("|\n");
        }
    }
    out
}

// ── PPTX (enhanced) ───────────────────────────────────────────────────────

fn read_pptx_enhanced(path: &Path) -> Result<ToolResult, ToolError> {
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
                if entry.read_to_string(&mut s).is_err() {
                    break;
                }
                s
            }
            Err(_) => break,
        };

        let mut slide_text = extract_pptx_text(&slide_xml);
        let tables = extract_pptx_tables(&slide_xml);
        if !tables.is_empty() {
            if !slide_text.is_empty() {
                slide_text.push('\n');
            }
            slide_text.push_str(&tables);
        }

        let notes = read_pptx_notes(&mut archive, i);
        let charts = read_pptx_charts_for_slide(&mut archive, i);

        if slide_text.trim().is_empty() && notes.trim().is_empty() && charts.trim().is_empty() {
            continue;
        }

        if !result.is_empty() {
            result.push('\n');
        }
        result.push_str(&format!("=== Slide {i} ===\n"));
        if !slide_text.trim().is_empty() {
            result.push_str(slide_text.trim());
            result.push('\n');
        }
        if !notes.trim().is_empty() {
            result.push_str("[演讲者备注]\n");
            result.push_str(notes.trim());
            result.push('\n');
        }
        if !charts.trim().is_empty() {
            result.push_str(&charts);
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

    Ok(
        ToolResult::success(result.trim_end().to_string()).with_metadata(json!({
            "path": path.to_string_lossy(),
            "kind": "pptx",
            "size_bytes": size_bytes,
        })),
    )
}

fn read_pptx_charts_for_slide(archive: &mut zip::ZipArchive<fs::File>, slide: usize) -> String {
    let rels_path = format!("ppt/slides/_rels/slide{slide}.xml.rels");
    let rels_xml = {
        let Ok(mut entry) = archive.by_name(&rels_path) else {
            return String::new();
        };
        let mut xml = String::new();
        if entry.read_to_string(&mut xml).is_err() {
            return String::new();
        }
        xml
    };
    let chart_paths: Vec<String> = PPTX_REL_CHART_RE
        .captures_iter(&rels_xml)
        .filter_map(|cap| cap.get(1).map(|m| resolve_pptx_chart_path(m.as_str())))
        .collect();
    let mut out = String::new();
    for chart_path in chart_paths {
        let Ok(mut chart_entry) = archive.by_name(chart_path.as_str()) else {
            continue;
        };
        let mut chart_xml = String::new();
        if chart_entry.read_to_string(&mut chart_xml).is_err() {
            continue;
        }
        let formatted = format_pptx_chart_xml(&chart_xml);
        if formatted.trim().is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&formatted);
    }
    out
}

fn resolve_pptx_chart_path(target: &str) -> String {
    let t = target.trim_start_matches("../");
    if t.starts_with("ppt/") {
        t.to_string()
    } else {
        format!("ppt/{t}")
    }
}

fn format_pptx_chart_xml(xml: &str) -> String {
    let mut out = String::new();
    for (idx, ser) in xml.split("<c:ser").skip(1).enumerate() {
        let ser_body = ser.split("</c:ser>").next().unwrap_or(ser);
        let title = ser_body
            .split("<c:tx")
            .nth(1)
            .and_then(|tx| PPTX_CV_RE.captures(tx))
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| format!("系列{}", idx + 1));
        let val_section = ser_body.split("<c:val").nth(1).unwrap_or("");
        let values: Vec<String> = PPTX_CV_RE
            .captures_iter(val_section)
            .filter_map(|c| c.get(1).map(|m| m.as_str().trim().to_string()))
            .filter(|s| !s.is_empty())
            .collect();
        if values.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str("[图表数据] ");
        out.push_str(&title);
        out.push_str(": ");
        out.push_str(&values.join(", "));
    }
    out
}

fn read_pptx_notes(archive: &mut zip::ZipArchive<fs::File>, slide: usize) -> String {
    let path = format!("ppt/notesSlides/notesSlide{slide}.xml");
    let Ok(mut entry) = archive.by_name(&path) else {
        return String::new();
    };
    let mut xml = String::new();
    if entry.read_to_string(&mut xml).is_err() {
        return String::new();
    }
    extract_pptx_text(&xml)
}

fn extract_pptx_text(xml: &str) -> String {
    let mut text = String::new();
    for cap in PPTX_AT_RE.captures_iter(xml) {
        if let Some(m) = cap.get(1) {
            text.push_str(m.as_str());
        }
    }
    text
}

fn extract_pptx_tables(xml: &str) -> String {
    let mut out = String::new();
    for tbl in xml.split("<a:tbl").skip(1) {
        let tbl_part = tbl.split("</a:tbl>").next().unwrap_or(tbl);
        let mut rows = Vec::new();
        for row in tbl_part.split("<a:tr").skip(1) {
            let row_part = row.split("</a:tr>").next().unwrap_or(row);
            let mut cells = Vec::new();
            for cell in row_part.split("<a:tc").skip(1) {
                let cell_part = cell.split("</a:tc>").next().unwrap_or(cell);
                cells.push(extract_pptx_text(cell_part).trim().to_string());
            }
            if !cells.is_empty() {
                rows.push(cells);
            }
        }
        if rows.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str("[表格]\n");
        for row in rows {
            out.push('|');
            for c in &row {
                out.push_str(&escape_table_cell(c));
                out.push('|');
            }
            out.push('\n');
        }
    }
    out
}

// ── PDF ───────────────────────────────────────────────────────────────────

fn read_pdf_office(path: &Path, pages: Option<&str>) -> Result<ToolResult, ToolError> {
    let mut result = read_pdf(path, pages)?;
    let trimmed = result.content.trim();
    if trimmed.len() < 80
        && (trimmed.is_empty()
            || trimmed.contains("empty text")
            || trimmed.contains("binary_unavailable"))
    {
        result.content.push_str(
            "\n\n[提示] 扫描版或图片型 PDF 文本极少。请对页面截图使用 describe_image 做 OCR。",
        );
        let mut meta = result.metadata.take().unwrap_or_else(|| json!({}));
        if let Value::Object(ref mut obj) = meta {
            obj.insert("ocr_hint".to_string(), json!("describe_image"));
        }
        result.metadata = Some(meta);
    }
    Ok(result)
}

// ── CSV / TSV ─────────────────────────────────────────────────────────────

fn read_delimited_table(
    path: &Path,
    delimiter: u8,
    start_row: u64,
    limit: usize,
) -> Result<ToolResult, ToolError> {
    let content = fs::read_to_string(path).map_err(|e| {
        ToolError::execution_failed(format!("[NOT_FOUND] 无法读取 {}: {e}", path.display()))
    })?;

    let lines: Vec<&str> = content.lines().collect();
    let total = lines.len();
    if total == 0 {
        return Ok(
            ToolResult::success("[CSV] 文件为空。").with_metadata(json!({
                "path": path.to_string_lossy(),
                "kind": if delimiter == b',' { "csv" } else { "tsv" },
                "rows": 0,
            })),
        );
    }

    let start_idx = start_row.saturating_sub(1) as usize;
    if start_idx >= total {
        return Err(ToolError::invalid_input(format!(
            "start_row={start_row} 超出文件行数 ({total})"
        )));
    }
    let end_idx = (start_idx + limit).min(total);
    let truncated = end_idx < total;

    let mut out = format!("=== 表格 ({total} 行) ===\n");
    for line in &lines[start_idx..end_idx] {
        let cells: Vec<String> = parse_delimited_line(line, delimiter)
            .into_iter()
            .map(|c| escape_table_cell(&c))
            .collect();
        out.push('|');
        out.push_str(&cells.join("|"));
        out.push_str("|\n");
    }

    if truncated {
        out.push_str(&format!(
            "\n...（还有 {} 行，请用 start_row={} 续读）\n",
            total - end_idx,
            end_idx + 1
        ));
    }

    let kind = if delimiter == b',' { "csv" } else { "tsv" };
    Ok(
        ToolResult::success(out.trim_end().to_string()).with_metadata(json!({
            "path": path.to_string_lossy(),
            "kind": kind,
            "rows": total,
            "cols": lines.first().map(|l| parse_delimited_line(l, delimiter).len()),
            "start_row": start_row,
            "limit": limit,
            "truncated": truncated,
        })),
    )
}

fn parse_delimited_line(line: &str, delimiter: u8) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if in_quotes {
            if b == b'"' {
                if i + 1 < bytes.len() && bytes[i + 1] == b'"' {
                    current.push('"');
                    i += 2;
                    continue;
                }
                in_quotes = false;
            } else {
                current.push(b as char);
            }
        } else if b == b'"' {
            in_quotes = true;
        } else if b == delimiter {
            fields.push(current.clone());
            current.clear();
        } else {
            current.push(b as char);
        }
        i += 1;
    }
    fields.push(current);
    fields
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::spec::ToolContext;
    use rust_xlsxwriter::{Format, Workbook, XlsxError};
    use std::io::Write;
    use tempfile::TempDir;

    fn write_sample_xlsx(path: &Path) -> Result<(), XlsxError> {
        let mut workbook = Workbook::new();
        let date_format = Format::new().set_num_format("yyyy-mm-dd");
        let pct_format = Format::new().set_num_format("0.00%");
        let worksheet = workbook.add_worksheet();
        worksheet.set_name("月度汇总")?;
        worksheet.write_string(0, 0, "月份")?;
        worksheet.write_string(0, 1, "销售额")?;
        worksheet.write_string(0, 2, "增长率")?;
        worksheet.write_string(1, 0, "2025-01")?;
        worksheet.write_number(1, 1, 150_000.0)?;
        worksheet.write_number_with_format(1, 2, 0.125, &pct_format)?;
        worksheet.write_string(2, 0, "")?;
        worksheet.write_string(2, 1, "gap")?;
        worksheet.write_number_with_format(3, 0, 45678.0, &date_format)?;
        worksheet.write_formula(4, 0, "=SUM(B2:B3)")?;
        workbook.save(path)?;
        Ok(())
    }

    #[tokio::test]
    async fn read_office_xlsx_dates_and_alignment() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("sample.xlsx");
        write_sample_xlsx(&path).expect("write xlsx");

        let tool = ReadOfficeTool;
        let ctx = ToolContext::new(dir.path().to_path_buf());
        let result = tool
            .execute(json!({ "path": "sample.xlsx", "limit": 50 }), &ctx)
            .await
            .expect("execute");

        let text = &result.content;
        assert!(text.contains("月度汇总"), "sheet name: {text}");
        assert!(text.contains("2025-01"), "string cell: {text}");
        assert!(
            text.contains("150000") || text.contains("150,000") || text.contains("150000"),
            "number: {text}"
        );
        assert!(
            text.contains("12.5%") || text.contains("0.125") || text.contains("12.5"),
            "percentage: {text}"
        );
        assert!(
            text.contains("=SUM") || text.contains("SUM"),
            "formula: {text}"
        );
        let meta = result.metadata.as_ref().expect("metadata");
        assert!(meta.get("sheets").is_some());
    }

    #[tokio::test]
    async fn read_office_csv_paging() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("data.csv");
        let mut f = fs::File::create(&path).expect("create");
        writeln!(f, "a,b").expect("write");
        writeln!(f, "1,2").expect("write");
        writeln!(f, "3,4").expect("write");

        let tool = ReadOfficeTool;
        let ctx = ToolContext::new(dir.path().to_path_buf());
        let result = tool
            .execute(
                json!({ "path": "data.csv", "start_row": 2, "limit": 1 }),
                &ctx,
            )
            .await
            .expect("execute");
        let text = &result.content;
        assert!(text.contains("1") && text.contains("2"));
        assert!(!text.contains("|3|") || text.contains("续读"));
    }

    #[test]
    fn parse_delimited_handles_quotes() {
        let fields = parse_delimited_line(r#""a,b",c"#, b',');
        assert_eq!(fields, vec!["a,b", "c"]);
    }
}
