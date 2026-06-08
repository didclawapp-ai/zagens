use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Lifecycle hook events — mirrors runtime-server `hooks::HookEvent`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookEventToml {
    SessionStart,
    SessionEnd,
    MessageSubmit,
    ToolCallBefore,
    ToolCallAfter,
    ModeChange,
    OnError,
    ShellEnv,
    PreCompact,
    PostCompact,
    SubagentStart,
    SubagentEnd,
}

/// Condition for when a hook should run.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[derive(Default)]
pub enum HookConditionToml {
    #[default]
    Always,
    ToolName {
        name: String,
    },
    ToolNameRegex {
        pattern: String,
    },
    ToolCategory {
        category: String,
    },
    Mode {
        mode: String,
    },
    ExitCode {
        code: i32,
    },
    All {
        conditions: Vec<HookConditionToml>,
    },
    Any {
        conditions: Vec<HookConditionToml>,
    },
}

/// Single hook definition under `[[hooks.hooks]]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookToml {
    pub event: HookEventToml,
    pub command: String,
    #[serde(default)]
    pub condition: Option<HookConditionToml>,
    #[serde(default = "default_hook_timeout")]
    pub timeout_secs: u64,
    #[serde(default)]
    pub background: bool,
    #[serde(default = "default_continue_on_error")]
    pub continue_on_error: bool,
    #[serde(default)]
    pub name: Option<String>,
}

fn default_hook_timeout() -> u64 {
    30
}

fn default_continue_on_error() -> bool {
    true
}

/// `[hooks]` table in `config.toml`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HooksConfigToml {
    #[serde(default)]
    pub hooks: Vec<HookToml>,
    #[serde(default = "default_hooks_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub default_timeout_secs: Option<u64>,
    #[serde(default)]
    pub working_dir: Option<PathBuf>,
    /// Optional JSONL audit log path for hook executions.
    #[serde(default)]
    pub audit_jsonl: Option<PathBuf>,
}

fn default_hooks_enabled() -> bool {
    true
}
