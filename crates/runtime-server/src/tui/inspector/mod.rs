//! Right-rail inspector panels (Phase 2).

mod agents;
mod checklist;
mod diff;
mod files;
mod mcp;

use std::path::Path;

use crate::config::Config;

use super::harness::ChecklistSnapshot;
use super::layout::InspectorTab;

pub use agents::AgentEntry;

#[derive(Debug, Clone, Default)]
pub struct InspectorCache {
    pub files: Vec<String>,
    pub diff: Vec<String>,
    pub checklist: Option<ChecklistSnapshot>,
    pub agents: Vec<AgentEntry>,
    pub mcp: Vec<String>,
}

impl InspectorCache {
    pub fn render(&self, tab: InspectorTab, height: usize) -> Vec<String> {
        let lines = match tab {
            InspectorTab::Files => &self.files,
            InspectorTab::Diff => &self.diff,
            InspectorTab::Checklist => {
                return checklist::render_panel(self.checklist.as_ref(), height);
            }
            InspectorTab::Agents => {
                return agents::render_panel(&self.agents, height);
            }
            InspectorTab::Mcp => &self.mcp,
        };
        clip_lines(lines, height.max(4))
    }

    pub fn refresh_static(&mut self, workspace: &Path, config: &Config) {
        self.files = files::list_workspace(workspace, 3);
        self.diff = diff::git_diff_stat(workspace);
        self.mcp = mcp::list_servers(config);
    }
}

fn clip_lines(lines: &[String], max: usize) -> Vec<String> {
    if lines.len() <= max {
        lines.to_vec()
    } else {
        lines[lines.len() - max..].to_vec()
    }
}
