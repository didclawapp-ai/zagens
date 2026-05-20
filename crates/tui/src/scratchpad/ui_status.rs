//! DS Pick UI enrichments for `GET /v1/threads/{id}/scratchpad/status` (Phase D2).

use serde_json::{Value, json};

/// Count checklist items from a `checklist_write` snapshot JSON.
#[must_use]
pub fn count_checklist_items(snapshot: &Value) -> (usize, usize) {
    let Some(items) = snapshot.get("items").and_then(|v| v.as_array()) else {
        return (0, 0);
    };
    let total = items.len();
    let completed = items
        .iter()
        .filter(|item| {
            item.get("status")
                .and_then(|v| v.as_str())
                .is_some_and(|s| s.eq_ignore_ascii_case("completed"))
        })
        .count();
    (completed, total)
}

/// Mechanical contract warnings for the audit dashboard (§6.13 D2).
#[must_use]
pub fn compute_contract_warnings(status: &Value, checklist_completed: usize) -> Vec<String> {
    let mut warnings = Vec::new();

    let notes_total = status.get("notes_total").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let areas_done = status.get("areas_done").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let areas_deferred = status
        .get("areas_deferred")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let areas_in_progress = status
        .get("areas_in_progress")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let accounted = areas_done + areas_deferred + areas_in_progress;

    if notes_total > 0 && accounted == 0 {
        warnings.push("notes_without_accounted".to_string());
    }

    if checklist_completed > 0 && accounted == 0 {
        warnings.push("checklist_inventory_mismatch".to_string());
    } else if checklist_completed > 0 && areas_done == 0 && areas_in_progress == 0 {
        warnings.push("checklist_inventory_mismatch".to_string());
    }

    warnings
}

/// Merge thread-local fields into a `build_status()` JSON object.
pub fn enrich_status_for_thread_ui(status: &mut Value, checklist_json: Option<&str>) {
    let (checklist_completed, checklist_total) = checklist_json
        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
        .map(|v| count_checklist_items(&v))
        .unwrap_or((0, 0));

    let warnings = compute_contract_warnings(status, checklist_completed);

    if let Some(obj) = status.as_object_mut() {
        obj.insert(
            "checklist_completed".to_string(),
            json!(checklist_completed),
        );
        obj.insert("checklist_total".to_string(), json!(checklist_total));
        obj.insert("contract_warnings".to_string(), json!(warnings));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn checklist_counts_completed() {
        let snap = json!({
            "items": [
                {"id": 1, "status": "completed"},
                {"id": 2, "status": "pending"},
                {"id": 3, "status": "completed"}
            ]
        });
        assert_eq!(count_checklist_items(&snap), (2, 3));
    }

    #[test]
    fn warns_checklist_inventory_mismatch() {
        let status = json!({
            "notes_total": 0,
            "areas_done": 0,
            "areas_deferred": 0,
            "areas_in_progress": 0
        });
        let w = compute_contract_warnings(&status, 5);
        assert!(w.contains(&"checklist_inventory_mismatch".to_string()));
    }
}
