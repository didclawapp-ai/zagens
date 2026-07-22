//! Shared glob → workspace file list for batch file tools.

use crate::tools::glob_files::{build_glob_set, path_matches_glob};
use crate::tools::spec::{ToolContext, ToolError};
use crate::tools::workspace_walk::collect_workspace_files;
use std::path::{Path, PathBuf};

/// Workspace-relative POSIX path (`crates/foo.rs`), resilient to `\\?\` prefixes.
#[must_use]
pub(crate) fn workspace_relative_posix(workspace: &Path, file: &Path) -> String {
    let workspace_canon = workspace
        .canonicalize()
        .unwrap_or_else(|_| workspace.to_path_buf());
    let file_canon = file.canonicalize().unwrap_or_else(|_| file.to_path_buf());
    if let Ok(rel) = file_canon.strip_prefix(&workspace_canon) {
        return rel.to_string_lossy().replace('\\', "/");
    }
    if let Ok(rel) = file.strip_prefix(workspace) {
        return rel.to_string_lossy().replace('\\', "/");
    }
    // Last resort: strip verbatim / drive prefixes from the string form.
    zagens_core::engine::normalize_repo_path(&file.to_string_lossy())
}

/// Normalize a path string that may already be absolute/`\\?\` into a repo-relative key.
#[must_use]
pub(crate) fn workspace_relative_from_str(workspace: &Path, path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let as_path = PathBuf::from(trimmed);
    if as_path.is_absolute()
        || trimmed.starts_with("//?/")
        || trimmed.starts_with(r"\\?\")
        || trimmed.starts_with("//./")
    {
        return workspace_relative_posix(workspace, &as_path);
    }
    zagens_core::engine::normalize_repo_path(trimmed)
}

/// Resolve files under `base_path_str` matching `pattern` (glob relative to base).
pub fn resolve_glob_targets(
    context: &ToolContext,
    pattern: &str,
    base_path_str: &str,
    respect_gitignore: bool,
) -> Result<Vec<(PathBuf, String)>, ToolError> {
    let base_path = context.resolve_path(base_path_str)?;
    if !base_path.is_dir() {
        return Err(ToolError::invalid_input(format!(
            "path must be a directory: {base_path_str}"
        )));
    }
    let glob_set = build_glob_set(pattern)?;

    let mut matches: Vec<(PathBuf, String)> = Vec::new();
    for path in collect_workspace_files(&base_path, respect_gitignore) {
        if !path.is_file() {
            continue;
        }
        let glob_relative = path
            .strip_prefix(&base_path)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        if !path_matches_glob(&glob_set, &glob_relative) {
            continue;
        }
        let workspace_rel = workspace_relative_posix(&context.workspace, &path);
        matches.push((path, workspace_rel));
    }
    matches.sort_by(|a, b| a.1.cmp(&b.1));
    Ok(matches)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn workspace_relative_strips_canonical_prefix_on_windows() {
        let tmp = tempdir().expect("tempdir");
        fs::create_dir_all(tmp.path().join("pkg/a")).expect("mkdir");
        let file = tmp.path().join("pkg/a/index.ts");
        fs::write(&file, "x").expect("write");
        let rel = workspace_relative_posix(tmp.path(), &file);
        assert_eq!(rel, "pkg/a/index.ts");
        assert!(!rel.contains(':'));
    }
}
