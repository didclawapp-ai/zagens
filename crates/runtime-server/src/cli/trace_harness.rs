//! Offline harness snapshot for trace export (structured task graph from thread store).

use anyhow::Result;
use serde_json::{Value, json};
use zagens_core::long_horizon::LongHorizonConfig;
use zagens_runtime_orchestrator::runtime_threads::{
    RuntimeThreadStore, ThreadRecord, TurnItemKind,
};

use crate::long_horizon::{
    build_task_graph_value_with_telemetry, snapshots::checklist_from_json,
    snapshots::plan_from_json,
};

const MAX_OFFLINE_NODE_RECORDS: usize = 64;

/// Parse `long_horizon.*` status lines from persisted turn items (experimental offline fallback).
pub fn parse_offline_harness_nodes(
    store: &RuntimeThreadStore,
    thread_id: &str,
) -> Result<Vec<Value>> {
    let turns = store.list_turns_for_thread(thread_id)?;
    let mut nodes = Vec::new();

    for turn in turns {
        let items = store.list_items_for_turn(&turn.id)?;
        for item in items {
            if !matches!(item.kind, TurnItemKind::Status) {
                continue;
            }
            let Some(detail) = item.detail.as_ref() else {
                continue;
            };
            let Some(rest) = detail.strip_prefix("long_horizon.") else {
                continue;
            };
            let kind = rest
                .split(|c: char| c == ':' || c.is_whitespace())
                .next()
                .unwrap_or("")
                .to_string();
            if kind.is_empty() {
                continue;
            }
            let payload = detail
                .find('{')
                .and_then(|i| serde_json::from_str::<Value>(&detail[i..]).ok());
            let ts_ms = item
                .ended_at
                .or(item.started_at)
                .map(|t| t.timestamp_millis())
                .unwrap_or(0);
            nodes.push(json!({
                "kind": kind,
                "ts_ms": ts_ms,
                "payload": payload.unwrap_or(Value::Null),
            }));
            if nodes.len() >= MAX_OFFLINE_NODE_RECORDS {
                return Ok(nodes);
            }
        }
    }

    Ok(nodes)
}

/// Build harness panel JSON from persisted thread snapshots (no live telemetry cache).
pub fn build_offline_harness_snapshot(
    thread: &ThreadRecord,
    lht: &LongHorizonConfig,
    store: Option<&RuntimeThreadStore>,
    thread_id: &str,
) -> Value {
    let plan = plan_from_json(thread.plan_snapshot.as_ref());
    let checklist = checklist_from_json(thread.checklist_snapshot.as_ref());

    let mut completion_gate = None;
    if lht.completion_gate.is_active() {
        let mode = match lht.completion_gate.mode {
            zagens_core::long_horizon::CompletionGateMode::Enforce => "enforce",
            zagens_core::long_horizon::CompletionGateMode::Observe => "observe",
        };
        completion_gate = Some(crate::long_horizon::CompletionGatePanelJson {
            active: true,
            mode: Some(mode.to_string()),
            ..Default::default()
        });
    }

    let mut value = build_task_graph_value_with_telemetry(
        &plan,
        &checklist,
        "en",
        lht,
        None,
        None,
        None,
        completion_gate,
        None,
    );

    let offline_nodes = store
        .and_then(|s| parse_offline_harness_nodes(s, thread_id).ok())
        .filter(|n| !n.is_empty());

    let nodes_source = if offline_nodes.is_some() {
        "turn_items_experimental"
    } else {
        "offline_empty"
    };

    if let Some(obj) = value.as_object_mut() {
        obj.insert("snapshot_source".to_string(), json!("thread_store_offline"));
        obj.insert(
            "recent_nodes".to_string(),
            Value::Array(offline_nodes.clone().unwrap_or_default()),
        );
    }

    json!({
        "task_graph": value,
        "nodes_source": nodes_source,
    })
}
