//! Pluggable sandbox backend abstraction.
//!
//! External sandbox backends route shell command execution to a remote service
//! (e.g. Alibaba OpenSandbox) instead of spawning a local process. This is
//! complementary to the OS-level sandbox module (Seatbelt / Landlock / Windows
//! — all of which live in `deepseek-runtime` OS sandbox modules) — the external backend
//! *replaces* local execution entirely when configured.
//!
//! M3 (Engine-struct strangler step) moved the trait + output types here so
//! the core-side `Engine` struct can hold `Option<Arc<dyn SandboxBackend>>`
//! without a sidecar dependency. The OS-level sandbox implementations
//! (`SandboxPolicy`, Seatbelt/Landlock plumbing) and the `create_backend(&Config)`
//! factory stay in `deepseek-runtime` because they depend on runtime-owned
//! config and OS-specific glue.

use std::collections::HashMap;

use anyhow::Result;
use async_trait::async_trait;

/// Output from a sandbox backend execution.
#[derive(Debug, Clone)]
pub struct SandboxOutput {
    /// Standard output from the command.
    pub stdout: String,
    /// Standard error from the command.
    pub stderr: String,
    /// Exit code (0 for success).
    pub exit_code: i32,
}

/// The kind of external sandbox backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxKind {
    /// No external sandbox — execute commands locally.
    None,
    /// Alibaba OpenSandbox remote execution.
    OpenSandbox,
}

impl SandboxKind {
    /// Parse a sandbox backend name from config (case-insensitive).
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "none" | "" => Some(Self::None),
            "opensandbox" | "open-sandbox" | "open_sandbox" => Some(Self::OpenSandbox),
            _ => None,
        }
    }

    /// Human-readable label.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::OpenSandbox => "opensandbox",
        }
    }
}

/// Abstract interface for an external sandbox backend.
///
/// Implementations send commands to a remote execution environment and return
/// structured output. The trait is `Send + Sync` so it can be stored in an
/// `Arc` and shared across async tasks.
#[async_trait]
pub trait SandboxBackend: Send + Sync {
    /// Execute a shell command and return its output.
    ///
    /// `cmd` is the full shell command string (e.g. `"ls -la"`).
    /// `env` contains additional environment variables to set.
    async fn exec(&self, cmd: &str, env: &HashMap<String, String>) -> Result<SandboxOutput>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sandbox_kind_parse_accepts_aliases() {
        assert_eq!(SandboxKind::parse("none"), Some(SandboxKind::None));
        assert_eq!(SandboxKind::parse(""), Some(SandboxKind::None));
        assert_eq!(SandboxKind::parse("opensandbox"), Some(SandboxKind::OpenSandbox));
        assert_eq!(SandboxKind::parse("Open-Sandbox"), Some(SandboxKind::OpenSandbox));
        assert_eq!(SandboxKind::parse("OPEN_SANDBOX"), Some(SandboxKind::OpenSandbox));
        assert_eq!(SandboxKind::parse("unknown"), None);
    }

    #[test]
    fn sandbox_kind_as_str_roundtrip() {
        assert_eq!(SandboxKind::None.as_str(), "none");
        assert_eq!(SandboxKind::OpenSandbox.as_str(), "opensandbox");
    }
}
