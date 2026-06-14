//! Automation settings overlay: list of rules + inline edit form.
//!
//! Opened via `/auto`. Key bindings (list mode):
//!   j/k or ↑/↓  — move selection
//!   Space        — toggle rule enabled/disabled
//!   n            — new rule (enter edit mode)
//!   Enter        — edit selected rule
//!   d / Delete   — delete selected rule
//!   e            — open config file in $EDITOR / notepad
//!   Esc          — close overlay
//!
//! Key bindings (edit mode):
//!   Tab / Shift+Tab — move between form fields
//!   ←/→ or Space   — cycle selector values (trigger/action type)
//!   any char        — type into text fields (name / secs / prompt text)
//!   Backspace       — delete last char in text field
//!   Enter           — save and return to list
//!   Esc             — cancel (discard edits)

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};

use super::super::automation::{ActionKind, AutomationConfig, AutomationRule, TriggerKind};
use super::super::theme;

// ── UI State ───────────────────────────────────────────────────────────────

/// Which edit form field currently has keyboard focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EditField {
    #[default]
    Name,
    TriggerType,
    /// Duration in seconds (Interval / Idle triggers).
    Secs,
    /// Optional tool-name filter (ToolComplete trigger).
    ToolFilter,
    ActionType,
    Text,
}

impl EditField {
    /// Total number of form fields (including conditionally hidden ones).
    const COUNT: usize = 6;

    fn index(self) -> usize {
        match self {
            Self::Name => 0,
            Self::TriggerType => 1,
            Self::Secs => 2,
            Self::ToolFilter => 3,
            Self::ActionType => 4,
            Self::Text => 5,
        }
    }

    fn from_index(i: usize) -> Self {
        match i {
            0 => Self::Name,
            1 => Self::TriggerType,
            2 => Self::Secs,
            3 => Self::ToolFilter,
            4 => Self::ActionType,
            _ => Self::Text,
        }
    }

    /// Returns `true` if this field accepts free-text typed characters.
    pub fn is_text_input(self) -> bool {
        matches!(
            self,
            Self::Name | Self::Secs | Self::ToolFilter | Self::Text
        )
    }
}

/// The mutable UI state for the automation overlay.
/// Stored in `AppState`; initialised by default (overlay starts closed).
#[derive(Debug, Default)]
pub struct AutomationUiState {
    /// Index of the highlighted rule in the list.
    pub list_selected: usize,
    /// Whether the edit form is currently shown.
    pub edit_mode: bool,
    /// Which form field has focus while editing.
    pub edit_field: EditField,
    /// `Some(id)` → editing existing rule; `None` → creating new rule.
    pub editing_id: Option<u64>,

    // Form buffers (owned strings typed by the user).
    pub f_name: String,
    pub f_trigger_idx: usize,
    /// Used for Interval/Idle triggers.
    pub f_secs: String,
    /// Used for ToolComplete trigger (optional tool name filter).
    pub f_tool_filter: String,
    pub f_action_idx: usize,
    pub f_text: String,
}

impl AutomationUiState {
    // ── List navigation ────────────────────────────────────────────────────

    pub fn clamp_selection(&mut self, len: usize) {
        if len == 0 {
            self.list_selected = 0;
        } else if self.list_selected >= len {
            self.list_selected = len - 1;
        }
    }

    pub fn move_down(&mut self, len: usize) {
        if len > 0 {
            self.list_selected = (self.list_selected + 1).min(len - 1);
        }
    }

    pub fn move_up(&mut self) {
        self.list_selected = self.list_selected.saturating_sub(1);
    }

    // ── Edit form management ───────────────────────────────────────────────

    pub fn start_new(&mut self) {
        self.edit_mode = true;
        self.editing_id = None;
        self.edit_field = EditField::Name;
        self.f_name = String::new();
        self.f_trigger_idx = 3; // default: TurnComplete (most common hook)
        self.f_secs = "300".to_string();
        self.f_tool_filter = String::new();
        self.f_action_idx = 0; // SendPrompt
        self.f_text = String::new();
    }

    pub fn start_edit(&mut self, rule: &AutomationRule) {
        self.edit_mode = true;
        self.editing_id = Some(rule.id);
        self.edit_field = EditField::Name;
        self.f_name = rule.name.clone();
        self.f_trigger_idx = rule.trigger.kind_index();
        self.f_secs = rule.trigger.secs().to_string();
        self.f_tool_filter = rule.trigger.tool_filter().unwrap_or("").to_string();
        // Populate from the first action (TUI form only edits the primary action).
        if let Some(first_action) = rule.actions.first() {
            self.f_action_idx = first_action.kind_index();
            self.f_text = first_action.text().to_string();
        } else {
            self.f_action_idx = 0;
            self.f_text = String::new();
        }
    }

    pub fn cancel_edit(&mut self) {
        self.edit_mode = false;
    }

    /// Build the rule from the current form buffers.
    /// Returns `None` if the name or text is empty.
    pub fn build_rule(&self, id: u64) -> Option<AutomationRule> {
        let name = self.f_name.trim().to_string();
        let text = self.f_text.trim().to_string();
        if name.is_empty() || text.is_empty() {
            return None;
        }
        let secs: u64 = self.f_secs.trim().parse().unwrap_or(300).max(10);
        let trigger = TriggerKind::from_parts(self.f_trigger_idx, secs, &self.f_tool_filter);
        let action = ActionKind::from_parts(self.f_action_idx, text);
        Some(AutomationRule::new(id, name, trigger, vec![action]))
    }

    // ── Field navigation ───────────────────────────────────────────────────

    /// Advance to the next visible field, skipping fields not relevant to the
    /// current trigger type.
    pub fn next_field(&mut self) {
        let has_secs = matches!(self.f_trigger_idx, 1 | 2);
        let has_tool = self.f_trigger_idx == 4;

        let cur = self.edit_field.index();
        let mut next = (cur + 1) % EditField::COUNT;
        if !has_secs && next == EditField::Secs.index() {
            next = (next + 1) % EditField::COUNT;
        }
        if !has_tool && next == EditField::ToolFilter.index() {
            next = (next + 1) % EditField::COUNT;
        }
        self.edit_field = EditField::from_index(next);
    }

    pub fn prev_field(&mut self) {
        let has_secs = matches!(self.f_trigger_idx, 1 | 2);
        let has_tool = self.f_trigger_idx == 4;

        let cur = self.edit_field.index();
        let mut prev = if cur == 0 {
            EditField::COUNT - 1
        } else {
            cur - 1
        };
        if !has_tool && prev == EditField::ToolFilter.index() {
            prev = if prev == 0 {
                EditField::COUNT - 1
            } else {
                prev - 1
            };
        }
        if !has_secs && prev == EditField::Secs.index() {
            prev = if prev == 0 {
                EditField::COUNT - 1
            } else {
                prev - 1
            };
        }
        self.edit_field = EditField::from_index(prev);
    }

    /// Cycle the current selector field (trigger type / action type).
    pub fn cycle_selector(&mut self, forward: bool) {
        match self.edit_field {
            EditField::TriggerType => {
                let n = TriggerKind::NAMES.len();
                self.f_trigger_idx = if forward {
                    (self.f_trigger_idx + 1) % n
                } else {
                    (self.f_trigger_idx + n - 1) % n
                };
            }
            EditField::ActionType => {
                let n = ActionKind::NAMES.len();
                self.f_action_idx = if forward {
                    (self.f_action_idx + 1) % n
                } else {
                    (self.f_action_idx + n - 1) % n
                };
            }
            _ => {}
        }
    }

    pub fn input_char(&mut self, ch: char) {
        match self.edit_field {
            EditField::Name => self.f_name.push(ch),
            EditField::Secs => {
                if ch.is_ascii_digit() {
                    self.f_secs.push(ch);
                }
            }
            EditField::ToolFilter => self.f_tool_filter.push(ch),
            EditField::Text => self.f_text.push(ch),
            EditField::TriggerType | EditField::ActionType => {
                if ch == ' ' {
                    self.cycle_selector(true);
                }
            }
        }
    }

    pub fn backspace(&mut self) {
        match self.edit_field {
            EditField::Name => {
                self.f_name.pop();
            }
            EditField::Secs => {
                self.f_secs.pop();
            }
            EditField::ToolFilter => {
                self.f_tool_filter.pop();
            }
            EditField::Text => {
                self.f_text.pop();
            }
            _ => {}
        }
    }
}

// ── Drawing ────────────────────────────────────────────────────────────────

/// Draw the automation overlay over the current frame.
pub fn draw_automation(frame: &mut Frame<'_>, config: &AutomationConfig, ui: &AutomationUiState) {
    let area = centered_rect(84, 80, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::border_focus())
        .style(theme::overlay_panel())
        .title(" Automation  [/auto] ");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if ui.edit_mode {
        draw_edit_view(frame, inner, config, ui);
    } else {
        draw_list_view(frame, inner, config, ui);
    }
}

// ── List view ──────────────────────────────────────────────────────────────

fn draw_list_view(
    frame: &mut Frame<'_>,
    area: Rect,
    config: &AutomationConfig,
    ui: &AutomationUiState,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // hint bar
            Constraint::Min(1),    // rule list
            Constraint::Length(1), // bottom hint / count
        ])
        .split(area);

    frame.render_widget(
        Paragraph::new(
            " j/k select  Space toggle  n new  Enter edit  d delete  e open editor  Esc close",
        )
        .style(theme::hint()),
        chunks[0],
    );

    let rules = &config.rules;
    if rules.is_empty() {
        frame.render_widget(
            Paragraph::new("\n  No automation rules yet. Press n to create one,\n  or e to open the TOML config in your editor.")
                .style(theme::hint()),
            chunks[1],
        );
    } else {
        let items: Vec<ListItem> = rules
            .iter()
            .map(|r| render_rule_item(r, chunks[1].width as usize))
            .collect();

        let mut list_state = ListState::default();
        list_state.select(Some(ui.list_selected.min(rules.len().saturating_sub(1))));

        let list = List::new(items)
            .highlight_style(theme::palette_selection())
            .highlight_symbol("> ");

        frame.render_stateful_widget(list, chunks[1], &mut list_state);
    }

    // Bottom hint: rule count + editor hint.
    let config_path = AutomationConfig::path()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "~/.deepseek/automation.toml".to_string());
    let count_text = format!(
        "  {} rule{}  |  e → {}",
        rules.len(),
        if rules.len() == 1 { "" } else { "s" },
        config_path,
    );
    frame.render_widget(Paragraph::new(count_text).style(theme::hint()), chunks[2]);
}

fn render_rule_item(rule: &AutomationRule, width: usize) -> ListItem<'static> {
    let enabled_marker = if rule.enabled { "✓" } else { "✗" };
    let trigger_sum = rule.trigger.summary();
    let action_sum = rule.primary_action_summary();

    // Append action count badge when a rule has multiple actions.
    let action_badge = if rule.actions.len() > 1 {
        format!("[+{}]", rule.actions.len() - 1)
    } else {
        String::new()
    };

    let name_w = 28usize;
    let name_display: String = if rule.name.chars().count() > name_w {
        let s: String = rule.name.chars().take(name_w - 1).collect();
        format!("{s}…")
    } else {
        format!("{:<name_w$}", rule.name)
    };

    let trigger_w = 14usize;
    let trigger_display = format!("{:<trigger_w$}", trigger_sum);

    let action_sum_w = width.saturating_sub(2 + 2 + name_w + 2 + trigger_w + 2);
    let action_with_badge = format!("{action_sum}{action_badge}");
    let action_display: String =
        if action_with_badge.chars().count() > action_sum_w && action_sum_w > 2 {
            let s: String = action_with_badge.chars().take(action_sum_w - 1).collect();
            format!("{s}…")
        } else {
            action_with_badge
        };

    let text = format!("{enabled_marker}  {name_display}  {trigger_display}  {action_display}");

    let style = if rule.enabled {
        theme::approval_body()
    } else {
        theme::hint()
    };
    ListItem::new(text).style(style)
}

// ── Edit view ──────────────────────────────────────────────────────────────

fn draw_edit_view(
    frame: &mut Frame<'_>,
    area: Rect,
    config: &AutomationConfig,
    ui: &AutomationUiState,
) {
    let _ = config;

    let title = if ui.editing_id.is_some() {
        " Edit Rule "
    } else {
        " New Rule "
    };
    let hint_text = " Tab next  Shift+Tab prev  ←/→ cycle  Enter save  Esc cancel ";

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // hint bar
            Constraint::Length(1), // spacer
            Constraint::Length(1), // Name
            Constraint::Length(1), // Trigger type
            Constraint::Length(1), // Secs (conditional)
            Constraint::Length(1), // Tool filter (conditional)
            Constraint::Length(1), // Action type
            Constraint::Length(1), // Text
            Constraint::Length(1), // template hint
            Constraint::Min(1),    // validation / remaining
        ])
        .split(area);

    let title_line = format!("{title}{hint_text}");
    frame.render_widget(Paragraph::new(title_line).style(theme::hint()), chunks[0]);

    let has_secs = matches!(ui.f_trigger_idx, 1 | 2);
    let has_tool = ui.f_trigger_idx == 4;

    render_form_field(
        frame,
        chunks[2],
        "Name",
        &ui.f_name,
        ui.edit_field == EditField::Name,
        false,
    );

    let trigger_val = TriggerKind::NAMES
        .get(ui.f_trigger_idx)
        .copied()
        .unwrap_or("?");
    render_selector_field(
        frame,
        chunks[3],
        "Trigger",
        trigger_val,
        ui.edit_field == EditField::TriggerType,
    );

    render_form_field(
        frame,
        chunks[4],
        "Seconds",
        if has_secs { &ui.f_secs } else { "—" },
        ui.edit_field == EditField::Secs,
        !has_secs,
    );

    let tool_hint = if ui.f_tool_filter.is_empty() && has_tool {
        "any tool (leave blank)"
    } else {
        &ui.f_tool_filter
    };
    render_form_field(
        frame,
        chunks[5],
        "Tool name",
        if has_tool { tool_hint } else { "—" },
        ui.edit_field == EditField::ToolFilter,
        !has_tool,
    );

    let action_val = ActionKind::NAMES
        .get(ui.f_action_idx)
        .copied()
        .unwrap_or("?");
    render_selector_field(
        frame,
        chunks[6],
        "Action",
        action_val,
        ui.edit_field == EditField::ActionType,
    );

    let text_label = match ui.f_action_idx {
        0 => "Prompt",
        2 => "Shell cmd",
        3 => "Message",
        _ => "Command",
    };
    render_form_field(
        frame,
        chunks[7],
        text_label,
        &ui.f_text,
        ui.edit_field == EditField::Text,
        false,
    );

    // Template variable hint (shown when editing text/cmd fields).
    let tpl_hint = "  vars: {{tool_name}}  {{error_message}}  {{session_id}}";
    frame.render_widget(Paragraph::new(tpl_hint).style(theme::hint()), chunks[8]);

    let ok = !ui.f_name.trim().is_empty() && !ui.f_text.trim().is_empty();
    let msg = if ok {
        "  Press Enter to save  |  use 'e' key (list mode) to add multiple actions via TOML"
    } else {
        "  Name and text/command are required"
    };
    let msg_style = if ok {
        theme::hint()
    } else {
        theme::approval_body()
    };
    frame.render_widget(Paragraph::new(msg).style(msg_style), chunks[9]);
}

fn render_form_field(
    frame: &mut Frame<'_>,
    area: Rect,
    label: &str,
    value: &str,
    focused: bool,
    dimmed: bool,
) {
    let label_w = 10usize;
    let label_part = format!("  {label:<label_w$} ");
    let label_style = if dimmed {
        theme::hint()
    } else if focused {
        Style::default().fg(theme::border_focus().fg.unwrap_or(Color::Cyan))
    } else {
        theme::approval_body()
    };
    let cursor = if focused && !dimmed { "▌" } else { "" };
    let value_display = format!("{value}{cursor}");
    let value_style = if dimmed {
        theme::hint()
    } else if focused {
        theme::palette_selection()
    } else {
        theme::approval_body()
    };

    let spans = vec![
        Span::styled(label_part, label_style),
        Span::styled(value_display, value_style),
    ];
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_selector_field(
    frame: &mut Frame<'_>,
    area: Rect,
    label: &str,
    value: &str,
    focused: bool,
) {
    let label_w = 10usize;
    let label_part = format!("  {label:<label_w$} ");
    let label_style = if focused {
        Style::default().fg(theme::border_focus().fg.unwrap_or(Color::Cyan))
    } else {
        theme::approval_body()
    };
    let value_display = format!("[ {value} ]  ←/→ cycle");
    let value_style = if focused {
        theme::palette_selection()
    } else {
        theme::hint()
    };

    let spans = vec![
        Span::styled(label_part, label_style),
        Span::styled(value_display, value_style),
    ];
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

// ── Geometry ───────────────────────────────────────────────────────────────

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vert[1])[1]
}
