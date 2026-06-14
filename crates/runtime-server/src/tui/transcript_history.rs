//! Restore transcript turns from persisted thread store + runtime events.

use std::collections::HashMap;

use serde_json::Value;

use crate::runtime_threads::RuntimeThreadStore;
use crate::runtime_threads::event_coalesce::coalesce_delta_events;
use crate::runtime_threads::types::{
    RuntimeEventRecord, TurnItemKind, TurnItemLifecycleStatus, TurnItemRecord, TurnRecord,
};

use super::runtime_events::map_record;
use super::transcript::{TranscriptItem, probe_noise_line, truncate_detail};
use super::transcript_filter::{format_tool_result_summary, format_tool_started_summary};
use super::transcript_turn::{TranscriptTurn, TurnThinking, TurnTool};
use zagens_core::events::Event;

const HISTORY_REPLAY_TURNS: usize = 20;

struct TurnReplayExtras {
    thinking: String,
    harness: Vec<String>,
}

/// Build transcript items from durable turn items plus coalesced runtime events.
pub fn seed_from_thread_store(
    store: &RuntimeThreadStore,
    turns: &[TurnRecord],
    events: &[RuntimeEventRecord],
    turn_limit: usize,
) -> Vec<TranscriptItem> {
    let limit = turn_limit.max(1);
    let selected: Vec<&TurnRecord> = turns
        .iter()
        .rev()
        .take(limit)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();

    let coalesced = coalesce_delta_events(events.to_vec());
    let extras = collect_turn_extras(&coalesced);

    let mut items = Vec::new();
    for turn in selected {
        let Ok(turn_items) = store.list_items_for_turn(&turn.id) else {
            continue;
        };
        if let Some(item) = build_turn_item(&turn_items, extras.get(&turn.id)) {
            items.push(item);
        }
    }
    items
}

fn collect_turn_extras(events: &[RuntimeEventRecord]) -> HashMap<String, TurnReplayExtras> {
    let mut map: HashMap<String, TurnReplayExtras> = HashMap::new();
    for record in events {
        let Some(turn_id) = record.turn_id.as_deref().filter(|id| !id.is_empty()) else {
            continue;
        };
        let entry = map
            .entry(turn_id.to_string())
            .or_insert_with(|| TurnReplayExtras {
                thinking: String::new(),
                harness: Vec::new(),
            });

        if record.event == "item.delta"
            && record.payload.get("kind").and_then(|v| v.as_str()) == Some("thinking")
        {
            if let Some(delta) = record.payload.get("delta").and_then(|v| v.as_str())
                && !probe_noise_line(delta)
            {
                entry.thinking.push_str(delta);
            }
            continue;
        }

        let Some(event) = map_record(record) else {
            continue;
        };
        match event {
            Event::CycleAdvanced { from, to, .. } => {
                entry.harness.push(format!("harness: cycle {from}->{to}"));
            }
            Event::CraftVerdict { verdict, .. } => {
                entry.harness.push(format!("craft review: {verdict}"));
            }
            Event::CraftBoardUpdated { .. } => {
                entry
                    .harness
                    .push("blackboard findings updated".to_string());
            }
            Event::AgentSpawned { id, .. } => {
                entry.harness.push(format!("subagent spawned: {id}"));
            }
            Event::AgentComplete { id, .. } => {
                entry.harness.push(format!("subagent done: {id}"));
            }
            Event::TurnComplete { end_reason, .. } => {
                if let Some(reason) = end_reason.filter(|r| !r.trim().is_empty()) {
                    entry.harness.push(format!("turn end: {reason}"));
                }
            }
            _ => {}
        }
    }
    map
}

fn build_turn_item(
    items: &[TurnItemRecord],
    extras: Option<&TurnReplayExtras>,
) -> Option<TranscriptItem> {
    let mut user = String::new();
    let mut tools = Vec::new();
    let mut content = String::new();

    for item in items {
        match item.kind {
            TurnItemKind::UserMessage => {
                user = item.detail.clone().unwrap_or_else(|| item.summary.clone());
            }
            TurnItemKind::AgentMessage => {
                let text = item.detail.clone().unwrap_or_else(|| item.summary.clone());
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if !content.is_empty() {
                    content.push_str("\n\n");
                }
                content.push_str(trimmed);
            }
            kind if is_tool_item_kind(kind) => {
                tools.push(tool_from_item(item));
            }
            _ => {}
        }
    }

    if user.trim().is_empty() && content.is_empty() && tools.is_empty() {
        return None;
    }

    let thinking_text = extras.map(|e| e.thinking.as_str()).unwrap_or("");
    let turn = TranscriptTurn {
        user,
        thinking: TurnThinking {
            text: thinking_text.to_string(),
            char_count: thinking_text.chars().count(),
            streaming: false,
        },
        tools,
        content,
        content_streaming: false,
        harness: extras.map(|e| e.harness.clone()).unwrap_or_default(),
        open: false,
        tools_collapsed: true,
        harness_collapsed: true,
        thinking_expanded: false,
    };
    Some(TranscriptItem::Turn(turn))
}

fn is_tool_item_kind(kind: TurnItemKind) -> bool {
    matches!(
        kind,
        TurnItemKind::ToolCall | TurnItemKind::FileChange | TurnItemKind::CommandExecution
    )
}

fn tool_from_item(item: &TurnItemRecord) -> TurnTool {
    let name = tool_name_from_item(item);
    let input = tool_input_from_item(item);
    let output = item.detail.as_deref().unwrap_or("");
    let success = item.status != TurnItemLifecycleStatus::Failed;
    let mut detail = truncate_detail(&input.to_string());
    let summary = if output.trim().is_empty() {
        format_tool_started_summary(&name, &input)
    } else {
        if !output.is_empty() {
            detail.push_str("\n---\n");
            detail.push_str(&truncate_detail(output));
        }
        format_tool_result_summary(&name, output, success)
    };
    TurnTool {
        id: item.id.clone(),
        name,
        summary,
        detail,
        expanded: false,
        done: true,
        success: Some(success),
    }
}

fn tool_name_from_item(item: &TurnItemRecord) -> String {
    if let Some(name) = item
        .metadata
        .as_ref()
        .and_then(|m| m.get("tool_name"))
        .and_then(|v| v.as_str())
    {
        return name.to_string();
    }
    let summary = item.summary.trim();
    if summary.ends_with(" started") {
        return summary.trim_end_matches(" started").trim().to_string();
    }
    if let Some((name, _)) = summary.split_once(':') {
        return name.trim().to_string();
    }
    match item.kind {
        TurnItemKind::FileChange => "file_change".to_string(),
        TurnItemKind::CommandExecution => "command_execution".to_string(),
        _ => "tool_call".to_string(),
    }
}

fn tool_input_from_item(item: &TurnItemRecord) -> Value {
    if let Some(input) = item.metadata.as_ref().and_then(|m| m.get("tool_input")) {
        return input.clone();
    }
    if let Some(detail) = item.detail.as_deref()
        && let Ok(v) = serde_json::from_str::<Value>(detail)
        && v.is_object()
    {
        return v;
    }
    serde_json::json!({})
}

pub fn default_history_turn_limit() -> usize {
    HISTORY_REPLAY_TURNS
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use serde_json::json;

    use crate::runtime_threads::RuntimeTurnStatus;

    fn temp_store() -> (tempfile::TempDir, RuntimeThreadStore) {
        let dir = tempfile::tempdir().expect("tempdir");
        let store =
            RuntimeThreadStore::open_json_only(dir.path().to_path_buf()).expect("open store");
        (dir, store)
    }

    fn tool_item(turn_id: &str, item_id: &str, name: &str, output: &str) -> TurnItemRecord {
        TurnItemRecord {
            schema_version: 1,
            id: item_id.to_string(),
            turn_id: turn_id.to_string(),
            kind: TurnItemKind::ToolCall,
            status: TurnItemLifecycleStatus::Completed,
            summary: format!("{name}: {output}"),
            detail: Some(output.to_string()),
            metadata: Some(json!({
                "tool_name": name,
                "tool_input": {"path": "lib.rs"},
            })),
            artifact_refs: Vec::new(),
            started_at: Some(Utc::now()),
            ended_at: Some(Utc::now()),
        }
    }

    #[test]
    fn restores_tools_and_thinking_on_reopen() {
        let (_dir, store) = temp_store();
        let thread_id = "thr_replay";
        let turn_id = "turn_replay";

        let user = TurnItemRecord {
            schema_version: 1,
            id: "item_user".to_string(),
            turn_id: turn_id.to_string(),
            kind: TurnItemKind::UserMessage,
            status: TurnItemLifecycleStatus::Completed,
            summary: "hello".to_string(),
            detail: Some("hello".to_string()),
            metadata: None,
            artifact_refs: Vec::new(),
            started_at: Some(Utc::now()),
            ended_at: Some(Utc::now()),
        };
        let tool = tool_item(turn_id, "item_tool", "read_file", "file contents");
        let agent = TurnItemRecord {
            schema_version: 1,
            id: "item_agent".to_string(),
            turn_id: turn_id.to_string(),
            kind: TurnItemKind::AgentMessage,
            status: TurnItemLifecycleStatus::Completed,
            summary: "done".to_string(),
            detail: Some("done".to_string()),
            metadata: None,
            artifact_refs: Vec::new(),
            started_at: Some(Utc::now()),
            ended_at: Some(Utc::now()),
        };

        for item in [&user, &tool, &agent] {
            store.save_item(item).expect("save item");
        }

        let turn = TurnRecord {
            schema_version: 1,
            id: turn_id.to_string(),
            thread_id: thread_id.to_string(),
            status: RuntimeTurnStatus::Completed,
            input_summary: "hello".to_string(),
            created_at: Utc::now(),
            started_at: Some(Utc::now()),
            ended_at: Some(Utc::now()),
            duration_ms: Some(1),
            usage: None,
            last_request_input_tokens: None,
            error: None,
            item_ids: vec![user.id.clone(), tool.id.clone(), agent.id.clone()],
            steer_count: 0,
        };
        store.save_turn(&turn).expect("save turn");

        let events = vec![RuntimeEventRecord {
            schema_version: 1,
            seq: 1,
            timestamp: Utc::now(),
            thread_id: thread_id.to_string(),
            turn_id: Some(turn_id.to_string()),
            item_id: Some("think_stream".to_string()),
            event: "item.delta".to_string(),
            payload: json!({ "kind": "thinking", "delta": "plan step" }),
        }];

        let turns = store.list_turns_for_thread(thread_id).expect("list turns");
        let items = seed_from_thread_store(&store, &turns, &events, 20);
        assert_eq!(items.len(), 1);
        let TranscriptItem::Turn(replay) = &items[0] else {
            panic!("expected turn");
        };
        assert_eq!(replay.user, "hello");
        assert_eq!(replay.content, "done");
        assert_eq!(replay.thinking.text, "plan step");
        assert_eq!(replay.tools.len(), 1);
        assert_eq!(replay.tools[0].name, "read_file");
        assert!(replay.tools[0].done);
    }
}
