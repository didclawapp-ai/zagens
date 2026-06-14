//! Automation rules: hooks, timers, and session-start triggers.
//!
//! Persisted to `~/.deepseek/automation.toml` (or the platform user-data dir).
//!
//! ## Config format (v2 — multiple actions per rule)
//!
//! ```toml
//! [[rules]]
//! id    = 1
//! name  = "Summarise after each turn"
//! enabled = true
//!
//! [rules.trigger]
//! type = "turn_complete"
//!
//! [[rules.actions]]
//! type = "send_prompt"
//! text = "Summarise what you just did in one sentence."
//!
//! [[rules.actions]]
//! type = "run_shell"
//! cmd  = "echo '{{tool_name}}' >> ~/audit.log"
//! ```
//!
//! ### Template variables (substituted at fire time)
//! | Variable | Source |
//! |---|---|
//! | `{{tool_name}}` | name of the completed tool (ToolComplete hook) |
//! | `{{error_message}}` | human-readable error text (OnError hook) |
//! | `{{session_id}}` | current session thread-id |
//!
//! ### Backward compatibility
//! Rules written with the old single-`action` key are transparently migrated to
//! the `actions` array on first load. The old key is never written back.

mod engine;

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub use engine::{AutomationEngine, FiredResult};

// ── EventContext ────────────────────────────────────────────────────────────

/// Runtime context carried by a fired rule; used for template substitution and
/// shell env-var injection.
#[derive(Debug, Clone, Default)]
pub struct EventContext {
    /// Name of the tool that just completed (ToolComplete hook).
    pub tool_name: Option<String>,
    /// Human-readable error message (OnError hook).
    pub error_message: Option<String>,
    /// The active session thread-id, set by the engine caller.
    pub session_id: String,
}

impl EventContext {
    /// Substitute `{{var}}` placeholders in `template`.
    pub fn apply(&self, template: &str) -> String {
        let mut s = template.to_string();
        s = s.replace("{{tool_name}}", self.tool_name.as_deref().unwrap_or(""));
        s = s.replace(
            "{{error_message}}",
            self.error_message.as_deref().unwrap_or(""),
        );
        s = s.replace("{{session_id}}", &self.session_id);
        s
    }
}

// ── Trigger ────────────────────────────────────────────────────────────────

/// How an automation rule is triggered.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TriggerKind {
    // ── Session / time triggers ───────────────────────────────────────────
    /// Fires once when a new session is created or resumed.
    SessionStart,
    /// Fires repeatedly every `every_secs` seconds.
    Interval { every_secs: u64 },
    /// Fires after the user has been idle for `after_secs` seconds.
    Idle { after_secs: u64 },

    // ── Event hooks ───────────────────────────────────────────────────────
    /// Fires when the AI finishes a response turn.
    TurnComplete,
    /// Fires when a tool call completes. `tool_name = None` means any tool.
    ToolComplete {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tool_name: Option<String>,
    },
    /// Fires when the runtime reports an error.
    OnError,
    /// Fires when an approval dialog is raised.
    OnApproval,
}

impl TriggerKind {
    /// Display names shown in the overlay selector (same order as `from_parts` index).
    pub const NAMES: &'static [&'static str] = &[
        "Session Start", // 0
        "Interval",      // 1
        "Idle",          // 2
        "Turn Complete", // 3
        "Tool Complete", // 4
        "On Error",      // 5
        "On Approval",   // 6
    ];

    pub fn kind_index(&self) -> usize {
        match self {
            Self::SessionStart => 0,
            Self::Interval { .. } => 1,
            Self::Idle { .. } => 2,
            Self::TurnComplete => 3,
            Self::ToolComplete { .. } => 4,
            Self::OnError => 5,
            Self::OnApproval => 6,
        }
    }

    /// `true` if this trigger needs a seconds duration field.
    pub fn has_secs(&self) -> bool {
        matches!(self, Self::Interval { .. } | Self::Idle { .. })
    }

    /// `true` if this trigger has an optional tool-name filter.
    pub fn has_tool_filter(&self) -> bool {
        matches!(self, Self::ToolComplete { .. })
    }

    pub fn secs(&self) -> u64 {
        match self {
            Self::Interval { every_secs } => *every_secs,
            Self::Idle { after_secs } => *after_secs,
            _ => 0,
        }
    }

    pub fn tool_filter(&self) -> Option<&str> {
        if let Self::ToolComplete { tool_name: Some(n) } = self {
            Some(n.as_str())
        } else {
            None
        }
    }

    pub fn from_parts(kind_idx: usize, secs: u64, tool_filter: &str) -> Self {
        let secs = secs.max(10);
        let filter = if tool_filter.trim().is_empty() {
            None
        } else {
            Some(tool_filter.trim().to_string())
        };
        match kind_idx {
            1 => Self::Interval { every_secs: secs },
            2 => Self::Idle { after_secs: secs },
            3 => Self::TurnComplete,
            4 => Self::ToolComplete { tool_name: filter },
            5 => Self::OnError,
            6 => Self::OnApproval,
            _ => Self::SessionStart,
        }
    }

    pub fn summary(&self) -> String {
        match self {
            Self::SessionStart => "on start".to_string(),
            Self::Interval { every_secs } => format_duration_label(*every_secs, "every"),
            Self::Idle { after_secs } => format_duration_label(*after_secs, "idle"),
            Self::TurnComplete => "turn done".to_string(),
            Self::ToolComplete { tool_name: None } => "any tool".to_string(),
            Self::ToolComplete { tool_name: Some(n) } => format!("tool:{n}"),
            Self::OnError => "on error".to_string(),
            Self::OnApproval => "on approval".to_string(),
        }
    }
}

// ── Action ─────────────────────────────────────────────────────────────────

/// What an automation rule does when triggered.
///
/// Template variables (`{{tool_name}}`, `{{error_message}}`, `{{session_id}}`)
/// are substituted in `text`/`cmd` fields at fire time.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ActionKind {
    /// Sends a prompt to the AI in the current session.
    SendPrompt { text: String },
    /// Executes a slash command (e.g. `/new`, `/model deepseek-v3`).
    SlashRun { cmd: String },
    /// Runs an arbitrary shell command in a child process (fire-and-forget).
    /// stdout/stderr are captured and the first 500 chars shown as a system line.
    /// Env vars: `ZAGENS_TOOL_NAME`, `ZAGENS_SESSION_ID`, `ZAGENS_ERROR_MSG`.
    RunShell { cmd: String },
    /// Displays a message as a system line in the TUI transcript.
    Notify { message: String },
}

impl ActionKind {
    pub const NAMES: &'static [&'static str] = &["Send Prompt", "Run /cmd", "Run Shell", "Notify"];

    pub fn kind_index(&self) -> usize {
        match self {
            Self::SendPrompt { .. } => 0,
            Self::SlashRun { .. } => 1,
            Self::RunShell { .. } => 2,
            Self::Notify { .. } => 3,
        }
    }

    /// The raw text/command value (for display in the TUI form).
    pub fn text(&self) -> &str {
        match self {
            Self::SendPrompt { text } => text,
            Self::SlashRun { cmd } => cmd,
            Self::RunShell { cmd } => cmd,
            Self::Notify { message } => message,
        }
    }

    pub fn from_parts(kind_idx: usize, text: String) -> Self {
        match kind_idx {
            1 => Self::SlashRun { cmd: text },
            2 => Self::RunShell { cmd: text },
            3 => Self::Notify { message: text },
            _ => Self::SendPrompt { text },
        }
    }

    pub fn summary(&self) -> String {
        let raw = self.text();
        let display = if raw.chars().count() > 28 {
            let truncated: String = raw.chars().take(27).collect();
            format!("{truncated}…")
        } else {
            raw.to_string()
        };
        match self {
            Self::SendPrompt { .. } => format!("→ \"{display}\""),
            Self::SlashRun { .. } => format!("→ /{display}"),
            Self::RunShell { .. } => format!("$ {display}"),
            Self::Notify { .. } => format!("🔔 {display}"),
        }
    }
}

// ── Rule ───────────────────────────────────────────────────────────────────

/// Intermediate struct for backward-compatible TOML deserialization.
/// Rules written with the old single `action` key are transparently promoted
/// to the `actions` array.
#[derive(Debug, Deserialize)]
struct AutomationRuleRaw {
    pub id: u64,
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub trigger: TriggerKind,
    /// New format: zero or more actions.
    #[serde(default)]
    pub actions: Vec<ActionKind>,
    /// Legacy format (v1): single action — migrated on load.
    #[serde(default)]
    pub action: Option<ActionKind>,
}

fn default_true() -> bool {
    true
}

impl From<AutomationRuleRaw> for AutomationRule {
    fn from(raw: AutomationRuleRaw) -> Self {
        let actions = if !raw.actions.is_empty() {
            raw.actions
        } else if let Some(action) = raw.action {
            vec![action]
        } else {
            Vec::new()
        };
        AutomationRule {
            id: raw.id,
            name: raw.name,
            enabled: raw.enabled,
            trigger: raw.trigger,
            actions,
        }
    }
}

/// A single automation rule.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationRule {
    pub id: u64,
    pub name: String,
    pub enabled: bool,
    pub trigger: TriggerKind,
    /// One or more actions fired when the trigger activates.
    pub actions: Vec<ActionKind>,
}

impl<'de> Deserialize<'de> for AutomationRule {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = AutomationRuleRaw::deserialize(d)?;
        Ok(AutomationRule::from(raw))
    }
}

impl AutomationRule {
    pub fn new(id: u64, name: String, trigger: TriggerKind, actions: Vec<ActionKind>) -> Self {
        Self {
            id,
            name,
            enabled: true,
            trigger,
            actions,
        }
    }

    /// First action's summary, or a placeholder if the rule has no actions yet.
    pub fn primary_action_summary(&self) -> String {
        match self.actions.first() {
            Some(a) => a.summary(),
            None => "(no actions)".to_string(),
        }
    }
}

// ── Config ─────────────────────────────────────────────────────────────────

/// Persisted automation configuration (serialised to `automation.toml`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AutomationConfig {
    #[serde(default)]
    pub rules: Vec<AutomationRule>,
    /// Monotonically increasing counter used to assign unique rule IDs.
    #[serde(default)]
    pub next_id: u64,
}

impl AutomationConfig {
    pub fn load() -> Self {
        let Some(path) = Self::path() else {
            return Self::default();
        };
        let Ok(raw) = fs::read_to_string(&path) else {
            return Self::default();
        };
        let mut cfg: AutomationConfig = toml::from_str(&raw).unwrap_or_default();
        // Guard against manually-edited TOML that adds rules with IDs higher than
        // next_id: bump next_id so alloc_id() never reuses an existing ID.
        let max_existing = cfg.rules.iter().map(|r| r.id).max().unwrap_or(0);
        if cfg.next_id < max_existing {
            cfg.next_id = max_existing;
        }
        cfg
    }

    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = Self::path() else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let body = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        fs::write(path, body)
    }

    pub fn path() -> Option<PathBuf> {
        zagens_config::user_data_path("automation.toml").ok()
    }

    /// Allocate a new unique rule ID.
    pub fn alloc_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────

fn format_duration_label(secs: u64, prefix: &str) -> String {
    if secs == 0 {
        format!("{prefix} —")
    } else if secs < 60 {
        format!("{prefix} {secs}s")
    } else if secs < 3600 {
        let m = secs / 60;
        let s = secs % 60;
        if s == 0 {
            format!("{prefix} {m}m")
        } else {
            format!("{prefix} {m}m{s}s")
        }
    } else {
        let h = secs / 3600;
        let rem = (secs % 3600) / 60;
        if rem == 0 {
            format!("{prefix} {h}h")
        } else {
            format!("{prefix} {h}h{rem}m")
        }
    }
}
