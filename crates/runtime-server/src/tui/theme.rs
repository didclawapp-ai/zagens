//! High-contrast ANSI palette for the center column (readable on dark terminals).

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use super::transcript::TranscriptLineKind;

/// Role prefix width for assistant continuation indent.
pub const AI_TAG: &str = "AI> ";
pub const USER_TAG: &str = "you> ";
pub const THINK_TAG: &str = "THK> ";
pub const TOOL_TAG: &str = "tool ";
pub const COMPOSER_PROMPT: &str = "> ";

pub fn border_focus() -> Style {
    Style::default()
        .fg(Color::LightCyan)
        .add_modifier(Modifier::BOLD)
}

pub fn border_idle() -> Style {
    Style::default().fg(Color::Gray)
}

pub fn title_bar() -> Style {
    Style::default()
        .fg(Color::White)
        .add_modifier(Modifier::BOLD)
}

pub fn hint() -> Style {
    Style::default().fg(Color::Gray)
}

pub fn composer_input() -> Style {
    Style::default()
        .fg(Color::LightGreen)
        .add_modifier(Modifier::BOLD)
}

pub fn composer_idle() -> Style {
    Style::default().fg(Color::Gray)
}

pub fn composer_cursor() -> Style {
    Style::default()
        .fg(Color::LightYellow)
        .add_modifier(Modifier::BOLD)
}

pub fn footer_separator() -> Style {
    Style::default().fg(Color::Gray)
}

pub fn footer_chip(color: Color) -> Style {
    Style::default().fg(color).add_modifier(Modifier::BOLD)
}

pub fn footer_model() -> Color {
    Color::LightCyan
}

pub fn footer_mode() -> Color {
    Color::LightYellow
}

pub fn footer_task() -> Color {
    Color::LightGreen
}

pub fn footer_workspace() -> Style {
    Style::default().fg(Color::Gray)
}

pub fn footer_context() -> Style {
    Style::default().fg(Color::LightMagenta)
}

pub fn footer_muted() -> Style {
    Style::default().fg(Color::Gray)
}

pub fn approval_color(label: &str) -> Color {
    match label {
        "Auto" => Color::LightYellow,
        "Never" => Color::LightRed,
        _ => Color::White,
    }
}

fn role_style(kind: TranscriptLineKind, live: bool) -> Style {
    match kind {
        TranscriptLineKind::User => Style::default()
            .fg(Color::LightCyan)
            .add_modifier(Modifier::BOLD),
        TranscriptLineKind::Assistant => Style::default()
            .fg(Color::LightBlue)
            .add_modifier(Modifier::BOLD),
        TranscriptLineKind::Thinking => Style::default()
            .fg(Color::LightYellow)
            .add_modifier(Modifier::BOLD),
        TranscriptLineKind::ToolChain => {
            Style::default()
                .fg(Color::LightYellow)
                .add_modifier(if live {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                })
        }
        TranscriptLineKind::System => Style::default()
            .fg(Color::LightRed)
            .add_modifier(Modifier::BOLD),
        TranscriptLineKind::Meta => Style::default().fg(Color::Gray),
        TranscriptLineKind::Spacer => Style::default(),
    }
}

fn body_style(kind: TranscriptLineKind, live: bool) -> Style {
    match kind {
        TranscriptLineKind::User => Style::default().fg(Color::White),
        TranscriptLineKind::Assistant => Style::default().fg(Color::White),
        TranscriptLineKind::Thinking => {
            if live {
                Style::default()
                    .fg(Color::LightYellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Yellow)
            }
        }
        TranscriptLineKind::ToolChain => {
            if live {
                Style::default()
                    .fg(Color::LightYellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Yellow)
            }
        }
        TranscriptLineKind::System => Style::default()
            .fg(Color::LightRed)
            .add_modifier(Modifier::BOLD),
        TranscriptLineKind::Meta => Style::default().fg(Color::Gray),
        TranscriptLineKind::Spacer => Style::default(),
    }
}

/// Build a transcript line with a bright role tag and contrasting body text.
pub fn transcript_line(kind: TranscriptLineKind, text: &str, live: bool) -> Line<'static> {
    if kind == TranscriptLineKind::Spacer {
        return Line::from(Span::raw(text.to_string()));
    }

    if kind == TranscriptLineKind::Assistant && super::markdown_table::is_table_render_line(text) {
        return Line::from(Span::styled(
            text.to_string(),
            Style::default().fg(Color::LightCyan),
        ));
    }

    if let Some(rest) = text.strip_prefix(USER_TAG) {
        return tagged_line(USER_TAG, rest, kind, live);
    }
    if let Some(rest) = text.strip_prefix(AI_TAG) {
        return tagged_line(AI_TAG, rest, kind, live);
    }
    if let Some(rest) = text.strip_prefix(THINK_TAG) {
        return tagged_line(THINK_TAG, rest, kind, live);
    }
    if let Some(rest) = text.strip_prefix(TOOL_TAG) {
        return tagged_line(TOOL_TAG, rest, kind, live);
    }
    if let Some(rest) = text.strip_prefix("-- ") {
        return Line::from(vec![
            Span::styled("-- ".to_string(), footer_separator()),
            Span::styled(rest.to_string(), body_style(kind, live)),
        ]);
    }
    if text.starts_with("     ") {
        return Line::from(Span::styled(
            text.to_string(),
            Style::default().fg(Color::LightYellow),
        ));
    }
    if text.starts_with("    ") {
        let style = match kind {
            TranscriptLineKind::Assistant => Style::default().fg(Color::White),
            TranscriptLineKind::ToolChain => Style::default().fg(Color::Yellow),
            _ => body_style(kind, live),
        };
        return Line::from(Span::styled(text.to_string(), style));
    }

    Line::from(Span::styled(text.to_string(), body_style(kind, live)))
}

fn tagged_line(tag: &str, body: &str, kind: TranscriptLineKind, live: bool) -> Line<'static> {
    Line::from(vec![
        Span::styled(tag.to_string(), role_style(kind, live)),
        Span::styled(body.to_string(), body_style(kind, live)),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transcript_line_splits_role_tag() {
        let line = transcript_line(TranscriptLineKind::User, "you> hello", false);
        assert_eq!(line.spans.len(), 2);
        assert_eq!(line.spans[0].content, "you> ");
        assert_eq!(line.spans[1].content, "hello");
    }
}
