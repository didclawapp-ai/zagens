//! Plain-language session coherence state (P2 PR4 → `deepseek-core`).

use serde::{Deserialize, Serialize};

/// User-facing coherence ladder for session health.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoherenceState {
    #[default]
    Healthy,
    GettingCrowded,
    RefreshingContext,
    VerifyingRecentWork,
    ResettingPlan,
}

impl CoherenceState {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::GettingCrowded => "getting crowded",
            Self::RefreshingContext => "refreshing context",
            Self::VerifyingRecentWork => "verifying recent work",
            Self::ResettingPlan => "resetting plan",
        }
    }

    #[must_use]
    pub fn description(self) -> &'static str {
        match self {
            Self::Healthy => "The session is stable and focused.",
            Self::GettingCrowded => "The session is approaching context pressure.",
            Self::RefreshingContext => "The engine is refreshing context before continuing.",
            Self::VerifyingRecentWork => {
                "The engine is checking recent tool results before continuing."
            }
            Self::ResettingPlan => {
                "The engine is rebuilding from canonical context and replanning."
            }
        }
    }
}
