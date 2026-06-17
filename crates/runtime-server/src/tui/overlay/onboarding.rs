//! First-run onboarding overlay (welcome, API key, default task type).

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::localization::{Locale, MessageId, tr};

use super::super::composer_editor::ComposerEditor;
use super::super::onboarding::OnboardingPhase;
use super::super::theme;
use super::centered_rect;

const MODE_OPTIONS: [&str; 3] = ["auto", "code", "office"];

#[derive(Debug)]
pub struct OnboardingUiState {
    pub phases: Vec<OnboardingPhase>,
    pub step_index: usize,
    pub mode_selected: usize,
    pub api_key: ComposerEditor,
    pub error: Option<String>,
    pub busy: bool,
}

impl OnboardingUiState {
    pub fn new(phases: Vec<OnboardingPhase>) -> Self {
        Self {
            phases,
            step_index: 0,
            mode_selected: 1, // code default
            api_key: ComposerEditor::default(),
            error: None,
            busy: false,
        }
    }

    pub fn phase(&self) -> Option<OnboardingPhase> {
        self.phases.get(self.step_index).copied()
    }

    pub fn is_first_step(&self) -> bool {
        self.step_index == 0
    }

    pub fn is_last_step(&self) -> bool {
        self.step_index + 1 >= self.phases.len()
    }

    pub fn advance(&mut self) {
        if self.step_index + 1 < self.phases.len() {
            self.step_index += 1;
            self.error = None;
        }
    }

    pub fn back(&mut self) {
        if self.step_index > 0 {
            self.step_index -= 1;
            self.error = None;
        }
    }

    pub fn move_mode_up(&mut self) {
        if self.mode_selected > 0 {
            self.mode_selected -= 1;
        }
    }

    pub fn move_mode_down(&mut self) {
        if self.mode_selected + 1 < MODE_OPTIONS.len() {
            self.mode_selected += 1;
        }
    }

    pub fn selected_task_type(&self) -> &'static str {
        MODE_OPTIONS[self.mode_selected.min(MODE_OPTIONS.len() - 1)]
    }

    pub fn insert_char(&mut self, ch: char) {
        if matches!(self.phase(), Some(OnboardingPhase::ApiKey)) {
            self.api_key.insert_char(ch);
            self.error = None;
        }
    }

    pub fn delete_backward(&mut self) {
        if matches!(self.phase(), Some(OnboardingPhase::ApiKey)) {
            self.api_key.delete_backward();
        }
    }

    pub fn paste(&mut self, text: &str) {
        if matches!(self.phase(), Some(OnboardingPhase::ApiKey)) {
            self.api_key.insert_str(text.trim());
            self.error = None;
        }
    }
}

pub fn draw_onboarding(
    frame: &mut Frame<'_>,
    locale: Locale,
    ui: &OnboardingUiState,
    workspace_display: &str,
) {
    let area = centered_rect(78, 72, frame.area());
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::border_focus())
        .style(theme::overlay_panel())
        .title(format!(" {} ", tr(locale, MessageId::TuiOnboardingTitle)));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();
    lines.push(step_rail(locale, ui));
    lines.push(Line::from(""));

    match ui.phase() {
        Some(OnboardingPhase::Welcome) => {
            lines.push(Line::from(Span::styled(
                tr(locale, MessageId::TuiOnboardingWelcomeTitle),
                Style::default().add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(tr(locale, MessageId::TuiOnboardingWelcomeBody)));
            lines.push(Line::from(""));
            lines.push(Line::from(format!(
                "{} {}",
                tr(locale, MessageId::TuiOnboardingWorkspace),
                workspace_display
            )));
        }
        Some(OnboardingPhase::ApiKey) => {
            lines.push(Line::from(Span::styled(
                tr(locale, MessageId::TuiOnboardingKeyTitle),
                Style::default().add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(tr(locale, MessageId::TuiOnboardingKeyHint)));
            lines.push(Line::from(""));
            let key_display = mask_api_key(ui.api_key.text());
            lines.push(Line::from(vec![
                Span::styled("sk- ", theme::composer_prompt()),
                Span::raw(key_display),
            ]));
        }
        Some(OnboardingPhase::TaskType) => {
            lines.push(Line::from(Span::styled(
                tr(locale, MessageId::TuiOnboardingModeTitle),
                Style::default().add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(""));
            for (idx, mode) in MODE_OPTIONS.iter().enumerate() {
                let mark = if idx == ui.mode_selected { "▸" } else { " " };
                let label = mode_label(locale, mode);
                let style = if idx == ui.mode_selected {
                    theme::footer_chip(theme::footer_mode())
                } else {
                    theme::hint()
                };
                lines.push(Line::from(Span::styled(format!("{mark} {label}"), style)));
            }
        }
        None => {}
    }

    if let Some(err) = &ui.error {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            err.clone(),
            Style::default().fg(Color::Red),
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(tr(locale, MessageId::TuiOnboardingFooter)));

    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .style(theme::approval_body()),
        inner,
    );
}

fn step_rail(locale: Locale, ui: &OnboardingUiState) -> Line<'static> {
    let mut spans = Vec::new();
    for (idx, phase) in ui.phases.iter().enumerate() {
        if idx > 0 {
            spans.push(Span::raw("  →  "));
        }
        let label = phase_label(locale, *phase);
        let active = idx == ui.step_index;
        spans.push(Span::styled(
            label.to_string(),
            if active {
                theme::footer_chip(theme::footer_mode())
            } else {
                theme::hint()
            },
        ));
    }
    Line::from(spans)
}

fn phase_label(locale: Locale, phase: OnboardingPhase) -> &'static str {
    match phase {
        OnboardingPhase::Welcome => tr(locale, MessageId::TuiOnboardingStepWelcome),
        OnboardingPhase::ApiKey => tr(locale, MessageId::TuiOnboardingStepKey),
        OnboardingPhase::TaskType => tr(locale, MessageId::TuiOnboardingStepMode),
    }
}

fn mode_label(locale: Locale, mode: &str) -> String {
    let title = match mode {
        "auto" => tr(locale, MessageId::TuiOnboardingModeAuto),
        "office" => tr(locale, MessageId::TuiOnboardingModeOffice),
        _ => tr(locale, MessageId::TuiOnboardingModeCode),
    };
    let desc = match mode {
        "auto" => tr(locale, MessageId::TuiOnboardingModeAutoDesc),
        "office" => tr(locale, MessageId::TuiOnboardingModeOfficeDesc),
        _ => tr(locale, MessageId::TuiOnboardingModeCodeDesc),
    };
    format!("{title} — {desc}")
}

fn mask_api_key(text: &str) -> String {
    let t = text.trim();
    if t.is_empty() {
        return String::new();
    }
    if t.len() <= 8 {
        return "*".repeat(t.len());
    }
    format!(
        "{}…{}",
        "*".repeat(t.len().saturating_sub(4)),
        &t[t.len() - 4..]
    )
}
