//! Right-rail inspector panels (Phase 2).

mod activity;
mod agents;
mod diff;
mod files;
mod lht_pane;
mod mcp;
mod panel;
mod preview;
pub(crate) mod syntax;

use std::path::Path;

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::config::Config;

use super::display_format::{display_width, truncate_display_width};
use super::layout::InspectorTab;
use super::theme::{self, TuiPanel};

const INSPECTOR: TuiPanel = TuiPanel::Inspector;

pub use activity::{
    line_count as activity_line_count, render_styled_panel as render_activity_styled,
};
pub use agents::{AgentEntry, line_count as agents_line_count};
pub use diff::{DiffPanelState, git_diff_patch, load_diff_panel};
pub use files::FileTreeState;
pub use lht_pane::{line_count as lht_line_count, render_styled_panel as render_lht_styled};
pub use mcp::{McpPanelState, load_mcp_panel};
pub use panel::{InspectorInteraction, LhtPaneUi};
pub use preview::read_text_preview;

#[derive(Debug, Clone, Default)]
pub struct InspectorCache {
    pub file_tree: FileTreeState,
    pub diff: DiffPanelState,
    pub mcp: McpPanelState,
    pub agents: Vec<AgentEntry>,
}

impl InspectorCache {
    pub fn refresh_files_diff(&mut self, workspace: &Path, config: &Config, diff_staged: bool) {
        self.file_tree.rescan(workspace);
        self.diff = load_diff_panel(workspace, diff_staged);
        let _ = config;
    }

    pub fn refresh_diff_only(&mut self, workspace: &Path, diff_staged: bool) {
        self.diff = load_diff_panel(workspace, diff_staged);
    }

    pub fn scrollable_line_count(
        &self,
        tab: InspectorTab,
        ui: &InspectorInteraction,
        activity_events: &[String],
    ) -> usize {
        match tab {
            InspectorTab::Files => {
                if ui.file_preview_rel.is_some() {
                    return 1;
                }
                self.file_tree.line_count()
            }
            InspectorTab::Diff => {
                if ui.diff_detail_path.is_some() {
                    return 1;
                }
                self.diff.line_count()
            }
            InspectorTab::Agents => agents::line_count(&self.agents),
            InspectorTab::Mcp => self.mcp.line_count(ui.mcp_expanded.as_deref()),
            InspectorTab::Activity => activity_line_count(activity_events),
        }
    }

    pub fn render_styled(
        &self,
        tab: InspectorTab,
        height: usize,
        max_cols: usize,
        ui: &InspectorInteraction,
        workspace: &Path,
        activity_events: &[String],
    ) -> Vec<Line<'static>> {
        let max_cols = max_cols.max(8);
        if tab == InspectorTab::Files {
            if let Some(body) = ui.file_preview_body.as_ref() {
                let rel = ui.file_preview_rel.as_deref().unwrap_or("");
                return clip_sidebar_lines(
                    render_detail_lines_highlighted(
                        &format!("preview: {rel} | Esc back"),
                        body.clone(),
                        height,
                        ui.scroll,
                        rel,
                    ),
                    max_cols,
                );
            }
            if let Some(rel) = ui.file_preview_rel.as_deref() {
                return clip_sidebar_lines(
                    render_detail_lines_highlighted(
                        &format!("preview: {rel} | Esc back"),
                        read_text_preview(workspace, rel),
                        height,
                        ui.scroll,
                        rel,
                    ),
                    max_cols,
                );
            }
            return clip_sidebar_lines(
                self.file_tree
                    .render_lines(ui.file_cursor, ui.scroll, height, max_cols)
                    .into_iter()
                    .map(|(text, selected)| {
                        Line::from(Span::styled(
                            text,
                            if selected {
                                theme::panel(INSPECTOR).item(true)
                            } else {
                                theme::panel(INSPECTOR).item(false)
                            },
                        ))
                    })
                    .collect(),
                max_cols,
            );
        }

        if tab == InspectorTab::Diff {
            if let Some(body) = ui.diff_detail_body.as_ref() {
                let path = ui.diff_detail_path.as_deref().unwrap_or("");
                return clip_sidebar_lines(
                    render_detail_lines(
                        &format!("diff: {path} | Esc back"),
                        body.clone(),
                        height,
                        ui.scroll,
                    ),
                    max_cols,
                );
            }
            return clip_sidebar_lines(
                render_diff_list(&self.diff, height, ui, max_cols),
                max_cols,
            );
        }

        if tab == InspectorTab::Agents {
            return clip_sidebar_lines(
                agents::render_styled_panel(
                    &self.agents,
                    height,
                    ui.scroll,
                    ui.agents_cursor,
                    ui.agents_expanded,
                    max_cols,
                ),
                max_cols,
            );
        }

        if tab == InspectorTab::Mcp {
            return clip_sidebar_lines(render_mcp_list(&self.mcp, height, ui, max_cols), max_cols);
        }

        if tab == InspectorTab::Activity {
            return clip_sidebar_lines(
                render_activity_styled(activity_events, height, ui.scroll, max_cols),
                max_cols,
            );
        }

        Vec::new()
    }

    pub fn refresh_static(&mut self, workspace: &Path, config: &Config) {
        self.refresh_files_diff(workspace, config, false);
        self.mcp = load_mcp_panel(config);
    }
}

fn render_diff_list(
    panel: &DiffPanelState,
    height: usize,
    ui: &InspectorInteraction,
    max_cols: usize,
) -> Vec<Line<'static>> {
    let visible = height.max(4);
    let path_budget = max_cols.saturating_sub(14).max(8);
    let mut lines: Vec<Line<'static>> = vec![Line::from(Span::styled(
        sidebar_plain(&panel.header, max_cols),
        theme::panel(INSPECTOR).hint(),
    ))];
    if let Some(err) = &panel.error {
        lines.push(Line::from(Span::styled(
            sidebar_plain(err, max_cols),
            theme::panel(INSPECTOR).item(false),
        )));
        return clip_lines(lines, visible, ui.scroll);
    }
    if panel.entries.is_empty() {
        lines.push(Line::from(Span::styled(
            "(clean)",
            theme::panel(INSPECTOR).hint(),
        )));
        return clip_lines(lines, visible, ui.scroll);
    }
    for (idx, entry) in panel.entries.iter().enumerate() {
        let mark = if idx == ui.diff_cursor { ">" } else { " " };
        let path = sidebar_plain(&entry.path, path_budget);
        lines.push(Line::from(Span::styled(
            format!("{mark} {:<8} {path}", entry.summary),
            if idx == ui.diff_cursor {
                theme::panel(INSPECTOR).item(true)
            } else {
                theme::panel(INSPECTOR).item(false)
            },
        )));
    }
    clip_lines(lines, visible, ui.scroll)
}

fn render_mcp_list(
    panel: &McpPanelState,
    height: usize,
    ui: &InspectorInteraction,
    max_cols: usize,
) -> Vec<Line<'static>> {
    let visible = height.max(4);
    let mut lines: Vec<Line<'static>> = vec![Line::from(Span::styled(
        sidebar_plain(&panel.header, max_cols),
        theme::panel(INSPECTOR).hint(),
    ))];
    if panel.servers.is_empty() {
        lines.push(Line::from(Span::styled(
            "(no servers configured)",
            theme::panel(INSPECTOR).hint(),
        )));
        return clip_lines(lines, visible, ui.scroll);
    }
    for (idx, server) in panel.servers.iter().enumerate() {
        let expanded = ui.mcp_expanded.as_deref() == Some(server.name.as_str());
        let mark = if idx == ui.mcp_cursor { ">" } else { " " };
        let chevron = if expanded { "v" } else { ">" };
        lines.push(Line::from(Span::styled(
            sidebar_plain(
                &format!(
                    "{mark} {chevron} {} ({}, tools:{})",
                    server.name,
                    server.transport,
                    server.tools.len()
                ),
                max_cols,
            ),
            if idx == ui.mcp_cursor {
                theme::panel(INSPECTOR).item(true)
            } else {
                theme::panel(INSPECTOR).item(false)
            },
        )));
        if expanded {
            if server.tools.is_empty() {
                lines.push(Line::from(Span::styled(
                    sidebar_plain("    (no tools)", max_cols),
                    theme::panel(INSPECTOR).hint(),
                )));
            } else {
                for tool in &server.tools {
                    lines.push(Line::from(Span::styled(
                        sidebar_plain(&format!("    · {tool}"), max_cols),
                        theme::panel(INSPECTOR).item(false),
                    )));
                }
            }
        }
    }
    clip_lines(lines, visible, ui.scroll)
}

fn render_detail_lines(
    title: &str,
    body: Vec<String>,
    height: usize,
    scroll: usize,
) -> Vec<Line<'static>> {
    render_detail_lines_inner(title, body, height, scroll, None)
}

fn render_detail_lines_highlighted(
    title: &str,
    body: Vec<String>,
    height: usize,
    scroll: usize,
    path: &str,
) -> Vec<Line<'static>> {
    render_detail_lines_inner(title, body, height, scroll, Some(path))
}

fn render_detail_lines_inner(
    title: &str,
    body: Vec<String>,
    height: usize,
    scroll: usize,
    highlight_path: Option<&str>,
) -> Vec<Line<'static>> {
    let visible = height.max(4);
    let mut lines = vec![Line::from(Span::styled(
        title.to_string(),
        Style::default()
            .fg(theme::footer_lht())
            .add_modifier(Modifier::BOLD),
    ))];
    let lang = highlight_path
        .map(syntax::Lang::from_path)
        .unwrap_or(syntax::Lang::Plain);
    let base_style = theme::panel(INSPECTOR).item(false);
    let gutter_style = base_style.fg(ratatui::style::Color::Rgb(0x44, 0x47, 0x5a)); // dim

    // Width of line-number column: number of digits in total line count + separator " │ "
    let num_width = body.len().max(1).ilog10() as usize + 1;
    let gutter_total = num_width + 2; // e.g. "42 " (num + space + separator char)

    let show_gutter = !matches!(lang, syntax::Lang::Plain);

    for (idx, line) in body.into_iter().enumerate() {
        let lineno = idx + 1;
        let mut spans: Vec<Span<'static>> = Vec::new();

        if show_gutter {
            let num_str = format!("{lineno:>num_width$}");
            spans.push(Span::styled(num_str, gutter_style));
            spans.push(Span::styled(" \u{2502}".to_string(), gutter_style)); // " │"
            spans.push(Span::styled(" ".to_string(), base_style));
            let _ = gutter_total; // suppress unused warning
        }

        let code_spans = if matches!(lang, syntax::Lang::Plain) {
            vec![Span::styled(line, base_style)]
        } else {
            syntax::highlight_line(&line, lang)
                .into_iter()
                .map(|s| {
                    let patched = base_style.patch(s.style);
                    Span::styled(s.content, patched)
                })
                .collect()
        };
        spans.extend(code_spans);
        lines.push(Line::from(spans));
    }
    clip_lines(lines, visible, scroll)
}

fn sidebar_plain(text: &str, max_cols: usize) -> String {
    if display_width(text) <= max_cols {
        text.to_string()
    } else {
        truncate_display_width(text, max_cols)
    }
}

fn clip_sidebar_lines(lines: Vec<Line<'static>>, max_cols: usize) -> Vec<Line<'static>> {
    lines
        .into_iter()
        .map(|line| clip_sidebar_line(line, max_cols))
        .collect()
}

fn clip_sidebar_line(line: Line<'static>, max_cols: usize) -> Line<'static> {
    if line.spans.is_empty() {
        return line;
    }
    // Fast path: total width fits.
    let total_w: usize = line
        .spans
        .iter()
        .map(|s| display_width(s.content.as_ref()))
        .sum();
    if total_w <= max_cols {
        return line;
    }
    if line.spans.len() == 1 {
        let span = &line.spans[0];
        return Line::from(Span::styled(
            truncate_display_width(span.content.as_ref(), max_cols),
            span.style,
        ));
    }
    // Multi-span: accumulate spans until we reach max_cols, then clip the last
    // partial span.  This preserves each span's individual style (gutter stays
    // dim, code content keeps its highlight colour — avoids the old bug where
    // the whole line was painted with the dim gutter colour).
    let mut result: Vec<Span<'static>> = Vec::with_capacity(line.spans.len());
    let mut remaining = max_cols;
    for span in line.spans {
        if remaining == 0 {
            break;
        }
        let w = display_width(span.content.as_ref());
        if w <= remaining {
            remaining -= w;
            result.push(span);
        } else {
            let clipped = truncate_display_width(span.content.as_ref(), remaining);
            result.push(Span::styled(clipped, span.style));
            remaining = 0;
        }
    }
    Line::from(result)
}

fn clip_lines(lines: Vec<Line<'static>>, max: usize, scroll: usize) -> Vec<Line<'static>> {
    if lines.is_empty() {
        return lines;
    }
    let max_scroll = lines.len().saturating_sub(max);
    let start = scroll.min(max_scroll);
    lines.into_iter().skip(start).take(max).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_list_shows_clean_when_empty() {
        let panel = DiffPanelState {
            header: "h".to_string(),
            entries: Vec::new(),
            error: None,
        };
        let ui = InspectorInteraction::default();
        let lines = render_diff_list(&panel, 8, &ui, 40);
        assert!(
            lines
                .iter()
                .any(|l| l.spans.iter().any(|s| s.content.contains("(clean)")))
        );
    }
}
