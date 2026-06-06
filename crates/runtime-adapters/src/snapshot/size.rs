//! Workspace byte-size estimation before side-git snapshot init.
//!
//! A full `git add -A` on multi-GB trees (node_modules, model weights, etc.)
//! can block for minutes. We walk the workspace with the same ripgrep-style
//! skip rules as search tools and bail out before touching git when the
//! on-disk footprint exceeds `[snapshots] max_workspace_gb`.

use std::io;
use std::path::Path;

use ignore::WalkBuilder;

use crate::tools::workspace_walk::configure_workspace_walk;

/// Default cap when `[snapshots] max_workspace_gb` is unset (matches upstream v0.8.32).
pub const DEFAULT_SNAPSHOT_MAX_WORKSPACE_GB: f64 = 2.0;

/// Returns `true` when the workspace tree exceeds `max_gb` ( gibibyte-style GB:
/// `max_gb * 1024^3` bytes).
pub fn workspace_exceeds_size_limit(workspace: &Path, max_gb: f64) -> io::Result<bool> {
    if max_gb <= 0.0 {
        return Ok(false);
    }
    let max_bytes = (max_gb * 1024.0 * 1024.0 * 1024.0) as u64;
    let total = estimate_workspace_bytes(workspace, Some(max_bytes.saturating_add(1)))?;
    Ok(total > max_bytes)
}

/// Sum file sizes under `workspace`, honoring gitignore + skip-dir rules.
/// Stops early once `limit_bytes` would be exceeded (pass `None` for full scan).
pub fn estimate_workspace_bytes(workspace: &Path, limit_bytes: Option<u64>) -> io::Result<u64> {
    if workspace.is_file() {
        let len = workspace.metadata()?.len();
        return Ok(len);
    }
    if !workspace.is_dir() {
        return Ok(0);
    }

    let mut builder = WalkBuilder::new(workspace);
    configure_workspace_walk(&mut builder, true);
    let walker = builder.build();

    let mut total = 0u64;
    for entry in walker.flatten() {
        if !entry.file_type().is_some_and(|ft| ft.is_file()) {
            continue;
        }
        let len = entry.metadata().map(|m| m.len()).unwrap_or(0);
        total = total.saturating_add(len);
        if limit_bytes.is_some_and(|limit| total > limit) {
            break;
        }
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn estimate_respects_limit_and_skips_heavy_dir_names() {
        let tmp = tempdir().expect("tempdir");
        let root = tmp.path();
        fs::write(root.join("small.txt"), "hello").expect("write");
        let nm = root.join("node_modules");
        fs::create_dir_all(&nm).expect("mkdir");
        fs::write(nm.join("huge.bin"), vec![0u8; 4096]).expect("write");

        let total = estimate_workspace_bytes(root, None).expect("estimate");
        assert!(
            total < 4096,
            "node_modules should be skipped, got {total} bytes"
        );
    }

    #[test]
    fn exceeds_limit_when_small_files_add_up() {
        let tmp = tempdir().expect("tempdir");
        let root = tmp.path();
        fs::write(root.join("a.bin"), vec![0u8; 512]).expect("write");
        fs::write(root.join("b.bin"), vec![0u8; 512]).expect("write");
        assert!(workspace_exceeds_size_limit(root, 0.0000005).expect("check"));
        assert!(!workspace_exceeds_size_limit(root, 10.0).expect("check"));
    }
}
