//! Workspace file tree (depth 2–3).

use std::fs;
use std::path::{Path, PathBuf};

const MAX_LINES: usize = 80;
const SKIP_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    ".zagens",
    ".deepseek",
    "dist",
    "build",
];

pub fn list_workspace(workspace: &Path, max_depth: u32) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push(format!("{}", display(workspace)));
    walk(workspace, workspace, 0, max_depth, &mut lines);
    if lines.len() > MAX_LINES {
        lines.truncate(MAX_LINES);
        lines.push("…".to_string());
    }
    lines
}

fn walk(root: &Path, dir: &Path, depth: u32, max_depth: u32, lines: &mut Vec<String>) {
    if depth >= max_depth || lines.len() >= MAX_LINES {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut names: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    names.sort();
    for path in names {
        if lines.len() >= MAX_LINES {
            break;
        }
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
        if path.is_dir() {
            if SKIP_DIRS.contains(&name) {
                continue;
            }
            let indent = "  ".repeat(depth as usize + 1);
            lines.push(format!("{indent}{name}/"));
            walk(root, &path, depth + 1, max_depth, lines);
        } else {
            let indent = "  ".repeat(depth as usize + 1);
            lines.push(format!("{indent}{name}"));
        }
    }
}

fn display(path: &Path) -> String {
    crate::utils::display_path(path)
}
