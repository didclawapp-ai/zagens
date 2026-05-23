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

    /// Whether HTTP/runtime should set `auto_approve: true` for a stored policy string.
    #[must_use]
    pub fn config_implies_auto_approve(value: &str) -> bool {
        matches!(Self::from_config_value(value), Some(Self::Auto))
    }
}

#[cfg(test)]
mod tests {
    use super::ApprovalMode;

    #[test]
    fn on_request_and_untrusted_are_suggest_not_auto() {
        assert!(!ApprovalMode::config_implies_auto_approve("on-request"));
        assert!(!ApprovalMode::config_implies_auto_approve("untrusted"));
        assert_eq!(
            ApprovalMode::from_config_value("on-request"),
            Some(ApprovalMode::Suggest)
        );
    }

    #[test]
    fn only_auto_policy_implies_auto_approve() {
        assert!(ApprovalMode::config_implies_auto_approve("auto"));
        assert!(!ApprovalMode::config_implies_auto_approve("never"));
    }
}
