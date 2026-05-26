use serde_json;

pub fn format_tool_result(result: &serde_json::Value) -> String {
    let is_error = result
        .get("isError")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

    let content = result
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
        );

    if is_error {
        format!("Error: {content}")
    } else {
        content
    }
}
