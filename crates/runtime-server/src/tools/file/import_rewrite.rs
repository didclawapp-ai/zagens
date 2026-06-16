//! Relative import path rewriting for TS/JS/Go (TS-07 codemod follow-up).

use regex::Regex;
use std::sync::LazyLock;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportRewrite {
    pub old_spec: String,
    pub new_spec: String,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileImportRewriteOutcome {
    pub updated: String,
    pub rewrites: Vec<ImportRewrite>,
}

fn split_posix(path: &str) -> Vec<String> {
    path.replace('\\', "/")
        .split('/')
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .collect()
}

fn normalize_posix(parts: &[String]) -> Vec<String> {
    let mut stack = Vec::new();
    for part in parts {
        if part == ".." {
            stack.pop();
        } else if part != "." {
            stack.push(part.clone());
        }
    }
    stack
}

/// Normalize a workspace-relative path (posix, no leading `./`).
#[must_use]
pub fn normalize_workspace_path(path: &str) -> String {
    normalize_posix(&split_posix(path.trim().trim_start_matches("./"))).join("/")
}

/// Resolve a relative import specifier from a source file directory.
#[must_use]
pub fn resolve_relative_import(from_dir: &str, import_spec: &str) -> String {
    let mut parts = split_posix(from_dir);
    for segment in split_posix(import_spec) {
        if segment == ".." {
            parts.pop();
        } else if segment != "." {
            parts.push(segment);
        }
    }
    normalize_posix(&parts).join("/")
}

/// Compute a relative import path from `from_dir` to workspace target `to_target`.
#[must_use]
pub fn relative_import_to(from_dir: &str, to_target: &str) -> String {
    let from = normalize_posix(&split_posix(from_dir));
    let to = normalize_posix(&split_posix(to_target));
    let mut common = 0usize;
    while common < from.len() && common < to.len() && from[common].eq_ignore_ascii_case(&to[common])
    {
        common += 1;
    }
    let ups = from.len().saturating_sub(common);
    let down = &to[common..];
    let mut parts: Vec<String> = vec!["..".to_string(); ups];
    parts.extend(down.iter().cloned());
    if parts.is_empty() {
        ".".to_string()
    } else if ups == 0 {
        format!("./{}", down.join("/"))
    } else {
        parts.join("/")
    }
}

fn strip_ts_extension(path: &str) -> &str {
    path.strip_suffix(".ts")
        .or_else(|| path.strip_suffix(".tsx"))
        .or_else(|| path.strip_suffix(".js"))
        .or_else(|| path.strip_suffix(".jsx"))
        .or_else(|| path.strip_suffix(".mjs"))
        .or_else(|| path.strip_suffix(".cjs"))
        .unwrap_or(path)
}

fn paths_match_target(resolved: &str, target: &str) -> bool {
    let resolved_norm = normalize_workspace_path(resolved);
    let target_norm = normalize_workspace_path(target);
    if resolved_norm == target_norm {
        return true;
    }
    let resolved_base = normalize_workspace_path(strip_ts_extension(&resolved_norm));
    let target_base = normalize_workspace_path(strip_ts_extension(&target_norm));
    resolved_base == target_base
        || resolved_norm.starts_with(&format!("{target_base}/"))
        || target_norm.starts_with(&format!("{resolved_base}/"))
}

static REL_IMPORT_SINGLE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"'(\.(?:\./|\.\./)[^']*)'").expect("REL_IMPORT_SINGLE"));
static REL_IMPORT_DOUBLE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#""(\.(?:\./|\.\./)[^"]*)""#).expect("REL_IMPORT_DOUBLE"));
static REL_IMPORT_BACKTICK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"`(\.(?:\./|\.\./)[^`]*)`").expect("REL_IMPORT_BACKTICK"));

fn relative_import_matches(line: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for caps in REL_IMPORT_SINGLE.captures_iter(line) {
        let spec = caps.get(1).expect("spec").as_str();
        out.push((format!("'{spec}'"), spec.to_string()));
    }
    for caps in REL_IMPORT_DOUBLE.captures_iter(line) {
        let spec = caps.get(1).expect("spec").as_str();
        out.push((format!("\"{spec}\""), spec.to_string()));
    }
    for caps in REL_IMPORT_BACKTICK.captures_iter(line) {
        let spec = caps.get(1).expect("spec").as_str();
        out.push((format!("`{spec}`"), spec.to_string()));
    }
    out
}

fn is_import_line(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with("import ")
        || trimmed.contains(" from ")
        || trimmed.contains("require(")
        || trimmed.starts_with("export ")
}

/// Rewrite relative imports in `content` that resolve to `from_target` → per-file path to `to_target`.
#[must_use]
pub fn rewrite_imports_in_file(
    content: &str,
    file_dir: &str,
    from_target: &str,
    to_target: &str,
) -> FileImportRewriteOutcome {
    let from_target = normalize_workspace_path(from_target);
    let to_target = normalize_workspace_path(to_target);
    let new_spec = relative_import_to(file_dir, &to_target);

    let mut rewrites = Vec::new();
    let mut lines: Vec<String> = Vec::new();

    for (idx, line) in content.lines().enumerate() {
        if !is_import_line(line) {
            lines.push(line.to_string());
            continue;
        }

        let mut new_line = line.to_string();
        for (full, old_spec) in relative_import_matches(line) {
            let resolved = resolve_relative_import(file_dir, &old_spec);
            if !paths_match_target(&resolved, &from_target) {
                continue;
            }
            let quote = full.chars().next().expect("quote");
            let replacement = format!("{quote}{new_spec}{quote}");
            if new_line.contains(&full) {
                new_line = new_line.replacen(&full, &replacement, 1);
                rewrites.push(ImportRewrite {
                    old_spec,
                    new_spec: new_spec.clone(),
                    line: idx + 1,
                });
            }
        }
        lines.push(new_line);
    }

    let line_ending = if content.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let mut updated = lines.join(line_ending);
    if content.ends_with('\n') && !updated.ends_with('\n') {
        updated.push('\n');
    }

    FileImportRewriteOutcome { updated, rewrites }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_import_to_same_depth() {
        assert_eq!(
            relative_import_to("pkg/module_0", "shared/new"),
            "../../shared/new"
        );
    }

    #[test]
    fn relative_import_to_deeper_file() {
        assert_eq!(
            relative_import_to("pkg/module_0", "shared/new"),
            "../../shared/new"
        );
        assert_eq!(
            relative_import_to("pkg/a/b", "shared/new"),
            "../../../shared/new"
        );
        assert_eq!(
            relative_import_to("pkg/a", "shared/new"),
            "../../shared/new"
        );
    }

    #[test]
    fn resolve_relative_import_matches_target() {
        assert_eq!(
            resolve_relative_import("pkg/module_0", "../../shared/old"),
            "shared/old"
        );
        assert_eq!(
            resolve_relative_import("pkg/a/b", "../../../shared/old"),
            "shared/old"
        );
    }

    #[test]
    fn rewrites_mixed_depth_files_differently() {
        let shallow = "import x from '../../shared/old';\n";
        let deep = "import x from '../../../shared/old';\n";

        let shallow_out = rewrite_imports_in_file(shallow, "pkg/a", "shared/old", "shared/new");
        assert_eq!(shallow_out.rewrites.len(), 1);
        assert!(shallow_out.updated.contains("'../../shared/new'"));

        let deep_out = rewrite_imports_in_file(deep, "pkg/a/b", "shared/old", "shared/new");
        assert_eq!(deep_out.rewrites.len(), 1);
        assert!(deep_out.updated.contains("'../../../shared/new'"));
    }

    #[test]
    fn skips_non_import_lines_and_unrelated_paths() {
        let src = "const s = '../../shared/old';\nimport y from '../other/x';\n";
        let out = rewrite_imports_in_file(src, "pkg/a", "shared/old", "shared/new");
        assert!(out.rewrites.is_empty());
        assert_eq!(out.updated, src);
    }

    #[test]
    fn go_single_import_rewritten() {
        let src = "package main\n\nimport \"../../shared/old\"\n";
        let out = rewrite_imports_in_file(src, "pkg/module_0", "shared/old", "shared/new");
        assert_eq!(out.rewrites.len(), 1);
        assert!(out.updated.contains("\"../../shared/new\""));
    }
}
