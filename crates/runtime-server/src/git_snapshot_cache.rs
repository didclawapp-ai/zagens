//! Short TTL + single-flight cache for workspace git snapshots.
//! Prevents Composer badge + Diff poll + visibility from stacking `git status`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use tokio::sync::Mutex as AsyncMutex;

use crate::git_read::{GitSnapshot, MAX_CHANGES, collect_snapshot};

/// Fresh enough for badge/Diff polling without feeling stale.
const SNAPSHOT_TTL: Duration = Duration::from_millis(2_000);

#[derive(Clone)]
struct CachedSnapshot {
    at: Instant,
    snap: Arc<GitSnapshot>,
}

static CACHE: LazyLock<Mutex<HashMap<PathBuf, CachedSnapshot>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Per-root lock so concurrent status+changes share one `git status`.
static INFLIGHT: LazyLock<Mutex<HashMap<PathBuf, Arc<AsyncMutex<()>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn inflight_gate(root: &Path) -> Arc<AsyncMutex<()>> {
    let mut map = INFLIGHT.lock().unwrap_or_else(|e| e.into_inner());
    map.entry(root.to_path_buf())
        .or_insert_with(|| Arc::new(AsyncMutex::new(())))
        .clone()
}

fn cache_get(root: &Path) -> Option<Arc<GitSnapshot>> {
    let map = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let hit = map.get(root)?;
    if hit.at.elapsed() <= SNAPSHOT_TTL {
        Some(Arc::clone(&hit.snap))
    } else {
        None
    }
}

fn cache_put(root: PathBuf, snap: GitSnapshot) -> Arc<GitSnapshot> {
    let arc = Arc::new(snap);
    let mut map = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    map.insert(
        root,
        CachedSnapshot {
            at: Instant::now(),
            snap: Arc::clone(&arc),
        },
    );
    // Bound map growth (workspaces rarely exceed a handful).
    if map.len() > 32 {
        let cutoff = Instant::now() - SNAPSHOT_TTL * 4;
        map.retain(|_, v| v.at > cutoff);
    }
    arc
}

/// Load snapshot with TTL cache + single-flight (one git status per root at a time).
pub async fn load_snapshot(root: PathBuf) -> Result<Arc<GitSnapshot>, String> {
    if let Some(hit) = cache_get(&root) {
        return Ok(hit);
    }

    let gate = inflight_gate(&root);
    let _guard = gate.lock().await;

    // Re-check after waiting — another waiter may have filled the cache.
    if let Some(hit) = cache_get(&root) {
        return Ok(hit);
    }

    let root_clone = root.clone();
    let snap = tokio::task::spawn_blocking(move || collect_snapshot(&root_clone, MAX_CHANGES))
        .await
        .map_err(|e| e.to_string())?;

    Ok(cache_put(root, snap))
}
