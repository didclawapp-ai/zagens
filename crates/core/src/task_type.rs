//! Task type (Office / Code) — shared between core and TUI.

use serde::{Deserialize, Serialize};

/// Session-fixed task category. Switching type requires a new session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskType {
    /// Chat + office documents; slim prompt + office tool surface.
    Office,
    /// Programming and repo work; full agent prompt + tools.
    #[default]
    Code,
}

impl TaskType {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Office => "office",
            Self::Code => "code",
        }
    }

    #[must_use]
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Office => "办公",
            Self::Code => "代码",
        }
    }

    #[must_use]
    pub fn uses_code_tool_surface(self) -> bool {
        matches!(self, Self::Code)
    }

    #[must_use]
    pub fn needs_full_code_prompt(self) -> bool {
        matches!(self, Self::Code)
    }

    /// Whether the system prompt should list workspace/global skills and allow `load_skill`.
    #[must_use]
    pub fn includes_skills_catalog(self) -> bool {
        matches!(self, Self::Office | Self::Code)
    }

    pub fn parse_str(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "office" => Some(Self::Office),
            "code" => Some(Self::Code),
            _ => None,
        }
    }
}
