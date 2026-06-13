//! Interactive workspace file tree for the Files inspector tab.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

const MAX_VISIBLE: usize = 512;
const SKIP_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    ".zagens",
    ".deepseek",
    "dist",
    "build",
];

#[derive(Debug, Clone)]
pub struct FileTreeLine {
    pub rel: String,
    pub depth: u32,
    pub is_dir: bool,
    pub label: String,
}

#[derive(Debug, Clone, Default)]
pub struct FileTreeState {
    root_display: String,
    lines: Vec<FileTreeLine>,
    expanded: BTreeSet<String>,
}

impl FileTreeState {
    pub fn rescan(&mut self, workspace: &Path) {
        self.root_display = crate::utils::display_path(workspace);
        if self.expanded.is_empty() {
            self.expanded.insert(String::new());
        }
        self.rebuild_lines(workspace);
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    pub fn line_at(&self, cursor: usize) -> Option<&FileTreeLine> {
        self.lines.get(cursor)
    }

    pub fn toggle_cursor(&mut self, cursor: usize, workspace: &Path) {
        let Some(line) = self.lines.get(cursor) else {
            return;
        };
        if !line.is_dir {
            return;
        }
        let key = line.rel.clone();
        if self.expanded.contains(&key) {
            self.expanded.remove(&key);
        } else {
            self.expanded.insert(key);
        }
        self.rebuild_lines(workspace);
    }

    pub fn render_lines(
        &self,
        cursor: usize,
        scroll: usize,
        height: usize,
        max_cols: usize,
    ) -> Vec<(String, bool)> {
        let visible = height.max(4);
        let max_cols = max_cols.max(8);
        let start = scroll.min(self.lines.len().saturating_sub(1));
        self.lines
            .iter()
            .enumerate()
            .skip(start)
            .take(visible)
            .map(|(idx, line)| {
                // Cursor is shown via row highlight (no leading prefix column), so text
                // sits right after the pane border. Dir chevron + space aligns file names
                // with directory names at the same depth.
                let marker = if line.is_dir {
                    if self.expanded.contains(&line.rel) {
                        "v "
                    } else {
                        "> "
                    }
                } else {
                    "- "
                };
                let indent = "  ".repeat(line.depth as usize);
                let raw = format!("{indent}{marker}{}{}", line.label, dir_suffix(line.is_dir));
                let text = super::super::display_format::truncate_display_width(&raw, max_cols);
                (text, idx == cursor)
            })
            .collect()
    }

    fn rebuild_lines(&mut self, workspace: &Path) {
        let mut lines = Vec::new();
        lines.push(FileTreeLine {
            rel: String::new(),
            depth: 0,
            is_dir: true,
            label: self.root_display.clone(),
        });
        if self.expanded.contains("") {
            append_dir(workspace, "", 1, &self.expanded, &mut lines);
        }
        if lines.len() > MAX_VISIBLE {
            lines.truncate(MAX_VISIBLE);
            lines.push(FileTreeLine {
                rel: String::new(),
                depth: 1,
                is_dir: false,
                label: "…".to_string(),
            });
        }
        self.lines = lines;
    }
}

fn dir_suffix(is_dir: bool) -> &'static str {
    if is_dir { "/" } else { "" }
}

fn append_dir(
    dir: &Path,
    rel: &str,
    depth: u32,
    expanded: &BTreeSet<String>,
    lines: &mut Vec<FileTreeLine>,
) {
    if lines.len() >= MAX_VISIBLE {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    paths.sort();
    for path in paths {
        if lines.len() >= MAX_VISIBLE {
            break;
        }
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
        if path.is_dir() && SKIP_DIRS.contains(&name) {
            continue;
        }
        let child_rel = if rel.is_empty() {
            name.to_string()
        } else {
            format!("{rel}/{name}")
        };
        let is_dir = path.is_dir();
        lines.push(FileTreeLine {
            rel: child_rel.clone(),
            depth,
            is_dir,
            label: name.to_string(),
        });
        if is_dir && expanded.contains(&child_rel) {
            append_dir(&path, &child_rel, depth + 1, expanded, lines);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn expand_dir_reveals_children() {
        let tmp = TempDir::new().expect("tempdir");
        fs::write(tmp.path().join("a.txt"), "x").expect("write");
        fs::create_dir(tmp.path().join("src")).expect("mkdir");
        fs::write(tmp.path().join("src/lib.rs"), "x").expect("write");

        let mut tree = FileTreeState::default();
        tree.rescan(tmp.path());
        assert!(tree.line_count() >= 2);

        let src_idx = tree
            .lines
            .iter()
            .position(|l| l.label == "src")
            .expect("src dir");
        tree.toggle_cursor(src_idx, tmp.path());
        assert!(tree.lines.iter().any(|l| l.label == "lib.rs"));
    }

    #[test]
    fn render_lines_truncates_to_max_cols() {
        let mut tree = FileTreeState::default();
        tree.lines.push(FileTreeLine {
            rel: "libzagens_runtime_adapters.rlib".into(),
            depth: 2,
            is_dir: false,
            label: "libzagens_runtime_adapters.rlib".into(),
        });
        let (text, _) = tree.render_lines(0, 0, 8, 24).remove(0);
        assert!(
            crate::tui::display_format::display_width(&text) <= 24,
            "line wider than pane: {text:?}"
        );
    }
}
