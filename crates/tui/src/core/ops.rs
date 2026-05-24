//! Operations submitted by the UI to the core engine.
//!
//! These operations flow from the TUI to the engine via a channel,
//! allowing the UI to remain responsive while the engine processes requests.

use crate::compaction::CompactionConfig;
use crate::context_snapshot::ThreadContextSnapshot;
use crate::models::{Message, SystemPrompt};
use tokio::sync::oneshot;
use crate::tui::app::AppMode;
use crate::tui::approval::ApprovalMode;
use std::path::PathBuf;

/// Operations that can be submitted to the engine.
#[derive(Debug)]
pub enum Op {
    /// Send a message to the AI
    SendMessage {
        content: String,
        mode: AppMode,
        model: String,
        goal_objective: Option<String>,
        /// Reasoning-effort tier: `"off" | "low" | "medium" | "high" | "max"`.
        /// `None` lets the provider apply its default.
        reasoning_effort: Option<String>,
        /// True when the user selected auto thinking, even though the UI sends
        /// a concrete per-turn value to the model API.
        reasoning_effort_auto: bool,
        /// True when the user selected auto model routing.
        auto_model: bool,
        allow_shell: bool,
        trust_mode: bool,
        auto_approve: bool,
        approval_mode: ApprovalMode,
        temperature: Option<f32>,
        top_p: Option<f32>,
        max_output_tokens: Option<u32>,
    },

    /// Cancel the current request
    #[allow(dead_code)]
    CancelRequest,

    /// Approve a tool call that requires permission
    #[allow(dead_code)]
    ApproveToolCall { id: String },

    /// Deny a tool call that requires permission
    #[allow(dead_code)]
    DenyToolCall { id: String },

    /// Spawn a sub-agent
    #[allow(dead_code)]
    SpawnSubAgent { prompt: String },

    /// List current sub-agents and their status
    ListSubAgents,

    /// Change the operating mode
    #[allow(dead_code)]
    ChangeMode { mode: AppMode },

    /// Update the model being used
    #[allow(dead_code)]
    SetModel { model: String },

    /// Update auto-compaction settings
    SetCompaction { config: CompactionConfig },

    /// Sync engine session state (used for resume/load)
    SyncSession {
        messages: Vec<Message>,
        system_prompt: Option<SystemPrompt>,
        model: String,
        workspace: PathBuf,
    },

    /// Run context compaction immediately.
    CompactContext,

    /// Run a Recursive Language Model (RLM) turn per Algorithm 1 of
    /// Zhang et al. (arXiv:2512.24601). The prompt is stored in the REPL
    /// as `context`; the root LLM only sees metadata.
    Rlm {
        /// The user's prompt — stored in REPL, NOT in the LLM context.
        content: String,
        /// The model to use for root LLM calls.
        model: String,
        /// The model to use for sub-LLM (llm_query) calls.
        child_model: String,
        /// Recursion budget for `sub_rlm()` calls. Paper experiments use
        /// depth=1; defaults set by the `/rlm` command.
        max_depth: u32,
    },

    /// Edit the last user message: remove the last user+assistant exchange
    /// from the session, then re-send with the new content.
    EditLastTurn { new_message: String },

    /// Drop the last user message and everything after it (#383 `/edit`, F4 HTTP).
    TruncateBeforeLastUserMessage {
        reply: oneshot::Sender<bool>,
    },

    /// Return a TUI-aligned context usage snapshot (DS Pick / runtime API).
    QueryContext {
        reply: oneshot::Sender<ThreadContextSnapshot>,
    },

    /// Shutdown the engine
    Shutdown,
}
