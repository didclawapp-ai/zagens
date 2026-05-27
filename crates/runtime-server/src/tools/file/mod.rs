//! File system tools: `read_file`, `write_file`, `edit_file`, `list_dir`
//!
//! These tools provide safe file system operations within the workspace,
//! with path validation to prevent escaping the workspace boundary.

mod edit;
mod list_dir;
mod read;
mod write;

pub use edit::EditFileTool;
pub use list_dir::ListDirTool;
pub use read::{ReadFileTool, sniff_encoding_label};
pub use write::WriteFileTool;

pub(crate) const MAX_FILE_SIZE: u64 = 100 * 1024 * 1024;
pub(crate) const FILE_SIZE_LINE_COUNT_LIMIT: u64 = 10 * 1024 * 1024;
pub(crate) const DEFAULT_LIMIT: usize = 2000;
pub(crate) const MAX_LIMIT: usize = 5000;

#[cfg(test)]
mod tests;
