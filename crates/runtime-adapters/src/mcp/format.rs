use serde_json;

/// Whether an MCP `tools/call` result signals a **tool-level** failure via
/// the spec's `isError` flag. This is distinct from a JSON-RPC protocol
/// error (handled in `connection.rs`): a tool can return a successful RPC
/// response whose payload still represents a failed tool invocation.
pub fn is_tool_error(result: &serde_json::Value) -> bool {
    result
        .get("isError")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
}

/// Extract the model-facing content from an MCP `tools/call` result.
///
/// Concatenates the text of every `text` content block; non-text blocks
/// (image, resource, …) are rendered as `[<type> content]` placeholders so
/// the model knows something was returned without flooding the context with
/// base64. Falls back to pretty-printed JSON when there's no `content` array.
pub fn extract_tool_content(result: &serde_json::Value) -> String {
    result
        .get("content")
        .and_then(|v| v.as_array())
        .map_or_else(
            || serde_json::to_string_pretty(result).unwrap_or_default(),
            |arr| {
                arr.iter()
                    .filter_map(|item| match item.get("type")?.as_str()? {
                        "text" => item.get("text")?.as_str().map(String::from),
                        other => Some(format!("[{other} content]")),
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            },
        )
}

/// Human-readable rendering of a tool result, prefixing `Error:` on failure.
/// Prefer [`is_tool_error`] + [`extract_tool_content`] on execution paths that
/// need to map failures onto a structured error type.
pub fn format_tool_result(result: &serde_json::Value) -> String {
    let content = extract_tool_content(result);
    if is_tool_error(result) {
        format!("Error: {content}")
    } else {
        content
    }
}
