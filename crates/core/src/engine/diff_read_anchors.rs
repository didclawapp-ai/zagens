//! Session anchors for differential `read_file` windows.
//!
//! Tracks the last edit/read citation window per path and by `tool_use_id`
//! so the model can request `around_last_edit` or `since_tool_use_id`.

use std::collections::HashMap;

use zagens_tools::EvidenceEnvelope;

use crate::engine::path_normalize::{normalize_repo_path, repo_paths_match};

/// Inclusive 1-based line window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineWindow {
    pub start_line: u64,
    pub end_line: u64,
}

impl LineWindow {
    #[must_use]
    pub fn new(start_line: u64, end_line: u64) -> Self {
        let start = start_line.max(1);
        let end = end_line.max(start);
        Self {
            start_line: start,
            end_line: end,
        }
    }

    /// Expand to a centered read window with the given radius.
    #[must_use]
    pub fn with_radius(self, radius: u64) -> Self {
        let mid = self.start_line.saturating_add(self.end_line) / 2;
        let start = mid.saturating_sub(radius).max(1);
        let end = mid.saturating_add(radius);
        Self {
            start_line: start,
            end_line: end,
        }
    }
}

/// In-session differential-read anchors (shared via `Arc<Mutex<_>>`).
#[derive(Debug, Default, Clone)]
pub struct DiffReadAnchors {
    /// Normalized relative path → last edit window.
    pub last_edit_by_path: HashMap<String, LineWindow>,
    /// Normalized relative path → last read/citation window.
    pub last_cite_by_path: HashMap<String, LineWindow>,
    /// tool_use_id → (path, window)
    pub by_tool_use: HashMap<String, (String, LineWindow)>,
}

impl DiffReadAnchors {
    #[must_use]
    pub fn normalize_path(path: &str) -> String {
        normalize_repo_path(path)
    }

    /// Record windows from a successful tool result's evidence.
    pub fn record_from_evidence(
        &mut self,
        tool_use_id: Option<&str>,
        tool_name: &str,
        envelope: &EvidenceEnvelope,
    ) {
        let is_edit = matches!(
            tool_name,
            "edit_file"
                | "write_file"
                | "apply_patch"
                | "edit_and_check"
                | "change_and_verify"
                | "batch_edit"
                | "fim_edit"
        );

        let edit_line_hint = envelope
            .facts
            .iter()
            .find(|f| f.key == "edit_line" || f.key == "start_line")
            .and_then(|f| f.value.parse::<u64>().ok());

        for cite in &envelope.citations {
            let path = Self::normalize_path(&cite.path);
            if path.is_empty() {
                continue;
            }
            let window = match (cite.start_line, cite.end_line) {
                (Some(s), Some(e)) => LineWindow::new(s, e),
                (Some(s), None) => LineWindow::new(s, s),
                (None, Some(e)) => LineWindow::new(e, e),
                (None, None) if is_edit => {
                    let line = edit_line_hint.unwrap_or(1);
                    LineWindow::new(line, line)
                }
                (None, None) => continue,
            };
            self.last_cite_by_path.insert(path.clone(), window);
            if is_edit {
                self.last_edit_by_path.insert(path.clone(), window);
            }
            if let Some(id) = tool_use_id.filter(|s| !s.is_empty()) {
                self.by_tool_use
                    .insert(id.to_string(), (path.clone(), window));
            }
        }

        // edit_file may only emit path fact without citations — use fact keys.
        if is_edit {
            let path_fact = envelope
                .facts
                .iter()
                .find(|f| f.key == "path")
                .map(|f| Self::normalize_path(&f.value));
            if let Some(path) = path_fact
                && !self
                    .last_edit_by_path
                    .keys()
                    .any(|k| repo_paths_match(k, &path))
            {
                let line = edit_line_hint.unwrap_or(1);
                let w = LineWindow::new(line, line);
                self.last_edit_by_path.insert(path.clone(), w);
                self.last_cite_by_path.insert(path, w);
            }
        }
    }

    #[must_use]
    pub fn window_for_last_edit(&self, path: &str) -> Option<LineWindow> {
        let key = Self::normalize_path(path);
        self.last_edit_by_path.get(&key).copied().or_else(|| {
            self.last_edit_by_path
                .iter()
                .find(|(p, _)| repo_paths_match(p, &key))
                .map(|(_, w)| *w)
        })
    }

    #[must_use]
    pub fn window_for_tool_use(&self, tool_use_id: &str) -> Option<(String, LineWindow)> {
        self.by_tool_use.get(tool_use_id).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zagens_tools::EvidenceCitation;

    #[test]
    fn records_edit_window() {
        let mut anchors = DiffReadAnchors::default();
        let env =
            EvidenceEnvelope::new().with_citation(EvidenceCitation::lines("src/lib.rs", 40, 45));
        anchors.record_from_evidence(Some("tu_1"), "edit_file", &env);
        assert_eq!(
            anchors.window_for_last_edit("src/lib.rs"),
            Some(LineWindow::new(40, 45))
        );
        assert_eq!(
            anchors.window_for_tool_use("tu_1").map(|(_, w)| w),
            Some(LineWindow::new(40, 45))
        );
    }

    #[test]
    fn last_edit_matches_verbatim_absolute_path() {
        let mut anchors = DiffReadAnchors::default();
        let env = EvidenceEnvelope::new().with_citation(EvidenceCitation::lines(
            "//?/F:/repo/crates/core/src/engine/agent_tool_phase.rs",
            6,
            6,
        ));
        anchors.record_from_evidence(None, "edit_file", &env);
        assert_eq!(
            anchors.window_for_last_edit("crates/core/src/engine/agent_tool_phase.rs"),
            Some(LineWindow::new(6, 6))
        );
    }
}
