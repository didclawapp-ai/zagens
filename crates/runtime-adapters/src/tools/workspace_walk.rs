//! Shared workspace directory walking (ripgrep-style defaults).
//!
//! Uses the same `ignore` crate as ripgrep: honors `.gitignore` / `.ignore`,
//! skips common build/vendor directory names. Symlinks are **not** followed
//! (C5) — matching ripgrep's own default — so a symlink inside the workspace
//! pointing outside it can't be used to read files beyond the workspace
//! boundary via grep / glob / file_search / project_tree.

use ignore::WalkBuilder;
use std::path::{Path, PathBuf};

/// Directory names skipped even when not listed in `.gitignore` (heavy trees).
pub const SKIP_DIR_NAMES: &[&str] = &[
    "target",
    "node_modules",
    ".git",
    "dist",
    "build",
    "__pycache__",
    ".venv",
    "venv",
    ".turbo",
    ".next",
];

/// Apply workspace walk settings aligned with `file_search` / ripgrep defaults.
pub fn configure_workspace_walk(builder: &mut WalkBuilder, respect_gitignore: bool) {
    builder
        .hidden(false)
        // C5: do not follow symlinks — a workspace-internal symlink to an
        // external path would otherwise let search tools read outside the
        // workspace boundary (the per-file path is not re-checked downstream).
        .follow_links(false)
        .require_git(false)
        .git_ignore(respect_gitignore)
        .git_global(respect_gitignore)
        .ignore(respect_gitignore)
        .parents(respect_gitignore)
        .filter_entry(|entry| {
            if entry.file_type().is_some_and(|ft| ft.is_dir()) {
                let name = entry.file_name().to_string_lossy();
                return !SKIP_DIR_NAMES.contains(&name.as_ref());
            }
            true
        });
}

/// Collect every file under `root` using workspace walk rules.
pub fn collect_workspace_files(root: &Path, respect_gitignore: bool) -> Vec<PathBuf> {
    if root.is_file() {
        return vec![root.to_path_buf()];
    }
    if !root.is_dir() {
        return Vec::new();
    }

    let mut builder = WalkBuilder::new(root);
    configure_workspace_walk(&mut builder, respect_gitignore);
    let walker = builder.build();

    let mut files = Vec::new();
    for entry in walker.flatten() {
        if entry.file_type().is_some_and(|ft| ft.is_file()) {
            files.push(entry.into_path());
        }
    }
    files
}

/// ripgrep-style binary sniff: NUL byte in the first 8 KiB ⇒ treat as binary.
///
/// Exception: files starting with a UTF-16 (LE/BE) or UTF-8 BOM are treated as
/// text. UTF-16-encoded text is full of NUL bytes (every other byte for ASCII
/// content), so the bare NUL heuristic would wrongly flag it as binary and a
/// grep over a UTF-16 source file (common on Chinese Windows) would find
/// nothing. `detect_and_decode` handles the actual decode downstream.
pub fn is_probably_binary(path: &Path) -> bool {
    use std::io::Read;

    const SNIFF_LEN: usize = 8 * 1024;

    let Ok(mut file) = std::fs::File::open(path) else {
        return true;
    };
    let mut buf = [0u8; SNIFF_LEN];
    let Ok(n) = file.read(&mut buf) else {
        return true;
    };
    let head = &buf[..n];
    if head.starts_with(&[0xFF, 0xFE]) // UTF-16 LE BOM
        || head.starts_with(&[0xFE, 0xFF]) // UTF-16 BE BOM
        || head.starts_with(&[0xEF, 0xBB, 0xBF])
    // UTF-8 BOM
    {
        return false;
    }
    head.contains(&0)
}
