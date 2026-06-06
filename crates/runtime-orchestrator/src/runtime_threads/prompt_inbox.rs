//! Durable prompt admission inbox (OpenCode `SessionInput` analogue).

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum PromptDelivery {
    Steer,
    Queue,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PromptAdmission {
    pub id: String,
    pub thread_id: String,
    pub admitted_seq: u64,
    pub prompt: String,
    pub delivery: PromptDelivery,
    pub time_created: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub promoted_seq: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PromptQueuedResponse {
    pub admitted: PromptAdmission,
    pub active_turn_id: String,
}
