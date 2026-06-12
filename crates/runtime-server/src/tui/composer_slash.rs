//! Composer slash commands (`/workspace`, `/model`, …) with picker UI.

use std::path::{Path, PathBuf};

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use super::theme;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlashAction {
    SwitchWorkspace(PathBuf),
    SwitchModel(String),
    NewSession,
    ShowHelp,
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
    New,
    Help,
    Clear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum SlashPickerMode {
    #[default]
    Commands,
    Models,
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

fn split_command_line(composer: &str) -> Option<(&str, &str)> {
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
        SlashActionKind::New => Some(SlashAction::NewSession),
        SlashActionKind::Help => Some(SlashAction::ShowHelp),
        SlashActionKind::Clear => Some(SlashAction::ClearComposer),
    }
}

pub fn apply_palette_selection(composer: &mut String, cmd: &SlashCommandDef) {
    if cmd.takes_arg {
        *composer = format!("/{} ", cmd.name);
    } else {
        *composer = format!("/{}", cmd.name);
    }
}

pub fn selected_command(composer: &str, selected: usize) -> Option<&'static SlashCommandDef> {
    filter_commands(composer).into_iter().nth(selected)
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
) -> Vec<Line<'static>> {
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
        pad(width, " Commands — ↑↓ select · Enter confirm · Esc cancel"),
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
        pad(width, " Models — ↑↓ select · Enter apply · Esc cancel"),
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
    fn model_picker_filters_catalog() {
        let catalog = vec![
            "auto".to_string(),
            "deepseek-v4-pro".to_string(),
            "deepseek-v4-flash".to_string(),
        ];
        let hits = filter_models("/model flash", &catalog);
        assert_eq!(hits, vec!["deepseek-v4-flash".to_string()]);
    }
}
