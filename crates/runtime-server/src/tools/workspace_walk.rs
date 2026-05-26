//! Shared workspace directory walking (ripgrep-style defaults).
//!
//! Uses the same `ignore` crate as ripgrep: honors `.gitignore` / `.ignore`,
//! skips common build/vendor directory names, and follows symlinks.

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
        .follow_links(true)
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
    buf[..n].contains(&0)
}
