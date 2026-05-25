//! Re-export shim — diagnostic shapes moved to `deepseek-core::lsp::diagnostics`
//! by M3 (Engine-struct strangler step). See
//! [`PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE`](../../../../../docs/tech/adr/PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md)
//! §3 row #27 and §6 M3 row for context.
//!
//! Existing call sites (`crate::lsp::DiagnosticBlock`,
//! `crate::lsp::render_blocks`, `crate::lsp::Severity`, etc.) continue to
//! resolve through this shim and the parent `tui::lsp::mod.rs`
//! `pub use diagnostics::{...}` re-export, so no import-path churn is
//! required.

pub use deepseek_core::lsp::diagnostics::{
    Diagnostic, DiagnosticBlock, Severity, render_blocks,
};
