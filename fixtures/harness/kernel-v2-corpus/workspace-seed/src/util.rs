//! Shared helpers: source spans and diagnostics rendering.

/// Byte-offset range into the original source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    #[must_use]
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Render a caret diagnostic pointing at `span` within `source`.
///
/// TODO: handle multi-line spans; today the caret line assumes the span
/// fits on the line containing `span.start`.
#[must_use]
pub fn render_span(source: &str, span: Span) -> String {
    let line_start = source[..span.start.min(source.len())]
        .rfind('\n')
        .map_or(0, |idx| idx + 1);
    let line_end = source[line_start..]
        .find('\n')
        .map_or(source.len(), |idx| line_start + idx);
    let line = &source[line_start..line_end];
    let caret_offset = span.start.saturating_sub(line_start);
    let caret_len = span.len().clamp(1, line.len().saturating_sub(caret_offset).max(1));
    format!(
        "{line}\n{}{}",
        " ".repeat(caret_offset),
        "^".repeat(caret_len)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caret_points_at_span() {
        let rendered = render_span("let x = 1;", Span::new(4, 5));
        assert!(rendered.ends_with("    ^"));
    }
}
