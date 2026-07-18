//! Git diff list + per-file patch preview for the Diff inspector tab.

use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffEntry {
    pub path: String,
    pub summary: String,
}

#[derive(Debug, Clone, Default)]
pub struct DiffPanelState {
    pub header: String,
    pub entries: Vec<DiffEntry>,
    pub error: Option<String>,
}

impl DiffPanelState {
    pub fn line_count(&self) -> usize {
        if self.entries.is_empty() {
            1
        } else {
            self.entries.len()
        }
    }

    pub fn entry_path(&self, cursor: usize) -> Option<&str> {
        self.entries.get(cursor).map(|e| e.path.as_str())
    }
}

pub fn load_diff_panel(workspace: &Path, staged: bool) -> DiffPanelState {
    let header = if staged {
        "[staged] git diff --cached"
    } else {
        "[worktree] git diff"
    };
    let mut args = vec!["diff", "--numstat", "--no-color"];
    if staged {
        args.push("--cached");
    }
    let output = Command::new("git")
        .args(&args)
        .current_dir(workspace)
        .output();
    match output {
        Ok(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout);
            let entries = parse_numstat(&text);
            DiffPanelState {
                header: header.to_string(),
                entries,
                error: None,
            }
        }
        Ok(out) => DiffPanelState {
            header: header.to_string(),
            entries: Vec::new(),
            error: Some(format!(
                "git diff failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            )),
        },
        Err(e) => DiffPanelState {
            header: header.to_string(),
            entries: Vec::new(),
            error: Some(format!("git not available: {e}")),
        },
    }
}

pub fn git_diff_patch(
    workspace: &Path,
    staged: bool,
    rel_path: &str,
    max_lines: usize,
) -> Vec<String> {
    crate::git_read::git_diff_patch_lines(workspace, staged, rel_path, max_lines)
}

fn parse_numstat(text: &str) -> Vec<DiffEntry> {
    let mut entries = Vec::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let Some((added, rest)) = line.split_once('\t') else {
            continue;
        };
        let Some((removed, path)) = rest.split_once('\t') else {
            continue;
        };
        if path.is_empty() {
            continue;
        }
        entries.push(DiffEntry {
            path: path.to_string(),
            summary: format!("+{added} -{removed}"),
        });
    }
    entries
}

/// Legacy flat lines for tests / fallback display.
pub fn git_diff_stat(workspace: &Path, staged: bool) -> Vec<String> {
    let panel = load_diff_panel(workspace, staged);
    let mut lines = vec![panel.header.clone()];
    if let Some(err) = panel.error {
        lines.push(err);
        return lines;
    }
    if panel.entries.is_empty() {
        lines.push("(clean)".to_string());
        return lines;
    }
    for entry in &panel.entries {
        lines.push(format!("  {}  {}", entry.summary, entry.path));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_numstat_builds_entries() {
        let text = "3\t1\tfoo.rs\n0\t12\tbar.txt\n";
        let entries = parse_numstat(text);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].path, "foo.rs");
        assert_eq!(entries[0].summary, "+3 -1");
    }

    #[test]
    fn header_reflects_staged_mode() {
        let lines = git_diff_stat(Path::new("."), true);
        assert!(lines.first().is_some_and(|l| l.contains("staged")));
    }
}
