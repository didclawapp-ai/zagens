//! Emit per-step `ToolCallFinished` kernel events for T5 composite tools (Phase 4.3).

use serde_json::Value;
use zagens_tools::ToolResult;

use crate::engine::context::summarize_text;
use crate::engine::kernel_event::{KernelEvent, ToolOutcome as KernelToolOutcome};
use crate::engine::tool_effects::tool_writes_state;
use crate::engine::turn_loop::inner_step_host::InnerStepHost;
use crate::engine::turn_machine::emit_kernel_event;

/// When a composite tool returns `metadata.composite_steps`, mirror each step as its own
/// `tool_call_finished` row so T1/T5 telemetry and replay stay faithful.
pub fn emit_composite_step_kernel_events<H: InnerStepHost>(
    host: &mut H,
    turn_id: &str,
    parent_call_id: &str,
    tool_result: &ToolResult,
) {
    let Some(meta) = tool_result.metadata.as_ref() else {
        return;
    };
    emit_composite_step_kernel_events_from_metadata(host, turn_id, parent_call_id, meta);
}

pub fn emit_composite_step_kernel_events_from_metadata<H: InnerStepHost>(
    host: &mut H,
    turn_id: &str,
    parent_call_id: &str,
    metadata: &Value,
) {
    let Some(steps) = metadata.get("composite_steps").and_then(|v| v.as_array()) else {
        return;
    };

    for (idx, step) in steps.iter().enumerate() {
        let tool_name = step
            .get("tool")
            .and_then(|v| v.as_str())
            .unwrap_or("<unknown>");
        let success = step
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let duration_ms = step
            .get("duration_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;
        let preview = step.get("preview").and_then(|v| v.as_str()).unwrap_or("");
        let outcome = if success {
            KernelToolOutcome::Success
        } else {
            KernelToolOutcome::ToolError {
                message: step
                    .get("error")
                    .and_then(|v| v.as_str())
                    .unwrap_or("composite step failed")
                    .to_string(),
            }
        };

        emit_kernel_event(
            host,
            KernelEvent::ToolCallFinished {
                turn_id: turn_id.to_string(),
                call_id: format!("{parent_call_id}:step:{idx}"),
                tool_name: tool_name.to_string(),
                outcome,
                duration_ms,
                wrote_state: tool_writes_state(tool_name),
                result_preview: summarize_text(preview, 512),
                session_content: String::new(),
            },
        );
    }
}
