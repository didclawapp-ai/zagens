//! Sidecar OpenAPI re-export (task schemas live in `deepseek-runtime-api` since E1-c6).

pub use deepseek_runtime_api::openapi::{
    SCHEMA_EXPORTS, build_openapi_value, build_openapi_value_with, build_paths,
    export_openapi_json, export_openapi_json_with, path_template_count,
};
