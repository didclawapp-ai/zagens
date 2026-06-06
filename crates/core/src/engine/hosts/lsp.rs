//! [`LspHost`] — engine boundary for the LSP post-edit diagnostics hook.
//!
//! Method surface derived from the live `Engine`'s direct calls on the
//! `lsp_manager` field (M3 R1 mitigation — call-graph driven):
//!
//! | File                                              | Call                                         |
//! |---------------------------------------------------|----------------------------------------------|
//! | `crates/tui/src/core/engine/lsp_hooks.rs:24`      | `self.lsp_manager.config().enabled` → `enabled()` |
//! | `crates/tui/src/core/engine/lsp_hooks.rs:38`      | `self.lsp_manager.diagnostics_for(path, seq).await` |
//! | `crates/tui/src/core/engine/tool_context.rs:74`   | `self.lsp_manager.config().enabled` (short-circuit) |
//!
//! Tools consume the concrete `Arc<LspManager>` via `ToolContext`; that
//! path is **not** abstracted in M3 — only the engine-owned call sites are.

use std::path::Path;

use async_trait::async_trait;

use crate::lsp::DiagnosticBlock;

/// Engine-side LSP host.
///
/// Implemented by `crates/tui/src/lsp/mod.rs`'s `LspManager` (an inherent
/// impl block — no wrapper struct). The trait surface lets the future
/// core-side `Engine` swap the `lsp_manager: Arc<LspManager>` field to
/// `Arc<dyn LspHost>` in M7 without inventing new methods.
#[async_trait]
pub trait LspHost: Send + Sync {
    /// Master switch — `false` lets the engine short-circuit the
    /// post-edit hook without an async hop.
    fn enabled(&self) -> bool;

    /// Run diagnostics for `file`. Returns `None` on disabled / language
    /// miss / read failure / timeout. `edit_seq` is currently a correlation
    /// tag only; it is preserved in the signature for the v0.7.x batched
    /// diagnostics-request roadmap.
    async fn diagnostics_for(&self, file: &Path, edit_seq: u64) -> Option<DiagnosticBlock>;
}
