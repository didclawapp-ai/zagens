//! Scratchpad directory TTL cleanup (B7).

use std::fs;
use std::path::Path;
use std::time::SystemTime;

use crate::scratchpad::config::ScratchpadConfig;

/// Remove stale `scratchpad/{run_id}/` directories under `workspace/.deepseek/scratchpad/`.
///
/// Skips `active_run_ids` (e.g. thread-bound runs still in use).
pub fn cleanup_stale_scratchpads(
    workspace: &Path,
    config: &ScratchpadConfig,
    active_run_ids: &[String],
) {
    if config.retention_days == 0 {
        return;
    }
    let root = workspace.join(".deepseek").join("scratchpad");
    if !root.is_dir() {
        return;
    }
    let max_age = std::time::Duration::from_secs(u64::from(config.retention_days) * 86_400);
    let now = SystemTime::now();
    let active: std::collections::HashSet<&str> =
        active_run_ids.iter().map(String::as_str).collect();

    let entries = match fs::read_dir(&root) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if active.contains(name) {
            continue;
        }
        let Ok(meta) = fs::metadata(&path) else {
            continue;
        };
        let Ok(modified) = meta.modified() else {
            continue;
        };
        let Ok(age) = now.duration_since(modified) else {
            continue;
        };
        if age > max_age {
            let _ = fs::remove_dir_all(&path);
            tracing::info!(
                run_id = name,
                "Removed stale audit scratchpad directory"
            );
        }
    }
}
