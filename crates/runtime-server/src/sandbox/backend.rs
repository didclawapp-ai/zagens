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

/// Result of initializing the configured external sandbox backend.
pub struct SandboxBackendInit {
    pub backend: Option<Box<dyn SandboxBackend>>,
    /// User-visible warning when external sandbox was requested but is inactive.
    pub user_warning: Option<String>,
}

fn configured_sandbox_kind(config: &Config) -> Option<SandboxKind> {
    config.sandbox_backend.as_deref().and_then(SandboxKind::parse)
}

fn invalid_sandbox_backend_value(config: &Config) -> bool {
    config
        .sandbox_backend
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty() && configured_sandbox_kind(config).is_none())
}

/// Initialize the configured sandbox backend and surface a user warning on failure.
///
/// Returns `None` backend when no external sandbox backend is configured (i.e. the
/// `sandbox_backend` key is absent, empty, or `"none"`). When `"opensandbox"`
/// is set, constructs an [`OpenSandboxBackend`](super::opensandbox::OpenSandboxBackend)
/// using `sandbox_url` and `sandbox_api_key`.
pub fn init_backend(config: &Config) -> SandboxBackendInit {
    if invalid_sandbox_backend_value(config) {
        let raw = config
            .sandbox_backend
            .as_deref()
            .unwrap_or_default()
            .trim()
            .to_string();
        return SandboxBackendInit {
            backend: None,
            user_warning: Some(format!(
                "Invalid sandbox_backend value \"{raw}\"; external sandbox disabled — shell runs without remote isolation."
            )),
        };
    }

    let kind = configured_sandbox_kind(config).unwrap_or(SandboxKind::None);
    let wants_external = kind != SandboxKind::None;

    match create_backend_for_kind(config, kind) {
        Ok(Some(backend)) => SandboxBackendInit {
            backend: Some(backend),
            user_warning: None,
        },
        Ok(None) => SandboxBackendInit {
            backend: None,
            user_warning: wants_external.then(|| {
                "External sandbox was configured but no backend is active; shell runs without external sandbox isolation.".to_string()
            }),
        },
        Err(err) => {
            tracing::warn!("Failed to create sandbox backend: {err}");
            SandboxBackendInit {
                backend: None,
                user_warning: Some(format!(
                    "External sandbox ({}) failed to start: {err}. Shell runs without external sandbox isolation.",
                    kind.as_str()
                )),
            }
        }
    }
}

/// Create the configured sandbox backend from config.
///
/// Returns `None` when no external sandbox backend is configured (i.e. the
/// `sandbox_backend` key is absent, empty, or `"none"`). When `"opensandbox"`
/// is set, constructs an [`OpenSandboxBackend`](super::opensandbox::OpenSandboxBackend)
/// using `sandbox_url` and `sandbox_api_key`.
pub fn create_backend(config: &Config) -> Result<Option<Box<dyn SandboxBackend>>> {
    let kind = configured_sandbox_kind(config).unwrap_or(SandboxKind::None);
    create_backend_for_kind(config, kind)
}

fn create_backend_for_kind(
    config: &Config,
    kind: SandboxKind,
) -> Result<Option<Box<dyn SandboxBackend>>> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_backend_warns_on_invalid_config_value() {
        let config = Config {
            sandbox_backend: Some("not-a-real-backend".to_string()),
            ..Config::default()
        };
        let init = init_backend(&config);
        assert!(init.backend.is_none());
        let warning = init
            .user_warning
            .expect("invalid sandbox_backend should warn");
        assert!(warning.contains("Invalid sandbox_backend"));
    }

    #[test]
    fn init_backend_silent_when_sandbox_disabled() {
        let config = Config::default();
        let init = init_backend(&config);
        assert!(init.backend.is_none());
        assert!(init.user_warning.is_none());
    }
}
