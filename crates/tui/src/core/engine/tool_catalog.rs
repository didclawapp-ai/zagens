//! TUI L2 shell for tool catalog: `AppMode` adapters + `code_execution` subprocess.

use std::path::Path;
use std::time::Duration;

use deepseek_core::turn::TurnLoopMode;
use serde_json::Value;

use crate::models::Tool;
use crate::tools::spec::{ToolError, ToolResult};
use crate::agent_surface::AppMode;

pub use deepseek_core::engine::tool_catalog::{
    active_tools_for_step, apply_mcp_tool_deferral, apply_native_tool_deferral,
    build_model_tool_catalog as build_model_tool_catalog_for_mode, ensure_advanced_tooling,
    execute_tool_search, initial_active_tools, is_tool_search_tool,
    maybe_activate_requested_deferred_tool, missing_tool_error_message,
    should_default_defer_tool as should_default_defer_tool_for_mode, CODE_EXECUTION_TOOL_NAME,
    MULTI_TOOL_PARALLEL_NAME, REQUEST_USER_INPUT_NAME, TOOL_SEARCH_BM25_NAME,
};

fn app_mode_to_turn_loop(mode: AppMode) -> TurnLoopMode {
    match mode {
        AppMode::Agent => TurnLoopMode::Agent,
        AppMode::Yolo => TurnLoopMode::Yolo,
        AppMode::Plan => TurnLoopMode::Plan,
    }
}

pub(super) fn should_default_defer_tool(name: &str, mode: AppMode) -> bool {
    should_default_defer_tool_for_mode(name, app_mode_to_turn_loop(mode))
}

pub(super) fn build_model_tool_catalog(
    native_tools: Vec<Tool>,
    mcp_tools: Vec<Tool>,
    mode: AppMode,
    scratchpad_run_id: Option<&str>,
) -> Vec<Tool> {
    build_model_tool_catalog_for_mode(
        native_tools,
        mcp_tools,
        app_mode_to_turn_loop(mode),
        scratchpad_run_id,
    )
}

pub(super) async fn execute_code_execution_tool(
    input: &Value,
    workspace: &Path,
) -> Result<ToolResult, ToolError> {
    let code = deepseek_tools::required_str(input, "code")?;
    let python_bin = crate::python_env::find_python()
        .map(|(bin, _, _)| bin)
        .unwrap_or_else(|| "python3".to_string());
    let mut cmd = tokio::process::Command::new(&python_bin);
    cmd.arg("-c");
    cmd.arg(code);
    cmd.current_dir(workspace);

    let output = tokio::time::timeout(Duration::from_secs(120), cmd.output())
        .await
        .map_err(|_| ToolError::Timeout { seconds: 120 })
        .and_then(|res| res.map_err(|e| ToolError::execution_failed(e.to_string())))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let return_code = output.status.code().unwrap_or(-1);
    let success = output.status.success();
    let payload = serde_json::json!({
        "type": "code_execution_result",
        "stdout": stdout,
        "stderr": stderr,
        "return_code": return_code,
        "content": [],
    });

    Ok(ToolResult {
        content: serde_json::to_string(&payload).unwrap_or_else(|_| payload.to_string()),
        success,
        metadata: Some(payload),
    })
}
