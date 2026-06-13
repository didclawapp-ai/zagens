//! `ToolCall` ↔ registry/`ToolResult` conversions for `EngineToolDispatch` (P2 PR4).

use serde_json::Value;
use zagens_protocol::{ToolOutput, ToolPayload};
use zagens_tools::{FunctionCallError, ToolCall, ToolCallSource, ToolError, ToolResult};

/// Build a protocol `ToolCall` from the engine's `(name, JSON input)` shape.
#[must_use]
pub fn value_to_tool_call(name: String, input: Value) -> ToolCall {
    let arguments = serde_json::to_string(&input).unwrap_or_else(|_| "{}".to_string());
    ToolCall {
        name,
        payload: ToolPayload::Function { arguments },
        source: ToolCallSource::Direct,
        raw_tool_call_id: None,
    }
}

/// Parse JSON arguments from a `ToolCall` payload.
pub fn tool_call_input(call: &ToolCall) -> Result<Value, FunctionCallError> {
    match &call.payload {
        ToolPayload::Function { arguments } => parse_tool_arguments(&call.name, arguments),
        ToolPayload::Custom { input } => parse_tool_arguments(&call.name, input),
        ToolPayload::Mcp { .. } | ToolPayload::LocalShell { .. } => {
            Err(FunctionCallError::ExecutionFailed {
                name: call.name.clone(),
                error: "MCP and local-shell tool calls must use the runtime MCP pool, not RegistryToolDispatch".to_string(),
            })
        }
    }
}

fn parse_tool_arguments(name: &str, raw: &str) -> Result<Value, FunctionCallError> {
    serde_json::from_str(raw).map_err(|err| FunctionCallError::ExecutionFailed {
        name: name.to_string(),
        error: format!("invalid tool arguments JSON: {err}"),
    })
}

#[must_use]
pub fn tool_result_to_output(result: ToolResult) -> ToolOutput {
    let body = serde_json::from_str(&result.content)
        .ok()
        .or(Some(Value::String(result.content)));
    ToolOutput::Function {
        body,
        success: result.success,
    }
}

pub fn tool_output_to_result(output: ToolOutput) -> Result<ToolResult, ToolError> {
    match output {
        ToolOutput::Function { body, success } => {
            let content = match body {
                Some(Value::String(s)) => s,
                Some(v) => serde_json::to_string_pretty(&v).unwrap_or_else(|_| v.to_string()),
                None => String::new(),
            };
            Ok(ToolResult {
                content,
                success,
                metadata: None,
            })
        }
        ToolOutput::Mcp { result } => {
            ToolResult::json(&result).map_err(|e| ToolError::execution_failed(e.to_string()))
        }
    }
}

pub fn function_call_to_tool_error(err: FunctionCallError, tool_name: &str) -> ToolError {
    match err {
        FunctionCallError::ToolNotFound { name } => {
            ToolError::not_available(format!("tool '{name}' is not registered"))
        }
        FunctionCallError::MutatingToolRejected { name } => ToolError::permission_denied(format!(
            "Tool '{name}' requires approval before mutating execution"
        )),
        FunctionCallError::TimedOut {
            name: _,
            timeout_ms,
        } => ToolError::Timeout {
            seconds: timeout_ms.div_ceil(1000).max(1),
        },
        FunctionCallError::Cancelled { name } => {
            ToolError::execution_failed(format!("Tool '{name}' was cancelled"))
        }
        FunctionCallError::KindMismatch { expected, got } => ToolError::invalid_input(format!(
            "Tool '{tool_name}' payload kind mismatch (expected {expected:?}, got {got:?})"
        )),
        FunctionCallError::ExecutionFailed { error, .. } => ToolError::execution_failed(error),
    }
}

/// Whether the tool may mutate workspace state (parallel batch / approval policy).
#[must_use]
pub fn tool_name_is_mutating(name: &str) -> bool {
    crate::engine::tool_effects::tool_writes_state(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tool_result_to_output_parses_json_body() {
        let out = tool_result_to_output(ToolResult::success(r#"{"ok":true}"#));
        match out {
            ToolOutput::Function { success, body } => {
                assert!(success);
                assert_eq!(body, Some(json!({"ok": true})));
            }
            _ => panic!("expected Function output"),
        }
    }

    #[test]
    fn tool_call_input_parses_function_arguments() {
        let call = value_to_tool_call("read_file".to_string(), json!({"path": "a.txt"}));
        let input = tool_call_input(&call).expect("parse");
        assert_eq!(input, json!({"path": "a.txt"}));
    }

    #[test]
    fn tool_output_round_trips_json() {
        let original = ToolResult::success(r#"{"n":1}"#);
        let out = tool_result_to_output(original);
        let back = tool_output_to_result(out).expect("result");
        assert!(back.success);
        assert_eq!(
            serde_json::from_str::<Value>(&back.content).expect("json"),
            json!({"n": 1})
        );
    }
}
