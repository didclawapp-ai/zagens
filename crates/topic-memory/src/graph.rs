//! Pheromone-style topic graph (B2 / topic-memory-graph port).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

pub const GRAPH_SCHEMA_VERSION: &str = "0.1.0";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PheromoneGraph {
    pub version: String,
    #[serde(rename = "lastDecay")]
    pub last_decay: String,
    pub nodes: HashMap<String, PheromoneNode>,
    pub edges: HashMap<String, PheromoneEdge>,
    #[serde(rename = "blockedPoints")]
    pub blocked_points: Vec<BlockedPoint>,
    #[serde(rename = "recentEmotions", skip_serializing_if = "Option::is_none")]
    pub recent_emotions: Option<Vec<EmotionMode>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trails: Option<Vec<CognitiveTrail>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PheromoneNode {
    pub count: u32,
    #[serde(rename = "lastSeen")]
    pub last_seen: String,
    pub strength: f64,
    pub depth: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dormant: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PheromoneEdge {
    pub weight: f64,
    #[serde(rename = "lastSeen")]
    pub last_seen: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BlockedPoint {
    pub node: String,
    pub context: String,
    pub since: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CognitiveTrail {
    pub entry: String,
    pub exit: String,
    pub date: String,
    pub emotion: EmotionMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EmotionMode {
    #[serde(rename = "A")]
    Angry,
    #[serde(rename = "B")]
    Happy,
    #[serde(rename = "C")]
    Sad,
    #[serde(rename = "N")]
    Neutral,
}

impl EmotionMode {
    #[must_use]
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::Angry => "A",
            Self::Happy => "B",
            Self::Sad => "C",
            Self::Neutral => "N",
        }
    }
}
