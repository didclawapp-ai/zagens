//! Portable tool helpers and host ports (D16 E1-a3+).

pub mod arg_repair;
pub mod diff_format;
pub mod host;
pub mod path;
pub mod schema_sanitize;
pub mod workspace_walk;

pub use arg_repair::{ArgRepairError, repair};
pub use diff_format::make_unified_diff;
pub use host::{
    RuntimeToolHostWire, ToolAutomationHost, ToolProgressEmit, ToolShellEnvHost, ToolTaskHost,
};
pub use path::path_has_prefix;
pub use schema_sanitize::sanitize;
pub use workspace_walk::{
    SKIP_DIR_NAMES, collect_workspace_files, configure_workspace_walk, is_probably_binary,
};
