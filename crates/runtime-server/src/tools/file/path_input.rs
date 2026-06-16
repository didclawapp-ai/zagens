//! Path field parsing for the file tool family (TS-01).
//!
//! Models often send `file` / `file_path` (ecosystem habit) while our schema
//! canonical field is `path`. Detect wrong aliases up front with a targeted
//! hint — same pattern as `edit_file`'s `replace` vs `replacement`.

use crate::tools::spec::{ToolError, required_str};
use serde_json::Value;

/// Alternate keys models use for the file path (aligned with `schedule_bridge`).
const PATH_ALIASES: &[&str] = &["file", "file_path", "filename", "target_path"];

fn path_alias_hint(tool: &str, alias: &str) -> ToolError {
    ToolError::invalid_input(format!(
        "{tool} uses 'path' for the file location, not '{alias}'. Re-send with 'path'."
    ))
}

/// Required path field: canonical `path`, or a clear hint when a known alias was used.
pub(crate) fn required_path_field<'a>(input: &'a Value, tool: &str) -> Result<&'a str, ToolError> {
    if let Some(v) = input.get("path").and_then(Value::as_str) {
        return Ok(v);
    }
    for alias in PATH_ALIASES {
        if input.get(*alias).is_some() {
            return Err(path_alias_hint(tool, alias));
        }
    }
    required_str(input, "path")
}

/// Optional path field (defaults to `.` when absent). Still hints on alias misuse.
pub(crate) fn optional_path_field<'a>(
    input: &'a Value,
    tool: &str,
) -> Result<Option<&'a str>, ToolError> {
    if let Some(v) = input.get("path").and_then(Value::as_str) {
        return Ok(Some(v));
    }
    for alias in PATH_ALIASES {
        if input.get(*alias).is_some() {
            return Err(path_alias_hint(tool, alias));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn required_path_field_accepts_canonical_path() {
        let input = json!({"path": "src/main.rs"});
        assert_eq!(
            required_path_field(&input, "read_file").unwrap(),
            "src/main.rs"
        );
    }

    #[test]
    fn required_path_field_hints_on_file_alias() {
        let input = json!({"file": "x.txt", "content": "hi"});
        let err = required_path_field(&input, "write_file").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("'path'"), "{msg}");
        assert!(msg.contains("'file'"), "{msg}");
        assert!(!msg.contains("missing required field 'path'"), "{msg}");
    }

    #[test]
    fn required_path_field_hints_on_file_path_alias() {
        let input = json!({"file_path": "lib.rs"});
        let err = required_path_field(&input, "edit_file").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("'path'") && msg.contains("'file_path'"),
            "{msg}"
        );
    }

    #[test]
    fn optional_path_field_allows_absent_path() {
        assert!(
            optional_path_field(&json!({}), "list_dir")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn optional_path_field_hints_on_alias() {
        let err = optional_path_field(&json!({"file": "src"}), "list_dir").unwrap_err();
        assert!(err.to_string().contains("'file'"));
    }
}
