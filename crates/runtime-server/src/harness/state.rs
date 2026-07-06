//! H3 state adapter — read-only paths for queue / handoff / checkpoint consumption.

use std::path::{Path, PathBuf};

use zagens_config::workspace_meta_file_write;

/// Stable read-only paths for harness orchestration state (Phase 1–2 consumers).
pub trait HarnessStateAdapter {
    fn workspace(&self) -> &Path;
    fn handoff_md(&self) -> PathBuf;
    fn night_queue_json(&self) -> PathBuf;
    fn queue_events_jsonl(&self) -> PathBuf;
}

/// Workspace-scoped harness state paths under `.zagens/`.
#[derive(Debug, Clone)]
pub struct WorkspaceHarnessState {
    workspace: PathBuf,
}

impl WorkspaceHarnessState {
    #[must_use]
    pub fn new(workspace: impl Into<PathBuf>) -> Self {
        Self {
            workspace: workspace.into(),
        }
    }
}

impl HarnessStateAdapter for WorkspaceHarnessState {
    fn workspace(&self) -> &Path {
        &self.workspace
    }

    fn handoff_md(&self) -> PathBuf {
        workspace_meta_file_write(&self.workspace, "handoff.md")
    }

    fn night_queue_json(&self) -> PathBuf {
        workspace_meta_file_write(&self.workspace, crate::night_queue::QUEUE_FILE)
    }

    fn queue_events_jsonl(&self) -> PathBuf {
        workspace_meta_file_write(&self.workspace, "queue_events.jsonl")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_paths_under_meta_dir() {
        let state = WorkspaceHarnessState::new("/tmp/ws");
        let handoff = state.handoff_md();
        assert!(handoff.to_string_lossy().contains("handoff.md"));
        assert!(
            state
                .night_queue_json()
                .to_string_lossy()
                .contains("night_queue.json")
        );
        assert!(
            state
                .queue_events_jsonl()
                .to_string_lossy()
                .contains("queue_events.jsonl")
        );
    }
}
