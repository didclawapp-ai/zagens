//! Shared `schemars` helpers for HTTP/OpenAPI export (D8).

use schemars::{Schema, SchemaGenerator};

/// `PathBuf` serializes as a string on the wire.
pub fn path_as_string(_gen: &mut SchemaGenerator) -> Schema {
    serde_json::from_value(serde_json::json!({ "type": "string" })).expect("path schema")
}

/// `Option<PathBuf>` serializes as an optional string on the wire.
pub fn path_as_string_option(_gen: &mut SchemaGenerator) -> Schema {
    serde_json::from_value(serde_json::json!({
        "type": ["string", "null"]
    }))
    .expect("optional path schema")
}

/// `Vec<PathBuf>` serializes as a string array on the wire.
pub fn path_vec_as_strings(_gen: &mut SchemaGenerator) -> Schema {
    serde_json::from_value(serde_json::json!({
        "type": "array",
        "items": { "type": "string" }
    }))
    .expect("path vec schema")
}
