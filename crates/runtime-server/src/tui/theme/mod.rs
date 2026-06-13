//! Dracula-aligned TUI palette (24-bit RGB where supported).
//!
//! **Authoritative spec:** `doc_Private/docs/TUI方案.md` §6.10 — semantic token
//! assignments live in [`palette`]; panel backgrounds are theme-specific in [`surfaces`].

mod palette;
mod surfaces;

pub use palette::*;
pub use surfaces::{
    ThemeLayout, TuiTheme, TuiThemeId, current, current_id, install, pane_chrome_rows,
};

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

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
                .fg(p::user_prompt())
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
        self.surface(false).fg(p::agent_reply())
    }

    #[must_use]
    pub fn checklist_in_progress(self) -> Style {
        self.surface(false)
            .fg(p::thinking())
            .add_modifier(Modifier::BOLD)
    }

    #[must_use]
    pub fn checklist_in_progress_active(self) -> Style {
        self.item(true)
            .fg(p::thinking())
            .add_modifier(Modifier::BOLD)
    }

    #[must_use]
    pub fn checklist_pending(self) -> Style {
        self.surface(false).fg(p::dim())
    }

    #[must_use]
    pub fn composer_prompt(self) -> Style {
        self.surface(true)
            .fg(p::user_prompt())
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
        self.surface(false).fg(p::tool_call())
    }

    #[must_use]
    pub fn palette_selection(self) -> Style {
        self.surface(true)
            .fg(p::user_prompt())
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
    p::user_prompt()
}

pub fn footer_mode() -> Color {
    p::warning()
}

pub fn footer_task() -> Color {
    p::agent_reply()
}

pub fn footer_lht() -> Color {
    p::tool_call()
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
    match kind {
        TranscriptLineKind::User => base.fg(p::user_prompt()).add_modifier(Modifier::BOLD),
        TranscriptLineKind::Assistant => base.fg(p::agent_reply()).add_modifier(Modifier::BOLD),
        TranscriptLineKind::Thinking => base.fg(p::thinking()).add_modifier(Modifier::BOLD),
        TranscriptLineKind::ToolChain => base.fg(p::tool_call()).add_modifier(if live {
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
        TranscriptLineKind::ToolError => base.fg(p::warning()).add_modifier(Modifier::BOLD),
        TranscriptLineKind::System => base.fg(p::error()).add_modifier(Modifier::BOLD),
        TranscriptLineKind::Notice => base.fg(p::dim()),
        TranscriptLineKind::Meta => base.fg(p::dim()),
        TranscriptLineKind::Spacer => shell_main(),
    }
}

fn table_line_style(text: &str) -> Style {
    let trimmed = text.trim();
    if !trimmed.is_empty()
        && trimmed
            .chars()
            .all(|ch| matches!(ch, '+' | '-' | '|' | ' '))
    {
        return shell_main().fg(p::dim());
    }
    body_style(TranscriptLineKind::Assistant, false)
}

/// Build a transcript line with a bright role tag and contrasting body text.
pub fn transcript_line(kind: TranscriptLineKind, text: &str, live: bool) -> Line<'static> {
    if kind == TranscriptLineKind::Spacer {
        return Line::from(Span::styled(text.to_string(), shell_main()));
    }

    if kind == TranscriptLineKind::Assistant && super::markdown_table::is_table_render_line(text) {
        return Line::from(Span::styled(text.to_string(), table_line_style(text)));
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
        let user = transcript_line(TranscriptLineKind::User, "you> hi", false);
        let think = transcript_line(TranscriptLineKind::Thinking, "THK> reasoning done", false);
        let tool = transcript_line(TranscriptLineKind::ToolChain, "tool + read_file: ok", false);
        let ai = transcript_line(TranscriptLineKind::Assistant, "AI> hello", false);
        let table_rule = transcript_line(TranscriptLineKind::Assistant, "+---+---+", false);
        let table_row = transcript_line(TranscriptLineKind::Assistant, "| a | b |", false);

        assert_eq!(tag_fg(&user), Some(p::user_prompt()));
        assert_eq!(body_fg(&user), Some(p::user_text()));
        assert_eq!(tag_fg(&think), Some(p::thinking()));
        assert_eq!(body_fg(&think), Some(p::dim()));
        assert_eq!(tag_fg(&tool), Some(p::tool_call()));
        assert_eq!(body_fg(&tool), Some(p::tool_call()));
        assert_eq!(tag_fg(&ai), Some(p::agent_reply()));
        assert_eq!(body_fg(&ai), Some(p::agent_reply()));
        assert_eq!(body_fg(&table_rule), Some(p::dim()));
        assert_eq!(body_bg(&table_rule), Some(p::bg()));
        assert_eq!(body_fg(&table_row), Some(p::agent_reply()));
        assert_eq!(body_bg(&table_row), Some(p::bg()));
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
