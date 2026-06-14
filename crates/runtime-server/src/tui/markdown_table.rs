//! Render Markdown pipe-tables as fixed-width ASCII grids in the Transcript.

use super::display_format::display_width;

const MIN_COL_WIDTH: usize = 2;
/// Minimum column width before we stop shrinking (keeps wrapped content readable).
const MIN_WRAP_WIDTH: usize = 10;

/// Strip inline markdown delimiters (`*`, `` ` ``) so we get the visually-rendered text.
/// Used for display-width calculations only; does not validate balanced pairs.
pub fn strip_inline_markers(text: &str) -> String {
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut out = String::new();
    let mut i = 0;
    while i < len {
        if bytes[i] == b'*' || bytes[i] == b'`' {
            i += 1;
        } else {
            let ch = text[i..].chars().next().expect("valid utf-8");
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssistantBlock {
    Prose(String),
    Table(Vec<Vec<String>>),
    /// A fenced code block.  `lang` is the language tag (e.g. "mermaid", "rust", "").
    Code {
        lang: String,
        lines: Vec<String>,
    },
}

/// Split assistant text into prose paragraphs, Markdown tables, and code fences.
pub fn split_assistant_blocks(text: &str) -> Vec<AssistantBlock> {
    let lines: Vec<&str> = text.lines().collect();
    let mut blocks = Vec::new();
    let mut prose: Vec<&str> = Vec::new();
    let mut i = 0usize;

    while i < lines.len() {
        // ── Fenced code block  ```lang … ```  ────────────────────────────
        if let Some(lang) = fence_open_lang(lines[i]) {
            flush_prose(&mut prose, &mut blocks);
            let lang = lang.to_string();
            let mut code_lines: Vec<String> = Vec::new();
            i += 1;
            while i < lines.len() {
                if is_fence_close(lines[i]) {
                    i += 1;
                    break;
                }
                code_lines.push(lines[i].to_string());
                i += 1;
            }
            blocks.push(AssistantBlock::Code {
                lang,
                lines: code_lines,
            });
            continue;
        }
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

/// If the line opens a code fence (``` or ~~~), return the language tag.
fn fence_open_lang(line: &str) -> Option<&str> {
    let t = line.trim_start();
    if let Some(rest) = t.strip_prefix("```") {
        // Must not be a closing fence (only backticks, no lang)
        // but opening can also be bare "```" (empty lang).
        if !rest.contains('`') {
            return Some(rest.trim());
        }
    }
    if let Some(rest) = t.strip_prefix("~~~") {
        if !rest.contains('~') {
            return Some(rest.trim());
        }
    }
    None
}

fn is_fence_close(line: &str) -> bool {
    let t = line.trim_start();
    t == "```" || t == "~~~" || t.starts_with("``` ") || t.starts_with("~~~ ")
}

/// Format a table to terminal lines using Unicode box-drawing characters.
///
/// Example output:
/// ```text
/// ┌──────┬───────┐
/// │ A    │ B     │
/// ├──────┼───────┤
/// │ 1    │ hello │
/// └──────┴───────┘
/// ```
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
                // Measure width of visually-rendered text (without markdown markers).
                let visual_w = display_width(&strip_inline_markers(cell)).max(MIN_COL_WIDTH);
                widths[idx] = widths[idx].max(visual_w);
            }
        }
    }
    fit_column_widths(&mut widths, max_cols);

    let top = border_line(&widths, "┌", "─", "┬", "┐");
    let mid = border_line(&widths, "├", "─", "┼", "┤");
    let bottom = border_line(&widths, "└", "─", "┴", "┘");

    let mut out = vec![top];
    for (idx, row) in rows.iter().enumerate() {
        if idx == 0 {
            // Header row: single display line, use box-drawing │.
            out.push(format_row(row, &widths, col_count, '│'));
            out.push(mid.clone());
        } else {
            // Data rows: word-wrap cell content so long text is not truncated.
            // Use ASCII | (unambiguous width) so separators can be dim-styled.
            out.extend(format_row_wrapped(row, &widths, col_count));
        }
    }
    out.push(bottom);
    out
}

pub fn is_table_render_line(line: &str) -> bool {
    let t = line.trim_start();
    // Match both legacy ASCII (+) and new box-drawing (┌├└) borders, and data rows (│ or |).
    t.starts_with('+')
        || t.starts_with('┌')
        || t.starts_with('├')
        || t.starts_with('└')
        || t.starts_with('│')
        || is_pipe_row(line)
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
    // overhead = col_count+1 border pipes + (col_count-1)*3 " | " gaps + 2 trailing " |"
    // which equals col_count+1 + (col_count-1)*3 = 4*col_count - 2 for col_count>=1
    // Simplified: same as before — border+gaps.
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
        // Don't shrink below MIN_WRAP_WIDTH — content will wrap rather than vanish.
        if widths[idx] <= MIN_WRAP_WIDTH {
            break;
        }
        widths[idx] -= 1;
        total -= 1;
    }
}

/// Word-wrap `text` (stripped of markdown markers) to at most `width` display columns.
/// Returns one or more lines, each padded with trailing spaces to exactly `width` columns.
fn wrap_cell_to_lines(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec!["".to_string()];
    }
    let stripped = strip_inline_markers(text);
    let stripped = stripped.trim();
    if stripped.is_empty() {
        return vec![" ".repeat(width)];
    }

    let mut lines: Vec<String> = Vec::new();
    let mut line_buf = String::new();
    let mut line_w = 0usize;

    for word in stripped.split_whitespace() {
        let word_w = display_width(word);
        if word_w == 0 {
            continue;
        }
        if line_w == 0 {
            // First word on this line.
            if word_w > width {
                // Single word wider than column — hard-truncate it.
                let mut seg = String::new();
                let mut seg_w = 0;
                for ch in word.chars() {
                    let cw = unicode_width(ch);
                    if seg_w + cw > width {
                        break;
                    }
                    seg.push(ch);
                    seg_w += cw;
                }
                lines.push(format!("{seg}{}", " ".repeat(width.saturating_sub(seg_w))));
            } else {
                line_buf.push_str(word);
                line_w = word_w;
            }
        } else if line_w + 1 + word_w <= width {
            // Word fits with a separating space.
            line_buf.push(' ');
            line_buf.push_str(word);
            line_w += 1 + word_w;
        } else {
            // Flush current line, start new one.
            lines.push(format!(
                "{line_buf}{}",
                " ".repeat(width.saturating_sub(line_w))
            ));
            line_buf = word.to_string();
            line_w = word_w;
        }
    }
    if !line_buf.is_empty() {
        lines.push(format!(
            "{line_buf}{}",
            " ".repeat(width.saturating_sub(line_w))
        ));
    }
    if lines.is_empty() {
        lines.push(" ".repeat(width));
    }
    lines
}

#[inline]
fn unicode_width(ch: char) -> usize {
    unicode_width::UnicodeWidthChar::width(ch).unwrap_or(1)
}

/// Render a data row as one or more display lines, wrapping cell content.
/// Returns at least one line.  Each line uses ASCII `|` as separator.
fn format_row_wrapped(row: &[String], widths: &[usize], col_count: usize) -> Vec<String> {
    // Wrap each cell independently.
    let cell_lines: Vec<Vec<String>> = (0..col_count)
        .map(|idx| {
            let cell = row.get(idx).map(String::as_str).unwrap_or("");
            let width = widths.get(idx).copied().unwrap_or(MIN_COL_WIDTH);
            wrap_cell_to_lines(cell, width)
        })
        .collect();

    let max_lines = cell_lines.iter().map(|cl| cl.len()).max().unwrap_or(1);
    let blank_for: Vec<String> = widths.iter().map(|&w| " ".repeat(w)).collect();

    (0..max_lines)
        .map(|line_idx| {
            let cells: Vec<&str> = (0..col_count)
                .map(|col| {
                    cell_lines[col]
                        .get(line_idx)
                        .map(String::as_str)
                        .unwrap_or_else(|| blank_for[col].as_str())
                })
                .collect();
            format!("| {} |", cells.join(" | "))
        })
        .collect()
}

/// Build a horizontal border line, e.g. `┌──────┬───────┐`.
fn border_line(widths: &[usize], left: &str, fill: &str, join: &str, right: &str) -> String {
    let parts: Vec<String> = widths.iter().map(|w| fill.repeat(*w + 2)).collect();
    format!("{left}{}{right}", parts.join(join))
}

fn format_row(row: &[String], widths: &[usize], col_count: usize, sep: char) -> String {
    let mut cells = Vec::with_capacity(col_count);
    for idx in 0..col_count {
        let cell = row.get(idx).map(String::as_str).unwrap_or("");
        let width = widths.get(idx).copied().unwrap_or(MIN_COL_WIDTH);
        cells.push(pad_cell(cell, width));
    }
    let s = sep.to_string();
    format!("{s} {} {s}", cells.join(&format!(" {s} ")))
}

fn pad_cell(text: &str, width: usize) -> String {
    // Use the visual width AFTER stripping markdown markers (*bold*, `code`)
    // so that padding is correct once the markers are rendered invisibly.
    let stripped = strip_inline_markers(text);
    let w = display_width(&stripped);
    if w > width {
        // Truncate the stripped text so the cell fits neatly.
        return truncate_chars_by_width(&stripped, width);
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
        assert!(lines.first().is_some_and(|l| l.starts_with('┌')));
        assert!(
            lines
                .iter()
                .any(|l| l.contains("类别") && l.contains("模块"))
        );
        assert!(lines.last().is_some_and(|l| l.starts_with('└')));
        // Header separator uses ├
        assert!(lines.iter().any(|l| l.starts_with('├')));
        // Data rows use │
        assert!(lines.iter().any(|l| l.starts_with('│')));
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
