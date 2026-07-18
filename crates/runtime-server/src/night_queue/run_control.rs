//! In-process cancel tokens for an active night-queue batch run.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use tokio_util::sync::CancellationToken;

fn registry() -> &'static Mutex<HashMap<PathBuf, CancellationToken>> {
    static REG: OnceLock<Mutex<HashMap<PathBuf, CancellationToken>>> = OnceLock::new();
    REG.get_or_init(|| Mutex::new(HashMap::new()))
}

fn workspace_key(workspace: &Path) -> PathBuf {
    std::fs::canonicalize(workspace).unwrap_or_else(|_| workspace.to_path_buf())
}

/// Register a new batch-run cancel token (replaces any stale registration).
pub fn begin_run(workspace: &Path) -> CancellationToken {
    let token = CancellationToken::new();
    let key = workspace_key(workspace);
    let mut guard = registry()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(prev) = guard.insert(key, token.clone()) {
        prev.cancel();
    }
    token
}

pub fn end_run(workspace: &Path) {
    let key = workspace_key(workspace);
    let mut guard = registry()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    guard.remove(&key);
}

/// Request stop for the active batch. Returns whether a run was registered.
pub fn request_stop(workspace: &Path) -> bool {
    let key = workspace_key(workspace);
    let guard = registry()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(token) = guard.get(&key) {
        token.cancel();
        true
    } else {
        false
    }
}

pub fn is_run_active(workspace: &Path) -> bool {
    let key = workspace_key(workspace);
    let guard = registry()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    guard.get(&key).is_some_and(|token| !token.is_cancelled())
}
