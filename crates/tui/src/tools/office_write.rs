//! `write_office` tool — generate .xlsx / .docx / .pptx files.
//!
//! Architecture:
//! - XLSX: pure Rust via `rust_xlsxwriter` (no Python dependency).
//! - DOCX: Python + `python-docx` (primary); pure-Rust minimal OOXML fallback.
//! - PPTX: Python + `python-pptx` (+ optional matplotlib paths in bundled builds).
//!
//! DOCX/PPTX subprocess uses [`crate::python_env::resolve_python_for_office`] —
//! prefers the bundled interpreter when shipped (offline), else an office venv.
//!
//! The tool uses `spawn_blocking` so the synchronous file-generation and
//! subprocess calls don't block the async runtime.

use async_trait::async_trait;
use serde_json::Value;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;
use wait_timeout::ChildExt;

use super::spec::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec,
    optional_str, required_str,
};

// ── WriteOfficeTool ──────────────────────────────────────────────────────

pub struct WriteOfficeTool;

#[async_trait]
impl ToolSpec for WriteOfficeTool {
    fn name(&self) -> &'static str {
        "write_office"
    }

    fn description(&self) -> &'static str {
        concat!(
            "Generate .xlsx / .docx / .pptx files from structured JSON data. XLSX uses pure Rust (no Python needed). DOCX/PPTX use Python. ",
            "READ THE USAGE GUIDE BELOW before constructing parameters.\n\n",
            include_str!("../prompts/write_office_guide.md")
        )
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "format": {
                    "type": "string",
                    "enum": ["xlsx", "docx", "pptx"],
                    "description": "Output format"
                },
                "path": {
                    "type": "string",
                    "description": "Output file path (relative to workspace or absolute)"
                },
                "title": {
                    "type": "string",
                    "description": "Document title. PPTX: generates a cover slide (use 'subtitle' for companion text). DOCX: appears as document-level title. If omitted, no cover page is created."
                },
                "subtitle": {
                    "type": "string",
                    "description": "Cover slide subtitle (PPTX only). Ignored when 'title' is not set."
                },
                "theme": {
                    "oneOf": [
                        {
                            "type": "string",
                            "enum": ["dark", "light", "warm", "minimal"],
                            "description": "dark (navy+cyan, tech), light (white+blue, corporate), warm (cream+orange, friendly), minimal (near-white+charcoal, academic)"
                        },
                        {
                            "type": "object",
                            "properties": {
                                "bg":     { "type": "string", "description": "Background hex #RRGGBB" },
                                "accent": { "type": "string", "description": "Accent hex #RRGGBB" },
                                "title":  { "type": "string", "description": "Title text hex #RRGGBB" },
                                "body":   { "type": "string", "description": "Body text hex #RRGGBB" },
                                "muted":  { "type": "string", "description": "Secondary text hex #RRGGBB" },
                                "font":   { "type": "string", "description": "Font family name" }
                            },
                            "required": ["bg", "accent", "title", "body", "muted", "font"]
                        }
                    ],
                    "description": "PPTX theme: preset name or custom { bg, accent, title, body, muted, font }. Default: dark."
                },
                "style": {
                    "type": "object",
                    "description": "XLSX global style (all fields optional). theme: corporate|tech|warm|minimal (default corporate). header_freeze: freeze top row (default true). border: thin|none (default thin). banded_rows: alternate row colour (default true). print: { orientation?: portrait|landscape, paper_size?: A4|A3|Letter, fit_to_width?: number, header?: string, footer?: string, margins?: { left?, right?, top?, bottom?, header?, footer? } }.",
                    "properties": {
                        "theme":          { "type": "string", "enum": ["corporate", "tech", "warm", "minimal"] },
                        "header_freeze":  { "type": "boolean" },
                        "border":         { "type": "string", "enum": ["thin", "none"] },
                        "banded_rows":    { "type": "boolean" },
                        "print":          { "type": "object" }
                    }
                },
                "sheets": {
                    "type": "array",
                    "description": "XLSX sheets: [{ name, header?: bool, columns?: [{ width?, label?, format?: text|number|currency|percentage|date, number_format?, formula?, wrap? }], merged_cells?: [{ row, col, rows, cols }], charts?: [{ type: bar|line|pie|stacked_bar, title?, categories_range, values_range, position: { row, col }, size?: { width, height } }], conditional_formats?: [{ range: { row, col, rows, cols }, type: data_bar|cell_highlight, color?, condition?, value? }], rows: [[value...]] }]",
                    "items": { "type": "object" }
                },
                "blocks": {
                    "type": "array",
                    "description": "DOCX content blocks: [{ type: heading|paragraph|list, ... }]",
                    "items": { "type": "object" }
                },
                "slides": {
                    "type": "array",
                    "description": "PPTX slides: [{ title, bullets?: [str], table?: { headers:[str], rows:[[value]] }, chart?: { type: bar|line|pie|stacked_bar|stacked_bar_pct|area|scatter|donut, categories:[str], series:[{name,values:[num]}], chart_title?, x_label?, y_label?, data_labels? }, notes?, theme? }]",
                    "items": { "type": "object" }
                }
            },
            "required": ["format", "path"]
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![
            ToolCapability::WritesFiles,
            ToolCapability::ExecutesCode,
            ToolCapability::RequiresApproval,
        ]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Suggest
    }

    fn supports_parallel(&self) -> bool {
        true
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let format = required_str(&input, "format")?;
        let path_str = required_str(&input, "path")?;
        let output_path = context.resolve_path(path_str)?;

        // Ensure parent directory exists
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                ToolError::execution_failed(format!("无法创建目录 {}: {}", parent.display(), e))
            })?;
        }

        let data = input.clone();
        let out = output_path.clone();

        let format_owned = format.to_string();
        let result = tokio::task::spawn_blocking(move || match format_owned.as_str() {
            "xlsx" => generate_xlsx(&data, &out),
            "docx" => generate_docx(&data, &out),
            "pptx" => generate_pptx(&data, &out),
            other => Err(format!("不支持的格式: {other}。支持: xlsx, docx, pptx")),
        })
        .await
        .map_err(|e| ToolError::execution_failed(format!("spawn_blocking 失败: {e}")))?;

        match result {
            Ok(engine) => {
                let meta = serde_json::json!({
                    "path": output_path.to_string_lossy(),
                    "format": format,
                    "engine": engine,
                });
                Ok(ToolResult::success(format!(
                    "已生成 {} ({} 引擎)",
                    output_path.display(),
                    engine
                ))
                .with_metadata(meta))
            }
            Err(msg) => Ok(ToolResult::error(msg)),
        }
    }
}

// ── XLSX: pure Rust ──────────────────────────────────────────────────────
//
// Phase 1 (production-ready core):
//   - Smart header formatting (bold + theme bg + center)
//   - Auto column widths with CJK character width estimation
//   - Number formatting (currency/percentage/date) via columns[].format
//   - Thin borders, banded rows, autofilter, freeze panes
//   - Text wrapping (columns[].wrap)
//   - Merged cells (structured row/col/rows/cols)
//   - Print settings (orientation, paper size, fit-to-page, header/footer)

/// CJK / wide character display width estimate (Excel column-width units).
fn char_display_width(c: char) -> f64 {
    if c.is_ascii() {
        return 1.0;
    }
    let code = c as u32;
    // Ranges that occupy ~2 monospace columns in East-Asian fonts:
    //   CJK Radicals through Yijing, Hiragana, Katakana, Bopomofo,
    //   Hangul Jamo & Syllables, CJK Unified Ideographs + Extensions,
    //   Fullwidth forms, enclosed alphanumerics, emoticons.
    if (0x1100..=0x115F).contains(&code)   // Hangul Jamo
        || (0x2E80..=0xA4CF).contains(&code)  // CJK + Yi
        || (0xAC00..=0xD7AF).contains(&code)  // Hangul Syllables
        || (0xF900..=0xFAFF).contains(&code)  // CJK Compatibility
        || (0xFF01..=0xFF60).contains(&code)  // Fullwidth forms
        || (0xFFE0..=0xFFE6).contains(&code)  // Fullwidth signs
        || (0x1F300..=0x1F9FF).contains(&code)
    // Emoticons / symbols
    {
        2.0
    } else {
        1.0
    }
}

/// Convert a 0-based column index to Excel column letter(s).
fn col_to_letter(mut col: u16) -> String {
    let mut chars: Vec<u8> = Vec::new();
    loop {
        chars.push(b'A' + (col % 26) as u8);
        if col < 26 {
            break;
        }
        col = col / 26 - 1;
    }
    chars.reverse();
    String::from_utf8(chars).unwrap()
}

/// Scan the first 1000 rows to compute per-column display widths.
/// Adds 2-char margin; floors at 8, ceilings at 60.
fn auto_column_widths(rows: &[Value], max_cols: usize) -> Vec<f64> {
    let mut widths = vec![0.0_f64; max_cols];
    let scan_limit = rows.len().min(1000);

    for row_val in rows.iter().take(scan_limit) {
        if let Some(row) = row_val.as_array() {
            for (ci, cell) in row.iter().enumerate() {
                if ci >= max_cols {
                    break;
                }
                let s = match cell {
                    Value::String(s) => s.clone(),
                    Value::Null => String::new(),
                    other => other.to_string(),
                };
                let w: f64 = s.chars().map(char_display_width).sum();
                if w > widths[ci] {
                    widths[ci] = w;
                }
            }
        }
    }

    widths
        .into_iter()
        .map(|w| (w + 2.0).clamp(8.0, 60.0))
        .collect()
}

/// Parse a "yyyy-mm-dd" or "yyyy/mm/dd" date string to an Excel serial
/// date number (days since 1899-12-30, with the Lotus 1900 leap-year bug).
fn parse_date_to_serial(s: &str) -> Result<f64, ()> {
    let (y_part, m_part, d_part): (&str, &str, &str) = if let Some(_rest) = s.strip_prefix('-') {
        return Err(());
    } else if let Some((y, rest)) = s.split_once('-') {
        let (m, d) = rest.split_once('-').ok_or(())?;
        (y, m, d)
    } else if let Some((y, rest)) = s.split_once('/') {
        let (m, d) = rest.split_once('/').ok_or(())?;
        (y, m, d)
    } else {
        return Err(());
    };

    let year: i32 = y_part.parse().map_err(|_| ())?;
    let month: u32 = m_part.parse().map_err(|_| ())?;
    let day: u32 = d_part.parse().map_err(|_| ())?;
    if year < 1900 || month < 1 || month > 12 || day < 1 || day > 31 {
        return Err(());
    }

    fn days_in_month(y: i32, m: u32) -> u32 {
        match m {
            4 | 6 | 9 | 11 => 30,
            2 => {
                if (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0) {
                    29
                } else {
                    28
                }
            }
            _ => 31,
        }
    }
    if day > days_in_month(year, month) {
        return Err(());
    }

    let mut days: i32 = 0;
    for y in 1900..year {
        if (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0) {
            days += 366;
        } else {
            days += 365;
        }
    }
    let cumulative: [i32; 13] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334, 365];
    let mut month_days = cumulative[(month - 1) as usize];
    if month > 2 && ((year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)) {
        month_days += 1;
    }
    days += month_days + (day as i32) - 1;
    // Excel serial: day 1 = 1900-01-01. Plus Lotus bug: 1900-02-29 exists.
    let serial = if days < 60 { days } else { days + 1 };
    Ok(serial as f64)
}

// ── Theme ────────────────────────────────────────────────────────────────

struct XlsxTheme {
    header_bg: &'static str,
    header_fg: &'static str,
    #[allow(dead_code)]
    border_color: &'static str,
    banded_bg: &'static str, // "" = no banding
}

const THEME_CORPORATE: XlsxTheme = XlsxTheme {
    header_bg: "#4472C4",
    header_fg: "#FFFFFF",
    border_color: "#D9E2F3",
    banded_bg: "#F2F7FB",
};
const THEME_TECH: XlsxTheme = XlsxTheme {
    header_bg: "#2D3748",
    header_fg: "#68D391",
    border_color: "#4A5568",
    banded_bg: "#EDF2F7",
};
const THEME_WARM: XlsxTheme = XlsxTheme {
    header_bg: "#ED8936",
    header_fg: "#FFFFFF",
    border_color: "#FBD38D",
    banded_bg: "#FFFBEB",
};
const THEME_MINIMAL: XlsxTheme = XlsxTheme {
    header_bg: "#FFFFFF",
    header_fg: "#1A202C",
    border_color: "#CBD5E0",
    banded_bg: "",
};

fn resolve_theme(input: &Value) -> &XlsxTheme {
    match input
        .pointer("/style/theme")
        .and_then(|v| v.as_str())
        .unwrap_or("corporate")
    {
        "tech" => &THEME_TECH,
        "warm" => &THEME_WARM,
        "minimal" => &THEME_MINIMAL,
        _ => &THEME_CORPORATE,
    }
}

// ── generate_xlsx (Phase 1 production rewrite) ───────────────────────────

fn generate_xlsx(input: &Value, path: &PathBuf) -> Result<String, String> {
    use rust_xlsxwriter::*;

    fn xlsx_hex_color(s: &str) -> &str {
        s.strip_prefix('#').unwrap_or(s)
    }

    // ── document-level config ─────────────────────────────────────────
    let theme = resolve_theme(input);
    let use_border = input
        .pointer("/style/border")
        .and_then(|v| v.as_str())
        .unwrap_or("thin")
        != "none";
    let header_freeze = input
        .pointer("/style/header_freeze")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let banded_rows = input
        .pointer("/style/banded_rows")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let doc_title = optional_str(input, "title").unwrap_or("");
    let print_cfg = input.pointer("/style/print");

    // ── build reusable Format objects ─────────────────────────────────
    let header_fmt = {
        let f = Format::new()
            .set_bold()
            .set_background_color(theme.header_bg)
            .set_font_color(theme.header_fg);
        if use_border {
            f.set_border(FormatBorder::Thin)
        } else {
            f
        }
    };
    let data_fmt = {
        let f = Format::new();
        if use_border {
            f.set_border(FormatBorder::Thin)
        } else {
            f
        }
    };
    let banded_fmt = {
        let f = Format::new();
        let f = if use_border {
            f.set_border(FormatBorder::Thin)
        } else {
            f
        };
        if banded_rows && !theme.banded_bg.is_empty() {
            f.set_background_color(theme.banded_bg)
        } else {
            f
        }
    };

    let mut workbook = Workbook::new();
    let sheets = input["sheets"]
        .as_array()
        .ok_or("`sheets` 字段必须是数组")?;

    // ── per-sheet loop ────────────────────────────────────────────────
    for sheet_val in sheets {
        let name = sheet_val["name"].as_str().unwrap_or("Sheet1").to_string();
        let rows = sheet_val["rows"]
            .as_array()
            .ok_or("每个 sheet 的 `rows` 必须是二维数组")?;
        let columns = sheet_val.get("columns").and_then(|v| v.as_array());
        let has_header = sheet_val
            .get("header")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let merged = sheet_val.get("merged_cells").and_then(|v| v.as_array());

        let worksheet = workbook.add_worksheet();
        worksheet
            .set_name(&name)
            .map_err(|e| format!("设置工作表名称失败: {e}"))?;

        // Determine maximum column count (columns def, or rows data)
        let max_cols: usize = {
            let from_cols = columns.map(|c| c.len()).unwrap_or(0);
            let from_rows = rows
                .iter()
                .filter_map(|r| r.as_array().map(|a| a.len()))
                .max()
                .unwrap_or(0);
            from_cols.max(from_rows)
        };

        // Row offset: if document title is present, the title occupies
        // row 0 and all content shifts down by 1 row.
        let row_offset: u32 = if !doc_title.is_empty() { 1 } else { 0 };

        // Charts (Phase 2)
        let charts = sheet_val.get("charts").and_then(|v| v.as_array());

        // Conditional formats (Phase 3)
        let conditional_formats = sheet_val
            .get("conditional_formats")
            .and_then(|v| v.as_array());

        // ── document title row (now FIRST — before header) ────────────
        if !doc_title.is_empty() && max_cols > 0 {
            let title_fmt = Format::new().set_bold().set_font_size(16);
            worksheet
                .merge_range(0, 0, 0, (max_cols - 1) as u16, doc_title, &title_fmt)
                .map_err(|e| format!("写入文档标题失败: {e}"))?;
        }

        // Build per-column Format objects
        struct ColFmt {
            fmt: Format,
            formula_template: Option<String>,
            is_date: bool,
        }
        let col_formats: Vec<ColFmt> = if let Some(cols) = columns {
            cols.iter()
                .map(|c| {
                    let col_type = c.get("format").and_then(|v| v.as_str()).unwrap_or("text");
                    let custom_nf = c.get("number_format").and_then(|v| v.as_str());
                    let wrap = c.get("wrap").and_then(|v| v.as_bool()).unwrap_or(false);
                    let formula = c
                        .get("formula")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());

                    let base = Format::new();
                    let base = if use_border {
                        base.set_border(FormatBorder::Thin)
                    } else {
                        base
                    };

                    let fmt = match col_type {
                        "number" | "currency" | "percentage" => {
                            let mut f = base.set_align(FormatAlign::Right);
                            if let Some(nf) = custom_nf {
                                f = f.set_num_format(nf);
                            } else if col_type == "currency" {
                                f = f.set_num_format("#,##0.00");
                            } else if col_type == "percentage" {
                                f = f.set_num_format("0.00%");
                            } else {
                                f = f.set_num_format("#,##0");
                            }
                            f
                        }
                        "date" => base.set_num_format(custom_nf.unwrap_or("yyyy-mm-dd")),
                        _ => {
                            // text – left-aligned is default
                            if wrap { base.set_text_wrap() } else { base }
                        }
                    };

                    ColFmt {
                        fmt,
                        formula_template: formula,
                        is_date: col_type == "date",
                    }
                })
                .collect()
        } else {
            Vec::new()
        };

        // ── header row ────────────────────────────────────────────────
        let data_start_row: u32;
        let has_header_row: bool;

        // Priority: columns[].label > rows[0] (if header != false)
        let use_col_labels = col_formats.iter().enumerate().any(|(i, _)| {
            columns
                .and_then(|cs| cs.get(i))
                .and_then(|c| c.get("label"))
                .and_then(|v| v.as_str())
                .is_some()
        });

        if use_col_labels && has_header {
            // Write labels from columns definition
            for (ci, _) in col_formats.iter().enumerate() {
                let label = columns
                    .and_then(|cs| cs.get(ci))
                    .and_then(|c| c.get("label"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                worksheet
                    .write_with_format(row_offset, ci as u16, label, &header_fmt)
                    .map_err(|e| format!("写入列标题失败: {e}"))?;
            }
            data_start_row = 1 + row_offset;
            has_header_row = true;
        } else if has_header && !rows.is_empty() {
            // Treat rows[0] as header
            if let Some(header_row) = rows[0].as_array() {
                for (ci, cell) in header_row.iter().enumerate() {
                    let s = match cell {
                        Value::String(s) => s.as_str(),
                        Value::Null => "",
                        other => {
                            // This is a rare path; we convert to string inline.
                            // We can't return a reference to a temporary, so write directly.
                            worksheet
                                .write_with_format(
                                    row_offset,
                                    ci as u16,
                                    &other.to_string(),
                                    &header_fmt,
                                )
                                .map_err(|e| format!("写入表头行失败: {e}"))?;
                            continue;
                        }
                    };
                    worksheet
                        .write_with_format(row_offset, ci as u16, s, &header_fmt)
                        .map_err(|e| format!("写入表头行失败: {e}"))?;
                }
            }
            data_start_row = 1 + row_offset;
            has_header_row = true;
        } else {
            data_start_row = row_offset;
            has_header_row = false;
        }

        // ── data rows ─────────────────────────────────────────────────
        let row_start_idx: usize = if has_header_row && !use_col_labels {
            1 // skip rows[0] which was used as header
        } else {
            0
        };

        let last_data_row: u32 = data_start_row + (rows.len() - row_start_idx) as u32;
        // saturating sub: if no data rows, last_data_row stays at data_start_row
        let last_data_row = if last_data_row > data_start_row {
            last_data_row - 1
        } else {
            data_start_row
        };

        for (ri, row_val) in rows.iter().enumerate().skip(row_start_idx) {
            let excel_row = data_start_row + (ri - row_start_idx) as u32;
            let row = row_val.as_array().ok_or("每行必须是数组")?;
            let is_even = (excel_row as usize) % 2 == 0;

            let base_fmt = if is_even && banded_rows && !theme.banded_bg.is_empty() {
                &banded_fmt
            } else {
                &data_fmt
            };

            for (ci, cell) in row.iter().enumerate() {
                let col_u16 = ci as u16;

                // Check for column-level formula
                if let Some(cf) = col_formats.get(ci) {
                    if let Some(ref tmpl) = cf.formula_template {
                        let col_letter = col_to_letter(col_u16);
                        let formula_str = tmpl
                            .replace("{{row}}", &(excel_row + 1).to_string())
                            .replace("{{col}}", &col_letter)
                            .replace("{{last_row}}", &(last_data_row + 1).to_string());
                        worksheet
                            .write_formula_with_format(
                                excel_row,
                                col_u16,
                                formula_str.as_str(),
                                &cf.fmt,
                            )
                            .map_err(|e| format!("写入公式失败: {e}"))?;
                        continue;
                    }
                }

                // Determine format for this cell
                let cell_fmt: &Format = if let Some(cf) = col_formats.get(ci) {
                    if matches!(cf.formula_template, Some(_)) {
                        // formula handled above; shouldn't reach here
                        base_fmt
                    } else {
                        &cf.fmt
                    }
                } else {
                    base_fmt
                };

                match cell {
                    Value::Null => {
                        worksheet
                            .write_with_format(excel_row, col_u16, "", cell_fmt)
                            .map_err(|e| format!("写入空值失败: {e}"))?;
                    }
                    Value::Number(n) => {
                        if let Some(i) = n.as_i64() {
                            worksheet
                                .write_with_format(excel_row, col_u16, i, cell_fmt)
                                .map_err(|e| format!("写入数字失败: {e}"))?;
                        } else if let Some(f) = n.as_f64() {
                            worksheet
                                .write_with_format(excel_row, col_u16, f, cell_fmt)
                                .map_err(|e| format!("写入数字失败: {e}"))?;
                        }
                    }
                    Value::String(s) => {
                        // When column format is "date", parse the string as a date
                        // and write it as an Excel serial number so that number
                        // formatting, sorting, and formulas work correctly.
                        let is_date_col = col_formats.get(ci).map(|cf| cf.is_date).unwrap_or(false);
                        if is_date_col {
                            match parse_date_to_serial(s) {
                                Ok(serial) => {
                                    worksheet
                                        .write_with_format(excel_row, col_u16, serial, cell_fmt)
                                        .map_err(|e| format!("写入日期失败: {e}"))?;
                                }
                                Err(()) => {
                                    // Fall back to string if parsing fails
                                    worksheet
                                        .write_with_format(excel_row, col_u16, s.as_str(), cell_fmt)
                                        .map_err(|e| format!("写入文本失败: {e}"))?;
                                }
                            }
                        } else {
                            worksheet
                                .write_with_format(excel_row, col_u16, s.as_str(), cell_fmt)
                                .map_err(|e| format!("写入文本失败: {e}"))?;
                        }
                    }
                    Value::Bool(b) => {
                        worksheet
                            .write_with_format(excel_row, col_u16, *b, cell_fmt)
                            .map_err(|e| format!("写入布尔值失败: {e}"))?;
                    }
                    _ => {
                        let s = cell.to_string();
                        worksheet
                            .write_with_format(excel_row, col_u16, s.as_str(), cell_fmt)
                            .map_err(|e| format!("写入值失败: {e}"))?;
                    }
                }
            }
        }

        // ── autofilter ────────────────────────────────────────────────
        if has_header_row && rows.len() > row_start_idx + 1 {
            let last_col = if max_cols > 0 {
                (max_cols - 1) as u16
            } else {
                0
            };
            worksheet
                .autofilter(data_start_row - 1, 0, last_data_row, last_col)
                .map_err(|e| format!("设置自动筛选失败: {e}"))?;
        }

        // ── freeze panes ──────────────────────────────────────────────
        if header_freeze && has_header_row {
            worksheet
                .set_freeze_panes(data_start_row, 0)
                .map_err(|e| format!("设置冻结窗格失败: {e}"))?;
        }

        // ── column widths ─────────────────────────────────────────────
        if let Some(cols) = columns {
            // Use explicitly-set widths where provided; auto-compute the rest
            let auto_widths = auto_column_widths(rows, max_cols);
            for ci in 0..max_cols {
                let width: f64 = cols
                    .get(ci)
                    .and_then(|c| c.get("width"))
                    .and_then(|v| v.as_f64())
                    .unwrap_or_else(|| *auto_widths.get(ci).unwrap_or(&12.0));
                worksheet
                    .set_column_width(ci as u16, width)
                    .map_err(|e| format!("设置列宽失败: {e}"))?;
            }
        } else {
            // Auto-compute all column widths
            let auto_widths = auto_column_widths(rows, max_cols);
            for (ci, w) in auto_widths.iter().enumerate() {
                worksheet
                    .set_column_width(ci as u16, *w)
                    .map_err(|e| format!("设置列宽失败: {e}"))?;
            }
        }

        // ── merged cells ──────────────────────────────────────────────
        if let Some(merges) = merged {
            for m in merges {
                let r0 = m.get("row").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                let c0 = m.get("col").and_then(|v| v.as_u64()).unwrap_or(0) as u16;
                let r_span = m.get("rows").and_then(|v| v.as_u64()).unwrap_or(1) as u32;
                let c_span = m.get("cols").and_then(|v| v.as_u64()).unwrap_or(1) as u16;
                let r1 = if r_span > 0 { r0 + r_span - 1 } else { r0 };
                let c1 = if c_span > 0 { c0 + c_span - 1 } else { c0 };
                worksheet
                    .merge_range(r0 + row_offset, c0, r1 + row_offset, c1, "", &data_fmt)
                    .map_err(|e| format!("合并单元格失败: {e}"))?;
            }
        }

        // ── charts (Phase 2) ────────────────────────────────────────────
        if let Some(charts) = charts {
            for chart_val in charts {
                let chart_type_str = chart_val
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("bar");
                let chart_type = match chart_type_str {
                    "bar" => ChartType::Bar,
                    "line" => ChartType::Line,
                    "pie" => ChartType::Pie,
                    "stacked_bar" => ChartType::BarStacked,
                    _ => ChartType::Bar,
                };

                let mut chart = Chart::new(chart_type);

                if let Some(title) = chart_val.get("title").and_then(|v| v.as_str()) {
                    chart.title().set_name(title);
                }

                // Build fully-qualified Excel ranges: "Sheet1!$B$2:$B$13"
                let qualify = |raw: &str| -> String {
                    let raw = raw.strip_prefix('=').unwrap_or(raw);
                    // If already fully qualified (contains '!'), use as-is
                    if raw.contains('!') {
                        return raw.to_string();
                    }
                    // Parse "A2:A10" or "$A$2:$A$10" into Sheet1!$A$2:$A$10
                    let parts: Vec<&str> = raw.split(':').collect();
                    if parts.len() != 2 {
                        return raw.to_string();
                    }
                    let left = parts[0].trim_start_matches('$');
                    let right = parts[1].trim_start_matches('$');
                    // Quote sheet name if it contains spaces or special chars
                    let sheet = if name.contains(char::is_whitespace) {
                        format!("'{}'", name)
                    } else {
                        name.clone()
                    };
                    format!("{}!${}:${}", sheet, left, right)
                };
                let cat_range = chart_val
                    .get("categories_range")
                    .and_then(|v| v.as_str())
                    .map(|r| qualify(r));
                let val_range = chart_val
                    .get("values_range")
                    .and_then(|v| v.as_str())
                    .map(|r| qualify(r));
                match (cat_range.as_deref(), val_range.as_deref()) {
                    (Some(cat), Some(val)) => {
                        chart.add_series().set_categories(cat).set_values(val);
                    }
                    (_, Some(val)) => {
                        chart.add_series().set_values(val);
                    }
                    _ => {}
                }

                let pos_row = chart_val
                    .get("position")
                    .and_then(|v| v.get("row"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32
                    + row_offset;
                let pos_col = chart_val
                    .get("position")
                    .and_then(|v| v.get("col"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u16;

                // Apply size if specified
                if let Some(size) = chart_val.get("size") {
                    if let (Some(w), Some(h)) = (
                        size.get("width").and_then(|v| v.as_f64()),
                        size.get("height").and_then(|v| v.as_f64()),
                    ) {
                        let w_px = w.round().clamp(1.0, f64::from(u32::MAX)) as u32;
                        let h_px = h.round().clamp(1.0, f64::from(u32::MAX)) as u32;
                        chart.set_width(w_px).set_height(h_px);
                    }
                }

                worksheet
                    .insert_chart(pos_row, pos_col, &chart)
                    .map_err(|e| format!("插入图表失败 ({chart_type_str}): {e}"))?;
            }
        }

        // ── conditional formats (Phase 3) ────────────────────────────
        if let Some(cfs) = conditional_formats {
            for cf_val in cfs {
                let cf_type = cf_val
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("data_bar");

                let range = cf_val.get("range");
                let r0 = range
                    .and_then(|r| r.get("row"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32
                    + row_offset;
                let c0 = range
                    .and_then(|r| r.get("col"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u16;
                let r_span = range
                    .and_then(|r| r.get("rows"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(1) as u32;
                let c_span = range
                    .and_then(|r| r.get("cols"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(1) as u16;
                let r1 = if r_span > 0 { r0 + r_span - 1 } else { r0 };
                let c1 = if c_span > 0 { c0 + c_span - 1 } else { c0 };

                match cf_type {
                    "data_bar" => {
                        let color = cf_val
                            .get("color")
                            .and_then(|v| v.as_str())
                            .unwrap_or("#4472C4");
                        let cf =
                            ConditionalFormatDataBar::new().set_fill_color(xlsx_hex_color(color));
                        worksheet
                            .add_conditional_format(r0, c0, r1, c1, &cf)
                            .map_err(|e| format!("添加数据条条件格式失败: {e}"))?;
                    }
                    "cell_highlight" => {
                        let condition = cf_val
                            .get("condition")
                            .and_then(|v| v.as_str())
                            .unwrap_or("greater_than");
                        let value = cf_val.get("value").and_then(|v| v.as_str()).unwrap_or("0");
                        let bg_color = cf_val
                            .get("color")
                            .and_then(|v| v.as_str())
                            .unwrap_or("#FFC7CE");

                        let make_highlight_fmt =
                            || Format::new().set_background_color(xlsx_hex_color(bg_color));

                        let cf = match value.parse::<f64>() {
                            Ok(n) => {
                                let rule = match condition {
                                    "greater_than" => ConditionalFormatCellRule::GreaterThan(n),
                                    "less_than" => ConditionalFormatCellRule::LessThan(n),
                                    "equal_to" => ConditionalFormatCellRule::EqualTo(n),
                                    _ => ConditionalFormatCellRule::GreaterThan(n),
                                };
                                ConditionalFormatCell::new()
                                    .set_rule(rule)
                                    .set_format(make_highlight_fmt())
                            }
                            Err(_) => {
                                let v = value.to_string();
                                let rule = match condition {
                                    "greater_than" => ConditionalFormatCellRule::GreaterThan(v),
                                    "less_than" => ConditionalFormatCellRule::LessThan(v),
                                    "equal_to" => ConditionalFormatCellRule::EqualTo(v),
                                    _ => ConditionalFormatCellRule::GreaterThan(v),
                                };
                                ConditionalFormatCell::new()
                                    .set_rule(rule)
                                    .set_format(make_highlight_fmt())
                            }
                        };
                        worksheet
                            .add_conditional_format(r0, c0, r1, c1, &cf)
                            .map_err(|e| format!("添加单元格高亮条件格式失败: {e}"))?;
                    }
                    _ => {}
                }
            }
        }
    }

    // ── print settings (document-level) ───────────────────────────────
    if let Some(pc) = print_cfg {
        // Apply to all worksheets — rust_xlsxwriter sets print on the
        // active worksheet, so we iterate again.
        // For simplicity, we capture settings here and apply to the
        // last sheet, or we can store a reference. Since print settings
        // in xlsxwriter apply per-sheet, this is a best-effort for now.
        if let Some(orientation) = pc.get("orientation").and_then(|v| v.as_str()) {
            // We'll apply to the first sheet as a reasonable default.
            // A full per-sheet print config can come in Phase 3.
            if let Ok(first_sheet) = workbook.worksheet_from_index(0) {
                if orientation == "landscape" {
                    first_sheet.set_landscape();
                }
            }
        }
        if let Some(ps) = pc.get("paper_size").and_then(|v| v.as_str()) {
            if let Ok(first_sheet) = workbook.worksheet_from_index(0) {
                let size: u8 = match ps {
                    "A3" => 8,
                    "A4" => 9,
                    "Letter" => 1,
                    _ => 9,
                };
                first_sheet.set_paper_size(size);
            }
        }
        if let Some(fit) = pc.get("fit_to_width").and_then(|v| v.as_u64()) {
            if let Ok(first_sheet) = workbook.worksheet_from_index(0) {
                first_sheet.set_print_fit_to_pages(fit as u16, 0);
            }
        }
        if let Some(hdr) = pc.get("header").and_then(|v| v.as_str()) {
            if let Ok(first_sheet) = workbook.worksheet_from_index(0) {
                first_sheet.set_header(hdr);
            }
        }
        if let Some(ftr) = pc.get("footer").and_then(|v| v.as_str()) {
            if let Ok(first_sheet) = workbook.worksheet_from_index(0) {
                first_sheet.set_footer(ftr);
            }
        }
        if let Some(margins) = pc.get("margins") {
            if let Ok(first_sheet) = workbook.worksheet_from_index(0) {
                let left = margins.get("left").and_then(|v| v.as_f64()).unwrap_or(0.7);
                let right = margins.get("right").and_then(|v| v.as_f64()).unwrap_or(0.7);
                let top = margins.get("top").and_then(|v| v.as_f64()).unwrap_or(0.75);
                let bottom = margins
                    .get("bottom")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.75);
                let header_margin = margins
                    .get("header")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.3);
                let footer_margin = margins
                    .get("footer")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.3);
                first_sheet.set_margins(left, right, top, bottom, header_margin, footer_margin);
            }
        }
    }

    workbook
        .save(path)
        .map_err(|e| format!("保存 XLSX 失败: {e}"))?;
    Ok("rust_xlsxwriter".to_string())
}

// ── DOCX ─────────────────────────────────────────────────────────────────

fn generate_docx(input: &Value, path: &PathBuf) -> Result<String, String> {
    // Try Python first
    match generate_via_python("docx", input, path) {
        Ok(()) => return Ok("python-docx".to_string()),
        Err(py_err) => {
            // Fall back to Rust minimal DOCX
            match generate_docx_rust_fallback(input, path) {
                Ok(()) => return Ok("rust-minimal-docx".to_string()),
                Err(rust_err) => {
                    return Err(format!(
                        "DOCX 生成失败。\nPython 引擎: {py_err}\nRust 兜底: {rust_err}"
                    ));
                }
            }
        }
    }
}

/// Minimal DOCX using raw OOXML ZIP assembly (headings, paragraphs, simple lists).
fn generate_docx_rust_fallback(input: &Value, path: &PathBuf) -> Result<(), String> {
    use std::io::Write;

    let blocks = input["blocks"]
        .as_array()
        .ok_or("`blocks` 字段必须是数组")?;
    let title = optional_str(input, "title").unwrap_or("");

    let mut doc_xml = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:body>
"#,
    );

    if !title.is_empty() {
        doc_xml.push_str(&format!(
            r#"<w:p><w:pPr><w:pStyle w:val="Title"/></w:pPr><w:r><w:t>{}</w:t></w:r></w:p>"#,
            xml_escape(title)
        ));
    }

    for block in blocks {
        match block["type"].as_str().unwrap_or("paragraph") {
            "heading" => {
                let level = block["level"].as_u64().unwrap_or(1).min(6);
                let text = block["text"].as_str().unwrap_or("");
                doc_xml.push_str(&format!(
                    r#"<w:p><w:pPr><w:pStyle w:val="Heading{level}"/></w:pPr><w:r><w:t>{}</w:t></w:r></w:p>"#,
                    xml_escape(text)
                ));
            }
            "paragraph" => {
                let text = block["text"].as_str().unwrap_or("");
                doc_xml.push_str(&format!(
                    r#"<w:p><w:r><w:t>{}</w:t></w:r></w:p>"#,
                    xml_escape(text)
                ));
            }
            "list" => {
                let style = block["style"].as_str().unwrap_or("bullet");
                let style_name = if style == "number" {
                    "ListNumber"
                } else {
                    "ListBullet"
                };
                let items = block["items"].as_array();
                if let Some(items) = items {
                    for item in items {
                        let text = item.as_str().unwrap_or("");
                        doc_xml.push_str(&format!(
                            r#"<w:p><w:pPr><w:pStyle w:val="{style_name}"/></w:pPr><w:r><w:t>{}</w:t></w:r></w:p>"#,
                            xml_escape(text)
                        ));
                    }
                }
            }
            "table" => {
                return Err("表格生成需要 Python 引擎。请安装 Python 3.8+ 后重试".to_string());
            }
            _ => {}
        }
    }

    doc_xml.push_str("</w:body></w:document>");

    let content_types = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>"#;

    let rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#;

    let doc_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
</Relationships>"#;

    let file = std::fs::File::create(path).map_err(|e| format!("创建文件失败: {e}"))?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default();

    zip.start_file("[Content_Types].xml", opts)
        .map_err(|e| e.to_string())?;
    zip.write_all(content_types.as_bytes())
        .map_err(|e| e.to_string())?;

    zip.start_file("_rels/.rels", opts)
        .map_err(|e| e.to_string())?;
    zip.write_all(rels.as_bytes()).map_err(|e| e.to_string())?;

    zip.start_file("word/document.xml", opts)
        .map_err(|e| e.to_string())?;
    zip.write_all(doc_xml.as_bytes())
        .map_err(|e| e.to_string())?;

    zip.start_file("word/_rels/document.xml.rels", opts)
        .map_err(|e| e.to_string())?;
    zip.write_all(doc_rels.as_bytes())
        .map_err(|e| e.to_string())?;

    zip.finish().map_err(|e| e.to_string())?;
    Ok(())
}

// ── PPTX ─────────────────────────────────────────────────────────────────

fn generate_pptx(input: &Value, path: &PathBuf) -> Result<String, String> {
    generate_via_python("pptx", input, path)?;
    Ok("python-pptx".to_string())
}

// ── Python subprocess helper ─────────────────────────────────────────────

/// Spawn the office venv Python with a stdin JSON payload, return stdout.
fn generate_via_python(format: &str, input: &Value, path: &PathBuf) -> Result<(), String> {
    let python_exe = crate::python_env::resolve_python_for_office()?;
    let script = find_office_script(format)?;

    // Serialise payload: only data fields (no path — that goes via --output)
    let data_payload = match format {
        "docx" => {
            let title = optional_str(input, "title").unwrap_or("");
            serde_json::json!({
                "title": title,
                "blocks": &input["blocks"],
            })
        }
        "pptx" => {
            let title = optional_str(input, "title").unwrap_or("");
            let subtitle = optional_str(input, "subtitle").unwrap_or("");
            let theme = input
                .get("theme")
                .cloned()
                .unwrap_or(Value::String("dark".to_string()));
            serde_json::json!({
                "title": title,
                "subtitle": subtitle,
                "theme": theme,
                "slides": &input["slides"],
            })
        }
        _ => input.clone(),
    };

    let mut child = Command::new(&python_exe)
        .env("PYTHONIOENCODING", "utf-8")
        .arg(&script)
        .arg("--output")
        .arg(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("启动 Python 脚本失败: {e}"))?;

    // Write JSON payload to stdin
    {
        let stdin = child.stdin.as_mut().unwrap();
        serde_json::to_writer(stdin, &data_payload).map_err(|e| format!("写入 stdin 失败: {e}"))?;
    }

    let exit_status = child
        .wait_timeout(Duration::from_secs(120))
        .map_err(|e| format!("等待 Python 脚本失败: {e}"))?
        .ok_or_else(|| "Python 脚本执行超时 (120s)".to_string())?;

    if !exit_status.success() {
        // Read stderr from the already-exited child
        let stderr_output = child
            .stderr
            .take()
            .and_then(|mut pipe| {
                use std::io::Read;
                let mut buf = Vec::new();
                pipe.read_to_end(&mut buf).ok()?;
                Some(String::from_utf8_lossy(&buf).to_string())
            })
            .unwrap_or_default();
        return Err(format!(
            "Python 脚本执行失败 (exit {:?}):\n{stderr_output}",
            exit_status.code()
        ));
    }

    Ok(())
}

/// Embedded Python scripts version — bump when scripts change.
const SCRIPTS_VERSION: &str = "9";

/// Resolve the Python script path for a given format.
fn find_office_script(format: &str) -> Result<PathBuf, String> {
    let scripts_dir = crate::python_env::office_venv_dir()
        .map(|d| d.join("scripts"))
        .ok_or_else(|| "无法确定 scripts 目录".to_string())?;

    let script_path = scripts_dir.join(format!("write_{format}.py"));
    let marker = scripts_dir.join(".scripts-installed-version");

    let need_install = !script_path.exists()
        || std::fs::read_to_string(&marker)
            .ok()
            .map_or(true, |v| v.trim() != SCRIPTS_VERSION);

    if need_install {
        install_embedded_scripts(&scripts_dir)?;
    }
    Ok(script_path)
}

/// Write the `include_str!`-embedded Python scripts to disk.
fn install_embedded_scripts(dir: &PathBuf) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("创建 scripts 目录失败: {e}"))?;

    // Single-file scripts
    let scripts: &[(&str, &str)] = &[
        ("write_docx.py", WRITE_DOCX_PY),
        ("write_pptx.py", WRITE_PPTX_PY),
    ];

    for (name, content) in scripts {
        let path = dir.join(name);
        std::fs::write(&path, content).map_err(|e| format!("写入脚本 {name} 失败: {e}"))?;
    }

    // pptx_engine/ package directory
    let engine_dir = dir.join("pptx_engine");
    std::fs::create_dir_all(&engine_dir).map_err(|e| format!("创建 pptx_engine 目录失败: {e}"))?;

    let engine_modules: &[(&str, &str)] = &[
        ("__init__.py", PPTX_ENGINE_INIT),
        ("theme.py", PPTX_ENGINE_THEME),
        ("layout.py", PPTX_ENGINE_LAYOUT),
        ("blocks.py", PPTX_ENGINE_BLOCKS),
        ("charts.py", PPTX_ENGINE_CHARTS),
        ("mpl.py", PPTX_ENGINE_MPL),
        ("template.py", PPTX_ENGINE_TEMPLATE),
    ];

    for (name, content) in engine_modules {
        let path = engine_dir.join(name);
        std::fs::write(&path, content).map_err(|e| format!("写入 pptx_engine/{name} 失败: {e}"))?;
    }

    let marker = dir.join(".scripts-installed-version");
    std::fs::write(&marker, SCRIPTS_VERSION).map_err(|e| format!("写入脚本版本标记失败: {e}"))?;

    Ok(())
}

// ── Embedded Python scripts ──────────────────────────────────────────────

const WRITE_DOCX_PY: &str = include_str!("../../assets/scripts/write_docx.py");
const WRITE_PPTX_PY: &str = include_str!("../../assets/scripts/write_pptx.py");

const PPTX_ENGINE_INIT: &str = include_str!("../../assets/scripts/pptx_engine/__init__.py");
const PPTX_ENGINE_THEME: &str = include_str!("../../assets/scripts/pptx_engine/theme.py");
const PPTX_ENGINE_LAYOUT: &str = include_str!("../../assets/scripts/pptx_engine/layout.py");
const PPTX_ENGINE_BLOCKS: &str = include_str!("../../assets/scripts/pptx_engine/blocks.py");
const PPTX_ENGINE_CHARTS: &str = include_str!("../../assets/scripts/pptx_engine/charts.py");
const PPTX_ENGINE_MPL: &str = include_str!("../../assets/scripts/pptx_engine/mpl.py");
const PPTX_ENGINE_TEMPLATE: &str = include_str!("../../assets/scripts/pptx_engine/template.py");

// ── Helpers ──────────────────────────────────────────────────────────────

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
