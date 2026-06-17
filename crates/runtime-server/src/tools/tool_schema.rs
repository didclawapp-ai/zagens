//! Derive tool `input_schema` JSON from `schemars` (kernel-v2 M2).
//!
//! Uses draft-07-shaped output with metadata stripped so derived schemas match
//! legacy hand-written tool schemas after `schema_sanitize`.

use schemars::JsonSchema;
use schemars::generate::SchemaSettings;
use serde_json::Value;

/// Root object schema for a tool input struct (no `$defs` / `$ref` wrapper).
#[must_use]
pub fn derived_input_schema<T: JsonSchema>() -> Value {
    let settings = SchemaSettings::draft07().with(|s| {
        s.meta_schema = None;
        s.inline_subschemas = true;
    });
    let mut value = settings
        .into_generator()
        .into_root_schema_for::<T>()
        .to_value();
    normalize_derived_tool_schema(&mut value);
    value
}

fn strip_tool_schema_metadata(value: &mut Value) {
    if let Some(obj) = value.as_object_mut() {
        obj.remove("$schema");
        obj.remove("title");
        obj.remove("$defs");
        obj.remove("definitions");
    }
}

/// Align schemars output with legacy hand-written tool schemas (no null unions,
/// strip numeric `format` only, enum values stay string-only, legacy key order).
fn normalize_derived_tool_schema(value: &mut Value) {
    strip_tool_schema_metadata(value);
    normalize_tool_schema_tree(value);
    if value.get("properties").is_some() {
        reorder_schema_root(value);
    }
}

fn normalize_tool_schema_tree(value: &mut Value) {
    if let Some(props) = value.get_mut("properties").and_then(|v| v.as_object_mut()) {
        for prop in props.values_mut() {
            normalize_tool_property_schema(prop);
        }
    }
    if let Some(items) = value.get_mut("items") {
        normalize_tool_schema_tree(items);
        if items.get("properties").is_some() {
            reorder_schema_root(items);
        }
    }
    for key in ["anyOf", "oneOf", "allOf"] {
        if let Some(members) = value.get_mut(key).and_then(|v| v.as_array_mut()) {
            for member in members.iter_mut() {
                normalize_tool_schema_tree(member);
            }
        }
    }
}

fn reorder_schema_root(value: &mut Value) {
    reorder_object_keys(
        value,
        &["type", "properties", "required", "additionalProperties"],
    );
}

fn reorder_property_schema(value: &mut Value) {
    reorder_object_keys(
        value,
        &[
            "type",
            "minimum",
            "maximum",
            "default",
            "enum",
            "items",
            "description",
        ],
    );
    if let Some(items) = value.get_mut("items") {
        reorder_property_schema(items);
    }
}

fn reorder_object_keys(value: &mut Value, key_order: &[&str]) {
    let Some(obj) = value.as_object_mut() else {
        return;
    };
    let old = std::mem::take(obj);
    let mut reordered = serde_json::Map::new();
    for key in key_order {
        if let Some(val) = old.get(*key) {
            reordered.insert((*key).to_string(), val.clone());
        }
    }
    for (key, val) in old {
        if !reordered.contains_key(&key) {
            reordered.insert(key, val);
        }
    }
    *obj = reordered;
}

fn normalize_tool_property_schema(prop: &mut Value) {
    let Some(obj) = prop.as_object_mut() else {
        return;
    };
    if let Some(types) = obj.get("type").and_then(|t| t.as_array())
        && let Some(non_null) = types.iter().find(|t| t.as_str() != Some("null"))
    {
        obj.insert("type".into(), non_null.clone());
    }
    if let Some(enums) = obj.get_mut("enum").and_then(|v| v.as_array_mut()) {
        enums.retain(|v| !v.is_null());
    }
    obj.remove("format");
    // Schemars adds `minimum: 0` on unsigned integers; legacy schemas only keep
    // bounds when paired with maximum/default (git family constraint fields).
    if obj.get("minimum") == Some(&Value::from(0))
        && !obj.contains_key("maximum")
        && !obj.contains_key("default")
    {
        obj.remove("minimum");
    }
    reorder_property_schema(prop);
    if prop.get("properties").is_some() {
        normalize_tool_schema_tree(prop);
        reorder_schema_root(prop);
    }
    if let Some(items) = prop.get_mut("items") {
        normalize_tool_schema_tree(items);
        if items.get("properties").is_some() {
            reorder_schema_root(items);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use schemars::JsonSchema;

    #[derive(JsonSchema)]
    #[schemars(inline)]
    struct Probe {
        required_field: String,
        optional_field: Option<u64>,
    }

    #[test]
    fn derived_schema_marks_only_non_option_fields_required() {
        let schema = derived_input_schema::<Probe>();
        assert_eq!(schema["type"], "object");
        assert_eq!(schema["required"], serde_json::json!(["required_field"]));
        assert!(
            schema["properties"]["optional_field"]
                .get("nullable")
                .is_none()
        );
        assert!(schema.get("$schema").is_none());
        assert!(schema.get("title").is_none());
    }
}
