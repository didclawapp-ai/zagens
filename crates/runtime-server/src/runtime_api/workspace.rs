//! Workspace git status / changes / file-diff / pulls (Diff thin layer, P4.5).

use std::path::{Path, PathBuf};

use axum::Json;
use axum::extract::{Query, State};
use serde::{Deserialize, Serialize};

use crate::gh_read::list_pulls;
use crate::git_read::{GitChangeKind, collect_status, file_diff, validate_rel_path};
use crate::git_snapshot_cache::load_snapshot;

/// Re-export for LHT / other callers that historically imported from this module.
pub(crate) use crate::git_read::run_git;

use super::{ApiError, RuntimeApiState};

#[derive(Debug, Deserialize)]
pub(crate) struct WorkspaceRootQuery {
    /// Optional absolute workspace root (thread / worktree). Falls back to sidecar default.
    #[serde(default)]
    workspace: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct WorkspaceFileDiffQuery {
    #[serde(default)]
    workspace: Option<String>,
    path: String,
    #[serde(default)]
    staged: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct WorkspacePullsQuery {
    #[serde(default)]
    workspace: Option<String>,
    /// open | closed | merged | all — default open
    #[serde(default)]
    state: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct WorkspaceStatusResponse {
    workspace: PathBuf,
    git_repo: bool,
    branch: Option<String>,
    staged: usize,
    unstaged: usize,
    untracked: usize,
    ahead: Option<u32>,
    behind: Option<u32>,
}

#[derive(Debug, Serialize)]
pub(crate) struct WorkspaceChangeEntry {
    path: String,
    index_status: String,
    worktree_status: String,
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    old_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct WorkspaceChangesResponse {
    workspace: PathBuf,
    git_repo: bool,
    truncated: bool,
    changes: Vec<WorkspaceChangeEntry>,
}

/// Status + changes in one response (Diff panel single round-trip).
#[derive(Debug, Serialize)]
pub(crate) struct WorkspaceSnapshotResponse {
    workspace: PathBuf,
    git_repo: bool,
    branch: Option<String>,
    staged: usize,
    unstaged: usize,
    untracked: usize,
    ahead: Option<u32>,
    behind: Option<u32>,
    truncated: bool,
    changes: Vec<WorkspaceChangeEntry>,
}

#[derive(Debug, Serialize)]
pub(crate) struct WorkspaceFileDiffResponse {
    workspace: PathBuf,
    path: String,
    staged: bool,
    diff_text: String,
    truncated: bool,
    binary: bool,
    untracked: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct WorkspacePullEntry {
    number: u64,
    title: String,
    url: String,
    head_ref_name: String,
    base_ref_name: String,
    is_draft: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated_at: Option<String>,
    /// success | pending | failure | neutral | unknown
    checks: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct WorkspacePullsResponse {
    workspace: PathBuf,
    state: String,
    pulls: Vec<WorkspacePullEntry>,
    /// Soft error: gh_missing | gh_auth | not_git_repo | gh_failed
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_message: Option<String>,
}

pub(crate) async fn workspace_status(
    State(state): State<RuntimeApiState>,
    Query(q): Query<WorkspaceRootQuery>,
) -> Result<Json<WorkspaceStatusResponse>, ApiError> {
    let root = resolve_workspace_root(&state, q.workspace.as_deref())?;
    let snap = load_snapshot(root.clone())
        .await
        .map_err(ApiError::internal)?;
    let status = &snap.status;
    Ok(Json(WorkspaceStatusResponse {
        workspace: root,
        git_repo: status.git_repo,
        branch: status.branch.clone(),
        staged: status.staged,
        unstaged: status.unstaged,
        untracked: status.untracked,
        ahead: status.ahead,
        behind: status.behind,
    }))
}

pub(crate) async fn workspace_changes(
    State(state): State<RuntimeApiState>,
    Query(q): Query<WorkspaceRootQuery>,
) -> Result<Json<WorkspaceChangesResponse>, ApiError> {
    let root = resolve_workspace_root(&state, q.workspace.as_deref())?;
    let snap = load_snapshot(root.clone())
        .await
        .map_err(ApiError::internal)?;

    Ok(Json(WorkspaceChangesResponse {
        workspace: root,
        git_repo: snap.status.git_repo,
        truncated: snap.truncated,
        changes: map_changes(&snap.changes),
    }))
}

pub(crate) async fn workspace_snapshot(
    State(state): State<RuntimeApiState>,
    Query(q): Query<WorkspaceRootQuery>,
) -> Result<Json<WorkspaceSnapshotResponse>, ApiError> {
    let root = resolve_workspace_root(&state, q.workspace.as_deref())?;
    let snap = load_snapshot(root.clone())
        .await
        .map_err(ApiError::internal)?;
    let status = &snap.status;
    Ok(Json(WorkspaceSnapshotResponse {
        workspace: root,
        git_repo: status.git_repo,
        branch: status.branch.clone(),
        staged: status.staged,
        unstaged: status.unstaged,
        untracked: status.untracked,
        ahead: status.ahead,
        behind: status.behind,
        truncated: snap.truncated,
        changes: map_changes(&snap.changes),
    }))
}

fn map_changes(changes: &[crate::git_read::GitChangeEntry]) -> Vec<WorkspaceChangeEntry> {
    changes
        .iter()
        .map(|c| WorkspaceChangeEntry {
            path: c.path.clone(),
            index_status: c.index_status.to_string(),
            worktree_status: c.worktree_status.to_string(),
            kind: c.kind.as_str().to_string(),
            old_path: c.old_path.clone(),
        })
        .collect()
}

pub(crate) async fn workspace_file_diff(
    State(state): State<RuntimeApiState>,
    Query(q): Query<WorkspaceFileDiffQuery>,
) -> Result<Json<WorkspaceFileDiffResponse>, ApiError> {
    let root = resolve_workspace_root(&state, q.workspace.as_deref())?;
    let path = validate_rel_path(&root, &q.path).map_err(ApiError::bad_request)?;
    let staged = q.staged;
    let root_clone = root.clone();
    let path_clone = path.clone();
    let diff = tokio::task::spawn_blocking(move || file_diff(&root_clone, &path_clone, staged))
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?
        .map_err(ApiError::bad_request)?;

    Ok(Json(WorkspaceFileDiffResponse {
        workspace: root,
        path: diff.path,
        staged: diff.staged,
        diff_text: diff.diff_text,
        truncated: diff.truncated,
        binary: diff.binary,
        untracked: diff.untracked,
    }))
}

/// Read-only PR list via async `gh` (does not occupy spawn_blocking). Soft-fails HTTP 200.
pub(crate) async fn workspace_pulls(
    State(state): State<RuntimeApiState>,
    Query(q): Query<WorkspacePullsQuery>,
) -> Result<Json<WorkspacePullsResponse>, ApiError> {
    let root = resolve_workspace_root(&state, q.workspace.as_deref())?;
    let state_s = q
        .state
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("open")
        .to_string();
    let root_clone = root.clone();
    let state_clone = state_s.clone();
    // Async process + kill_on_drop: timeout cancels without blocking the thread pool.
    let result = match tokio::time::timeout(
        std::time::Duration::from_secs(10),
        list_pulls(&root_clone, &state_clone),
    )
    .await
    {
        Ok(inner) => inner,
        Err(_) => {
            return Ok(Json(WorkspacePullsResponse {
                workspace: root,
                state: state_s,
                pulls: Vec::new(),
                error: Some("gh_failed".into()),
                error_message: Some("gh pr list timed out (10s)".into()),
            }));
        }
    };

    match result {
        Ok(pulls) => Ok(Json(WorkspacePullsResponse {
            workspace: root,
            state: state_s,
            pulls: pulls
                .into_iter()
                .map(|p| WorkspacePullEntry {
                    number: p.number,
                    title: p.title,
                    url: p.url,
                    head_ref_name: p.head_ref_name,
                    base_ref_name: p.base_ref_name,
                    is_draft: p.is_draft,
                    updated_at: p.updated_at,
                    checks: p.checks,
                })
                .collect(),
            error: None,
            error_message: None,
        })),
        Err(e) => Ok(Json(WorkspacePullsResponse {
            workspace: root,
            state: state_s,
            pulls: Vec::new(),
            error: Some(e.code().to_string()),
            error_message: Some(e.message()),
        })),
    }
}

fn resolve_workspace_root(
    state: &RuntimeApiState,
    workspace_q: Option<&str>,
) -> Result<PathBuf, ApiError> {
    let raw = workspace_q.map(str::trim).filter(|s| !s.is_empty());
    let path = match raw {
        Some(ws) => PathBuf::from(ws),
        None => state.workspace.clone(),
    };
    let base = path
        .canonicalize()
        .map_err(|e| ApiError::bad_request(format!("workspace: {e}")))?;
    if !base.is_dir() {
        return Err(ApiError::bad_request("workspace is not a directory"));
    }
    Ok(base)
}

/// Re-export for tests / TUI bridging.
#[allow(dead_code)]
pub(crate) fn collect_workspace_status(workspace: &Path) -> WorkspaceStatusResponse {
    let status = collect_status(workspace);
    WorkspaceStatusResponse {
        workspace: workspace.to_path_buf(),
        git_repo: status.git_repo,
        branch: status.branch,
        staged: status.staged,
        unstaged: status.unstaged,
        untracked: status.untracked,
        ahead: status.ahead,
        behind: status.behind,
    }
}

#[allow(dead_code)]
pub(crate) fn change_kind_label(kind: GitChangeKind) -> &'static str {
    kind.as_str()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git_read::parse_porcelain;

    #[test]
    fn porcelain_conflict_kind() {
        let entries = parse_porcelain("UU conflict.rs\n", 10);
        assert_eq!(entries[0].kind, GitChangeKind::Conflict);
    }

    #[test]
    fn resolve_falls_back_without_query() {
        // smoke: validate_rel_path used by file-diff
        let dir = tempfile::tempdir().unwrap();
        let p = validate_rel_path(dir.path(), "a/b.rs").unwrap();
        assert_eq!(p, "a/b.rs");
    }
}
