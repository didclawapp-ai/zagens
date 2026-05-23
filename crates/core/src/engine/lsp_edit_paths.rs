//! Pure helpers for post-edit LSP path extraction (P2 tool_execution portization).

use std::path::PathBuf;

use serde_json::Value;

/// Derive workspace-relative paths touched by an edit tool call.
pub fn edited_paths_for_tool(tool_name: &str, input: &Value) -> Vec<PathBuf> {
    match tool_name {
        "edit_file" | "write_file" => input
            .get("path")
            .and_then(Value::as_str)
            .map(|path| vec![PathBuf::from(path)])
            .unwrap_or_default(),
        "apply_patch" => {
            let mut out = Vec::new();
            if let Some(path) = input.get("path").and_then(Value::as_str) {
                out.push(PathBuf::from(path));
            }
            if let Some(files) = input.get("files").and_then(Value::as_array) {
                for entry in files {
                    if let Some(path) = entry.get("path").and_then(Value::as_str) {
                        out.push(PathBuf::from(path));
                    }
                }
            }
            if out.is_empty()
                && let Some(patch) = input.get("patch").and_then(Value::as_str)
            {
                out.extend(parse_patch_paths(patch));
            }
            out
        }
        _ => Vec::new(),
    }
}

/// Parse `+++ b/<path>` headers from a unified diff patch.
pub fn parse_patch_paths(patch: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for line in patch.lines() {
        if let Some(rest) = line.strip_prefix("+++ ") {
            let trimmed = rest.trim();
            let path = trimmed.strip_prefix("b/").unwrap_or(trimmed);
            if path == "/dev/null" {
                continue;
            }
            out.push(PathBuf::from(path));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn edited_paths_edit_and_write_file() {
        let input = json!({"path": "src/main.rs"});
        assert_eq!(
            edited_paths_for_tool("edit_file", &input),
            vec![PathBuf::from("src/main.rs")]
        );
        assert_eq!(
            edited_paths_for_tool("write_file", &input),
            vec![PathBuf::from("src/main.rs")]
        );
    }

    #[test]
    fn edited_paths_apply_patch_files_and_patch_fallback() {
        let files = json!({"files": [{"path": "a.rs"}, {"path": "b.rs"}]});
        assert_eq!(
            edited_paths_for_tool("apply_patch", &files),
            vec![PathBuf::from("a.rs"), PathBuf::from("b.rs")]
        );

        let patch = json!({"patch": "--- a/x\n+++ b/y\n"});
        assert_eq!(
            edited_paths_for_tool("apply_patch", &patch),
            vec![PathBuf::from("y")]
        );
    }

    #[test]
    fn edited_paths_ignores_non_edit_tools() {
        let input = json!({"path": "src/main.rs"});
        assert!(edited_paths_for_tool("read_file", &input).is_empty());
        assert!(edited_paths_for_tool("grep_files", &input).is_empty());
    }

    #[test]
    fn parse_patch_paths_skips_dev_null() {
        let patch = "--- a/old\n+++ /dev/null\n--- a/keep\n+++ b/src/lib.rs\n";
        assert_eq!(
            parse_patch_paths(patch),
            vec![PathBuf::from("src/lib.rs")]
        );
    }
}
