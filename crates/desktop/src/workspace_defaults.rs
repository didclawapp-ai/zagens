//! Default Composer workspace paths (avoid process cwd / System32).

use std::path::PathBuf;

const WORKSPACE_DIR_NAME: &str = "Zagens";
const LEGACY_WORKSPACE_DIR_NAME: &str = "Zagens";
/// Office deliverables root under the Composer workspace (see docs/task-type-prompt-architecture.md).
pub const OFFICE_OUTPUT_DIR_NAME: &str = "deliverables";

/// User Documents folder (platform-specific via `dirs` crate).
pub fn user_documents_dir() -> Result<PathBuf, String> {
    dirs::document_dir().ok_or_else(|| {
        "Cannot resolve the Documents directory on this system.".to_string()
    })
}

/// Default Composer workspace: `<Documents>/Zagens` (or legacy `<Documents>/Zagens` if it exists).
pub fn default_composer_workspace() -> Result<String, String> {
    let docs = user_documents_dir()?;
    let new_root = docs.join(WORKSPACE_DIR_NAME);
    let legacy_root = docs.join(LEGACY_WORKSPACE_DIR_NAME);
    let root = if new_root.is_dir() || !legacy_root.is_dir() {
        new_root
    } else {
        legacy_root
    };
    if !root.is_dir() {
        std::fs::create_dir_all(&root)
            .map_err(|e| format!("Failed to create workspace directory {}: {e}", root.display()))?;
    }
    let _ = std::fs::create_dir_all(root.join(OFFICE_OUTPUT_DIR_NAME));
    let display = path_for_ui_display(root.canonicalize().unwrap_or(root));
    Ok(display)
}

/// Avoid `\\?\` verbatim prefixes in UI / HTTP query strings (Windows canonicalize).
pub(crate) fn path_for_ui_display(path: PathBuf) -> String {
    let s = path.to_string_lossy();
    #[cfg(windows)]
    {
        if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
            return format!(r"\\{rest}");
        }
        if let Some(rest) = s.strip_prefix(r"\\?\") {
            return rest.to_string();
        }
    }
    s.into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_workspace_is_under_documents() {
        let docs = user_documents_dir().expect("documents");
        let ws = default_composer_workspace().expect("workspace");
        assert!(ws.contains(WORKSPACE_DIR_NAME));
        let root = PathBuf::from(&ws);
        assert!(root.starts_with(&docs) || docs.canonicalize().is_ok_and(|d| root.starts_with(d)));
    }
}
