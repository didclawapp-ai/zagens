//! Turn context and tracking.
//!
//! A "turn" is one user message and the resulting AI response,
//! including any tool calls that occur.
//!
//! ## Snapshot lifecycle hooks
//!
//! [`pre_turn_snapshot`] and [`post_turn_snapshot`] book-end a turn by
//! taking a workspace-level snapshot into a side git repo (see
//! `crate::snapshot`). They are intentionally non-blocking and
//! non-fatal: any IO error is logged at WARN and swallowed so a busted
//! filesystem or missing `git` binary never derails the agent loop.
//! `/restore N` and the `revert_turn` tool both consume these
//! snapshots.

pub use deepseek_core::turn::{TurnContext, TurnToolCall};

use crate::snapshot::SnapshotRepo;
use std::path::Path;

/// Take a `pre-turn:<seq>` workspace snapshot.
///
/// Returns the snapshot SHA on success, `None` on any error. Errors are
/// logged at WARN; the turn loop must not block on this.
pub fn pre_turn_snapshot(workspace: &Path, turn_seq: u64, max_workspace_gb: f64) -> Option<String> {
    snapshot_with_label(workspace, &format!("pre-turn:{turn_seq}"), max_workspace_gb)
}

/// Take a `tool:<call_id>` workspace snapshot, taken before executing a
/// file-modifying tool call (write_file, edit_file, apply_patch).
///
/// This enables surgical undo: `/undo` can restore to the most recent
/// `tool:<call_id>` snapshot to revert just the last file write.
///
/// Returns the snapshot SHA on success, `None` on any error. Errors are
/// logged at WARN and are non-fatal.
pub fn pre_tool_snapshot(workspace: &Path, call_id: &str, max_workspace_gb: f64) -> Option<String> {
    snapshot_with_label(workspace, &format!("tool:{call_id}"), max_workspace_gb)
}

/// Take a `post-turn:<seq>` workspace snapshot. Same failure model as
/// [`pre_turn_snapshot`].
pub fn post_turn_snapshot(
    workspace: &Path,
    turn_seq: u64,
    max_workspace_gb: f64,
) -> Option<String> {
    snapshot_with_label(
        workspace,
        &format!("post-turn:{turn_seq}"),
        max_workspace_gb,
    )
}

fn snapshot_with_label(workspace: &Path, label: &str, max_workspace_gb: f64) -> Option<String> {
    match SnapshotRepo::open_or_init_with_max_gb(workspace, max_workspace_gb) {
        Ok(repo) => match repo.snapshot(label) {
            Ok(id) => Some(id.0),
            Err(e) => {
                tracing::warn!(target: "snapshot", "snapshot '{label}' failed: {e}");
                None
            }
        },
        Err(e) => {
            tracing::warn!(target: "snapshot", "snapshot repo init failed: {e}");
            None
        }
    }
}
