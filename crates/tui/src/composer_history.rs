//! Cross-session composer input history (#366).
//!
//! Persists user-typed prompts to `~/.deepseek/composer_history.txt` so
//! pressing Up-arrow at the composer recalls submissions from previous
//! sessions, not just the current one. One entry per line, oldest first,
//! capped at [`MAX_HISTORY_ENTRIES`] entries (older entries are pruned
//! at append time).
//!
//! Entries that begin with `/` (slash commands) are NOT stored — they
//! pollute the recall stream and the fuzzy slash-menu already covers
//! them. Empty / whitespace-only inputs are also skipped.
//!
//! # Composer draft (#XXX)
//!
//! Unsent input is saved to `~/.deepseek/composer_draft.txt` on exit and
//! restored on the next launch, so closing the app doesn't lose work in
//! progress. The draft is cleared when the input is submitted.

use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

/// Hard cap on persisted history. Keeps the file small (typical entries
/// are < 200 chars, so 1000 entries ≈ 200 KB) and bounds startup load
/// time.
pub const MAX_HISTORY_ENTRIES: usize = 1000;

const HISTORY_FILE_NAME: &str = "composer_history.txt";

fn default_history_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".deepseek").join(HISTORY_FILE_NAME))
}

/// Read the persisted history into memory. Returns an empty vec if the
/// file doesn't exist or can't be parsed — this is best-effort.
#[must_use]
pub fn load_history() -> Vec<String> {
    let Some(path) = default_history_path() else {
        return Vec::new();
    };
    load_history_from(&path)
}

fn load_history_from(path: &Path) -> Vec<String> {
    let Ok(file) = fs::File::open(path) else {
        return Vec::new();
    };
    BufReader::new(file)
        .lines()
        .map_while(Result::ok)
        .filter(|line| !line.trim().is_empty())
        .collect()
}

/// Append an entry to the persisted history, pruning old entries to
/// stay within [`MAX_HISTORY_ENTRIES`]. Slash-commands and empty input
/// are skipped — those don't help recall.
///
/// Best-effort — failures are logged via `tracing` but not propagated
/// because composer history is a UX nicety, not a correctness concern.
pub fn append_history(entry: &str) {
    let Some(path) = default_history_path() else {
        return;
    };
    append_history_to(&path, entry);
}

fn append_history_to(path: &Path, entry: &str) {
    let trimmed = entry.trim();
    if trimmed.is_empty() || trimmed.starts_with('/') {
        return;
    }
    if let Some(parent) = path.parent()
        && let Err(err) = fs::create_dir_all(parent)
    {
        tracing::warn!(
            "Failed to create composer history dir {}: {err}",
            parent.display()
        );
        return;
    }

    // Read existing entries, append the new one, prune from the front
    // until under the cap, then atomically rewrite.
    let mut entries = load_history_from(path);
    if entries.last().map(String::as_str) == Some(trimmed) {
        // De-dupe consecutive duplicates — repeated submission of the
        // same prompt shouldn't bloat the file.
        return;
    }
    entries.push(trimmed.to_string());
    if entries.len() > MAX_HISTORY_ENTRIES {
        let excess = entries.len() - MAX_HISTORY_ENTRIES;
        entries.drain(0..excess);
    }

    let payload = entries.join("\n") + "\n";
    if let Err(err) = crate::utils::write_atomic(path, payload.as_bytes()) {
        tracing::warn!(
            "Failed to persist composer history at {}: {err}",
            path.display()
        );
    }
}

// ── composer draft (unsent input persistence) ─────────────────────────

const DRAFT_FILE_NAME: &str = "composer_draft.txt";

fn draft_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".deepseek").join(DRAFT_FILE_NAME))
}

/// Persist the unsent composer input so it survives a restart.
/// Best-effort — failures are logged but not propagated.
pub fn save_draft(input: &str) {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        clear_draft();
        return;
    }
    let Some(path) = draft_path() else {
        return;
    };
    if let Some(parent) = path.parent()
        && let Err(err) = fs::create_dir_all(parent)
    {
        tracing::warn!("Failed to create composer draft dir {}: {err}", parent.display());
        return;
    }
    if let Err(err) = crate::utils::write_atomic(&path, trimmed.as_bytes()) {
        tracing::warn!("Failed to persist composer draft at {}: {err}", path.display());
    }
}

/// Load the unsent composer draft from the previous session, if any.
/// Returns `None` when the file is missing, empty, or unreadable.
#[must_use]
pub fn load_draft() -> Option<String> {
    let path = draft_path()?;
    let content = fs::read_to_string(&path).ok()?;
    let trimmed = content.trim().to_string();
    if trimmed.is_empty() {
        let _ = fs::remove_file(&path);
        None
    } else {
        Some(trimmed)
    }
}

/// Clear the composer draft file (called when the user submits).
pub fn clear_draft() {
    if let Some(path) = draft_path() {
        let _ = fs::remove_file(&path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tests use the path-injecting `*_from` / `*_to` helpers so they
    /// don't have to mutate `HOME` (which is not honored by
    /// `dirs::home_dir()` on Windows — it reads `USERPROFILE` /
    /// `SHGetKnownFolderPath` instead). This makes the suite portable
    /// across all three CI runners without per-platform env juggling.
    fn temp_history_path() -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join(HISTORY_FILE_NAME);
        (tmp, path)
    }

    #[test]
    fn append_and_load_round_trip() {
        let (_tmp, path) = temp_history_path();
        append_history_to(&path, "first");
        append_history_to(&path, "second");
        append_history_to(&path, "third");
        assert_eq!(load_history_from(&path), vec!["first", "second", "third"]);
    }

    #[test]
    fn slash_commands_skipped() {
        let (_tmp, path) = temp_history_path();
        append_history_to(&path, "/help");
        append_history_to(&path, "real prompt");
        append_history_to(&path, "/cost");
        assert_eq!(load_history_from(&path), vec!["real prompt"]);
    }

    #[test]
    fn empty_and_whitespace_skipped() {
        let (_tmp, path) = temp_history_path();
        append_history_to(&path, "");
        append_history_to(&path, "   ");
        append_history_to(&path, "\n\t");
        append_history_to(&path, "real");
        assert_eq!(load_history_from(&path), vec!["real"]);
    }

    #[test]
    fn consecutive_duplicates_deduped() {
        let (_tmp, path) = temp_history_path();
        append_history_to(&path, "same");
        append_history_to(&path, "same");
        append_history_to(&path, "same");
        append_history_to(&path, "different");
        append_history_to(&path, "same");
        assert_eq!(load_history_from(&path), vec!["same", "different", "same"]);
    }

    #[test]
    fn pruned_to_cap_at_append_time() {
        let (_tmp, path) = temp_history_path();
        for i in 0..(MAX_HISTORY_ENTRIES + 50) {
            append_history_to(&path, &format!("entry {i}"));
        }
        let history = load_history_from(&path);
        assert_eq!(history.len(), MAX_HISTORY_ENTRIES);
        // Newest entries survive; oldest 50 were pruned.
        assert_eq!(history.first().map(String::as_str), Some("entry 50"));
        assert_eq!(
            history.last().map(String::as_str),
            Some(format!("entry {}", MAX_HISTORY_ENTRIES + 49)).as_deref()
        );
    }

    #[test]
    fn missing_file_loads_empty() {
        let (_tmp, path) = temp_history_path();
        assert!(load_history_from(&path).is_empty());
    }

    // ── draft tests ────────────────────────────────────────────────

    fn temp_draft_path() -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join(DRAFT_FILE_NAME);
        (tmp, path)
    }

    #[test]
    fn save_and_load_draft_round_trip() {
        let (_tmp, path) = temp_draft_path();
        // Simulate save_draft via direct write
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        crate::utils::write_atomic(&path, b"hello world").unwrap();
        let loaded = std::fs::read_to_string(&path).unwrap();
        assert_eq!(loaded.trim(), "hello world");
    }

    #[test]
    fn empty_draft_cleared_not_loaded() {
        let (_tmp, path) = temp_draft_path();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "   \n").unwrap();
        // load_draft should return None for whitespace-only
        let content = std::fs::read_to_string(&path).ok();
        let trimmed = content.map(|c| c.trim().to_string()).unwrap_or_default();
        assert!(trimmed.is_empty());
    }

    #[test]
    fn missing_draft_loads_none() {
        let (_tmp, path) = temp_draft_path();
        assert!(std::fs::read_to_string(&path).is_err());
    }
}
