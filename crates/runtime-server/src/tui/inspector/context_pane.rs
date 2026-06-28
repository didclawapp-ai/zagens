//! Context Explorer breakdown panel (P2-6 TUI).

use ratatui::text::{Line, Span};

use super::super::display_format::truncate_display_width;
use super::super::theme::{self, TuiPanel};
use zagens_core::engine::{ContextCategory, ContextNextAction, ContextUsageBreakdown};

const INSPECTOR: TuiPanel = TuiPanel::Inspector;

#[must_use]
pub fn format_token_count(tokens: u32) -> String {
    if tokens >= 1_000_000 {
        format!("{:.2}M", f64::from(tokens) / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{:.1}K", f64::from(tokens) / 1_000.0)
    } else {
        tokens.to_string()
    }
}

#[must_use]
pub fn format_next_action(action: ContextNextAction) -> &'static str {
    match action {
        ContextNextAction::None => "none",
        ContextNextAction::SeamL1 => "seam L1",
        ContextNextAction::SeamL2 => "seam L2",
        ContextNextAction::SeamL3 => "seam L3",
        ContextNextAction::Cycle => "cycle",
        ContextNextAction::CompactSuggested => "compact",
        ContextNextAction::Overflow => "overflow",
    }
}

fn mini_bar(tokens: u32, window: u32, width: usize) -> String {
    if window == 0 || width == 0 {
        return String::new();
    }
    let filled = ((f64::from(tokens) / f64::from(window)) * width as f64)
        .round()
        .clamp(0.0, width as f64) as usize;
    format!(
        "[{}{}]",
        "#".repeat(filled),
        "-".repeat(width.saturating_sub(filled))
    )
}

fn category_lines(
    category: &ContextCategory,
    window: u32,
    max_cols: usize,
    indent: usize,
) -> Vec<Line<'static>> {
    let prefix = " ".repeat(indent);
    let bar_w = 8usize;
    let bar = mini_bar(category.tokens, window, bar_w);
    let count_suffix = category
        .item_count
        .map(|n| format!(" · {n} msgs"))
        .unwrap_or_default();
    let head = format!(
        "{prefix}{}{} {}{}",
        category.label,
        count_suffix,
        format_token_count(category.tokens),
        if bar.is_empty() {
            String::new()
        } else {
            format!(" {bar}")
        }
    );
    let mut lines = vec![Line::from(Span::styled(
        truncate_display_width(&head, max_cols),
        theme::panel(INSPECTOR).item(false),
    ))];
    if let Some(hint) = category.user_action_hint.as_ref() {
        let hint_line = format!("{prefix}  {hint}");
        lines.push(Line::from(Span::styled(
            truncate_display_width(&hint_line, max_cols),
            theme::panel(INSPECTOR).hint(),
        )));
    }
    if let Some(children) = category.children.as_ref() {
        for child in children {
            lines.extend(category_lines(child, window, max_cols, indent + 2));
        }
    }
    lines
}

pub fn line_count(breakdown: Option<&ContextUsageBreakdown>) -> usize {
    let Some(breakdown) = breakdown else {
        return 1;
    };
    let mut n = 4; // header, window line, profile/next, spacer
    for category in &breakdown.categories {
        if category.tokens == 0 {
            continue;
        }
        n += category_lines(category, breakdown.context_window_tokens, 80, 0).len();
    }
    n.max(1)
}

pub fn render_styled_panel(
    breakdown: Option<&ContextUsageBreakdown>,
    height: usize,
    scroll: usize,
    max_cols: usize,
) -> Vec<Line<'static>> {
    let max_cols = max_cols.max(8);
    let Some(breakdown) = breakdown else {
        return vec![Line::from(Span::styled(
            "No context breakdown.",
            theme::panel(INSPECTOR).hint(),
        ))];
    };

    let mut lines = Vec::new();
    let pct = breakdown.usage_percent.round().clamp(0.0, 100.0) as u32;
    let model = truncate_display_width(&breakdown.model, max_cols.saturating_sub(8));
    lines.push(Line::from(Span::styled(
        truncate_display_width(
            &format!(
                "{model} · {}% · {}",
                pct,
                format_token_count(breakdown.estimated_input_tokens)
            ),
            max_cols,
        ),
        theme::panel(INSPECTOR).heading(),
    )));
    lines.push(Line::from(Span::styled(
        truncate_display_width(
            &format!(
                "window {} · profile {}",
                format_token_count(breakdown.context_window_tokens),
                breakdown.profile.to_ascii_uppercase()
            ),
            max_cols,
        ),
        theme::panel(INSPECTOR).item(false),
    )));
    let next = format_next_action(breakdown.next_action);
    lines.push(Line::from(Span::styled(
        truncate_display_width(&format!("next: {next}"), max_cols),
        theme::panel(INSPECTOR).hint(),
    )));
    lines.push(Line::from(Span::raw("")));

    for category in breakdown.categories.iter().filter(|c| c.tokens > 0) {
        lines.extend(category_lines(
            category,
            breakdown.context_window_tokens,
            max_cols,
            0,
        ));
    }

    if lines.len() <= 4 {
        lines.push(Line::from(Span::styled(
            "(no category data)",
            theme::panel(INSPECTOR).hint(),
        )));
    }

    let visible = height.max(4);
    let max_scroll = lines.len().saturating_sub(visible);
    let start = scroll.min(max_scroll);
    lines.into_iter().skip(start).take(visible).collect()
}

#[cfg(test)]
mod tests {
    use super::super::super::display_format::display_width;
    use super::*;
    use zagens_core::engine::ContextCategory;

    #[test]
    fn token_count_formats_k_and_m() {
        assert_eq!(format_token_count(500), "500");
        assert_eq!(format_token_count(1500), "1.5K");
        assert_eq!(format_token_count(1_500_000), "1.50M");
    }

    #[test]
    fn line_count_includes_turn_children() {
        let breakdown = ContextUsageBreakdown {
            model: "deepseek-v4-pro".into(),
            context_window_tokens: 1_000_000,
            estimated_input_tokens: 1000,
            usage_percent: 0.1,
            profile: "large".into(),
            next_action: ContextNextAction::None,
            categories: vec![ContextCategory {
                id: "conversation".into(),
                label: "Conversation".into(),
                tokens: 1000,
                item_count: Some(3),
                children: Some(vec![ContextCategory {
                    id: "conversation.turn.1".into(),
                    label: "Turn 1".into(),
                    tokens: 600,
                    item_count: Some(2),
                    children: None,
                    user_action_hint: None,
                }]),
                user_action_hint: None,
            }],
        };
        assert!(line_count(Some(&breakdown)) >= 5);
    }

    #[test]
    fn render_empty_breakdown_placeholder() {
        let lines = render_styled_panel(None, 8, 0, 40);
        assert_eq!(lines.len(), 1);
        assert!(display_width(&lines[0].spans[0].content) > 0);
    }
}
