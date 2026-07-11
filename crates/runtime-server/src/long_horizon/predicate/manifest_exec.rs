//! Shell execution context shared by layer-2 gate and named predicates.

use tokio_util::sync::CancellationToken;

use crate::tools::shell::SharedShellManager;

/// Shell + argv execution context for completion gate / predicates.
pub struct CompletionGateExec<'a> {
    pub shell_manager: &'a SharedShellManager,
    pub cancel_token: Option<&'a CancellationToken>,
    /// Live status lines (e.g. `long_horizon.manifest_gate_running`) flushed
    /// while a long toolchain verify runs — without this the UI stays on
    /// 「生成中」until the whole gate round finishes (up to many minutes).
    pub progress_tx: Option<&'a tokio::sync::mpsc::UnboundedSender<String>>,
}
