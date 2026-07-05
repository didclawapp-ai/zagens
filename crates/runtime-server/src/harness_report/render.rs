//! Markdown + Office JSON payloads from [`ReportContext`].

use serde_json::{Value, json};

use super::context::{ReportContext, ReportSection};

pub fn render_markdown(ctx: &ReportContext) -> String {
    let mut out = String::new();
    out.push_str(&format!("# {}\n\n", ctx.title));
    if let Some(ref sub) = ctx.subtitle {
        out.push_str(&format!("_{sub}_\n\n"));
    }
    out.push_str(&format!("**Generated:** {}\n\n", ctx.generated_at));

    for section in &ctx.sections {
        match section {
            ReportSection::Summary { items } => {
                for item in items {
                    out.push_str(&format!("- {item}\n"));
                }
                out.push('\n');
            }
            ReportSection::Heading { level, text } => {
                let hashes = "#".repeat(*level as usize);
                out.push_str(&format!("{hashes} {text}\n\n"));
            }
            ReportSection::Paragraph { text } => {
                out.push_str(text);
                out.push_str("\n\n");
            }
            ReportSection::Table {
                title,
                headers,
                rows,
            } => {
                if let Some(title) = title {
                    out.push_str(&format!("**{title}**\n\n"));
                }
                out.push('|');
                for h in headers {
                    out.push_str(h);
                    out.push('|');
                }
                out.push('\n');
                out.push('|');
                for _ in headers {
                    out.push_str("---|");
                }
                out.push('\n');
                for row in rows {
                    out.push('|');
                    for cell in row {
                        out.push_str(cell);
                        out.push('|');
                    }
                    out.push('\n');
                }
                out.push('\n');
            }
        }
    }
    out
}

pub fn build_docx_payload(ctx: &ReportContext) -> Value {
    let mut blocks = Vec::new();
    if let Some(ref sub) = ctx.subtitle {
        blocks.push(json!({"type": "paragraph", "text": sub}));
    }
    blocks.push(json!({
        "type": "paragraph",
        "text": format!("Generated: {}", ctx.generated_at),
    }));

    for section in &ctx.sections {
        match section {
            ReportSection::Summary { items } => {
                blocks.push(json!({
                    "type": "list",
                    "style": "bullet",
                    "items": items,
                }));
            }
            ReportSection::Heading { level, text } => {
                blocks.push(json!({"type": "heading", "level": level, "text": text}));
            }
            ReportSection::Paragraph { text } => {
                blocks.push(json!({"type": "paragraph", "text": text}));
            }
            ReportSection::Table {
                title,
                headers,
                rows,
            } => {
                if let Some(title) = title {
                    blocks.push(json!({"type": "heading", "level": 3, "text": title}));
                }
                blocks.push(json!({
                    "type": "table",
                    "headers": headers,
                    "rows": rows,
                }));
            }
        }
    }

    json!({
        "title": ctx.title,
        "blocks": blocks,
    })
}

pub fn build_xlsx_evidence_payload(ctx: &ReportContext) -> Value {
    let mut summary_rows = vec![vec!["Field".into(), "Value".into()]];
    summary_rows.push(vec!["Title".into(), ctx.title.clone()]);
    summary_rows.push(vec!["Generated".into(), ctx.generated_at.clone()]);
    if let Some(ref sub) = ctx.subtitle {
        summary_rows.push(vec!["Subtitle".into(), sub.clone()]);
    }

    for section in &ctx.sections {
        if let ReportSection::Summary { items } = section {
            for item in items {
                summary_rows.push(vec!["Summary".into(), item.clone()]);
            }
        }
    }

    let mut sheets = vec![json!({
        "name": "Summary",
        "header": true,
        "rows": summary_rows,
    })];

    for section in &ctx.sections {
        if let ReportSection::Table {
            title,
            headers,
            rows,
        } = section
        {
            let name = title
                .clone()
                .unwrap_or_else(|| "Evidence".into())
                .chars()
                .take(31)
                .collect::<String>();
            sheets.push(json!({
                "name": name,
                "header": true,
                "columns": headers.iter().enumerate().map(|(i, h)| json!({"header": h, "width": 18 + i})).collect::<Vec<_>>(),
                "rows": rows,
            }));
        }
    }

    json!({
        "title": ctx.title,
        "sheets": sheets,
        "style": {
            "header_freeze": true,
            "banded_rows": true,
        }
    })
}

pub fn build_pptx_progress_payload(ctx: &ReportContext) -> Value {
    let mut slides = vec![json!({
        "layout": "title",
        "title": ctx.title,
        "subtitle": ctx.subtitle.clone().unwrap_or_else(|| ctx.generated_at.clone()),
    })];

    for section in &ctx.sections {
        match section {
            ReportSection::Summary { items } if !items.is_empty() => {
                slides.push(json!({
                    "layout": "bullet",
                    "title": "Summary",
                    "bullets": items,
                }));
            }
            ReportSection::Heading { text, .. } => {
                slides.push(json!({
                    "layout": "section",
                    "title": text,
                }));
            }
            ReportSection::Table {
                title,
                headers,
                rows,
            } => {
                let slide_title = title.clone().unwrap_or_else(|| "Evidence".into());
                let mut bullets: Vec<String> = Vec::new();
                for row in rows.iter().take(8) {
                    let line = headers
                        .iter()
                        .zip(row.iter())
                        .map(|(h, v)| format!("{h}: {v}"))
                        .collect::<Vec<_>>()
                        .join(" · ");
                    bullets.push(line);
                }
                slides.push(json!({
                    "layout": "bullet",
                    "title": slide_title,
                    "bullets": bullets,
                }));
            }
            _ => {}
        }
    }

    json!({
        "title": ctx.title,
        "subtitle": ctx.generated_at,
        "theme": "dark",
        "slides": slides,
    })
}
