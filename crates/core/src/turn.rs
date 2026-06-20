//! Turn types shared between the runtime core and the TUI shell.
//!
//! These types are pure data — no LLM client dependency, no IO.

use std::time::Duration;

/// Final status for a turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnOutcomeStatus {
    Completed,
    Interrupted,
    Failed,
}

/// Record of a tool call within a turn.
#[derive(Debug, Clone)]
pub struct TurnToolCall {
    pub id: String,
    pub name: String,
    pub input: serde_json::Value,
    pub result: Option<String>,
    pub error: Option<String>,
    pub duration: Option<Duration>,
}

impl TurnToolCall {
    pub fn new(id: String, name: String, input: serde_json::Value) -> Self {
        Self {
            id,
            name,
            input,
            result: None,
            error: None,
            duration: None,
        }
    }

    pub fn set_result(&mut self, result: String, duration: Duration) {
        self.result = Some(result);
        self.duration = Some(duration);
    }

    pub fn set_error(&mut self, error: String, duration: Duration) {
        self.error = Some(error);
        self.duration = Some(duration);
    }
}

/// Application mode slice used by the turn loop (mirrors TUI `AppMode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnLoopMode {
    Agent,
    Yolo,
    Plan,
}

impl TurnLoopMode {
    #[must_use]
    pub const fn is_plan(self) -> bool {
        matches!(self, Self::Plan)
    }

    /// Parse a runtime mode string (`"agent"` / `"yolo"` / `"plan"`).
    /// Mirrors `tui::AppMode::from_setting` so HTTP/runtime callers can
    /// stay shell-agnostic. Unknown values fall back to `Agent` to match
    /// the tui-side contract.
    #[must_use]
    pub fn from_setting(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "plan" => Self::Plan,
            "yolo" => Self::Yolo,
            _ => Self::Agent,
        }
    }
}

/// Context for a single turn (user message + AI response).
#[derive(Debug)]
pub struct TurnContext {
    pub id: String,
    pub step: u32,
    pub max_steps: u32,
    pub tool_calls: Vec<TurnToolCall>,
    pub cancelled: bool,
    pub usage: crate::models::Usage,
}

impl TurnContext {
    #[must_use]
    pub fn new(max_steps: u32) -> Self {
        Self::with_id(max_steps, uuid::Uuid::new_v4().to_string())
    }

    /// Begin a turn with a caller-supplied id (runtime thread store / trace export SSOT).
    #[must_use]
    pub fn with_id(max_steps: u32, turn_id: impl Into<String>) -> Self {
        Self {
            id: turn_id.into(),
            step: 0,
            max_steps,
            tool_calls: Vec::new(),
            cancelled: false,
            usage: crate::models::Usage::default(),
        }
    }

    pub fn next_step(&mut self) -> bool {
        self.step += 1;
        self.step <= self.max_steps
    }

    #[must_use]
    pub fn at_max_steps(&self) -> bool {
        self.step >= self.max_steps
    }

    /// Remaining assistant steps before `max_steps` (inclusive of current step).
    #[must_use]
    pub fn steps_remaining(&self) -> u32 {
        self.max_steps.saturating_sub(self.step)
    }

    pub fn record_tool_call(&mut self, call: TurnToolCall) {
        self.tool_calls.push(call);
    }

    pub fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub fn add_usage(&mut self, usage: &crate::models::Usage) {
        self.usage.input_tokens += usage.input_tokens;
        self.usage.output_tokens += usage.output_tokens;
        self.usage.prompt_cache_hit_tokens = add_optional_usage(
            self.usage.prompt_cache_hit_tokens,
            usage.prompt_cache_hit_tokens,
        );
        self.usage.prompt_cache_miss_tokens = add_optional_usage(
            self.usage.prompt_cache_miss_tokens,
            usage.prompt_cache_miss_tokens,
        );
        self.usage.reasoning_tokens =
            add_optional_usage(self.usage.reasoning_tokens, usage.reasoning_tokens);
    }
}

fn add_optional_usage(total: Option<u32>, delta: Option<u32>) -> Option<u32> {
    match (total, delta) {
        (Some(total), Some(delta)) => Some(total.saturating_add(delta)),
        (None, Some(delta)) => Some(delta),
        (Some(total), None) => Some(total),
        (None, None) => None,
    }
}

/// Lightweight turn step counter and tool-call log (no LLM dependency).
///
/// Prefer [`TurnContext`] for the live engine loop; this remains for callers
/// that only need step/tool-call tracking without usage aggregation.
#[derive(Debug)]
pub struct TurnState {
    pub id: String,
    pub step: u32,
    pub max_steps: u32,
    pub tool_calls: Vec<TurnToolCall>,
    pub cancelled: bool,
}

impl TurnState {
    pub fn new(max_steps: u32) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            step: 0,
            max_steps,
            tool_calls: Vec::new(),
            cancelled: false,
        }
    }

    pub fn next_step(&mut self) -> bool {
        self.step += 1;
        self.step <= self.max_steps
    }

    pub fn at_max_steps(&self) -> bool {
        self.step >= self.max_steps
    }

    pub fn record_tool_call(&mut self, call: TurnToolCall) {
        self.tool_calls.push(call);
    }

    pub fn cancel(&mut self) {
        self.cancelled = true;
    }

    /// Sorted, de-duplicated tool names called so far.
    pub fn tool_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.tool_calls.iter().map(|tc| tc.name.clone()).collect();
        names.sort();
        names.dedup();
        names
    }
}
