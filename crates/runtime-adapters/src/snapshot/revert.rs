//! Pre-turn snapshot revert helpers (shared by `revert_turn` tool and HTTP API).

use std::io;

use super::repo::{Snapshot, SnapshotRepo};

/// Default offset: revert the most-recent turn.
pub const DEFAULT_REVERT_TURN_OFFSET: u64 = 1;
/// Hard cap so callers cannot roll back arbitrarily far.
pub const MAX_REVERT_TURN_OFFSET: u64 = 50;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevertPreTurnError {
    pub message: String,
}

impl RevertPreTurnError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for RevertPreTurnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for RevertPreTurnError {}

/// Collect `pre-turn:*` snapshots from the newest-first list (same order as `revert_turn`).
pub fn list_pre_turn_snapshots(repo: &SnapshotRepo) -> io::Result<Vec<Snapshot>> {
    let fetch = (MAX_REVERT_TURN_OFFSET as usize)
        .saturating_mul(2)
        .saturating_add(16);
    Ok(repo
        .list(fetch)?
        .into_iter()
        .filter(|s| s.label.starts_with("pre-turn:"))
        .collect())
}

/// Restore workspace to the `offset`-th newest `pre-turn:*` snapshot (`1` = latest).
pub fn revert_pre_turn_offset(
    repo: &SnapshotRepo,
    offset: u64,
) -> Result<Snapshot, RevertPreTurnError> {
    if offset == 0 || offset > MAX_REVERT_TURN_OFFSET {
        return Err(RevertPreTurnError::new(format!(
            "turn_offset must be between 1 and {MAX_REVERT_TURN_OFFSET}; got {offset}",
        )));
    }
    let pre_turns = list_pre_turn_snapshots(repo)
        .map_err(|e| RevertPreTurnError::new(format!("Snapshot list failed: {e}")))?;
    let target = pre_turns.get((offset - 1) as usize).ok_or_else(|| {
        RevertPreTurnError::new(format!(
            "Only {} pre-turn snapshot(s) exist; turn_offset={offset} is out of range.",
            pre_turns.len(),
        ))
    })?;
    repo.restore(&target.id)
        .map_err(|e| RevertPreTurnError::new(format!("Restore failed: {e}")))?;
    Ok(target.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn revert_offset_one_restores_latest_pre_turn() {
        let tmp = tempdir().unwrap();
        let workspace = tmp.path().join("ws");
        std::fs::create_dir_all(&workspace).unwrap();
        let repo = SnapshotRepo::open_or_init(&workspace).unwrap();
        std::fs::write(workspace.join("a.txt"), b"original").unwrap();
        repo.snapshot("pre-turn:1").unwrap();
        std::fs::write(workspace.join("a.txt"), b"modified").unwrap();
        repo.snapshot("post-turn:1").unwrap();

        let snap = revert_pre_turn_offset(&repo, 1).unwrap();
        assert_eq!(snap.label, "pre-turn:1");
        let content = std::fs::read_to_string(workspace.join("a.txt")).unwrap();
        assert_eq!(content, "original");
    }
}
