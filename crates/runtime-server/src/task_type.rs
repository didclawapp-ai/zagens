//! Task type — enum re-exported from core + heuristic inference.

use std::path::Path;

pub use zagens_core::task_type::TaskType;

/// Resolve UI / API value (`auto` | `office` | `code`) to a concrete [`TaskType`].
///
/// Legacy `office` values are coerced to [`TaskType::Code`].
#[must_use]
pub fn resolve_task_type(
    raw: Option<&str>,
    workspace: &Path,
    first_message: Option<&str>,
) -> TaskType {
    match raw.map(str::trim).map(|s| s.to_ascii_lowercase()) {
        Some(s) if s == "office" || s == "code" => TaskType::Code,
        _ => infer_task_type(workspace, first_message),
    }
}

/// Heuristic classification for `auto` and unknown values.
///
/// Always [`TaskType::Code`] after Office mode removal (documents go through
/// `load_skill zagens-office` + external CLI).
#[must_use]
pub fn infer_task_type(_workspace: &Path, _first_message: Option<&str>) -> TaskType {
    TaskType::Code
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn resolve_coerces_legacy_office_to_code() {
        let ws = PathBuf::from(".");
        assert_eq!(resolve_task_type(Some("office"), &ws, None), TaskType::Code);
        assert_eq!(resolve_task_type(Some("code"), &ws, None), TaskType::Code);
        assert_eq!(resolve_task_type(Some("auto"), &ws, None), TaskType::Code);
    }

    #[test]
    fn infer_always_code() {
        let ws = PathBuf::from(".");
        assert_eq!(infer_task_type(&ws, Some("写一份周报 PPT")), TaskType::Code);
    }
}
