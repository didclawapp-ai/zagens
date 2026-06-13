//! Input schemas for the office tool family (kernel-v2 M2).

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::tools::tool_schema::derived_input_schema;

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
struct ReadOfficeInput {
    #[schemars(description = "Path to the file (relative to workspace or absolute)")]
    pub path: String,
    #[schemars(
        description = "XLSX/ODS: sheet name or 0-based index (default: first sheet). Lists all sheet names in metadata when omitted."
    )]
    pub sheet: Option<String>,
    #[schemars(description = "PDF only: page range, e.g. \"1-5\" or \"10\"")]
    pub pages: Option<String>,
    #[schemars(description = "XLSX/CSV: first data row to read (1-based, default: 1)")]
    pub start_row: Option<u64>,
    #[schemars(description = "XLSX/CSV: maximum rows to return (default: 2000, max: 5000)")]
    pub limit: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(inline)]
struct LoadOfficePayloadInput {
    #[schemars(description = "Path to the office file (e.g. deliverables/report.pptx)")]
    pub path: String,
}

#[must_use]
pub fn read_office_input_schema() -> Value {
    derived_input_schema::<ReadOfficeInput>()
}

#[must_use]
pub fn load_office_payload_input_schema() -> Value {
    derived_input_schema::<LoadOfficePayloadInput>()
}

/// `write_office` accepts loosely typed nested JSON; keep the legacy hand-shaped schema
/// byte-stable rather than deriving from Rust structs.
#[must_use]
pub fn write_office_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "format": {
                "type": "string",
                "enum": ["xlsx", "docx", "pptx", "pdf"],
                "description": "Output format"
            },
            "path": {
                "type": "string",
                "description": "Output file path (optional). Default: deliverables/<title-or-timestamp>.<format> (auto-increment on collision)"
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
            "page": {
                "type": "object",
                "description": "DOCX page setup: { paper?: A4|A3|Letter, orientation?: portrait|landscape, margins?: { top?, right?, bottom?, left? } }"
            },
            "header": {
                "type": "object",
                "description": "DOCX page header: { text?: str } or { left?, center?, right? }"
            },
            "footer": {
                "type": "object",
                "description": "DOCX page footer: { text?: str } or { left?, center?, right? }. &P=页码, &N=总页数"
            },
            "font": {
                "type": "object",
                "description": "DOCX global font: { name?: str, size?: num } (e.g. 微软雅黑, 11)"
            },
            "sheets": {
                "type": "array",
                "description": "XLSX sheets: [{ name, source?: path or { path, sheet?, start_row?, limit? } — loads CSV/TSV/XLSX rows without retyping, rows?: [[value...]] (omit when source set), header?, columns?, merged_cells?, charts?, conditional_formats? }]",
                "items": { "type": "object" }
            },
            "blocks": {
                "type": "array",
                "description": "DOCX/PDF body blocks. heading: {level,text}. paragraph: {text} or {runs:[{text,bold?,italic?,color? #RRGGBB,size? pt}], align?: left|center|right|justify, page_break_before?: bool}. list: {style: bullet|number, items: [str | {text, subitems?}]}. table: {headers?, rows:[[]]}. image: {path, width?, height? px @96dpi, caption?}. page_break: {} (PDF). toc: {title?} (DOCX only; Word field placeholder). DOCX-only: table style name; Rust DOCX fallback: heading, plain paragraph, list, table only.",
                "items": { "type": "object" }
            },
            "slides": {
                "type": "array",
                "description": "PPTX slides: [{ title, bullets?: [str], table?: { headers:[str], rows:[[value]] }, chart?: { type: bar|line|pie|stacked_bar|stacked_bar_pct|area|scatter|donut, categories:[str], series:[{name,values:[num]}], chart_title?, x_label?, y_label?, data_labels? }, notes?, theme? }]",
                "items": { "type": "object" }
            }
        },
        "required": ["format"]
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::office_payload::LoadOfficePayloadTool;
    use crate::tools::office_read::ReadOfficeTool;
    use crate::tools::office_write::WriteOfficeTool;
    use crate::tools::schema_sanitize;
    use crate::tools::spec::ToolSpec;

    fn model_visible_input_schema(tool: &dyn ToolSpec) -> Value {
        let mut schema = tool.input_schema();
        schema_sanitize::sanitize(&mut schema);
        schema
    }

    const OFFICE_SCHEMA_SNAPSHOT_DIR: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/harness/kernel-v2-schema-snapshots"
    );

    #[test]
    #[ignore = "bootstrap kernel-v2 office-tool schema snapshot fixtures"]
    fn dump_office_tool_schemas_for_snapshot_bootstrap() {
        let tools: [(&str, &dyn ToolSpec); 3] = [
            ("read_office", &ReadOfficeTool),
            ("write_office", &WriteOfficeTool),
            ("load_office_payload", &LoadOfficePayloadTool),
        ];
        for (name, tool) in tools {
            let schema = model_visible_input_schema(tool);
            let pretty = serde_json::to_string_pretty(&schema).expect("serialize");
            println!("=== {name} ===\n{pretty}\n");
        }
    }

    #[test]
    fn office_tool_model_visible_schemas_match_snapshots() {
        let tools: [(&str, &dyn ToolSpec); 3] = [
            ("read_office", &ReadOfficeTool),
            ("write_office", &WriteOfficeTool),
            ("load_office_payload", &LoadOfficePayloadTool),
        ];
        for (name, tool) in tools {
            assert_eq!(tool.name(), name);
            let schema = model_visible_input_schema(tool);
            let path = format!("{OFFICE_SCHEMA_SNAPSHOT_DIR}/office-{name}.json");
            let expected: Value = serde_json::from_str(
                &std::fs::read_to_string(&path)
                    .unwrap_or_else(|e| panic!("missing snapshot {path}: {e}")),
            )
            .expect("parse snapshot JSON");
            assert_eq!(
                schema, expected,
                "model-visible schema drift for {name} — update fixture only after explicit KV-cache review"
            );
        }
    }
}
