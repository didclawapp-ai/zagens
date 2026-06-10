//! In-process read-through cache for CRAFT blackboard JSON files (mtime-keyed).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

const MAX_CACHE_ENTRIES: usize = 64;

struct CacheEntry {
    mtime: SystemTime,
    raw: String,
}

static CACHE: Mutex<Option<HashMap<PathBuf, CacheEntry>>> = Mutex::new(None);

fn cache_map() -> std::sync::MutexGuard<'static, Option<HashMap<PathBuf, CacheEntry>>> {
    CACHE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Read blackboard file contents, reusing cached bytes when mtime is unchanged.
pub(crate) fn read_cached(path: &Path) -> Option<String> {
    let mtime = std::fs::metadata(path).ok()?.modified().ok()?;
    let mut guard = cache_map();
    let map = guard.get_or_insert_with(HashMap::new);
    if let Some(entry) = map.get(path) {
        if entry.mtime == mtime {
            return Some(entry.raw.clone());
        }
    }
    let raw = std::fs::read_to_string(path).ok()?;
    if map.len() >= MAX_CACHE_ENTRIES {
        map.clear();
    }
    map.insert(
        path.to_path_buf(),
        CacheEntry {
            mtime,
            raw: raw.clone(),
        },
    );
    Some(raw)
}

/// Drop cached entry after a blackboard write (or external mutation).
pub(crate) fn invalidate(path: &Path) {
    let mut guard = cache_map();
    if let Some(map) = guard.as_mut() {
        map.remove(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_reuses_entry_until_invalidate() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("board.json");
        std::fs::write(&path, r#"{"v":1}"#).expect("write");

        let first = read_cached(&path).expect("read");
        let second = read_cached(&path).expect("cached");
        assert_eq!(first, second);
        assert_eq!(first, r#"{"v":1}"#);

        std::fs::write(&path, r#"{"v":2}"#).expect("rewrite");
        invalidate(&path);
        let fresh = read_cached(&path).expect("fresh");
        assert_eq!(fresh, r#"{"v":2}"#);
    }
}
