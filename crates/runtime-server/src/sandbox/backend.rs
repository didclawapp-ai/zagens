//! Runtime-side re-export shim + factory for the external sandbox backend.
//!
//! The trait + output types live in
//! [`deepseek_core::sandbox`](deepseek_core::sandbox) (moved by M3 — see
//! [`PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE`](../../../../../docs/tech/adr/PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md)
//! §3 row #26 / §6 M3 row). The factory `create_backend(&Config)` stays
//! in this crate because it consumes runtime `Config` and constructs
//! `OpenSandboxBackend`.
//!
//! External sandbox backends route shell command execution to a remote
//! service (e.g. Alibaba OpenSandbox) instead of spawning a local process.
//! This is complementary to the OS-level sandbox modules in this crate
//! (Seatbelt / Landlock / Windows) — the external backend *replaces*
//! local execution entirely when configured.

use anyhow::Result;

pub use deepseek_core::sandbox::{SandboxBackend, SandboxKind, SandboxOutput};

use crate::config::Config;

/// Create the configured sandbox backend from config.
///
/// Returns `None` when no external sandbox backend is configured (i.e. the
/// `sandbox_backend` key is absent, empty, or `"none"`). When `"opensandbox"`
/// is set, constructs an [`OpenSandboxBackend`](super::opensandbox::OpenSandboxBackend)
/// using `sandbox_url` and `sandbox_api_key`.
pub fn create_backend(config: &Config) -> Result<Option<Box<dyn SandboxBackend>>> {
    let kind = config
        .sandbox_backend
        .as_deref()
        .and_then(SandboxKind::parse)
        .unwrap_or(SandboxKind::None);

    match kind {
        SandboxKind::None => Ok(None),
        SandboxKind::OpenSandbox => {
            let base_url = config
                .sandbox_url
                .clone()
                .unwrap_or_else(|| "http://localhost:8080".to_string());
            let api_key = config.sandbox_api_key.clone();
            let backend = super::opensandbox::OpenSandboxBackend::new(base_url, api_key, 30)?;
            Ok(Some(Box::new(backend)))
        }
    }
}
