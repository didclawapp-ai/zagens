//! LSP configuration — shared between core and TUI.
//!
//! M3 also moved the diagnostic rendering data types into [`diagnostics`]
//! so the future core-side Engine struct can hold
//! `Vec<DiagnosticBlock>` and call `LspHost::diagnostics_for` without a
//! tui dependency. The tui crate re-exports both from `tui::lsp`.

pub mod diagnostics;

pub use diagnostics::{Diagnostic, DiagnosticBlock, Severity, render_blocks};

use std::collections::HashMap;

use serde::Deserialize;

/// `[lsp]` config schema. Mirrors the TOML keys documented in
/// `config.example.toml`. Unknown keys are ignored.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct LspConfig {
    /// Master switch. When `false`, the manager skips every operation and
    /// returns an empty diagnostics list.
    pub enabled: bool,
    /// Maximum time in milliseconds to wait for the LSP server to publish
    /// diagnostics after a `didOpen`/`didChange`. Default 5000 ms.
    pub poll_after_edit_ms: u64,
    /// Maximum diagnostics to keep per file. Excess items are dropped after
    /// sorting by severity. Default 20.
    pub max_diagnostics_per_file: usize,
    /// When `true`, warnings (severity 2) are kept in the output. When
    /// `false` (default), only errors (severity 1) are surfaced.
    pub include_warnings: bool,
    /// Optional override for the `Language -> (cmd, args)` table. Keys use
    /// `Language::as_key` (e.g. `"rust"`).
    pub servers: HashMap<String, Vec<String>>,
}

impl Default for LspConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            poll_after_edit_ms: 5_000,
            max_diagnostics_per_file: 20,
            include_warnings: false,
            servers: HashMap::new(),
        }
    }
}
