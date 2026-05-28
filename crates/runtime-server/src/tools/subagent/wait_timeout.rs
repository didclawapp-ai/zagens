use serde_json::{Value, json};

use deepseek_core::subagent::SubAgentResult;

pub(crate) fn wait_progress_metadata(snapshot: &SubAgentResult) -> Value {
    let remaining = snapshot.max_steps.saturating_sub(snapshot.steps_taken);
    let estimated_remaining_ms = snapshot
        .step_timeout_ms
        .saturating_mul(u64::from(remaining));
    json!({
        "steps_done": snapshot.steps_taken,
        "max_steps": snapshot.max_steps,
        "estimated_remaining_ms": estimated_remaining_ms,
    })
}
