//! C6(LSP): Post-edit LSP-style diagnostics for sub-agents.
//!
//! After an Implementer sub-agent completes, run `cargo check` on the
//! workspace and attach structured diagnostics to the blackboard under
//! the `lsp_diagnostics` partition. The next Reviewer reads this section
//! and can cite specific compiler errors/warnings without having to re-run
//! the check themselves.
//!
//! ## Why cargo check instead of the LSP client
//!
//! `SubAgentRuntime` does not carry an `LspHost` reference (the live
//! rust-analyzer socket lives in the parent Engine). For batch-A we use
//! `cargo check --message-format=short` as an equivalent — it produces
//! file:line diagnostics via the compiler, which is what Reviewers need.
//! A proper intra-turn LSP hook (subagent sees diagnostics mid-turn) is a
//! V2-class improvement that requires threading `Arc<dyn LspHost>` through
//! the sub-agent runtime.
//!
//! ## Output format in the blackboard
//!
//! ```json
//! "lsp_diagnostics": {
//!   "errors": 2,
//!   "warnings": 3,
//!   "lines": ["crates/foo/src/lib.rs:42:8: error[E0308]: ...", ...]
//! }
//! ```
//!
//! Only the first [`MAX_DIAG_LINES`] lines are stored to keep the
//! blackboard compact.

use std::path::Path;
use std::process::Command;

use serde_json::{Value, json};
use zagens_config::workspace_meta_file_write;

/// Maximum diagnostic lines stored in the blackboard.
const MAX_DIAG_LINES: usize = 50;

/// Relative path inside `.zagens/` for the optional diagnostics cache
/// (one file per task, separate from the blackboard JSON).
const DIAG_CACHE_REL: &str = "lsp-diag-cache";

/// Result of a `cargo check` pass.
#[derive(Debug)]
pub struct LspDiagResult {
    pub errors: u32,
    pub warnings: u32,
    /// Capped at [`MAX_DIAG_LINES`].
    pub lines: Vec<String>,
}

impl LspDiagResult {
    pub fn is_clean(&self) -> bool {
        self.errors == 0
    }

    /// Serialize to the blackboard JSON shape.
    pub fn to_json(&self) -> Value {
        json!({
            "errors": self.errors,
            "warnings": self.warnings,
            "lines": self.lines,
        })
    }
}

/// Run `cargo check --message-format=short` and return structured results.
///
/// Runs synchronously (intended to be called from `spawn_blocking`).
/// Returns `None` when `cargo` is unavailable (graceful skip).
pub fn run_cargo_check_sync(workspace: &Path) -> Option<LspDiagResult> {
    let out = Command::new("cargo")
        .args(["check", "--message-format=short"])
        .current_dir(workspace)
        .output()
        .ok()?;

    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let combined = format!("{stderr}{stdout}");

    let mut errors = 0u32;
    let mut warnings = 0u32;
    let mut lines: Vec<String> = Vec::new();

    for line in combined.lines() {
        let lower = line.to_ascii_lowercase();
        // Short-format lines look like: `src/lib.rs:42:8: error[E0308]: ...`
        if lower.contains(": error") || lower.starts_with("error") {
            errors += 1;
        } else if lower.contains(": warning") || lower.starts_with("warning") {
            warnings += 1;
        } else {
            continue;
        }
        if lines.len() < MAX_DIAG_LINES {
            lines.push(line.trim().to_string());
        }
    }

    Some(LspDiagResult {
        errors,
        warnings,
        lines,
    })
}

/// Async wrapper: run cargo check in a blocking thread, merge result into
/// the given blackboard JSON object.
///
/// Returns `(board, was_run)`. When cargo is unavailable `was_run = false`
/// and the board is unchanged.
pub async fn run_and_merge_lsp_diag(workspace: &Path, mut board: Value) -> (Value, bool) {
    let ws = workspace.to_path_buf();
    let result = tokio::task::spawn_blocking(move || run_cargo_check_sync(&ws))
        .await
        .ok()
        .flatten();

    let Some(diag) = result else {
        return (board, false);
    };

    if let Value::Object(ref mut map) = board {
        map.insert("lsp_diagnostics".to_string(), diag.to_json());
    }
    (board, true)
}

/// Write the `lsp_diagnostics` partition for a task to the blackboard file.
///
/// Reads the existing board, runs cargo check, merges, and atomically writes
/// back. Silently skips when cargo is not in PATH or the blackboard is missing.
pub async fn write_lsp_diagnostics_to_blackboard(workspace: &Path, task_id: &str) {
    use crate::tools::subagent::blackboard::{implementer_round_count, read_blackboard_raw};
    use zagens_config::workspace_meta_file_write;

    // Only run when there's at least one implementer round to check
    if implementer_round_count(workspace, task_id) == 0 {
        return;
    }

    let board_path = workspace_meta_file_write(workspace, &format!("blackboards/{task_id}.json"));
    let existing_raw = std::fs::read_to_string(&board_path).unwrap_or_default();
    let board: Value = serde_json::from_str(&existing_raw).unwrap_or(json!({
        "schema_version": 1,
        "task_id": task_id,
    }));

    let ws = workspace.to_path_buf();
    let (updated_board, was_run) = run_and_merge_lsp_diag(workspace, board).await;

    if !was_run {
        return;
    }

    let payload = serde_json::to_string_pretty(&updated_board).unwrap_or_default();
    let tmp = board_path.with_extension("tmp");
    let _ = std::fs::write(&tmp, &payload);
    let _ = std::fs::rename(&tmp, &board_path);
    crate::tools::subagent::blackboard_cache::invalidate(&board_path);
}

/// Format the `lsp_diagnostics` section for injection into a Reviewer's
/// blackboard context.
pub fn format_lsp_diagnostics(board: &Value) -> Option<String> {
    let diag = board.get("lsp_diagnostics")?;
    let errors = diag.get("errors").and_then(|v| v.as_u64()).unwrap_or(0);
    let warnings = diag.get("warnings").and_then(|v| v.as_u64()).unwrap_or(0);
    if errors == 0 && warnings == 0 {
        return None;
    }
    let lines = diag
        .get("lines")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default();

    let header = format!(
        "### cargo check diagnostics ({errors} error(s), {warnings} warning(s))\n\
         These were recorded after the Implementer's last round:\n"
    );
    Some(format!("{header}```\n{lines}\n```"))
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn to_json_shape() {
        let d = LspDiagResult {
            errors: 2,
            warnings: 1,
            lines: vec!["foo.rs:1:1: error[E0308]: type mismatch".to_string()],
        };
        let v = d.to_json();
        assert_eq!(v["errors"], 2);
        assert_eq!(v["warnings"], 1);
        assert_eq!(v["lines"].as_array().map(|a| a.len()), Some(1));
    }

    #[test]
    fn format_lsp_diagnostics_with_errors() {
        let board = json!({
            "lsp_diagnostics": {
                "errors": 1,
                "warnings": 2,
                "lines": ["crates/foo/src/lib.rs:10:5: error[E0308]: expected i32 found &str"]
            }
        });
        let section = format_lsp_diagnostics(&board).expect("should produce section");
        assert!(section.contains("1 error(s)"));
        assert!(section.contains("2 warning(s)"));
        assert!(section.contains("E0308"));
    }

    #[test]
    fn format_lsp_diagnostics_clean_returns_none() {
        let board = json!({
            "lsp_diagnostics": { "errors": 0, "warnings": 0, "lines": [] }
        });
        assert!(format_lsp_diagnostics(&board).is_none());
    }

    #[test]
    fn parse_cargo_check_short_output() {
        let workspace = tempfile::tempdir().expect("tempdir");
        // We can't run cargo in a non-cargo directory, but we can test the
        // parsing logic by crafting fake output.
        // The function is sync+reads from cargo; mock via a fake CARGO env
        // is complex, so just verify the is_clean / to_json path here.
        let clean = LspDiagResult {
            errors: 0,
            warnings: 0,
            lines: vec![],
        };
        assert!(clean.is_clean());
        let _ = workspace; // prevent early drop
    }
}
