//! T5 composite tool helpers (Phase 4.3) — shared step metadata for kernel_events.

use std::time::Instant;

use serde::Serialize;
use serde_json::{Value, json};

/// One sub-step executed inside a composite tool call.
#[derive(Debug, Clone, Serialize)]
pub struct CompositeStep {
    pub tool: &'static str,
    pub success: bool,
    pub duration_ms: u64,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub preview: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl CompositeStep {
    pub fn ok(tool: &'static str, started: Instant, preview: impl Into<String>) -> Self {
        Self {
            tool,
            success: true,
            duration_ms: started.elapsed().as_millis() as u64,
            preview: preview.into(),
            error: None,
        }
    }

    pub fn fail(tool: &'static str, started: Instant, error: impl Into<String>) -> Self {
        let error = error.into();
        Self {
            tool,
            success: false,
            duration_ms: started.elapsed().as_millis() as u64,
            preview: String::new(),
            error: Some(error),
        }
    }
}

#[must_use]
pub fn composite_metadata(steps: &[CompositeStep]) -> Value {
    json!({ "composite_steps": steps })
}
