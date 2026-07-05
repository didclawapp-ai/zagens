//! Harness predicate API types (Phase 1b.1).

use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::manifest_exec::CompletionGateExec;

/// Registered predicate names (`predicate::*` public API).
pub mod names {
    pub const EXIT_CODE: &str = "exit_code";
    pub const FILE_EXISTS: &str = "file_exists";
    pub const TESTS_PASS: &str = "tests_pass";
    pub const COMMAND_OUTPUT_MATCHES: &str = "command_output_matches";
}

/// Execution context for async predicates (shell-backed).
pub struct PredicateContext<'a> {
    pub workspace: &'a Path,
    pub timeout_ms: u64,
    pub exec: Option<&'a CompletionGateExec<'a>>,
    pub run_id: String,
}

impl<'a> PredicateContext<'a> {
    #[must_use]
    pub fn timeout(&self) -> Duration {
        Duration::from_millis(self.timeout_ms)
    }
}

/// Outcome of `predicate::evaluate` (harness-facing; maps to `VerifyRunResult` in layer-2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredicateResult {
    pub predicate: String,
    pub pass: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
    pub duration_ms: u32,
    pub exit_code: i32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub stdout_tail: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub stderr_tail: String,
}

impl PredicateResult {
    #[must_use]
    pub fn pass(predicate: impl Into<String>, duration_ms: u32) -> Self {
        Self {
            predicate: predicate.into(),
            pass: true,
            code: None,
            suggestion: None,
            duration_ms,
            exit_code: 0,
            stdout_tail: String::new(),
            stderr_tail: String::new(),
        }
    }

    #[must_use]
    pub fn fail(
        predicate: impl Into<String>,
        code: impl Into<String>,
        suggestion: impl Into<String>,
        duration_ms: u32,
        exit_code: i32,
    ) -> Self {
        Self {
            predicate: predicate.into(),
            pass: false,
            code: Some(code.into()),
            suggestion: Some(suggestion.into()),
            duration_ms,
            exit_code,
            stdout_tail: String::new(),
            stderr_tail: String::new(),
        }
    }

    pub fn with_output(mut self, stdout: String, stderr: String) -> Self {
        self.stdout_tail = stdout;
        self.stderr_tail = stderr;
        self
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PredicateError {
    #[error("unknown predicate: {0}")]
    Unknown(String),
    #[error("invalid args for {predicate}: {message}")]
    InvalidArgs { predicate: String, message: String },
    #[error("predicate {0} requires shell execution context")]
    NeedsExec(String),
}

pub fn arg_str<'a>(args: &'a Value, key: &str) -> Result<&'a str, PredicateError> {
    args.get(key)
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| PredicateError::InvalidArgs {
            predicate: String::new(),
            message: format!("missing or empty `{key}`"),
        })
}
