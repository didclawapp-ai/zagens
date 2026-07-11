//! OpenAPI 3.1 export for Zagens runtime HTTP API (D16 E1-c phase 1).

mod paths;
mod schemas;

use schemars::Schema;
use serde_json::{Map, Value, json};

pub use paths::{build_paths, path_template_count};
pub use schemas::{
    RestoreSnapshotResponse, ResumeSessionKernelReplay, ResumeSessionResponse,
    RevertTurnWorkspaceRequest, SessionDetailResponse, SessionsListResponse, StartTurnResponse,
    StreamTurnRequest, ThreadSummary,
};
pub use schemas::{SCHEMA_EXPORTS, SchemaExportFn};

fn rewrite_refs(value: &mut Value) {
    match value {
        Value::Object(map) => {
            if let Some(Value::String(r)) = map.get("$ref")
                && let Some(stripped) = r.strip_prefix("#/$defs/")
            {
                map.insert(
                    "$ref".into(),
                    Value::String(format!("#/components/schemas/{stripped}")),
                );
            }
            for v in map.values_mut() {
                rewrite_refs(v);
            }
        }
        Value::Array(arr) => {
            for v in arr {
                rewrite_refs(v);
            }
        }
        _ => {}
    }
}

fn register_schema(components: &mut Map<String, Value>, name: &str, mut schema: Value) {
    if let Some(defs) = schema.as_object_mut().and_then(|o| o.remove("$defs"))
        && let Some(def_map) = defs.as_object()
    {
        for (def_name, def_schema) in def_map {
            register_schema(components, def_name, def_schema.clone());
        }
    }
    rewrite_refs(&mut schema);
    components.insert(name.into(), schema);
}

fn register_exports(components: &mut Map<String, Value>, exports: &[(&str, SchemaExportFn)]) {
    for (name, make_schema) in exports {
        let schema: Schema = make_schema();
        let value = serde_json::to_value(&schema).unwrap_or_else(|e| {
            panic!("failed to serialize schema {name}: {e}");
        });
        register_schema(components, name, value);
    }
}

/// Assemble the full OpenAPI document (JSON Schema 2020-12 components).
pub fn build_openapi_value_with(extra_schemas: &[(&str, SchemaExportFn)]) -> Value {
    let mut components_schemas = Map::new();
    register_exports(&mut components_schemas, SCHEMA_EXPORTS);
    register_exports(&mut components_schemas, extra_schemas);

    json!({
        "openapi": "3.1.0",
        "info": {
            "title": "Zagens Runtime HTTP API",
            "version": "1.0.0",
            "description": "Local sidecar (`zagens-runtime`) HTTP/SSE surface consumed by Zagens desktop web-ui. SSOT routes: `crates/runtime-api/src/openapi/paths.rs`."
        },
        "servers": [{ "url": "http://127.0.0.1:7878" }],
        "components": {
            "securitySchemes": {
                "BearerAuth": {
                    "type": "http",
                    "scheme": "bearer",
                    "description": "Runtime token from Zagens shell (`DEEPSEEK_RUNTIME_TOKEN` / Tauri proxy)."
                }
            },
            "schemas": Value::Object(components_schemas)
        },
        "paths": Value::Object(build_paths())
    })
}

/// Pretty-printed OpenAPI JSON for check-in and TS codegen.
pub fn export_openapi_json_with(extra_schemas: &[(&str, SchemaExportFn)]) -> String {
    serde_json::to_string_pretty(&build_openapi_value_with(extra_schemas)).expect("openapi json")
}

/// Assemble the full OpenAPI document (no sidecar-only schema extensions).
pub fn build_openapi_value() -> Value {
    build_openapi_value_with(&[])
}

/// Pretty-printed OpenAPI JSON for check-in and TS codegen.
pub fn export_openapi_json() -> String {
    export_openapi_json_with(&[])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `router.rs` registers 60 path templates (2026-07-06: +symbol-index/search);
    /// update if routes change.
    const EXPECTED_PATH_TEMPLATES: usize = 60;

    #[test]
    fn openapi_path_templates_match_router() {
        assert_eq!(
            path_template_count(),
            EXPECTED_PATH_TEMPLATES,
            "update EXPECTED_PATH_TEMPLATES or paths.rs when router.rs changes"
        );
    }

    #[test]
    fn openapi_exports_core_schemas() {
        assert!(SCHEMA_EXPORTS.iter().any(|(n, _)| *n == "ThreadRecord"));
        assert!(SCHEMA_EXPORTS.iter().any(|(n, _)| *n == "SessionMetadata"));
    }

    #[test]
    fn openapi_exports_task_schemas() {
        assert!(SCHEMA_EXPORTS.iter().any(|(n, _)| *n == "TaskRecord"));
        assert!(SCHEMA_EXPORTS.iter().any(|(n, _)| *n == "TasksResponse"));
    }

    #[test]
    fn openapi_components_resolve_session_list_ref() {
        let doc = build_openapi_value_with(&[]);
        let schemas = &doc["components"]["schemas"];
        let list = &schemas["SessionsListResponse"];
        let items_ref = &list["properties"]["sessions"]["items"]["$ref"];
        assert_eq!(
            items_ref.as_str(),
            Some("#/components/schemas/SessionMetadata")
        );
        assert!(schemas.get("SessionMetadata").is_some());
    }

    #[test]
    fn openapi_agent_health_ref_resolves_with_extra_schema() {
        fn tool_telemetry_stub() -> schemars::Schema {
            schemars::json_schema!({
                "type": "object",
                "additionalProperties": true
            })
        }
        let doc = build_openapi_value_with(&[("ToolTelemetryReport", tool_telemetry_stub)]);
        let schemas = &doc["components"]["schemas"];
        assert!(schemas.get("ToolTelemetryReport").is_some());
        let resp = &doc["paths"]["/v1/agent-health"]["get"]["responses"]["200"]["content"]["application/json"]
            ["schema"]["$ref"];
        assert_eq!(
            resp.as_str(),
            Some("#/components/schemas/ToolTelemetryReport")
        );
    }
}
