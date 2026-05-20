//! Workspace-bound path resolution for CLI session I/O (H08/H09).

use std::path::{Path, PathBuf};

use crate::tools::spec::{ToolContext, ToolError};

/// Resolve `raw` under `workspace` using the same rules as tool file access.
pub fn resolve_under_workspace(workspace: &Path, raw: &str) -> Result<PathBuf, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("路径不能为空".to_string());
    }
    if trimmed.contains("..") {
        return Err("路径不能包含 ..".to_string());
    }
    ToolContext::new(workspace)
        .resolve_path(trimmed)
        .map_err(|e| match e {
            ToolError::PathEscape { path } => {
                format!("路径越出工作区: {}", path.display())
            }
            other => other.to_string(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn rejects_parent_segments() {
        let ws = Path::new("/tmp/ws");
        assert!(resolve_under_workspace(ws, "../etc/passwd").is_err());
    }

    #[test]
    fn rejects_empty_path() {
        let ws = Path::new("/tmp/ws");
        assert!(resolve_under_workspace(ws, "   ").is_err());
    }
}
