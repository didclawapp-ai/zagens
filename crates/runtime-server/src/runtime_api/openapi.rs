//! Sidecar OpenAPI merge — task schemas until `task_manager` ports (D16 E1-c phase 1).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::task_manager::{TaskCounts, TaskRecord, TaskStatus, TaskSummary};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TasksResponse {
    pub tasks: Vec<TaskSummary>,
    pub counts: TaskCounts,
}

/// Task/automation API schemas (sidecar-only until E1-b task port completes).
pub const TASK_SCHEMA_EXPORTS: &[(&str, fn() -> schemars::Schema)] = &[
    ("TaskRecord", || schemars::schema_for!(TaskRecord)),
    ("TaskSummary", || schemars::schema_for!(TaskSummary)),
    ("TaskCounts", || schemars::schema_for!(TaskCounts)),
    ("TasksResponse", || schemars::schema_for!(TasksResponse)),
    ("TaskStatus", || schemars::schema_for!(TaskStatus)),
];

pub use deepseek_runtime_api::openapi::{
    build_openapi_value_with, build_paths, export_openapi_json_with, path_template_count,
    SCHEMA_EXPORTS,
};

/// Assemble the full OpenAPI document including sidecar task schemas.
pub fn build_openapi_value() -> serde_json::Value {
    build_openapi_value_with(TASK_SCHEMA_EXPORTS)
}

/// Pretty-printed OpenAPI JSON for check-in and TS codegen.
pub fn export_openapi_json() -> String {
    export_openapi_json_with(TASK_SCHEMA_EXPORTS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openapi_exports_task_schemas() {
        let doc = build_openapi_value();
        let schemas = &doc["components"]["schemas"];
        assert!(schemas.get("TaskRecord").is_some());
        assert!(schemas.get("TasksResponse").is_some());
    }
}
