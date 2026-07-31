//! Task type — shared between core and TUI (Code-only; legacy `office` coerced to Code).

use serde::{Deserialize, Serialize};

/// Session-fixed task category. Switching type requires a new session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskType {
    /// Programming, documents via skills/CLI, and repo work; full agent prompt + tools.
    #[default]
    Code,
}

impl TaskType {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        "code"
    }

    #[must_use]
    pub fn display_name(self) -> &'static str {
        "代码"
    }

    #[must_use]
    pub fn uses_code_tool_surface(self) -> bool {
        true
    }

    #[must_use]
    pub fn needs_full_code_prompt(self) -> bool {
        true
    }

    /// Whether the system prompt should list workspace/global skills and allow `load_skill`.
    #[must_use]
    pub fn includes_skills_catalog(self) -> bool {
        true
    }

    pub fn parse_str(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "office" | "code" => Some(Self::Code),
            _ => None,
        }
    }
}
