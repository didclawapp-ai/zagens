//! T3 affected-test hints after code edits (Phase 3.5 / 2b.4 prototype).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use zagens_core::engine::edited_paths_for_tool;

/// Scoped `run_tests` suggestion derived from edited paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AffectedTestSuggestion {
    pub edited_paths: Vec<String>,
    pub run_tests_args: String,
    pub packages: Vec<String>,
}

const EDIT_TOOLS: &[&str] = &["edit_file", "write_file", "apply_patch"];

#[must_use]
pub fn is_edit_tool(tool_name: &str) -> bool {
    EDIT_TOOLS.contains(&tool_name)
}

/// Build a tool-result suffix nudging `run_tests` with a minimal crate scope.
#[must_use]
pub fn hint_suffix_for_paths(workspace: &Path, paths: &[PathBuf]) -> Option<String> {
    let suggestion = suggest_for_edited_paths(workspace, paths)?;
    Some(format_tool_result_suffix(&suggestion))
}

#[must_use]
pub fn hint_suffix_for_tool(
    workspace: &Path,
    tool_name: &str,
    tool_input: &serde_json::Value,
) -> Option<String> {
    if !is_edit_tool(tool_name) {
        return None;
    }
    let paths = edited_paths_for_tool(tool_name, tool_input);
    hint_suffix_for_paths(workspace, &paths)
}

#[must_use]
pub fn suggest_for_edited_paths(
    workspace: &Path,
    paths: &[PathBuf],
) -> Option<AffectedTestSuggestion> {
    if paths.is_empty() {
        return None;
    }

    let mut rel_paths = Vec::new();
    let mut packages = BTreeSet::new();

    for path in paths {
        let rel = normalize_rel_path(workspace, path);
        if rel.is_empty() {
            continue;
        }
        if !looks_like_testable_source(&rel) {
            continue;
        }
        rel_paths.push(rel.clone());
        if let Some(pkg) = package_name_for_rel_path(workspace, Path::new(&rel)) {
            packages.insert(pkg);
        }
    }

    if rel_paths.is_empty() {
        return None;
    }

    let packages: Vec<String> = packages.into_iter().collect();
    let run_tests_args = if packages.is_empty() {
        "--lib".to_string()
    } else {
        packages
            .iter()
            .map(|p| format!("-p {p}"))
            .collect::<Vec<_>>()
            .join(" ")
    };

    Some(AffectedTestSuggestion {
        edited_paths: rel_paths,
        run_tests_args,
        packages,
    })
}

#[must_use]
pub fn format_tool_result_suffix(suggestion: &AffectedTestSuggestion) -> String {
    let paths = suggestion.edited_paths.join(", ");
    format!(
        "\n\n[T3 affected tests] Edited: {paths}. Suggested: `run_tests` with args `{}`.",
        suggestion.run_tests_args
    )
}

fn looks_like_testable_source(rel: &str) -> bool {
    let lower = rel.replace('\\', "/").to_ascii_lowercase();
    [
        ".rs", ".go", ".py", ".ts", ".tsx", ".js", ".jsx", ".java", ".cs",
    ]
    .iter()
    .any(|ext| lower.ends_with(ext))
}

fn normalize_rel_path(workspace: &Path, path: &Path) -> String {
    if path.is_absolute() {
        path.strip_prefix(workspace)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|_| path.to_string_lossy().replace('\\', "/"))
    } else {
        path.to_string_lossy().replace('\\', "/")
    }
}

fn package_name_for_rel_path(workspace: &Path, rel: &Path) -> Option<String> {
    let mut dir = workspace.join(rel).parent()?.to_path_buf();
    if !dir.starts_with(workspace) {
        dir = workspace.to_path_buf();
    }
    loop {
        let manifest = dir.join("Cargo.toml");
        if manifest.is_file() {
            return read_cargo_package_name(&manifest);
        }
        if dir == workspace || !dir.pop() {
            break;
        }
    }
    None
}

fn read_cargo_package_name(manifest: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(manifest).ok()?;
    let mut in_package = false;
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_package = trimmed == "[package]";
            continue;
        }
        if in_package && trimmed.starts_with("name") {
            return trimmed
                .split('"')
                .nth(1)
                .map(str::to_string)
                .or_else(|| trimmed.split('\'').nth(1).map(str::to_string));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn suggests_package_for_crate_member_edit() {
        let dir = TempDir::new().expect("tempdir");
        let crate_dir = dir.path().join("crates").join("demo");
        fs::create_dir_all(crate_dir.join("src")).expect("mkdir");
        fs::write(
            crate_dir.join("Cargo.toml"),
            "[package]\nname = \"zagens-demo\"\n",
        )
        .expect("write manifest");
        fs::write(crate_dir.join("src/lib.rs"), "fn main() {}").expect("write rs");

        let suggestion =
            suggest_for_edited_paths(dir.path(), &[PathBuf::from("crates/demo/src/lib.rs")])
                .expect("suggestion");

        assert_eq!(suggestion.packages, vec!["zagens-demo".to_string()]);
        assert_eq!(suggestion.run_tests_args, "-p zagens-demo");
    }

    #[test]
    fn edit_tool_hint_suffix_uses_tool_input_path() {
        let dir = TempDir::new().expect("tempdir");
        fs::create_dir_all(dir.path().join("src")).expect("mkdir");
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"root-crate\"\n",
        )
        .expect("manifest");
        let input = serde_json::json!({ "path": "src/foo.rs", "content": "x" });
        let suffix = hint_suffix_for_tool(dir.path(), "write_file", &input).expect("suffix");
        assert!(suffix.contains("run_tests"));
        assert!(suffix.contains("-p root-crate"));
    }
}
