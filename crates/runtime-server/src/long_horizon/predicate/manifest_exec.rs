//! Shell execution context shared by layer-2 gate and named predicates.

use tokio_util::sync::CancellationToken;

use crate::tools::shell::SharedShellManager;

/// Shell + argv execution context for completion gate / predicates.
pub struct CompletionGateExec<'a> {
    pub shell_manager: &'a SharedShellManager,
    pub cancel_token: Option<&'a CancellationToken>,
}
