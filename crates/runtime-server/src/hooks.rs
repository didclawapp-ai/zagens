//! Hooks system for `DeepSeek` CLI
//!
//! Provides lifecycle hooks that execute user-defined shell commands at:
//! - Session start/end
//! - Tool call before/after

//! - Mode changes
//! - Message submission
//! - Error events
//!
//! Configuration is done via `[[hooks.hooks]]` in config.toml.

// Note: anyhow is available if needed for future error handling
#[allow(unused_imports)]
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Read;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use wait_timeout::ChildExt;

/// Events that can trigger hook execution
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HookEvent {
    /// Triggered when a new session starts
    SessionStart,
    /// Triggered when a session ends (quit, Ctrl+C)
    SessionEnd,
    /// Triggered before a user message is sent to the LLM
    MessageSubmit,
    /// Triggered before a tool is executed
    ToolCallBefore,
    /// Triggered after a tool completes (success or failure)
    ToolCallAfter,
    /// Triggered when the user changes modes (Plan, Agent, Yolo)
    ModeChange,
    /// Triggered when an error occurs
    OnError,
    /// Triggered immediately before each `exec_shell` invocation. The hook's
    /// stdout is parsed as `KEY=VALUE\n` lines and merged on top of the
    /// shell command's environment — useful for ephemeral credentials,
    /// per-skill PATH adjustments, or short-lived tokens (#456). Hooks that
    /// fail or time out are logged but do *not* abort the shell call; they
    /// simply contribute no env vars.
    ShellEnv,
    /// Triggered immediately before context compaction (manual or auto).
    PreCompact,
    /// Triggered after context compaction completes successfully.
    PostCompact,
    /// Triggered when a sub-agent task is about to start (can block spawn).
    SubagentStart,
    /// Triggered when a sub-agent task reaches a terminal state.
    SubagentEnd,
}

impl HookEvent {
    /// Get string representation for environment variable
    #[allow(dead_code)] // Used in tests and future hook dispatch
    pub fn as_str(self) -> &'static str {
        match self {
            HookEvent::SessionStart => "session_start",
            HookEvent::SessionEnd => "session_end",
            HookEvent::MessageSubmit => "message_submit",
            HookEvent::ToolCallBefore => "tool_call_before",
            HookEvent::ToolCallAfter => "tool_call_after",
            HookEvent::ModeChange => "mode_change",
            HookEvent::OnError => "on_error",
            HookEvent::ShellEnv => "shell_env",
            HookEvent::PreCompact => "pre_compact",
            HookEvent::PostCompact => "post_compact",
            HookEvent::SubagentStart => "subagent_start",
            HookEvent::SubagentEnd => "subagent_end",
        }
    }
}

/// Condition for when a hook should run
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[derive(Default)]
pub enum HookCondition {
    /// Always run this hook
    #[default]
    Always,
    /// Only run for specific tool names
    ToolName {
        /// Tool name to match (e.g., "`exec_shell`", "`write_file`")
        name: String,
    },
    /// Only run when tool name matches a regex pattern
    ToolNameRegex {
        /// Rust regex syntax (e.g. `exec_.*`, `write_file|edit_file`)
        pattern: String,
    },
    /// Only run for specific tool categories
    ToolCategory {
        /// Category: "safe", "`file_write`", "shell"
        category: String,
    },
    /// Only run in specific modes
    Mode {
        /// Mode: "plan", "agent", "yolo"
        mode: String,
    },
    /// Only run when exit code matches (for `ToolCallAfter`)
    ExitCode {
        /// Exit code to match
        code: i32,
    },
    /// Combine multiple conditions with AND
    All { conditions: Vec<HookCondition> },
    /// Combine multiple conditions with OR
    Any { conditions: Vec<HookCondition> },
}

/// A single hook definition
#[derive(Debug, Clone)]
pub struct Hook {
    /// The event that triggers this hook
    pub event: HookEvent,

    /// Shell command to execute (platform shell: `sh -c` on Unix, `cmd /C` on Windows)
    pub command: String,

    /// Optional condition for when this hook should run
    pub condition: Option<HookCondition>,

    /// Per-hook timeout in seconds. `None` means "use `HooksConfig::default_timeout_secs`
    /// if set, otherwise fall back to the built-in 30 s default".
    pub timeout_secs: Option<u64>,

    /// Run in background (don't wait for completion)
    pub background: bool,

    /// Continue if this hook fails (default: true)
    pub continue_on_error: bool,

    /// Optional name for logging/debugging
    pub name: Option<String>,
}

fn default_timeout() -> u64 {
    30
}
fn default_continue_on_error() -> bool {
    true
}

impl Hook {
    /// Create a new hook with minimal configuration
    #[allow(dead_code)] // Public builder API, used in tests
    pub fn new(event: HookEvent, command: &str) -> Self {
        Self {
            event,
            command: command.to_string(),
            condition: None,
            timeout_secs: None,
            background: false,
            continue_on_error: true,
            name: None,
        }
    }

    /// Builder: set condition
    #[allow(dead_code)] // Public builder API, used in tests
    pub fn with_condition(mut self, condition: HookCondition) -> Self {
        self.condition = Some(condition);
        self
    }

    /// Builder: set timeout
    #[allow(dead_code)] // Public builder API, used in tests
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = Some(secs);
        self
    }

    /// Builder: run in background
    #[allow(dead_code)] // Public builder API, used in tests
    pub fn background(mut self) -> Self {
        self.background = true;
        self
    }

    /// Builder: set name
    #[allow(dead_code)] // Public builder API, used in tests
    pub fn with_name(mut self, name: &str) -> Self {
        self.name = Some(name.to_string());
        self
    }
}

/// Configuration for hooks (loaded from config.toml)
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct HooksConfig {
    /// List of hooks to execute
    #[serde(default)]
    pub hooks: Vec<Hook>,

    /// Global enable/disable for all hooks
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Global timeout override (applies if hook doesn't specify one)
    #[serde(default)]
    pub default_timeout_secs: Option<u64>,

    /// Working directory for hook execution (default: workspace)
    #[serde(default)]
    pub working_dir: Option<PathBuf>,

    /// Optional JSONL audit log for hook executions (one JSON object per line).
    #[serde(default)]
    pub audit_jsonl: Option<PathBuf>,
}

fn default_enabled() -> bool {
    true
}

impl HooksConfig {
    /// Get hooks for a specific event
    pub fn hooks_for_event(&self, event: HookEvent) -> Vec<&Hook> {
        if !self.enabled {
            return Vec::new();
        }
        self.hooks.iter().filter(|h| h.event == event).collect()
    }

    /// Check if hooks are configured and enabled
    #[allow(dead_code)] // Public API for hook system consumers
    pub fn has_hooks(&self) -> bool {
        self.enabled && !self.hooks.is_empty()
    }
}

/// Context passed to hooks via environment variables and stdin JSON.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct HookContext {
    /// Tool name (for ToolCallBefore/After)
    pub tool_name: Option<String>,
    /// Tool arguments as JSON string
    pub tool_args: Option<String>,
    /// Tool result output (truncated)
    pub tool_result: Option<String>,
    /// Tool exit code if applicable
    pub tool_exit_code: Option<i32>,
    /// Whether tool succeeded
    pub tool_success: Option<bool>,
    /// Current mode
    pub mode: Option<String>,
    /// Previous mode (for `ModeChange`)
    pub previous_mode: Option<String>,
    /// Session ID
    pub session_id: Option<String>,
    /// User message content
    pub message: Option<String>,
    /// Error message (for `OnError`)
    pub error_message: Option<String>,
    /// Workspace path
    pub workspace: Option<PathBuf>,
    /// Current model name
    pub model: Option<String>,
    /// Total tokens used
    pub total_tokens: Option<u32>,
    /// Session cost in USD
    pub session_cost: Option<f64>,
    /// Sub-agent id (for `SubagentStart` / `SubagentEnd`)
    pub subagent_id: Option<String>,
    /// Sub-agent type slug (e.g. `explore`, `general`)
    pub subagent_type: Option<String>,
    /// Terminal sub-agent status (e.g. `completed`, `failed`)
    pub subagent_status: Option<String>,
    /// Whether compaction was manual (`true`) or automatic (`false`)
    pub compaction_manual: Option<bool>,
    /// Message count before compaction (when available)
    pub compaction_messages_before: Option<usize>,
    /// Message count after compaction (when available)
    pub compaction_messages_after: Option<usize>,
}

impl HookContext {
    pub fn new() -> Self {
        Self::default()
    }

    #[allow(dead_code)] // Public builder API, used in tests
    pub fn with_tool_name(mut self, name: &str) -> Self {
        self.tool_name = Some(name.to_string());
        self
    }

    #[allow(dead_code)] // Public builder API
    pub fn with_tool_args(mut self, args: &serde_json::Value) -> Self {
        self.tool_args = Some(args.to_string());
        self
    }

    #[allow(dead_code)] // Public builder API
    pub fn with_tool_result(mut self, result: &str, success: bool, exit_code: Option<i32>) -> Self {
        self.tool_result = Some(result.to_string());
        self.tool_success = Some(success);
        self.tool_exit_code = exit_code;
        self
    }

    #[allow(dead_code)] // Public builder API, used in tests
    pub fn with_mode(mut self, mode: &str) -> Self {
        self.mode = Some(mode.to_string());
        self
    }

    pub fn with_previous_mode(mut self, mode: &str) -> Self {
        self.previous_mode = Some(mode.to_string());
        self
    }

    #[allow(dead_code)] // Public builder API, used in tests
    pub fn with_workspace(mut self, path: PathBuf) -> Self {
        self.workspace = Some(path);
        self
    }

    pub fn with_model(mut self, model: &str) -> Self {
        self.model = Some(model.to_string());
        self
    }

    pub fn with_session_id(mut self, session_id: &str) -> Self {
        self.session_id = Some(session_id.to_string());
        self
    }

    #[allow(dead_code)] // Public builder API
    pub fn with_message(mut self, message: &str) -> Self {
        self.message = Some(message.to_string());
        self
    }

    #[allow(dead_code)] // Public builder API
    pub fn with_error(mut self, error: &str) -> Self {
        self.error_message = Some(error.to_string());
        self
    }

    pub fn with_tokens(mut self, tokens: u32) -> Self {
        self.total_tokens = Some(tokens);
        self
    }

    #[allow(dead_code)] // Public builder API
    pub fn with_cost(mut self, cost: f64) -> Self {
        self.session_cost = Some(cost);
        self
    }

    pub fn with_subagent_id(mut self, id: &str) -> Self {
        self.subagent_id = Some(id.to_string());
        self
    }

    pub fn with_subagent_type(mut self, agent_type: &str) -> Self {
        self.subagent_type = Some(agent_type.to_string());
        self
    }

    pub fn with_subagent_status(mut self, status: &str) -> Self {
        self.subagent_status = Some(status.to_string());
        self
    }

    pub fn with_compaction_manual(mut self, manual: bool) -> Self {
        self.compaction_manual = Some(manual);
        self
    }

    pub fn with_compaction_stats(
        mut self,
        manual: bool,
        messages_before: usize,
        messages_after: usize,
    ) -> Self {
        self.compaction_manual = Some(manual);
        self.compaction_messages_before = Some(messages_before);
        self.compaction_messages_after = Some(messages_after);
        self
    }

    /// Convert to environment variables
    pub fn to_env_vars(&self) -> HashMap<String, String> {
        let mut env = HashMap::new();

        if let Some(ref name) = self.tool_name {
            env.insert("DEEPSEEK_TOOL_NAME".to_string(), name.clone());
        }
        if let Some(ref args) = self.tool_args {
            env.insert("DEEPSEEK_TOOL_ARGS".to_string(), args.clone());
        }
        if let Some(ref result) = self.tool_result {
            // Truncate result to 10KB to avoid environment variable size limits
            let truncated = if result.len() > 10000 {
                let safe_end = result
                    .char_indices()
                    .take_while(|(i, _)| *i < 10000)
                    .last()
                    .map(|(i, c)| i + c.len_utf8())
                    .unwrap_or(0);
                format!("{}...[truncated]", &result[..safe_end])
            } else {
                result.clone()
            };
            env.insert("DEEPSEEK_TOOL_RESULT".to_string(), truncated);
        }
        if let Some(code) = self.tool_exit_code {
            env.insert("DEEPSEEK_TOOL_EXIT_CODE".to_string(), code.to_string());
        }
        if let Some(success) = self.tool_success {
            env.insert("DEEPSEEK_TOOL_SUCCESS".to_string(), success.to_string());
        }
        if let Some(ref mode) = self.mode {
            env.insert("DEEPSEEK_MODE".to_string(), mode.clone());
        }
        if let Some(ref prev) = self.previous_mode {
            env.insert("DEEPSEEK_PREVIOUS_MODE".to_string(), prev.clone());
        }
        if let Some(ref session_id) = self.session_id {
            env.insert("DEEPSEEK_SESSION_ID".to_string(), session_id.clone());
        }
        if let Some(ref message) = self.message {
            // Truncate message to prevent env var issues
            let truncated = if message.len() > 5000 {
                let safe_end = message
                    .char_indices()
                    .take_while(|(i, _)| *i < 5000)
                    .last()
                    .map(|(i, c)| i + c.len_utf8())
                    .unwrap_or(0);
                format!("{}...[truncated]", &message[..safe_end])
            } else {
                message.clone()
            };
            env.insert("DEEPSEEK_MESSAGE".to_string(), truncated);
        }
        if let Some(ref error) = self.error_message {
            env.insert("DEEPSEEK_ERROR".to_string(), error.clone());
        }
        if let Some(ref ws) = self.workspace {
            env.insert("DEEPSEEK_WORKSPACE".to_string(), ws.display().to_string());
        }
        if let Some(ref model) = self.model {
            env.insert("DEEPSEEK_MODEL".to_string(), model.clone());
        }
        if let Some(tokens) = self.total_tokens {
            env.insert("DEEPSEEK_TOTAL_TOKENS".to_string(), tokens.to_string());
        }
        if let Some(cost) = self.session_cost {
            env.insert("DEEPSEEK_SESSION_COST".to_string(), format!("{cost:.6}"));
        }
        if let Some(ref id) = self.subagent_id {
            env.insert("DEEPSEEK_SUBAGENT_ID".to_string(), id.clone());
        }
        if let Some(ref agent_type) = self.subagent_type {
            env.insert("DEEPSEEK_SUBAGENT_TYPE".to_string(), agent_type.clone());
        }
        if let Some(ref status) = self.subagent_status {
            env.insert("DEEPSEEK_SUBAGENT_STATUS".to_string(), status.clone());
        }
        if let Some(manual) = self.compaction_manual {
            env.insert("DEEPSEEK_COMPACTION_MANUAL".to_string(), manual.to_string());
        }
        if let Some(before) = self.compaction_messages_before {
            env.insert(
                "DEEPSEEK_COMPACTION_MESSAGES_BEFORE".to_string(),
                before.to_string(),
            );
        }
        if let Some(after) = self.compaction_messages_after {
            env.insert(
                "DEEPSEEK_COMPACTION_MESSAGES_AFTER".to_string(),
                after.to_string(),
            );
        }

        env
    }

    /// JSON payload written to hook stdin (`{"event":"…","context":{…}}`).
    pub fn to_invoke_json(&self, event: HookEvent) -> String {
        #[derive(Serialize)]
        struct Payload<'a> {
            event: &'static str,
            context: &'a HookContext,
        }
        let ctx = self.for_json_payload();
        serde_json::to_string(&Payload {
            event: event.as_str(),
            context: &ctx,
        })
        .unwrap_or_else(|_| "{}".to_string())
    }

    pub(crate) fn for_json_payload(&self) -> HookContext {
        let mut ctx = self.clone();
        if let Some(ref message) = ctx.message {
            ctx.message = Some(truncate_hook_text(message, 5000));
        }
        if let Some(ref result) = ctx.tool_result {
            ctx.tool_result = Some(truncate_hook_text(result, 10000));
        }
        ctx
    }
}

/// Result of a hook execution
#[derive(Debug, Clone)]
#[allow(dead_code)] // Fields are part of public API for hook consumers
pub struct HookResult {
    /// Hook name (if specified)
    pub name: Option<String>,
    /// Whether the hook succeeded
    pub success: bool,
    /// Exit code from the hook command
    pub exit_code: Option<i32>,
    /// Standard output
    pub stdout: String,
    /// Standard error
    pub stderr: String,
    /// Time taken to execute
    pub duration: Duration,
    /// Error message if execution failed
    pub error: Option<String>,
}

/// Cursor-style deny: exit code 2, or stdout JSON `{"decision":"deny"}` /
/// `{"allow":false}` with optional `reason` / `message`.
fn truncate_hook_text(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_string();
    }
    let safe_end = text
        .char_indices()
        .take_while(|(i, _)| *i < max_bytes)
        .last()
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(0);
    format!("{}...[truncated]", &text[..safe_end])
}

fn parse_hook_deny_stdout(stdout: &str) -> Option<String> {
    let trimmed = stdout.trim();
    if trimmed.is_empty() || !trimmed.starts_with('{') {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(trimmed).ok()?;
    let obj = value.as_object()?;
    if obj.get("decision").and_then(|v| v.as_str()) == Some("deny") {
        return obj
            .get("reason")
            .or_else(|| obj.get("message"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| Some("hook denied".to_string()));
    }
    if obj.get("allow").and_then(|v| v.as_bool()) == Some(false) {
        return obj
            .get("reason")
            .or_else(|| obj.get("message"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| Some("hook denied".to_string()));
    }
    None
}

fn apply_stdout_deny(result: &mut HookResult) {
    if let Some(reason) = parse_hook_deny_stdout(&result.stdout) {
        result.success = false;
        if result.error.is_none() {
            result.error = Some(reason);
        }
    }
}

fn hook_blocks_strict(result: &HookResult, hook: &Hook) -> bool {
    if parse_hook_deny_stdout(&result.stdout).is_some() {
        return true;
    }
    if result.exit_code == Some(2) {
        return true;
    }
    !result.success && !hook.continue_on_error
}

fn hook_block_detail(result: &HookResult) -> String {
    result
        .error
        .clone()
        .filter(|s| !s.is_empty())
        .or_else(|| {
            let stderr = result.stderr.trim();
            if stderr.is_empty() {
                None
            } else {
                Some(stderr.to_string())
            }
        })
        .unwrap_or_else(|| format!("exit code {:?}", result.exit_code))
}

/// Parse optional tool-input override from a `tool_call_before` hook stdout JSON.
pub fn parse_hook_updated_tool_input(stdout: &str) -> Option<serde_json::Value> {
    let trimmed = stdout.trim();
    if trimmed.is_empty() || !trimmed.starts_with('{') {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(trimmed).ok()?;
    let obj = value.as_object()?;
    for key in ["updatedInput", "updated_input", "toolInput", "tool_input"] {
        if let Some(v) = obj.get(key).filter(|v| !v.is_null()) {
            return Some(v.clone());
        }
    }
    None
}

/// Outcome of a blocking hook chain (message submit / tool call before).
#[derive(Debug, Clone, Default)]
pub struct HookBlockingOutcome {
    pub updated_tool_input: Option<serde_json::Value>,
}

/// Executor for running hooks
#[derive(Debug, Clone)]
pub struct HookExecutor {
    config: HooksConfig,
    default_working_dir: PathBuf,
    session_id: String,
}

impl HookExecutor {
    fn build_shell_command(command: &str) -> Command {
        #[cfg(windows)]
        {
            let (shell, arg) = crate::sandbox::windows_shell();
            let mut cmd = Command::new(shell);
            cmd.arg(arg).arg(command);
            cmd
        }
        #[cfg(not(windows))]
        {
            let mut cmd = Command::new("sh");
            cmd.arg("-c").arg(command);
            cmd
        }
    }

    /// Create a new `HookExecutor` with configuration
    pub fn new(config: HooksConfig, default_working_dir: PathBuf) -> Self {
        // Generate a session ID
        let session_id = format!("sess_{}", &uuid::Uuid::new_v4().to_string()[..8]);
        Self::with_session(config, default_working_dir, session_id)
    }

    /// Create a `HookExecutor` bound to a stable session / thread id.
    pub fn with_session(
        config: HooksConfig,
        default_working_dir: PathBuf,
        session_id: impl Into<String>,
    ) -> Self {
        Self {
            config,
            default_working_dir,
            session_id: session_id.into(),
        }
    }

    /// Baseline context fields shared across hook events for this executor.
    pub fn base_context(&self) -> HookContext {
        HookContext::new()
            .with_session_id(&self.session_id)
            .with_workspace(self.default_working_dir.clone())
    }

    /// Create a disabled `HookExecutor` (no hooks will run)
    #[allow(dead_code)] // Used in tests and as convenience constructor
    pub fn disabled() -> Self {
        Self {
            config: HooksConfig {
                enabled: false,
                ..Default::default()
            },
            default_working_dir: PathBuf::from("."),
            session_id: String::new(),
        }
    }

    /// Check if hooks are enabled
    #[allow(dead_code)] // Public API for hook system consumers
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Get the session ID
    /// Read-only access to the underlying configuration. Used by
    /// `/hooks` (#460 read-only MVP) so the user can list configured
    /// hooks without reaching for `cat ~/.deepseek/config.toml`.
    pub fn config(&self) -> &HooksConfig {
        &self.config
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Fire `subagent_start` hooks; returns `Err` when a hook blocks spawn.
    pub fn fire_subagent_start(
        &self,
        context: &HookContext,
        agent_id: &str,
        agent_type: &str,
        prompt: &str,
    ) -> Result<(), String> {
        if !self.has_hooks_for_event(HookEvent::SubagentStart) {
            return Ok(());
        }
        let ctx = context
            .clone()
            .with_subagent_id(agent_id)
            .with_subagent_type(agent_type)
            .with_message(prompt);
        self.execute_blocking(HookEvent::SubagentStart, &ctx)
            .map(|_| ())
    }

    /// Fire `subagent_end` hooks after a sub-agent reaches a terminal state.
    pub fn fire_subagent_end(
        &self,
        context: &HookContext,
        agent_id: &str,
        agent_type: &str,
        status: &str,
        summary: &str,
    ) {
        if !self.has_hooks_for_event(HookEvent::SubagentEnd) {
            return;
        }
        let ctx = context
            .clone()
            .with_subagent_id(agent_id)
            .with_subagent_type(agent_type)
            .with_subagent_status(status)
            .with_message(summary);
        self.execute(HookEvent::SubagentEnd, &ctx);
    }

    /// Fire `post_compact` hooks after a successful compaction.
    pub fn fire_post_compact(
        &self,
        context: &HookContext,
        manual: bool,
        messages_before: usize,
        messages_after: usize,
    ) {
        if !self.has_hooks_for_event(HookEvent::PostCompact) {
            return;
        }
        let ctx = context
            .clone()
            .with_compaction_stats(manual, messages_before, messages_after);
        self.execute(HookEvent::PostCompact, &ctx);
    }

    fn maybe_audit_hook(
        &self,
        event: HookEvent,
        hook: &Hook,
        context: &HookContext,
        result: &HookResult,
    ) {
        let Some(path) = self.config.audit_jsonl.as_ref() else {
            return;
        };
        let line = match serde_json::to_string(&serde_json::json!({
            "at": chrono::Utc::now().to_rfc3339(),
            "event": event.as_str(),
            "hook": hook.name,
            "command": hook.command,
            "session_id": self.session_id,
            "success": result.success,
            "exit_code": result.exit_code,
            "duration_ms": result.duration.as_millis(),
            "denied": hook_blocks_strict(result, hook),
            "context": context.for_json_payload(),
        })) {
            Ok(s) => s,
            Err(err) => {
                tracing::warn!(target: "hooks", error = %err, "failed to encode hook audit entry");
                return;
            }
        };
        if let Some(parent) = path.parent()
            && let Err(err) = std::fs::create_dir_all(parent)
        {
            tracing::warn!(
                target: "hooks",
                path = %parent.display(),
                error = %err,
                "failed to create hook audit directory"
            );
            return;
        }
        use std::io::Write;
        match std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            Ok(mut file) => {
                if let Err(err) = writeln!(file, "{line}") {
                    tracing::warn!(
                        target: "hooks",
                        path = %path.display(),
                        error = %err,
                        "failed to append hook audit entry"
                    );
                }
            }
            Err(err) => {
                tracing::warn!(
                    target: "hooks",
                    path = %path.display(),
                    error = %err,
                    "failed to open hook audit log"
                );
            }
        }
    }

    /// Cheap pre-check: are there any enabled hooks for this event?
    /// Lets call sites avoid building a [`HookContext`] (which allocates
    /// for `workspace`, `model`, `session_id`, …) on every tool call
    /// when the user hasn't configured any hooks. The cost matters
    /// because `ToolCallBefore` / `ToolCallAfter` fire from
    /// `tool_routing.rs` on every tool dispatch (#455).
    #[must_use]
    pub fn has_hooks_for_event(&self, event: HookEvent) -> bool {
        self.config.enabled && self.config.hooks.iter().any(|h| h.event == event)
    }

    /// Run every `ShellEnv` hook for this context and merge their stdout
    /// (`KEY=VALUE\n` lines) into a single env-var map. Used by the
    /// `exec_shell` tool to inject ephemeral credentials, per-skill PATH
    /// adjustments, etc. (#456). Failures don't abort the shell call —
    /// the hook simply contributes no vars and a `tracing::warn!` lands.
    ///
    /// Each successful hook's keys (NOT values) are written to the audit
    /// log so a session can be reconciled later without leaking the
    /// secret material itself.
    pub fn collect_shell_env(&self, context: &HookContext) -> HashMap<String, String> {
        let mut merged: HashMap<String, String> = HashMap::new();
        if !self.config.enabled {
            return merged;
        }
        let hooks = self.config.hooks_for_event(HookEvent::ShellEnv);
        if hooks.is_empty() {
            return merged;
        }
        let env_vars = self.env_for_event(HookEvent::ShellEnv, context);
        for hook in hooks {
            if !self.matches_condition(hook, context) {
                continue;
            }
            // ShellEnv hooks must be synchronous — their stdout is the contract.
            let result = self.execute_sync(hook, HookEvent::ShellEnv, context, &env_vars);
            if !result.success {
                tracing::warn!(
                    target: "hooks",
                    hook = result.name.as_deref().unwrap_or("(unnamed)"),
                    event = "shell_env",
                    exit_code = ?result.exit_code,
                    error = result.error.as_deref().unwrap_or(""),
                    "shell_env hook failed; contributing no env vars"
                );
                continue;
            }
            let parsed = parse_env_lines(&result.stdout);
            if parsed.is_empty() {
                continue;
            }
            // Audit-log the *keys* — never the values.
            crate::audit::log_sensitive_event(
                "shell_env_hook",
                serde_json::json!({
                    "hook": result.name,
                    "tool": context.tool_name,
                    "keys": parsed.keys().cloned().collect::<Vec<_>>(),
                }),
            );
            // Later hooks override earlier ones. Documented behavior.
            merged.extend(parsed);
        }
        merged
    }

    /// Execute all hooks for an event
    pub fn execute(&self, event: HookEvent, context: &HookContext) -> Vec<HookResult> {
        if !self.config.enabled {
            return Vec::new();
        }

        let hooks = self.config.hooks_for_event(event);
        if hooks.is_empty() {
            // Fast path: no hooks for this event → skip the
            // `context.to_env_vars()` HashMap allocation. With
            // `tool_call_before` / `tool_call_after` firing per-tool
            // (#455) this allocation would otherwise happen on every
            // tool dispatch even for users with zero hooks configured.
            return Vec::new();
        }
        let env_vars = self.env_for_event(event, context);
        let mut results = Vec::new();

        for hook in hooks {
            if !self.matches_condition(hook, context) {
                continue;
            }

            let result = if hook.background {
                self.execute_background(hook, event, context, &env_vars)
            } else {
                self.execute_sync(hook, event, context, &env_vars)
            };

            // Log failures via tracing so operators tailing
            // `deepseek` with `RUST_LOG=warn` can see hook errors
            // without instrumenting each call site. Successful runs
            // log nothing (would be too noisy on per-tool events).
            if !result.success {
                let label = result.name.as_deref().unwrap_or("(unnamed)");
                tracing::warn!(
                    target: "hooks",
                    hook = label,
                    event = event.as_str(),
                    exit_code = ?result.exit_code,
                    duration_ms = result.duration.as_millis() as u64,
                    error = result.error.as_deref().unwrap_or(""),
                    stderr_head = %result.stderr.lines().next().unwrap_or(""),
                    "hook failed"
                );
            }

            let should_continue = result.success || hook.continue_on_error;
            results.push(result);

            if !should_continue {
                break;
            }
        }

        results
    }

    /// Execute hooks for an event; return `Err` when a hook fails with
    /// `continue_on_error = false` (used to block message submit / tool calls).
    pub fn execute_blocking(
        &self,
        event: HookEvent,
        context: &HookContext,
    ) -> Result<Vec<HookResult>, String> {
        self.execute_blocking_with_outcome(event, context)
            .map(|(results, _)| results)
    }

    /// Like [`Self::execute_blocking`], but also surfaces stdout JSON tool-input
    /// overrides from `tool_call_before` hooks.
    pub fn execute_blocking_with_outcome(
        &self,
        event: HookEvent,
        context: &HookContext,
    ) -> Result<(Vec<HookResult>, HookBlockingOutcome), String> {
        if !self.config.enabled {
            return Ok((Vec::new(), HookBlockingOutcome::default()));
        }

        let hooks = self.config.hooks_for_event(event);
        if hooks.is_empty() {
            return Ok((Vec::new(), HookBlockingOutcome::default()));
        }

        let env_vars = self.env_for_event(event, context);
        let mut results = Vec::new();
        let mut outcome = HookBlockingOutcome::default();

        for hook in hooks {
            if !self.matches_condition(hook, context) {
                continue;
            }

            // Blocking events (message_submit, tool_call_before, etc.) must always run
            // synchronously so they can deny or modify the operation. A hook marked
            // `background = true` is silently promoted to sync; background semantics
            // only make sense for fire-and-forget events (session_start, post_compact…).
            let result = if hook.background {
                tracing::debug!(
                    target: "hooks",
                    hook = hook.name.as_deref().unwrap_or("(unnamed)"),
                    event = event.as_str(),
                    "background=true ignored in blocking event chain; running synchronously"
                );
                self.execute_sync(hook, event, context, &env_vars)
            } else {
                self.execute_sync(hook, event, context, &env_vars)
            };

            if event == HookEvent::ToolCallBefore
                && let Some(updated) = parse_hook_updated_tool_input(&result.stdout)
            {
                outcome.updated_tool_input = Some(updated);
            }

            if !result.success {
                let label = result.name.as_deref().unwrap_or("(unnamed)");
                tracing::warn!(
                    target: "hooks",
                    hook = label,
                    event = event.as_str(),
                    exit_code = ?result.exit_code,
                    duration_ms = result.duration.as_millis() as u64,
                    error = result.error.as_deref().unwrap_or(""),
                    stderr_head = %result.stderr.lines().next().unwrap_or(""),
                    "hook failed (blocking)"
                );
            }

            results.push(result);
            if hook_blocks_strict(results.last().expect("just pushed"), hook) {
                let last = results.last().expect("just pushed");
                let label = last.name.as_deref().unwrap_or("(unnamed)");
                return Err(format!(
                    "Hook '{label}' blocked: {}",
                    hook_block_detail(last)
                ));
            }
        }

        Ok((results, outcome))
    }

    fn env_for_event(&self, event: HookEvent, context: &HookContext) -> HashMap<String, String> {
        let mut env_vars = context.to_env_vars();
        env_vars.insert(
            "DEEPSEEK_HOOK_EVENT".to_string(),
            event.as_str().to_string(),
        );
        env_vars
    }

    /// Check if a hook's condition matches the context
    #[allow(clippy::only_used_in_recursion)]
    fn matches_condition(&self, hook: &Hook, context: &HookContext) -> bool {
        match &hook.condition {
            None | Some(HookCondition::Always) => true,
            Some(HookCondition::ToolName { name }) => {
                context.tool_name.as_ref().is_some_and(|n| n == name)
            }
            Some(HookCondition::ToolNameRegex { pattern }) => {
                let Ok(re) = regex::Regex::new(pattern) else {
                    tracing::warn!(
                        target: "hooks",
                        pattern = pattern.as_str(),
                        "invalid tool_name_regex hook condition"
                    );
                    return false;
                };
                context.tool_name.as_ref().is_some_and(|n| re.is_match(n))
                    || context
                        .subagent_type
                        .as_ref()
                        .is_some_and(|t| re.is_match(t))
            }
            Some(HookCondition::ToolCategory { category }) => {
                // Map tool names to categories
                let tool_category = context.tool_name.as_ref().map(|name| match name.as_str() {
                    "exec_shell" => "shell",
                    "write_file" | "edit_file" | "apply_patch" => "file_write",
                    "read_file" | "file_info" | "list_dir" | "grep_files" => "safe",
                    _ => "other",
                });
                tool_category.is_some_and(|c| c == category.as_str())
            }
            Some(HookCondition::Mode { mode }) => context
                .mode
                .as_ref()
                .is_some_and(|m| m.to_lowercase() == mode.to_lowercase()),
            Some(HookCondition::ExitCode { code }) => context.tool_exit_code == Some(*code),
            Some(HookCondition::All { conditions }) => conditions.iter().all(|c| {
                self.matches_condition(
                    &Hook {
                        condition: Some(c.clone()),
                        ..hook.clone()
                    },
                    context,
                )
            }),
            Some(HookCondition::Any { conditions }) => conditions.iter().any(|c| {
                self.matches_condition(
                    &Hook {
                        condition: Some(c.clone()),
                        ..hook.clone()
                    },
                    context,
                )
            }),
        }
    }

    /// Execute a hook synchronously
    fn execute_sync(
        &self,
        hook: &Hook,
        event: HookEvent,
        context: &HookContext,
        env_vars: &HashMap<String, String>,
    ) -> HookResult {
        let started = Instant::now();
        let working_dir = self
            .config
            .working_dir
            .clone()
            .unwrap_or_else(|| self.default_working_dir.clone());

        // Per-hook timeout takes priority; fall back to global default, then built-in 30 s.
        let timeout_secs = hook
            .timeout_secs
            .or(self.config.default_timeout_secs)
            .unwrap_or(30);
        let timeout = Duration::from_secs(timeout_secs);
        let stdin_payload = context.to_invoke_json(event);

        let mut child = match Self::build_shell_command(&hook.command)
            .current_dir(&working_dir)
            .envs(env_vars)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(e) => {
                return HookResult {
                    name: hook.name.clone(),
                    success: false,
                    exit_code: None,
                    stdout: String::new(),
                    stderr: String::new(),
                    duration: started.elapsed(),
                    error: Some(format!("Failed to spawn hook: {e}")),
                };
            }
        };

        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(stdin_payload.as_bytes());
        }

        fn read_pipe(mut pipe: impl Read) -> String {
            let mut buf = String::new();
            let _ = pipe.read_to_string(&mut buf);
            buf
        }

        let mut result = match child.wait_timeout(timeout) {
            Ok(Some(status)) => HookResult {
                name: hook.name.clone(),
                success: status.success(),
                exit_code: status.code(),
                stdout: child.stdout.take().map(read_pipe).unwrap_or_default(),
                stderr: child.stderr.take().map(read_pipe).unwrap_or_default(),
                duration: started.elapsed(),
                error: None,
            },
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                HookResult {
                    name: hook.name.clone(),
                    success: false,
                    exit_code: None,
                    stdout: String::new(),
                    stderr: String::new(),
                    duration: started.elapsed(),
                    error: Some(format!("Hook timed out after {}s", timeout_secs)),
                }
            }
            Err(e) => HookResult {
                name: hook.name.clone(),
                success: false,
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
                duration: started.elapsed(),
                error: Some(format!("Failed to wait for hook: {e}")),
            },
        };

        apply_stdout_deny(&mut result);
        if result.exit_code == Some(2) && result.error.is_none() {
            result.error = Some(
                parse_hook_deny_stdout(&result.stdout)
                    .unwrap_or_else(|| "hook denied (exit 2)".to_string()),
            );
        }
        self.maybe_audit_hook(event, hook, context, &result);
        result
    }

    /// Execute a hook in the background (non-blocking, fire-and-forget).
    ///
    /// The spawned thread passes the full context JSON on stdin and enforces the
    /// resolved timeout — if the process exceeds it the child is killed.
    fn execute_background(
        &self,
        hook: &Hook,
        event: HookEvent,
        context: &HookContext,
        env_vars: &HashMap<String, String>,
    ) -> HookResult {
        let started = Instant::now();
        let working_dir = self
            .config
            .working_dir
            .clone()
            .unwrap_or_else(|| self.default_working_dir.clone());

        let timeout_secs = hook
            .timeout_secs
            .or(self.config.default_timeout_secs)
            .unwrap_or(30);
        let stdin_payload = context.to_invoke_json(event);
        let cmd = hook.command.clone();
        let env = env_vars.clone();
        let wd = working_dir.clone();
        let hook_name = hook.name.clone();

        std::thread::spawn(move || {
            let timeout = Duration::from_secs(timeout_secs);
            let mut child = match HookExecutor::build_shell_command(&cmd)
                .current_dir(&wd)
                .envs(&env)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!(
                        target: "hooks",
                        hook = ?hook_name,
                        "background hook failed to spawn: {e}"
                    );
                    return;
                }
            };
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(stdin_payload.as_bytes());
            }
            match child.wait_timeout(timeout) {
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    tracing::warn!(
                        target: "hooks",
                        hook = ?hook_name,
                        "background hook timed out after {timeout_secs}s"
                    );
                }
                Ok(Some(status)) if !status.success() => {
                    tracing::warn!(
                        target: "hooks",
                        hook = ?hook_name,
                        exit_code = ?status.code(),
                        "background hook exited with non-zero status"
                    );
                }
                Err(e) => {
                    tracing::warn!(target: "hooks", hook = ?hook_name, "background hook wait error: {e}");
                }
                Ok(_) => {}
            }
        });

        // Return immediately — background execution is fire-and-forget from the caller's perspective.
        HookResult {
            name: hook.name.clone(),
            success: true,
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
            duration: started.elapsed(),
            error: None,
        }
    }
}

/// Parse `KEY=VALUE\n` lines from a `shell_env` hook's stdout into a map.
///
/// Tolerated: blank lines, leading whitespace, `#` comment lines (ignored),
/// `export KEY=VALUE` (the `export ` prefix is dropped), surrounding quotes
/// on the value. Lines without `=` are silently dropped — easier than
/// failing the whole hook for one stray line of human-friendly output.
/// Values are otherwise taken verbatim; we don't run them through a shell
/// for variable expansion to avoid surprises.
fn parse_env_lines(stdout: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for raw in stdout.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() {
            continue;
        }
        let value = value.trim();
        let stripped = value
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
            .unwrap_or(value);
        out.insert(key.to_string(), stripped.to_string());
    }
    out
}

// === Unit Tests ===

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::PathBuf;

    /// #456 — `parse_env_lines` covers the formats users actually emit from
    /// shell hooks: bare `KEY=VAL`, `export KEY=VAL`, quoted values, comments,
    /// blank lines. Lines without `=` are dropped; values are taken verbatim
    /// (no shell expansion).
    #[test]
    fn parse_env_lines_handles_realistic_hook_output() {
        let stdout = r#"
# Aux comment line, ignored
AWS_ACCESS_KEY_ID=AKIAEXAMPLE
export GITHUB_TOKEN=ghp_examplevalue
QUOTED="value with spaces"
SINGLE='also valid'

= empty key dropped
NOEQUAL line dropped
"#;
        let parsed = super::parse_env_lines(stdout);
        assert_eq!(
            parsed.get("AWS_ACCESS_KEY_ID"),
            Some(&"AKIAEXAMPLE".to_string())
        );
        assert_eq!(
            parsed.get("GITHUB_TOKEN"),
            Some(&"ghp_examplevalue".to_string())
        );
        assert_eq!(parsed.get("QUOTED"), Some(&"value with spaces".to_string()));
        assert_eq!(parsed.get("SINGLE"), Some(&"also valid".to_string()));
        assert!(!parsed.contains_key(""));
        assert!(!parsed.contains_key("NOEQUAL line dropped"));
        // 4 valid entries above; nothing else.
        assert_eq!(parsed.len(), 4);
    }

    /// #456 — empty stdout (or only blank/comments) yields an empty map.
    #[test]
    fn parse_env_lines_empty_when_no_assignments() {
        let parsed = super::parse_env_lines("# nothing\n\n  \n");
        assert!(parsed.is_empty());
    }

    #[test]
    fn test_hook_event_as_str() {
        assert_eq!(HookEvent::SessionStart.as_str(), "session_start");
        assert_eq!(HookEvent::ToolCallAfter.as_str(), "tool_call_after");
        assert_eq!(HookEvent::ModeChange.as_str(), "mode_change");
        assert_eq!(HookEvent::PostCompact.as_str(), "post_compact");
        assert_eq!(HookEvent::SubagentStart.as_str(), "subagent_start");
    }

    #[test]
    fn test_hook_context_subagent_env_vars() {
        let ctx = HookContext::new()
            .with_subagent_id("agent_1")
            .with_subagent_type("explore")
            .with_subagent_status("completed");
        let env = ctx.to_env_vars();
        assert_eq!(
            env.get("DEEPSEEK_SUBAGENT_ID"),
            Some(&"agent_1".to_string())
        );
        assert_eq!(
            env.get("DEEPSEEK_SUBAGENT_TYPE"),
            Some(&"explore".to_string())
        );
        assert_eq!(
            env.get("DEEPSEEK_SUBAGENT_STATUS"),
            Some(&"completed".to_string())
        );
    }

    #[test]
    fn test_hook_context_to_env_vars() {
        let ctx = HookContext::new()
            .with_tool_name("exec_shell")
            .with_mode("agent")
            .with_workspace(PathBuf::from("/tmp"));

        let env = ctx.to_env_vars();

        assert_eq!(
            env.get("DEEPSEEK_TOOL_NAME"),
            Some(&"exec_shell".to_string())
        );
        assert_eq!(env.get("DEEPSEEK_MODE"), Some(&"agent".to_string()));
        assert_eq!(env.get("DEEPSEEK_WORKSPACE"), Some(&"/tmp".to_string()));
    }

    #[test]
    fn test_hook_condition_always() {
        let hook = Hook::new(HookEvent::SessionStart, "echo test");
        let executor = HookExecutor::disabled();
        let context = HookContext::new();

        assert!(executor.matches_condition(&hook, &context));
    }

    #[test]
    fn test_hook_condition_tool_name() {
        let hook = Hook::new(HookEvent::ToolCallBefore, "echo test").with_condition(
            HookCondition::ToolName {
                name: "exec_shell".to_string(),
            },
        );

        let executor = HookExecutor::disabled();

        let context_match = HookContext::new().with_tool_name("exec_shell");
        let context_no_match = HookContext::new().with_tool_name("write_file");

        assert!(executor.matches_condition(&hook, &context_match));
        assert!(!executor.matches_condition(&hook, &context_no_match));
    }

    #[test]
    fn test_hook_condition_mode() {
        let hook =
            Hook::new(HookEvent::ModeChange, "echo test").with_condition(HookCondition::Mode {
                mode: "agent".to_string(),
            });

        let executor = HookExecutor::disabled();

        let context_match = HookContext::new().with_mode("AGENT"); // Case insensitive
        let context_no_match = HookContext::new().with_mode("normal");

        assert!(executor.matches_condition(&hook, &context_match));
        assert!(!executor.matches_condition(&hook, &context_no_match));
    }

    #[test]
    fn test_hooks_config_for_event() {
        let config = HooksConfig {
            enabled: true,
            hooks: vec![
                Hook::new(HookEvent::SessionStart, "echo start"),
                Hook::new(HookEvent::SessionEnd, "echo end"),
                Hook::new(HookEvent::SessionStart, "echo start2"),
            ],
            ..Default::default()
        };

        let start_hooks = config.hooks_for_event(HookEvent::SessionStart);
        assert_eq!(start_hooks.len(), 2);

        let end_hooks = config.hooks_for_event(HookEvent::SessionEnd);
        assert_eq!(end_hooks.len(), 1);
    }

    #[test]
    fn test_hooks_config_disabled() {
        let config = HooksConfig {
            enabled: false,
            hooks: vec![Hook::new(HookEvent::SessionStart, "echo start")],
            ..Default::default()
        };

        let hooks = config.hooks_for_event(HookEvent::SessionStart);
        assert!(hooks.is_empty());
    }

    #[test]
    fn test_hook_builder() {
        let hook = Hook::new(HookEvent::ToolCallAfter, "notify.sh")
            .with_name("notify_tool")
            .with_timeout(60)
            .background()
            .with_condition(HookCondition::ToolCategory {
                category: "shell".to_string(),
            });

        assert_eq!(hook.name, Some("notify_tool".to_string()));
        assert_eq!(hook.timeout_secs, Some(60));
        assert!(hook.background);
        assert!(matches!(
            hook.condition,
            Some(HookCondition::ToolCategory { .. })
        ));
    }

    #[test]
    fn test_hook_timeout_enforced() {
        let command = if cfg!(windows) {
            "ping -n 3 127.0.0.1 > nul"
        } else {
            "sleep 2"
        };
        let hook = Hook::new(HookEvent::SessionStart, command).with_timeout(1);
        let executor = HookExecutor::new(HooksConfig::default(), PathBuf::from("."));
        let env_vars = HashMap::new();
        let context = HookContext::new();

        let result = executor.execute_sync(&hook, HookEvent::SessionStart, &context, &env_vars);
        assert!(!result.success);
        assert!(
            result
                .error
                .as_ref()
                .is_some_and(|e| e.contains("timed out"))
        );
    }

    #[test]
    fn test_executor_session_id() {
        let executor = HookExecutor::new(HooksConfig::default(), PathBuf::from("."));

        assert!(executor.session_id().starts_with("sess_"));
        assert_eq!(executor.session_id().len(), 13); // "sess_" + 8 chars
    }

    #[test]
    fn has_hooks_for_event_fast_path_returns_false_for_empty_config() {
        let executor = HookExecutor::disabled();
        // No hooks configured AT ALL — every event is a fast skip.
        for event in [
            HookEvent::SessionStart,
            HookEvent::SessionEnd,
            HookEvent::MessageSubmit,
            HookEvent::ToolCallBefore,
            HookEvent::ToolCallAfter,
            HookEvent::ModeChange,
            HookEvent::OnError,
        ] {
            assert!(
                !executor.has_hooks_for_event(event),
                "empty config must short-circuit for {event:?}"
            );
        }
    }

    #[test]
    fn has_hooks_for_event_returns_false_when_globally_disabled() {
        let config = HooksConfig {
            enabled: false,
            hooks: vec![Hook::new(HookEvent::ToolCallBefore, "echo blocked")],
            ..HooksConfig::default()
        };
        let executor = HookExecutor::new(config, PathBuf::from("."));
        assert!(
            !executor.has_hooks_for_event(HookEvent::ToolCallBefore),
            "globally-disabled hooks must report no fires even when one is configured"
        );
    }

    #[test]
    fn has_hooks_for_event_distinguishes_event_types() {
        let config = HooksConfig {
            enabled: true,
            hooks: vec![
                Hook::new(HookEvent::SessionStart, "echo start"),
                Hook::new(HookEvent::ToolCallBefore, "echo before"),
            ],
            ..HooksConfig::default()
        };
        let executor = HookExecutor::new(config, PathBuf::from("."));
        // Configured events return true.
        assert!(executor.has_hooks_for_event(HookEvent::SessionStart));
        assert!(executor.has_hooks_for_event(HookEvent::ToolCallBefore));
        // Unconfigured events return false even when other events are present.
        assert!(!executor.has_hooks_for_event(HookEvent::ToolCallAfter));
        assert!(!executor.has_hooks_for_event(HookEvent::OnError));
        assert!(!executor.has_hooks_for_event(HookEvent::ModeChange));
    }

    #[test]
    fn parse_hook_updated_tool_input_reads_common_keys() {
        let stdout = r#"{"decision":"allow","updatedInput":{"path":"foo.txt"}}"#;
        let updated = super::parse_hook_updated_tool_input(stdout).unwrap();
        assert_eq!(
            updated.get("path").and_then(|v| v.as_str()),
            Some("foo.txt")
        );
    }

    #[test]
    fn parse_hook_deny_stdout_recognizes_decision_and_allow_false() {
        assert_eq!(
            super::parse_hook_deny_stdout(r#"{"decision":"deny","reason":"nope"}"#),
            Some("nope".to_string())
        );
        assert_eq!(
            super::parse_hook_deny_stdout(r#"{"allow":false,"message":"blocked"}"#),
            Some("blocked".to_string())
        );
        assert!(super::parse_hook_deny_stdout("KEY=VALUE").is_none());
    }

    #[test]
    fn test_hook_condition_tool_name_regex() {
        let hook = Hook::new(HookEvent::ToolCallBefore, "echo test").with_condition(
            HookCondition::ToolNameRegex {
                pattern: r"write_.*|edit_file".to_string(),
            },
        );
        let executor = HookExecutor::disabled();
        assert!(
            executor.matches_condition(&hook, &HookContext::new().with_tool_name("write_file"))
        );
        assert!(executor.matches_condition(&hook, &HookContext::new().with_tool_name("edit_file")));
        assert!(
            !executor.matches_condition(&hook, &HookContext::new().with_tool_name("read_file"))
        );
    }

    #[test]
    fn execute_blocking_honors_stdout_deny_json() {
        let command = if cfg!(windows) {
            r#"Write-Output '{"decision":"deny","reason":"policy"}'"#
        } else {
            r#"echo '{"decision":"deny","reason":"policy"}'"#
        };
        let config = HooksConfig {
            enabled: true,
            hooks: vec![Hook {
                event: HookEvent::MessageSubmit,
                command: command.to_string(),
                condition: None,
                timeout_secs: Some(5),
                background: false,
                continue_on_error: true,
                name: Some("deny-json".to_string()),
            }],
            ..Default::default()
        };
        let executor = HookExecutor::new(config, PathBuf::from("."));
        let err = executor
            .execute_blocking(HookEvent::MessageSubmit, &HookContext::new())
            .expect_err("stdout deny must block even when continue_on_error=true");
        assert!(err.contains("policy"), "unexpected: {err}");
    }
}
