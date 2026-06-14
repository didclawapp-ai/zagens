//! Composer slash commands (`/workspace`, `/model`, …) with picker UI.

use std::path::{Path, PathBuf};

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use zagens_config::LhtComposerMode;

use super::composer_editor::ComposerEditor;
use super::theme;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlashAction {
    SwitchWorkspace(PathBuf),
    SwitchModel(String),
    SetLhtMode(LhtComposerMode),
    CycleLhtMode,
    SwitchTheme(super::theme::TuiThemeId),
    CycleTheme,
    NewSession,
    ShowHelp,
    ShowAutomation,
    ClearComposer,
}

#[derive(Debug, Clone, Copy)]
pub struct SlashCommandDef {
    pub name: &'static str,
    pub description: &'static str,
    pub takes_arg: bool,
    pub action: SlashActionKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SlashActionKind {
    Workspace,
    Model,
    Lht,
    Theme,
    New,
    Help,
    Automation,
    Clear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum SlashPickerMode {
    #[default]
    Commands,
    Models,
    LhtModes,
    Themes,
}

const COMMANDS: &[SlashCommandDef] = &[
    SlashCommandDef {
        name: "workspace",
        description: "Switch workspace directory",
        takes_arg: true,
        action: SlashActionKind::Workspace,
    },
    SlashCommandDef {
        name: "cd",
        description: "Switch workspace (alias)",
        takes_arg: true,
        action: SlashActionKind::Workspace,
    },
    SlashCommandDef {
        name: "model",
        description: "Switch text model for this session",
        takes_arg: true,
        action: SlashActionKind::Model,
    },
    SlashCommandDef {
        name: "m",
        description: "Switch model (alias)",
        takes_arg: true,
        action: SlashActionKind::Model,
    },
    SlashCommandDef {
        name: "lht",
        description: "LHT mode: auto / strict / off (empty cycles)",
        takes_arg: true,
        action: SlashActionKind::Lht,
    },
    SlashCommandDef {
        name: "theme",
        description: "Switch TUI color theme (empty cycles)",
        takes_arg: true,
        action: SlashActionKind::Theme,
    },
    SlashCommandDef {
        name: "new",
        description: "New session in current workspace",
        takes_arg: false,
        action: SlashActionKind::New,
    },
    SlashCommandDef {
        name: "help",
        description: "Show keyboard shortcuts",
        takes_arg: false,
        action: SlashActionKind::Help,
    },
    SlashCommandDef {
        name: "auto",
        description: "Automation rules: hooks, timers, triggers",
        takes_arg: false,
        action: SlashActionKind::Automation,
    },
    SlashCommandDef {
        name: "clear",
        description: "Clear composer input",
        takes_arg: false,
        action: SlashActionKind::Clear,
    },
];

#[derive(Debug, Clone, Default)]
pub struct SlashCommandState {
    pub open: bool,
    pub selected: usize,
    mode: SlashPickerMode,
}

impl SlashCommandState {
    pub fn sync(&mut self, composer: &str, composer_focus: bool, model_catalog: &[String]) {
        if !composer_focus || !composer.starts_with('/') || composer.contains('\n') {
            self.close();
            return;
        }
        // Freeform path entry — close the command palette so typing/paste work normally.
        if workspace_arg_active(composer) {
            self.close();
            return;
        }
        if lht_picker_active(composer) {
            self.open = true;
            self.mode = SlashPickerMode::LhtModes;
            let count = filter_lht_modes(composer).len();
            if count == 0 {
                self.selected = 0;
            } else if self.selected >= count {
                self.selected = count - 1;
            }
            return;
        }
        if theme_picker_active(composer) {
            self.open = true;
            self.mode = SlashPickerMode::Themes;
            let count = filter_themes(composer).len();
            if count == 0 {
                self.selected = 0;
            } else if self.selected >= count {
                self.selected = count - 1;
            }
            return;
        }
        if model_picker_active(composer) {
            self.open = true;
            self.mode = SlashPickerMode::Models;
            let count = filter_models(composer, model_catalog).len();
            if count == 0 {
                self.selected = 0;
            } else if self.selected >= count {
                self.selected = count - 1;
            }
            return;
        }
        self.mode = SlashPickerMode::Commands;
        self.open = true;
        let count = filter_commands(composer).len();
        if count == 0 {
            self.selected = 0;
        } else if self.selected >= count {
            self.selected = count - 1;
        }
    }

    pub fn move_up(&mut self, composer: &str, model_catalog: &[String]) {
        if !self.open {
            return;
        }
        self.selected = self.selected.saturating_sub(1);
        let _ = (composer, model_catalog);
    }

    pub fn move_down(&mut self, composer: &str, model_catalog: &[String]) {
        if !self.open {
            return;
        }
        let count = match self.mode {
            SlashPickerMode::LhtModes => filter_lht_modes(composer).len(),
            SlashPickerMode::Themes => filter_themes(composer).len(),
            SlashPickerMode::Models => filter_models(composer, model_catalog).len(),
            SlashPickerMode::Commands => filter_commands(composer).len(),
        };
        if count == 0 {
            self.selected = 0;
        } else {
            self.selected = (self.selected + 1).min(count - 1);
        }
    }

    pub fn close(&mut self) {
        self.open = false;
        self.selected = 0;
        self.mode = SlashPickerMode::Commands;
    }
}

pub fn filter_commands(composer: &str) -> Vec<&'static SlashCommandDef> {
    let query = slash_query(composer);
    COMMANDS
        .iter()
        .filter(|cmd| query.is_empty() || cmd.name.starts_with(&query))
        .collect()
}

const LHT_MODES: &[LhtComposerMode] = &[
    LhtComposerMode::Auto,
    LhtComposerMode::Strict,
    LhtComposerMode::Off,
];

pub fn filter_lht_modes(composer: &str) -> Vec<LhtComposerMode> {
    let arg = lht_arg(composer).unwrap_or("");
    let query = arg.trim().to_ascii_lowercase();
    LHT_MODES
        .iter()
        .copied()
        .filter(|mode| {
            query.is_empty() || mode.as_str().starts_with(&query) || mode.as_str().contains(&query)
        })
        .collect()
}

pub fn lht_picker_active(composer: &str) -> bool {
    split_command_line(composer)
        .map(|(name, _)| is_lht_command(name))
        .unwrap_or(false)
}

fn is_lht_command(name: &str) -> bool {
    name.eq_ignore_ascii_case("lht")
}

fn lht_arg(composer: &str) -> Option<&str> {
    let (name, arg) = split_command_line(composer)?;
    if is_lht_command(name) {
        Some(arg)
    } else {
        None
    }
}

fn parse_lht_arg(arg: &str) -> Option<LhtComposerMode> {
    match arg.trim().to_ascii_lowercase().as_str() {
        "auto" => Some(LhtComposerMode::Auto),
        "strict" => Some(LhtComposerMode::Strict),
        "off" => Some(LhtComposerMode::Off),
        _ => None,
    }
}

// ── Theme picker ─────────────────────────────────────────────────────────────

pub fn theme_picker_active(composer: &str) -> bool {
    split_command_line(composer)
        .map(|(name, _)| is_theme_command(name))
        .unwrap_or(false)
}

fn is_theme_command(name: &str) -> bool {
    name.eq_ignore_ascii_case("theme")
}

fn theme_arg(composer: &str) -> Option<&str> {
    let (name, arg) = split_command_line(composer)?;
    if is_theme_command(name) {
        Some(arg)
    } else {
        None
    }
}

fn parse_theme_arg(arg: &str) -> Option<super::theme::TuiThemeId> {
    let lower = arg.trim().to_ascii_lowercase().replace('_', "-");
    super::theme::TuiThemeId::ALL
        .iter()
        .copied()
        .find(|id| id.as_str() == lower.as_str())
}

pub fn filter_themes(composer: &str) -> Vec<super::theme::TuiThemeId> {
    let arg = theme_arg(composer).unwrap_or("");
    let query = arg.trim().to_ascii_lowercase().replace('_', "-");
    super::theme::TuiThemeId::ALL
        .iter()
        .copied()
        .filter(|id| {
            query.is_empty()
                || id.as_str().contains(query.as_str())
                || id.label().to_ascii_lowercase().contains(query.as_str())
        })
        .collect()
}

pub fn selected_theme(composer: &str, selected: usize) -> Option<super::theme::TuiThemeId> {
    filter_themes(composer).into_iter().nth(selected)
}

pub fn filter_models(composer: &str, catalog: &[String]) -> Vec<String> {
    let arg = model_arg(composer).unwrap_or("");
    let query = arg.trim().to_ascii_lowercase();
    catalog
        .iter()
        .filter(|m| {
            query.is_empty()
                || m.to_ascii_lowercase().contains(&query)
                || m.to_ascii_lowercase().starts_with(&query)
        })
        .cloned()
        .collect()
}

pub fn model_picker_active(composer: &str) -> bool {
    split_command_line(composer)
        .map(|(name, _)| is_model_command(name))
        .unwrap_or(false)
}

/// `/workspace` or `/cd` with optional path — not the command palette (freeform arg).
pub fn workspace_arg_active(composer: &str) -> bool {
    split_command_line(composer)
        .map(|(name, _)| is_workspace_command(name))
        .unwrap_or(false)
}

pub(crate) fn is_workspace_command(name: &str) -> bool {
    matches!(name.to_ascii_lowercase().as_str(), "workspace" | "cd")
}

fn is_model_command(name: &str) -> bool {
    matches!(name.to_ascii_lowercase().as_str(), "model" | "m")
}

fn model_arg(composer: &str) -> Option<&str> {
    let (name, arg) = split_command_line(composer)?;
    if is_model_command(name) {
        Some(arg)
    } else {
        None
    }
}

fn slash_query(composer: &str) -> String {
    composer
        .trim_start_matches('/')
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
}

pub(crate) fn split_command_line(composer: &str) -> Option<(&str, &str)> {
    let line = composer.trim();
    if !line.starts_with('/') {
        return None;
    }
    let body = line.trim_start_matches('/').trim_start();
    let mut parts = body.splitn(2, char::is_whitespace);
    let name = parts.next()?.trim();
    if name.is_empty() {
        return None;
    }
    let arg = parts.next().unwrap_or("").trim();
    Some((name, arg))
}

fn find_command(name: &str) -> Option<&'static SlashCommandDef> {
    let lower = name.to_ascii_lowercase();
    COMMANDS
        .iter()
        .find(|c| c.name.eq_ignore_ascii_case(&lower))
}

pub fn try_parse_action(composer: &str, current_workspace: &Path) -> Option<SlashAction> {
    let (name, arg) = split_command_line(composer)?;
    let cmd = find_command(name)?;
    match cmd.action {
        SlashActionKind::Workspace => {
            if arg.is_empty() {
                return None;
            }
            let path = resolve_workspace_path(arg, current_workspace).ok()?;
            Some(SlashAction::SwitchWorkspace(path))
        }
        SlashActionKind::Model => {
            if arg.is_empty() {
                return None;
            }
            Some(SlashAction::SwitchModel(arg.trim().to_string()))
        }
        SlashActionKind::Lht => {
            if arg.is_empty() {
                Some(SlashAction::CycleLhtMode)
            } else {
                parse_lht_arg(arg).map(SlashAction::SetLhtMode)
            }
        }
        SlashActionKind::Theme => {
            if arg.is_empty() {
                Some(SlashAction::CycleTheme)
            } else {
                parse_theme_arg(arg).map(SlashAction::SwitchTheme)
            }
        }
        SlashActionKind::New => Some(SlashAction::NewSession),
        SlashActionKind::Help => Some(SlashAction::ShowHelp),
        SlashActionKind::Automation => Some(SlashAction::ShowAutomation),
        SlashActionKind::Clear => Some(SlashAction::ClearComposer),
    }
}

pub fn apply_palette_selection(composer: &mut ComposerEditor, cmd: &SlashCommandDef) {
    composer.clear();
    if cmd.takes_arg {
        composer.insert_str(&format!("/{} ", cmd.name));
    } else {
        composer.insert_str(&format!("/{}", cmd.name));
    }
}

pub fn selected_command(composer: &str, selected: usize) -> Option<&'static SlashCommandDef> {
    filter_commands(composer).into_iter().nth(selected)
}

pub fn selected_lht_mode(composer: &str, selected: usize) -> Option<LhtComposerMode> {
    filter_lht_modes(composer).into_iter().nth(selected)
}

pub fn selected_model(composer: &str, selected: usize, catalog: &[String]) -> Option<String> {
    filter_models(composer, catalog).into_iter().nth(selected)
}

pub fn render_palette(
    composer: &str,
    selected: usize,
    width: usize,
    max_rows: usize,
    model_catalog: &[String],
    current_model: &str,
    current_lht_mode: LhtComposerMode,
) -> Vec<Line<'static>> {
    if lht_picker_active(composer) {
        return render_lht_palette(composer, selected, width, max_rows, current_lht_mode);
    }
    if theme_picker_active(composer) {
        return render_theme_palette(composer, selected, width, max_rows);
    }
    if model_picker_active(composer) {
        return render_model_palette(
            composer,
            selected,
            width,
            max_rows,
            model_catalog,
            current_model,
        );
    }
    render_command_palette(composer, selected, width, max_rows)
}

fn render_command_palette(
    composer: &str,
    selected: usize,
    width: usize,
    max_rows: usize,
) -> Vec<Line<'static>> {
    let matches = filter_commands(composer);
    if matches.is_empty() {
        return vec![Line::from(Span::styled(
            pad(width, " (no matching commands)"),
            theme::hint(),
        ))];
    }

    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled(
        pad(width, " Commands | ^v select  Enter confirm  Esc cancel"),
        theme::hint(),
    )));

    let visible = max_rows.saturating_sub(1).max(1);
    let start = if selected >= visible {
        selected + 1 - visible
    } else {
        0
    };
    for (idx, cmd) in matches.iter().enumerate().skip(start).take(visible) {
        let mark = if idx == selected { ">" } else { " " };
        let hint = if cmd.takes_arg {
            format!("/{} <arg>", cmd.name)
        } else {
            format!("/{}", cmd.name)
        };
        let label = format!("{mark} {hint:<22} {desc}", desc = cmd.description);
        let style = if idx == selected {
            theme::palette_selection()
        } else {
            theme::hint()
        };
        lines.push(Line::from(Span::styled(pad(width, &label), style)));
    }
    lines
}

fn render_model_palette(
    composer: &str,
    selected: usize,
    width: usize,
    max_rows: usize,
    catalog: &[String],
    current_model: &str,
) -> Vec<Line<'static>> {
    let matches = filter_models(composer, catalog);
    if matches.is_empty() {
        return vec![Line::from(Span::styled(
            pad(width, " (no matching models)"),
            theme::hint(),
        ))];
    }

    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled(
        pad(width, " Models | ^v select  Enter apply  Esc cancel"),
        theme::hint(),
    )));

    let visible = max_rows.saturating_sub(1).max(1);
    let start = if selected >= visible {
        selected + 1 - visible
    } else {
        0
    };
    for (idx, model) in matches.iter().enumerate().skip(start).take(visible) {
        let mark = if idx == selected { ">" } else { " " };
        let active = model.eq_ignore_ascii_case(current_model);
        let suffix = if active { "  (current)" } else { "" };
        let label = format!("{mark} {model}{suffix}");
        let style = if idx == selected {
            Style::default()
                .fg(theme::footer_model())
                .add_modifier(Modifier::BOLD)
        } else if active {
            Style::default().fg(theme::footer_model())
        } else {
            theme::hint()
        };
        lines.push(Line::from(Span::styled(pad(width, &label), style)));
    }
    lines
}

fn render_lht_palette(
    composer: &str,
    selected: usize,
    width: usize,
    max_rows: usize,
    current_mode: LhtComposerMode,
) -> Vec<Line<'static>> {
    let matches = filter_lht_modes(composer);
    if matches.is_empty() {
        return vec![Line::from(Span::styled(
            pad(width, " (no matching LHT modes)"),
            theme::hint(),
        ))];
    }

    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled(
        pad(
            width,
            " LHT | ^v select  Enter apply  empty /lht cycles  Esc cancel",
        ),
        theme::hint(),
    )));

    let visible = max_rows.saturating_sub(1).max(1);
    let start = if selected >= visible {
        selected + 1 - visible
    } else {
        0
    };
    for (idx, mode) in matches.iter().enumerate().skip(start).take(visible) {
        let mark = if idx == selected { ">" } else { " " };
        let active = *mode == current_mode;
        let suffix = if active { "  (current)" } else { "" };
        let label = format!("{mark} {}{suffix}", mode.as_str());
        let style = if idx == selected {
            Style::default()
                .fg(theme::footer_lht())
                .add_modifier(Modifier::BOLD)
        } else if active {
            Style::default().fg(theme::footer_lht())
        } else {
            theme::hint()
        };
        lines.push(Line::from(Span::styled(pad(width, &label), style)));
    }
    lines
}

fn render_theme_palette(
    composer: &str,
    selected: usize,
    width: usize,
    max_rows: usize,
) -> Vec<Line<'static>> {
    let matches = filter_themes(composer);
    if matches.is_empty() {
        return vec![Line::from(Span::styled(
            pad(width, " (no matching themes)"),
            theme::hint(),
        ))];
    }

    let current_id = super::theme::current_id();
    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled(
        pad(
            width,
            " Theme | ^v select  Enter apply  empty /theme cycles  Esc cancel",
        ),
        theme::hint(),
    )));

    let visible = max_rows.saturating_sub(1).max(1);
    let start = if selected >= visible {
        selected + 1 - visible
    } else {
        0
    };
    for (idx, id) in matches.iter().enumerate().skip(start).take(visible) {
        let mark = if idx == selected { ">" } else { " " };
        let active = *id == current_id;
        let suffix = if active { "  (current)" } else { "" };
        let label = format!("{mark} {}{suffix}", id.label());
        let style = if idx == selected {
            Style::default()
                .fg(theme::footer_context()
                    .fg
                    .unwrap_or(ratatui::style::Color::Cyan))
                .add_modifier(Modifier::BOLD)
        } else if active {
            Style::default().fg(theme::footer_context()
                .fg
                .unwrap_or(ratatui::style::Color::Cyan))
        } else {
            theme::hint()
        };
        lines.push(Line::from(Span::styled(pad(width, &label), style)));
    }
    lines
}

fn pad(width: usize, text: &str) -> String {
    super::display_format::pad_line_display_width(text, width.max(8))
}

pub fn resolve_workspace_path(input: &str, current: &Path) -> anyhow::Result<PathBuf> {
    use anyhow::{Context, bail};

    let trimmed = input.trim();
    if trimmed.is_empty() {
        bail!("workspace path is empty");
    }
    let path = PathBuf::from(trimmed);
    let candidate = if path.is_absolute() {
        path
    } else {
        current.join(path)
    };
    let canon = std::fs::canonicalize(&candidate)
        .with_context(|| format!("workspace path not found: {}", candidate.display()))?;
    if !canon.is_dir() {
        bail!("workspace path is not a directory: {}", canon.display());
    }
    Ok(canon)
}

pub fn composer_is_slash_command(composer: &str) -> bool {
    composer.trim_start().starts_with('/')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_matches_prefix() {
        let matches = filter_commands("/wor");
        assert!(matches.iter().any(|c| c.name == "workspace"));
    }

    #[test]
    fn filter_model_command() {
        let matches = filter_commands("/mod");
        assert!(matches.iter().any(|c| c.name == "model"));
    }

    #[test]
    fn parse_model_with_id() {
        let action = try_parse_action("/model deepseek-v4-flash", Path::new(".")).expect("action");
        match action {
            SlashAction::SwitchModel(m) => assert_eq!(m, "deepseek-v4-flash"),
            _ => panic!("expected model"),
        }
    }

    #[test]
    fn parse_workspace_with_path() {
        let dir = std::env::temp_dir();
        let action = try_parse_action(&format!("/workspace {}", dir.display()), Path::new("."))
            .expect("action");
        match action {
            SlashAction::SwitchWorkspace(p) => assert!(p.is_dir()),
            _ => panic!("expected workspace"),
        }
    }

    #[test]
    fn incomplete_model_returns_none() {
        assert!(try_parse_action("/model", Path::new(".")).is_none());
    }

    #[test]
    fn filter_lht_command() {
        let matches = filter_commands("/lht");
        assert!(matches.iter().any(|c| c.name == "lht"));
    }

    #[test]
    fn parse_lht_cycle_when_empty() {
        let action = try_parse_action("/lht", Path::new(".")).expect("action");
        assert_eq!(action, SlashAction::CycleLhtMode);
    }

    #[test]
    fn parse_lht_strict() {
        let action = try_parse_action("/lht strict", Path::new(".")).expect("action");
        assert_eq!(action, SlashAction::SetLhtMode(LhtComposerMode::Strict));
    }

    #[test]
    fn lht_picker_filters_modes() {
        let hits = filter_lht_modes("/lht st");
        assert_eq!(hits, vec![LhtComposerMode::Strict]);
    }

    #[test]
    fn model_picker_filters_catalog() {
        let catalog = vec![
            "auto".to_string(),
            "deepseek-v4-pro".to_string(),
            "deepseek-v4-flash".to_string(),
        ];
        let hits = filter_models("/model flash", &catalog);
        assert_eq!(hits, vec!["deepseek-v4-flash".to_string()]);
    }

    #[test]
    fn workspace_arg_active_after_command_selected() {
        assert!(workspace_arg_active("/workspace "));
        assert!(workspace_arg_active("/workspace F:\\repo"));
        assert!(workspace_arg_active("/cd ../other"));
        assert!(!workspace_arg_active("/wor"));
        assert!(!workspace_arg_active("/model "));
    }

    #[test]
    fn apply_palette_selection_moves_cursor_to_end() {
        let mut editor = ComposerEditor::default();
        editor.insert_str("/wor");
        editor.move_home();
        apply_palette_selection(
            &mut editor,
            &SlashCommandDef {
                name: "workspace",
                description: "",
                takes_arg: true,
                action: SlashActionKind::Workspace,
            },
        );
        assert_eq!(editor.text(), "/workspace ");
        assert_eq!(editor.cursor(), editor.text().len());
    }
}
