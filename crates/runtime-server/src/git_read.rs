//! Read-only git helpers for workspace status / changes / file diff.
//! Shared by Runtime API (Diff thin layer) and TUI inspector.

use std::path::{Component, Path, PathBuf};
use std::process::Command;

pub const MAX_CHANGES: usize = 500;
pub const MAX_DIFF_CHARS: usize = 40_000;
pub const MAX_DIFF_LINES: usize = 400;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitStatusCounts {
    pub git_repo: bool,
    pub branch: Option<String>,
    pub staged: usize,
    pub unstaged: usize,
    pub untracked: usize,
    pub ahead: Option<u32>,
    pub behind: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitChangeEntry {
    pub path: String,
    /// Index (staged) status char, space if clean.
    pub index_status: char,
    /// Worktree status char, space if clean.
    pub worktree_status: char,
    /// Coarse kind for UI badges.
    pub kind: GitChangeKind,
    /// For renames/copies: previous path when present.
    pub old_path: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitChangeKind {
    Modified,
    Added,
    Deleted,
    Renamed,
    Copied,
    Untracked,
    Conflict,
    TypeChange,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitFileDiff {
    pub path: String,
    pub staged: bool,
    pub diff_text: String,
    pub truncated: bool,
    pub binary: bool,
    pub untracked: bool,
}

/// Run `git` in `workspace`; returns stdout on success.
pub fn run_git(workspace: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(workspace)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

pub fn collect_status(workspace: &Path) -> GitStatusCounts {
    let mut status = GitStatusCounts {
        git_repo: false,
        branch: None,
        staged: 0,
        unstaged: 0,
        untracked: 0,
        ahead: None,
        behind: None,
    };

    let Some(repo_check) = run_git(workspace, &["rev-parse", "--is-inside-work-tree"]) else {
        return status;
    };
    if repo_check.trim() != "true" {
        return status;
    }

    status.git_repo = true;
    status.branch = run_git(workspace, &["rev-parse", "--abbrev-ref", "HEAD"])
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    if let Some(porcelain) = run_git(workspace, &["status", "--porcelain=v1"]) {
        for line in porcelain.lines() {
            if line.starts_with("??") {
                status.untracked += 1;
                continue;
            }
            let chars: Vec<char> = line.chars().collect();
            if chars.len() >= 2 {
                if chars[0] != ' ' {
                    status.staged += 1;
                }
                if chars[1] != ' ' {
                    status.unstaged += 1;
                }
            }
        }
    }

    if let Some(counts) = run_git(
        workspace,
        &["rev-list", "--left-right", "--count", "@{upstream}...HEAD"],
    ) {
        let mut parts = counts.split_whitespace();
        if let (Some(behind), Some(ahead)) = (parts.next(), parts.next()) {
            status.behind = behind.parse::<u32>().ok();
            status.ahead = ahead.parse::<u32>().ok();
        }
    }

    status
}

pub fn collect_changes(workspace: &Path, limit: usize) -> Vec<GitChangeEntry> {
    let Some(porcelain) = run_git(workspace, &["status", "--porcelain=v1"]) else {
        return Vec::new();
    };
    parse_porcelain(&porcelain, limit)
}

/// Parse `git status --porcelain=v1` lines into change entries.
pub fn parse_porcelain(porcelain: &str, limit: usize) -> Vec<GitChangeEntry> {
    let mut out = Vec::new();
    for line in porcelain.lines() {
        if out.len() >= limit {
            break;
        }
        if line.len() < 2 {
            continue;
        }
        let mut chars = line.chars();
        let index_status = chars.next().unwrap_or(' ');
        let worktree_status = chars.next().unwrap_or(' ');
        let rest = line.get(2..).unwrap_or("").trim_start();
        if rest.is_empty() {
            continue;
        }

        let (path, old_path) = if let Some((left, right)) = rest.split_once('\t') {
            // rename/copy: ORIG_PATH \t PATH
            (right.trim().to_string(), Some(left.trim().to_string()))
        } else if let Some((left, right)) = rest.split_once(" -> ") {
            (right.trim().to_string(), Some(left.trim().to_string()))
        } else {
            (rest.to_string(), None)
        };

        let kind = classify_change(index_status, worktree_status, old_path.is_some());
        out.push(GitChangeEntry {
            path,
            index_status,
            worktree_status,
            kind,
            old_path,
        });
    }
    out
}

fn classify_change(index: char, worktree: char, has_old: bool) -> GitChangeKind {
    if index == '?' && worktree == '?' {
        return GitChangeKind::Untracked;
    }
    if index == 'U'
        || worktree == 'U'
        || (index == 'A' && worktree == 'A')
        || (index == 'D' && worktree == 'D')
    {
        return GitChangeKind::Conflict;
    }
    if has_old || index == 'R' || worktree == 'R' {
        return GitChangeKind::Renamed;
    }
    if index == 'C' || worktree == 'C' {
        return GitChangeKind::Copied;
    }
    if index == 'A' || worktree == 'A' {
        return GitChangeKind::Added;
    }
    if index == 'D' || worktree == 'D' {
        return GitChangeKind::Deleted;
    }
    if index == 'T' || worktree == 'T' {
        return GitChangeKind::TypeChange;
    }
    if index == 'M' || worktree == 'M' {
        return GitChangeKind::Modified;
    }
    GitChangeKind::Other
}

impl GitChangeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Modified => "modified",
            Self::Added => "added",
            Self::Deleted => "deleted",
            Self::Renamed => "renamed",
            Self::Copied => "copied",
            Self::Untracked => "untracked",
            Self::Conflict => "conflict",
            Self::TypeChange => "typechange",
            Self::Other => "other",
        }
    }
}

/// Validate a workspace-relative path (may not exist — deleted/untracked).
pub fn validate_rel_path(workspace: &Path, rel: &str) -> Result<String, String> {
    let base = workspace
        .canonicalize()
        .map_err(|e| format!("workspace: {e}"))?;
    let trimmed = rel.trim().trim_start_matches(['/', '\\']);
    if trimmed.is_empty() {
        return Err("path required".into());
    }
    if trimmed.contains('\0') {
        return Err("invalid path".into());
    }
    let rel_pb = PathBuf::from(trimmed);
    if rel_pb.is_absolute() {
        return Err("path must be relative to workspace".into());
    }
    for c in rel_pb.components() {
        if matches!(c, Component::ParentDir) {
            return Err("invalid path".into());
        }
    }
    let candidate = base.join(&rel_pb);
    // Lexical containment: normalize `..` already rejected; also check prefix after
    // optional canonicalize when the path exists.
    if let Ok(canon) = candidate.canonicalize() {
        if !canon.starts_with(&base) {
            return Err("path outside workspace".into());
        }
    } else {
        // Deleted / not yet created: ensure parent chain stays under base.
        let mut check = candidate.as_path();
        loop {
            if check.starts_with(&base) {
                break;
            }
            match check.parent() {
                Some(p) if p != check => check = p,
                _ => return Err("path outside workspace".into()),
            }
        }
    }
    Ok(trimmed.replace('\\', "/"))
}

/// Unified diff for one path. Prefer worktree (`staged=false`) or index (`staged=true`).
pub fn file_diff(workspace: &Path, rel_path: &str, staged: bool) -> Result<GitFileDiff, String> {
    let path = validate_rel_path(workspace, rel_path)?;
    let untracked = is_untracked(workspace, &path);

    if staged {
        return diff_cached(workspace, &path, untracked);
    }

    // Worktree vs index
    if let Some(text) = run_git(
        workspace,
        &["diff", "--no-color", "--no-ext-diff", "--", &path],
    ) && !text.trim().is_empty()
    {
        return Ok(truncate_diff(path, staged, text, false, untracked));
    }

    // Tracked but only staged changes, or deleted from worktree already staged
    if let Some(text) = run_git(
        workspace,
        &[
            "diff",
            "--cached",
            "--no-color",
            "--no-ext-diff",
            "--",
            &path,
        ],
    ) && !text.trim().is_empty()
    {
        return Ok(truncate_diff(path, true, text, false, untracked));
    }

    if untracked {
        return diff_untracked(workspace, &path);
    }

    Ok(GitFileDiff {
        path,
        staged,
        diff_text: "(no diff hunks)\n".into(),
        truncated: false,
        binary: false,
        untracked: false,
    })
}

fn is_untracked(workspace: &Path, path: &str) -> bool {
    let Some(out) = run_git(workspace, &["status", "--porcelain=v1", "--", path]) else {
        return false;
    };
    out.lines().any(|l| l.starts_with("??"))
}

fn diff_cached(workspace: &Path, path: &str, untracked: bool) -> Result<GitFileDiff, String> {
    let text = run_git(
        workspace,
        &[
            "diff",
            "--cached",
            "--no-color",
            "--no-ext-diff",
            "--",
            path,
        ],
    )
    .unwrap_or_default();
    if text.trim().is_empty() && untracked {
        return diff_untracked(workspace, path);
    }
    Ok(truncate_diff(
        path.to_string(),
        true,
        text,
        false,
        untracked,
    ))
}

fn diff_untracked(workspace: &Path, path: &str) -> Result<GitFileDiff, String> {
    // `git diff --no-index` exits 1 when files differ — capture stdout anyway.
    let null = if cfg!(windows) { "NUL" } else { "/dev/null" };
    let output = Command::new("git")
        .args([
            "diff",
            "--no-color",
            "--no-ext-diff",
            "--no-index",
            "--",
            null,
            path,
        ])
        .current_dir(workspace)
        .output()
        .map_err(|e| format!("git not available: {e}"))?;
    let text = String::from_utf8_lossy(&output.stdout).to_string();
    if text.trim().is_empty() {
        // Binary or empty: probe
        let abs = workspace.join(path);
        if abs.is_file() {
            let meta = std::fs::metadata(&abs).map_err(|e| e.to_string())?;
            if meta.len() > MAX_DIFF_CHARS as u64 {
                return Ok(GitFileDiff {
                    path: path.to_string(),
                    staged: false,
                    diff_text: format!("Binary or large untracked file ({path})\n"),
                    truncated: true,
                    binary: true,
                    untracked: true,
                });
            }
            match std::fs::read_to_string(&abs) {
                Ok(content) => {
                    let mut diff = format!("--- /dev/null\n+++ b/{path}\n@@ -0,0 +1,");
                    let lines: Vec<&str> = content.lines().collect();
                    diff.push_str(&format!("{} @@\n", lines.len().max(1)));
                    for line in &lines {
                        diff.push('+');
                        diff.push_str(line);
                        diff.push('\n');
                    }
                    if content.is_empty() {
                        diff.push('+');
                        diff.push('\n');
                    }
                    return Ok(truncate_diff(path.to_string(), false, diff, false, true));
                }
                Err(_) => {
                    return Ok(GitFileDiff {
                        path: path.to_string(),
                        staged: false,
                        diff_text: format!("Binary untracked file ({path})\n"),
                        truncated: false,
                        binary: true,
                        untracked: true,
                    });
                }
            }
        }
        return Ok(GitFileDiff {
            path: path.to_string(),
            staged: false,
            diff_text: "(no diff hunks)\n".into(),
            truncated: false,
            binary: false,
            untracked: true,
        });
    }
    if text.contains("Binary files") || text.contains("GIT binary patch") {
        return Ok(GitFileDiff {
            path: path.to_string(),
            staged: false,
            diff_text: text,
            truncated: false,
            binary: true,
            untracked: true,
        });
    }
    Ok(truncate_diff(path.to_string(), false, text, false, true))
}

fn truncate_diff(
    path: String,
    staged: bool,
    text: String,
    binary: bool,
    untracked: bool,
) -> GitFileDiff {
    let binary = binary || text.contains("Binary files") || text.contains("GIT binary patch");
    let line_count = text.lines().count();
    let mut truncated = false;
    let mut diff_text = text;
    if line_count > MAX_DIFF_LINES {
        diff_text = diff_text
            .lines()
            .take(MAX_DIFF_LINES)
            .collect::<Vec<_>>()
            .join("\n");
        diff_text.push_str(&format!("\n… (truncated at {MAX_DIFF_LINES} lines)\n"));
        truncated = true;
    }
    if diff_text.len() > MAX_DIFF_CHARS {
        let mut end = MAX_DIFF_CHARS;
        while end > 0 && !diff_text.is_char_boundary(end) {
            end -= 1;
        }
        diff_text.truncate(end);
        diff_text.push_str("\n… (truncated)\n");
        truncated = true;
    }
    GitFileDiff {
        path,
        staged,
        diff_text,
        truncated,
        binary,
        untracked,
    }
}

/// Line-oriented patch for TUI (shared with inspector).
pub fn git_diff_patch_lines(
    workspace: &Path,
    staged: bool,
    rel_path: &str,
    max_lines: usize,
) -> Vec<String> {
    match file_diff(workspace, rel_path, staged) {
        Ok(d) => {
            let mut lines: Vec<String> = d.diff_text.lines().map(ToString::to_string).collect();
            if lines.len() > max_lines {
                lines.truncate(max_lines);
                lines.push(format!("… (truncated at {max_lines} lines)"));
            }
            if lines.is_empty() {
                lines.push("(no diff hunks)".into());
            }
            lines
        }
        Err(e) => vec![e],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;

    fn init_repo(dir: &Path) {
        assert!(
            Command::new("git")
                .args(["init"])
                .current_dir(dir)
                .status()
                .unwrap()
                .success()
        );
        let _ = Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(dir)
            .status();
        let _ = Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(dir)
            .status();
    }

    #[test]
    fn parse_porcelain_untracked_and_modified() {
        let text = " M src/a.rs\n?? new.txt\nA  staged.txt\n";
        let entries = parse_porcelain(text, 50);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].path, "src/a.rs");
        assert_eq!(entries[0].kind, GitChangeKind::Modified);
        assert_eq!(entries[1].kind, GitChangeKind::Untracked);
        assert_eq!(entries[2].kind, GitChangeKind::Added);
        assert_eq!(entries[2].index_status, 'A');
    }

    #[test]
    fn parse_porcelain_rename() {
        let text = "R  old.rs\tnew.rs\n";
        let entries = parse_porcelain(text, 10);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "new.rs");
        assert_eq!(entries[0].old_path.as_deref(), Some("old.rs"));
        assert_eq!(entries[0].kind, GitChangeKind::Renamed);
    }

    #[test]
    fn validate_rel_path_rejects_parent() {
        let dir = tempfile::tempdir().unwrap();
        let err = validate_rel_path(dir.path(), "../escape").unwrap_err();
        assert!(err.contains("invalid"));
    }

    #[test]
    fn collect_status_and_file_diff_on_fixture() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        fs::write(dir.path().join("tracked.txt"), "one\n").unwrap();
        assert!(
            Command::new("git")
                .args(["add", "tracked.txt"])
                .current_dir(dir.path())
                .status()
                .unwrap()
                .success()
        );
        assert!(
            Command::new("git")
                .args(["commit", "-m", "init"])
                .current_dir(dir.path())
                .status()
                .unwrap()
                .success()
        );
        fs::write(dir.path().join("tracked.txt"), "two\n").unwrap();
        fs::write(dir.path().join("fresh.txt"), "new\n").unwrap();

        let status = collect_status(dir.path());
        assert!(status.git_repo);
        assert!(status.unstaged >= 1 || status.untracked >= 1);

        let changes = collect_changes(dir.path(), 50);
        assert!(changes.iter().any(|c| c.path == "tracked.txt"));
        assert!(changes.iter().any(|c| c.path == "fresh.txt"));

        let diff = file_diff(dir.path(), "tracked.txt", false).unwrap();
        assert!(diff.diff_text.contains("two") || diff.diff_text.contains("+two"));
    }
}
