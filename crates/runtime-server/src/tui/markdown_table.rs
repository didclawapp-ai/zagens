//! Render Markdown pipe-tables as fixed-width ASCII grids in the Transcript.

use super::display_format::display_width;

const MIN_COL_WIDTH: usize = 2;
/// Blank rows inserted between markdown table data rows (not border rules).
const TABLE_DATA_ROW_GAP_LINES: usize = 0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssistantBlock {
    Prose(String),
    Table(Vec<Vec<String>>),
}

/// Split assistant text into prose paragraphs and Markdown tables.
pub fn split_assistant_blocks(text: &str) -> Vec<AssistantBlock> {
    let lines: Vec<&str> = text.lines().collect();
    let mut blocks = Vec::new();
    let mut prose: Vec<&str> = Vec::new();
    let mut i = 0usize;

    while i < lines.len() {
        if is_table_start(&lines, i) {
            flush_prose(&mut prose, &mut blocks);
            let (rows, next) = parse_table_block(&lines, i);
            if rows.len() >= 2 {
                blocks.push(AssistantBlock::Table(rows));
            } else {
                prose.extend_from_slice(&lines[i..next]);
            }
            i = next;
            continue;
        }
        prose.push(lines[i]);
        i += 1;
    }
    flush_prose(&mut prose, &mut blocks);
    blocks
}

/// Format a table to terminal lines with `+`/`-` borders and `|` column separators.
pub fn format_table(rows: &[Vec<String>], max_cols: usize) -> Vec<String> {
    if rows.is_empty() {
        return Vec::new();
    }
    let max_cols = max_cols.max(16);
    let col_count = rows.iter().map(|r| r.len()).max().unwrap_or(0).max(1);
    let mut widths = vec![MIN_COL_WIDTH; col_count];
    for row in rows {
        for (idx, cell) in row.iter().enumerate() {
            if idx < col_count {
                widths[idx] = widths[idx].max(display_width(cell).max(MIN_COL_WIDTH));
            }
        }
    }
    fit_column_widths(&mut widths, max_cols);
    let rule = horizontal_rule(&widths);
    let mut out = vec![rule.clone()];
    for (idx, row) in rows.iter().enumerate() {
        out.push(format_row(row, &widths, col_count));
        if idx == 0 {
            out.push(rule.clone());
        } else if TABLE_DATA_ROW_GAP_LINES > 0 && idx + 1 < rows.len() {
            for _ in 0..TABLE_DATA_ROW_GAP_LINES {
                out.push(String::new());
            }
        }
    }
    out.push(rule);
    out
}

pub fn is_table_render_line(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with('+') || is_pipe_row(line)
}

fn flush_prose(prose: &mut Vec<&str>, blocks: &mut Vec<AssistantBlock>) {
    if prose.is_empty() {
        return;
    }
    let text = prose.join("\n");
    let trimmed = text.trim().to_string();
    if !trimmed.is_empty() {
        blocks.push(AssistantBlock::Prose(trimmed));
    }
    prose.clear();
}

fn is_table_start(lines: &[&str], i: usize) -> bool {
    if !is_pipe_row(lines[i]) {
        return false;
    }
    if i + 1 < lines.len() && is_separator_row(lines[i + 1]) {
        return true;
    }
    i + 1 < lines.len() && is_pipe_row(lines[i + 1])
}

fn parse_table_block(lines: &[&str], start: usize) -> (Vec<Vec<String>>, usize) {
    let mut rows = vec![parse_pipe_row(lines[start])];
    let mut i = start + 1;
    if i < lines.len() && is_separator_row(lines[i]) {
        i += 1;
    }
    while i < lines.len() && is_pipe_row(lines[i]) {
        rows.push(parse_pipe_row(lines[i]));
        i += 1;
    }
    (rows, i)
}

fn is_pipe_row(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with('|') && trimmed.ends_with('|') && trimmed.matches('|').count() >= 2
}

fn is_separator_row(line: &str) -> bool {
    let trimmed = line.trim();
    if !trimmed.starts_with('|') || !trimmed.ends_with('|') {
        return false;
    }
    trimmed
        .chars()
        .all(|ch| matches!(ch, '|' | '-' | ':' | ' '))
        && trimmed.contains('-')
}

fn parse_pipe_row(line: &str) -> Vec<String> {
    let trimmed = line.trim();
    let inner = trimmed
        .strip_prefix('|')
        .and_then(|s| s.strip_suffix('|'))
        .unwrap_or(trimmed);
    inner
        .split('|')
        .map(|cell| cell.trim().to_string())
        .collect()
}

fn fit_column_widths(widths: &mut [usize], max_cols: usize) {
    let border = widths.len().saturating_add(1);
    let gaps = widths.len().saturating_sub(1) * 3;
    let mut total = widths.iter().sum::<usize>() + border + gaps;
    while total > max_cols {
        let max_idx = widths
            .iter()
            .enumerate()
            .max_by_key(|(_, w)| **w)
            .map(|(i, _)| i);
        let Some(idx) = max_idx else {
            break;
        };
        if widths[idx] <= MIN_COL_WIDTH {
            break;
        }
        widths[idx] -= 1;
        total -= 1;
    }
}

fn horizontal_rule(widths: &[usize]) -> String {
    let parts: Vec<String> = widths.iter().map(|w| "-".repeat(*w + 2)).collect();
    format!("+{}+", parts.join("+"))
}

fn format_row(row: &[String], widths: &[usize], col_count: usize) -> String {
    let mut cells = Vec::with_capacity(col_count);
    for idx in 0..col_count {
        let cell = row.get(idx).map(String::as_str).unwrap_or("");
        let width = widths.get(idx).copied().unwrap_or(MIN_COL_WIDTH);
        cells.push(pad_cell(cell, width));
    }
    format!("| {} |", cells.join(" | "))
}

fn pad_cell(text: &str, width: usize) -> String {
    let w = display_width(text);
    if w > width {
        return truncate_chars_by_width(text, width);
    }
    let mut out = text.to_string();
    out.push_str(&" ".repeat(width - w));
    out
}

fn truncate_chars_by_width(text: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    if display_width(text) <= max_width {
        return text.to_string();
    }
    if max_width == 1 {
        return "…".to_string();
    }
    let mut out = String::new();
    let mut w = 0usize;
    for ch in text.chars() {
        let cw = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if w + cw > max_width.saturating_sub(1) {
            break;
        }
        out.push(ch);
        w += cw;
    }
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_markdown_table_block() {
        let text = "intro\n\n| A | B |\n|---|---|\n| 1 | 2 |";
        let blocks = split_assistant_blocks(text);
        assert_eq!(blocks.len(), 2);
        assert!(matches!(&blocks[0], AssistantBlock::Prose(s) if s == "intro"));
        assert!(matches!(&blocks[1], AssistantBlock::Table(rows) if rows.len() == 2));
    }

    #[test]
    fn formats_table_with_borders() {
        let rows = vec![
            vec!["类别".to_string(), "模块".to_string()],
            vec!["运行时核心".to_string(), "runtime-server".to_string()],
        ];
        let lines = format_table(&rows, 80);
        assert!(lines.first().is_some_and(|l| l.starts_with('+')));
        assert!(
            lines
                .iter()
                .any(|l| l.contains("类别") && l.contains("模块"))
        );
        assert!(lines.last().is_some_and(|l| l.starts_with('+')));
    }

    #[test]
    fn formats_table_without_extra_row_gaps() {
        let rows = vec![
            vec!["项目".to_string(), "数值".to_string()],
            vec!["天气".to_string(), "晴".to_string()],
            vec!["温度".to_string(), "30°C".to_string()],
        ];
        let lines = format_table(&rows, 80);
        let weather_idx = lines
            .iter()
            .position(|l| l.contains("天气"))
            .expect("weather row");
        let temp_idx = lines
            .iter()
            .position(|l| l.contains("温度"))
            .expect("temp row");
        assert!(
            !lines[weather_idx + 1..temp_idx]
                .iter()
                .any(|l| l.trim().is_empty()),
            "expected no blank row between table data rows"
        );
    }

    #[test]
    fn parses_table_without_separator_row() {
        let text = "| A | B |\n| 1 | 2 |";
        let blocks = split_assistant_blocks(text);
        assert_eq!(blocks.len(), 1);
        assert!(matches!(&blocks[0], AssistantBlock::Table(rows) if rows.len() == 2));
    }
}
