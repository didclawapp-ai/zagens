//! CLI command handlers (`run_*`, config helpers) extracted from `main.rs` (B3).

mod legacy;

pub use legacy::*;
pub use legacy::{resolve_cli_auto_route, CliAutoRoute};
