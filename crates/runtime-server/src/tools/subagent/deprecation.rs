use serde_json::{Value, json};

use crate::tools::spec::ToolResult;

use super::constants::DEPRECATION_REMOVAL_VERSION;

// === Deprecation helpers ===

/// Wrap a `ToolResult` with a `_deprecation` block in its metadata.
///
/// Applied exclusively on alias paths (not on canonical tool names) so the
/// model can detect and migrate away from the old name before removal in
/// v`DEPRECATION_REMOVAL_VERSION`.
///
/// The `_deprecation` key is merged into any existing metadata so other
/// metadata (e.g. `status`, `timed_out`) is preserved unchanged.
pub(crate) fn wrap_with_deprecation_notice(
    mut result: ToolResult,
    this_tool: &str,
    use_instead: &str,
) -> ToolResult {
    tracing::warn!(
        "Deprecated tool '{}' invoked — use '{}' instead (removal: v{})",
        this_tool,
        use_instead,
        DEPRECATION_REMOVAL_VERSION,
    );

    let notice = json!({
        "_deprecation": {
            "this_tool": this_tool,
            "use_instead": use_instead,
            "removed_in": DEPRECATION_REMOVAL_VERSION,
            "message": format!(
                "Tool '{}' is deprecated; switch to '{}' before v{}.",
                this_tool, use_instead, DEPRECATION_REMOVAL_VERSION
            )
        }
    });

    result.metadata = Some(match result.metadata.take() {
        Some(Value::Object(mut map)) => {
            if let Value::Object(notice_map) = notice {
                map.extend(notice_map);
            }
            Value::Object(map)
        }
        Some(other) => {
            // Existing metadata was not an object — keep it as-is and add
            // the deprecation notice as a sibling under a wrapper.
            json!({ "_deprecation": notice["_deprecation"].clone(), "_original_metadata": other })
        }
        None => notice,
    });

    result
}
