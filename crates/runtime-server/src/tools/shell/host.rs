//! Engine host newtype for shared shell manager access.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use super::manager::ShellManager;

/// Thread-safe wrapper for `ShellManager`
pub type SharedShellManager = Arc<Mutex<ShellManager>>;

/// Engine boundary newtype wrapping the shared shell manager.
///
/// Introduced by M3 (Engine-struct strangler step). Mirrors
/// [`TuiSandboxHost`](crate::sandbox::TuiSandboxHost): the live
/// `Engine` never calls a method on the shell manager directly — it
/// only clones the `SharedShellManager` into `ToolContext`. Going
/// through this newtype lets M7 swap `Engine::shell_manager` from
/// `SharedShellManager` to `Box<dyn ShellHost>` without inventing
/// new trait surface.
///
/// `ShellManager` itself is `Send` but not `Sync` (it holds
/// `Box<dyn Write + Send>` and `Box<dyn portable_pty::Child + Send>`
/// fields, neither of which is `Sync`). The `SharedShellManager =
/// Arc<Mutex<ShellManager>>` alias *is* `Send + Sync` so this
/// wrapper satisfies the host trait bounds.
#[derive(Clone)]
pub struct TuiShellHost(pub SharedShellManager);

impl TuiShellHost {
    /// Construct from an existing [`SharedShellManager`].
    #[must_use]
    pub fn new(manager: SharedShellManager) -> Self {
        Self(manager)
    }

    /// Underlying manager handle. Used by `ToolContext::with_shell_manager`.
    #[must_use]
    pub fn shared(&self) -> SharedShellManager {
        Arc::clone(&self.0)
    }
}

impl deepseek_core::engine::hosts::ShellHost for TuiShellHost {}

/// Create a new shared shell manager with default sandbox policy.
pub fn new_shared_shell_manager(workspace: PathBuf) -> SharedShellManager {
    Arc::new(Mutex::new(ShellManager::new(workspace)))
}
