//! Normalized start-turn payload (P2 PR3).
//!
//! `RuntimeThreadManager::start_turn` builds this before touching the live
//! `Engine` in `deepseek-runtime`. Keeps validation and field wiring in core so
//! HTTP and sidecar paths share one contract.

use crate::approval::ApprovalMode;

/// Inputs required to begin an agent turn (after routing/model resolution).
#[derive(Debug, Clone)]
pub struct StartTurnParams {
    pub prompt: String,
    /// Runtime mode string (`agent`, `plan`, …) — parsed in the shell.
    pub mode: String,
    pub model: String,
    pub reasoning_effort: Option<String>,
    pub reasoning_effort_auto: bool,
    pub auto_model: bool,
    pub allow_shell: bool,
    pub trust_mode: bool,
    pub auto_approve: bool,
    pub approval_mode: ApprovalMode,
    /// Optional sampling overrides applied to the session for this turn.
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub max_output_tokens: Option<u32>,
}

impl StartTurnParams {
    /// Returns `Err` when the prompt is empty (mirrors runtime API validation).
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.prompt.trim().is_empty() {
            return Err("prompt is required");
        }
        Ok(())
    }
}
