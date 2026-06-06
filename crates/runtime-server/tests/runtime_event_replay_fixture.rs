//! A5.5 runtime event replay fixtures — monotonic seq, schema version, lifecycle ordering.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize)]
struct FixtureEvent {
    schema_version: u32,
    seq: u64,
    timestamp: DateTime<Utc>,
    thread_id: String,
    turn_id: Option<String>,
    event: String,
    payload: Value,
}

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn load_fixture(path: &Path) -> Vec<FixtureEvent> {
    let file = std::fs::File::open(path).unwrap_or_else(|e| panic!("open {}: {e}", path.display()));
    let reader = BufReader::new(file);
    reader
        .lines()
        .filter_map(|line| {
            let line = line.expect("read line");
            if line.trim().is_empty() {
                return None;
            }
            Some(serde_json::from_str(&line).expect("parse fixture event"))
        })
        .collect()
}

fn assert_monotonic_schema(events: &[FixtureEvent]) {
    let mut prev_seq = 0u64;
    for ev in events {
        assert_eq!(ev.schema_version, 2, "fixture schema_version must be 2");
        assert!(ev.seq > prev_seq, "seq must be strictly increasing");
        prev_seq = ev.seq;
        assert_eq!(ev.thread_id, "thr_fixture");
        assert_eq!(ev.turn_id.as_deref(), Some("turn_fixture"));
    }
}

#[test]
fn runtime_turn_minimal_fixture_has_monotonic_seq_and_lifecycle() {
    let events = load_fixture(&fixture_path("runtime_turn_minimal.jsonl"));
    assert!(
        events.len() >= 3,
        "minimal fixture should cover at least started → item → completed"
    );
    assert_monotonic_schema(&events);

    let names: Vec<&str> = events.iter().map(|e| e.event.as_str()).collect();
    assert_eq!(names.first(), Some(&"turn.started"));
    assert!(names.contains(&"item.completed"));
    assert_eq!(names.last(), Some(&"turn.completed"));
}

/// G2 / A5.5 — 10–20 step turn replay covering thinking, tool, approval, and completion.
#[test]
fn runtime_turn_replay_fixture_covers_full_turn_lifecycle() {
    let events = load_fixture(&fixture_path("runtime_turn_replay.jsonl"));
    assert!(
        (10..=20).contains(&events.len()),
        "replay fixture should have 10–20 events, got {}",
        events.len()
    );
    assert_monotonic_schema(&events);

    let names: Vec<&str> = events.iter().map(|e| e.event.as_str()).collect();
    assert_eq!(names.first(), Some(&"turn.started"));
    assert_eq!(names.last(), Some(&"turn.completed"));
    assert!(
        names.contains(&"approval.required"),
        "replay should include approval gate"
    );
    assert!(
        names.iter().filter(|e| **e == "item.delta").count() >= 3,
        "replay should include multiple streaming deltas"
    );
    assert!(
        names.contains(&"item.started"),
        "replay should include item.started"
    );
}
