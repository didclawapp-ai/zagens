//! B3.3 — Coalesce consecutive `item.delta` events after broadcast lag (backpressure).

use serde_json::{json, Value};

use super::types::RuntimeEventRecord;

fn delta_kind(payload: &Value) -> Option<&str> {
    payload.get("kind").and_then(|v| v.as_str())
}

fn delta_item_id(record: &RuntimeEventRecord) -> Option<&str> {
    record.item_id.as_deref()
}

fn append_delta(target: &mut RuntimeEventRecord, incoming: &RuntimeEventRecord) {
    let Some(delta) = incoming.payload.get("delta").and_then(|v| v.as_str()) else {
        return;
    };
    if let Some(existing) = target.payload.get("delta").and_then(|v| v.as_str()) {
        let merged = format!("{existing}{delta}");
        target.payload["delta"] = json!(merged);
    } else {
        target.payload["delta"] = json!(delta);
    }
    target.seq = incoming.seq;
}

/// Merge consecutive `item.delta` records for the same `(item_id, kind)`.
#[must_use]
pub fn coalesce_delta_events(events: Vec<RuntimeEventRecord>) -> Vec<RuntimeEventRecord> {
    let mut out: Vec<RuntimeEventRecord> = Vec::with_capacity(events.len());
    for event in events {
        if event.event == "item.delta" {
            let kind = delta_kind(&event.payload);
            let item = delta_item_id(&event);
            if let Some(last) = out.last_mut() {
                if last.event == "item.delta"
                    && delta_item_id(last) == item
                    && delta_kind(&last.payload) == kind
                {
                    append_delta(last, &event);
                    continue;
                }
            }
        }
        out.push(event);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn delta(seq: u64, item: &str, kind: &str, text: &str) -> RuntimeEventRecord {
        RuntimeEventRecord {
            schema_version: 1,
            seq,
            timestamp: chrono::Utc::now(),
            thread_id: "t1".to_string(),
            turn_id: Some("turn1".to_string()),
            item_id: Some(item.to_string()),
            event: "item.delta".to_string(),
            payload: json!({ "kind": kind, "delta": text }),
        }
    }

    #[test]
    fn merges_same_item_deltas() {
        let events = vec![
            delta(1, "i1", "agent_message", "hel"),
            delta(2, "i1", "agent_message", "lo"),
            delta(3, "i2", "thinking", "?"),
        ];
        let merged = coalesce_delta_events(events);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].payload["delta"], "hello");
        assert_eq!(merged[0].seq, 2);
    }

    #[test]
    fn does_not_merge_different_kinds() {
        let events = vec![
            delta(1, "i1", "agent_message", "a"),
            delta(2, "i1", "thinking", "b"),
        ];
        assert_eq!(coalesce_delta_events(events).len(), 2);
    }
}
