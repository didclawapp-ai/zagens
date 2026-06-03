//! Task type (Office / Code) — enum re-exported from core + heuristic inference.

use std::path::Path;

// Re-exported from deepseek-core (P2 PR4).
pub use deepseek_core::task_type::TaskType;

/// Resolve UI / API value (`auto` | `office` | `code`) to a concrete [`TaskType`].
#[must_use]
pub fn resolve_task_type(
    raw: Option<&str>,
    workspace: &Path,
    first_message: Option<&str>,
) -> TaskType {
    match raw.map(str::trim).map(|s| s.to_ascii_lowercase()) {
        Some(s) if s == "office" => TaskType::Office,
        Some(s) if s == "code" => TaskType::Code,
        _ => infer_task_type(workspace, first_message),
    }
}

/// Heuristic classification for `auto` and unknown values.
#[must_use]
pub fn infer_task_type(workspace: &Path, first_message: Option<&str>) -> TaskType {
    if let Some(msg) = first_message {
        if message_implies_code(msg) {
            return TaskType::Code;
        }
        if message_implies_office(msg) && !message_implies_code(msg) {
            return TaskType::Office;
        }
    }
    if workspace_looks_like_code_repo(workspace) {
        return TaskType::Code;
    }
    TaskType::Office
}

fn message_implies_code(msg: &str) -> bool {
    let lower = msg.to_ascii_lowercase();
    const CODE: &[&str] = &[
        "edit_file",
        "apply_patch",
        "bugfix",
        "bug fix",
        "fix bug",
        " fix",
        "bug",
        "refactor",
        "implement",
        "cargo ",
        "npm run",
        "grep_files",
        "compile",
        "debug",
        "unit test",
        "pull request",
        "pr ",
        "代码",
        "修复",
        "实现",
        "重构",
        "调试",
        "编译",
    ];
    CODE.iter().any(|k| lower.contains(k))
}

fn message_implies_office(msg: &str) -> bool {
    let lower = msg.to_ascii_lowercase();
    const OFFICE: &[&str] = &[
        "xlsx",
        "docx",
        "pptx",
        "pdf",
        "excel",
        "word",
        "powerpoint",
        "write_office",
        "read_office",
        "deliverables",
        "表格",
        "文档",
        "演示",
        "汇报",
        "ppt",
        "周报",
        "月报",
        "会议纪要",
        "纪要",
        "写一份",
        "整理成",
        "总结为",
        "汇报材料",
        "方案",
        "报告",
        "合同",
        "邮件",
        "简历",
        "发布说明",
    ];
    OFFICE.iter().any(|k| lower.contains(k))
}

fn workspace_looks_like_code_repo(workspace: &Path) -> bool {
    const MARKERS: &[&str] = &[
        "Cargo.toml",
        "package.json",
        "pnpm-lock.yaml",
        "yarn.lock",
        "go.mod",
        "pyproject.toml",
        "requirements.txt",
        ".git",
    ];
    for name in MARKERS {
        if workspace.join(name).exists() {
            return true;
        }
    }
    if shallow_has_code_extension(workspace, 2) {
        return true;
    }
    workspace.join("src").is_dir() && shallow_has_code_extension(&workspace.join("src"), 2)
}

fn shallow_has_code_extension(dir: &Path, max_depth: u32) -> bool {
    shallow_has_code_extension_inner(dir, max_depth, 0)
}

fn shallow_has_code_extension_inner(dir: &Path, max_depth: u32, depth: u32) -> bool {
    if depth > max_depth {
        return false;
    }
    let Ok(read) = std::fs::read_dir(dir) else {
        return false;
    };
    for entry in read.flatten() {
        let path = entry.path();
        if path.is_file() {
            if is_code_extension(path.extension().and_then(|e| e.to_str())) {
                return true;
            }
        } else if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if matches!(name, "node_modules" | "target" | "dist" | ".git") {
                continue;
            }
            if shallow_has_code_extension_inner(&path, max_depth, depth + 1) {
                return true;
            }
        }
    }
    false
}

fn is_code_extension(ext: Option<&str>) -> bool {
    matches!(
        ext,
        Some(
            "rs" | "ts" | "tsx" | "js" | "jsx" | "py" | "go" | "java" | "kt" | "c" | "cpp"
                | "h" | "hpp" | "cs" | "rb" | "php" | "swift" | "toml"
        )
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn parse_and_display() {
        assert_eq!(TaskType::parse_str("office"), Some(TaskType::Office));
        assert_eq!(TaskType::parse_str("CODE"), Some(TaskType::Code));
        assert_eq!(TaskType::Office.as_str(), "office");
    }

    #[test]
    fn infer_code_from_fix_message() {
        let dir = tempdir().unwrap();
        assert_eq!(
            infer_task_type(dir.path(), Some("please fix the login bug")),
            TaskType::Code
        );
    }

    #[test]
    fn infer_office_from_xlsx_message() {
        let dir = tempdir().unwrap();
        assert_eq!(
            infer_task_type(dir.path(), Some("generate quarterly report xlsx")),
            TaskType::Office
        );
    }

    #[test]
    fn infer_code_from_cargo_workspace() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
        assert_eq!(infer_task_type(dir.path(), None), TaskType::Code);
    }

    #[test]
    fn resolve_auto_uses_infer() {
        let dir = tempdir().unwrap();
        assert_eq!(
            resolve_task_type(Some("auto"), dir.path(), Some("write a docx summary")),
            TaskType::Office
        );
    }

    #[test]
    fn infer_office_from_chinese_weekly_report() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "[package]\n").unwrap();
        assert_eq!(
            infer_task_type(dir.path(), Some("帮我整理一份发布说明")),
            TaskType::Office
        );
        assert_eq!(
            infer_task_type(dir.path(), Some("实现发布说明页面")),
            TaskType::Code
        );
    }

    #[test]
    fn resolve_explicit_overrides_workspace() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "[package]\n").unwrap();
        assert_eq!(
            resolve_task_type(Some("office"), dir.path(), None),
            TaskType::Office
        );
    }
}
