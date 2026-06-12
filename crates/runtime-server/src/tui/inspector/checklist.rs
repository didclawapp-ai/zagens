//! Checklist inspector tab.

use ratatui::text::{Line, Span};

use super::super::harness::{ChecklistSnapshot, ChecklistStatus};
use super::super::theme;

pub fn render_panel(snapshot: Option<&ChecklistSnapshot>, height: usize) -> Vec<String> {
    render_styled_panel(snapshot, height)
        .into_iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|s| s.content.clone())
                .collect::<String>()
        })
        .collect()
}

pub fn render_styled_panel(
    snapshot: Option<&ChecklistSnapshot>,
    height: usize,
) -> Vec<Line<'static>> {
    match snapshot {
        Some(snap) => styled_checklist(snap, height),
        None => vec![
            Line::from(Span::styled("No checklist yet.", theme::sidebar_hint())),
            Line::from(Span::styled(
                "Prompt the agent to use checklist_write.",
                theme::sidebar_hint(),
            )),
        ],
    }
}

fn styled_checklist(snapshot: &ChecklistSnapshot, height: usize) -> Vec<Line<'static>> {
    let completed = snapshot
        .items
        .iter()
        .filter(|i| i.status == ChecklistStatus::Completed)
        .count();
    let header = format!(
        "Checklist {}% ({}/{})",
        snapshot.completion_pct,
        completed,
        snapshot.items.len()
    );
    let mut lines = vec![Line::from(Span::styled(header, theme::checklist_header()))];

    let bar_width = 20usize;
    let filled = (snapshot.completion_pct as usize * bar_width / 100).min(bar_width);
    let progress = format!(
        "[{}{}]",
        "#".repeat(filled),
        "-".repeat(bar_width.saturating_sub(filled))
    );
    lines.push(Line::from(vec![Span::styled(
        progress,
        theme::checklist_done(),
    )]));

    for item in &snapshot.items {
        let (mark, style) = match item.status {
            ChecklistStatus::Completed => ("[x]", theme::checklist_done()),
            ChecklistStatus::InProgress => ("[>]", theme::checklist_in_progress()),
            ChecklistStatus::Pending => ("[ ]", theme::checklist_pending()),
        };
        let content = truncate_line(&item.content, 48);
        lines.push(Line::from(vec![
            Span::styled(format!("{mark} "), style),
            Span::styled(content, style),
        ]));
    }

    if lines.len() > height.max(4) {
        let skip = lines.len() - height.max(4);
        lines.drain(1..skip + 1);
    }
    lines
}

fn truncate_line(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let cut: String = text.chars().take(max.saturating_sub(1)).collect();
        format!("{cut}…")
    }
}
