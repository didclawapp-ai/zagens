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

/// Lightweight turn step counter and tool-call log (no LLM dependency).
///
/// The full `TurnContext` in `deepseek-tui` wraps additional fields
/// (usage, snapshots) that are TUI-specific.
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
