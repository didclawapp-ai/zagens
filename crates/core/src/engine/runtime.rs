//! Core `Engine` struct (M7 strangler step).
//!
//! Holds lean config, host trait objects, channels, and session state.
//! Platform-specific op dispatch and tool wiring live in the sidecar via
//! [`super::platform_ext::EnginePlatformExt`].

use std::sync::{Arc, Mutex as StdMutex};

use tokio::sync::{RwLock, mpsc};
use tokio_util::sync::CancellationToken;

use crate::capacity::CapacityController;
use crate::chat::LlmClient;
use crate::coherence::CoherenceState;
use crate::engine::approval::{ApprovalDecision, UserInputDecision};
use crate::engine::config::EngineConfig;
use crate::engine::hosts::{
    LspHost, SandboxHost, SeamHost, ShellHost, TopicMemoryHost, WorkshopHost,
};
use crate::engine::op::Op;
use crate::engine::scratchpad_state::ScratchpadStepState;
use crate::events::Event;
use crate::lsp::DiagnosticBlock;
use crate::session::Session;

/// The core engine that processes operations and emits events.
pub struct Engine<P, R> {
    pub config: EngineConfig,
    /// Tui-only extension config + concrete subsystem handles (type-erased).
    pub ext: Box<dyn crate::engine::platform_ext::EnginePlatformExt<P, R>>,
    pub deepseek_client: Option<Arc<dyn LlmClient>>,
    pub deepseek_client_error: Option<String>,
    pub api_key_env_only_recovery: Option<String>,
    pub session: Session,
    pub shell: Box<dyn ShellHost>,
    pub rx_op: mpsc::Receiver<Op>,
    pub tx_approval: mpsc::Sender<ApprovalDecision<P>>,
    pub rx_approval: mpsc::Receiver<ApprovalDecision<P>>,
    pub rx_user_input: mpsc::Receiver<UserInputDecision<R>>,
    pub rx_steer: mpsc::Receiver<String>,
    pub tx_event: mpsc::Sender<Event>,
    pub cancel_token: CancellationToken,
    pub shared_cancel_token: Arc<StdMutex<CancellationToken>>,
    pub tool_exec_lock: Arc<RwLock<()>>,
    pub capacity_controller: CapacityController,
    pub seam: Option<Box<dyn SeamHost>>,
    pub coherence_state: CoherenceState,
    pub turn_counter: u64,
    pub lsp: Arc<dyn LspHost>,
    pub workshop: Option<Box<dyn WorkshopHost>>,
    pub sandbox: Box<dyn SandboxHost>,
    pub pending_lsp_blocks: Vec<DiagnosticBlock>,
    pub scratchpad_step: ScratchpadStepState,
    pub scratchpad_run_id: Option<String>,
    pub scratchpad_summary_injected_this_turn: bool,
    /// One-shot guard: inject incomplete-audit continue nudge before prose-only turn break.
    pub scratchpad_audit_continue_injected_this_turn: bool,
    /// One-shot guard: inject LHT continue nudge before prose-only turn break.
    pub long_horizon_continue_injected_this_turn: bool,
    /// "一推到底" (C2): auto-continue rounds consumed this turn — bounds the
    /// give-up override (`long_horizon.max_auto_continue_rounds`). Reset per
    /// user message alongside the other per-turn LHT guards.
    pub long_horizon_auto_continue_rounds: u32,
    pub topic_memory: Box<dyn TopicMemoryHost>,
}
