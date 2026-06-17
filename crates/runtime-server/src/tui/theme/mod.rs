//! Dracula-aligned TUI palette (24-bit RGB where supported).
//!
//! **Authoritative spec:** `doc_Private/docs/TUI方案.md` §6.10 — semantic token
//! assignments live in [`palette`]; panel backgrounds are theme-specific in [`surfaces`].

mod palette;
mod surfaces;

pub use palette::*;
pub use surfaces::{TuiTheme, TuiThemeId, current, current_id, install, pane_chrome_rows};

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use super::inline_markdown::inline_spans;
use super::transcript::TranscriptLineKind;

use palette as p;

/// Role prefix width for assistant continuation indent.
pub const AI_TAG: &str = "AI> ";
pub const USER_TAG: &str = "you> ";
pub const THINK_TAG: &str = "THK> ";
pub const TOOL_TAG: &str = "tool ";
pub const COMPOSER_PROMPT: &str = "> ";

/// Layout region for surface-aware styling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TuiPanel {
    Title,
    Left,
    Transcript,
    Activity,
    Composer,
    Status,
    Inspector,
    Lht,
}

impl TuiPanel {
    #[must_use]
    pub fn surface_style(self, focused: bool) -> Style {
        let theme = current();
        let bg = if focused {
            theme.surfaces.active_for(self)
        } else {
            theme.surfaces.surface_for(self)
        };
        Style::default().fg(p::foreground()).bg(bg)
    }

    #[must_use]
    pub fn surface_color(self, focused: bool) -> Color {
        self.surface_style(focused).bg.unwrap_or(p::bg())
    }
}

/// Panel-scoped style helpers (preferred over legacy `sidebar_*` / `shell_*`).
#[derive(Debug, Clone, Copy)]
pub struct PanelStyles {
    panel: TuiPanel,
}

#[must_use]
pub fn panel(panel: TuiPanel) -> PanelStyles {
    PanelStyles { panel }
}

impl PanelStyles {
    #[must_use]
    pub fn surface(self, focused: bool) -> Style {
        self.panel.surface_style(focused)
    }

    #[must_use]
    pub fn heading(self) -> Style {
        self.surface(false)
            .fg(p::foreground())
            .add_modifier(Modifier::BOLD)
    }

    #[must_use]
    pub fn item(self, selected: bool) -> Style {
        let theme = current();
        let bg = if selected {
            theme.surfaces.active_for(self.panel)
        } else {
            theme.surfaces.surface_for(self.panel)
        };
        Style::default().fg(p::item_text()).bg(bg)
    }

    #[must_use]
    pub fn hint(self) -> Style {
        self.surface(false).fg(p::dim())
    }

    #[must_use]
    pub fn tab(self, active: bool) -> Style {
        if active {
            self.surface(false)
                .fg(current().palette.user_prompt)
                .add_modifier(Modifier::BOLD)
        } else {
            self.hint()
        }
    }

    #[must_use]
    pub fn checklist_header(self) -> Style {
        self.heading()
    }

    #[must_use]
    pub fn checklist_done(self) -> Style {
        self.surface(false).fg(current().palette.agent_reply)
    }

    #[must_use]
    pub fn checklist_in_progress(self) -> Style {
        self.surface(false)
            .fg(current().palette.thinking)
            .add_modifier(Modifier::BOLD)
    }

    #[must_use]
    pub fn checklist_in_progress_active(self) -> Style {
        self.item(true)
            .fg(current().palette.thinking)
            .add_modifier(Modifier::BOLD)
    }

    #[must_use]
    pub fn checklist_pending(self) -> Style {
        self.surface(false).fg(p::dim())
    }

    #[must_use]
    pub fn composer_prompt(self) -> Style {
        self.surface(true)
            .fg(current().palette.user_prompt)
            .add_modifier(Modifier::BOLD)
    }

    #[must_use]
    pub fn composer_input(self) -> Style {
        self.surface(true)
            .fg(p::user_text())
            .add_modifier(Modifier::BOLD)
    }

    #[must_use]
    pub fn composer_idle(self) -> Style {
        self.surface(false).fg(p::dim())
    }

    #[must_use]
    pub fn footer_separator(self) -> Style {
        self.surface(false).fg(p::border_idle())
    }

    #[must_use]
    pub fn footer_chip(self, color: Color) -> Style {
        self.surface(false).fg(color).add_modifier(Modifier::BOLD)
    }

    #[must_use]
    pub fn footer_muted(self) -> Style {
        self.surface(false).fg(p::dim())
    }

    #[must_use]
    pub fn footer_workspace(self) -> Style {
        self.footer_muted()
    }

    #[must_use]
    pub fn footer_context(self) -> Style {
        self.surface(false).fg(current().palette.tool_call)
    }

    #[must_use]
    pub fn palette_selection(self) -> Style {
        self.surface(true)
            .fg(current().palette.user_prompt)
            .add_modifier(Modifier::BOLD)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityPhase {
    Thinking,
    Tools,
    Streaming,
    Other,
}

/// Initialize theme from persisted layout prefs (`tui-layout.toml` → `tui_theme`).
pub fn install_from_prefs(theme_key: Option<&str>) {
    let id = TuiThemeId::from_storage(theme_key);
    install(TuiTheme::resolve(id));
}

pub fn warning() -> Color {
    p::warning()
}

// ── Legacy helpers (delegate to default panels) ─────────────────────────────

pub fn shell_main() -> Style {
    panel(TuiPanel::Transcript).surface(false)
}

pub fn shell_sidebar() -> Style {
    panel(TuiPanel::Left).surface(false)
}

pub fn sidebar_heading() -> Style {
    panel(TuiPanel::Left).heading()
}

pub fn sidebar_item(selected: bool) -> Style {
    panel(TuiPanel::Left).item(selected)
}

pub fn sidebar_item_muted() -> Style {
    panel(TuiPanel::Left).hint()
}

pub fn sidebar_tab(active: bool) -> Style {
    panel(TuiPanel::Left).tab(active)
}

pub fn sidebar_hint() -> Style {
    panel(TuiPanel::Left).hint()
}

pub fn overlay_panel() -> Style {
    shell_main()
}

pub fn border_focus() -> Style {
    Style::default()
        .fg(p::border_focus())
        .bg(TuiPanel::Transcript.surface_color(false))
}

pub fn border_idle() -> Style {
    Style::default()
        .fg(p::border_idle())
        .bg(TuiPanel::Transcript.surface_color(false))
}

pub fn border_focus_sidebar() -> Style {
    Style::default()
        .fg(p::border_focus())
        .bg(TuiPanel::Left.surface_color(false))
}

pub fn border_idle_sidebar() -> Style {
    Style::default()
        .fg(p::border_idle())
        .bg(TuiPanel::Left.surface_color(false))
}

pub fn title_bar() -> Style {
    panel(TuiPanel::Title)
        .surface(false)
        .add_modifier(Modifier::BOLD)
}

pub fn hint() -> Style {
    panel(TuiPanel::Composer).hint()
}

pub fn composer_prompt() -> Style {
    panel(TuiPanel::Composer).composer_prompt()
}

pub fn composer_input() -> Style {
    panel(TuiPanel::Composer).composer_input()
}

pub fn composer_idle() -> Style {
    panel(TuiPanel::Composer).composer_idle()
}

pub fn composer_cursor() -> Style {
    composer_prompt()
}

pub fn composer_line(prompt_and_body: &str, focused: bool) -> Line<'static> {
    let styles = panel(TuiPanel::Composer);
    let body_style = if focused {
        styles.composer_input()
    } else {
        styles.composer_idle()
    };
    if let Some(body) = prompt_and_body.strip_prefix(COMPOSER_PROMPT) {
        return Line::from(vec![
            Span::styled(COMPOSER_PROMPT.to_string(), styles.composer_prompt()),
            Span::styled(body.to_string(), body_style),
        ]);
    }
    Line::from(Span::styled(prompt_and_body.to_string(), body_style))
}

pub fn footer_separator() -> Style {
    panel(TuiPanel::Status).footer_separator()
}

pub fn footer_chip(color: Color) -> Style {
    panel(TuiPanel::Status).footer_chip(color)
}

pub fn footer_model() -> Color {
    current().palette.user_prompt
}

pub fn footer_mode() -> Color {
    p::warning()
}

pub fn footer_task() -> Color {
    current().palette.agent_reply
}

pub fn footer_lht() -> Color {
    current().palette.tool_call
}

pub fn footer_workspace() -> Style {
    panel(TuiPanel::Status).footer_workspace()
}

pub fn footer_context() -> Style {
    panel(TuiPanel::Status).footer_context()
}

pub fn footer_muted() -> Style {
    panel(TuiPanel::Status).footer_muted()
}

pub fn palette_selection() -> Style {
    panel(TuiPanel::Composer).palette_selection()
}

pub fn activity_phase_color(phase: ActivityPhase) -> Color {
    let pal = current().palette;
    match phase {
        ActivityPhase::Thinking => pal.thinking,
        ActivityPhase::Tools => pal.tool_call,
        ActivityPhase::Streaming => pal.agent_reply,
        ActivityPhase::Other => p::dim(),
    }
}

pub fn activity_strip_line(text: &str, phase: ActivityPhase) -> Line<'static> {
    Line::from(Span::styled(
        text.to_string(),
        panel(TuiPanel::Activity)
            .surface(false)
            .fg(activity_phase_color(phase))
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
    panel(TuiPanel::Inspector).checklist_header()
}

pub fn checklist_done() -> Style {
    panel(TuiPanel::Inspector).checklist_done()
}

pub fn checklist_in_progress() -> Style {
    panel(TuiPanel::Inspector).checklist_in_progress()
}

pub fn checklist_in_progress_active() -> Style {
    panel(TuiPanel::Inspector).checklist_in_progress_active()
}

pub fn checklist_pending() -> Style {
    panel(TuiPanel::Inspector).checklist_pending()
}

fn role_style(kind: TranscriptLineKind, live: bool) -> Style {
    let base = shell_main();
    let pal = current().palette;
    match kind {
        TranscriptLineKind::User => base.fg(pal.user_prompt).add_modifier(Modifier::BOLD),
        TranscriptLineKind::Assistant => base.fg(pal.agent_reply).add_modifier(Modifier::BOLD),
        TranscriptLineKind::Thinking => base.fg(pal.thinking).add_modifier(Modifier::BOLD),
        TranscriptLineKind::ToolChain => base.fg(pal.tool_call).add_modifier(if live {
            Modifier::BOLD
        } else {
            Modifier::empty()
        }),
        TranscriptLineKind::ToolError => base.fg(p::warning()).add_modifier(Modifier::BOLD),
        TranscriptLineKind::System => base.fg(p::error()).add_modifier(Modifier::BOLD),
        TranscriptLineKind::Notice => base.fg(p::dim()),
        TranscriptLineKind::Meta => base.fg(p::dim()),
        TranscriptLineKind::Spacer => shell_main(),
    }
}

fn body_style(kind: TranscriptLineKind, live: bool) -> Style {
    let base = shell_main();
    let pal = current().palette;
    match kind {
        TranscriptLineKind::User => base.fg(p::user_text()),
        TranscriptLineKind::Assistant => base.fg(pal.agent_reply),
        TranscriptLineKind::Thinking => {
            if live {
                base.fg(pal.thinking)
            } else {
                base.fg(p::dim())
            }
        }
        TranscriptLineKind::ToolChain => {
            if live {
                base.fg(pal.tool_call).add_modifier(Modifier::BOLD)
            } else {
                base.fg(pal.tool_call)
            }
        }
        TranscriptLineKind::ToolError => base.fg(p::warning()).add_modifier(Modifier::BOLD),
        TranscriptLineKind::System => base.fg(p::error()).add_modifier(Modifier::BOLD),
        TranscriptLineKind::Notice => base.fg(p::dim()),
        TranscriptLineKind::Meta => base.fg(p::dim()),
        TranscriptLineKind::Spacer => shell_main(),
    }
}

/// Style for inline code spans (`` `code` ``).
pub fn code_inline() -> Style {
    shell_main().fg(p::tool_call()).bg(p::code_bg())
}

fn table_line_style(text: &str) -> Style {
    let trimmed = text.trim();
    // Box-drawing border lines (┌ ├ └) or legacy ASCII (+) get dim style.
    let is_border = !trimmed.is_empty()
        && (trimmed.starts_with('┌')
            || trimmed.starts_with('├')
            || trimmed.starts_with('└')
            || trimmed.chars().all(|ch| matches!(ch, '+' | '-' | ' ')));
    if is_border {
        return shell_main().fg(p::dim());
    }
    body_style(TranscriptLineKind::Assistant, false)
}

/// Render a data row that uses ASCII `|` separators as multiple spans:
/// the `|` characters get dim style, cell content keeps body style.
/// ASCII `|` is 1-column wide (unambiguous), so multi-span is safe here.
fn table_data_row_line(text: &str, content_style: Style) -> Line<'static> {
    let sep_style = shell_main().fg(p::dim());
    let mut spans: Vec<Span<'static>> = Vec::new();
    let clean = super::markdown_table::strip_inline_markers(text);
    let mut buf = String::new();
    for ch in clean.chars() {
        if ch == '|' {
            if !buf.is_empty() {
                spans.push(Span::styled(buf.clone(), content_style));
                buf.clear();
            }
            spans.push(Span::styled("|".to_string(), sep_style));
        } else {
            buf.push(ch);
        }
    }
    if !buf.is_empty() {
        spans.push(Span::styled(buf, content_style));
    }
    Line::from(spans)
}

/// Build a transcript line with a bright role tag and contrasting body text.
///
/// For `Assistant` continuation lines (4-space indent), this also:
/// - Detects Markdown headings (`# `, `## `, `### `) and renders them bold+bright.
/// - Detects list items (`- `, `* `, `+ `) and replaces the marker with `•`.
/// - Applies inline Markdown parsing (`**bold**`, `` `code` ``, `*italic*`).
pub fn transcript_line(
    kind: TranscriptLineKind,
    text: &str,
    live: bool,
    code_lang: Option<&str>,
) -> Line<'static> {
    if kind == TranscriptLineKind::Spacer {
        return Line::from(Span::styled(text.to_string(), shell_main()));
    }

    // ── Code-block lines (inside ``` fences) ──────────────────────────────
    if let Some(lang) = code_lang {
        let base = shell_main();
        // Header line "__header__<lang>": render as styled label
        if let Some(actual_lang) = lang.strip_prefix("__header__") {
            let label_style = base.fg(p::dim()).add_modifier(Modifier::BOLD);
            // For mermaid, append an "o:open" hint in a different colour.
            if actual_lang.eq_ignore_ascii_case("mermaid") {
                let hint_style = base.fg(p::dim());
                // text is "    [mermaid]" — append hint after the bracket
                let content = text.trim_end_matches(']');
                return Line::from(vec![
                    Span::styled(content.to_string(), label_style),
                    Span::styled(" o:open]".to_string(), hint_style),
                ]);
            }
            return Line::from(Span::styled(text.to_string(), label_style));
        }
        // Content line: syntax highlight using inspector syntax module.
        let syn_lang = super::inspector::syntax::Lang::from_path(&format!("file.{lang}"));
        let indent = "    ";
        let raw = text.strip_prefix(indent).unwrap_or(text);
        let mut spans: Vec<Span<'static>> = vec![Span::styled(indent.to_string(), base)];
        spans.extend(super::inspector::syntax::highlight_line(raw, syn_lang));
        return Line::from(spans);
    }

    if kind == TranscriptLineKind::Assistant && super::markdown_table::is_table_render_line(text) {
        // Header rows use Unicode │ (single span to avoid ambiguous-width artefacts).
        if text.trim_start().starts_with('│') {
            let clean = super::markdown_table::strip_inline_markers(text);
            return Line::from(Span::styled(clean, body_style(kind, live)));
        }
        // Data rows use ASCII | separator — safe to split into multiple spans because
        // ASCII | is unambiguously 1-column wide.
        if text.trim_start().starts_with('|') {
            return table_data_row_line(text, body_style(kind, live));
        }
        // Border lines (┌ ├ └ or +) get dim style.
        return Line::from(Span::styled(text.to_string(), table_line_style(text)));
    }

    if let Some(rest) = text.strip_prefix(USER_TAG) {
        return tagged_line(USER_TAG, rest, kind, live);
    }
    if kind == TranscriptLineKind::User {
        if let Some(rest) = text.strip_prefix("+ ") {
            return tagged_line("+ ", rest, kind, live);
        }
    }
    if let Some(rest) = text.strip_prefix(AI_TAG) {
        // First assistant line: apply inline markdown to the body after the tag.
        let base = body_style(kind, live);
        let tag_style = role_style(kind, live);
        let mut spans = vec![Span::styled(AI_TAG.to_string(), tag_style)];
        if !rest.is_empty() {
            spans.extend(inline_spans(rest, base, code_inline()));
        }
        return Line::from(spans);
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

    // ── User continuation lines (4-space indent, no repeated you> tag) ─────────
    if kind == TranscriptLineKind::User && text.starts_with("    ") {
        let base = body_style(kind, live);
        return Line::from(vec![
            Span::styled("    ".to_string(), base),
            Span::styled(text[4..].to_string(), base),
        ]);
    }

    // ── Assistant continuation lines (4-space indent) ──────────────────────────
    if kind == TranscriptLineKind::Assistant && text.starts_with("    ") {
        let prefix = "    ";
        let content = &text[4..];
        let base = body_style(kind, live);
        let indent = Span::styled(prefix.to_string(), base);

        // Heading: # / ## / ###
        for (marker, level) in [("### ", 3u8), ("## ", 2), ("# ", 1)] {
            if let Some(heading) = content.strip_prefix(marker) {
                let heading_style = heading_style(level);
                let mut spans = vec![indent];
                spans.push(Span::styled(marker.to_string(), heading_style));
                spans.extend(inline_spans(heading, heading_style, code_inline()));
                return Line::from(spans);
            }
        }

        // Bullet list: - / * / +
        for bullet_marker in ["- ", "* ", "+ "] {
            if let Some(item) = content.strip_prefix(bullet_marker) {
                let mut spans = vec![
                    indent,
                    Span::styled("• ".to_string(), base.add_modifier(Modifier::BOLD)),
                ];
                spans.extend(inline_spans(item, base, code_inline()));
                return Line::from(spans);
            }
        }

        // Horizontal rule
        if content.trim() == "---" || content.trim() == "***" || content.trim() == "___" {
            let rule = "─".repeat(40);
            return Line::from(Span::styled(rule, shell_main().fg(p::dim())));
        }

        // Plain continuation: apply inline markdown
        let mut spans = vec![indent];
        spans.extend(inline_spans(content, base, code_inline()));
        return Line::from(spans);
    }

    Line::from(Span::styled(text.to_string(), body_style(kind, live)))
}

fn heading_style(level: u8) -> Style {
    let pal = current().palette;
    let base = shell_main();
    match level {
        1 => base
            .fg(pal.agent_reply)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        2 => base.fg(pal.agent_reply).add_modifier(Modifier::BOLD),
        _ => base.fg(p::foreground()).add_modifier(Modifier::BOLD),
    }
}

fn tagged_line(tag: &str, body: &str, kind: TranscriptLineKind, live: bool) -> Line<'static> {
    if matches!(
        kind,
        TranscriptLineKind::Assistant
            | TranscriptLineKind::ToolChain
            | TranscriptLineKind::ToolError
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

    fn setup() {
        install(TuiTheme::default_theme());
    }

    #[test]
    fn transcript_roles_use_palette_tokens() {
        setup();
        let user = transcript_line(TranscriptLineKind::User, "you> hi", false, None);
        let think = transcript_line(
            TranscriptLineKind::Thinking,
            "THK> reasoning done",
            false,
            None,
        );
        let tool = transcript_line(
            TranscriptLineKind::ToolChain,
            "tool + read_file: ok",
            false,
            None,
        );
        let ai = transcript_line(TranscriptLineKind::Assistant, "AI> hello", false, None);
        let table_rule = transcript_line(TranscriptLineKind::Assistant, "+---+---+", false, None);
        let table_row = transcript_line(TranscriptLineKind::Assistant, "| a | b |", false, None);

        assert_eq!(tag_fg(&user), Some(current().palette.user_prompt));
        assert_eq!(body_fg(&user), Some(p::user_text()));
        assert_eq!(tag_fg(&think), Some(current().palette.thinking));
        assert_eq!(body_fg(&think), Some(p::dim()));
        assert_eq!(tag_fg(&tool), Some(current().palette.tool_call));
        assert_eq!(body_fg(&tool), Some(current().palette.tool_call));
        assert_eq!(tag_fg(&ai), Some(current().palette.agent_reply));
        assert_eq!(body_fg(&ai), Some(current().palette.agent_reply));
        assert_eq!(body_fg(&table_rule), Some(p::dim()));
        assert_eq!(body_bg(&table_rule), Some(p::bg()));
        // table_data_row_line emits multi-span: [|dim, content, |dim, content, |dim …]
        // The last span is the trailing dim "|"; the first content span (index 1) carries
        // the body colour.
        let row_content_fg = table_row.spans.get(1).and_then(|s| s.style.fg);
        assert_eq!(row_content_fg, Some(current().palette.agent_reply));
        assert_ne!(body_bg(&table_row), Some(p::code_bg()));
    }

    #[test]
    fn shell_main_uses_transcript_surface() {
        setup();
        assert_eq!(
            shell_main().bg,
            Some(TuiTheme::default_theme().surfaces.transcript)
        );
    }

    #[test]
    fn cool_blue_left_differs_from_transcript() {
        setup();
        assert_ne!(
            panel(TuiPanel::Left).surface(false).bg,
            panel(TuiPanel::Transcript).surface(false).bg
        );
    }
}
