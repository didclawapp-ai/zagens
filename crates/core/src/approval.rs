//! Tool approval policy shared between engine session state and the TUI shell.

/// Determines when tool executions require user approval.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ApprovalMode {
    /// Auto-approve all tools (YOLO mode / --yolo flag)
    Auto,
    /// Suggest approval for non-safe tools (non-YOLO modes)
    #[default]
    Suggest,
    /// Never execute tools requiring approval
    Never,
}

impl ApprovalMode {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "AUTO",
            Self::Suggest => "SUGGEST",
            Self::Never => "NEVER",
        }
    }

    #[must_use]
    pub fn from_config_value(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(Self::Auto),
            "suggest" | "suggested" | "on-request" | "untrusted" => Some(Self::Suggest),
            "never" | "deny" | "denied" => Some(Self::Never),
            _ => None,
        }
    }
}
