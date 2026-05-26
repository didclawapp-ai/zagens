//! Agent-facing mode types shared by the TUI, HTTP runtime, and engine core (D6).
//!
//! Kept free of ratatui / crossterm so `deepseek-runtime` can link without the TUI stack.

/// Supported application modes for agent turns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    Agent,
    Yolo,
    Plan,
}

impl AppMode {
    #[must_use]
    pub fn from_setting(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "plan" => Self::Plan,
            "yolo" => Self::Yolo,
            _ => Self::Agent,
        }
    }

    #[must_use]
    pub fn as_setting(self) -> &'static str {
        match self {
            Self::Agent => "agent",
            Self::Yolo => "yolo",
            Self::Plan => "plan",
        }
    }

    /// Short label used in the UI footer.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Agent => "AGENT",
            Self::Yolo => "YOLO",
            Self::Plan => "PLAN",
        }
    }

    #[allow(dead_code)]
    #[must_use]
    pub fn description(self) -> &'static str {
        match self {
            Self::Agent => "Agent mode - autonomous task execution with tools",
            Self::Yolo => "YOLO mode - full tool access without approvals",
            Self::Plan => "Plan mode - design before implementing",
        }
    }
}

/// Reasoning-effort tier selected in the composer or thread config.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ReasoningEffort {
    Off,
    Low,
    Medium,
    High,
    Auto,
    #[default]
    Max,
}

impl ReasoningEffort {
    /// Parse a config-file string into an effort tier. Unknown values fall
    /// back to the default (`Max`) rather than erroring out.
    #[must_use]
    pub fn from_setting(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "off" | "disabled" | "none" | "false" => Self::Off,
            "low" | "minimal" => Self::Low,
            "medium" | "mid" => Self::Medium,
            "high" => Self::High,
            "auto" | "automatic" => Self::Auto,
            "max" | "maximum" | "xhigh" => Self::Max,
            _ => Self::default(),
        }
    }

    /// Canonical lowercase label used for config storage and UI hints.
    #[must_use]
    pub fn as_setting(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Auto => "auto",
            Self::Max => "max",
        }
    }

    /// Short label for the header chip.
    #[must_use]
    pub fn short_label(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Low => "low",
            Self::Medium => "med",
            Self::High => "high",
            Self::Auto => "auto",
            Self::Max => "max",
        }
    }

    /// Value forwarded to the engine/client.
    #[must_use]
    pub fn api_value(self) -> Option<&'static str> {
        Some(self.as_setting())
    }

    /// Cycle through the three behaviorally distinct tiers.
    #[must_use]
    pub fn cycle_next(self) -> Self {
        match self {
            Self::Off => Self::High,
            Self::Auto => Self::Off,
            Self::Low | Self::Medium | Self::High => Self::Max,
            Self::Max => Self::Off,
        }
    }
}
