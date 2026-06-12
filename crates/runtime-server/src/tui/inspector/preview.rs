//! Read-only file preview for the Files inspector tab.

use std::fs;
use std::path::Path;

const MAX_PREVIEW_BYTES: u64 = 32_768;
const MAX_PREVIEW_LINES: usize = 200;

pub fn read_text_preview(workspace: &Path, rel: &str) -> Vec<String> {
    let rel = rel.trim();
    if rel.is_empty() {
        return vec!["(select a file)".to_string()];
    }
    let path = workspace.join(rel);
    let meta = match fs::metadata(&path) {
        Ok(m) => m,
        Err(e) => return vec![format!("cannot read {}: {e}", rel)],
    };
    if !meta.is_file() {
        return vec![format!("not a file: {rel}")];
    }
    if meta.len() > MAX_PREVIEW_BYTES {
        return vec![
            format!("{rel}"),
            format!(
                "(file too large: {} bytes; max {MAX_PREVIEW_BYTES})",
                meta.len()
            ),
        ];
    }
    let raw = match fs::read(&path) {
        Ok(b) => b,
        Err(e) => return vec![format!("read failed: {e}")],
    };
    if raw.contains(&0) {
        return vec![
            format!("{rel}"),
            "(binary file — preview skipped)".to_string(),
        ];
    }
    let text = String::from_utf8_lossy(&raw);
    let mut lines: Vec<String> = text
        .lines()
        .take(MAX_PREVIEW_LINES)
        .map(ToString::to_string)
        .collect();
    if text.lines().count() > MAX_PREVIEW_LINES {
        lines.push(format!("… (truncated at {MAX_PREVIEW_LINES} lines)"));
    }
    if lines.is_empty() {
        lines.push("(empty file)".to_string());
    }
    lines
}
