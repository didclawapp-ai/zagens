//! File system tools: `read_file`, `write_file`, `edit_file`, `list_dir`
//!
//! These tools provide safe file system operations within the workspace,
//! with path validation to prevent escaping the workspace boundary.

mod batch_edit;
mod edit;
mod glob_targets;
mod import_rewrite;
mod list_dir;
mod path_input;
mod read;
mod refactor_imports;
mod restore_file;
mod schemas;
mod search_replace;
mod write;

pub use batch_edit::BatchEditTool;
pub use refactor_imports::RefactorImportsTool;
pub use restore_file::RestoreFileTool;

pub use edit::EditFileTool;
pub use list_dir::ListDirTool;
pub use read::{ReadFileTool, sniff_encoding_label};
pub use write::WriteFileTool;

// Shared file-IO helpers reused by other write paths (apply_patch) and search.
pub(crate) use read::{detect_and_decode, read_pdf};
pub(crate) use write::{
    DecodedFile, atomic_write, encode_text, normalize_line_endings, read_decoded_for_edit,
};

/// Detect a text's dominant line ending. CRLF when any `\r\n` is present.
pub(crate) fn line_ending_of(text: &str) -> &'static str {
    if text.contains("\r\n") { "\r\n" } else { "\n" }
}

pub(crate) const MAX_FILE_SIZE: u64 = 100 * 1024 * 1024;
pub(crate) const FILE_SIZE_LINE_COUNT_LIMIT: u64 = 10 * 1024 * 1024;
pub(crate) const DEFAULT_LIMIT: usize = 2000;
pub(crate) const MAX_LIMIT: usize = 5000;

/// Upper bound on `write_file` `content` size — mirrors `read_file`'s
/// `MAX_FILE_SIZE` so the two tools agree on what counts as "too large".
pub(crate) const MAX_WRITE_SIZE: usize = 100 * 1024 * 1024;
/// When either the prior file or the new content exceeds this, skip the
/// (potentially O(N·D)) unified-diff computation and emit a compact summary
/// instead — keeps large writes cheap and avoids flooding the model context.
pub(crate) const DIFF_MAX_INPUT_BYTES: usize = 256 * 1024;
/// Max files per `batch_edit` call (TS-07).
pub(crate) const MAX_BATCH_FILES: usize = 32;
/// Max combined before+after bytes across all planned edits in one batch.
pub(crate) const MAX_BATCH_DIFF_BYTES: usize = 512 * 1024;
/// Number of leading lines echoed back as a preview when full diff is skipped.
pub(crate) const WRITE_PREVIEW_LINES: usize = 20;

#[cfg(test)]
mod tests;
