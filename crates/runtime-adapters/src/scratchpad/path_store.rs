//! Path-based scratchpad read helpers (no tool runtime context).

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::scratchpad::schema::{Inventory, NoteLine, parse_note_line};

/// Workspace-relative display path for a run directory.
#[must_use]
pub fn display_run_path(run_id: &str) -> String {
    format!(".deepseek/scratchpad/{run_id}")
}

fn validate_run_id(run_id: &str) -> bool {
    let run_id = run_id.trim();
    !run_id.is_empty()
        && !run_id.contains("..")
        && !run_id.contains('/')
        && !run_id.contains('\\')
}

/// Open an existing scratchpad run directory under `workspace`.
#[must_use]
pub fn try_open_run_dir(workspace: &Path, run_id: Option<&str>) -> Option<(PathBuf, String)> {
    let run_id = run_id?.trim();
    if !validate_run_id(run_id) {
        return None;
    }
    let dir = workspace.join(".deepseek/scratchpad").join(run_id);
    if dir.is_dir() {
        Some((dir, run_id.to_string()))
    } else {
        None
    }
}

#[must_use]
pub fn read_inventory(run_dir: &Path) -> Option<Inventory> {
    let raw = std::fs::read_to_string(run_dir.join("inventory.json")).ok()?;
    serde_json::from_str(&raw).ok()
}

#[must_use]
pub fn read_notes(run_dir: &Path) -> Option<Vec<NoteLine>> {
    let path = run_dir.join("notes.jsonl");
    if !path.exists() {
        return Some(Vec::new());
    }
    let raw = std::fs::read_to_string(path).ok()?;
    let mut notes = Vec::new();
    for (idx, line) in raw.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let value: Value = serde_json::from_str(trimmed).ok()?;
        notes.push(parse_note_line(&value, idx + 1));
    }
    Some(notes)
}
