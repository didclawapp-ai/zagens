//! LHT lower pane — objective, plan phases, checklist (no completion gate).

use ratatui::text::{Line, Span};

use super::super::display_format::{display_width, truncate_display_width};
use super::super::harness::ChecklistStatus;
use super::super::task_graph::{PhaseStatus, TaskGraphSnapshot};
use super::super::theme::{self, TuiPanel};

const LHT: TuiPanel = TuiPanel::Lht;

pub fn line_count(graph: Option<&TaskGraphSnapshot>) -> usize {
    let Some(graph) = graph else {
        return 1;
    };
    if !graph.has_activity() {
        return 1;
    }
    let mut n = 3; // header + progress + spacer
    if !graph.objective.is_empty() {
        n += 1;
    }
    if graph.lht_blocked || graph.nudge_count > 0 {
        n += 1;
    }
    if !graph.phases.is_empty() {
        n += 1 + graph.phases.len();
    }
    if !graph.checklist.is_empty() {
        n += 1 + graph.checklist.len();
    }
    n
}

pub fn render_styled_panel(
    graph: Option<&TaskGraphSnapshot>,
    height: usize,
    scroll: usize,
    max_cols: usize,
) -> Vec<Line<'static>> {
    let max_cols = max_cols.max(8);
    let Some(graph) = graph.filter(|g| g.has_activity()) else {
        return vec![Line::from(Span::styled(
            "No LHT activity.",
            theme::panel(LHT).hint(),
        ))];
    };

    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled(
        "LHT",
        theme::panel(LHT).checklist_header(),
    )));

    if !graph.objective.is_empty() {
        lines.push(Line::from(Span::styled(
            truncate_display(&graph.objective, max_cols),
            theme::panel(LHT).item(false),
        )));
    }

    let bar_width = 16usize;
    let filled = (graph.completion_pct as usize * bar_width / 100).min(bar_width);
    let progress = format!(
        "{:>3}% [{}{}] open:{}",
        graph.completion_pct,
        "#".repeat(filled),
        "-".repeat(bar_width.saturating_sub(filled)),
        graph.open_items
    );
    lines.push(Line::from(Span::styled(
        progress,
        theme::panel(LHT).checklist_done(),
    )));

    if graph.lht_blocked || graph.nudge_count > 0 {
        let mut badges = Vec::new();
        if graph.lht_blocked {
            badges.push(Span::styled(
                " blocked ",
                theme::panel(LHT).surface(false).fg(theme::warning()),
            ));
        }
        if graph.nudge_count > 0 {
            badges.push(Span::styled(
                format!(" nudge:{} ", graph.nudge_count),
                theme::panel(LHT).hint(),
            ));
        }
        lines.push(Line::from(badges));
    }

    if !graph.phases.is_empty() {
        let plan_outline = !graph.checklist.is_empty()
            && graph
                .phases
                .iter()
                .any(|p| p.status != PhaseStatus::Completed)
            && graph.completion_pct >= 100;
        let plan_title = if plan_outline {
            "Plan (outline)"
        } else {
            "Plan"
        };
        lines.push(Line::from(Span::styled(
            plan_title,
            theme::panel(LHT).heading(),
        )));
        for phase in &graph.phases {
            let dim = plan_outline && phase.status != PhaseStatus::Completed;
            let (mark, style) = phase_style(phase.status, dim);
            lines.push(Line::from(vec![
                Span::styled(format!("{mark} "), style),
                Span::styled(
                    truncate_display(&phase.step, max_cols.saturating_sub(4)),
                    style,
                ),
            ]));
        }
    }

    if !graph.checklist.is_empty() {
        lines.push(Line::from(Span::styled(
            "Checklist",
            theme::panel(LHT).heading(),
        )));
        for item in &graph.checklist {
            let active = graph.in_progress_id.is_some_and(|id| id == item.id)
                || item.status == ChecklistStatus::InProgress;
            let (mark, style) = match item.status {
                ChecklistStatus::Completed => ("[x]", theme::panel(LHT).checklist_done()),
                ChecklistStatus::InProgress => {
                    ("[>]", theme::panel(LHT).checklist_in_progress_active())
                }
                ChecklistStatus::Pending if active => {
                    ("[>]", theme::panel(LHT).checklist_in_progress_active())
                }
                ChecklistStatus::Pending => ("[ ]", theme::panel(LHT).checklist_pending()),
            };
            lines.push(Line::from(vec![
                Span::styled(format!("{mark} "), style),
                Span::styled(
                    truncate_display(&item.content, max_cols.saturating_sub(4)),
                    style,
                ),
            ]));
        }
    }

    let visible = height.max(4);
    let max_scroll = lines.len().saturating_sub(visible);
    let start = scroll.min(max_scroll);
    lines.into_iter().skip(start).take(visible).collect()
}

fn phase_style(status: PhaseStatus, dim: bool) -> (&'static str, ratatui::style::Style) {
    if dim {
        return ("[ ]", theme::panel(LHT).hint());
    }
    match status {
        PhaseStatus::Completed => ("[x]", theme::panel(LHT).checklist_done()),
        PhaseStatus::InProgress => ("[>]", theme::panel(LHT).checklist_in_progress_active()),
        PhaseStatus::Pending => ("[ ]", theme::panel(LHT).checklist_pending()),
    }
}

fn truncate_display(text: &str, max_cols: usize) -> String {
    if max_cols == 0 {
        return String::new();
    }
    if display_width(text) <= max_cols {
        return text.to_string();
    }
    truncate_display_width(text, max_cols)
}
