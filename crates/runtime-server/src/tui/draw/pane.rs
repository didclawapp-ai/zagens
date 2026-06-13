//! Buffer-backed pane rendering (border block + clipped text).

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::prelude::*;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders};

use super::super::display_format::{
    fill_styled_lines, pad_styled_line, paint_styled_lines_in_area, truncate_display_width,
};
use super::super::layout::CENTER_CONTENT_PAD;
use super::super::theme;
use super::chrome::BorderPlan;

pub fn inset_content_area(area: Rect) -> Rect {
    let pad = CENTER_CONTENT_PAD;
    if area.width <= pad.saturating_mul(2) {
        return area;
    }
    Rect {
        x: area.x.saturating_add(pad),
        y: area.y,
        width: area.width.saturating_sub(pad * 2),
        height: area.height,
    }
}

/// Text line budget inside a pane (content rows/cols, excluding chrome title when borderless).
pub fn line_budget(area: Rect, borders: BorderPlan) -> (usize, usize) {
    if area.width == 0 || area.height == 0 {
        return (0, 0);
    }
    if borders.into_borders() == Borders::NONE {
        let chrome = theme::pane_chrome_rows();
        let h = area.height.saturating_sub(chrome).max(1) as usize;
        return (area.width.max(1) as usize, h);
    }
    let inner = Block::default()
        .borders(borders.into())
        .title(" t ")
        .inner(area);
    (inner.width.max(1) as usize, inner.height.max(1) as usize)
}

/// Line budget for center panes that inset transcript/composer text horizontally.
pub fn inset_line_budget(area: Rect, borders: BorderPlan) -> (usize, usize) {
    let (w, h) = line_budget(area, borders);
    let inset = inset_content_area(Rect {
        x: 0,
        y: 0,
        width: w as u16,
        height: h as u16,
    });
    (inset.width.max(1) as usize, h)
}

fn pad_lines_leading(
    lines: Vec<Line<'static>>,
    cols: usize,
    pad_style: Style,
) -> Vec<Line<'static>> {
    if cols == 0 {
        return lines;
    }
    let pad = " ".repeat(cols);
    lines
        .into_iter()
        .map(|line| {
            let mut spans = vec![Span::styled(pad.clone(), pad_style)];
            spans.extend(line.spans);
            Line::from(spans)
        })
        .collect()
}

/// Draw a titled pane: ratatui `Block` for chrome (bordered themes), buffer fill for inner text.
pub fn paint_pane(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    borders: BorderPlan,
    border_style: Style,
    fill_style: Style,
    lines: Vec<Line<'static>>,
    inset_text: bool,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let bordered = borders.into_borders() != Borders::NONE;
    if bordered {
        paint_pane_bordered(
            frame,
            area,
            title,
            borders,
            border_style,
            fill_style,
            lines,
            inset_text,
        );
    } else {
        paint_pane_borderless(
            frame,
            area,
            title,
            border_style,
            fill_style,
            lines,
            inset_text,
        );
    }
}

/// Classic bordered chrome + cell paint (sidebar ghost fix path).
fn paint_pane_bordered(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    borders: BorderPlan,
    border_style: Style,
    fill_style: Style,
    lines: Vec<Line<'static>>,
    inset_text: bool,
) {
    clear_rect(frame.buffer_mut(), area, fill_style);
    let block = Block::default()
        .borders(borders.into())
        .border_style(border_style)
        .style(fill_style)
        .title(title);
    let inner = block.inner(area);
    let text_area = if inset_text {
        inset_content_area(inner)
    } else {
        inner
    };
    frame.render_widget(block, area);
    if text_area.width == 0 || text_area.height == 0 {
        return;
    }
    let filled = fill_styled_lines(
        lines,
        text_area.height as usize,
        text_area.width as usize,
        fill_style,
    );
    paint_styled_lines_in_area(frame.buffer_mut(), text_area, &filled, fill_style);
}

/// Borderless: full-area `clear_rect` + per-cell paint (Windows-safe; avoids `Paragraph` ghosts).
fn paint_pane_borderless(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    border_style: Style,
    fill_style: Style,
    lines: Vec<Line<'static>>,
    inset_text: bool,
) {
    let buf = frame.buffer_mut();
    clear_rect(buf, area, fill_style);

    let chrome = theme::pane_chrome_rows().min(area.height);
    let title_area = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: chrome,
    };
    let content_area = Rect {
        x: area.x,
        y: area.y.saturating_add(chrome),
        width: area.width,
        height: area.height.saturating_sub(chrome),
    };

    if title_area.width > 0 && title_area.height > 0 {
        let title_text = truncate_display_width(title.trim(), title_area.width as usize);
        let title_line = pad_styled_line(
            Line::from(Span::styled(title_text, border_style)),
            title_area.width as usize,
            fill_style,
        );
        paint_styled_lines_in_area(
            buf,
            title_area,
            std::slice::from_ref(&title_line),
            fill_style,
        );
    }

    if content_area.width == 0 || content_area.height == 0 {
        return;
    }
    let mut content = lines;
    if inset_text {
        content = pad_lines_leading(content, CENTER_CONTENT_PAD as usize, fill_style);
    }
    let filled = fill_styled_lines(
        content,
        content_area.height as usize,
        content_area.width as usize,
        fill_style,
    );
    paint_styled_lines_in_area(buf, content_area, &filled, fill_style);
}

/// Erase a rectangle cell-by-cell (clears stale divider glyphs on Windows).
pub fn clear_rect(buf: &mut Buffer, area: Rect, style: Style) {
    let area = area.intersection(buf.area);
    if area.width == 0 || area.height == 0 {
        return;
    }
    for row in 0..area.height {
        for col in 0..area.width {
            buf[(area.x + col, area.y + row)]
                .set_symbol(" ")
                .set_style(style);
        }
    }
}

#[cfg(test)]
mod tests {
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::style::{Color, Style};

    use super::*;
    use crate::tui::theme::{TuiPanel, TuiTheme, install, panel};

    #[test]
    fn clear_rect_replaces_stale_divider_glyph() {
        install(TuiTheme::default_theme());
        let mut buf = Buffer::empty(Rect::new(0, 0, 10, 3));
        for row in 0..3 {
            buf[(0, row)]
                .set_symbol("│")
                .set_style(Style::default().bg(Color::Black));
        }
        let fill = panel(TuiPanel::Inspector).surface(false);
        clear_rect(&mut buf, Rect::new(0, 0, 4, 3), fill);
        assert_eq!(buf[(0, 0)].symbol(), " ");
        assert_eq!(buf[(0, 1)].symbol(), " ");
        assert_eq!(buf[(0, 0)].style().bg, fill.bg);
    }
}
