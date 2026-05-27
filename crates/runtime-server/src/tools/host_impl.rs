//! Sidecar implementations of adapter tool host ports (D16 E1-a3).

use std::collections::HashMap;
use std::sync::Arc;

use deepseek_runtime_adapters::tools::ToolShellEnvHost;
use serde_json::Value;

use crate::hooks::{HookContext, HookExecutor};

/// Bridges `HookExecutor` to the adapter `ToolShellEnvHost` port.
pub struct HookShellEnvHost(pub Arc<HookExecutor>);

impl ToolShellEnvHost for HookShellEnvHost {
    fn collect_shell_env(&self, tool_name: &str, tool_args: &Value) -> HashMap<String, String> {
        let hook_ctx = HookContext::new()
            .with_tool_name(tool_name)
            .with_tool_args(tool_args);
        self.0.collect_shell_env(&hook_ctx)
    }
}
