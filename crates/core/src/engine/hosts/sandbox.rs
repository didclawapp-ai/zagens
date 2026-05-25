//! [`SandboxHost`] — engine boundary for the optional external sandbox
//! backend (Alibaba OpenSandbox and similar remote-execution drivers).
//!
//! The live `Engine` never calls a method on the backend directly — it
//! only forwards the `Option<Arc<dyn SandboxBackend>>` into
//! `ToolContext`. The single call site is
//! `crates/tui/src/core/engine/tool_context.rs:61`.
//!
//! Introducing the host trait now (per spike §2.2 target layout) lets
//! M7 swap `sandbox_backend: Option<Arc<dyn SandboxBackend>>` to
//! `Box<dyn SandboxHost>` without inventing the surface later. The
//! underlying [`SandboxBackend`](crate::sandbox::SandboxBackend) trait
//! already moved into core in the same PR (M3) — the host trait wraps
//! the **field shape**, the backend trait wraps the **execution shape**.

use std::sync::Arc;

use crate::sandbox::SandboxBackend;

/// Engine-side external-sandbox host.
///
/// Implemented by `crates/tui/src/sandbox/mod.rs`'s `TuiSandboxHost`
/// newtype, which wraps the optional `Arc<dyn SandboxBackend>` produced
/// by `crate::sandbox::backend::create_backend(&Config)`.
pub trait SandboxHost: Send + Sync {
    /// External sandbox backend. `None` when no remote execution is
    /// configured (the default). Engine forwards the `Arc` clone into
    /// `ToolContext`.
    fn backend(&self) -> Option<&Arc<dyn SandboxBackend>>;
}
