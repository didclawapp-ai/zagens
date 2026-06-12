//! Dracula-aligned TUI palette (24-bit RGB where supported).
//!
//! **Authoritative spec:** `doc_Private/docs/TUI方案.md` §6.10 — do not change token
//! assignments without updating that section and the maintainer color sheet.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use super::transcript::TranscriptLineKind;

/// Role prefix width for assistant continuation indent.
pub const AI_TAG: &str = "AI> ";
pub const USER_TAG: &str = "you> ";
pub const THINK_TAG: &str = "THK> ";
pub const TOOL_TAG: &str = "tool ";
pub const COMPOSER_PROMPT: &str = "> ";

/// Canonical hex tokens — mirror §6.10 table (also used in tests/docs).
pub mod palette {
    use ratatui::style::Color;

    // §6.10.1 核心语义
    pub const USER_PROMPT: &str = "#8be9fd";
    pub const USER_TEXT: &str = "#f8f8f2";
    pub const AGENT_REPLY: &str = "#50fa7b";
    pub const THINKING: &str = "#f1fa8c";
    pub const WARNING: &str = "#ffb86c";
    pub const ERROR: &str = "#ff5555";
    pub const TOOL_CALL: &str = "#bd93f9";
    pub const DIM: &str = "#6272a4";

    // §6.10.2 侧边栏
    pub const SIDEBAR_BG: &str = "#1e1f29";
    pub const SIDEBAR_ACTIVE: &str = "#343746";
    pub const BADGE: &str = "#bd93f9";
    pub const ITEM_TEXT: &str = "#f8f8f2";

    // §6.10.3 Checklist
    pub const CHECKLIST_DONE: &str = "#50fa7b";
    pub const CHECKLIST_IN_PROGRESS: &str = "#f1fa8c";
    pub const CHECKLIST_PENDING: &str = "#6272a4";
    pub const PROGRESS_FILL: &str = "#50fa7b";

    // §6.10.4 背景 / 表面
    pub const BG: &str = "#282a36";
    pub const FOREGROUND: &str = "#f8f8f2";
    pub const CODE_BG: &str = "#44475a";
    pub const TAG_BG: &str = "#44475a";

    const fn rgb(r: u8, g: u8, b: u8) -> Color {
        Color::Rgb(r, g, b)
    }

    pub fn user_prompt() -> Color {
        rgb(0x8b, 0xe9, 0xfd)
    }
    pub fn user_text() -> Color {
        rgb(0xf8, 0xf8, 0xf2)
    }
    pub fn agent_reply() -> Color {
        rgb(0x50, 0xfa, 0x7b)
    }
    pub fn thinking() -> Color {
        rgb(0xf1, 0xfa, 0x8c)
    }
    pub fn warning() -> Color {
        rgb(0xff, 0xb8, 0x6c)
    }
    pub fn error() -> Color {
        rgb(0xff, 0x55, 0x55)
    }
    pub fn tool_call() -> Color {
        rgb(0xbd, 0x93, 0xf9)
    }
    pub fn dim() -> Color {
        rgb(0x62, 0x72, 0xa4)
    }
    pub fn sidebar_bg() -> Color {
        rgb(0x1e, 0x1f, 0x29)
    }
    pub fn sidebar_active() -> Color {
        rgb(0x34, 0x37, 0x46)
    }
    pub fn badge() -> Color {
        rgb(0xbd, 0x93, 0xf9)
    }
    pub fn item_text() -> Color {
        rgb(0xf8, 0xf8, 0xf2)
    }
    pub fn bg() -> Color {
        rgb(0x28, 0x2a, 0x36)
    }
    pub fn foreground() -> Color {
        rgb(0xf8, 0xf8, 0xf2)
    }
    pub fn code_bg() -> Color {
        rgb(0x44, 0x47, 0x5a)
    }
    pub fn tag_bg() -> Color {
        rgb(0x44, 0x47, 0x5a)
    }
}

use palette as p;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityPhase {
    Thinking,
    Tools,
    Streaming,
    Other,
}

/// Full-frame / chat column surface (`#282a36`).
pub fn shell_main() -> Style {
    Style::default().fg(p::foreground()).bg(p::bg())
}

/// Left / right rail surface (`#1e1f29`).
pub fn shell_sidebar() -> Style {
    Style::default().fg(p::item_text()).bg(p::sidebar_bg())
}

pub fn sidebar_heading() -> Style {
    Style::default()
        .fg(p::foreground())
        .bg(p::sidebar_bg())
        .add_modifier(Modifier::BOLD)
}

pub fn sidebar_item(selected: bool) -> Style {
    if selected {
        Style::default().fg(p::item_text()).bg(p::sidebar_active())
    } else {
        Style::default().fg(p::item_text()).bg(p::sidebar_bg())
    }
}

pub fn sidebar_item_muted() -> Style {
    Style::default().fg(p::dim()).bg(p::sidebar_bg())
}

pub fn sidebar_tab(active: bool) -> Style {
    if active {
        Style::default()
            .fg(p::user_prompt())
            .bg(p::sidebar_bg())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(p::dim()).bg(p::sidebar_bg())
    }
}

pub fn sidebar_hint() -> Style {
    Style::default().fg(p::dim()).bg(p::sidebar_bg())
}

pub fn overlay_panel() -> Style {
    shell_main()
}

pub fn border_focus() -> Style {
    Style::default()
        .fg(p::user_prompt())
        .bg(p::bg())
        .add_modifier(Modifier::BOLD)
}

pub fn border_idle() -> Style {
    Style::default().fg(p::dim()).bg(p::bg())
}

pub fn border_focus_sidebar() -> Style {
    Style::default()
        .fg(p::user_prompt())
        .bg(p::sidebar_bg())
        .add_modifier(Modifier::BOLD)
}

pub fn border_idle_sidebar() -> Style {
    Style::default().fg(p::dim()).bg(p::sidebar_bg())
}

pub fn title_bar() -> Style {
    Style::default()
        .fg(p::foreground())
        .bg(p::bg())
        .add_modifier(Modifier::BOLD)
}

pub fn hint() -> Style {
    Style::default().fg(p::dim()).bg(p::bg())
}

pub fn composer_prompt() -> Style {
    Style::default()
        .fg(p::user_prompt())
        .bg(p::bg())
        .add_modifier(Modifier::BOLD)
}

pub fn composer_input() -> Style {
    Style::default()
        .fg(p::user_text())
        .bg(p::bg())
        .add_modifier(Modifier::BOLD)
}

pub fn composer_idle() -> Style {
    Style::default().fg(p::dim()).bg(p::bg())
}

pub fn composer_cursor() -> Style {
    Style::default()
        .fg(p::user_prompt())
        .bg(p::bg())
        .add_modifier(Modifier::BOLD)
}

pub fn composer_line(prompt_and_body: &str, focused: bool) -> Line<'static> {
    let body_style = if focused {
        composer_input()
    } else {
        composer_idle()
    };
    if let Some(body) = prompt_and_body.strip_prefix(COMPOSER_PROMPT) {
        return Line::from(vec![
            Span::styled(COMPOSER_PROMPT.to_string(), composer_prompt()),
            Span::styled(body.to_string(), body_style),
        ]);
    }
    Line::from(Span::styled(prompt_and_body.to_string(), body_style))
}

pub fn footer_separator() -> Style {
    Style::default().fg(p::dim()).bg(p::bg())
}

pub fn footer_chip(color: Color) -> Style {
    Style::default()
        .fg(color)
        .bg(p::bg())
        .add_modifier(Modifier::BOLD)
}

pub fn footer_model() -> Color {
    p::user_prompt()
}

pub fn footer_mode() -> Color {
    p::warning()
}

pub fn footer_task() -> Color {
    p::agent_reply()
}

pub fn footer_workspace() -> Style {
    Style::default().fg(p::dim()).bg(p::bg())
}

pub fn footer_context() -> Style {
    Style::default().fg(p::tool_call()).bg(p::bg())
}

pub fn footer_muted() -> Style {
    Style::default().fg(p::dim()).bg(p::bg())
}

pub fn palette_selection() -> Style {
    Style::default()
        .fg(p::user_prompt())
        .bg(p::bg())
        .add_modifier(Modifier::BOLD)
}

pub fn activity_phase_color(phase: ActivityPhase) -> Color {
    match phase {
        ActivityPhase::Thinking => p::thinking(),
        ActivityPhase::Tools => p::tool_call(),
        ActivityPhase::Streaming => p::agent_reply(),
        ActivityPhase::Other => p::dim(),
    }
}

pub fn activity_strip_line(text: &str, phase: ActivityPhase) -> Line<'static> {
    Line::from(Span::styled(
        text.to_string(),
        Style::default()
            .fg(activity_phase_color(phase))
            .bg(p::bg())
            .add_modifier(Modifier::BOLD),
    ))
}

pub fn approval_color(label: &str) -> Color {
    match label {
        "Auto" => p::warning(),
        "Never" => p::error(),
        "Untrusted" => p::tool_call(),
        _ => p::foreground(),
    }
}

pub fn approval_border() -> Style {
    Style::default().fg(p::warning()).bg(p::bg())
}

pub fn approval_body() -> Style {
    Style::default().fg(p::foreground()).bg(p::bg())
}

pub fn checklist_header() -> Style {
    Style::default()
        .fg(p::foreground())
        .bg(p::sidebar_bg())
        .add_modifier(Modifier::BOLD)
}

pub fn checklist_done() -> Style {
    Style::default().fg(p::agent_reply()).bg(p::sidebar_bg())
}

pub fn checklist_in_progress() -> Style {
    Style::default()
        .fg(p::thinking())
        .bg(p::sidebar_bg())
        .add_modifier(Modifier::BOLD)
}

pub fn checklist_pending() -> Style {
    Style::default().fg(p::dim()).bg(p::sidebar_bg())
}

fn role_style(kind: TranscriptLineKind, live: bool) -> Style {
    let base = shell_main();
    match kind {
        TranscriptLineKind::User => base.fg(p::user_prompt()).add_modifier(Modifier::BOLD),
        TranscriptLineKind::Assistant => base.fg(p::agent_reply()).add_modifier(Modifier::BOLD),
        TranscriptLineKind::Thinking => base.fg(p::thinking()).add_modifier(Modifier::BOLD),
        TranscriptLineKind::ToolChain => base.fg(p::tool_call()).add_modifier(if live {
            Modifier::BOLD
        } else {
            Modifier::empty()
        }),
        TranscriptLineKind::System => base.fg(p::error()).add_modifier(Modifier::BOLD),
        TranscriptLineKind::Meta => base.fg(p::dim()),
        TranscriptLineKind::Spacer => shell_main(),
    }
}

fn body_style(kind: TranscriptLineKind, live: bool) -> Style {
    let base = shell_main();
    match kind {
        TranscriptLineKind::User => base.fg(p::user_text()),
        TranscriptLineKind::Assistant => base.fg(p::agent_reply()),
        TranscriptLineKind::Thinking => {
            if live {
                base.fg(p::thinking())
            } else {
                base.fg(p::dim())
            }
        }
        TranscriptLineKind::ToolChain => {
            if live {
                base.fg(p::tool_call()).add_modifier(Modifier::BOLD)
            } else {
                base.fg(p::tool_call())
            }
        }
        TranscriptLineKind::System => base.fg(p::error()).add_modifier(Modifier::BOLD),
        TranscriptLineKind::Meta => base.fg(p::dim()),
        TranscriptLineKind::Spacer => shell_main(),
    }
}

fn code_surface() -> Style {
    Style::default().fg(p::foreground()).bg(p::code_bg())
}

/// Build a transcript line with a bright role tag and contrasting body text.
pub fn transcript_line(kind: TranscriptLineKind, text: &str, live: bool) -> Line<'static> {
    if kind == TranscriptLineKind::Spacer {
        return Line::from(Span::styled(text.to_string(), shell_main()));
    }

    if kind == TranscriptLineKind::Assistant && super::markdown_table::is_table_render_line(text) {
        return Line::from(Span::styled(text.to_string(), code_surface()));
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
            body_style(TranscriptLineKind::Thinking, live),
        ));
    }
    if text.starts_with("    ") {
        return Line::from(Span::styled(text.to_string(), body_style(kind, live)));
    }

    Line::from(Span::styled(text.to_string(), body_style(kind, live)))
}

fn tagged_line(tag: &str, body: &str, kind: TranscriptLineKind, live: bool) -> Line<'static> {
    // Assistant / tool tag and body share the same token — one span avoids
    // Windows terminal width overflow when tag+body are split (streaming overlap).
    if matches!(
        kind,
        TranscriptLineKind::Assistant | TranscriptLineKind::ToolChain
    ) {
        return Line::from(Span::styled(format!("{tag}{body}"), body_style(kind, live)));
    }
    Line::from(vec![
        Span::styled(tag.to_string(), role_style(kind, live)),
        Span::styled(body.to_string(), body_style(kind, live)),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body_fg(line: &Line<'_>) -> Option<Color> {
        line.spans.last().and_then(|s| s.style.fg)
    }

    fn tag_fg(line: &Line<'_>) -> Option<Color> {
        line.spans.first().and_then(|s| s.style.fg)
    }

    fn body_bg(line: &Line<'_>) -> Option<Color> {
        line.spans.last().and_then(|s| s.style.bg)
    }

    #[test]
    fn transcript_roles_use_palette_tokens() {
        let user = transcript_line(TranscriptLineKind::User, "you> hi", false);
        let think = transcript_line(TranscriptLineKind::Thinking, "THK> reasoning done", false);
        let tool = transcript_line(TranscriptLineKind::ToolChain, "tool + read_file: ok", false);
        let ai = transcript_line(TranscriptLineKind::Assistant, "AI> hello", false);
        let table = transcript_line(TranscriptLineKind::Assistant, "| a | b |", false);

        assert_eq!(tag_fg(&user), Some(p::user_prompt()));
        assert_eq!(body_fg(&user), Some(p::user_text()));
        assert_eq!(tag_fg(&think), Some(p::thinking()));
        assert_eq!(body_fg(&think), Some(p::dim()));
        assert_eq!(tag_fg(&tool), Some(p::tool_call()));
        assert_eq!(body_fg(&tool), Some(p::tool_call()));
        assert_eq!(tag_fg(&ai), Some(p::agent_reply()));
        assert_eq!(body_fg(&ai), Some(p::agent_reply()));
        assert_eq!(body_fg(&table), Some(p::foreground()));
        assert_eq!(body_bg(&table), Some(p::code_bg()));
    }

    #[test]
    fn shell_main_uses_dracula_bg() {
        assert_eq!(shell_main().bg, Some(p::bg()));
    }
}
