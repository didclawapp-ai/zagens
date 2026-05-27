//! Schema sanitizer for tool `input_schema` before sending to DeepSeek.

use serde_json::{Map, Value};

/// Sanitize a JSON Schema in-place for DeepSeek strict-tool compatibility.
pub fn sanitize(schema: &mut Value) {
    collapse_nullable_unions(schema);
    inject_properties_on_bare_objects(schema);
    prune_dangling_required(schema);
    collapse_single_element_unions(schema);
    if let Some(obj) = schema.as_object_mut() {
        for (_, v) in obj.iter_mut() {
            sanitize(v);
        }
    } else if let Some(arr) = schema.as_array_mut() {
        for v in arr.iter_mut() {
            sanitize(v);
        }
    }
}

fn collapse_nullable_unions(schema: &mut Value) {
    let Some(obj) = schema.as_object_mut() else {
        return;
    };
    for key in ["anyOf", "oneOf"] {
        let members: Vec<Value> = match obj.get(key).and_then(|v| v.as_array()) {
            Some(arr) => arr.clone(),
            None => continue,
        };
        let (nulls, nons): (Vec<_>, Vec<_>) = members.into_iter().partition(is_null_type);
        if nulls.len() == 1 && nons.len() == 1 {
            obj.remove(key);
            if let Value::Object(non_obj) = nons.into_iter().next().unwrap() {
                for (k, v) in non_obj {
                    if k != "type" || v != "null" {
                        obj.insert(k, v);
                    }
                }
            }
            obj.insert("nullable".into(), Value::Bool(true));
        }
    }
}

fn is_null_type(v: &Value) -> bool {
    v.as_object()
        .and_then(|o| o.get("type"))
        .and_then(|t| t.as_str())
        == Some("null")
}

fn inject_properties_on_bare_objects(schema: &mut Value) {
    let Some(obj) = schema.as_object_mut() else {
        return;
    };
    if obj.get("type").and_then(|t| t.as_str()) != Some("object") {
        return;
    }
    if obj.contains_key("properties") || obj.contains_key("additionalProperties") {
        return;
    }
    obj.insert("properties".into(), Value::Object(Map::new()));
}

fn prune_dangling_required(schema: &mut Value) {
    let Some(obj) = schema.as_object_mut() else {
        return;
    };
    let known_keys: Vec<String> = obj
        .get("properties")
        .and_then(|v| v.as_object())
        .map(|props| props.keys().cloned().collect())
        .unwrap_or_default();
    let Some(required) = obj.get_mut("required").and_then(|v| v.as_array_mut()) else {
        return;
    };
    required.retain(|entry| {
        entry
            .as_str()
            .is_some_and(|k| known_keys.iter().any(|known| known == k))
    });
    if required.is_empty() {
        obj.remove("required");
    }
}

fn collapse_single_element_unions(schema: &mut Value) {
    let Some(obj) = schema.as_object_mut() else {
        return;
    };
    for key in ["oneOf", "allOf", "anyOf"] {
        let single = match obj.get(key).and_then(|v| v.as_array()) {
            Some(arr) if arr.len() == 1 => arr[0].clone(),
            _ => continue,
        };
        obj.remove(key);
        if let Value::Object(inner) = single {
            for (k, v) in inner {
                if !obj.contains_key(&k) {
                    obj.insert(k, v);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn collapses_nullable_anyof() {
        let mut schema = json!({
            "anyOf": [
                {"type": "string"},
                {"type": "null"}
            ]
        });
        sanitize(&mut schema);
        assert_eq!(schema["type"], "string");
        assert_eq!(schema["nullable"], true);
        assert!(schema.get("anyOf").is_none());
    }

    #[test]
    fn injects_properties_on_bare_object() {
        let mut schema = json!({"type": "object"});
        sanitize(&mut schema);
        assert!(schema.get("properties").is_some());
        assert_eq!(schema["properties"], json!({}));
    }

    #[test]
    fn idempotent() {
        let mut schema = json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "maybe": {
                    "anyOf": [{"type": "integer"}, {"type": "null"}]
                }
            },
            "required": ["name", "missing_field"]
        });
        sanitize(&mut schema);
        let after_first = schema.clone();
        sanitize(&mut schema);
        assert_eq!(schema, after_first, "sanitize must be idempotent");
    }
}
