//! Engine construction (`Engine::with_hosts`, M7).

use std::sync::{Arc, Mutex as StdMutex};

use tokio::sync::{mpsc, RwLock};
use tokio_util::sync::CancellationToken;

use crate::coherence::CoherenceState;
use crate::engine::config::EngineConfig;
use crate::engine::handle::EngineHandle;
use crate::engine::host_bundle::EngineHostBundle;
use crate::engine::runtime::Engine;
use crate::session::Session;

impl<P, R> Engine<P, R>
where
    P: Send + Sync + 'static,
    R: Send + Sync + 'static,
{
    /// Construct an engine from lean config + tui-wired host bundle.
    ///
    /// Creates the seven mpsc channel pairs core-side (spike R11) and
    /// returns `(engine, handle)` for the tui builder to spawn the loop.
    #[must_use]
    pub fn with_hosts(
        config: EngineConfig,
        session: Session,
        hosts: EngineHostBundle<P, R>,
    ) -> (Self, EngineHandle<P, R>) {
        let EngineHostBundle {
            lsp,
            shell,
            sandbox,
            seam,
            workshop,
            topic_memory,
            capacity_controller,
            deepseek_client,
            deepseek_client_error,
            api_key_env_only_recovery,
            ext,
            scratchpad_run_id,
        } = hosts;

        let (tx_op, rx_op) = mpsc::channel(32);
        let (tx_event, rx_event) = mpsc::channel(256);
        let (tx_approval, rx_approval) = mpsc::channel(64);
        let (tx_user_input, rx_user_input) = mpsc::channel(32);
        let (tx_steer, rx_steer) = mpsc::channel(64);
        let cancel_token = CancellationToken::new();
        let shared_cancel_token = Arc::new(StdMutex::new(cancel_token.clone()));
        let tool_exec_lock = Arc::new(RwLock::new(()));

        let engine = Self {
            config,
            ext,
            deepseek_client,
            deepseek_client_error,
            api_key_env_only_recovery,
            session,
            shell,
            rx_op,
            tx_approval: tx_approval.clone(),
            rx_approval,
            rx_user_input,
            rx_steer,
            tx_event,
            cancel_token,
            shared_cancel_token: shared_cancel_token.clone(),
            tool_exec_lock,
            capacity_controller,
            seam,
            coherence_state: CoherenceState::default(),
            turn_counter: 0,
            lsp,
            workshop,
            sandbox,
            pending_lsp_blocks: Vec::new(),
            scratchpad_step: Default::default(),
            scratchpad_run_id,
            scratchpad_summary_injected_this_turn: false,
            topic_memory,
        };

        let handle = EngineHandle::new(
            tx_op,
            Arc::new(RwLock::new(rx_event)),
            shared_cancel_token,
            tx_approval,
            tx_user_input,
            tx_steer,
        );

        (engine, handle)
    }
}
