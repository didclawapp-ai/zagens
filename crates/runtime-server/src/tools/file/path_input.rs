//! Path field parsing for the file tool family (TS-01).
//!
//! Models often send `file` / `file_path` (ecosystem habit) while our schema
//! canonical field is `path`. Accept known aliases (same keys as
//! `schedule_bridge`) so one wrong key does not waste a whole turn.

use crate::tools::spec::ToolError;
use serde_json::Value;

/// Alternate keys models use for the file path (aligned with `schedule_bridge`).
const PATH_ALIASES: &[&str] = &["file", "file_path", "filename", "target_path"];

fn path_from_aliases<'a>(input: &'a Value) -> Option<&'a str> {
    for key in std::iter::once("path").chain(PATH_ALIASES.iter().copied()) {
        if let Some(v) = input.get(key).and_then(Value::as_str) {
            let trimmed = v.trim();
            if !trimmed.is_empty() {
                return Some(trimmed);
            }
        }
    }
    None
}

/// Required path field: canonical `path` or accepted alias.
pub(crate) fn required_path_field<'a>(input: &'a Value, tool: &str) -> Result<&'a str, ToolError> {
    path_from_aliases(input).ok_or_else(|| {
        ToolError::invalid_input(format!(
            "{tool} requires 'path' (aliases accepted: file, file_path, filename, target_path)."
        ))
    })
}

/// Optional path field (defaults to `.` when absent).
pub(crate) fn optional_path_field<'a>(
    input: &'a Value,
    _tool: &str,
) -> Result<Option<&'a str>, ToolError> {
    Ok(path_from_aliases(input))
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
        assert_eq!(required_path_field(&input, "write_file").unwrap(), "x.txt");
    }

    #[test]
    fn required_path_field_hints_on_file_path_alias() {
        let input = json!({"file_path": "lib.rs"});
        assert_eq!(required_path_field(&input, "edit_file").unwrap(), "lib.rs");
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
        assert_eq!(
            optional_path_field(&json!({"file": "src"}), "list_dir")
                .unwrap()
                .as_deref(),
            Some("src")
        );
    }
}
