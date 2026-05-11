//! `write_office` tool — generate .xlsx / .docx / .pptx files.
//!
//! Architecture:
//! - XLSX: pure Rust via `rust_xlsxwriter` (no Python dependency).
//! - DOCX: Python + `python-docx` (primary); pure-Rust minimal OOXML fallback.
//! - PPTX: Python + `python-pptx` only; clear error if Python unavailable.
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
        "Generate .xlsx / .docx / .pptx files from structured JSON data. XLSX uses pure Rust (no Python needed). DOCX/PPTX require Python 3.8+ with python-docx / python-pptx."
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
                "sheets": {
                    "type": "array",
                    "description": "XLSX sheets: [{ name, rows: [[value...]] }]",
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

    async fn execute(
        &self,
        input: Value,
        context: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let format = required_str(&input, "format")?;
        let path_str = required_str(&input, "path")?;
        let output_path = context.resolve_path(path_str)?;

        // Ensure parent directory exists
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                ToolError::execution_failed(format!(
                    "无法创建目录 {}: {}",
                    parent.display(),
                    e
                ))
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

fn generate_xlsx(input: &Value, path: &PathBuf) -> Result<String, String> {
    use rust_xlsxwriter::*;

    let mut workbook = Workbook::new();
    let sheets = input["sheets"].as_array().ok_or("`sheets` 字段必须是数组")?;

    for sheet_val in sheets {
        let name = sheet_val["name"]
            .as_str()
            .unwrap_or("Sheet1")
            .to_string();
        let rows = sheet_val["rows"].as_array().ok_or("每个 sheet 的 `rows` 必须是二维数组")?;

        let worksheet = workbook.add_worksheet();
        worksheet.set_name(&name).map_err(|e| format!("设置工作表名称失败: {e}"))?;

        for (row_idx, row_val) in rows.iter().enumerate() {
            let row = row_val.as_array().ok_or("每行必须是数组")?;
            for (col_idx, cell) in row.iter().enumerate() {
                let row_u32 = row_idx as u32;
                let col_u16 = col_idx as u16;
                match cell {
                    Value::Null => {}
                    Value::Number(n) => {
                        if let Some(i) = n.as_i64() {
                            worksheet
                                .write(row_u32, col_u16, i)
                                .map_err(|e| format!("写入数字失败: {e}"))?;
                        } else if let Some(f) = n.as_f64() {
                            worksheet
                                .write(row_u32, col_u16, f)
                                .map_err(|e| format!("写入数字失败: {e}"))?;
                        }
                    }
                    Value::String(s) => {
                        worksheet
                            .write(row_u32, col_u16, s.as_str())
                            .map_err(|e| format!("写入文本失败: {e}"))?;
                    }
                    Value::Bool(b) => {
                        worksheet
                            .write(row_u32, col_u16, *b)
                            .map_err(|e| format!("写入布尔值失败: {e}"))?;
                    }
                    _ => {
                        let s = cell.to_string();
                        worksheet
                            .write(row_u32, col_u16, s.as_str())
                            .map_err(|e| format!("写入值失败: {e}"))?;
                    }
                }
            }
        }
    }

    workbook.save(path).map_err(|e| format!("保存 XLSX 失败: {e}"))?;
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

    let blocks = input["blocks"].as_array().ok_or("`blocks` 字段必须是数组")?;
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
                let style_name = if style == "number" { "ListNumber" } else { "ListBullet" };
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

    zip.start_file("[Content_Types].xml", opts).map_err(|e| e.to_string())?;
    zip.write_all(content_types.as_bytes()).map_err(|e| e.to_string())?;

    zip.start_file("_rels/.rels", opts).map_err(|e| e.to_string())?;
    zip.write_all(rels.as_bytes()).map_err(|e| e.to_string())?;

    zip.start_file("word/document.xml", opts).map_err(|e| e.to_string())?;
    zip.write_all(doc_xml.as_bytes()).map_err(|e| e.to_string())?;

    zip.start_file("word/_rels/document.xml.rels", opts).map_err(|e| e.to_string())?;
    zip.write_all(doc_rels.as_bytes()).map_err(|e| e.to_string())?;

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
    let venv_python = crate::python_env::ensure_office_venv()?;
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
            let theme = input.get("theme").cloned().unwrap_or(Value::String("dark".to_string()));
            serde_json::json!({
                "title": title,
                "subtitle": subtitle,
                "theme": theme,
                "slides": &input["slides"],
            })
        }
        _ => input.clone(),
    };

    let mut child = Command::new(&venv_python)
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
        serde_json::to_writer(stdin, &data_payload)
            .map_err(|e| format!("写入 stdin 失败: {e}"))?;
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
        return Err(format!("Python 脚本执行失败 (exit {:?}):\n{stderr_output}", exit_status.code()));
    }

    Ok(())
}

/// Embedded Python scripts version — bump when scripts change.
const SCRIPTS_VERSION: &str = "7";

/// Resolve the Python script path for a given format.
fn find_office_script(format: &str) -> Result<PathBuf, String> {
    let scripts_dir = crate::python_env::office_venv_dir()
        .map(|d| d.join("scripts"))
        .ok_or_else(|| "无法确定 scripts 目录".to_string())?;

    let script_path = scripts_dir.join(format!("write_{format}.py"));
    let marker = scripts_dir.join(".scripts-installed-version");

    let need_install = !script_path.exists()
        || std::fs::read_to_string(&marker).ok().map_or(true, |v| v.trim() != SCRIPTS_VERSION);

    if need_install {
        install_embedded_scripts(&scripts_dir)?;
    }
    Ok(script_path)
}

/// Write the `include_str!`-embedded Python scripts to disk.
fn install_embedded_scripts(dir: &PathBuf) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("创建 scripts 目录失败: {e}"))?;

    let scripts: &[(&str, &str)] = &[
        ("write_docx.py", WRITE_DOCX_PY),
        ("write_pptx.py", WRITE_PPTX_PY),
    ];

    for (name, content) in scripts {
        let path = dir.join(name);
        std::fs::write(&path, content)
            .map_err(|e| format!("写入脚本 {name} 失败: {e}"))?;
    }

    let marker = dir.join(".scripts-installed-version");
    std::fs::write(&marker, SCRIPTS_VERSION)
        .map_err(|e| format!("写入脚本版本标记失败: {e}"))?;

    Ok(())
}

// ── Embedded Python scripts ──────────────────────────────────────────────

const WRITE_DOCX_PY: &str = include_str!("../../assets/scripts/write_docx.py");
const WRITE_PPTX_PY: &str = include_str!("../../assets/scripts/write_pptx.py");

// ── Helpers ──────────────────────────────────────────────────────────────

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
